use serde::{Deserialize, Serialize};

use crate::app_state::{FloatingBarPhase, ProcessingMode, TranscriptionSegment};

// ---------------------------------------------------------------------------
// Event name constants – keep in sync with the frontend
// ---------------------------------------------------------------------------

pub const BAR_PHASE_CHANGED: &str = "bar-phase-changed";
pub const TRANSCRIPT_UPDATED: &str = "transcript-updated";
pub const AUDIO_LEVEL: &str = "audio-level";
pub const SESSION_ERROR: &str = "session-error";
pub const SESSION_FINALIZED: &str = "session-finalized";
pub const MODEL_DOWNLOAD_PROGRESS: &str = "model-download-progress";
pub const MODEL_DOWNLOAD_COMPLETE: &str = "model-download-complete";
pub const AUTH_STATE_CHANGED: &str = "auth-state-changed";
pub const MODE_CHANGED: &str = "mode-changed";

// ---------------------------------------------------------------------------
// Payload structs for each event
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BarPhaseChangedPayload {
    pub phase: FloatingBarPhase,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TranscriptUpdatedPayload {
    pub segments: Vec<TranscriptionSegment>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AudioLevelPayload {
    pub level: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SessionErrorPayload {
    pub message: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SessionFinalizedPayload {
    pub text: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ModelDownloadProgressPayload {
    pub model_id: String,
    pub progress: f64,
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ModelDownloadCompletePayload {
    pub model_id: String,
    pub success: bool,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AuthStateChangedPayload {
    pub is_authenticated: bool,
    pub phone: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ModeChangedPayload {
    pub mode: ProcessingMode,
}
