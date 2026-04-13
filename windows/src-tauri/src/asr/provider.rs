use super::traits::{ASRProviderConfig, CredentialField};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Provider Enum
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ASRProvider {
    Cloud,
    Volcano,
    Soniox,
    Deepgram,
    ElevenLabs,
    OpenAI,
    Sherpa,
}

impl ASRProvider {
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Cloud => "Type4Me Cloud",
            Self::Volcano => "Volcano (Doubao)",
            Self::Soniox => "Soniox",
            Self::Deepgram => "Deepgram",
            Self::ElevenLabs => "ElevenLabs",
            Self::OpenAI => "OpenAI",
            Self::Sherpa => "Local (SenseVoice)",
        }
    }

    pub fn is_streaming(&self) -> bool {
        !matches!(self, Self::OpenAI)
    }

    pub fn is_local(&self) -> bool {
        matches!(self, Self::Sherpa)
    }
}

// ---------------------------------------------------------------------------
// Provider Info (for frontend)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize)]
pub struct ASRProviderInfo {
    pub id: ASRProvider,
    pub name: String,
    pub description: String,
    pub is_streaming: bool,
    pub requires_credentials: bool,
    pub credential_fields: Vec<CredentialField>,
}

pub fn all_providers() -> Vec<ASRProviderInfo> {
    vec![
        ASRProviderInfo {
            id: ASRProvider::Cloud,
            name: "Type4Me Cloud".into(),
            description: "Cloud proxy, no API keys needed".into(),
            is_streaming: true,
            requires_credentials: false,
            credential_fields: CloudASRConfig::credential_fields(),
        },
        ASRProviderInfo {
            id: ASRProvider::Volcano,
            name: "Volcano (Doubao)".into(),
            description: "Volcengine streaming ASR, best for Chinese".into(),
            is_streaming: true,
            requires_credentials: true,
            credential_fields: VolcanoASRConfig::credential_fields(),
        },
        ASRProviderInfo {
            id: ASRProvider::Soniox,
            name: "Soniox".into(),
            description: "Soniox real-time streaming ASR".into(),
            is_streaming: true,
            requires_credentials: true,
            credential_fields: SonioxASRConfig::credential_fields(),
        },
        ASRProviderInfo {
            id: ASRProvider::Deepgram,
            name: "Deepgram".into(),
            description: "Deepgram Nova streaming ASR".into(),
            is_streaming: true,
            requires_credentials: true,
            credential_fields: DeepgramASRConfig::credential_fields(),
        },
        ASRProviderInfo {
            id: ASRProvider::ElevenLabs,
            name: "ElevenLabs".into(),
            description: "ElevenLabs Scribe v2 real-time ASR".into(),
            is_streaming: true,
            requires_credentials: true,
            credential_fields: ElevenLabsASRConfig::credential_fields(),
        },
        ASRProviderInfo {
            id: ASRProvider::OpenAI,
            name: "OpenAI".into(),
            description: "OpenAI Whisper / GPT-4o batch transcription".into(),
            is_streaming: false,
            requires_credentials: true,
            credential_fields: OpenAIASRConfig::credential_fields(),
        },
        ASRProviderInfo {
            id: ASRProvider::Sherpa,
            name: "Local (SenseVoice)".into(),
            description: "On-device ASR via sherpa-onnx, no network needed".into(),
            is_streaming: true,
            requires_credentials: false,
            credential_fields: SherpaASRConfig::credential_fields(),
        },
    ]
}

// ===========================================================================
// Config Structs
// ===========================================================================

// ---- Cloud (no credentials, uses JWT) ----

#[derive(Clone, Debug)]
pub struct CloudASRConfig;

impl ASRProviderConfig for CloudASRConfig {
    fn provider(&self) -> ASRProvider {
        ASRProvider::Cloud
    }

    fn credential_fields() -> Vec<CredentialField> {
        vec![]
    }

    fn from_credentials(_creds: &HashMap<String, String>) -> Option<Box<dyn ASRProviderConfig>> {
        Some(Box::new(CloudASRConfig))
    }

