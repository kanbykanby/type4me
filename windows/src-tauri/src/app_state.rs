use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;

// ---------------------------------------------------------------------------
// Enums & structs exposed to the frontend via IPC
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum FloatingBarPhase {
    Hidden,
    Preparing,
    Recording,
    Processing,
    Done,
    Error,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TranscriptionSegment {
    pub id: String,
    pub text: String,
    pub is_confirmed: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProcessingMode {
    pub id: String,
    pub name: String,
    pub prompt: String,
    pub is_builtin: bool,
    pub processing_label: String,
    pub hotkey_vk: Option<u32>,
    pub hotkey_modifiers: Option<u32>,
    pub hotkey_style: HotkeyStyle,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum HotkeyStyle {
    Hold,
    Toggle,
}

// ---------------------------------------------------------------------------
// Inner mutable state
// ---------------------------------------------------------------------------

pub struct AppStateInner {
    pub bar_phase: FloatingBarPhase,
    pub segments: Vec<TranscriptionSegment>,
    pub audio_level: f32,
    pub current_mode: ProcessingMode,
    pub available_modes: Vec<ProcessingMode>,
    pub feedback_message: String,
    pub is_recording: bool,
}

impl AppStateInner {
    pub fn new() -> Self {
        let modes = Self::default_modes();
        let current = modes[0].clone();
        Self {
            bar_phase: FloatingBarPhase::Hidden,
            segments: Vec::new(),
            audio_level: 0.0,
            current_mode: current,
            available_modes: modes,
            feedback_message: String::new(),
            is_recording: false,
        }
    }

    /// Built-in processing modes shipped with the app.
    pub fn default_modes() -> Vec<ProcessingMode> {
        vec![
            ProcessingMode {
                id: "direct".into(),
                name: "直接输入".into(),
                prompt: String::new(),
                is_builtin: true,
                processing_label: "识别中…".into(),
                // Ctrl  = VK_CONTROL (0xA2), Space = VK_SPACE (0x20)
                // MOD_CONTROL = 0x0002
                hotkey_vk: Some(0x20),
                hotkey_modifiers: Some(0x0002),
                hotkey_style: HotkeyStyle::Hold,
            },
            ProcessingMode {
                id: "polish".into(),
                name: "语音润色".into(),
                prompt: "请润色以下语音转文字结果，修正口语化表达，保持原意，输出书面中文：".into(),
                is_builtin: true,
                processing_label: "润色中…".into(),
                // Ctrl+Shift: MOD_CONTROL | MOD_SHIFT = 0x0002 | 0x0004 = 0x0006
                hotkey_vk: Some(0x20),
                hotkey_modifiers: Some(0x0006),
                hotkey_style: HotkeyStyle::Hold,
            },
            ProcessingMode {
                id: "translate_en".into(),
                name: "英文翻译".into(),
                prompt: "Translate the following speech-to-text result into fluent English. Output English only:".into(),
                is_builtin: true,
                processing_label: "翻译中…".into(),
                hotkey_vk: None,
                hotkey_modifiers: None,
                hotkey_style: HotkeyStyle::Hold,
            },
        ]
    }
}

impl Default for AppStateInner {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Shared handle passed to Tauri as managed state
// ---------------------------------------------------------------------------

pub type AppState = Arc<Mutex<AppStateInner>>;

/// Create a fresh `AppState` ready to be managed by Tauri.
pub fn create_app_state() -> AppState {
    Arc::new(Mutex::new(AppStateInner::new()))
}
