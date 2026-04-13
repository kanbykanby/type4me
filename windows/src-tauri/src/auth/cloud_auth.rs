use anyhow::{bail, Context, Result};
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

use super::cloud_config::{CloudConfig, CloudRegion};
use crate::credential::CredentialStorage;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AuthResult {
    pub token: String,
    pub email: String,
    pub user_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AuthStatus {
    pub is_logged_in: bool,
    pub email: Option<String>,
    pub user_id: Option<String>,
}

// ---------------------------------------------------------------------------
// JWT payload (just the fields we care about)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct JWTPayload {
    exp: Option<i64>,
    email: Option<String>,
    user_id: Option<String>,
    sub: Option<String>,
}

fn decode_jwt_payload(token: &str) -> Result<JWTPayload> {
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        bail!("malformed JWT: expected 3 segments, got {}", parts.len());
    }

    // The middle segment is the payload, base64url-encoded (no padding).
    let payload_b64 = parts[1];
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload_b64)
        .or_else(|_| {
            // Some JWTs use standard base64 with padding
            base64::engine::general_purpose::STANDARD.decode(payload_b64)
        })
        .context("failed to base64-decode JWT payload")?;

    serde_json::from_slice(&bytes).context("failed to parse JWT payload JSON")
}

fn is_token_expired(token: &str) -> bool {
    match decode_jwt_payload(token) {
        Ok(payload) => {
            if let Some(exp) = payload.exp {
                let now = chrono::Utc::now().timestamp();
                // Consider expired if within 60 seconds of expiry
                exp <= now + 60
            } else {
                // No exp claim: treat as non-expiring
                false
            }
        }
        Err(e) => {
            warn!("failed to decode JWT for expiry check: {e}");
            true
        }
    }
}

// ---------------------------------------------------------------------------
// Credential storage keys
// ---------------------------------------------------------------------------

const CRED_KEY_JWT: &str = "type4me_cloud_jwt";
const CRED_KEY_EMAIL: &str = "type4me_cloud_email";
const CRED_KEY_USER_ID: &str = "type4me_cloud_user_id";

// ---------------------------------------------------------------------------
// CloudAuthManager
// ---------------------------------------------------------------------------

pub struct CloudAuthManager {
    jwt_token: Option<String>,
    user_email: Option<String>,
    user_id: Option<String>,
    region: CloudRegion,
    http: reqwest::Client,
}

impl CloudAuthManager {
    pub fn new(region: CloudRegion) -> Self {
        Self {
            jwt_token: None,
            user_email: None,
            user_id: None,
            region,
            http: reqwest::Client::new(),
        }
    }

    /// Load a previously-saved JWT token from credential storage.
    pub fn load_saved_token(&mut self) -> Result<()> {
        let store = CredentialStorage::new()?;

        if let Some(token) = store.load_secure(CRED_KEY_JWT)? {
            if is_token_expired(&token) {
                info!("saved JWT is expired, discarding");
                self.clear_stored_credentials();
                return Ok(());
            }

            // Extract user info from the token payload
            if let Ok(payload) = decode_jwt_payload(&token) {
                self.user_email = payload.email;
                self.user_id = payload.user_id.or(payload.sub);
            }

            // Also try to load the email/user_id we saved alongside the token,
            // in case the JWT payload doesn't include them.
            if self.user_email.is_none() {
                if let Ok(Some(email)) = store.load_secure(CRED_KEY_EMAIL) {
                    self.user_email = Some(email);
                }
            }
            if self.user_id.is_none() {
                if let Ok(Some(uid)) = store.load_secure(CRED_KEY_USER_ID) {
                    self.user_id = Some(uid);
                }
            }

            self.jwt_token = Some(token);
            info!(email = ?self.user_email, "loaded saved JWT");
        }

        Ok(())
    }

