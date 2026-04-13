use anyhow::{bail, Result};
use serde::Serialize;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use crate::app_state::{FloatingBarPhase, ProcessingMode, TranscriptionSegment};
use crate::asr::traits::{ASRRequestOptions, RecognitionEvent, RecognitionTranscript, SpeechRecognizer};
use crate::injection::{InjectionOutcome, TextInjectionEngine};
use crate::llm::traits::LLMClient;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    Idle,
    Starting,
    Recording,
    Finishing,
    PostProcessing,
    Injecting,
}

#[derive(Clone, Debug)]
pub enum SessionEvent {
    StateChanged(SessionState),
    BarPhaseChanged(FloatingBarPhase),
    TranscriptUpdated(Vec<TranscriptionSegment>),
    AudioLevel(f32),
    Finalized {
        text: String,
        outcome: InjectionOutcome,
    },
    Error(String),
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Build `TranscriptionSegment` list from a `RecognitionTranscript`.
fn transcript_to_segments(t: &RecognitionTranscript) -> Vec<TranscriptionSegment> {
    let mut segments = Vec::new();

    for (i, confirmed) in t.confirmed_segments.iter().enumerate() {
        segments.push(TranscriptionSegment {
            id: format!("c{i}"),
            text: confirmed.clone(),
            is_confirmed: true,
        });
    }

    if !t.partial_text.is_empty() {
        segments.push(TranscriptionSegment {
            id: "partial".to_string(),
            text: t.partial_text.clone(),
            is_confirmed: false,
        });
    }

    segments
}

// ---------------------------------------------------------------------------
// RecognitionSession
// ---------------------------------------------------------------------------

pub struct RecognitionSession {
    state: SessionState,
    generation: u64,
    event_tx: mpsc::UnboundedSender<SessionEvent>,
    /// Handle to the spawned orchestrator task, for cancellation.
    task_handle: Option<tokio::task::JoinHandle<()>>,
    /// Sender to signal "stop recording" to the running task.
    stop_tx: Option<tokio::sync::oneshot::Sender<()>>,
}

impl RecognitionSession {
    /// Create a new idle session.
    ///
    /// Returns the session and a receiver for events that the caller should
    /// forward to the frontend (e.g. via Tauri events).
    pub fn new() -> (Self, mpsc::UnboundedReceiver<SessionEvent>) {
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let session = Self {
            state: SessionState::Idle,
            generation: 0,
            event_tx,
            task_handle: None,
            stop_tx: None,
        };
        (session, event_rx)
    }

    /// Start a recording session.
    ///
    /// - `mode`: the processing mode (direct, polish, translate, etc.)
    /// - `asr_client`: a ready-to-use speech recognizer (not yet connected)
    /// - `llm_client`: optional LLM for post-processing (only used if mode has a prompt)
    /// - `audio_rx`: channel receiving raw 16-bit PCM chunks from the audio capture
    /// - `level_rx`: channel receiving audio level (0.0 .. 1.0) values for the UI meter
    /// - `options`: ASR request options (hotwords, punctuation, etc.)
    pub async fn start(
        &mut self,
        mode: ProcessingMode,
        asr_client: Box<dyn SpeechRecognizer>,
        llm_client: Option<Box<dyn LLMClient>>,
        audio_rx: mpsc::UnboundedReceiver<Vec<u8>>,
        level_rx: mpsc::UnboundedReceiver<f32>,
        options: ASRRequestOptions,
    ) -> Result<()> {
        if self.state != SessionState::Idle {
            bail!("session not idle (state: {:?})", self.state);
        }

        self.generation += 1;
        let gen = self.generation;
        self.set_state(SessionState::Starting);
        self.emit(SessionEvent::BarPhaseChanged(FloatingBarPhase::Preparing));

        let event_tx = self.event_tx.clone();
        let (stop_tx, stop_rx) = tokio::sync::oneshot::channel::<()>();
        self.stop_tx = Some(stop_tx);

        let handle = tokio::spawn(async move {
            if let Err(e) = run_session(
                gen,
                mode,
                asr_client,
                llm_client,
                audio_rx,
                level_rx,
                options,
                stop_rx,
                event_tx.clone(),
            )
            .await
            {
                error!("session error: {e:#}");
                let _ = event_tx.send(SessionEvent::Error(format!("{e:#}")));
                let _ = event_tx.send(SessionEvent::BarPhaseChanged(FloatingBarPhase::Error));
                let _ = event_tx.send(SessionEvent::StateChanged(SessionState::Idle));
            }
        });

        self.task_handle = Some(handle);
        Ok(())
    }

