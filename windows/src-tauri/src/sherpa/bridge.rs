// sherpa-onnx C FFI bridge for local ASR inference.
//
// The entire module is gated behind `#[cfg(feature = "sherpa")]`.
// When the feature is disabled, this file compiles to nothing.
//
// The actual linking to the sherpa-onnx shared library happens via build.rs.

#[cfg(feature = "sherpa")]
mod inner {
    use anyhow::{bail, Context, Result};
    use std::ffi::{CStr, CString};
    use std::os::raw::{c_char, c_float, c_int};
    use std::path::Path;
    use tracing::{debug, info};

    // -----------------------------------------------------------------------
    // FFI struct layouts (must match sherpa-onnx C API headers)
    // -----------------------------------------------------------------------

    #[repr(C)]
    struct SherpaOnnxOfflineSenseVoiceModelConfig {
        model: *const c_char,
        language: *const c_char,
        use_itn: c_int,
    }

    #[repr(C)]
    struct SherpaOnnxOfflineModelConfig {
        transducer: SherpaOnnxOfflineTransducerModelConfig,
        paraformer: SherpaOnnxOfflineParaformerModelConfig,
        nemo_ctc: SherpaOnnxOfflineNemoEncDecCtcModelConfig,
        whisper: SherpaOnnxOfflineWhisperModelConfig,
        tdnn: SherpaOnnxOfflineTdnnModelConfig,
        tokens: *const c_char,
        num_threads: c_int,
        debug: c_int,
        provider: *const c_char,
        model_type: *const c_char,
        modeling_unit: *const c_char,
        bpe_vocab: *const c_char,
        telespeech_ctc: *const c_char,
        sense_voice: SherpaOnnxOfflineSenseVoiceModelConfig,
    }

    // Placeholder structs for model types we don't use but need for layout.
    #[repr(C)]
    struct SherpaOnnxOfflineTransducerModelConfig {
        encoder: *const c_char,
        decoder: *const c_char,
        joiner: *const c_char,
    }

    #[repr(C)]
    struct SherpaOnnxOfflineParaformerModelConfig {
        model: *const c_char,
    }

    #[repr(C)]
    struct SherpaOnnxOfflineNemoEncDecCtcModelConfig {
        model: *const c_char,
    }

    #[repr(C)]
    struct SherpaOnnxOfflineWhisperModelConfig {
        encoder: *const c_char,
        decoder: *const c_char,
        language: *const c_char,
        task: *const c_char,
        tail_paddings: c_int,
    }

    #[repr(C)]
    struct SherpaOnnxOfflineTdnnModelConfig {
        model: *const c_char,
    }

    #[repr(C)]
    struct SherpaOnnxOfflineRecognizerConfig {
        feat_config: SherpaOnnxFeatureConfig,
        model_config: SherpaOnnxOfflineModelConfig,
        lm_config: SherpaOnnxOfflineLmConfig,
        decoding_method: *const c_char,
        max_active_paths: c_int,
        hotwords_file: *const c_char,
        hotwords_score: c_float,
        rule_fsts: *const c_char,
        rule_fars: *const c_char,
    }

    #[repr(C)]
    struct SherpaOnnxFeatureConfig {
        sample_rate: c_int,
        feature_dim: c_int,
    }

    #[repr(C)]
    struct SherpaOnnxOfflineLmConfig {
        model: *const c_char,
        scale: c_float,
    }

    // Opaque pointers
    enum SherpaOnnxOfflineRecognizer {}
    enum SherpaOnnxOfflineStream {}

    #[repr(C)]
    struct SherpaOnnxOfflineRecognizerResult {
        text: *const c_char,
        timestamps: *const c_float,
        count: c_int,
        tokens: *const *const c_char,
        tokens_arr: *const *const c_char,
        lang: *const c_char,
        emotion: *const c_char,
        event: *const c_char,
    }

    // -----------------------------------------------------------------------
    // Extern C functions
    // -----------------------------------------------------------------------

