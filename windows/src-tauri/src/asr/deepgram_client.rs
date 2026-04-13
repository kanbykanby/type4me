//! Deepgram streaming ASR WebSocket client.
//!
//! Mirrors the macOS DeepgramASRClient.swift implementation.

use super::provider::DeepgramASRConfig;
use super::traits::{ASRRequestOptions, RecognitionEvent, RecognitionTranscript, SpeechRecognizer};
use crate::protocol::deepgram_protocol;
use anyhow::{Context, Result};
use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;
use tracing::{debug, info, warn};

pub struct DeepgramASRClient {
    config: DeepgramASRConfig,
    event_tx: mpsc::UnboundedSender<RecognitionEvent>,
    event_rx: Option<mpsc::UnboundedReceiver<RecognitionEvent>>,
    ws_tx: Option<mpsc::UnboundedSender<WsOutbound>>,
    receive_handle: Option<tokio::task::JoinHandle<()>>,
    send_handle: Option<tokio::task::JoinHandle<()>>,
    audio_packet_count: u64,
    did_request_close: bool,
}

enum WsOutbound {
    Text(String),
    Binary(Vec<u8>),
}

impl DeepgramASRClient {
    pub fn new(config: DeepgramASRConfig) -> Self {
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
        }
    }
}

#[async_trait]
impl SpeechRecognizer for DeepgramASRClient {
    async fn connect(&mut self, options: &ASRRequestOptions) -> Result<()> {
        let ws_url = deepgram_protocol::build_ws_url(
            &self.config.api_key,
            &self.config.model,
            &self.config.language,
            options.enable_punc,
            &options.hotwords,
        );

        // Build request with Authorization header
        let request = http::Request::builder()
            .uri(&ws_url)
            .header("Authorization", format!("Token {}", self.config.api_key))
            .header(
                "Host",
                "api.deepgram.com",
            )
            .header("Connection", "Upgrade")
            .header("Upgrade", "websocket")
            .header("Sec-WebSocket-Version", "13")
            .header(
                "Sec-WebSocket-Key",
                tokio_tungstenite::tungstenite::handshake::client::generate_key(),
            )
            .body(())
            .context("failed to build Deepgram HTTP request")?;

        let (ws_stream, _) = tokio_tungstenite::connect_async(request)
            .await
            .context("Deepgram WebSocket connection failed")?;

        let (mut ws_sink, mut ws_source) = ws_stream.split();
        info!("Deepgram WebSocket connected");

        self.audio_packet_count = 0;
        self.did_request_close = false;

        // Outbound channel
        let (ws_msg_tx, mut ws_msg_rx) = mpsc::unbounded_channel::<WsOutbound>();
        self.ws_tx = Some(ws_msg_tx);

        // Send loop
        let send_handle = tokio::spawn(async move {
            while let Some(outbound) = ws_msg_rx.recv().await {
                let msg = match outbound {
                    WsOutbound::Text(t) => Message::Text(t.into()),
                    WsOutbound::Binary(b) => Message::Binary(b.into()),
                };
                if let Err(e) = ws_sink.send(msg).await {
                    debug!("Deepgram WS send error: {}", e);
                    break;
                }
            }
        });
        self.send_handle = Some(send_handle);

        // Receive loop
        let event_tx = self.event_tx.clone();
        let receive_handle = tokio::spawn(async move {
            let mut confirmed_segments: Vec<String> = Vec::new();
            let mut last_transcript = RecognitionTranscript::empty();

            while let Some(msg_result) = ws_source.next().await {
                match msg_result {
                    Ok(Message::Text(text)) => {
                        let text_str: &str = &text;
                        match deepgram_protocol::parse_response(text_str) {
                            Ok(Some(response)) => {
                                let trimmed = response.transcript.clone();
                                let is_final = response.is_final;

                                if trimmed.is_empty() && (!is_final || confirmed_segments.is_empty())
                                {
                                    continue;
                                }

                                let mut next_confirmed = confirmed_segments.clone();
                                let mut partial_text = String::new();

                                if !trimmed.is_empty() {
                                    let existing = next_confirmed.join("");
                                    let normalized =
                                        deepgram_protocol::normalize_segment(&trimmed, &existing);
                                    if is_final {
                                        next_confirmed.push(normalized);
                                    } else {
                                        partial_text = normalized;
                                    }
                                }

                                let authoritative: String = next_confirmed
                                    .iter()
                                    .chain(if partial_text.is_empty() {
                                        None
                                    } else {
                                        Some(&partial_text)
                                    })
                                    .cloned()
                                    .collect::<Vec<_>>()
                                    .join("");

                                let transcript = RecognitionTranscript {
                                    confirmed_segments: next_confirmed.clone(),
                                    partial_text,
                                    authoritative_text: authoritative,
                                    is_final,
                                };

                                if transcript != last_transcript {
                                    last_transcript = transcript.clone();
                                    confirmed_segments = next_confirmed;

                                    debug!(
                                        confirmed = transcript.confirmed_segments.len(),
                                        partial_len = transcript.partial_text.len(),
                                        is_final,
                                        "Deepgram transcript update"
                                    );
                                    let _ =
                                        event_tx.send(RecognitionEvent::Transcript(transcript));
                                }
                            }
                            Ok(None) => {
                                // Non-Results message type (metadata, etc.)
                            }
                            Err(e) => {
                                warn!("Deepgram parse error: {}", e);
                            }
                        }
                    }
                    Ok(Message::Close(_)) => {
                        debug!("Deepgram WebSocket closed");
                        let _ = event_tx.send(RecognitionEvent::Completed);
                        break;
                    }
                    Err(e) => {
                        debug!("Deepgram receive error: {}", e);
                        let _ = event_tx.send(RecognitionEvent::Completed);
                        break;
                    }
                    _ => {}
                }
            }

            debug!("Deepgram receive loop ended");
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
            self.did_request_close = true;
            tx.send(WsOutbound::Text(
                deepgram_protocol::close_stream_message().to_string(),
            ))
            .ok();
            info!(
                packets = self.audio_packet_count,
                "Sent Deepgram CloseStream"
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
        debug!("Deepgram client disconnected");
    }

    fn take_event_rx(&mut self) -> Option<mpsc::UnboundedReceiver<RecognitionEvent>> {
        self.event_rx.take()
    }
}
