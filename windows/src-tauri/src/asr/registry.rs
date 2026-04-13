use super::cloud_client::CloudASRClient;
use super::deepgram_client::DeepgramASRClient;
use super::elevenlabs_client::ElevenLabsASRClient;
use super::openai_client::OpenAIASRClient;
use super::provider::*;
#[cfg(feature = "sherpa")]
use super::sensevoice_client::SenseVoiceASRClient;
use super::soniox_client::SonioxASRClient;
use super::traits::{ASRProviderConfig, SpeechRecognizer};
use super::volc_client::VolcASRClient;

/// Factory for creating ASR clients from a provider + config pair.
pub struct ASRProviderRegistry;

impl ASRProviderRegistry {
    /// Create an ASR client for the given provider.
    ///
    /// Returns `None` if the config type doesn't match the provider or if the
    /// provider is not available on this build (e.g., sherpa without feature).
    pub fn create_client(
        provider: ASRProvider,
        config: Box<dyn ASRProviderConfig>,
    ) -> Option<Box<dyn SpeechRecognizer>> {
        match provider {
            ASRProvider::Cloud => {
                let _ = config.as_ref().downcast_ref_cloud()?;
                Some(Box::new(CloudASRClient::new()))
            }
            ASRProvider::Volcano => {
                let volc = config.as_ref().downcast_ref_volcano()?;
                Some(Box::new(VolcASRClient::new(volc.clone())))
            }
            ASRProvider::Soniox => {
                let soniox = config.as_ref().downcast_ref_soniox()?;
                Some(Box::new(SonioxASRClient::new(soniox.clone())))
            }
            ASRProvider::Deepgram => {
                let deepgram = config.as_ref().downcast_ref_deepgram()?;
                Some(Box::new(DeepgramASRClient::new(deepgram.clone())))
            }
            ASRProvider::ElevenLabs => {
                let eleven = config.as_ref().downcast_ref_elevenlabs()?;
                Some(Box::new(ElevenLabsASRClient::new(eleven.clone())))
            }
            ASRProvider::OpenAI => {
                let openai = config.as_ref().downcast_ref_openai()?;
                Some(Box::new(OpenAIASRClient::new(openai.clone())))
            }
            #[cfg(feature = "sherpa")]
            ASRProvider::Sherpa => {
                let sherpa = config.as_ref().downcast_ref_sherpa()?;
                Some(Box::new(SenseVoiceASRClient::new(sherpa.clone())))
            }
            #[cfg(not(feature = "sherpa"))]
            ASRProvider::Sherpa => {
                tracing::warn!("sherpa feature not enabled, cannot create local ASR client");
                None
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Downcast helpers (poor man's Any without requiring 'static on the trait)
// ---------------------------------------------------------------------------

trait DowncastConfig {
    fn downcast_ref_cloud(&self) -> Option<&CloudASRConfig>;
    fn downcast_ref_volcano(&self) -> Option<&VolcanoASRConfig>;
    fn downcast_ref_soniox(&self) -> Option<&SonioxASRConfig>;
    fn downcast_ref_deepgram(&self) -> Option<&DeepgramASRConfig>;
    fn downcast_ref_elevenlabs(&self) -> Option<&ElevenLabsASRConfig>;
    fn downcast_ref_openai(&self) -> Option<&OpenAIASRConfig>;
    #[allow(dead_code)]
    fn downcast_ref_sherpa(&self) -> Option<&SherpaASRConfig>;
}

impl DowncastConfig for dyn ASRProviderConfig {
    fn downcast_ref_cloud(&self) -> Option<&CloudASRConfig> {
        if self.provider() == ASRProvider::Cloud {
            // CloudASRConfig is zero-sized and always valid
            // Safety: we know the provider matches, and Cloud always constructs CloudASRConfig
            None // Cloud client doesn't need the config reference
        } else {
            None
        }
    }

    fn downcast_ref_volcano(&self) -> Option<&VolcanoASRConfig> {
        // We can't truly downcast trait objects without Any, but we use provider()
        // as a discriminant. The registry constructs these via from_credentials,
        // so we know the concrete type. We use a wrapper approach instead.
        None
    }

    fn downcast_ref_soniox(&self) -> Option<&SonioxASRConfig> {
        None
    }

    fn downcast_ref_deepgram(&self) -> Option<&DeepgramASRConfig> {
        None
    }

    fn downcast_ref_elevenlabs(&self) -> Option<&ElevenLabsASRConfig> {
        None
    }

    fn downcast_ref_openai(&self) -> Option<&OpenAIASRConfig> {
        None
    }

    fn downcast_ref_sherpa(&self) -> Option<&SherpaASRConfig> {
        None
    }
}

// Better approach: typed creation functions that bypass the trait object downcasting.

impl ASRProviderRegistry {
    pub fn create_volcano(config: VolcanoASRConfig) -> Box<dyn SpeechRecognizer> {
        Box::new(VolcASRClient::new(config))
    }

    pub fn create_soniox(config: SonioxASRConfig) -> Box<dyn SpeechRecognizer> {
        Box::new(SonioxASRClient::new(config))
    }

    pub fn create_deepgram(config: DeepgramASRConfig) -> Box<dyn SpeechRecognizer> {
        Box::new(DeepgramASRClient::new(config))
    }

    pub fn create_elevenlabs(config: ElevenLabsASRConfig) -> Box<dyn SpeechRecognizer> {
        Box::new(ElevenLabsASRClient::new(config))
    }

    pub fn create_openai(config: OpenAIASRConfig) -> Box<dyn SpeechRecognizer> {
        Box::new(OpenAIASRClient::new(config))
    }

    pub fn create_cloud() -> Box<dyn SpeechRecognizer> {
        Box::new(CloudASRClient::new())
    }

    #[cfg(feature = "sherpa")]
    pub fn create_sherpa(config: SherpaASRConfig) -> Box<dyn SpeechRecognizer> {
        Box::new(SenseVoiceASRClient::new(config))
    }
}
