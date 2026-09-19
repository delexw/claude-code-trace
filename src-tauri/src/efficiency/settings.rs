use serde::{Deserialize, Serialize};
use std::fs;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum PayloadMode {
    #[default]
    Minimized,
    FullTranscript,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum RecommendationProvider {
    #[serde(rename_all = "camelCase")]
    CodexSubscription { model: Option<String> },
    #[serde(rename_all = "camelCase")]
    ClaudeCodeSubscription { model: Option<String> },
    #[serde(rename_all = "camelCase")]
    OpenaiCompatible {
        base_url: String,
        model: String,
        api_key_configured: bool,
    },
}

impl Default for RecommendationProvider {
    fn default() -> Self {
        Self::CodexSubscription { model: None }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AnalyticsConfiguration {
    #[serde(default)]
    pub default_payload_mode: PayloadMode,
    #[serde(default)]
    pub recommendation_provider: RecommendationProvider,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JevSettingsStatus {
    pub configured: bool,
    pub source: Option<String>,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalyticsSettingsResponse {
    pub jev: JevSettingsStatus,
    pub default_payload_mode: PayloadMode,
    pub recommendation_provider: RecommendationProvider,
}

fn path() -> Result<std::path::PathBuf, String> {
    crate::settings::config_root()
        .map(|root| root.join("analytics.json"))
        .ok_or_else(|| "no config directory".to_string())
}

pub fn load() -> AnalyticsConfiguration {
    path()
        .ok()
        .and_then(|path| fs::read_to_string(path).ok())
        .and_then(|contents| serde_json::from_str(&contents).ok())
        .unwrap_or_default()
}

pub fn save(configuration: &AnalyticsConfiguration) -> Result<(), String> {
    let path = path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let mut persisted = configuration.clone();
    if let RecommendationProvider::OpenaiCompatible {
        api_key_configured, ..
    } = &mut persisted.recommendation_provider
    {
        *api_key_configured = false;
    }
    let contents = serde_json::to_string_pretty(&persisted).map_err(|error| error.to_string())?;
    fs::write(path, contents).map_err(|error| error.to_string())
}

pub fn response() -> Result<AnalyticsSettingsResponse, String> {
    let mut configuration = load();
    let source =
        crate::credentials::api_tokens::source(crate::credentials::api_tokens::ApiToken::Jev)?;
    if let RecommendationProvider::OpenaiCompatible {
        api_key_configured, ..
    } = &mut configuration.recommendation_provider
    {
        *api_key_configured = crate::credentials::api_tokens::source(
            crate::credentials::api_tokens::ApiToken::RecommendationProvider,
        )?
        .is_some();
    }
    Ok(AnalyticsSettingsResponse {
        jev: JevSettingsStatus {
            configured: source.is_some(),
            source: source.map(str::to_string),
            status: if source.is_some() {
                "configured"
            } else {
                "not_configured"
            }
            .to_string(),
        },
        default_payload_mode: configuration.default_payload_mode,
        recommendation_provider: configuration.recommendation_provider,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_minimized_and_codex_subscription() {
        let settings = AnalyticsConfiguration::default();
        assert_eq!(settings.default_payload_mode, PayloadMode::Minimized);
        assert!(matches!(
            settings.recommendation_provider,
            RecommendationProvider::CodexSubscription { .. }
        ));
    }

    #[test]
    fn serialized_provider_never_contains_a_secret_field() {
        let json = serde_json::to_string(&RecommendationProvider::OpenaiCompatible {
            base_url: "http://localhost:1234/v1".into(),
            model: "local-model".into(),
            api_key_configured: true,
        })
        .unwrap();
        assert!(!json.to_lowercase().contains("apikey\""));
        assert!(!json.contains("secret"));
    }
}
