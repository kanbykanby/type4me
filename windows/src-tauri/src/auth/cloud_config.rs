use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CloudRegion {
    CN,
    Overseas,
}

pub struct CloudConfig;

impl CloudConfig {
    /// API endpoint for the given region.
    pub fn api_endpoint(region: CloudRegion) -> &'static str {
        match region {
            CloudRegion::CN => "http://115.190.217.85",
            // TODO: deploy separate overseas endpoint
            CloudRegion::Overseas => "http://115.190.217.85",
        }
    }

    /// Pick a default region based on system locale.
    pub fn default_region() -> CloudRegion {
        // Check if the system locale hints at a Chinese environment.
        // On Windows: GetUserDefaultLocaleName, on other platforms: LANG env.
        let locale = std::env::var("LANG")
            .or_else(|_| std::env::var("LC_ALL"))
            .unwrap_or_default()
            .to_lowercase();

        if locale.starts_with("zh") || locale.contains("cn") {
            CloudRegion::CN
        } else {
            // For now both point to the same server, so default CN is fine.
            CloudRegion::CN
        }
    }
}
