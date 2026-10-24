//! Session-source plugins. Each source of sessions lives in its own folder
//! under `plugins/` and registers here; see `specs/15-session-source-plugins.md`.

pub mod claude;

use std::sync::Arc;

use axum::Router;
use serde::Serialize;

use crate::http_api::HttpState;
use crate::settings::Settings;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PluginDescriptor {
    pub id: &'static str,
    pub label: &'static str,
    /// false for a plugin the app cannot run without.
    pub can_disable: bool,
    pub enabled_by_default: bool,
}

pub trait SourcePlugin: Sync {
    fn descriptor(&self) -> PluginDescriptor;
    /// The plugin's HTTP routes; merged into the API router behind the auth layer.
    fn http_routes(&self) -> Router<Arc<HttpState>>;
}

pub static PLUGINS: &[&dyn SourcePlugin] = &[&claude::ClaudePlugin];

/// What the frontends need to know about each plugin, in registry order.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PluginStatus {
    pub id: &'static str,
    pub label: &'static str,
    pub enabled: bool,
    pub can_disable: bool,
}

/// Every plugin's HTTP routes merged into one router.
pub fn http_routes() -> Router<Arc<HttpState>> {
    PLUGINS.iter().fold(Router::new(), |router, plugin| {
        router.merge(plugin.http_routes())
    })
}

pub fn is_enabled(_settings: &Settings, id: &str) -> bool {
    PLUGINS
        .iter()
        .map(|plugin| plugin.descriptor())
        .find(|descriptor| descriptor.id == id)
        .is_some_and(|descriptor| !descriptor.can_disable || descriptor.enabled_by_default)
}

pub fn statuses(settings: &Settings) -> Vec<PluginStatus> {
    PLUGINS
        .iter()
        .map(|plugin| {
            let descriptor = plugin.descriptor();
            PluginStatus {
                id: descriptor.id,
                label: descriptor.label,
                enabled: is_enabled(settings, descriptor.id),
                can_disable: descriptor.can_disable,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn plugin_ids_are_unique() {
        let ids: Vec<_> = PLUGINS.iter().map(|p| p.descriptor().id).collect();
        let unique: HashSet<_> = ids.iter().collect();
        assert_eq!(ids.len(), unique.len(), "duplicate plugin id in {ids:?}");
    }

    #[test]
    fn claude_is_registered_and_cannot_be_disabled() {
        let claude = PLUGINS
            .iter()
            .map(|p| p.descriptor())
            .find(|d| d.id == "claude")
            .expect("claude plugin registered");
        assert!(!claude.can_disable);
        assert!(is_enabled(&Settings::default(), "claude"));
    }

    #[test]
    fn unknown_plugin_is_not_enabled() {
        assert!(!is_enabled(&Settings::default(), "no-such-plugin"));
    }

    #[test]
    fn statuses_follow_registry_order() {
        let statuses = statuses(&Settings::default());
        let ids: Vec<_> = statuses.iter().map(|s| s.id).collect();
        let registry: Vec<_> = PLUGINS.iter().map(|p| p.descriptor().id).collect();
        assert_eq!(ids, registry);
        assert_eq!(
            statuses[0],
            PluginStatus {
                id: "claude",
                label: "Claude Code",
                enabled: true,
                can_disable: false,
            }
        );
    }

    #[test]
    fn status_serializes_with_the_frontend_field_names() {
        let json = serde_json::to_value(&statuses(&Settings::default())[0]).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "id": "claude",
                "label": "Claude Code",
                "enabled": true,
                "can_disable": false,
            })
        );
    }
}
