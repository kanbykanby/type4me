//! ElevenLabs Scribe v2 streaming ASR WebSocket client.
//!
//! Mirrors the macOS ElevenLabsASRClient.swift implementation.
//!
//! Protocol:
//! - Connect to wss://api.elevenlabs.io/v1/speech-to-text/realtime
//! - Auth via `xi-api-key` header
//! - Send audio as base64-encoded chunks in JSON messages
//! - Receive partial_transcript / committed_transcript events

use super::provider::ElevenLabsASRConfig;
use super::traits::{ASRRequestOptions, RecognitionEvent, RecognitionTranscript, SpeechRecognizer};
use anyhow::{Context, Result};
use async_trait::async_trait;
use base64::Engine;
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;
use tracing::{debug, info, warn};

const ENDPOINT: &str = "wss://api.elevenlabs.io/v1/speech-to-text/realtime";
const SAMPLE_RATE: u32 = 16000;

pub struct ElevenLabsASRClient {
    config: ElevenLabsASRConfig,
    event_tx: mpsc::UnboundedSender<RecognitionEvent>,
    event_rx: Option<mpsc::UnboundedReceiver<RecognitionEvent>>,
    ws_tx: Option<mpsc::UnboundedSender<String>>,
    receive_handle: Option<tokio::task::JoinHandle<()>>,
    send_handle: Option<tokio::task::JoinHandle<()>>,
    audio_packet_count: u64,
    did_request_close: bool,
    pending_final_commit: Arc<AtomicBool>,
}

impl ElevenLabsASRClient {
    pub fn new(config: ElevenLabsASRConfig) -> Self {
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        Self {
            config,
            event_tx,
            event_rx: Some(event_rx),
            ws_tx: None,
            receive_handle: None,
            send_handle: None,
            audio_packet_count: 0,
            did_request_close: false,
            pending_final_commit: Arc::new(AtomicBool::new(false)),
        }
    }
}

