//! Deepgram streaming ASR protocol.
//!
//! Mirrors the macOS DeepgramProtocol.swift implementation.

use anyhow::Result;
use serde::Deserialize;

pub const ENDPOINT: &str = "wss://api.deepgram.com/v1/listen";
const KEYWORD_INTENSITY: u8 = 2;
const MAX_URL_KEYTERMS: usize = 30;

// ---------------------------------------------------------------------------
// URL building
// ---------------------------------------------------------------------------

/// Build the full WebSocket URL with query parameters for Deepgram.
///
/// Auth is done via `Authorization: Token {api_key}` header, not query param.
pub fn build_ws_url(
    api_key: &str,
    model: &str,
    language: &str,
    enable_punc: bool,
    hotwords: &[String],
) -> String {
    let _ = api_key; // Auth via header, not URL

    let mut params = vec![
        format!("model={}", urlencoding(model)),
        format!("language={}", urlencoding(language)),
        "encoding=linear16".to_string(),
        "sample_rate=16000".to_string(),
        "channels=1".to_string(),
        "interim_results=true".to_string(),
        format!("punctuate={}", if enable_punc { "true" } else { "false" }),
        "smart_format=true".to_string(),
    ];

    let cleaned: Vec<&str> = hotwords
        .iter()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .take(MAX_URL_KEYTERMS)
        .collect();

    let is_nova3 = model.to_lowercase().starts_with("nova-3");
    for hw in &cleaned {
        if is_nova3 {
            params.push(format!("keyterm={}", urlencoding(hw)));
        } else {
            params.push(format!("keywords={}:{}", urlencoding(hw), KEYWORD_INTENSITY));
        }
    }

    format!("{}?{}", ENDPOINT, params.join("&"))
}

/// The close-stream message to signal end of audio.
pub fn close_stream_message() -> &'static str {
    r#"{"type":"CloseStream"}"#
}

// ---------------------------------------------------------------------------
// Response parsing
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct DeepgramResponse {
    pub transcript: String,
    pub confidence: f64,
    pub is_final: bool,
    pub speech_final: bool,
}

/// Parse a Deepgram JSON response message.
///
/// Returns `None` if the message type is not "Results" (e.g., metadata, error).
pub fn parse_response(text: &str) -> Result<Option<DeepgramResponse>> {
    let envelope: Envelope = serde_json::from_str(text)?;
    if envelope.r#type != "Results" {
        return Ok(None);
    }

    let msg: ResultsMessage = serde_json::from_str(text)?;
    let alt = msg
        .channel
        .as_ref()
        .and_then(|ch| ch.alternatives.first());

    let transcript = alt
        .map(|a| a.transcript.trim().to_string())
        .unwrap_or_default();
    let confidence = alt.map(|a| a.confidence).unwrap_or(0.0);

    Ok(Some(DeepgramResponse {
        transcript,
        confidence,
        is_final: msg.is_final || msg.speech_final || msg.from_finalize,
        speech_final: msg.speech_final,
    }))
}

// ---------------------------------------------------------------------------
// Segment normalization (matches macOS DeepgramProtocol.normalize)
// ---------------------------------------------------------------------------

/// Insert a space between segments when needed (English words), but not for CJK.
pub fn normalize_segment(segment: &str, existing_text: &str) -> String {
    if segment.is_empty() {
        return String::new();
    }
    let last = match existing_text.chars().last() {
        Some(c) => c,
        None => return segment.to_string(),
    };
    let first = match segment.chars().next() {
        Some(c) => c,
        None => return segment.to_string(),
    };

    if last.is_whitespace() || first.is_whitespace() {
        return segment.to_string();
    }
    if is_closing_punct(first) || is_opening_punct(last) {
        return segment.to_string();
    }
    if is_cjk(last) || is_cjk(first) {
        return segment.to_string();
    }

    format!(" {}", segment)
}

fn is_cjk(c: char) -> bool {
    matches!(c as u32,
        0x4E00..=0x9FFF   // CJK Unified Ideographs
        | 0x3400..=0x4DBF // CJK Unified Ideographs Extension A
        | 0xF900..=0xFAFF // CJK Compatibility Ideographs
        | 0x3000..=0x303F // CJK Symbols and Punctuation
    )
}

fn is_closing_punct(c: char) -> bool {
    matches!(c, ')' | ']' | '}' | '>' | '）' | '】' | '》' | '」' | '"'
        | '.' | ',' | '!' | '?' | ';' | ':' | '。' | '，' | '！' | '？' | '；' | '：')
}

fn is_opening_punct(c: char) -> bool {
    matches!(c, '(' | '[' | '{' | '<' | '（' | '【' | '《' | '「' | '"')
}

fn urlencoding(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => result.push(c),
            _ => {
                for byte in c.to_string().as_bytes() {
                    result.push_str(&format!("%{:02X}", byte));
                }
            }
        }
    }
    result
}

// ---------------------------------------------------------------------------
// Internal Serde types
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct Envelope {
    r#type: String,
}

#[derive(Deserialize)]
struct ResultsMessage {
    #[allow(dead_code)]
    r#type: String,
    channel: Option<Channel>,
    #[serde(default)]
    is_final: bool,
    #[serde(default)]
    speech_final: bool,
    #[serde(default)]
    from_finalize: bool,
}

#[derive(Deserialize)]
struct Channel {
    alternatives: Vec<Alternative>,
}

#[derive(Deserialize)]
struct Alternative {
    transcript: String,
    #[serde(default)]
    confidence: f64,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_ws_url() {
        let url = build_ws_url("key", "nova-3", "zh", true, &[]);
        assert!(url.starts_with(ENDPOINT));
        assert!(url.contains("model=nova-3"));
        assert!(url.contains("language=zh"));
        assert!(url.contains("punctuate=true"));
        assert!(url.contains("smart_format=true"));
    }

    #[test]
    fn test_build_ws_url_nova3_keyterms() {
        let hotwords = vec!["Type4Me".to_string()];
        let url = build_ws_url("key", "nova-3", "zh", true, &hotwords);
        assert!(url.contains("keyterm=Type4Me"));
        assert!(!url.contains("keywords="));
    }

    #[test]
    fn test_build_ws_url_nova2_keywords() {
        let hotwords = vec!["Type4Me".to_string()];
        let url = build_ws_url("key", "nova-2", "zh", true, &hotwords);
        assert!(url.contains(&format!("keywords=Type4Me:{}", KEYWORD_INTENSITY)));
    }

    #[test]
    fn test_parse_results() {
        let json = r#"{
            "type": "Results",
            "channel": {
                "alternatives": [
                    {"transcript": "hello world", "confidence": 0.98}
                ]
            },
            "is_final": true,
            "speech_final": false,
            "from_finalize": false
        }"#;
        let resp = parse_response(json).unwrap().unwrap();
        assert_eq!(resp.transcript, "hello world");
        assert!((resp.confidence - 0.98).abs() < 0.001);
        assert!(resp.is_final);
        assert!(!resp.speech_final);
    }

    #[test]
    fn test_parse_metadata_ignored() {
        let json = r#"{"type": "Metadata", "request_id": "abc"}"#;
        assert!(parse_response(json).unwrap().is_none());
    }

    #[test]
    fn test_normalize_cjk() {
        assert_eq!(normalize_segment("世界", "你好"), "世界");
    }

    #[test]
    fn test_normalize_english() {
        assert_eq!(normalize_segment("world", "hello"), " world");
    }

    #[test]
    fn test_normalize_after_space() {
        assert_eq!(normalize_segment("world", "hello "), "world");
    }
}