    extern "C" {
        fn SherpaOnnxCreateOfflineRecognizer(
            config: *const SherpaOnnxOfflineRecognizerConfig,
        ) -> *mut SherpaOnnxOfflineRecognizer;

        fn SherpaOnnxDestroyOfflineRecognizer(recognizer: *mut SherpaOnnxOfflineRecognizer);

        fn SherpaOnnxCreateOfflineStream(
            recognizer: *const SherpaOnnxOfflineRecognizer,
        ) -> *mut SherpaOnnxOfflineStream;

        fn SherpaOnnxDestroyOfflineStream(stream: *mut SherpaOnnxOfflineStream);

        fn SherpaOnnxAcceptWaveformOffline(
            stream: *mut SherpaOnnxOfflineStream,
            sample_rate: c_int,
            samples: *const c_float,
            n: c_int,
        );

        fn SherpaOnnxDecodeOfflineStream(
            recognizer: *mut SherpaOnnxOfflineRecognizer,
            stream: *mut SherpaOnnxOfflineStream,
        );

        fn SherpaOnnxGetOfflineStreamResult(
            stream: *const SherpaOnnxOfflineStream,
        ) -> *const SherpaOnnxOfflineRecognizerResult;

        fn SherpaOnnxDestroyOfflineRecognizerResult(
            result: *const SherpaOnnxOfflineRecognizerResult,
        );
    }

    // -----------------------------------------------------------------------
    // Safe Rust wrapper
    // -----------------------------------------------------------------------

    pub struct SherpaRecognizer {
        recognizer: *mut SherpaOnnxOfflineRecognizer,
    }

    // SAFETY: The sherpa-onnx C API is thread-safe for separate recognizer instances.
    unsafe impl Send for SherpaRecognizer {}
    unsafe impl Sync for SherpaRecognizer {}

    impl SherpaRecognizer {
        /// Create a new offline recognizer from the model directory.
        ///
        /// The directory must contain the SenseVoice model file and tokens.txt.
        pub fn new(model_path: &Path) -> Result<Self> {
            let model_file = model_path
                .join("model.onnx")
                .to_string_lossy()
                .to_string();
            let tokens_file = model_path
                .join("tokens.txt")
                .to_string_lossy()
                .to_string();

            // Verify files exist
            if !Path::new(&model_file).exists() {
                // Try alternative name
                let alt = model_path.join("model.int8.onnx");
                if !alt.exists() {
                    bail!(
                        "model file not found: {} or {}",
                        model_file,
                        alt.display()
                    );
                }
            }
            if !Path::new(&tokens_file).exists() {
                bail!("tokens file not found: {tokens_file}");
            }

            let c_model = CString::new(model_file.as_str()).context("invalid model path")?;
            let c_tokens = CString::new(tokens_file.as_str()).context("invalid tokens path")?;
            let c_empty = CString::new("").unwrap();
            let c_language = CString::new("auto").unwrap();
            let c_provider = CString::new("cpu").unwrap();
            let c_greedy = CString::new("greedy_search").unwrap();

            let config = SherpaOnnxOfflineRecognizerConfig {
                feat_config: SherpaOnnxFeatureConfig {
                    sample_rate: 16000,
                    feature_dim: 80,
                },
                model_config: SherpaOnnxOfflineModelConfig {
                    transducer: SherpaOnnxOfflineTransducerModelConfig {
                        encoder: c_empty.as_ptr(),
                        decoder: c_empty.as_ptr(),
                        joiner: c_empty.as_ptr(),
                    },
                    paraformer: SherpaOnnxOfflineParaformerModelConfig {
                        model: c_empty.as_ptr(),
                    },
                    nemo_ctc: SherpaOnnxOfflineNemoEncDecCtcModelConfig {
                        model: c_empty.as_ptr(),
                    },
                    whisper: SherpaOnnxOfflineWhisperModelConfig {
                        encoder: c_empty.as_ptr(),
                        decoder: c_empty.as_ptr(),
                        language: c_empty.as_ptr(),
                        task: c_empty.as_ptr(),
                        tail_paddings: 0,
                    },
                    tdnn: SherpaOnnxOfflineTdnnModelConfig {
                        model: c_empty.as_ptr(),
                    },
                    tokens: c_tokens.as_ptr(),
                    num_threads: 4,
                    debug: 0,
                    provider: c_provider.as_ptr(),
                    model_type: c_empty.as_ptr(),
                    modeling_unit: c_empty.as_ptr(),
                    bpe_vocab: c_empty.as_ptr(),
                    telespeech_ctc: c_empty.as_ptr(),
                    sense_voice: SherpaOnnxOfflineSenseVoiceModelConfig {
                        model: c_model.as_ptr(),
                        language: c_language.as_ptr(),
                        use_itn: 1,
                    },
                },
                lm_config: SherpaOnnxOfflineLmConfig {
                    model: c_empty.as_ptr(),
                    scale: 1.0,
                },
                decoding_method: c_greedy.as_ptr(),
                max_active_paths: 4,
                hotwords_file: c_empty.as_ptr(),
                hotwords_score: 1.5,
                rule_fsts: c_empty.as_ptr(),
                rule_fars: c_empty.as_ptr(),
            };

            let recognizer =
                unsafe { SherpaOnnxCreateOfflineRecognizer(&config) };

            if recognizer.is_null() {
                bail!("SherpaOnnxCreateOfflineRecognizer returned null");
            }

            info!("sherpa-onnx recognizer created");
            Ok(Self { recognizer })
        }