#[async_trait]
impl SpeechRecognizer for ElevenLabsASRClient {
    async fn connect(&mut self, options: &ASRRequestOptions) -> Result<()> {
        // Build URL with query params
        let mut url = format!(
            "{}?model_id=scribe_v2_realtime&audio_format=pcm_{}",
            ENDPOINT, SAMPLE_RATE
        );
        if !self.config.language.is_empty() {
            url.push_str(&format!("&language_code={}", self.config.language));
        }

        // Add keyterms (hotwords)
        for hw in options
            .hotwords
            .iter()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .take(1000)
        {
            url.push_str(&format!("&keyterm={}", urlencoding(hw)));
        }

        // Build request with auth header
        let request = http::Request::builder()
            .uri(&url)
            .header("xi-api-key", &self.config.api_key)
            .header("Host", "api.elevenlabs.io")
            .header("Connection", "Upgrade")
            .header("Upgrade", "websocket")
            .header("Sec-WebSocket-Version", "13")
            .header(
                "Sec-WebSocket-Key",
                tokio_tungstenite::tungstenite::handshake::client::generate_key(),
            )
            .body(())
            .context("failed to build ElevenLabs HTTP request")?;

        let (ws_stream, _) = tokio_tungstenite::connect_async(request)
            .await
            .context("ElevenLabs WebSocket connection failed")?;

        let (mut ws_sink, mut ws_source) = ws_stream.split();
        info!("ElevenLabs WebSocket connected");

        self.audio_packet_count = 0;
        self.did_request_close = false;
        self.pending_final_commit.store(false, Ordering::SeqCst);

        // Outbound channel (all text/JSON)
        let (ws_msg_tx, mut ws_msg_rx) = mpsc::unbounded_channel::<String>();
        self.ws_tx = Some(ws_msg_tx);

        // Send loop
        let send_handle = tokio::spawn(async move {
            while let Some(text) = ws_msg_rx.recv().await {
                if let Err(e) = ws_sink.send(Message::Text(text.into())).await {
                    debug!("ElevenLabs WS send error: {}", e);
                    break;
                }
            }
        });
        self.send_handle = Some(send_handle);

        // Receive loop
        let event_tx = self.event_tx.clone();
        let pending_flag = self.pending_final_commit.clone();
        let receive_handle = tokio::spawn(async move {
            let mut confirmed_segments: Vec<String> = Vec::new();
            let mut last_transcript = RecognitionTranscript::empty();

            while let Some(msg_result) = ws_source.next().await {
                match msg_result {
                    Ok(Message::Text(text)) => {
                        let text_str: &str = &text;
                        match serde_json::from_str::<InboundMessage>(text_str) {
                            Ok(msg) => {
                                match msg.message_type.as_str() {
                                    "partial_transcript" => {
                                        if let Some(text) = &msg.text {
                                            let trimmed = text.trim();
                                            if trimmed.is_empty() {
                                                continue;
                                            }
                                            let existing = confirmed_segments.join("");
                                            let normalized =
                                                normalize_segment(trimmed, &existing);
                                            let authoritative: String = confirmed_segments
                                                .iter()
                                                .chain(std::iter::once(&normalized))
                                                .cloned()
                                                .collect::<Vec<_>>()
                                                .join("");
                                            let transcript = RecognitionTranscript {
                                                confirmed_segments: confirmed_segments.clone(),
                                                partial_text: normalized,
                                                authoritative_text: authoritative,
                                                is_final: false,
                                            };
                                            if transcript != last_transcript {
                                                last_transcript = transcript.clone();
                                                let _ = event_tx.send(
                                                    RecognitionEvent::Transcript(transcript),
                                                );
                                            }
                                        }
                                    }
                                    "committed_transcript" => {
                                        let trimmed = msg
                                            .text
                                            .as_deref()
                                            .unwrap_or("")
                                            .trim()
                                            .to_string();
                                        if !trimmed.is_empty() {
                                            let existing = confirmed_segments.join("");
                                            let normalized =
                                                normalize_segment(&trimmed, &existing);
                                            confirmed_segments.push(normalized);
                                        }
                                        // Only set is_final on explicit endAudio commit
                                        let is_final = pending_flag.load(Ordering::SeqCst);
                                        let authoritative = confirmed_segments.join("");
                                        let transcript = RecognitionTranscript {
                                            confirmed_segments: confirmed_segments.clone(),
                                            partial_text: String::new(),
                                            authoritative_text: authoritative,
                                            is_final,
                                        };
                                        if transcript != last_transcript {
                                            last_transcript = transcript.clone();
                                            let _ = event_tx
                                                .send(RecognitionEvent::Transcript(transcript));
                                        }
                                    }
                                    "commit_throttled" => {
                                        // VAD already committed. Treat as final if we
                                        // have confirmed segments or were waiting for commit.
                                        if !confirmed_segments.is_empty() || pending_flag.load(Ordering::SeqCst) {
                                            let authoritative = confirmed_segments.join("");
                                            let transcript = RecognitionTranscript {
                                                confirmed_segments: confirmed_segments.clone(),
                                                partial_text: String::new(),
                                                authoritative_text: authoritative,
                                                is_final: true,
                                            };
                                            let _ = event_tx
                                                .send(RecognitionEvent::Transcript(transcript));
                                        }
                                    }
                                    "unaccepted_terms" => {
                                        let _ = event_tx.send(RecognitionEvent::Error(
                                            "ElevenLabs terms not accepted. Visit elevenlabs.io/app/product-terms to enable STT."
                                                .to_string(),
                                        ));
                                    }
                                    // Mark that the next committed_transcript is the final one
                                    other => {
                                        debug!(msg_type = other, "ElevenLabs unknown message type");
                                    }
                                }
                            }
                            Err(e) => {
                                warn!("ElevenLabs JSON parse error: {}", e);
                            }
                        }
                    }
                    Ok(Message::Close(_)) => {
                        debug!("ElevenLabs WebSocket closed");
                        let _ = event_tx.send(RecognitionEvent::Completed);
                        break;
                    }
                    Err(e) => {
                        debug!("ElevenLabs receive error: {}", e);
                        let _ = event_tx.send(RecognitionEvent::Completed);
                        break;
                    }
                    _ => {}
                }
            }

            debug!("ElevenLabs receive loop ended");
        });
        self.receive_handle = Some(receive_handle);

        Ok(())
    }

