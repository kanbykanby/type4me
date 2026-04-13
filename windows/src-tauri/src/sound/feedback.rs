use anyhow::Result;
use rodio::{OutputStream, Sink, Source};
use std::time::Duration;
use tracing::{debug, warn};

/// Programmatically generated sound feedback for recording lifecycle events.
pub struct SoundFeedback {
    // OutputStream must be kept alive for the sink to produce audio.
    _stream: OutputStream,
    stream_handle: rodio::OutputStreamHandle,
}

impl SoundFeedback {
    pub fn new() -> Result<Self> {
        let (stream, handle) = OutputStream::try_default()
            .map_err(|e| anyhow::anyhow!("failed to open audio output: {e}"))?;
        debug!("sound feedback initialized");
        Ok(Self {
            _stream: stream,
            stream_handle: handle,
        })
    }

    /// Short ascending beep: 440 Hz → 880 Hz over 100 ms.
    pub fn play_start(&self) {
        if let Err(e) = self.play_sweep(440.0, 880.0, Duration::from_millis(100)) {
            warn!("failed to play start sound: {e}");
        }
    }

    /// Short descending beep: 880 Hz → 440 Hz over 100 ms.
    pub fn play_stop(&self) {
        if let Err(e) = self.play_sweep(880.0, 440.0, Duration::from_millis(100)) {
            warn!("failed to play stop sound: {e}");
        }
    }

    /// Two short low beeps: 220 Hz, 80 ms each, 40 ms gap.
    pub fn play_error(&self) {
        if let Err(e) = self.play_double_beep(220.0, Duration::from_millis(80), Duration::from_millis(40)) {
            warn!("failed to play error sound: {e}");
        }
    }

    fn play_sweep(&self, start_hz: f32, end_hz: f32, duration: Duration) -> Result<()> {
        let source = SweepTone::new(start_hz, end_hz, duration);
        let sink = Sink::try_new(&self.stream_handle)?;
        sink.append(source);
        sink.detach();
        Ok(())
    }

    fn play_double_beep(
        &self,
        freq: f32,
        beep_dur: Duration,
        gap_dur: Duration,
    ) -> Result<()> {
        let beep1 = SweepTone::new(freq, freq, beep_dur);
        let silence = rodio::source::Zero::<f32>::new(1, SAMPLE_RATE).take_duration(gap_dur);
        let beep2 = SweepTone::new(freq, freq, beep_dur);

        // Sink::append queues sources sequentially, so we get beep-gap-beep.
        let sink = Sink::try_new(&self.stream_handle)?;
        sink.append(beep1);
        sink.append(silence);
        sink.append(beep2);
        sink.detach();
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Sine-sweep source
// ---------------------------------------------------------------------------

const SAMPLE_RATE: u32 = 44100;
const AMPLITUDE: f32 = 0.25;

/// A sine tone that linearly sweeps from `start_hz` to `end_hz`.
struct SweepTone {
    start_hz: f32,
    end_hz: f32,
    total_samples: usize,
    current_sample: usize,
}

impl SweepTone {
    fn new(start_hz: f32, end_hz: f32, duration: Duration) -> Self {
        let total_samples = (SAMPLE_RATE as f64 * duration.as_secs_f64()) as usize;
        Self {
            start_hz,
            end_hz,
            total_samples,
            current_sample: 0,
        }
    }
}

impl Iterator for SweepTone {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        if self.current_sample >= self.total_samples {
            return None;
        }

        let t = self.current_sample as f32 / self.total_samples as f32;
        let freq = self.start_hz + (self.end_hz - self.start_hz) * t;
        let phase = 2.0 * std::f32::consts::PI * freq * (self.current_sample as f32 / SAMPLE_RATE as f32);
        let sample = AMPLITUDE * phase.sin();

        // Apply a short fade-in / fade-out envelope to avoid clicks.
        let fade_samples = (SAMPLE_RATE as f32 * 0.005) as usize; // 5 ms
        let envelope = if self.current_sample < fade_samples {
            self.current_sample as f32 / fade_samples as f32
        } else if self.current_sample > self.total_samples - fade_samples {
            (self.total_samples - self.current_sample) as f32 / fade_samples as f32
        } else {
            1.0
        };

        self.current_sample += 1;
        Some(sample * envelope)
    }
}

impl Source for SweepTone {
    fn current_frame_len(&self) -> Option<usize> {
        Some(self.total_samples - self.current_sample)
    }

    fn channels(&self) -> u16 {
        1
    }

    fn sample_rate(&self) -> u32 {
        SAMPLE_RATE
    }

    fn total_duration(&self) -> Option<Duration> {
        let remaining = self.total_samples.saturating_sub(self.current_sample);
        Some(Duration::from_secs_f64(remaining as f64 / SAMPLE_RATE as f64))
    }
}