    /// Stop recording and trigger finalization (ASR final result, optional LLM, inject).
    pub async fn stop(&mut self) -> Result<()> {
        if self.state != SessionState::Recording && self.state != SessionState::Starting {
            warn!(state = ?self.state, "stop called in unexpected state");
            return Ok(());
        }

        info!("stopping recording");

        // Signal the orchestrator task to stop recording.
        if let Some(tx) = self.stop_tx.take() {
            let _ = tx.send(());
        }

        // The task will transition through Finishing -> PostProcessing -> Injecting -> Idle.
        // We don't await it here; the event stream carries all updates.
        Ok(())
    }

    /// Cancel the current session immediately. Tears down ASR and resets to idle.
    pub fn cancel(&mut self) {
        info!("cancelling session");

        if let Some(handle) = self.task_handle.take() {
            handle.abort();
        }
        self.stop_tx = None;

        self.set_state(SessionState::Idle);
        self.emit(SessionEvent::BarPhaseChanged(FloatingBarPhase::Hidden));
    }

    pub fn state(&self) -> SessionState {
        self.state
    }

    /// Call this when you receive a `StateChanged` event to keep the
    /// local state in sync with the background task.
    pub fn sync_state(&mut self, state: SessionState) {
        self.state = state;

        // Clean up handles when we go back to idle
        if state == SessionState::Idle {
            self.task_handle = None;
            self.stop_tx = None;
        }
    }

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    fn set_state(&mut self, state: SessionState) {
        self.state = state;
        let _ = self.event_tx.send(SessionEvent::StateChanged(state));
    }

