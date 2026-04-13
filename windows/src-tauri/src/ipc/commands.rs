use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};
use tracing::{debug, info, warn};

use crate::app_state::{AppState, ProcessingMode};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Resolve the `modes.json` path inside the app data directory.
fn modes_json_path(app: &AppHandle) -> std::path::PathBuf {
    let dir = app
        .path()
        .app_data_dir()
        .unwrap_or_else(|_| std::path::PathBuf::from("."));
    dir.join("modes.json")
}

// ---------------------------------------------------------------------------
// Mode management (Phase 1 - fully implemented)
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn get_modes(state: State<'_, AppState>) -> Result<Vec<ProcessingMode>, String> {
    let inner = state.lock().await;
    Ok(inner.available_modes.clone())
}

#[tauri::command]
pub async fn save_modes(
    app: AppHandle,
    state: State<'_, AppState>,
    modes: Vec<ProcessingMode>,
) -> Result<(), String> {
    // Persist to disk
    let path = modes_json_path(&app);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(&modes).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())?;
    info!(path = %path.display(), "modes persisted");

    // Update in-memory state
    let mut inner = state.lock().await;
    inner.available_modes = modes.clone();
    // If current mode was removed, fall back to the first available
    if !modes.iter().any(|m| m.id == inner.current_mode.id) {
        if let Some(first) = modes.first() {
            inner.current_mode = first.clone();
        }
    }

    Ok(())
}

#[tauri::command]
pub async fn get_current_mode(state: State<'_, AppState>) -> Result<ProcessingMode, String> {
    let inner = state.lock().await;
    Ok(inner.current_mode.clone())
}

#[tauri::command]
pub async fn set_current_mode(
    state: State<'_, AppState>,
    mode_id: String,
) -> Result<(), String> {
    let mut inner = state.lock().await;
    if let Some(mode) = inner.available_modes.iter().find(|m| m.id == mode_id) {
        inner.current_mode = mode.clone();
        debug!(mode_id, "mode switched");
        Ok(())
    } else {
        Err(format!("unknown mode: {mode_id}"))
    }
}

#[tauri::command]
pub fn open_settings_window(app: AppHandle) -> Result<(), String> {
    if let Some(win) = app.get_webview_window("settings") {
        win.show().map_err(|e| e.to_string())?;
        win.set_focus().map_err(|e| e.to_string())?;
        debug!("settings window shown");
    } else {
        warn!("settings window not found in config");
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Session commands (stubs for Phase 2+)
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn start_recording(
    _app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let mut inner = state.lock().await;
    if inner.is_recording {
        return Err("already recording".into());
    }
    inner.is_recording = true;
    inner.bar_phase = crate::app_state::FloatingBarPhase::Recording;
    debug!("recording started (stub)");
    // TODO(phase2): initialize audio capture, start ASR session
    Ok(())
}

#[tauri::command]
pub async fn stop_recording(
    _app: AppHandle,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let mut inner = state.lock().await;
    if !inner.is_recording {
        return Err("not recording".into());
    }
    inner.is_recording = false;
    inner.bar_phase = crate::app_state::FloatingBarPhase::Hidden;
    debug!("recording stopped (stub)");
    // TODO(phase2): finalize ASR, inject text
    Ok(String::new())
}

#[tauri::command]
pub async fn cancel_recording(
    _app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let mut inner = state.lock().await;
    inner.is_recording = false;
    inner.bar_phase = crate::app_state::FloatingBarPhase::Hidden;
    inner.segments.clear();
    debug!("recording cancelled");
    Ok(())
}

// ---------------------------------------------------------------------------
// Auth commands (stubs)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AuthStatus {
    pub is_authenticated: bool,
    pub phone: Option<String>,
    pub edition: String,
}

#[tauri::command]
pub async fn auth_send_code(_phone: String) -> Result<(), String> {
    debug!("auth_send_code stub");
    Err("auth not implemented yet".into())
}

#[tauri::command]
pub async fn auth_verify(_phone: String, _code: String) -> Result<AuthStatus, String> {
    debug!("auth_verify stub");
    Err("auth not implemented yet".into())
}

#[tauri::command]
pub async fn auth_sign_out() -> Result<(), String> {
    debug!("auth_sign_out stub");
    Ok(())
}

#[tauri::command]
pub async fn auth_status() -> Result<AuthStatus, String> {
    Ok(AuthStatus {
        is_authenticated: false,
        phone: None,
        edition: "free".into(),
    })
}

// ---------------------------------------------------------------------------
// Quota commands (stubs)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct QuotaInfo {
    pub remaining: i64,
    pub total: i64,
    pub reset_at: Option<String>,
}

