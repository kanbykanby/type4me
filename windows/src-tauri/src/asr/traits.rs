use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio::sync::mpsc;

// ---------------------------------------------------------------------------
// Transcript
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecognitionTranscript {
    pub confirmed_segments: Vec<String>,
    pub partial_text: String,
    pub authoritative_text: String,
    pub is_final: bool,
}

impl RecognitionTranscript {
    pub fn empty() -> Self {
        Self::default()
    }

    /// Confirmed text joined with current partial, for display purposes.
    pub fn composed_text(&self) -> String {
        let mut pieces: Vec<&str> = self.confirmed_segments.iter().map(|s| s.as_str()).collect();
        if !self.partial_text.is_empty() {
            pieces.push(&self.partial_text);
        }
        pieces.join("")
    }

    /// Best available text: authoritative if present, otherwise composed.
    pub fn display_text(&self) -> String {
        if self.authoritative_text.is_empty() {
            self.composed_text()
        } else {
            self.authoritative_text.clone()
        }
    }
}

// ---------------------------------------------------------------------------
// Events
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub enum RecognitionEvent {
    Ready,
    Transcript(RecognitionTranscript),
    Error(String),
    Completed,
}

// ---------------------------------------------------------------------------
// Request Options
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct ASRRequestOptions {
    pub enable_punc: bool,
    pub hotwords: Vec<String>,
    pub boosting_table_id: Option<String>,
    /// When set, ASR clients connect to this URL instead of their default
    /// endpoint. Used by CloudASRClient to route through the Type4Me Cloud proxy.
    pub cloud_proxy_url: Option<String>,
}

impl Default for ASRRequestOptions {
    fn default() -> Self {
        Self {
            enable_punc: true,
            hotwords: Vec::new(),
            boosting_table_id: None,
            cloud_proxy_url: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Credential Field descriptor (mirrors macOS CredentialField)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize)]
pub struct CredentialField {
    pub key: String,
    pub label: String,
    pub is_secure: bool,
    pub placeholder: String,
}

impl CredentialField {
    pub fn new(key: &str, label: &str, is_secure: bool, placeholder: &str) -> Self {
        Self {
            key: key.to_string(),
            label: label.to_string(),
            is_secure,
            placeholder: placeholder.to_string(),
        }
    }
}

// ---------------------------------------------------------------------------
// Provider Config trait
// ---------------------------------------------------------------------------

pub trait ASRProviderConfig: Send + Sync {
    fn provider(&self) -> super::provider::ASRProvider;
    fn credential_fields() -> Vec<CredentialField>
    where
        Self: Sized;
    fn from_credentials(creds: &HashMap<String, String>) -> Option<Box<dyn ASRProviderConfig>>
    where
        Self: Sized;
    fn is_valid(&self) -> bool;
}

// ---------------------------------------------------------------------------
// SpeechRecognizer trait
// ---------------------------------------------------------------------------

#[async_trait]
pub trait SpeechRecognizer: Send + Sync {
    /// Open the WebSocket / HTTP connection and prepare for audio.
    async fn connect(&mut self, options: &ASRRequestOptions) -> anyhow::Result<()>;

    /// Feed a chunk of 16-bit PCM audio (16 kHz, mono).
    async fn send_audio(&mut self, data: &[u8]) -> anyhow::Result<()>;

    /// Signal that no more audio will be sent.
    async fn end_audio(&mut self) -> anyhow::Result<()>;

    /// Tear down the connection and release resources.
    async fn disconnect(&mut self);

    /// Take the event receiver. Returns `None` after the first call.
    fn take_event_rx(&mut self) -> Option<mpsc::UnboundedReceiver<RecognitionEvent>>;
}
