//! Volcengine binary protocol encoder/decoder.
//!
//! Byte-compatible with the macOS Swift implementation (VolcProtocol.swift / VolcHeader.swift).
//!
//! 4-byte header layout:
//! - Byte 0: `(version << 4) | header_size`  (version=1, header_size=1 meaning 4 bytes)
//! - Byte 1: `(message_type << 4) | flags`
//! - Byte 2: `(serialization << 4) | compression`
//! - Byte 3: reserved (0x00)

use anyhow::{bail, Context, Result};
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use std::io::{Read, Write};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

pub const VERSION: u8 = 1;
pub const HEADER_SIZE_UNITS: u8 = 1; // 1 unit = 4 bytes

// Message types
pub const MSG_FULL_CLIENT_REQUEST: u8 = 0x01;
pub const MSG_AUDIO_ONLY: u8 = 0x02;
pub const MSG_SERVER_RESPONSE: u8 = 0x09;
pub const MSG_SERVER_ERROR: u8 = 0x0F;

// Flags
pub const FLAG_NO_SEQUENCE: u8 = 0x00;
pub const FLAG_HAS_SEQUENCE: u8 = 0x01;
pub const FLAG_LAST_NO_SEQUENCE: u8 = 0x02;
pub const FLAG_LAST_HAS_SEQUENCE: u8 = 0x03;
pub const FLAG_ASYNC_FINAL: u8 = 0x04;

// Serialization
pub const SER_NONE: u8 = 0x00;
pub const SER_JSON: u8 = 0x01;

// Compression
pub const COMP_NONE: u8 = 0x00;
pub const COMP_GZIP: u8 = 0x01;

// ---------------------------------------------------------------------------
// Server Message
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct VolcServerMessage {
    pub message_type: u8,
    pub flags: u8,
    pub sequence: Option<i32>,
    pub payload: serde_json::Value,
}

// ---------------------------------------------------------------------------
// Encode: full_client_request
// ---------------------------------------------------------------------------

/// Encode a full_client_request message.
///
/// Header: version=1, headerSize=1, msgType=0x01, flags=0x00, ser=JSON, comp=gzip
/// Body: 4-byte big-endian payload size + gzip-compressed JSON
pub fn encode_full_client_request(request_json: &serde_json::Value) -> Vec<u8> {
    let json_bytes = serde_json::to_vec(request_json).expect("JSON serialization cannot fail");

    // Gzip compress the JSON payload
    let compressed = gzip_compress(&json_bytes).unwrap_or_else(|_| json_bytes.clone());

    let mut message = Vec::with_capacity(4 + 4 + compressed.len());

    // Header: version=1, headerSize=1, fullClientRequest, noSequence, JSON, gzip
    message.push((VERSION << 4) | HEADER_SIZE_UNITS); // 0x11
    message.push((MSG_FULL_CLIENT_REQUEST << 4) | FLAG_NO_SEQUENCE); // 0x10
    message.push((SER_JSON << 4) | COMP_GZIP); // 0x11
    message.push(0x00); // reserved

    // 4-byte big-endian payload size
    let size = compressed.len() as u32;
    message.extend_from_slice(&size.to_be_bytes());

    // Compressed payload
    message.extend_from_slice(&compressed);

    message
}

// ---------------------------------------------------------------------------
// Encode: audio packet
// ---------------------------------------------------------------------------

/// Encode an audio-only packet.
///
/// Header: version=1, headerSize=1, msgType=0x02, flags depend on is_last,
/// serialization=none, compression=none.
/// Body: 4-byte big-endian size + raw audio data.
pub fn encode_audio_packet(audio_data: &[u8], is_last: bool) -> Vec<u8> {
    let flags = if is_last {
        FLAG_LAST_NO_SEQUENCE
    } else {
        FLAG_NO_SEQUENCE
    };

    let mut message = Vec::with_capacity(4 + 4 + audio_data.len());

    message.push((VERSION << 4) | HEADER_SIZE_UNITS); // 0x11
    message.push((MSG_AUDIO_ONLY << 4) | flags); // 0x20 or 0x22
    message.push((SER_NONE << 4) | COMP_NONE); // 0x00
    message.push(0x00); // reserved

    let size = audio_data.len() as u32;
    message.extend_from_slice(&size.to_be_bytes());
    message.extend_from_slice(audio_data);

    message
}

