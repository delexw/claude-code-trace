//! Each plugin's section of `settings.json`: `plugins.<id>`, holding whether
//! the plugin is on plus whatever keys the plugin itself owns.

use std::collections::BTreeMap;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize};

use crate::settings::Settings;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PluginConfig {
    /// `None` until the user turns the plugin on or off; the plugin's
    /// `enabled_by_default` applies meanwhile.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    /// The plugin's own keys, kept verbatim so a key this build does not know
    /// (written by a newer version) survives a save.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

/// Reads `plugins` without ever failing the whole settings file. An explicit
/// `null`, or a plugin entry that is not an object, reads as empty: one bad
/// value must not reset every other setting to its default.
pub fn deserialize_plugin_configs<'de, D>(
    deserializer: D,
) -> Result<BTreeMap<String, PluginConfig>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Option::<serde_json::Value>::deserialize(deserializer)?;
    let Some(serde_json::Value::Object(entries)) = value else {
        return Ok(BTreeMap::new());
    };
    Ok(entries
        .into_iter()
        .map(|(id, entry)| (id, plugin_config_from(entry)))
        .collect())
}

fn plugin_config_from(entry: serde_json::Value) -> PluginConfig {
    let serde_json::Value::Object(mut fields) = entry else {
        return PluginConfig::default();
    };
    let enabled = fields.remove("enabled").and_then(|v| v.as_bool());
    PluginConfig {
        enabled,
        extra: fields,
    }
}

/// A plugin's own keys read as `T`; `T::default()` when the section is
/// missing or does not match.
pub fn plugin_config<T: DeserializeOwned + Default>(settings: &Settings, id: &str) -> T {
    settings
        .plugins
        .get(id)
        .and_then(|config| {
            serde_json::from_value(serde_json::Value::Object(config.extra.clone())).ok()
        })
        .unwrap_or_default()
}

/// Set (or, with `None`, remove) one of a plugin's own keys.
pub fn set_plugin_key(
    settings: &mut Settings,
    id: &str,
    key: &str,
    value: Option<serde_json::Value>,
) {
    let config = settings.plugins.entry(id.to_owned()).or_default();
    match value {
        Some(value) => {
            config.extra.insert(key.to_owned(), value);
        }
        None => {
            config.extra.remove(key);
        }
    }
    if *config == PluginConfig::default() {
        settings.plugins.remove(id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(json: &str) -> Settings {
        serde_json::from_str(json).expect("settings parse")
    }

    #[test]
    fn missing_plugins_section_is_empty() {
        assert!(parse("{}").plugins.is_empty());
    }

    #[test]
    fn explicit_null_does_not_fail_the_settings_file() {
        let settings = parse(r#"{"projects_dir":"/p","plugins":null}"#);
        assert!(settings.plugins.is_empty());
        assert_eq!(settings.projects_dir.as_deref(), Some("/p"));
    }

    #[test]
    fn bad_plugin_entries_read_as_empty_and_keep_other_settings() {
        let settings =
            parse(r#"{"projects_dir":"/p","plugins":{"a":null,"b":"nope","c":{"enabled":"yes"}}}"#);
        assert_eq!(settings.projects_dir.as_deref(), Some("/p"));
        assert_eq!(settings.plugins["a"], PluginConfig::default());
        assert_eq!(settings.plugins["b"], PluginConfig::default());
        assert_eq!(settings.plugins["c"].enabled, None);
    }

    #[test]
    fn unknown_plugin_keys_survive_a_round_trip() {
        let settings = parse(r#"{"plugins":{"x":{"enabled":true,"root":"/r","future":[1,2]}}}"#);
        assert_eq!(settings.plugins["x"].enabled, Some(true));
        let json = serde_json::to_value(&settings).unwrap();
        assert_eq!(
            json["plugins"]["x"],
            serde_json::json!({"enabled": true, "root": "/r", "future": [1, 2]})
        );
    }

    #[test]
    fn empty_plugins_section_is_not_written() {
        let json = serde_json::to_string(&Settings::default()).unwrap();
        assert!(!json.contains("plugins"));
    }

    #[derive(Deserialize, Default, Debug, PartialEq)]
    struct Own {
        #[serde(default)]
        root: Option<String>,
    }

    #[test]
    fn plugin_reads_and_writes_its_own_keys() {
        let mut settings = Settings::default();
        assert_eq!(plugin_config::<Own>(&settings, "x"), Own::default());

        set_plugin_key(&mut settings, "x", "root", Some("/r".into()));
        assert_eq!(
            plugin_config::<Own>(&settings, "x").root.as_deref(),
            Some("/r")
        );

        set_plugin_key(&mut settings, "x", "root", None);
        assert!(
            settings.plugins.is_empty(),
            "an emptied section is dropped rather than written as {{}}"
        );
    }

    #[test]
    fn removing_a_key_keeps_the_enabled_flag() {
        let mut settings = parse(r#"{"plugins":{"x":{"enabled":true,"root":"/r"}}}"#);
        set_plugin_key(&mut settings, "x", "root", None);
        assert_eq!(settings.plugins["x"].enabled, Some(true));
    }
}
