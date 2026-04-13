//! Soniox streaming ASR WebSocket client.
//!
//! Mirrors the macOS SonioxASRClient.swift implementation.

use super::provider::SonioxASRConfig;
use super::traits::{ASRRequestOptions, RecognitionEvent, RecognitionTranscript, SpeechRecognizer};
use crate::protocol::soniox_protocol;
use anyhow::{Context, Result};
use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;
use tracing::{debug, error, info, warn};

pub struct SonioxASRClient {
    config: SonioxASRConfig,
    event_tx: mpsc::UnboundedSender<RecognitionEvent>,
    event_rx: Option<mpsc::UnboundedReceiver<RecognitionEvent>>,
    ws_tx: Option<mpsc::UnboundedSender<WsOutbound>>,
    receive_handle: Option<tokio::task::JoinHandle<()>>,
    send_handle: Option<tokio::task::JoinHandle<()>>,
    audio_packet_count: u64,
}

/// Outbound WebSocket messages: text or binary.
enum WsOutbound {
    Text(String),
    Binary(Vec<u8>),
}

impl SonioxASRClient {
    pub fn new(config: SonioxASRConfig) -> Self {
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        Self {
            config,
            event_tx,
            event_rx: Some(event_rx),
            ws_tx: None,
            receive_handle: None,
            send_handle: None,
            audio_packet_count: 0,
        }
    }
}

#[async_trait]
impl SpeechRecognizer for SonioxASRClient {
    async fn connect(&mut self, options: &ASRRequestOptions) -> Result<()> {
        let ws_url = options
            .cloud_proxy_url
            .as_deref()
            .unwrap_or(soniox_protocol::DEFAULT_ENDPOINT);

        let is_cloud_proxy = options.cloud_proxy_url.is_some();

        let (ws_stream, _) = tokio_tungstenite::connect_async(ws_url)
            .await
            .context("Soniox WebSocket connection failed")?;

        let (mut ws_sink, mut ws_source) = ws_stream.split();

        self.audio_packet_count = 0;

        // Outbound channel
        let (ws_msg_tx, mut ws_msg_rx) = mpsc::unbounded_channel::<WsOutbound>();
        self.ws_tx = Some(ws_msg_tx.clone());

        // Send the start/config message
        let start_msg = soniox_protocol::build_start_message(
            &self.config.api_key,
            &self.config.model,
            &options.hotwords,
            is_cloud_proxy,
        );
        info!("Sending Soniox start message");
        ws_sink
            .send(Message::Text(start_msg.into()))
            .await
            .context("failed to send Soniox start message")?;
        info!("Soniox start message sent OK");

        // Spawn send loop
        let send_handle = tokio::spawn(async move {
            while let Some(outbound) = ws_msg_rx.recv().await {
                let msg = match outbound {
                    WsOutbound::Text(t) => Message::Text(t.into()),
                    WsOutbound::Binary(b) => Message::Binary(b.into()),
                };
                if let Err(e) = ws_sink.send(msg).await {
                    debug!("Soniox WS send error: {}", e);
                    break;
                }
            }
        });
        self.send_handle = Some(send_handle);

        // Spawn receive loop
        let event_tx = self.event_tx.clone();
        let receive_handle = tokio::spawn(async move {
            let mut accumulator = TranscriptAccumulator::new();

            while let Some(msg_result) = ws_source.next().await {
                match msg_result {
                    Ok(Message::Text(text)) => {
                        let text_str: &str = &text;
                        match soniox_protocol::parse_response(text_str) {
                            Ok(response) => {
                                if let Some(ref err) = response.error {
                                    error!(
                                        code = err.code,
                                        msg = %err.message,
                                        "Soniox server error"
                                    );
                                    let _ = event_tx.send(RecognitionEvent::Error(format!(
                                        "Soniox error ({}): {}",
                                        err.code, err.message
                                    )));
                                    let _ = event_tx.send(RecognitionEvent::Completed);
                                    return;
                                }

                                if let Some(ref update) = response.transcript {
                                    accumulator.apply(update);
                                    let transcript = accumulator.transcript();
                                    debug!(
                                        confirmed_len = transcript.confirmed_segments.len(),
                                        partial_len = transcript.partial_text.len(),
                                        "Soniox transcript update"
                                    );
                                    let _ =
                                        event_tx.send(RecognitionEvent::Transcript(transcript));
                                }

                                if response.is_finished {
                                    info!("Soniox session finished by server");
                                    let _ = event_tx.send(RecognitionEvent::Completed);
                                    return;
                                }
                            }
                            Err(e) => {
                                warn!("Soniox parse error: {}", e);
                            }
                        }
                    }
                    Ok(Message::Binary(data)) => {
                        // Soniox may also send binary-framed JSON
                        let data_vec: Vec<u8> = data.into();
                        if let Ok(text) = std::str::from_utf8(&data_vec) {
                            if let Ok(response) = soniox_protocol::parse_response(text) {
                                if let Some(ref err) = response.error {
                                    let _ = event_tx.send(RecognitionEvent::Error(format!(
                                        "Soniox error ({}): {}",
                                        err.code, err.message
                                    )));
                                    let _ = event_tx.send(RecognitionEvent::Completed);
                                    return;
                                }

                                if let Some(ref update) = response.transcript {
                                    accumulator.apply(update);
                                    let _ = event_tx
                                        .send(RecognitionEvent::Transcript(accumulator.transcript()));
                                }

                                if response.is_finished {
                                    let _ = event_tx.send(RecognitionEvent::Completed);
                                    return;
                                }
                            }
                        }
                    }
                    Ok(Message::Close(_)) => {
                        debug!("Soniox WebSocket closed");
                        let _ = event_tx.send(RecognitionEvent::Completed);
                        break;
                    }
                    Err(e) => {
                        debug!("Soniox receive error: {}", e);
                        let _ = event_tx.send(RecognitionEvent::Completed);
                        break;
                    }
                    _ => {}
                }
            }

            debug!("Soniox receive loop ended");
        });
        self.receive_handle = Some(receive_handle);

        Ok(())
    }