    fn is_valid(&self) -> bool {
        true
    }
}

// ---- Volcano ----

#[derive(Clone, Debug)]
pub struct VolcanoASRConfig {
    pub app_key: String,
    pub access_key: String,
    pub resource_id: String,
}

impl VolcanoASRConfig {
    pub const RESOURCE_SEED_ASR: &'static str = "volc.seedasr.sauc.duration";
    pub const RESOURCE_BIG_ASR: &'static str = "volc.bigasr.sauc.duration";
}

impl ASRProviderConfig for VolcanoASRConfig {
    fn provider(&self) -> ASRProvider {
        ASRProvider::Volcano
    }

    fn credential_fields() -> Vec<CredentialField> {
        vec![
            CredentialField::new("appKey", "App ID", false, "APPID"),
            CredentialField::new("accessKey", "Access Token", true, "Access token"),
            CredentialField::new("resourceId", "Model", false, "volc.seedasr.sauc.duration"),
        ]
    }

    fn from_credentials(creds: &HashMap<String, String>) -> Option<Box<dyn ASRProviderConfig>> {
        let app_key = creds.get("appKey").filter(|s| !s.is_empty())?.clone();
        let access_key = creds.get("accessKey").filter(|s| !s.is_empty())?.clone();
        let resource_id = creds
            .get("resourceId")
            .filter(|s| !s.is_empty())
            .cloned()
            .unwrap_or_else(|| Self::RESOURCE_SEED_ASR.to_string());
        Some(Box::new(VolcanoASRConfig {
            app_key,
            access_key,
            resource_id,
        }))
    }

    fn is_valid(&self) -> bool {
        !self.app_key.is_empty() && !self.access_key.is_empty()
    }
}

// ---- Soniox ----

#[derive(Clone, Debug)]
pub struct SonioxASRConfig {
    pub api_key: String,
    pub model: String,
}

impl SonioxASRConfig {
    pub const DEFAULT_MODEL: &'static str = "stt-rt-v4";
}

impl ASRProviderConfig for SonioxASRConfig {
    fn provider(&self) -> ASRProvider {
        ASRProvider::Soniox
    }

    fn credential_fields() -> Vec<CredentialField> {
        vec![CredentialField::new(
            "apiKey",
            "API Key",
            true,
            "Paste your API Key",
        )]
    }

    fn from_credentials(creds: &HashMap<String, String>) -> Option<Box<dyn ASRProviderConfig>> {
        let api_key = creds.get("apiKey").filter(|s| !s.is_empty())?.clone();
        let model = creds
            .get("model")
            .filter(|s| !s.is_empty())
            .cloned()
            .unwrap_or_else(|| Self::DEFAULT_MODEL.to_string());
        Some(Box::new(SonioxASRConfig { api_key, model }))
    }

    fn is_valid(&self) -> bool {
        !self.api_key.is_empty()
    }
}

// ---- Deepgram ----

#[derive(Clone, Debug)]
pub struct DeepgramASRConfig {
    pub api_key: String,
    pub model: String,
    pub language: String,
}

impl DeepgramASRConfig {
    pub const DEFAULT_MODEL: &'static str = "nova-3";
    pub const DEFAULT_LANGUAGE: &'static str = "zh";
}

impl ASRProviderConfig for DeepgramASRConfig {
    fn provider(&self) -> ASRProvider {
        ASRProvider::Deepgram
    }

    fn credential_fields() -> Vec<CredentialField> {
        vec![
            CredentialField::new("apiKey", "API Key", true, "Paste your API Key"),
            CredentialField::new("model", "Model", false, "nova-3"),
            CredentialField::new("language", "Language", false, "zh"),
        ]
    }

