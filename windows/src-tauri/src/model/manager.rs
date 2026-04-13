use anyhow::{bail, Context, Result};
use serde::Serialize;
use std::path::{Path, PathBuf};
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const MODEL_NAME: &str = "sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17";

const MODEL_DOWNLOAD_URL: &str = "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17.tar.bz2";

/// Expected size of the model directory once extracted (approx. 228 MB).
const MODEL_SIZE_MB: f64 = 228.0;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize)]
pub struct ModelStatus {
    pub downloaded: bool,
    pub size_mb: f64,
    pub path: Option<String>,
    pub model_name: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct DownloadProgress {
    pub percent: f64,
    pub bytes_downloaded: u64,
    pub total_bytes: u64,
}

// ---------------------------------------------------------------------------
// ModelManager
// ---------------------------------------------------------------------------

pub struct ModelManager {
    models_dir: PathBuf,
    http: reqwest::Client,
}

impl ModelManager {
    /// Create a new model manager. The models directory is created if needed.
    pub fn new() -> Result<Self> {
        let models_dir = Self::default_models_dir()?;
        std::fs::create_dir_all(&models_dir)
            .with_context(|| format!("failed to create models dir: {}", models_dir.display()))?;

        debug!(path = %models_dir.display(), "model manager initialized");

        Ok(Self {
            models_dir,
            http: reqwest::Client::new(),
        })
    }

    /// Default model storage directory.
    ///  - Windows: `%APPDATA%/Type4Me/models/`
    ///  - macOS / Linux: `~/.config/type4me/models/`
    fn default_models_dir() -> Result<PathBuf> {
        let base = if cfg!(windows) {
            dirs::data_dir()
                .context("cannot resolve %APPDATA%")?
                .join("Type4Me")
        } else {
            dirs::config_dir()
                .context("cannot resolve config dir")?
                .join("type4me")
        };
        Ok(base.join("models"))
    }

    /// Path to the extracted model directory.
    fn model_dir(&self) -> PathBuf {
        self.models_dir.join(MODEL_NAME)
    }

    /// Check whether the model is downloaded and ready.
    pub fn status(&self) -> ModelStatus {
        let dir = self.model_dir();
        let downloaded = dir.exists() && dir.is_dir();

        // Quick sanity: check for the .onnx file inside
        let has_model_file = if downloaded {
            Self::find_onnx_file(&dir).is_some()
        } else {
            false
        };

        ModelStatus {
            downloaded: has_model_file,
            size_mb: if has_model_file { MODEL_SIZE_MB } else { 0.0 },
            path: if has_model_file {
                Some(dir.to_string_lossy().to_string())
            } else {
                None
            },
            model_name: MODEL_NAME.to_string(),
        }
    }

    /// Download and extract the SenseVoice model.
    ///
    /// Progress events are sent through `progress_tx`. The function blocks
    /// (async) until the download and extraction are complete.
    pub async fn download(
        &self,
        progress_tx: mpsc::UnboundedSender<DownloadProgress>,
    ) -> Result<()> {
        let dest = self.model_dir();
        if dest.exists() {
            info!("model directory already exists, removing for fresh download");
            std::fs::remove_dir_all(&dest).context("failed to remove existing model dir")?;
        }

        info!(url = MODEL_DOWNLOAD_URL, "starting model download");

        let resp = self
            .http
            .get(MODEL_DOWNLOAD_URL)
            .send()
            .await
            .context("model download request failed")?;

        if !resp.status().is_success() {
            bail!("download failed: HTTP {}", resp.status());
        }

        let total_bytes = resp.content_length().unwrap_or(0);
        let mut bytes_downloaded: u64 = 0;

        // Stream the response body into a temporary file
        let tmp_path = self.models_dir.join(format!("{MODEL_NAME}.tar.bz2.tmp"));

        {
            use tokio::io::AsyncWriteExt;
            let mut file = tokio::fs::File::create(&tmp_path)
                .await
                .context("failed to create temp file")?;

            let mut stream = resp.bytes_stream();
            use futures_util::StreamExt;

            while let Some(chunk) = stream.next().await {
                let chunk = chunk.context("error reading download stream")?;
                file.write_all(&chunk).await.context("error writing to temp file")?;

                bytes_downloaded += chunk.len() as u64;
                let percent = if total_bytes > 0 {
                    (bytes_downloaded as f64 / total_bytes as f64) * 100.0
                } else {
                    0.0
                };

                let _ = progress_tx.send(DownloadProgress {
                    percent,
                    bytes_downloaded,
                    total_bytes,
                });
            }

            file.flush().await?;
        }

        info!(
            bytes = bytes_downloaded,
            "download complete, extracting archive"
        );

        // Extract tar.bz2
        let models_dir = self.models_dir.clone();
        let tmp_for_extract = tmp_path.clone();
        tokio::task::spawn_blocking(move || extract_tar_bz2(&tmp_for_extract, &models_dir))
            .await
            .context("extraction task panicked")??;

        // Clean up temp file
        if let Err(e) = tokio::fs::remove_file(&tmp_path).await {
            warn!("failed to remove temp file: {e}");
        }

        // Verify extraction
        let status = self.status();
        if !status.downloaded {
            bail!("extraction completed but model files not found");
        }

        info!(path = %dest.display(), "model ready");
        Ok(())
    }

    /// Delete the downloaded model.
    pub fn delete(&self) -> Result<()> {
        let dir = self.model_dir();
        if dir.exists() {
            std::fs::remove_dir_all(&dir)
                .with_context(|| format!("failed to delete model: {}", dir.display()))?;
            info!("model deleted");
        } else {
            debug!("model directory does not exist, nothing to delete");
        }
        Ok(())
    }

    /// Returns the path to the model directory if downloaded.
    pub fn model_path(&self) -> Option<PathBuf> {
        let dir = self.model_dir();
        if dir.exists() && Self::find_onnx_file(&dir).is_some() {
            Some(dir)
        } else {
            None
        }
    }

    /// Look for a `.onnx` model file inside the directory.
    fn find_onnx_file(dir: &Path) -> Option<PathBuf> {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if let Some(ext) = path.extension() {
                    if ext == "onnx" {
                        return Some(path);
                    }
                }
            }
        }
        None
    }
}

// ---------------------------------------------------------------------------
// tar.bz2 extraction
// ---------------------------------------------------------------------------

fn extract_tar_bz2(archive_path: &Path, dest_dir: &Path) -> Result<()> {
    use std::fs::File;
    use std::io::BufReader;

    let file = File::open(archive_path)
        .with_context(|| format!("cannot open archive: {}", archive_path.display()))?;
    let reader = BufReader::new(file);

    // bz2 decompression
    let decompressor = bzip2::read::BzDecoder::new(reader);

    // tar extraction
    let mut archive = tar::Archive::new(decompressor);
    archive
        .unpack(dest_dir)
        .context("failed to extract tar.bz2 archive")?;

    info!(dest = %dest_dir.display(), "archive extracted");
    Ok(())
}