    async fn send_audio(&mut self, data: &[u8]) -> Result<()> {
        if let Some(ref tx) = self.ws_tx {
            tx.send(WsOutbound::Binary(data.to_vec())).ok();
            self.audio_packet_count += 1;
        }
        Ok(())
    }

    async fn end_audio(&mut self) -> Result<()> {
        if let Some(ref tx) = self.ws_tx {
            // Soniox end-of-stream: send empty string ""
            tx.send(WsOutbound::Text(String::new())).ok();
            info!(
                packets = self.audio_packet_count,
                "Sent Soniox end-of-stream"
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
        debug!("Soniox client disconnected");
    }

    fn take_event_rx(&mut self) -> Option<mpsc::UnboundedReceiver<RecognitionEvent>> {
        self.event_rx.take()
    }
}

// ---------------------------------------------------------------------------
// Transcript accumulator (mirrors macOS SonioxTranscriptAccumulator)
// ---------------------------------------------------------------------------

struct TranscriptAccumulator {
    confirmed_text: String,
    partial_text: String,
    #[allow(dead_code)]
    last_transcript: RecognitionTranscript,
}

impl TranscriptAccumulator {
    fn new() -> Self {
        Self {
            confirmed_text: String::new(),
            partial_text: String::new(),
            last_transcript: RecognitionTranscript::empty(),
        }
    }

    fn apply(&mut self, update: &soniox_protocol::SonioxTranscriptUpdate) {
        if !update.finalized_text.is_empty() {
            self.confirmed_text.push_str(&update.finalized_text);
        }
        self.partial_text = update.partial_text.clone();
    }

    fn transcript(&self) -> RecognitionTranscript {
        let authoritative = format!("{}{}", self.confirmed_text, self.partial_text);
        let confirmed_segments = if self.confirmed_text.is_empty() {
            vec![]
        } else {
            vec![self.confirmed_text.clone()]
        };
        let is_final = self.partial_text.is_empty() && !self.confirmed_text.is_empty();

        RecognitionTranscript {
            confirmed_segments,
            partial_text: self.partial_text.clone(),
            authoritative_text: authoritative,
            is_final,
        }
    }
}
