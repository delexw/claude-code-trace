//! Session-source plugins. Each source of sessions lives in its own folder
//! under `plugins/` and registers here; see `specs/15-session-source-plugins.md`.

pub mod antigravity;
pub mod claude;
pub mod config;

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

pub static PLUGINS: &[&dyn SourcePlugin] =
    &[&claude::ClaudePlugin, &antigravity::AntigravityPlugin];

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

fn descriptor(id: &str) -> Option<PluginDescriptor> {
    PLUGINS
        .iter()
        .map(|plugin| plugin.descriptor())
        .find(|descriptor| descriptor.id == id)
}

/// Whether plugin `id` is on: always for a plugin that cannot be disabled,
/// else the user's stored choice, else the plugin's default.
pub fn is_enabled(settings: &Settings, id: &str) -> bool {
    descriptor(id).is_some_and(|descriptor| {
        !descriptor.can_disable
            || settings
                .plugins
                .get(id)
                .and_then(|config| config.enabled)
                .unwrap_or(descriptor.enabled_by_default)
    })
}

/// Record the user's choice to turn plugin `id` on or off.
pub fn set_enabled(settings: &mut Settings, id: &str, enabled: bool) -> Result<(), String> {
    let descriptor = descriptor(id).ok_or_else(|| format!("unknown plugin: {id}"))?;
    if !descriptor.can_disable {
        return if enabled {
            Ok(())
        } else {
            Err(format!("{} cannot be turned off", descriptor.label))
        };
    }
    settings.plugins.entry(id.to_owned()).or_default().enabled = Some(enabled);
    Ok(())
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
    fn antigravity_is_registered_off_by_default() {
        let antigravity = descriptor("antigravity").expect("antigravity registered");
        assert!(antigravity.can_disable);
        assert!(!is_enabled(&Settings::default(), "antigravity"));
    }

    #[test]
    fn statuses_honour_the_stored_choice() {
        let mut settings = Settings::default();
        set_enabled(&mut settings, "antigravity", true).unwrap();
        let on = statuses(&settings);
        assert!(on.iter().find(|s| s.id == "antigravity").unwrap().enabled);

        set_enabled(&mut settings, "antigravity", false).unwrap();
        let off = statuses(&settings);
        assert!(!off.iter().find(|s| s.id == "antigravity").unwrap().enabled);
    }

    #[test]
    fn set_enabled_rejects_unknown_plugins() {
        let mut settings = Settings::default();
        assert!(set_enabled(&mut settings, "no-such-plugin", true).is_err());
        assert!(settings.plugins.is_empty());
    }

    #[test]
    fn claude_cannot_be_turned_off() {
        let mut settings = Settings::default();
        assert!(set_enabled(&mut settings, "claude", false).is_err());
        assert!(set_enabled(&mut settings, "claude", true).is_ok());
        assert!(is_enabled(&settings, "claude"));
        assert!(settings.plugins.is_empty());
    }

    #[test]
    fn a_stored_flag_cannot_turn_claude_off() {
        let settings: Settings =
            serde_json::from_str(r#"{"plugins":{"claude":{"enabled":false}}}"#).unwrap();
        assert!(is_enabled(&settings, "claude"));
    }

    #[test]
    fn the_choice_survives_a_save_and_load() {
        let mut settings = Settings::default();
        set_enabled(&mut settings, "antigravity", true).unwrap();
        let json = serde_json::to_string(&settings).unwrap();
        let loaded: Settings = serde_json::from_str(&json).unwrap();
        assert!(is_enabled(&loaded, "antigravity"));
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
