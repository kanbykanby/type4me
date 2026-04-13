use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::asr::traits::CredentialField;

// ---------------------------------------------------------------------------
// Provider enum
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LLMProvider {
    Cloud,
    OpenAI,
    Claude,
    Deepseek,
    Doubao,
}

impl LLMProvider {
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Cloud => "Type4Me Cloud",
            Self::OpenAI => "OpenAI",
            Self::Claude => "Claude",
            Self::Deepseek => "Deepseek",
            Self::Doubao => "Doubao (Volcengine Seed)",
        }
    }
}

// ---------------------------------------------------------------------------
// Provider info (for frontend settings UI)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize)]
pub struct LLMProviderInfo {
    pub id: LLMProvider,
    pub name: String,
    pub description: String,
    pub requires_credentials: bool,
    pub credential_fields: Vec<CredentialField>,
    pub default_model: String,
    pub available_models: Vec<String>,
}

pub fn all_llm_providers() -> Vec<LLMProviderInfo> {
    vec![
        LLMProviderInfo {
            id: LLMProvider::Cloud,
            name: "Type4Me Cloud".into(),
            description: "Cloud proxy, uses your account quota".into(),
            requires_credentials: false,
            credential_fields: vec![],
            default_model: String::new(),
            available_models: vec![],
        },
        LLMProviderInfo {
            id: LLMProvider::OpenAI,
            name: "OpenAI".into(),
            description: "GPT-4o, GPT-4o-mini, etc.".into(),
            requires_credentials: true,
            credential_fields: vec![
                CredentialField::new("apiKey", "API Key", true, "sk-..."),
                CredentialField::new("baseURL", "Base URL", false, "https://api.openai.com/v1"),
                CredentialField::new("model", "Model", false, "gpt-4o-mini"),
            ],
            default_model: "gpt-4o-mini".into(),
            available_models: vec![
                "gpt-4o".into(),
                "gpt-4o-mini".into(),
                "gpt-4.1-mini".into(),
                "gpt-4.1-nano".into(),
            ],
        },
        LLMProviderInfo {
            id: LLMProvider::Claude,
            name: "Claude".into(),
            description: "Claude Sonnet, Haiku, etc.".into(),
            requires_credentials: true,
            credential_fields: vec![
                CredentialField::new("apiKey", "API Key", true, "sk-ant-..."),
                CredentialField::new("model", "Model", false, "claude-sonnet-4-20250514"),
            ],
            default_model: "claude-sonnet-4-20250514".into(),
            available_models: vec![
                "claude-sonnet-4-20250514".into(),
                "claude-haiku-4-20250514".into(),
            ],
        },
        LLMProviderInfo {
            id: LLMProvider::Deepseek,
            name: "Deepseek".into(),
            description: "Deepseek V3, R1".into(),
            requires_credentials: true,
            credential_fields: vec![
                CredentialField::new("apiKey", "API Key", true, "sk-..."),
                CredentialField::new("model", "Model", false, "deepseek-chat"),
            ],
            default_model: "deepseek-chat".into(),
            available_models: vec!["deepseek-chat".into(), "deepseek-reasoner".into()],
        },
        LLMProviderInfo {
            id: LLMProvider::Doubao,
            name: "Doubao (Volcengine Seed)".into(),
            description: "Doubao Pro, Lite models".into(),
            requires_credentials: true,
            credential_fields: vec![
                CredentialField::new("apiKey", "API Key", true, "Paste your API Key"),
                CredentialField::new("model", "Model", false, "doubao-1.5-pro-32k"),
            ],
            default_model: "doubao-1.5-pro-32k".into(),
            available_models: vec![
                "doubao-1.5-pro-32k".into(),
                "doubao-1.5-lite-32k".into(),
            ],
        },
    ]
}

// ---------------------------------------------------------------------------
// LLM client trait
// ---------------------------------------------------------------------------

#[async_trait]
pub trait LLMClient: Send + Sync {
    /// Process text with the given system prompt. Returns the processed text.
    async fn process(&self, text: &str, system_prompt: &str) -> anyhow::Result<String>;
}
