use anyhow::Result;
use tokio::sync::mpsc;

/// Target audio format (must match macOS Type4Me):
/// - 16 kHz mono, 16-bit signed integer (little-endian)
#[allow(dead_code)]
const TARGET_SAMPLE_RATE: u32 = 16_000;
#[allow(dead_code)]
const TARGET_CHANNELS: u16 = 1;
/// 200ms chunk = 3200 samples at 16 kHz
#[allow(dead_code)]
const SAMPLES_PER_CHUNK: usize = 3_200;
/// 3200 samples * 2 bytes = 6400 bytes per chunk
#[allow(dead_code)]
const BYTES_PER_CHUNK: usize = SAMPLES_PER_CHUNK * 2;
/// Calculate audio level every ~50ms = 800 samples at 16 kHz
#[allow(dead_code)]
const LEVEL_INTERVAL_SAMPLES: usize = 800;

// =============================================================================
// Windows implementation (cpal)
// =============================================================================

#[cfg(windows)]
mod platform {
    use super::*;
    use crate::audio::level::calculate_rms_level;
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    use std::sync::{Arc, Mutex};

    /// Shared state between the cpal callback thread and the control side.
    struct CaptureShared {
        /// Accumulated i16 samples at 16 kHz (the target rate)
        accumulated: Vec<i16>,
        /// Pending samples not yet emitted as a chunk
        pending_chunk: Vec<i16>,
        /// Pending samples not yet used for a level reading
        pending_level: Vec<i16>,
        /// Whether we are actively capturing
        active: bool,
    }

    pub struct AudioCaptureEngine {
        shared: Arc<Mutex<CaptureShared>>,
        stream: Option<cpal::Stream>,
    }

    impl AudioCaptureEngine {
        pub fn new() -> Result<Self> {
            Ok(Self {
                shared: Arc::new(Mutex::new(CaptureShared {
                    accumulated: Vec::new(),
                    pending_chunk: Vec::new(),
                    pending_level: Vec::new(),
                    active: false,
                })),
                stream: None,
            })
        }

        pub fn start(
            &mut self,
        ) -> Result<(mpsc::UnboundedReceiver<Vec<u8>>, mpsc::UnboundedReceiver<f32>)> {
            let (chunk_tx, chunk_rx) = mpsc::unbounded_channel::<Vec<u8>>();
            let (level_tx, level_rx) = mpsc::unbounded_channel::<f32>();

            // Reset shared state
            {
                let mut shared = self.shared.lock().unwrap();
                shared.accumulated.clear();
                shared.pending_chunk.clear();
                shared.pending_level.clear();
                shared.active = true;
            }

            let host = cpal::default_host();
            let device = host
                .default_input_device()
                .ok_or_else(|| anyhow::anyhow!("no input audio device available"))?;

            let device_name = device.name().unwrap_or_else(|_| "<unknown>".into());
            tracing::info!("audio capture device: {}", device_name);

            let supported_config = device.default_input_config()?;
            let device_sample_rate = supported_config.sample_rate().0;
            let device_channels = supported_config.channels();

            tracing::info!(
                "device format: {}Hz, {} channels, {:?}",
                device_sample_rate,
                device_channels,
                supported_config.sample_format()
            );

            let needs_resample = device_sample_rate != TARGET_SAMPLE_RATE;
            let shared = Arc::clone(&self.shared);

            // Build the data callback. We always read f32 from cpal (most compatible),
            // then convert to i16 and optionally resample to 16 kHz.
            let config: cpal::StreamConfig = supported_config.clone().into();

            let err_callback = |err: cpal::StreamError| {
                tracing::error!("audio stream error: {}", err);
            };

            let stream = match supported_config.sample_format() {
                cpal::SampleFormat::F32 => {
                    let callback = build_callback::<f32>(
                        shared,
                        chunk_tx,
                        level_tx,
                        device_sample_rate,
                        device_channels,
                        needs_resample,
                    );
                    device.build_input_stream(&config, callback, err_callback, None)?
                }
                cpal::SampleFormat::I16 => {
                    let callback = build_callback::<i16>(
                        shared,
                        chunk_tx,
                        level_tx,
                        device_sample_rate,
                        device_channels,
                        needs_resample,
                    );
                    device.build_input_stream(&config, callback, err_callback, None)?
                }
                cpal::SampleFormat::U16 => {
                    let callback = build_callback::<u16>(
                        shared,
                        chunk_tx,
                        level_tx,
                        device_sample_rate,
                        device_channels,
                        needs_resample,
                    );
                    device.build_input_stream(&config, callback, err_callback, None)?
                }
                fmt => {
                    anyhow::bail!("unsupported sample format: {:?}", fmt);
                }
            };

            stream.play()?;
            self.stream = Some(stream);

            tracing::info!("audio capture started");
            Ok((chunk_rx, level_rx))
        }

        pub fn stop(&mut self) {
            if let Some(stream) = self.stream.take() {
                drop(stream);
            }
            if let Ok(mut shared) = self.shared.lock() {
                shared.active = false;
            }
            tracing::info!("audio capture stopped");
        }

        pub fn get_accumulated_audio(&self) -> Vec<u8> {
            let shared = self.shared.lock().unwrap();
            samples_to_bytes(&shared.accumulated)
        }
    }

