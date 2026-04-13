pub mod cloud_client;
pub mod deepgram_client;
pub mod elevenlabs_client;
pub mod openai_client;
pub mod provider;
pub mod registry;
#[cfg(feature = "sherpa")]
pub mod sensevoice_client;
pub mod soniox_client;
pub mod traits;
pub mod volc_client;

// Re-exports for convenience
pub use provider::{all_providers, ASRProvider, ASRProviderInfo};
pub use registry::ASRProviderRegistry;
pub use traits::{
    ASRProviderConfig, ASRRequestOptions, CredentialField, RecognitionEvent, RecognitionTranscript,
    SpeechRecognizer,
};
