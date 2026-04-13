// Type4Me Windows – Tauri v2 voice input app
// Crate: type4me_lib

pub mod app_state;
pub mod audio;
pub mod asr;
pub mod auth;
pub mod credential;
pub mod hotkey;
pub mod injection;
pub mod ipc;
pub mod llm;
pub mod model;
pub mod protocol;
pub mod session;
pub mod sherpa;
pub mod sound;
pub mod tray;

use app_state::create_app_state;
use tauri::Manager;
use tracing_subscriber::EnvFilter;

/// Application entry point called from `main.rs`.
pub fn run() {
    // Initialize tracing (respects RUST_LOG env, defaults to info)
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    tracing::info!("Type4Me starting up");

    let state = create_app_state();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            // Mode management
            ipc::commands::get_modes,
            ipc::commands::save_modes,
            ipc::commands::get_current_mode,
            ipc::commands::set_current_mode,
            ipc::commands::open_settings_window,
            // Session
            ipc::commands::start_recording,
            ipc::commands::stop_recording,
            ipc::commands::cancel_recording,
            // Auth
            ipc::commands::auth_send_code,
            ipc::commands::auth_verify,
            ipc::commands::auth_sign_out,
            ipc::commands::auth_status,
            // Quota
            ipc::commands::quota_refresh,
            ipc::commands::quota_can_use,
            // ASR settings
            ipc::commands::get_asr_provider,
            ipc::commands::set_asr_provider,
            ipc::commands::get_asr_credentials,
            ipc::commands::save_asr_credentials,
            // LLM settings
            ipc::commands::get_llm_provider,
            ipc::commands::set_llm_provider,
            ipc::commands::get_llm_credentials,
            ipc::commands::save_llm_credentials,
            // Models
            ipc::commands::get_model_status,
            ipc::commands::download_model,
            ipc::commands::delete_model,
            // General
            ipc::commands::get_app_edition,
            ipc::commands::set_app_edition,
            ipc::commands::get_cloud_region,
            ipc::commands::set_cloud_region,
        ])
        .setup(|app| {
            tray::setup_tray(app)?;

            // Load persisted modes if they exist
            let handle = app.handle().clone();
            let state: tauri::State<'_, app_state::AppState> = app.state();
            let state_clone = state.inner().clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = load_persisted_modes(&handle, &state_clone).await {
                    tracing::warn!("failed to load persisted modes: {e}");
                }
            });

            tracing::info!("Type4Me setup complete");
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("failed to run Type4Me");
}

/// Load modes from `modes.json` if it exists, merging with defaults.
async fn load_persisted_modes(
    app: &tauri::AppHandle,
    state: &app_state::AppState,
) -> anyhow::Result<()> {
    let path = app
        .path()
        .app_data_dir()
        .unwrap_or_else(|_| std::path::PathBuf::from("."))
        .join("modes.json");

    if !path.exists() {
        return Ok(());
    }

    let content = std::fs::read_to_string(&path)?;
    let modes: Vec<app_state::ProcessingMode> = serde_json::from_str(&content)?;

    let mut inner = state.lock().await;
    inner.available_modes = modes;
    if !inner
        .available_modes
        .iter()
        .any(|m| m.id == inner.current_mode.id)
    {
        if let Some(first) = inner.available_modes.first() {
            inner.current_mode = first.clone();
        }
    }

    tracing::info!(count = inner.available_modes.len(), "loaded persisted modes");
    Ok(())
}