    /// Send a verification code to the given email.
    pub async fn send_code(&self, email: &str) -> Result<()> {
        let endpoint = CloudConfig::api_endpoint(self.region);
        let url = format!("{endpoint}/auth/send-code");

        let resp = self
            .http
            .post(&url)
            .json(&serde_json::json!({ "email": email }))
            .send()
            .await
            .context("failed to send code request")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            bail!("send-code failed ({status}): {body}");
        }

        info!(email, "verification code sent");
        Ok(())
    }

    /// Verify the code and obtain a JWT token.
    pub async fn verify(&mut self, email: &str, code: &str) -> Result<AuthResult> {
        let endpoint = CloudConfig::api_endpoint(self.region);
        let url = format!("{endpoint}/auth/verify");

        let resp = self
            .http
            .post(&url)
            .json(&serde_json::json!({
                "email": email,
                "code": code,
            }))
            .send()
            .await
            .context("failed to send verify request")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            bail!("verify failed ({status}): {body}");
        }

        #[derive(Deserialize)]
        struct VerifyResponse {
            token: String,
            #[serde(default)]
            email: Option<String>,
            #[serde(default)]
            user_id: Option<String>,
        }

        let data: VerifyResponse = resp.json().await.context("bad verify response JSON")?;

        let result_email = data.email.unwrap_or_else(|| email.to_string());

        // Try to extract user_id from the token if not in the response
        let result_user_id = data.user_id.unwrap_or_else(|| {
            decode_jwt_payload(&data.token)
                .ok()
                .and_then(|p| p.user_id.or(p.sub))
                .unwrap_or_default()
        });

        // Save to memory
        self.jwt_token = Some(data.token.clone());
        self.user_email = Some(result_email.clone());
        self.user_id = Some(result_user_id.clone());

        // Persist to credential storage
        if let Err(e) = self.save_credentials() {
            warn!("failed to persist auth credentials: {e}");
        }

        let result = AuthResult {
            token: data.token,
            email: result_email,
            user_id: result_user_id,
        };

        info!(email = %result.email, "authentication successful");
        Ok(result)
    }

    /// Returns the access token if it exists and is not expired.
    pub fn access_token(&self) -> Option<&str> {
        self.jwt_token.as_deref().filter(|t| !is_token_expired(t))
    }

    pub fn is_logged_in(&self) -> bool {
        self.access_token().is_some()
    }

    pub fn user_email(&self) -> Option<&str> {
        if self.is_logged_in() {
            self.user_email.as_deref()
        } else {
            None
        }
    }

    pub fn user_id(&self) -> Option<&str> {
        if self.is_logged_in() {
            self.user_id.as_deref()
        } else {
            None
        }
    }

    pub fn status(&self) -> AuthStatus {
        AuthStatus {
            is_logged_in: self.is_logged_in(),
            email: self.user_email().map(|s| s.to_string()),
            user_id: self.user_id().map(|s| s.to_string()),
        }
    }

    /// Clear token and user info from memory and storage.
    pub fn sign_out(&mut self) {
        self.jwt_token = None;
        self.user_email = None;
        self.user_id = None;
        self.clear_stored_credentials();
        info!("signed out");
    }

    pub fn set_region(&mut self, region: CloudRegion) {
        self.region = region;
    }

    pub fn region(&self) -> CloudRegion {
        self.region
    }

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    fn save_credentials(&self) -> Result<()> {
        let store = CredentialStorage::new()?;
        if let Some(ref token) = self.jwt_token {
            store.save_secure(CRED_KEY_JWT, token)?;
        }
        if let Some(ref email) = self.user_email {
            store.save_secure(CRED_KEY_EMAIL, email)?;
        }
        if let Some(ref uid) = self.user_id {
            store.save_secure(CRED_KEY_USER_ID, uid)?;
        }
        debug!("auth credentials persisted");
        Ok(())
    }

    fn clear_stored_credentials(&self) {
        if let Ok(store) = CredentialStorage::new() {
            let _ = store.delete_secure(CRED_KEY_JWT);
            let _ = store.delete_secure(CRED_KEY_EMAIL);
            let _ = store.delete_secure(CRED_KEY_USER_ID);
        }
    }
}