        /// Decode audio samples and return the recognized text.
        ///
        /// `audio` is expected to be f32 samples at 16 kHz mono.
        pub fn decode(&self, audio: &[f32]) -> Result<String> {
            if audio.is_empty() {
                return Ok(String::new());
            }

            unsafe {
                let stream = SherpaOnnxCreateOfflineStream(self.recognizer);
                if stream.is_null() {
                    bail!("SherpaOnnxCreateOfflineStream returned null");
                }

                SherpaOnnxAcceptWaveformOffline(
                    stream,
                    16000,
                    audio.as_ptr(),
                    audio.len() as c_int,
                );

                SherpaOnnxDecodeOfflineStream(self.recognizer, stream);

                let result = SherpaOnnxGetOfflineStreamResult(stream);
                let text = if !result.is_null() && !(*result).text.is_null() {
                    CStr::from_ptr((*result).text)
                        .to_string_lossy()
                        .trim()
                        .to_string()
                } else {
                    String::new()
                };

                if !result.is_null() {
                    SherpaOnnxDestroyOfflineRecognizerResult(result);
                }
                SherpaOnnxDestroyOfflineStream(stream);

                debug!(text_len = text.len(), "sherpa decode complete");
                Ok(text)
            }
        }

        /// Reset internal state (no-op for offline recognizer, included for API symmetry).
        pub fn reset(&self) {
            // Offline recognizer doesn't have persistent state between decodes.
        }
    }

    impl Drop for SherpaRecognizer {
        fn drop(&mut self) {
            if !self.recognizer.is_null() {
                unsafe {
                    SherpaOnnxDestroyOfflineRecognizer(self.recognizer);
                }
                debug!("sherpa-onnx recognizer destroyed");
            }
        }
    }
}

#[cfg(feature = "sherpa")]
pub use inner::SherpaRecognizer;

// When the sherpa feature is disabled, provide a stub so downstream code
// can reference the type without cfg-gating every usage.
#[cfg(not(feature = "sherpa"))]
pub struct SherpaRecognizerStub;

#[cfg(not(feature = "sherpa"))]
impl SherpaRecognizerStub {
    pub fn new(_model_path: &std::path::Path) -> anyhow::Result<Self> {
        anyhow::bail!("sherpa feature is not enabled");
    }

    pub fn decode(&self, _audio: &[f32]) -> anyhow::Result<String> {
        anyhow::bail!("sherpa feature is not enabled");
    }

    pub fn reset(&self) {}
}
