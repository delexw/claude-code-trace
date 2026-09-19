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
    pub subscription_providers_available: bool,
}

pub fn subscription_providers_available_for(runtime: Option<&str>) -> bool {
    runtime != Some("docker")
}

pub fn subscription_providers_available() -> bool {
    subscription_providers_available_for(std::env::var("CCTRACE_RUNTIME").ok().as_deref())
}

fn credential_source(
    token: crate::credentials::api_tokens::ApiToken,
) -> Result<Option<&'static str>, String> {
    match crate::credentials::api_tokens::source(token) {
        Err(_) if !subscription_providers_available() => Ok(None),
        result => result,
    }
}

pub fn ensure_provider_supported(provider: &RecommendationProvider) -> Result<(), String> {
    if subscription_providers_available()
        || matches!(provider, RecommendationProvider::OpenaiCompatible { .. })
    {
        Ok(())
    } else {
        Err(
            "Codex and Claude Code subscription providers are unavailable in Docker. Use an OpenAI-compatible endpoint instead."
                .to_string(),
        )
    }
}

pub fn ensure_web_provider_supported(provider: &RecommendationProvider) -> Result<(), String> {
    let RecommendationProvider::OpenaiCompatible { base_url, .. } = provider else {
        return Ok(());
    };
    let url = reqwest::Url::parse(base_url).map_err(|_| {
        "Web mode requires a valid local OpenAI-compatible endpoint URL".to_string()
    })?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err("Web mode requires an HTTP(S) OpenAI-compatible endpoint".to_string());
    }
    if !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(
            "Web mode does not allow credentials or tokens in the provider URL".to_string(),
        );
    }
    let is_loopback = url.host_str().is_some_and(|host| {
        host.eq_ignore_ascii_case("localhost")
            || host
                .trim_matches(['[', ']'])
                .parse::<std::net::IpAddr>()
                .is_ok_and(|address| address.is_loopback())
    });
    if !is_loopback {
        return Err(
            "Web mode supports only a local, unauthenticated OpenAI-compatible endpoint"
                .to_string(),
        );
    }
    Ok(())
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
    let source = credential_source(crate::credentials::api_tokens::ApiToken::Jev)?;
    if let RecommendationProvider::OpenaiCompatible {
        api_key_configured, ..
    } = &mut configuration.recommendation_provider
    {
        *api_key_configured =
            credential_source(crate::credentials::api_tokens::ApiToken::RecommendationProvider)?
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
        subscription_providers_available: subscription_providers_available(),
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

    #[test]
    fn docker_runtime_disables_subscription_providers() {
        assert!(!subscription_providers_available_for(Some("docker")));
        assert!(subscription_providers_available_for(None));
        assert!(subscription_providers_available_for(Some("native")));
    }

    #[test]
    fn web_provider_accepts_only_local_urls_without_embedded_credentials() {
        for base_url in [
            "http://localhost:1234/v1",
            "http://127.0.0.1:1234/v1",
            "http://127.1.2.3:1234/v1",
            "http://[::1]:1234/v1",
        ] {
            assert!(
                ensure_web_provider_supported(&RecommendationProvider::OpenaiCompatible {
                    base_url: base_url.into(),
                    model: "local-model".into(),
                    api_key_configured: false,
                })
                .is_ok()
            );
        }

        for base_url in [
            "https://api.example.com/v1",
            "http://token@localhost:1234/v1",
            "http://localhost:1234/v1?api_key=secret",
        ] {
            assert!(
                ensure_web_provider_supported(&RecommendationProvider::OpenaiCompatible {
                    base_url: base_url.into(),
                    model: "local-model".into(),
                    api_key_configured: false,
                })
                .is_err()
            );
        }
    }
}