    fn from_credentials(creds: &HashMap<String, String>) -> Option<Box<dyn ASRProviderConfig>> {
        let api_key = creds.get("apiKey").filter(|s| !s.is_empty())?.clone();
        let model = creds
            .get("model")
            .filter(|s| !s.is_empty())
            .cloned()
            .unwrap_or_else(|| Self::DEFAULT_MODEL.to_string());
        let language = creds
            .get("language")
            .filter(|s| !s.is_empty())
            .cloned()
            .unwrap_or_else(|| Self::DEFAULT_LANGUAGE.to_string());
        Some(Box::new(DeepgramASRConfig {
            api_key,
            model,
            language,
        }))
    }

    fn is_valid(&self) -> bool {
        !self.api_key.is_empty() && !self.model.is_empty() && !self.language.is_empty()
    }
}

// ---- ElevenLabs ----

#[derive(Clone, Debug)]
pub struct ElevenLabsASRConfig {
    pub api_key: String,
    pub language: String,
}

impl ASRProviderConfig for ElevenLabsASRConfig {
    fn provider(&self) -> ASRProvider {
        ASRProvider::ElevenLabs
    }

    fn credential_fields() -> Vec<CredentialField> {
        vec![
            CredentialField::new("apiKey", "API Key", true, "Paste your API Key"),
            CredentialField::new("language", "Language", false, "auto"),
        ]
    }

    fn from_credentials(creds: &HashMap<String, String>) -> Option<Box<dyn ASRProviderConfig>> {
        let api_key = creds.get("apiKey").filter(|s| !s.is_empty())?.clone();
        let language = creds
            .get("language")
            .cloned()
            .unwrap_or_default();
        Some(Box::new(ElevenLabsASRConfig { api_key, language }))
    }

    fn is_valid(&self) -> bool {
        !self.api_key.is_empty()
    }
}

// ---- OpenAI ----

#[derive(Clone, Debug)]
pub struct OpenAIASRConfig {
    pub api_key: String,
    pub model: String,
    pub base_url: String,
}

impl OpenAIASRConfig {
    pub const DEFAULT_MODEL: &'static str = "gpt-4o-transcribe";
    pub const DEFAULT_BASE_URL: &'static str = "https://api.openai.com/v1";
}

impl ASRProviderConfig for OpenAIASRConfig {
    fn provider(&self) -> ASRProvider {
        ASRProvider::OpenAI
    }

    fn credential_fields() -> Vec<CredentialField> {
        vec![
            CredentialField::new("apiKey", "API Key", true, "sk-..."),
            CredentialField::new("model", "Model", false, "gpt-4o-transcribe"),
        ]
    }

    fn from_credentials(creds: &HashMap<String, String>) -> Option<Box<dyn ASRProviderConfig>> {
        let api_key = creds.get("apiKey").filter(|s| !s.is_empty())?.clone();
        let model = creds
            .get("model")
            .filter(|s| !s.is_empty())
            .cloned()
            .unwrap_or_else(|| Self::DEFAULT_MODEL.to_string());
        let base_url = creds
            .get("baseURL")
            .filter(|s| !s.is_empty())
            .cloned()
            .unwrap_or_else(|| Self::DEFAULT_BASE_URL.to_string());
        Some(Box::new(OpenAIASRConfig {
            api_key,
            model,
            base_url,
        }))
    }

    fn is_valid(&self) -> bool {
        !self.api_key.is_empty()
    }
}

// ---- Sherpa (local) ----

#[derive(Clone, Debug)]
pub struct SherpaASRConfig {
    pub model_path: String,
}

impl ASRProviderConfig for SherpaASRConfig {
    fn provider(&self) -> ASRProvider {
        ASRProvider::Sherpa
    }

    fn credential_fields() -> Vec<CredentialField> {
        vec![]
    }

    fn from_credentials(creds: &HashMap<String, String>) -> Option<Box<dyn ASRProviderConfig>> {
        let model_path = creds
            .get("modelPath")
            .cloned()
            .unwrap_or_default();
        Some(Box::new(SherpaASRConfig { model_path }))
    }

    fn is_valid(&self) -> bool {
        !self.model_path.is_empty()
    }
}
