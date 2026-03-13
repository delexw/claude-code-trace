use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Settings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub projects_dir: Option<String>,
}

fn settings_path() -> Result<PathBuf, String> {
    let config = dirs::config_dir().ok_or("no config directory")?;
    Ok(config.join("tail-claude-gui").join("settings.json"))
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
    }
}