    fn emit(&self, event: SessionEvent) {
        let _ = self.event_tx.send(event);
    }
}

// ---------------------------------------------------------------------------
// Orchestrator: the async task that runs the full pipeline
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
async fn run_session(
    _generation: u64,
    mode: ProcessingMode,
    mut asr_client: Box<dyn SpeechRecognizer>,
    llm_client: Option<Box<dyn LLMClient>>,
    mut audio_rx: mpsc::UnboundedReceiver<Vec<u8>>,
    mut level_rx: mpsc::UnboundedReceiver<f32>,
    options: ASRRequestOptions,
    stop_rx: tokio::sync::oneshot::Receiver<()>,
    event_tx: mpsc::UnboundedSender<SessionEvent>,
) -> Result<()> {
    let emit = |evt: SessionEvent| {
        let _ = event_tx.send(evt);
    };

    // We wrap the oneshot in a fuse so `select!` can poll it repeatedly.
    let mut stop_rx = stop_rx;
    let mut stop_received = false;

    // ------------------------------------------------------------------
    // Phase 1: Connect ASR
    // ------------------------------------------------------------------
    info!("connecting ASR client");
    asr_client.connect(&options).await?;

    let mut asr_event_rx = asr_client
        .take_event_rx()
        .ok_or_else(|| anyhow::anyhow!("ASR client has no event receiver"))?;

    // Wait for ASR Ready event (with timeout)
    let ready = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        while let Some(evt) = asr_event_rx.recv().await {
            match evt {
                RecognitionEvent::Ready => return Ok(()),
                RecognitionEvent::Error(e) => return Err(anyhow::anyhow!("ASR connect error: {e}")),
                _ => {}
            }
        }
        Err(anyhow::anyhow!("ASR event channel closed before Ready"))
    })
    .await;

    match ready {
        Ok(Ok(())) => {
            info!("ASR connected and ready");
        }
        Ok(Err(e)) => return Err(e),
        Err(_) => bail!("ASR connection timed out (10s)"),
    }

    emit(SessionEvent::StateChanged(SessionState::Recording));
    emit(SessionEvent::BarPhaseChanged(FloatingBarPhase::Recording));

    // ------------------------------------------------------------------
    // Phase 2: Stream audio + collect ASR events
    // ------------------------------------------------------------------
    let mut latest_transcript = RecognitionTranscript::empty();

    while !stop_received {
        tokio::select! {
            // Audio chunk from capture
            chunk = audio_rx.recv() => {
                match chunk {
                    Some(data) => {
                        if let Err(e) = asr_client.send_audio(&data).await {
                            warn!("error sending audio to ASR: {e}");
                        }
                    }
                    None => {
                        // Audio channel closed (capture stopped unexpectedly)
                        info!("audio channel closed, finishing");
                        break;
                    }
                }
            }

            // Audio level for the UI meter
            level = level_rx.recv() => {
                if let Some(lv) = level {
                    emit(SessionEvent::AudioLevel(lv));
                }
            }

            // ASR transcript events
            asr_evt = asr_event_rx.recv() => {
                match asr_evt {
                    Some(RecognitionEvent::Transcript(t)) => {
                        latest_transcript = t.clone();
                        let segments = transcript_to_segments(&t);
                        emit(SessionEvent::TranscriptUpdated(segments));
                    }
                    Some(RecognitionEvent::Error(e)) => {
                        warn!("ASR error during recording: {e}");
                    }
                    Some(RecognitionEvent::Completed) => {
                        info!("ASR signaled completion during recording");
                        break;
                    }
                    Some(RecognitionEvent::Ready) => {
                        // Duplicate Ready, ignore.
                    }
                    None => {
                        warn!("ASR event channel closed unexpectedly");
                        break;
                    }
                }
            }

            // Stop signal from the user
            _ = &mut stop_rx, if !stop_received => {
                info!("stop signal received");
                stop_received = true;
            }
        }
    }

    // ------------------------------------------------------------------
    // Phase 3: Finishing (get final ASR result)
    // ------------------------------------------------------------------
    emit(SessionEvent::StateChanged(SessionState::Finishing));
    emit(SessionEvent::BarPhaseChanged(FloatingBarPhase::Processing));

    // Signal ASR that no more audio is coming
    if let Err(e) = asr_client.end_audio().await {
        warn!("error ending ASR audio: {e}");
    }

    // Drain remaining ASR events until Completed / final transcript / timeout
    let drain_timeout = std::time::Duration::from_secs(15);
    let drain_result = tokio::time::timeout(drain_timeout, async {
        while let Some(evt) = asr_event_rx.recv().await {
            match evt {
                RecognitionEvent::Transcript(t) => {
                    latest_transcript = t.clone();
                    let segments = transcript_to_segments(&t);
                    emit(SessionEvent::TranscriptUpdated(segments));

                    if t.is_final {
                        debug!("received final transcript");
                        break;
                    }
                }
                RecognitionEvent::Completed => {
                    debug!("ASR completed");
                    break;
                }
                RecognitionEvent::Error(e) => {
                    warn!("ASR error during finalization: {e}");
                    break;
                }
                RecognitionEvent::Ready => {}
            }
        }
    })
    .await;

    if drain_result.is_err() {
        warn!("ASR finalization timed out after {drain_timeout:?}");
    }

    // Disconnect ASR
    asr_client.disconnect().await;

    // Build the recognized text
    let raw_text = latest_transcript.display_text();
    info!(text_len = raw_text.len(), "ASR finalized");

    if raw_text.trim().is_empty() {
        info!("empty transcript, nothing to inject");
        emit(SessionEvent::Finalized {
            text: String::new(),
            outcome: InjectionOutcome::Inserted,
        });
        emit(SessionEvent::StateChanged(SessionState::Idle));
        emit(SessionEvent::BarPhaseChanged(FloatingBarPhase::Hidden));
        return Ok(());
    }

    // ------------------------------------------------------------------
    // Phase 4: Optional LLM post-processing
    // ------------------------------------------------------------------
    let has_prompt = !mode.prompt.is_empty();
    let final_text = if has_prompt {
        if let Some(llm) = llm_client {
            emit(SessionEvent::StateChanged(SessionState::PostProcessing));
            info!("running LLM post-processing");

            match llm.process(&raw_text, &mode.prompt).await {
                Ok(processed) => {
                    info!(
                        original_len = raw_text.len(),
                        processed_len = processed.len(),
                        "LLM processing done"
                    );
                    processed
                }
                Err(e) => {
                    warn!("LLM processing failed: {e:#}, using raw text");
                    emit(SessionEvent::Error(format!("LLM error: {e:#}")));
                    raw_text
                }
            }
        } else {
            warn!("mode has prompt but no LLM client configured, using raw text");
            raw_text
        }
    } else {
        raw_text
    };

    // ------------------------------------------------------------------
    // Phase 5: Text injection
    // ------------------------------------------------------------------
    emit(SessionEvent::StateChanged(SessionState::Injecting));

    let engine = TextInjectionEngine::new();
    let outcome = match engine.inject(&final_text) {
        Ok(o) => {
            info!(outcome = ?o, "text injected");
            o
        }
        Err(e) => {
            warn!("injection failed: {e:#}, copying to clipboard");
            if let Err(e2) = engine.copy_to_clipboard(&final_text) {
                error!("clipboard copy also failed: {e2:#}");
            }
            InjectionOutcome::CopiedToClipboard
        }
    };

    // ------------------------------------------------------------------
    // Done
    // ------------------------------------------------------------------
    emit(SessionEvent::Finalized {
        text: final_text,
        outcome,
    });
    emit(SessionEvent::BarPhaseChanged(FloatingBarPhase::Done));

    // Brief pause so the "Done" phase is visible in the UI
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    emit(SessionEvent::StateChanged(SessionState::Idle));
    emit(SessionEvent::BarPhaseChanged(FloatingBarPhase::Hidden));

    Ok(())
}