    async fn send_audio(&mut self, data: &[u8]) -> Result<()> {
        if let Some(ref tx) = self.ws_tx {
            let b64 = base64::engine::general_purpose::STANDARD.encode(data);
            let json = serde_json::json!({
                "message_type": "input_audio_chunk",
                "audio_base_64": b64,
                "sample_rate": SAMPLE_RATE,
            });
            tx.send(json.to_string()).ok();
            self.audio_packet_count += 1;
        }
        Ok(())
    }

    async fn end_audio(&mut self) -> Result<()> {
        if let Some(ref tx) = self.ws_tx {
            self.did_request_close = true;
            self.pending_final_commit.store(true, Ordering::SeqCst);
            // Send a commit message (empty audio with commit flag)
            let json = serde_json::json!({
                "message_type": "input_audio_chunk",
                "audio_base_64": "",
                "commit": true,
                "sample_rate": SAMPLE_RATE,
            });
            tx.send(json.to_string()).ok();
            info!(
                packets = self.audio_packet_count,
                "Sent ElevenLabs commit"
            );
        }
        Ok(())
    }

    async fn disconnect(&mut self) {
        self.ws_tx.take();
        if let Some(h) = self.send_handle.take() {
            h.abort();
        }
        if let Some(h) = self.receive_handle.take() {
            h.abort();
        }
        self.audio_packet_count = 0;
        self.did_request_close = false;
        debug!("ElevenLabs client disconnected");
    }

    fn take_event_rx(&mut self) -> Option<mpsc::UnboundedReceiver<RecognitionEvent>> {
        self.event_rx.take()
    }
}

// ---------------------------------------------------------------------------
// Inbound message
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct InboundMessage {
    message_type: String,
    text: Option<String>,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn normalize_segment(segment: &str, existing_text: &str) -> String {
    if segment.is_empty() {
        return String::new();
    }
    let last = match existing_text.chars().last() {
        Some(c) => c,
        None => return segment.to_string(),
    };
    let first = match segment.chars().next() {
        Some(c) => c,
        None => return segment.to_string(),
    };

    if last.is_whitespace() || first.is_whitespace() {
        return segment.to_string();
    }
    if is_closing_punct(first) || is_opening_punct(last) {
        return segment.to_string();
    }
    if is_cjk(last) || is_cjk(first) {
        return segment.to_string();
    }
    format!(" {}", segment)
}

fn is_cjk(c: char) -> bool {
    matches!(c as u32,
        0x4E00..=0x9FFF | 0x3400..=0x4DBF | 0xF900..=0xFAFF | 0x3000..=0x303F
    )
}

fn is_closing_punct(c: char) -> bool {
    matches!(
        c,
        ')' | ']'
            | '}'
            | '>'
            | '）'
            | '】'
            | '》'
            | '」'
            | '"'
            | '.'
            | ','
            | '!'
            | '?'
            | ';'
            | ':'
            | '。'
            | '，'
            | '！'
            | '？'
            | '；'
            | '：'
    )
}

fn is_opening_punct(c: char) -> bool {
    matches!(c, '(' | '[' | '{' | '<' | '（' | '【' | '《' | '「' | '"')
}

fn urlencoding(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => result.push(c),
            _ => {
                for byte in c.to_string().as_bytes() {
                    result.push_str(&format!("%{:02X}", byte));
                }
            }
        }
    }
    result
}