// ---------------------------------------------------------------------------
// Decode: server message
// ---------------------------------------------------------------------------

/// Decode a server response or error from the binary wire format.
pub fn decode_server_message(data: &[u8]) -> Result<VolcServerMessage> {
    if data.len() < 4 {
        bail!("message too short: {} bytes", data.len());
    }

    let byte0 = data[0];
    let byte1 = data[1];
    let byte2 = data[2];

    let _version = (byte0 >> 4) & 0x0F;
    let header_size = (byte0 & 0x0F) as usize; // in 4-byte units
    let header_bytes = header_size * 4;

    let message_type = (byte1 >> 4) & 0x0F;
    let flags = byte1 & 0x0F;

    let serialization = (byte2 >> 4) & 0x0F;
    let compression = byte2 & 0x0F;

    let mut offset = header_bytes;

    // Read sequence number if flags indicate presence
    let sequence = if flags == FLAG_HAS_SEQUENCE || flags == FLAG_LAST_HAS_SEQUENCE {
        if data.len() < offset + 4 {
            bail!("message too short for sequence number");
        }
        let seq = i32::from_be_bytes([
            data[offset],
            data[offset + 1],
            data[offset + 2],
            data[offset + 3],
        ]);
        offset += 4;
        Some(seq)
    } else {
        None
    };

    // Read payload size
    if data.len() < offset + 4 {
        bail!("message too short for payload size");
    }
    let payload_size = u32::from_be_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    ]) as usize;
    offset += 4;

    if data.len() < offset + payload_size {
        bail!(
            "message truncated: expected {} payload bytes, got {}",
            payload_size,
            data.len() - offset
        );
    }

    let mut payload_bytes = data[offset..offset + payload_size].to_vec();

    // Decompress if gzip
    if compression == COMP_GZIP && !payload_bytes.is_empty() {
        payload_bytes = gzip_decompress(&payload_bytes).context("gzip decompression failed")?;
    }

    // Handle server error
    if message_type == MSG_SERVER_ERROR {
        if serialization == SER_JSON && !payload_bytes.is_empty() {
            if let Ok(json) = serde_json::from_slice::<serde_json::Value>(&payload_bytes) {
                let code = json.get("code").and_then(|v| v.as_i64());
                let msg = json
                    .get("message")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown error");
                bail!(
                    "Volcengine server error (code={}): {}",
                    code.unwrap_or(-1),
                    msg
                );
            }
        }
        bail!("Volcengine server error (no details)");
    }

    // Parse JSON payload
    let payload = if serialization == SER_JSON && !payload_bytes.is_empty() {
        serde_json::from_slice(&payload_bytes).context("failed to parse JSON payload")?
    } else {
        serde_json::Value::Null
    };

    Ok(VolcServerMessage {
        message_type,
        flags,
        sequence,
        payload,
    })
}

// ---------------------------------------------------------------------------
// Build start request JSON
// ---------------------------------------------------------------------------

/// Build the full_client_request JSON body matching the macOS VolcProtocol.buildClientRequest.
pub fn build_start_request(
    uid: &str,
    _request_id: &str,
    language: &str,
    hotwords: &[String],
    enable_punc: bool,
    boosting_table_id: Option<&str>,
) -> serde_json::Value {
    let mut request_dict = serde_json::json!({
        "model_name": "bigmodel",
        "enable_punc": enable_punc,
        "enable_ddc": true,
        "enable_nonstream": true,
        "show_utterances": true,
        "result_type": "full",
        "end_window_size": 1500,
        "force_to_speech_time": 1000,
    });

    // Hotwords / boosting table
    let boosting_id = boosting_table_id
        .map(|s| s.trim())
        .filter(|s| !s.is_empty());

    if let Some(table_id) = boosting_id {
        // Cloud boosting table: use table ID, skip inline hotwords
        request_dict["corpus"] = serde_json::json!({
            "boosting_table_id": table_id,
        });
    } else {
        // Inline hotwords
        let cleaned: Vec<&str> = hotwords
            .iter()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .collect();
        if !cleaned.is_empty() {
            let hw_list: Vec<serde_json::Value> = cleaned
                .iter()
                .map(|w| {
                    serde_json::json!({
                        "word": *w,
                        "scale": 5.0,
                    })
                })
                .collect();
            let context_obj = serde_json::json!({ "hotwords": hw_list });
            request_dict["context"] = serde_json::Value::String(context_obj.to_string());
        }
    }

    // Language hint: not part of the original macOS protocol but useful
    let _ = language;

    serde_json::json!({
        "user": { "uid": uid },
        "audio": {
            "format": "pcm",
            "codec": "raw",
            "rate": 16000,
            "bits": 16,
            "channel": 1,
        },
        "request": request_dict,
    })
}