    /// Trait to convert various cpal sample types to f64.
    trait SampleToF64: cpal::Sample + Send + 'static {
        fn to_f64(self) -> f64;
    }

    impl SampleToF64 for f32 {
        fn to_f64(self) -> f64 {
            self as f64
        }
    }

    impl SampleToF64 for i16 {
        fn to_f64(self) -> f64 {
            self as f64 / 32768.0
        }
    }

    impl SampleToF64 for u16 {
        fn to_f64(self) -> f64 {
            // u16 audio: 0..65535, midpoint at 32768
            (self as f64 - 32768.0) / 32768.0
        }
    }

    /// Build the cpal data callback that handles resampling + chunking + level calculation.
    fn build_callback<S: SampleToF64>(
        shared: Arc<Mutex<CaptureShared>>,
        chunk_tx: mpsc::UnboundedSender<Vec<u8>>,
        level_tx: mpsc::UnboundedSender<f32>,
        device_sample_rate: u32,
        device_channels: u16,
        needs_resample: bool,
    ) -> impl FnMut(&[S], &cpal::InputCallbackInfo) + Send + 'static {
        // Resample state: fractional accumulator for linear interpolation
        let mut resample_accum: f64 = 0.0;
        let mut prev_sample: f64 = 0.0;
        let ratio = TARGET_SAMPLE_RATE as f64 / device_sample_rate as f64;

        move |data: &[S], _info: &cpal::InputCallbackInfo| {
            let mut shared = match shared.lock() {
                Ok(s) => s,
                Err(_) => return,
            };
            if !shared.active {
                return;
            }

            // Step 1: Convert to mono f64 samples at device rate
            let mono_samples: Vec<f64> = data
                .chunks(device_channels as usize)
                .map(|frame| {
                    // Average all channels to mono
                    let sum: f64 = frame.iter().map(|s| s.to_f64()).sum();
                    sum / device_channels as f64
                })
                .collect();

            // Step 2: Resample to 16 kHz if needed, or convert directly
            let resampled: Vec<i16> = if needs_resample {
                let mut out = Vec::new();
                for &sample in &mono_samples {
                    resample_accum += ratio;
                    while resample_accum >= 1.0 {
                        resample_accum -= 1.0;
                        // Linear interpolation between prev_sample and sample
                        let t = resample_accum; // 0..1 fraction past the sample point
                        let interp = prev_sample * t + sample * (1.0 - t);
                        let clamped = interp.clamp(-1.0, 1.0);
                        out.push((clamped * 32767.0) as i16);
                    }
                    prev_sample = sample;
                }
                out
            } else {
                mono_samples
                    .iter()
                    .map(|&s| {
                        let clamped = s.clamp(-1.0, 1.0);
                        (clamped * 32767.0) as i16
                    })
                    .collect()
            };

            // Step 3: Accumulate for batch retrieval
            shared.accumulated.extend_from_slice(&resampled);

            // Step 4: Accumulate for level calculation (~50ms intervals)
            shared.pending_level.extend_from_slice(&resampled);
            while shared.pending_level.len() >= LEVEL_INTERVAL_SAMPLES {
                let level_samples: Vec<i16> =
                    shared.pending_level.drain(..LEVEL_INTERVAL_SAMPLES).collect();
                let level = calculate_rms_level(&level_samples);
                // Best-effort send; if the receiver dropped, ignore
                let _ = level_tx.send(level);
            }

            // Step 5: Accumulate for chunk emission (200ms = 3200 samples)
            shared.pending_chunk.extend_from_slice(&resampled);
            while shared.pending_chunk.len() >= SAMPLES_PER_CHUNK {
                let chunk_samples: Vec<i16> =
                    shared.pending_chunk.drain(..SAMPLES_PER_CHUNK).collect();
                let bytes = samples_to_bytes(&chunk_samples);
                debug_assert_eq!(bytes.len(), BYTES_PER_CHUNK);
                let _ = chunk_tx.send(bytes);
            }
        }
    }

    /// Convert i16 samples to little-endian bytes.
    fn samples_to_bytes(samples: &[i16]) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(samples.len() * 2);
        for &s in samples {
            bytes.extend_from_slice(&s.to_le_bytes());
        }
        bytes
    }
}

// =============================================================================
// Non-Windows stub
// =============================================================================

#[cfg(not(windows))]
mod platform {
    use super::*;

    pub struct AudioCaptureEngine;

    impl AudioCaptureEngine {
        pub fn new() -> Result<Self> {
            tracing::warn!("AudioCaptureEngine: stub on non-Windows platform");
            Ok(Self)
        }

        pub fn start(
            &mut self,
        ) -> Result<(mpsc::UnboundedReceiver<Vec<u8>>, mpsc::UnboundedReceiver<f32>)> {
            let (_chunk_tx, chunk_rx) = mpsc::unbounded_channel::<Vec<u8>>();
            let (_level_tx, level_rx) = mpsc::unbounded_channel::<f32>();
            tracing::warn!("AudioCaptureEngine::start() is a no-op on this platform");
            Ok((chunk_rx, level_rx))
        }

        pub fn stop(&mut self) {
            tracing::warn!("AudioCaptureEngine::stop() is a no-op on this platform");
        }

        pub fn get_accumulated_audio(&self) -> Vec<u8> {
            Vec::new()
        }
    }
}

pub use platform::AudioCaptureEngine;
