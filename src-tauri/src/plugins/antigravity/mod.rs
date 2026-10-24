//! The Antigravity (Gemini IDE) session source: a read-only viewer for the
//! "brain" conversations under `~/.gemini/<flavor>/brain/<uuid>/`.
//!
//! Off by default — it reads another tool's conversations, so the user opts
//! in. Nothing is ever written under a brain root; the transcript index is
//! cached in this app's own config dir.

#[cfg(feature = "desktop")]
pub mod commands;
mod http;
#[cfg(test)]
mod reader_tests;
pub mod roots;
pub mod transcript;

use std::path::PathBuf;
use std::sync::Arc;

use axum::Router;
use serde::{Deserialize, Serialize};

use super::config::{plugin_config, set_plugin_key};
use super::{PluginDescriptor, SourcePlugin};
use crate::http_api::HttpState;
use crate::settings::Settings;
use roots::{brain_root_candidates, merge_brain_roots, AntigravityBrainFolder, BrainRoot};
use transcript::{load_antigravity_window, AntigravityWindow};

pub const ID: &str = "antigravity";

pub struct AntigravityPlugin;

impl SourcePlugin for AntigravityPlugin {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor {
            id: ID,
            label: "Antigravity",
            can_disable: true,
            enabled_by_default: false,
        }
    }

    fn http_routes(&self) -> Router<Arc<HttpState>> {
        http::routes()
    }
}

/// This plugin's own keys in `settings.json` (`plugins.antigravity`).
#[derive(Deserialize, Default)]
struct AntigravityConfig {
    /// A brain root the user picked, replacing flavor auto-detection.
    #[serde(default)]
    root: Option<String>,
}

/// What Settings shows for this plugin.
#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
pub struct AntigravitySettings {
    /// The user's override, or `None` while roots are auto-detected.
    pub root: Option<String>,
    /// Every root that is scanned (or would be), in priority order.
    pub roots: Vec<BrainRoot>,
}

/// Every brain root candidate, from the user's override, `$GEMINI_HOME`, or
/// the home directory.
pub fn resolve_roots(settings: &Settings) -> Vec<BrainRoot> {
    let config: AntigravityConfig = plugin_config(settings, ID);
    let os_home = dirs::home_dir().map(|h| h.to_string_lossy().into_owned());
    brain_root_candidates(
        config.root.as_deref(),
        std::env::var(roots::ENV_GEMINI_HOME).ok().as_deref(),
        os_home.as_deref(),
    )
}

/// The conversations to list: every existing root merged, newest first.
/// Empty while the plugin is off.
pub fn visible_folders(settings: &Settings) -> Vec<AntigravityBrainFolder> {
    if !super::is_enabled(settings, ID) {
        return Vec::new();
    }
    merge_brain_roots(&resolve_roots(settings))
}

pub fn settings_view(settings: &Settings) -> AntigravitySettings {
    let config: AntigravityConfig = plugin_config(settings, ID);
    AntigravitySettings {
        root: config.root,
        roots: resolve_roots(settings),
    }
}

/// Point the reader at an explicit brain root, or (`None` / blank) go back to
/// auto-detection. Rejects a path that is not a directory, so a typo surfaces
/// as an error instead of silently emptying the picker.
pub fn set_root(settings: &mut Settings, path: Option<String>) -> Result<(), String> {
    let path = path
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned);
    if let Some(p) = &path {
        if !PathBuf::from(p).is_dir() {
            return Err(format!("path does not exist or is not a directory: {p}"));
        }
    }
    set_plugin_key(settings, ID, "root", path.map(serde_json::Value::String));
    Ok(())
}

/// Read a conversation. Omitting `limit` reads the whole thing in one
/// contiguous pass; `limit: Some(0)` returns the role index with no bodies.
pub fn load_session(
    path: &str,
    start: Option<usize>,
    limit: Option<usize>,
) -> Result<AntigravityWindow, String> {
    let folder = PathBuf::from(path);
    if !folder.is_dir() {
        return Err(format!("path is not a directory: {path}"));
    }
    Ok(load_antigravity_window(
        &folder,
        start.unwrap_or(0),
        limit.unwrap_or(usize::MAX),
    ))
}