// ---------------------------------------------------------------------------
// Result parsing helpers
// ---------------------------------------------------------------------------

/// A single utterance in the Volcengine response.
#[derive(Debug, Clone)]
pub struct VolcUtterance {
    pub text: String,
    pub definite: bool,
}

/// Parsed ASR result from a Volcengine server response.
#[derive(Debug, Clone)]
pub struct VolcASRResult {
    pub text: String,
    pub utterances: Vec<VolcUtterance>,
}

/// Parse the result object from a decoded server message payload.
pub fn parse_asr_result(payload: &serde_json::Value) -> VolcASRResult {
    let result_obj = payload.get("result").unwrap_or(payload);

    let text = result_obj
        .get("text")
        .and_then(|v| v.as_str())
        .or_else(|| payload.get("text").and_then(|v| v.as_str()))
        .unwrap_or("")
        .to_string();

    let utterances_source = result_obj
        .get("utterances")
        .or_else(|| payload.get("utterances"));

    let utterances = if let Some(serde_json::Value::Array(utts)) = utterances_source {
        utts.iter()
            .map(|u| VolcUtterance {
                text: u
                    .get("text")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                definite: u
                    .get("definite")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false),
            })
            .collect()
    } else {
        Vec::new()
    };

    VolcASRResult { text, utterances }
}

// ---------------------------------------------------------------------------
// Gzip helpers
// ---------------------------------------------------------------------------

fn gzip_compress(data: &[u8]) -> Result<Vec<u8>> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(data)?;
    let compressed = encoder.finish()?;
    Ok(compressed)
}

fn gzip_decompress(data: &[u8]) -> Result<Vec<u8>> {
    let mut decoder = GzDecoder::new(data);
    let mut decompressed = Vec::new();
    decoder.read_to_end(&mut decompressed)?;
    Ok(decompressed)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_audio_packet_header() {
        let packet = encode_audio_packet(&[0x01, 0x02, 0x03], false);
        assert_eq!(packet[0], 0x11); // version=1, headerSize=1
        assert_eq!(packet[1], 0x20); // audioOnly=0x02, noSequence=0x00
        assert_eq!(packet[2], 0x00); // no serialization, no compression
        assert_eq!(packet[3], 0x00); // reserved

        // Payload size = 3
        assert_eq!(&packet[4..8], &[0, 0, 0, 3]);
        assert_eq!(&packet[8..], &[0x01, 0x02, 0x03]);
    }

    #[test]
    fn test_encode_last_audio_packet() {
        let packet = encode_audio_packet(&[], true);
        assert_eq!(packet[1], 0x22); // audioOnly=0x02, lastNoSequence=0x02
        assert_eq!(&packet[4..8], &[0, 0, 0, 0]); // zero-length payload
        assert_eq!(packet.len(), 8);
    }

    #[test]
    fn test_full_client_request_header() {
        let json = serde_json::json!({"test": true});
        let msg = encode_full_client_request(&json);
        assert_eq!(msg[0], 0x11); // version=1, headerSize=1
        assert_eq!(msg[1], 0x10); // fullClientRequest=0x01, noSequence=0x00
        assert_eq!(msg[2], 0x11); // JSON=0x01, gzip=0x01
        assert_eq!(msg[3], 0x00); // reserved
    }

    #[test]
    fn test_build_start_request_structure() {
        let req = build_start_request("test_uid", "req_123", "zh", &[], true, None);
        assert_eq!(req["user"]["uid"], "test_uid");
        assert_eq!(req["audio"]["rate"], 16000);
        assert_eq!(req["audio"]["bits"], 16);
        assert_eq!(req["request"]["model_name"], "bigmodel");
        assert_eq!(req["request"]["enable_punc"], true);
    }

    #[test]
    fn test_gzip_roundtrip() {
        let original = b"hello world, this is a test of gzip compression";
        let compressed = gzip_compress(original).unwrap();
        let decompressed = gzip_decompress(&compressed).unwrap();
        assert_eq!(decompressed, original);
    }
}
