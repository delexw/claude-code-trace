//! Read-only Codex rollout source. No files are written under a Codex home.

#[cfg(feature = "desktop")]
pub mod commands;
mod costs;
mod http;
pub mod reader;
pub mod roots;

use std::sync::Arc;

use axum::Router;
use serde::{Deserialize, Serialize};

use super::config::{plugin_config, set_plugin_key};
use super::{PluginDescriptor, SourcePlugin};
use crate::http_api::HttpState;
use crate::settings::Settings;

pub const ID: &str = "codex";
const DISABLED: &str = "Codex source is disabled";
pub struct CodexPlugin;

impl SourcePlugin for CodexPlugin {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor {
            id: ID,
            label: "Codex",
            can_disable: true,
            enabled_by_default: false,
        }
    }

    fn http_routes(&self) -> Router<Arc<HttpState>> {
        http::routes()
    }
}

#[derive(Default, Deserialize)]
struct CodexConfig {
    root: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CodexSettings {
    pub root: Option<String>,
    pub roots: Vec<roots::CodexRoot>,
}

pub fn settings_view(settings: &Settings) -> CodexSettings {
    let config: CodexConfig = plugin_config(settings, ID);
    CodexSettings {
        roots: roots::resolve_roots(config.root.as_deref()),
        root: config.root,
    }
}

pub fn set_root(settings: &mut Settings, path: Option<String>) -> Result<(), String> {
    let path = path.map(|p| p.trim().to_owned()).filter(|p| !p.is_empty());
    if let Some(path) = &path {
        if !std::path::Path::new(path).is_dir() {
            return Err(format!("path does not exist or is not a directory: {path}"));
        }
    }
    set_plugin_key(settings, ID, "root", path.map(serde_json::Value::String));
    Ok(())
}

pub fn visible_sessions(settings: &Settings) -> Vec<reader::CodexSession> {
    if !super::is_enabled(settings, ID) {
        return Vec::new();
    }
    reader::discover_sessions(&settings_view(settings).roots)
}

/// Every read of a rollout by path goes through here, so the HTTP routes and
/// the desktop commands share one gate: the source must be on, and the path
/// must be a rollout inside a configured Codex home (see
/// [`roots::authorize_rollout`]). Returns the resolved real path.
fn authorized_rollout(settings: &Settings, path: &str) -> Result<std::path::PathBuf, String> {
    if !super::is_enabled(settings, ID) {
        return Err(DISABLED.into());
    }
    roots::authorize_rollout(&settings_view(settings).roots, path)
}

pub fn load_session(
    settings: &Settings,
    path: &str,
    start: Option<usize>,
    limit: Option<usize>,
) -> Result<reader::CodexWindow, String> {
    let path = authorized_rollout(settings, path)?;
    reader::load_window(&path, start.unwrap_or(0), limit)
}

pub fn prepare_efficiency_payload(
    settings: &Settings,
    path: &str,
    mode: Option<crate::efficiency::settings::PayloadMode>,
) -> Result<crate::efficiency::PreparedEfficiencyPayload, String> {
    let window = load_session(settings, path, None, None)?;
    crate::commands::efficiency::prepare_display_efficiency_payload(
        path,
        mode.unwrap_or_else(|| crate::efficiency::settings::load().default_payload_mode),
        &window.messages,
        window.total_tokens,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROLLOUT: &str = r#"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"Invent a small demo"}]}}"#;

    /// Settings with Codex on and `home` as its only Codex home.
    fn enabled_settings(home: &std::path::Path) -> Settings {
        let mut settings = Settings::default();
        crate::plugins::set_enabled(&mut settings, ID, true).unwrap();
        set_root(&mut settings, Some(home.to_string_lossy().into_owned())).unwrap();
        settings
    }

    fn home_with_rollout() -> (tempfile::TempDir, String) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir
            .path()
            .join("sessions/2026/01/01/rollout-fabricated.jsonl");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, ROLLOUT).unwrap();
        let path = path.to_string_lossy().into_owned();
        (dir, path)
    }

    #[test]
    fn a_rollout_inside_a_configured_home_loads() {
        let (home, path) = home_with_rollout();
        let settings = enabled_settings(home.path());
        let window = load_session(&settings, &path, Some(0), Some(1)).unwrap();
        assert_eq!(window.count, 1);
        assert_eq!(window.messages[0].content, "Invent a small demo");
    }

    #[test]
    fn load_session_refuses_files_outside_a_configured_home() {
        let (home, _) = home_with_rollout();
        let settings = enabled_settings(home.path());
        let outside = tempfile::tempdir().unwrap();
        let stray = outside.path().join("rollout-fabricated.jsonl");
        std::fs::write(&stray, ROLLOUT).unwrap();
        let notes = home.path().join("session_index.jsonl");
        std::fs::write(&notes, ROLLOUT).unwrap();
        for path in [stray, notes] {
            let error = load_session(&settings, &path.to_string_lossy(), None, None).unwrap_err();
            assert!(error.contains("configured Codex home"), "{error}");
        }
    }

    #[test]
    fn efficiency_payload_refuses_files_outside_a_configured_home() {
        let (home, _) = home_with_rollout();
        let settings = enabled_settings(home.path());
        let outside = tempfile::tempdir().unwrap();
        let stray = outside.path().join("rollout-fabricated.jsonl");
        std::fs::write(&stray, ROLLOUT).unwrap();
        let error =
            prepare_efficiency_payload(&settings, &stray.to_string_lossy(), None).unwrap_err();
        assert!(error.contains("configured Codex home"), "{error}");
    }

    #[test]
    fn a_disabled_source_refuses_every_read_route() {
        let (home, path) = home_with_rollout();
        let mut settings = enabled_settings(home.path());
        crate::plugins::set_enabled(&mut settings, ID, false).unwrap();
        assert_eq!(
            load_session(&settings, &path, None, None).unwrap_err(),
            "Codex source is disabled"
        );
        assert_eq!(
            prepare_efficiency_payload(&settings, &path, None).unwrap_err(),
            "Codex source is disabled"
        );
        assert!(visible_sessions(&settings).is_empty());
        // Turning it back on makes the same chat readable again.
        crate::plugins::set_enabled(&mut settings, ID, true).unwrap();
        assert!(load_session(&settings, &path, None, None).is_ok());
    }

    #[test]
    fn codex_is_off_until_the_user_turns_it_on() {
        let (home, path) = home_with_rollout();
        let mut settings = Settings::default();
        set_root(
            &mut settings,
            Some(home.path().to_string_lossy().into_owned()),
        )
        .unwrap();
        assert_eq!(
            load_session(&settings, &path, None, None).unwrap_err(),
            "Codex source is disabled"
        );
    }
}
