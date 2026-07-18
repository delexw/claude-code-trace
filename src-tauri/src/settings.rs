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

fn settings_path() -> Result<PathBuf, String> {
    let config = dirs::config_dir().ok_or("no config directory")?;
    Ok(config.join("claude-code-trace").join("settings.json"))
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
