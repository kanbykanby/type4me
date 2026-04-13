//! Cloud ASR client that talks to the Type4Me backend's /asr WebSocket endpoint.
//!
//! Mirrors the macOS CloudASRClient.swift implementation.
//!
//! Flow:
//! 1. Get JWT token from CloudAuthManager (via auth module)
//! 2. Determine region (CN vs overseas)
//! 3. Build proxy URL: `{api_endpoint}/asr?token={jwt}&device_id={id}`
//! 4. Delegate to Volcengine protocol (CN) or Soniox protocol (overseas)
//! 5. Proxy audio and receive transcripts

use super::provider::{SonioxASRConfig, VolcanoASRConfig};
use super::soniox_client::SonioxASRClient;
use super::traits::{ASRRequestOptions, RecognitionEvent, SpeechRecognizer};
use super::volc_client::VolcASRClient;
use anyhow::{bail, Result};
use async_trait::async_trait;
use tokio::sync::mpsc;
use tracing::info;

/// Cloud region, determines which upstream ASR protocol to use.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CloudRegion {
    /// China: use Volcengine protocol through proxy
    CN,
    /// Overseas: use Soniox protocol through proxy
    Overseas,
}

/// Configuration for the Cloud ASR proxy.
/// In production, these come from CloudAuthManager and CloudConfig.
pub struct CloudContext {
    pub access_token: String,
    pub device_id: String,
    pub api_endpoint: String,
    pub region: CloudRegion,
}

pub struct CloudASRClient {
    inner: Option<InnerClient>,
    event_tx: mpsc::UnboundedSender<RecognitionEvent>,
    event_rx: Option<mpsc::UnboundedReceiver<RecognitionEvent>>,
    cloud_context: Option<CloudContext>,
}

enum InnerClient {
    Volc(VolcASRClient),
    Soniox(SonioxASRClient),
}

impl CloudASRClient {
    pub fn new() -> Self {
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        Self {
            inner: None,
            event_tx,
            event_rx: Some(event_rx),
            cloud_context: None,
        }
    }

    /// Set the cloud context before calling connect().
    /// This must be called by the session layer after obtaining a JWT token.
    pub fn set_context(&mut self, ctx: CloudContext) {
        self.cloud_context = Some(ctx);
    }
}

#[async_trait]
impl SpeechRecognizer for CloudASRClient {
    async fn connect(&mut self, options: &ASRRequestOptions) -> Result<()> {
        let ctx = self
            .cloud_context
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("CloudASRClient: no cloud context set (not authenticated?)"))?;

        // Build the proxy URL with auth
        let proxy_url = format!(
            "{}/asr?token={}&device_id={}",
            ctx.api_endpoint, ctx.access_token, ctx.device_id
        );

        let mut proxy_options = options.clone();
        proxy_options.cloud_proxy_url = Some(proxy_url);

        match ctx.region {
            CloudRegion::CN => {
                // China: speak Volcengine protocol through proxy
                let config = VolcanoASRConfig {
                    app_key: "cloud".to_string(),
                    access_key: "cloud".to_string(),
                    resource_id: VolcanoASRConfig::RESOURCE_SEED_ASR.to_string(),
                };
                let mut client = VolcASRClient::new(config);
                client.connect(&proxy_options).await?;

                // Forward events from inner client to our channel
                if let Some(mut inner_rx) = client.take_event_rx() {
                    let tx = self.event_tx.clone();
                    tokio::spawn(async move {
                        while let Some(event) = inner_rx.recv().await {
                            if tx.send(event).is_err() {
                                break;
                            }
                        }
                    });
                }

                self.inner = Some(InnerClient::Volc(client));
                info!("Connected via Cloud proxy (CN/Volcengine)");
            }
            CloudRegion::Overseas => {
                // Overseas: speak Soniox protocol through proxy
                let config = SonioxASRConfig {
                    api_key: "cloud".to_string(),
                    model: SonioxASRConfig::DEFAULT_MODEL.to_string(),
                };
                let mut client = SonioxASRClient::new(config);
                client.connect(&proxy_options).await?;

                if let Some(mut inner_rx) = client.take_event_rx() {
                    let tx = self.event_tx.clone();
                    tokio::spawn(async move {
                        while let Some(event) = inner_rx.recv().await {
                            if tx.send(event).is_err() {
                                break;
                            }
                        }
                    });
                }

                self.inner = Some(InnerClient::Soniox(client));
                info!("Connected via Cloud proxy (Overseas/Soniox)");
            }
        }

        Ok(())
    }

    async fn send_audio(&mut self, data: &[u8]) -> Result<()> {
        match &mut self.inner {
            Some(InnerClient::Volc(c)) => c.send_audio(data).await,
            Some(InnerClient::Soniox(c)) => c.send_audio(data).await,
            None => bail!("CloudASRClient: not connected"),
        }
    }

    async fn end_audio(&mut self) -> Result<()> {
        match &mut self.inner {
            Some(InnerClient::Volc(c)) => c.end_audio().await,
            Some(InnerClient::Soniox(c)) => c.end_audio().await,
            None => bail!("CloudASRClient: not connected"),
        }
    }

    async fn disconnect(&mut self) {
        match &mut self.inner {
            Some(InnerClient::Volc(c)) => c.disconnect().await,
            Some(InnerClient::Soniox(c)) => c.disconnect().await,
            None => {}
        }
        self.inner = None;
    }

    fn take_event_rx(&mut self) -> Option<mpsc::UnboundedReceiver<RecognitionEvent>> {
        self.event_rx.take()
    }
}
