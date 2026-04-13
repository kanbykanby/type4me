use anyhow::{Context, Result};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use tracing::{debug, warn};

/// Credential storage backed by DPAPI (Windows) or plain JSON (non-Windows dev).
///
/// Secure values: each key → separate file under `secure/`, DPAPI-encrypted then
/// base64-encoded on Windows, plain text on other platforms (dev only).
///
/// Plain values: single `credentials.json` keyed by name.
pub struct CredentialStorage {
    app_data_dir: PathBuf,
}

impl CredentialStorage {
    /// Auto-detect the platform data directory.
    ///  - Windows: `%APPDATA%/Type4Me/`
    ///  - macOS / Linux: `~/.config/type4me/`
    pub fn new() -> Result<Self> {
        let base = if cfg!(windows) {
            dirs::data_dir()
                .context("cannot resolve %APPDATA%")?
                .join("Type4Me")
        } else {
            dirs::config_dir()
                .context("cannot resolve config dir")?
                .join("type4me")
        };

        fs::create_dir_all(&base)
            .with_context(|| format!("failed to create data dir: {}", base.display()))?;

        debug!(path = %base.display(), "credential storage initialized");
        Ok(Self { app_data_dir: base })
    }

    // ----- secure storage -----

    fn secure_dir(&self) -> PathBuf {
        self.app_data_dir.join("secure")
    }

    fn secure_path(&self, key: &str) -> PathBuf {
        // sanitize key for filesystem
        let safe: String = key
            .chars()
            .map(|c| if c.is_alphanumeric() || c == '_' || c == '-' { c } else { '_' })
            .collect();
        self.secure_dir().join(format!("{safe}.enc"))
    }

    /// Save a value to secure (DPAPI on Windows) storage.
    pub fn save_secure(&self, key: &str, value: &str) -> Result<()> {
        let dir = self.secure_dir();
        fs::create_dir_all(&dir)?;

        let encoded = self.protect(value.as_bytes())?;
        fs::write(self.secure_path(key), &encoded)?;
        debug!(key, "secure credential saved");
        Ok(())
    }

    /// Load a value from secure storage. Returns `None` if the key doesn't exist.
    pub fn load_secure(&self, key: &str) -> Result<Option<String>> {
        let path = self.secure_path(key);
        if !path.exists() {
            return Ok(None);
        }
        let encoded = fs::read_to_string(&path)?;
        let bytes = self.unprotect(&encoded)?;
        let value = String::from_utf8(bytes).context("secure value is not valid UTF-8")?;
        Ok(Some(value))
    }

    /// Delete a key from secure storage.
    pub fn delete_secure(&self, key: &str) -> Result<()> {
        let path = self.secure_path(key);
        if path.exists() {
            fs::remove_file(&path)?;
            debug!(key, "secure credential deleted");
        }
        Ok(())
    }

    // ----- plain (JSON) storage -----

    fn plain_path(&self) -> PathBuf {
        self.app_data_dir.join("credentials.json")
    }

    fn read_plain_map(&self) -> Result<HashMap<String, serde_json::Value>> {
        let path = self.plain_path();
        if !path.exists() {
            return Ok(HashMap::new());
        }
        let content = fs::read_to_string(&path)?;
        let map: HashMap<String, serde_json::Value> =
            serde_json::from_str(&content).unwrap_or_default();
        Ok(map)
    }

    fn write_plain_map(&self, map: &HashMap<String, serde_json::Value>) -> Result<()> {
        let json = serde_json::to_string_pretty(map)?;
        let path = self.plain_path();
        fs::write(&path, json)?;

        // Restrict file permissions on Unix (0600)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
        }

        Ok(())
    }

    /// Save a non-secret value to the plain JSON store.
    pub fn save_plain(&self, key: &str, value: &serde_json::Value) -> Result<()> {
        let mut map = self.read_plain_map()?;
        map.insert(key.to_string(), value.clone());
        self.write_plain_map(&map)?;
        debug!(key, "plain credential saved");
        Ok(())
    }

    /// Load a non-secret value from the plain JSON store.
    pub fn load_plain(&self, key: &str) -> Result<Option<serde_json::Value>> {
        let map = self.read_plain_map()?;
        Ok(map.get(key).cloned())
    }

    // -----------------------------------------------------------------------
    // Platform-specific protect / unprotect
    // -----------------------------------------------------------------------

    /// Encrypt bytes → base64 string.
    #[cfg(windows)]
    fn protect(&self, plaintext: &[u8]) -> Result<String> {
        use base64::Engine as _;
        use windows::Win32::Security::Cryptography::{
            CryptProtectData, CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN,
        };

        let mut input_blob = CRYPT_INTEGER_BLOB {
            cbData: plaintext.len() as u32,
            pbData: plaintext.as_ptr() as *mut u8,
        };
        let mut output_blob = CRYPT_INTEGER_BLOB {
            cbData: 0,
            pbData: std::ptr::null_mut(),
        };

        unsafe {
            CryptProtectData(
                &mut input_blob,
                None,                       // description
                None,                       // optional entropy
                None,                       // reserved
                None,                       // prompt struct
                CRYPTPROTECT_UI_FORBIDDEN,  // flags
                &mut output_blob,
            )
            .ok()
            .context("CryptProtectData failed")?;

            let encrypted =
                std::slice::from_raw_parts(output_blob.pbData, output_blob.cbData as usize)
                    .to_vec();

            // Free the buffer allocated by DPAPI
            windows::Win32::System::Memory::LocalFree(
                windows::Win32::Foundation::HLOCAL(output_blob.pbData as _),
            );

            Ok(base64::engine::general_purpose::STANDARD.encode(&encrypted))
        }
    }

    /// Decrypt base64 string → bytes.
    #[cfg(windows)]
    fn unprotect(&self, encoded: &str) -> Result<Vec<u8>> {
        use base64::Engine as _;
        use windows::Win32::Security::Cryptography::{
            CryptUnprotectData, CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN,
        };

        let encrypted = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .context("invalid base64 in secure file")?;

        let mut input_blob = CRYPT_INTEGER_BLOB {
            cbData: encrypted.len() as u32,
            pbData: encrypted.as_ptr() as *mut u8,
        };
        let mut output_blob = CRYPT_INTEGER_BLOB {
            cbData: 0,
            pbData: std::ptr::null_mut(),
        };

        unsafe {
            CryptUnprotectData(
                &mut input_blob,
                None,                       // description out
                None,                       // entropy
                None,                       // reserved
                None,                       // prompt struct
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output_blob,
            )
            .ok()
            .context("CryptUnprotectData failed")?;

            let decrypted =
                std::slice::from_raw_parts(output_blob.pbData, output_blob.cbData as usize)
                    .to_vec();

            windows::Win32::System::Memory::LocalFree(
                windows::Win32::Foundation::HLOCAL(output_blob.pbData as _),
            );

            Ok(decrypted)
        }
    }

    /// Non-Windows stub: store plain text (development only).
    #[cfg(not(windows))]
    fn protect(&self, plaintext: &[u8]) -> Result<String> {
        use base64::Engine as _;
        warn!("DPAPI unavailable – storing credential as base64 (dev mode)");
        Ok(base64::engine::general_purpose::STANDARD.encode(plaintext))
    }

    #[cfg(not(windows))]
    fn unprotect(&self, encoded: &str) -> Result<Vec<u8>> {
        use base64::Engine as _;
        base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .context("invalid base64 in secure file")
    }
}
