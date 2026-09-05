use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Settings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub projects_dir: Option<String>,
    /// Names of WSL distributions whose `~/.claude/projects` should also be
    /// scanned for sessions (Windows host discovering sessions inside WSL).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub wsl_distros: Vec<String>,
    /// Extra CORS origins allowed to call the local HTTP API, configured via
    /// the Settings UI. Unioned at request time with the built-in defaults
    /// and the `CCTRACE_ALLOWED_ORIGINS` env var — see `http_api::build_cors`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowed_origins: Vec<String>,
}

/// Env var that relocates the whole per-user config directory (`settings.json`,
/// `api-token`). Used by the e2e suite so a test run never touches a
/// developer's real files; also handy for containers and multi-profile setups.
pub const ENV_CONFIG_DIR: &str = "CCTRACE_CONFIG_DIR";

/// The app's config directory: `$CCTRACE_CONFIG_DIR` when set and non-empty,
/// else `dirs::config_dir()/claude-code-trace`.
pub fn config_root() -> Option<PathBuf> {
    config_root_from(std::env::var(ENV_CONFIG_DIR).ok(), dirs::config_dir())
}

/// Pure core of [`config_root`] for tests.
pub fn config_root_from(
    override_dir: Option<String>,
    os_config: Option<PathBuf>,
) -> Option<PathBuf> {
    match override_dir
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
    {
        Some(dir) => Some(PathBuf::from(dir)),
        None => os_config.map(|c| c.join("claude-code-trace")),
    }
}

fn settings_path() -> Result<PathBuf, String> {
    let root = config_root().ok_or("no config directory")?;
    Ok(root.join("settings.json"))
}

pub fn load_settings() -> Settings {
    settings_path()
        .ok()
        .and_then(|p| fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save_settings(settings: &Settings) -> Result<(), String> {
    let path = settings_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    fs::write(&path, json).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn config_root_prefers_env_override() {
        assert_eq!(
            config_root_from(Some(" /custom/cfg ".into()), Some(PathBuf::from("/os"))),
            Some(PathBuf::from("/custom/cfg"))
        );
    }

    #[test]
    fn config_root_falls_back_to_os_config_dir_subfolder() {
        assert_eq!(
            config_root_from(None, Some(PathBuf::from("/os"))),
            Some(PathBuf::from("/os/claude-code-trace"))
        );
        assert_eq!(
            config_root_from(Some("   ".into()), Some(PathBuf::from("/os"))),
            Some(PathBuf::from("/os/claude-code-trace"))
        );
        assert_eq!(config_root_from(None, None), None);
    }

    #[test]
    fn default_settings_has_no_projects_dir() {
        let settings = Settings::default();
        assert!(settings.projects_dir.is_none());
    }

    #[test]
    fn save_and_load_roundtrip() {
        let tmp = env::temp_dir().join("tail-test-settings-roundtrip");
        fs::create_dir_all(&tmp).unwrap();
        let path = tmp.join("settings.json");

        let settings = Settings {
            projects_dir: Some("/custom/path".to_string()),
            ..Default::default()
        };
        let json = serde_json::to_string_pretty(&settings).unwrap();
        fs::write(&path, &json).unwrap();

        let loaded: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.projects_dir, Some("/custom/path".to_string()));

        fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn deserialize_empty_json_gives_defaults() {
        let settings: Settings = serde_json::from_str("{}").unwrap();
        assert!(settings.projects_dir.is_none());
        assert!(settings.wsl_distros.is_empty());
        assert!(settings.allowed_origins.is_empty());
    }

    #[test]
    fn wsl_distros_roundtrip() {
        let settings = Settings {
            wsl_distros: vec!["Ubuntu".to_string(), "Debian".to_string()],
            ..Default::default()
        };
        let json = serde_json::to_string_pretty(&settings).unwrap();
        let loaded: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.wsl_distros, vec!["Ubuntu", "Debian"]);
    }

    #[test]
    fn empty_wsl_distros_omitted_from_json() {
        let settings = Settings::default();
        let json = serde_json::to_string(&settings).unwrap();
        assert!(!json.contains("wsl_distros"));
    }

    #[test]
    fn allowed_origins_roundtrip() {
        let settings = Settings {
            allowed_origins: vec![
                "http://a.example".to_string(),
                "http://b.example".to_string(),
            ],
            ..Default::default()
        };
        let json = serde_json::to_string_pretty(&settings).unwrap();
        let loaded: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(
            loaded.allowed_origins,
            vec!["http://a.example", "http://b.example"]
        );
    }

    #[test]
    fn empty_allowed_origins_omitted_from_json() {
        let settings = Settings::default();
        let json = serde_json::to_string(&settings).unwrap();
        assert!(!json.contains("allowed_origins"));
    }
}
