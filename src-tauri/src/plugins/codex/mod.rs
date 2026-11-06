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

pub fn load_session(
    path: &str,
    start: Option<usize>,
    limit: Option<usize>,
) -> Result<reader::CodexWindow, String> {
    reader::load_window(std::path::Path::new(path), start.unwrap_or(0), limit)
}

pub fn prepare_efficiency_payload(
    path: &str,
    mode: Option<crate::efficiency::settings::PayloadMode>,
) -> Result<crate::efficiency::PreparedEfficiencyPayload, String> {
    let window = load_session(path, None, None)?;
    crate::commands::efficiency::prepare_display_efficiency_payload(
        path,
        mode.unwrap_or_else(|| crate::efficiency::settings::load().default_payload_mode),
        &window.messages,
        window.total_tokens,
    )
}
