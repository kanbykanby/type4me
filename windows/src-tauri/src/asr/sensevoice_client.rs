//! Local ASR using sherpa-onnx (SenseVoice model).
//!
//! Gated behind `#[cfg(feature = "sherpa")]`.
//!
//! Strategy: VAD + periodic offline decoding to simulate streaming.
//! - Accumulate audio in a ring buffer
//! - Every ~500ms of accumulated speech: decode and emit partial transcript
//! - On endAudio: final full decode
//!
//! The actual sherpa-onnx FFI calls are in the `sherpa` module (bridge).
//! This file contains the SpeechRecognizer implementation that orchestrates them.

#![cfg(feature = "sherpa")]

use super::provider::SherpaASRConfig;
use super::traits::{ASRRequestOptions, RecognitionEvent, RecognitionTranscript, SpeechRecognizer};
use anyhow::{bail, Result};
use async_trait::async_trait;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

/// Number of 16-bit samples at 16 kHz to accumulate before triggering a decode.
/// 500ms * 16000 = 8000 samples = 16000 bytes of PCM.
const DECODE_INTERVAL_BYTES: usize = 16_000;

pub struct SenseVoiceASRClient {
    config: SherpaASRConfig,
    event_tx: mpsc::UnboundedSender<RecognitionEvent>,
    event_rx: Option<mpsc::UnboundedReceiver<RecognitionEvent>>,
    audio_buffer: Vec<u8>,
    pending_since_last_decode: usize,
    last_partial: String,
    // TODO: sherpa-onnx recognizer handle
    // recognizer: Option<sherpa::OfflineRecognizer>,
}

impl SenseVoiceASRClient {
    pub fn new(config: SherpaASRConfig) -> Self {
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        Self {
            config,
            event_tx,
            event_rx: Some(event_rx),
            audio_buffer: Vec::new(),
            pending_since_last_decode: 0,
            last_partial: String::new(),
        }
    }

    /// Decode the accumulated audio buffer and return the transcript text.
    ///
    /// TODO: Replace with actual sherpa-onnx FFI call.
    fn decode_audio(&self, _audio: &[u8]) -> String {
        // Placeholder: in the real implementation, this calls:
        //   1. Convert PCM bytes to f32 samples
        //   2. Create sherpa::OfflineStream
        //   3. stream.accept_waveform(sample_rate, &samples)
        //   4. recognizer.decode(&stream)
        //   5. stream.get_result().text
        //
        // For now, return empty to indicate "not yet implemented"
        warn!("sherpa-onnx FFI not yet wired up, returning empty transcript");
        String::new()
    }
}

#[async_trait]
impl SpeechRecognizer for SenseVoiceASRClient {
    async fn connect(&mut self, _options: &ASRRequestOptions) -> Result<()> {
        if self.config.model_path.is_empty() {
            bail!("SenseVoice model path is empty");
        }

        // TODO: Initialize sherpa-onnx recognizer
        // let recognizer = sherpa::create_offline_recognizer(&self.config.model_path)?;
        // self.recognizer = Some(recognizer);

        self.audio_buffer.clear();
        self.pending_since_last_decode = 0;
        self.last_partial.clear();

        let _ = self.event_tx.send(RecognitionEvent::Ready);
        info!(model = %self.config.model_path, "SenseVoice ASR initialized");

        Ok(())
    }

    async fn send_audio(&mut self, data: &[u8]) -> Result<()> {
        self.audio_buffer.extend_from_slice(data);
        self.pending_since_last_decode += data.len();

        // Periodically decode to provide streaming-like partial results
        if self.pending_since_last_decode >= DECODE_INTERVAL_BYTES {
            self.pending_since_last_decode = 0;

            let text = self.decode_audio(&self.audio_buffer);
            if !text.is_empty() && text != self.last_partial {
                self.last_partial = text.clone();
                let transcript = RecognitionTranscript {
                    confirmed_segments: vec![],
                    partial_text: text,
                    authoritative_text: String::new(),
                    is_final: false,
                };
                let _ = self.event_tx.send(RecognitionEvent::Transcript(transcript));
            }
        }

        Ok(())
    }

    async fn end_audio(&mut self) -> Result<()> {
        // Final full decode on all accumulated audio
        let text = self.decode_audio(&self.audio_buffer);

        if !text.is_empty() {
            let transcript = RecognitionTranscript {
                confirmed_segments: vec![text.clone()],
                partial_text: String::new(),
                authoritative_text: text,
                is_final: true,
            };
            let _ = self.event_tx.send(RecognitionEvent::Transcript(transcript));
        }

        let _ = self.event_tx.send(RecognitionEvent::Completed);
        info!(
            audio_bytes = self.audio_buffer.len(),
            "SenseVoice final decode complete"
        );

        Ok(())
    }

    async fn disconnect(&mut self) {
        self.audio_buffer.clear();
        self.pending_since_last_decode = 0;
        self.last_partial.clear();
        // TODO: Drop recognizer
        // self.recognizer = None;
        debug!("SenseVoice client disconnected");
    }

    fn take_event_rx(&mut self) -> Option<mpsc::UnboundedReceiver<RecognitionEvent>> {
        self.event_rx.take()
    }
}
