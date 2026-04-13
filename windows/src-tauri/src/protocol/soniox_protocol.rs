//! Soniox streaming ASR protocol.
//!
//! Mirrors the macOS SonioxProtocol.swift implementation.

use anyhow::Result;
use serde::Deserialize;

pub const DEFAULT_ENDPOINT: &str = "wss://stt-rt.soniox.com/transcribe-websocket";

/// Marker tokens that should be stripped from visible text.
const IGNORED_MARKERS: &[&str] = &["<end>", "<fin>"];

// ---------------------------------------------------------------------------
// Start message
// ---------------------------------------------------------------------------

/// Build the JSON config message sent as the first text frame.
///
/// When `cloud_proxy_url` is `Some`, the `api_key` is omitted (the proxy injects it).
pub fn build_start_message(
    api_key: &str,
    model: &str,
    hotwords: &[String],
    is_cloud_proxy: bool,
) -> String {
    let mut payload = serde_json::json!({
        "model": model,
        "audio_format": "pcm_s16le",
        "sample_rate": 16000,
        "num_channels": 1,
        "enable_endpoint_detection": true,
        "max_endpoint_delay_ms": 3000,
        "language_hints": ["zh", "en"],
        "language_hints_strict": true,
    });

    if !is_cloud_proxy {
        payload["api_key"] = serde_json::Value::String(api_key.to_string());
    }

    let terms: Vec<&str> = hotwords
        .iter()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();
    if !terms.is_empty() {
        payload["context"] = serde_json::json!({ "terms": terms });
    }

    serde_json::to_string(&payload).expect("JSON serialization cannot fail")
}

// ---------------------------------------------------------------------------
// Response parsing
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct SonioxResponse {
    pub transcript: Option<SonioxTranscriptUpdate>,
    pub is_finished: bool,
    pub error: Option<SonioxServerError>,
}

#[derive(Debug, Clone)]
pub struct SonioxTranscriptUpdate {
    pub finalized_text: String,
    pub partial_text: String,
}

#[derive(Debug, Clone)]
pub struct SonioxServerError {
    pub code: i64,
    pub message: String,
}

/// Parse a server JSON message into transcript update + status.
pub fn parse_response(text: &str) -> Result<SonioxResponse> {
    let raw: RawResponse = serde_json::from_str(text)?;

    let error = raw.error_code.map(|code| SonioxServerError {
        code,
        message: raw
            .error_message
            .unwrap_or_else(|| "Soniox request failed".to_string()),
    });

    let tokens = raw.tokens.unwrap_or_default();
    let finalized_text = visible_text(&tokens, true);
    let partial_text = visible_text(&tokens, false);

    let transcript = if !finalized_text.is_empty() || !partial_text.is_empty() {
        Some(SonioxTranscriptUpdate {
            finalized_text,
            partial_text,
        })
    } else {
        None
    };

    Ok(SonioxResponse {
        transcript,
        is_finished: raw.finished.unwrap_or(false),
        error,
    })
}

// ---------------------------------------------------------------------------
// Internals
// ---------------------------------------------------------------------------

fn visible_text(tokens: &[RawToken], is_final: bool) -> String {
    tokens
        .iter()
        .filter(|t| t.is_final.unwrap_or(false) == is_final)
        .filter_map(|t| {
            let text = t.text.as_deref()?;
            if IGNORED_MARKERS.contains(&text) {
                None
            } else {
                Some(text)
            }
        })
        .collect::<Vec<_>>()
        .join("")
}

#[derive(Deserialize)]
struct RawResponse {
    tokens: Option<Vec<RawToken>>,
    finished: Option<bool>,
    error_code: Option<i64>,
    error_message: Option<String>,
}

#[derive(Deserialize)]
struct RawToken {
    text: Option<String>,
    is_final: Option<bool>,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_start_message_direct() {
        let msg = build_start_message("test-key", "stt-rt-v4", &[], false);
        let parsed: serde_json::Value = serde_json::from_str(&msg).unwrap();
        assert_eq!(parsed["api_key"], "test-key");
        assert_eq!(parsed["model"], "stt-rt-v4");
        assert_eq!(parsed["sample_rate"], 16000);
    }

    #[test]
    fn test_build_start_message_cloud_proxy() {
        let msg = build_start_message("ignored", "stt-rt-v4", &[], true);
        let parsed: serde_json::Value = serde_json::from_str(&msg).unwrap();
        assert!(parsed.get("api_key").is_none());
    }

    #[test]
    fn test_parse_tokens() {
        let json = r#"{
            "tokens": [
                {"text": "hello", "is_final": true},
                {"text": " world", "is_final": false},
                {"text": "<end>", "is_final": true}
            ],
            "finished": false
        }"#;
        let resp = parse_response(json).unwrap();
        let t = resp.transcript.unwrap();
        assert_eq!(t.finalized_text, "hello");
        assert_eq!(t.partial_text, " world");
        assert!(!resp.is_finished);
    }

    #[test]
    fn test_parse_finished() {
        let json = r#"{"finished": true}"#;
        let resp = parse_response(json).unwrap();
        assert!(resp.is_finished);
        assert!(resp.transcript.is_none());
    }

    #[test]
    fn test_parse_error() {
        let json = r#"{"error_code": 401, "error_message": "Invalid API key"}"#;
        let resp = parse_response(json).unwrap();
        let err = resp.error.unwrap();
        assert_eq!(err.code, 401);
        assert_eq!(err.message, "Invalid API key");
    }
}
