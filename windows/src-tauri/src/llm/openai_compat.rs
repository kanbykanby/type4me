use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use serde::Deserialize;
use tracing::debug;

use super::traits::LLMClient;

/// OpenAI-compatible chat completions client.
///
/// Works with OpenAI, Deepseek, Doubao/Volcengine Seed, and any other
/// provider that implements the `/chat/completions` endpoint.
pub struct OpenAICompatClient {
    http: reqwest::Client,
    base_url: String,
    api_key: String,
    model: String,
}

impl OpenAICompatClient {
    pub fn new(base_url: &str, api_key: &str, model: &str) -> Self {
        // Normalize: strip trailing slash
        let base_url = base_url.trim_end_matches('/').to_string();
        Self {
            http: reqwest::Client::new(),
            base_url,
            api_key: api_key.to_string(),
            model: model.to_string(),
        }
    }

    /// Convenience constructors for known providers.

    pub fn openai(api_key: &str, model: &str) -> Self {
        Self::new("https://api.openai.com/v1", api_key, model)
    }

    pub fn deepseek(api_key: &str, model: &str) -> Self {
        Self::new("https://api.deepseek.com/v1", api_key, model)
    }

    pub fn doubao(api_key: &str, model: &str) -> Self {
        Self::new(
            "https://ark.cn-beijing.volces.com/api/v3",
            api_key,
            model,
        )
    }
}

// ---------------------------------------------------------------------------
// Response types (only the fields we need)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct ChatCompletion {
    choices: Vec<ChatChoice>,
}

#[derive(Deserialize)]
struct ChatChoice {
    message: ChatMessage,
}

#[derive(Deserialize)]
struct ChatMessage {
    content: Option<String>,
}

#[async_trait]
impl LLMClient for OpenAICompatClient {
    async fn process(&self, text: &str, system_prompt: &str) -> Result<String> {
        let url = format!("{}/chat/completions", self.base_url);

        debug!(
            url = %url,
            model = %self.model,
            text_len = text.len(),
            "OpenAI-compat LLM request"
        );

        let body = serde_json::json!({
            "model": &self.model,
            "messages": [
                { "role": "system", "content": system_prompt },
                { "role": "user", "content": text },
            ],
            "temperature": 0.3,
            "max_tokens": 4096,
        });

        let resp = self
            .http
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .context("LLM request failed")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            bail!("LLM API error ({status}): {body}");
        }

        let completion: ChatCompletion = resp
            .json()
            .await
            .context("bad chat completion response JSON")?;

        let content = completion
            .choices
            .into_iter()
            .next()
            .and_then(|c| c.message.content)
            .unwrap_or_default();

        if content.is_empty() {
            bail!("LLM returned empty response");
        }

        debug!(result_len = content.len(), "LLM response received");
        Ok(content)
    }
}
