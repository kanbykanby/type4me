//! OpenAI Whisper / GPT-4o batch (non-streaming) ASR client.
//!
//! Mirrors the macOS OpenAIASRClient.swift implementation.
//!
//! Accumulates raw PCM audio during recording, then on endAudio() sends the
//! entire recording as a WAV file to the REST transcription endpoint.

use super::provider::OpenAIASRConfig;
use super::traits::{ASRRequestOptions, RecognitionEvent, RecognitionTranscript, SpeechRecognizer};
use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use tokio::sync::mpsc;
use tracing::{error, info};

pub struct OpenAIASRClient {
    config: OpenAIASRConfig,
    event_tx: mpsc::UnboundedSender<RecognitionEvent>,
    event_rx: Option<mpsc::UnboundedReceiver<RecognitionEvent>>,
    audio_buffer: Vec<u8>,
}

impl OpenAIASRClient {
    pub fn new(config: OpenAIASRConfig) -> Self {
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        Self {
            config,
            event_tx,
            event_rx: Some(event_rx),
            audio_buffer: Vec::new(),
        }
    }
}

#[async_trait]
impl SpeechRecognizer for OpenAIASRClient {
    async fn connect(&mut self, _options: &ASRRequestOptions) -> Result<()> {
        self.audio_buffer.clear();

        let _ = self.event_tx.send(RecognitionEvent::Ready);

        // Show placeholder during recording (non-streaming, user sees this while recording)
        let placeholder = RecognitionTranscript {
            confirmed_segments: vec![],
            partial_text: "Recording...".to_string(),
            authoritative_text: String::new(),
            is_final: false,
        };
        let _ = self.event_tx.send(RecognitionEvent::Transcript(placeholder));

        Ok(())
    }

    async fn send_audio(&mut self, data: &[u8]) -> Result<()> {
        self.audio_buffer.extend_from_slice(data);
        Ok(())
    }

    async fn end_audio(&mut self) -> Result<()> {
        if self.audio_buffer.is_empty() {
            let _ = self
                .event_tx
                .send(RecognitionEvent::Error("No audio data recorded".to_string()));
            let _ = self.event_tx.send(RecognitionEvent::Completed);
            return Ok(());
        }

        let wav_data = pcm_to_wav(&self.audio_buffer);
        info!(wav_bytes = wav_data.len(), "Sending WAV to OpenAI transcription");

        match self.transcribe(&wav_data).await {
            Ok(text) => {
                if !text.is_empty() {
                    let transcript = RecognitionTranscript {
                        confirmed_segments: vec![text.clone()],
                        partial_text: String::new(),
                        authoritative_text: text,
                        is_final: true,
                    };
                    let _ = self.event_tx.send(RecognitionEvent::Transcript(transcript));
                }
            }
            Err(e) => {
                error!("OpenAI transcription failed: {}", e);
                let _ = self.event_tx.send(RecognitionEvent::Error(e.to_string()));
            }
        }

        let _ = self.event_tx.send(RecognitionEvent::Completed);
        Ok(())
    }

    async fn disconnect(&mut self) {
        self.audio_buffer.clear();
    }

    fn take_event_rx(&mut self) -> Option<mpsc::UnboundedReceiver<RecognitionEvent>> {
        self.event_rx.take()
    }
}

