use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use serde::Deserialize;
use tracing::{debug, warn};

use super::traits::LLMClient;

/// LLM client that routes through the Type4Me Cloud proxy.
///
/// The user's JWT token is attached as a Bearer header.
/// The server picks an appropriate backend model.
pub struct CloudLLMClient {
    http: reqwest::Client,
    endpoint: String,
    token: String,
}

impl CloudLLMClient {
    pub fn new(endpoint: &str, token: &str) -> Self {
        Self {
            http: reqwest::Client::new(),
            endpoint: endpoint.to_string(),
            token: token.to_string(),
        }
    }
}

#[derive(Deserialize)]
struct CloudLLMResponse {
    result: Option<String>,
    error: Option<String>,
}

#[async_trait]
impl LLMClient for CloudLLMClient {
    async fn process(&self, text: &str, system_prompt: &str) -> Result<String> {
        let url = format!("{}/api/llm", self.endpoint);

        debug!(url = %url, text_len = text.len(), "cloud LLM request");

        let resp = self
            .http
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.token))
            .json(&serde_json::json!({
                "text": text,
                "prompt": system_prompt,
                "mode": "polish",
            }))
            .send()
            .await
            .context("cloud LLM request failed")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            bail!("cloud LLM error ({status}): {body}");
        }

        let data: CloudLLMResponse = resp
            .json()
            .await
            .context("bad cloud LLM response JSON")?;

        if let Some(err) = data.error {
            warn!(error = %err, "cloud LLM returned error");
            bail!("cloud LLM error: {err}");
        }

        data.result
            .ok_or_else(|| anyhow::anyhow!("cloud LLM response missing 'result' field"))
    }
}
