//! Volcengine streaming ASR WebSocket client.
//!
//! Uses the Volcengine binary protocol (volc_protocol.rs).

use super::provider::VolcanoASRConfig;
use super::traits::{ASRRequestOptions, RecognitionEvent, RecognitionTranscript, SpeechRecognizer};
use crate::protocol::volc_protocol;
use anyhow::{Context, Result};
use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

const ENDPOINT: &str = "wss://openspeech.bytedance.com/api/v3/sauc/bigmodel_async";

pub struct VolcASRClient {
    config: VolcanoASRConfig,
    event_tx: mpsc::UnboundedSender<RecognitionEvent>,
    event_rx: Option<mpsc::UnboundedReceiver<RecognitionEvent>>,
    ws_tx: Option<mpsc::UnboundedSender<Vec<u8>>>,
    receive_handle: Option<tokio::task::JoinHandle<()>>,
    send_handle: Option<tokio::task::JoinHandle<()>>,
    audio_packet_count: u64,
}

impl VolcASRClient {
    pub fn new(config: VolcanoASRConfig) -> Self {
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
impl SpeechRecognizer for VolcASRClient {
    async fn connect(&mut self, options: &ASRRequestOptions) -> Result<()> {
        let connect_id = Uuid::new_v4().to_string();
        let is_cloud_proxy = options.cloud_proxy_url.is_some();

        let target_url = if let Some(ref proxy_url) = options.cloud_proxy_url {
            proxy_url.clone()
        } else {
            ENDPOINT.to_string()
        };

        // Build WebSocket request with auth headers for direct connections
        let request = target_url
            .parse::<http::Uri>()
            .context("invalid WebSocket URL")?;

        let ws_url = target_url.clone();

        // For direct connections, we need custom headers.
        // tokio-tungstenite accepts a Request with headers.
        let http_request = if !is_cloud_proxy {
            http::Request::builder()
                .uri(&ws_url)
                .header("X-Api-App-Key", &self.config.app_key)
                .header("X-Api-Access-Key", &self.config.access_key)
                .header("X-Api-Resource-Id", &self.config.resource_id)
                .header("X-Api-Connect-Id", &connect_id)
                .header("Host", request.host().unwrap_or("openspeech.bytedance.com"))
                .header("Connection", "Upgrade")
                .header("Upgrade", "websocket")
                .header("Sec-WebSocket-Version", "13")
                .header(
                    "Sec-WebSocket-Key",
                    tokio_tungstenite::tungstenite::handshake::client::generate_key(),
                )
                .body(())
                .context("failed to build HTTP request")?
        } else {
            http::Request::builder()
                .uri(&ws_url)
                .header(
                    "Host",
                    request.host().unwrap_or("api.type4me.com"),
                )
                .header("Connection", "Upgrade")
                .header("Upgrade", "websocket")
                .header("Sec-WebSocket-Version", "13")
                .header(
                    "Sec-WebSocket-Key",
                    tokio_tungstenite::tungstenite::handshake::client::generate_key(),
                )
                .body(())
                .context("failed to build HTTP request")?
        };

        let (ws_stream, _response) =
            tokio_tungstenite::connect_async(http_request)
                .await
                .context("WebSocket connection failed")?;

        let (mut ws_sink, mut ws_source) = ws_stream.split();

        // Build and send full_client_request
        let uid = Uuid::new_v4().to_string();
        let request_json = volc_protocol::build_start_request(
            &uid,
            &connect_id,
            "zh",
            &options.hotwords,
            options.enable_punc,
            options.boosting_table_id.as_deref(),
        );
        let init_message = volc_protocol::encode_full_client_request(&request_json);

        info!(
            bytes = init_message.len(),
            connect_id = %connect_id,
            "Sending full_client_request"
        );
        ws_sink
            .send(Message::Binary(init_message.into()))
            .await
            .context("failed to send full_client_request")?;
        info!("full_client_request sent OK");

        self.audio_packet_count = 0;

        // Channel for outbound WS messages
        let (ws_msg_tx, mut ws_msg_rx) = mpsc::unbounded_channel::<Vec<u8>>();
        self.ws_tx = Some(ws_msg_tx);

        // Spawn send loop: forwards queued binary messages to the WebSocket
        let send_handle = tokio::spawn(async move {
            while let Some(data) = ws_msg_rx.recv().await {
                if let Err(e) = ws_sink.send(Message::Binary(data.into())).await {
                    debug!("WS send error (may be normal on close): {}", e);
                    break;
                }
            }
        });
        self.send_handle = Some(send_handle);

        // Spawn receive loop
        let event_tx = self.event_tx.clone();
        let receive_handle = tokio::spawn(async move {
            let mut last_transcript = RecognitionTranscript::empty();

            while let Some(msg_result) = ws_source.next().await {
                match msg_result {
                    Ok(Message::Binary(data)) => {
                        let data_vec: Vec<u8> = data.into();
                        if data_vec.len() < 2 {
                            continue;
                        }

                        let msg_type = (data_vec[1] >> 4) & 0x0F;

                        // Server error (0x0F)
                        if msg_type == volc_protocol::MSG_SERVER_ERROR {
                            if let Err(e) = volc_protocol::decode_server_message(&data_vec) {
                                error!("Volcengine server error: {}", e);
                                let _ = event_tx.send(RecognitionEvent::Error(e.to_string()));
                            }
                            let _ = event_tx.send(RecognitionEvent::Completed);
                            break;
                        }

                        // Parse server response
                        match volc_protocol::decode_server_message(&data_vec) {
                            Ok(server_msg) => {
                                let result = volc_protocol::parse_asr_result(&server_msg.payload);
                                let is_final =
                                    server_msg.flags == volc_protocol::FLAG_ASYNC_FINAL;

                                let transcript = make_transcript(&result, is_final);
                                if transcript != last_transcript {
                                    last_transcript = transcript.clone();
                                    debug!(
                                        confirmed = transcript.confirmed_segments.len(),
                                        partial_len = transcript.partial_text.len(),
                                        is_final,
                                        "Transcript update"
                                    );
                                    let _ =
                                        event_tx.send(RecognitionEvent::Transcript(transcript));
                                }
                            }
                            Err(e) => {
                                warn!("Decode error: {}", e);
                            }
                        }
                    }
                    Ok(Message::Close(_)) => {
                        debug!("WebSocket closed by server");
                        let _ = event_tx.send(RecognitionEvent::Completed);
                        break;
                    }
                    Err(e) => {
                        debug!("WebSocket receive error: {}", e);
                        let _ = event_tx.send(RecognitionEvent::Completed);
                        break;
                    }
                    _ => {}
                }
            }

            debug!("Volcengine receive loop ended");
        });
        self.receive_handle = Some(receive_handle);

        Ok(())
    }

    async fn send_audio(&mut self, data: &[u8]) -> Result<()> {
        if let Some(ref tx) = self.ws_tx {
            let packet = volc_protocol::encode_audio_packet(data, false);
            tx.send(packet).ok();
            self.audio_packet_count += 1;
        }
        Ok(())
    }

    async fn end_audio(&mut self) -> Result<()> {
        if let Some(ref tx) = self.ws_tx {
            let packet = volc_protocol::encode_audio_packet(&[], true);
            tx.send(packet).ok();
            info!("Sent last audio packet (isLast=true)");
        }
        Ok(())
    }

    async fn disconnect(&mut self) {
        // Drop the send channel to stop the send loop
        self.ws_tx.take();

        if let Some(h) = self.send_handle.take() {
            h.abort();
        }
        if let Some(h) = self.receive_handle.take() {
            h.abort();
        }

        debug!("Volcengine client disconnected");
    }

    fn take_event_rx(&mut self) -> Option<mpsc::UnboundedReceiver<RecognitionEvent>> {
        self.event_rx.take()
    }
}

fn make_transcript(result: &volc_protocol::VolcASRResult, is_final: bool) -> RecognitionTranscript {
    let confirmed_segments: Vec<String> = result
        .utterances
        .iter()
        .filter(|u| u.definite && !u.text.is_empty())
        .map(|u| u.text.clone())
        .collect();

    let partial_text = result
        .utterances
        .iter()
        .rev()
        .find(|u| !u.definite && !u.text.is_empty())
        .map(|u| u.text.clone())
        .unwrap_or_default();

    let composed: String = confirmed_segments
        .iter()
        .chain(if partial_text.is_empty() {
            None
        } else {
            Some(&partial_text)
        })
        .cloned()
        .collect::<Vec<_>>()
        .join("");

    let authoritative_text = if result.text.is_empty() {
        composed
    } else {
        result.text.clone()
    };

    RecognitionTranscript {
        confirmed_segments,
        partial_text,
        authoritative_text,
        is_final,
    }
}