impl OpenAIASRClient {
    async fn transcribe(&self, wav_data: &[u8]) -> Result<String> {
        let url = format!("{}/audio/transcriptions", self.config.base_url);

        // Build multipart form data
        let boundary = uuid::Uuid::new_v4().to_string();
        let mut body = Vec::new();

        // File part
        append_multipart_file(&mut body, &boundary, "file", "audio.wav", "audio/wav", wav_data);
        // Model part
        append_multipart_field(&mut body, &boundary, "model", &self.config.model);
        // Response format
        append_multipart_field(&mut body, &boundary, "response_format", "json");
        // Closing boundary
        body.extend_from_slice(format!("--{}--\r\n", boundary).as_bytes());

        let client = reqwest::Client::new();
        let response = client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .header(
                "Content-Type",
                format!("multipart/form-data; boundary={}", boundary),
            )
            .timeout(std::time::Duration::from_secs(60))
            .body(body)
            .send()
            .await
            .context("OpenAI API request failed")?;

        let status = response.status();
        let response_bytes = response.bytes().await?;

        if !status.is_success() {
            let error_text = String::from_utf8_lossy(&response_bytes[..response_bytes.len().min(500)]);
            error!("OpenAI API HTTP {}: {}", status, error_text);
            bail!("OpenAI API returned HTTP {}", status);
        }

        let json: serde_json::Value =
            serde_json::from_slice(&response_bytes).context("Failed to parse OpenAI response")?;

        let text = json
            .get("text")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();

        info!(chars = text.len(), "OpenAI transcription result");
        Ok(text)
    }
}

// ---------------------------------------------------------------------------
// WAV encoding (16-bit PCM, 16 kHz, mono)
// ---------------------------------------------------------------------------

fn pcm_to_wav(pcm_data: &[u8]) -> Vec<u8> {
    let data_size = pcm_data.len() as u32;
    let file_size = 36 + data_size;

    let mut wav = Vec::with_capacity(44 + pcm_data.len());

    // RIFF header
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&file_size.to_le_bytes());
    wav.extend_from_slice(b"WAVE");

    // fmt chunk
    wav.extend_from_slice(b"fmt ");
    wav.extend_from_slice(&16u32.to_le_bytes()); // chunk size
    wav.extend_from_slice(&1u16.to_le_bytes()); // PCM format
    wav.extend_from_slice(&1u16.to_le_bytes()); // mono
    wav.extend_from_slice(&16000u32.to_le_bytes()); // sample rate
    wav.extend_from_slice(&32000u32.to_le_bytes()); // byte rate (16000 * 2)
    wav.extend_from_slice(&2u16.to_le_bytes()); // block align
    wav.extend_from_slice(&16u16.to_le_bytes()); // bits per sample

    // data chunk
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_size.to_le_bytes());
    wav.extend_from_slice(pcm_data);

    wav
}

// ---------------------------------------------------------------------------
// Multipart helpers
// ---------------------------------------------------------------------------

fn append_multipart_file(
    body: &mut Vec<u8>,
    boundary: &str,
    name: &str,
    filename: &str,
    mime_type: &str,
    data: &[u8],
) {
    body.extend_from_slice(format!("--{}\r\n", boundary).as_bytes());
    body.extend_from_slice(
        format!(
            "Content-Disposition: form-data; name=\"{}\"; filename=\"{}\"\r\n",
            name, filename
        )
        .as_bytes(),
    );
    body.extend_from_slice(format!("Content-Type: {}\r\n\r\n", mime_type).as_bytes());
    body.extend_from_slice(data);
    body.extend_from_slice(b"\r\n");
}

fn append_multipart_field(body: &mut Vec<u8>, boundary: &str, name: &str, value: &str) {
    body.extend_from_slice(format!("--{}\r\n", boundary).as_bytes());
    body.extend_from_slice(
        format!("Content-Disposition: form-data; name=\"{}\"\r\n\r\n", name).as_bytes(),
    );
    body.extend_from_slice(value.as_bytes());
    body.extend_from_slice(b"\r\n");
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pcm_to_wav_header() {
        let pcm = vec![0u8; 100];
        let wav = pcm_to_wav(&pcm);

        assert_eq!(&wav[0..4], b"RIFF");
        let file_size = u32::from_le_bytes([wav[4], wav[5], wav[6], wav[7]]);
        assert_eq!(file_size, 36 + 100);
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(&wav[12..16], b"fmt ");

        // Sample rate at offset 24
        let sample_rate = u32::from_le_bytes([wav[24], wav[25], wav[26], wav[27]]);
        assert_eq!(sample_rate, 16000);

        assert_eq!(&wav[36..40], b"data");
        let data_size = u32::from_le_bytes([wav[40], wav[41], wav[42], wav[43]]);
        assert_eq!(data_size, 100);
    }
}