#[cfg(test)]
pub(crate) mod test_support {
    use std::fs;
    use std::path::{Path, PathBuf};

    pub fn tmpdir(name: &str) -> PathBuf {
        let tmp = std::env::temp_dir().join(name);
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        tmp
    }

    /// Create `<root>/<uuid>` with a one-line task title.
    pub fn make_folder(root: &Path, uuid: &str, title: &str) -> PathBuf {
        let dir = root.join(uuid);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("task.md"), title).unwrap();
        dir
    }

    /// Create `<root>/<uuid>/.system_generated/logs/transcript.jsonl`.
    pub fn make_transcript(root: &Path, uuid: &str, rows: &str) -> PathBuf {
        let dir = root.join(uuid).join(".system_generated/logs");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("transcript.jsonl"), rows).unwrap();
        root.join(uuid)
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::*;
    use super::*;

    fn enabled_with_root(root: &std::path::Path) -> Settings {
        let mut settings = Settings::default();
        super::super::set_enabled(&mut settings, ID, true).unwrap();
        set_root(&mut settings, Some(root.to_string_lossy().into_owned())).unwrap();
        settings
    }

    #[test]
    fn folders_are_empty_while_the_plugin_is_off() {
        let tmp = tmpdir("cctrace-antig-plugin-off");
        make_folder(
            &tmp,
            "00000000-0000-0000-0000-0000000000f1",
            "a conversation",
        );
        let mut settings = enabled_with_root(&tmp);
        assert_eq!(visible_folders(&settings).len(), 1);

        super::super::set_enabled(&mut settings, ID, false).unwrap();
        assert!(visible_folders(&settings).is_empty());
    }

    #[test]
    fn the_root_override_is_the_only_root_and_is_stored_under_the_plugin() {
        let tmp = tmpdir("cctrace-antig-plugin-root");
        let settings = enabled_with_root(&tmp);
        let view = settings_view(&settings);
        assert_eq!(view.root.as_deref(), Some(tmp.to_string_lossy().as_ref()));
        assert_eq!(view.roots.len(), 1);
        assert_eq!(view.roots[0].flavor, "custom");
        assert!(view.roots[0].exists);

        let json = serde_json::to_value(&settings).unwrap();
        assert_eq!(
            json["plugins"]["antigravity"]["root"],
            tmp.to_string_lossy().as_ref()
        );
    }

    #[test]
    fn set_root_rejects_a_missing_directory_and_clears_on_blank() {
        let tmp = tmpdir("cctrace-antig-plugin-root-bad");
        let mut settings = enabled_with_root(&tmp);
        assert!(set_root(
            &mut settings,
            Some(tmp.join("nope").to_string_lossy().into())
        )
        .is_err());
        assert!(
            settings_view(&settings).root.is_some(),
            "a rejected path changes nothing"
        );

        set_root(&mut settings, Some("   ".into())).unwrap();
        assert!(settings_view(&settings).root.is_none());
        assert!(
            super::super::is_enabled(&settings, ID),
            "clearing the root keeps the plugin on"
        );
    }

    #[test]
    fn load_session_rejects_a_path_that_is_not_a_folder() {
        assert!(load_session("/definitely/not/here", None, None).is_err());
    }

    #[test]
    fn settings_view_serializes_with_the_frontend_field_names() {
        let view = AntigravitySettings {
            root: None,
            roots: vec![BrainRoot {
                flavor: "antigravity-ide".into(),
                path: "/h/.gemini/antigravity-ide/brain".into(),
                exists: false,
            }],
        };
        assert_eq!(
            serde_json::to_value(&view).unwrap(),
            serde_json::json!({
                "root": null,
                "roots": [{"flavor": "antigravity-ide", "path": "/h/.gemini/antigravity-ide/brain", "exists": false}],
            })
        );
    }
}
