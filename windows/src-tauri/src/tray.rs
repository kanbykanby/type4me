use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    App, Manager,
};
use tracing::debug;

use crate::app_state::AppState;

/// Wire up the system tray icon, menu, and event handlers.
pub fn setup_tray(app: &App) -> Result<(), Box<dyn std::error::Error>> {
    let toggle_recording = MenuItem::with_id(app, "toggle_recording", "Start Recording", true, None::<&str>)?;
    let separator1 = PredefinedMenuItem::separator(app)?;
    let settings = MenuItem::with_id(app, "settings", "Settings", true, None::<&str>)?;
    let about = MenuItem::with_id(app, "about", "About Type4Me", true, None::<&str>)?;
    let separator2 = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;

    let menu = Menu::with_items(
        app,
        &[
            &toggle_recording,
            &separator1,
            &settings,
            &about,
            &separator2,
            &quit,
        ],
    )?;

    let _tray = TrayIconBuilder::new()
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(move |app, event| {
            let id = event.id.as_ref();
            debug!(menu_id = id, "tray menu event");

            match id {
                "toggle_recording" => {
                    let app = app.clone();
                    tauri::async_runtime::spawn(async move {
                        handle_toggle_recording(&app).await;
                    });
                }
                "settings" => {
                    if let Some(win) = app.get_webview_window("settings") {
                        let _ = win.show();
                        let _ = win.set_focus();
                    }
                }
                "about" => {
                    // For now just show the settings window; a proper About
                    // dialog can be added later.
                    if let Some(win) = app.get_webview_window("settings") {
                        let _ = win.show();
                        let _ = win.set_focus();
                    }
                }
                "quit" => {
                    app.exit(0);
                }
                _ => {}
            }
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle().clone();
                tauri::async_runtime::spawn(async move {
                    handle_toggle_recording(&app).await;
                });
            }
        })
        .build(app)?;

    Ok(())
}

async fn handle_toggle_recording(app: &tauri::AppHandle) {
    let state: tauri::State<'_, AppState> = app.state();
    let mut inner = state.lock().await;

    if inner.is_recording {
        inner.is_recording = false;
        inner.bar_phase = crate::app_state::FloatingBarPhase::Hidden;
        debug!("tray: recording stopped");
        // TODO(phase2): actually stop audio + ASR session

        // Update the tray menu label. If the menu item is accessible, update
        // its text. We don't hold onto the MenuItem directly because Tauri
        // manages the tray; this is a best-effort approach.
    } else {
        inner.is_recording = true;
        inner.bar_phase = crate::app_state::FloatingBarPhase::Recording;
        debug!("tray: recording started");
        // TODO(phase2): actually start audio + ASR session
    }
}
