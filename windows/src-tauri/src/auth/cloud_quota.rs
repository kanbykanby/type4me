use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::time::Instant;
use tracing::{debug, info, warn};

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct QuotaInfo {
    pub plan: String,
    pub is_paid: bool,
    pub free_chars_remaining: i32,
    pub week_chars: i32,
    pub total_chars: i32,
}

// ---------------------------------------------------------------------------
// CloudQuotaManager
// ---------------------------------------------------------------------------

/// Cache duration: don't re-fetch quota within 30 seconds unless forced.
const CACHE_TTL_SECS: u64 = 30;

pub struct CloudQuotaManager {
    plan: String,
    is_paid: bool,
    free_chars_remaining: i32,
    week_chars: i32,
    total_chars: i32,
    last_refresh: Option<Instant>,
    http: reqwest::Client,
}

impl CloudQuotaManager {
    pub fn new() -> Self {
        Self {
            plan: "free".to_string(),
            is_paid: false,
            free_chars_remaining: 0,
            week_chars: 0,
            total_chars: 0,
            last_refresh: None,
            http: reqwest::Client::new(),
        }
    }

    /// Fetch latest quota and usage from the server.
    ///
    /// Skips the network call if data was refreshed within the last 30 seconds,
    /// unless `force` is true.
    pub async fn refresh(&mut self, endpoint: &str, token: &str, force: bool) -> Result<()> {
        if !force {
            if let Some(last) = self.last_refresh {
                if last.elapsed().as_secs() < CACHE_TTL_SECS {
                    debug!("quota cache still fresh, skipping refresh");
                    return Ok(());
                }
            }
        }

        let auth_header = format!("Bearer {token}");

        // Fetch quota info
        #[derive(Deserialize)]
        struct QuotaResponse {
            remaining_chars: Option<i32>,
            is_paid: Option<bool>,
            plan: Option<String>,
        }

        let quota_resp = self
            .http
            .get(format!("{endpoint}/api/quota"))
            .header("Authorization", &auth_header)
            .send()
            .await
            .context("quota request failed")?;

        if !quota_resp.status().is_success() {
            let status = quota_resp.status();
            let body = quota_resp.text().await.unwrap_or_default();
            bail!("quota API error ({status}): {body}");
        }

        let quota: QuotaResponse = quota_resp
            .json()
            .await
            .context("bad quota response JSON")?;

        self.free_chars_remaining = quota.remaining_chars.unwrap_or(0);
        self.is_paid = quota.is_paid.unwrap_or(false);
        self.plan = quota.plan.unwrap_or_else(|| "free".to_string());

        // Fetch usage stats
        #[derive(Deserialize)]
        struct UsageResponse {
            total_chars: Option<i32>,
            week_chars: Option<i32>,
        }

        let usage_resp = self
            .http
            .get(format!("{endpoint}/api/usage"))
            .header("Authorization", &auth_header)
            .send()
            .await
            .context("usage request failed")?;

        if usage_resp.status().is_success() {
            if let Ok(usage) = usage_resp.json::<UsageResponse>().await {
                self.total_chars = usage.total_chars.unwrap_or(0);
                self.week_chars = usage.week_chars.unwrap_or(0);
            }
        } else {
            warn!(
                status = %usage_resp.status(),
                "usage API returned error, keeping stale data"
            );
        }

        self.last_refresh = Some(Instant::now());
        info!(
            plan = %self.plan,
            remaining = self.free_chars_remaining,
            "quota refreshed"
        );

        Ok(())
    }

    /// Whether the user can currently use the service.
    pub fn can_use(&self) -> bool {
        self.is_paid || self.free_chars_remaining > 0
    }

    /// Optimistic local deduction so the UI reflects usage immediately.
    /// The server-side deduction happens via `report_usage`.
    pub fn deduct_local(&mut self, chars: i32) {
        if !self.is_paid {
            self.free_chars_remaining = (self.free_chars_remaining - chars).max(0);
        }
        self.week_chars += chars;
        self.total_chars += chars;
        debug!(chars, remaining = self.free_chars_remaining, "local deduction");
    }

