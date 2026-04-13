use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use serde::Deserialize;
use tracing::debug;

use super::traits::LLMClient;

/// Claude Messages API client.
///
/// Uses the Anthropic REST API directly (not OpenAI-compatible).
pub struct ClaudeLLMClient {
    http: reqwest::Client,
    api_key: String,
    model: String,
}

impl ClaudeLLMClient {
    pub fn new(api_key: &str, model: &str) -> Self {
        Self {
            http: reqwest::Client::new(),
            api_key: api_key.to_string(),
            model: model.to_string(),
        }
    }
}

// ---------------------------------------------------------------------------
// Response types
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct MessagesResponse {
    content: Vec<ContentBlock>,
}

#[derive(Deserialize)]
struct ContentBlock {
    #[serde(rename = "type")]
    block_type: String,
    text: Option<String>,
}

#[derive(Deserialize)]
struct ErrorResponse {
    error: Option<ErrorDetail>,
}

#[derive(Deserialize)]
struct ErrorDetail {
    message: Option<String>,
}

#[async_trait]
impl LLMClient for ClaudeLLMClient {
    async fn process(&self, text: &str, system_prompt: &str) -> Result<String> {
        let url = "https://api.anthropic.com/v1/messages";

        debug!(
            model = %self.model,
            text_len = text.len(),
            "Claude Messages API request"
        );

        let body = serde_json::json!({
            "model": &self.model,
            "max_tokens": 4096,
            "system": system_prompt,
            "messages": [
                { "role": "user", "content": text },
            ],
        });

        let resp = self
            .http
            .post(url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .context("Claude API request failed")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body_text = resp.text().await.unwrap_or_default();

            // Try to extract a useful error message
            if let Ok(err_resp) = serde_json::from_str::<ErrorResponse>(&body_text) {
                if let Some(detail) = err_resp.error {
                    let msg = detail.message.unwrap_or_else(|| body_text.clone());
                    bail!("Claude API error ({status}): {msg}");
                }
            }

            bail!("Claude API error ({status}): {body_text}");
        }

        let data: MessagesResponse = resp
            .json()
            .await
            .context("bad Claude API response JSON")?;

        // Extract text from the first text content block
        let result = data
            .content
            .into_iter()
            .find(|b| b.block_type == "text")
            .and_then(|b| b.text)
            .unwrap_or_default();

        if result.is_empty() {
            bail!("Claude returned empty response");
        }

        debug!(result_len = result.len(), "Claude response received");
        Ok(result)
    }
}
