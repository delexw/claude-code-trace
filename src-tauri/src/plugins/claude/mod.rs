//! The Claude Code session source: discovers, loads and watches the JSONL
//! transcripts under `~/.claude/projects` (and WSL distros).

pub mod commands;
mod http;

use std::sync::Arc;

use axum::Router;

use super::{PluginDescriptor, SourcePlugin};
use crate::http_api::HttpState;

pub struct ClaudePlugin;

impl SourcePlugin for ClaudePlugin {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor {
            id: "claude",
            label: "Claude Code",
            can_disable: false,
            enabled_by_default: true,
        }
    }

    fn http_routes(&self) -> Router<Arc<HttpState>> {
        http::routes()
    }
}