    /// Report actual usage to the server for billing/tracking.
    pub async fn report_usage(
        &self,
        endpoint: &str,
        token: &str,
        chars: i32,
        mode: &str,
    ) -> Result<()> {
        let resp = self
            .http
            .post(format!("{endpoint}/api/report-usage"))
            .header("Authorization", format!("Bearer {token}"))
            .json(&serde_json::json!({
                "char_count": chars,
                "mode": mode,
            }))
            .send()
            .await
            .context("report-usage request failed")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            warn!("report-usage error ({status}): {body}");
        } else {
            debug!(chars, mode, "usage reported");
        }

        Ok(())
    }

    /// Build a `QuotaInfo` snapshot for the frontend.
    pub fn quota_info(&self) -> QuotaInfo {
        QuotaInfo {
            plan: self.plan.clone(),
            is_paid: self.is_paid,
            free_chars_remaining: self.free_chars_remaining,
            week_chars: self.week_chars,
            total_chars: self.total_chars,
        }
    }
}

// ---------------------------------------------------------------------------
// Text unit counting (must match Go backend exactly)
// ---------------------------------------------------------------------------

/// Returns true if `ch` falls in a CJK or CJK-adjacent Unicode range.
///
/// Ranges match the Go backend:
///  - CJK Unified Ideographs:           U+4E00..U+9FFF
///  - CJK Extension A:                  U+3400..U+4DBF
///  - CJK Compatibility Ideographs:     U+F900..U+FAFF
///  - CJK Symbols and Punctuation:      U+3000..U+303F
///  - Fullwidth Forms:                  U+FF00..U+FFEF
///  - Hiragana:                         U+3040..U+309F
///  - Katakana:                         U+30A0..U+30FF
fn is_cjk(ch: char) -> bool {
    let c = ch as u32;
    matches!(
        c,
        0x4E00..=0x9FFF
        | 0x3400..=0x4DBF
        | 0xF900..=0xFAFF
        | 0x3000..=0x303F
        | 0xFF00..=0xFFEF
        | 0x3040..=0x309F
        | 0x30A0..=0x30FF
    )
}

/// Count text units matching the Go backend's logic:
///
/// - Each CJK character counts as 1 unit.
/// - English (non-CJK, non-whitespace) characters are grouped into words
///   separated by whitespace, each word counting as 1 unit.
pub fn count_text_units(text: &str) -> i32 {
    let mut count: i32 = 0;
    let mut in_word = false;

    for ch in text.chars() {
        if is_cjk(ch) {
            // Flush any pending English word
            if in_word {
                count += 1;
                in_word = false;
            }
            // CJK character: 1 unit
            count += 1;
        } else if ch.is_whitespace() {
            // Flush any pending English word
            if in_word {
                count += 1;
                in_word = false;
            }
        } else {
            // Non-CJK, non-whitespace: accumulate as part of a word
            in_word = true;
        }
    }

    // Flush last word
    if in_word {
        count += 1;
    }

    count
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_count_text_units_pure_chinese() {
        assert_eq!(count_text_units("你好世界"), 4);
    }

    #[test]
    fn test_count_text_units_pure_english() {
        assert_eq!(count_text_units("hello world"), 2);
    }

    #[test]
    fn test_count_text_units_mixed() {
        // "你好 world" => 你(1) + 好(1) + space(flush) + world(1) = 3
        assert_eq!(count_text_units("你好 world"), 3);
    }

    #[test]
    fn test_count_text_units_english_no_space_cjk() {
        // "hello你好" => hello gets flushed by CJK(你), 你(1), 好(1) = 1+1+1 = 3
        assert_eq!(count_text_units("hello你好"), 3);
    }

    #[test]
    fn test_count_text_units_empty() {
        assert_eq!(count_text_units(""), 0);
    }

    #[test]
    fn test_count_text_units_only_spaces() {
        assert_eq!(count_text_units("   "), 0);
    }

    #[test]
    fn test_count_text_units_japanese() {
        // Hiragana + Katakana
        assert_eq!(count_text_units("こんにちは"), 5);
    }

    #[test]
    fn test_count_text_units_fullwidth() {
        // Fullwidth letters (U+FF00-U+FFEF range)
        assert_eq!(count_text_units("Ａ"), 1);
    }
}