#[tauri::command]
pub async fn quota_refresh() -> Result<QuotaInfo, String> {
    Ok(QuotaInfo {
        remaining: -1,
        total: -1,
        reset_at: None,
    })
}

#[tauri::command]
pub async fn quota_can_use() -> Result<bool, String> {
    // Free/BYOK mode: always available
    Ok(true)
}

// ---------------------------------------------------------------------------
// Settings: ASR provider
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn get_asr_provider() -> Result<String, String> {
    // Default to "volcano" – will be loaded from credential storage later
    Ok("volcano".into())
}

#[tauri::command]
pub async fn set_asr_provider(_provider: String) -> Result<(), String> {
    debug!(provider = %_provider, "set_asr_provider stub");
    Ok(())
}

#[tauri::command]
pub async fn get_asr_credentials(
    _provider: String,
) -> Result<serde_json::Value, String> {
    Ok(serde_json::json!({}))
}

#[tauri::command]
pub async fn save_asr_credentials(
    _provider: String,
    _credentials: serde_json::Value,
) -> Result<(), String> {
    debug!("save_asr_credentials stub");
    Ok(())
}

// ---------------------------------------------------------------------------
// Settings: LLM provider
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn get_llm_provider() -> Result<String, String> {
    Ok("none".into())
}

#[tauri::command]
pub async fn set_llm_provider(_provider: String) -> Result<(), String> {
    debug!(provider = %_provider, "set_llm_provider stub");
    Ok(())
}

#[tauri::command]
pub async fn get_llm_credentials(
    _provider: String,
) -> Result<serde_json::Value, String> {
    Ok(serde_json::json!({}))
}

#[tauri::command]
pub async fn save_llm_credentials(
    _provider: String,
    _credentials: serde_json::Value,
) -> Result<(), String> {
    debug!("save_llm_credentials stub");
    Ok(())
}

// ---------------------------------------------------------------------------
// Model management (stubs for local ASR)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ModelStatus {
    pub model_id: String,
    pub is_downloaded: bool,
    pub size_bytes: Option<u64>,
    pub path: Option<String>,
}

#[tauri::command]
pub async fn get_model_status(_model_id: String) -> Result<ModelStatus, String> {
    Ok(ModelStatus {
        model_id: _model_id,
        is_downloaded: false,
        size_bytes: None,
        path: None,
    })
}

#[tauri::command]
pub async fn download_model(_model_id: String) -> Result<(), String> {
    debug!(model = %_model_id, "download_model stub");
    Err("model download not implemented yet".into())
}

#[tauri::command]
pub async fn delete_model(_model_id: String) -> Result<(), String> {
    debug!(model = %_model_id, "delete_model stub");
    Err("model deletion not implemented yet".into())
}

// ---------------------------------------------------------------------------
// App edition / region
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn get_app_edition() -> Result<String, String> {
    Ok("free".into())
}

#[tauri::command]
pub async fn set_app_edition(_edition: String) -> Result<(), String> {
    debug!(edition = %_edition, "set_app_edition stub");
    Ok(())
}

#[tauri::command]
pub async fn get_cloud_region() -> Result<String, String> {
    Ok("cn".into())
}

#[tauri::command]
pub async fn set_cloud_region(_region: String) -> Result<(), String> {
    debug!(region = %_region, "set_cloud_region stub");
    Ok(())
}
