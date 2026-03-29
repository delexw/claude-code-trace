use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;

/// Entry represents a raw JSONL line from a Claude Code session file.
#[derive(Debug, Deserialize, Default)]
pub struct Entry {
    #[serde(default, rename = "type")]
    pub entry_type: String,
    #[serde(default)]
    pub uuid: String,
    #[serde(default)]
    pub timestamp: String,
    #[serde(default, rename = "isSidechain")]
    pub is_sidechain: bool,
    #[serde(default, rename = "isMeta")]
    pub is_meta: bool,
    #[serde(default)]
    pub message: EntryMessage,
    #[serde(default)]
    pub cwd: String,
    #[serde(default, rename = "gitBranch")]
    pub git_branch: String,
    #[serde(default, rename = "permissionMode")]
    pub permission_mode: String,
    #[serde(default, rename = "toolUseResult")]
    pub tool_use_result: Option<Value>,
    #[serde(default, rename = "sourceToolUseID")]
    pub source_tool_use_id: String,
    #[serde(default, rename = "leafUuid")]
    pub leaf_uuid: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default, rename = "requestId")]
    pub request_id: String,
    #[serde(default, rename = "teamName")]
    pub team_name: String,
    #[serde(default, rename = "agentName")]
    pub agent_name: String,
    #[serde(default)]
    pub data: Option<Value>,
    // Top-level fields present in system/hook_progress entries (verbose/stream-json mode).
    #[serde(default)]
    pub subtype: String,
    #[serde(default, rename = "hookEvent")]
    pub hook_event: String,
    #[serde(default, rename = "hookName")]
    pub hook_name: String,
}

#[derive(Debug, Deserialize, Default)]
pub struct EntryMessage {
    #[serde(default)]
    pub role: String,
    #[serde(default)]
    pub content: Option<Value>,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub stop_reason: Option<String>,
    #[serde(default)]
    pub usage: EntryUsage,
}

#[derive(Debug, Deserialize, Default)]
pub struct EntryUsage {
    #[serde(default)]
    pub input_tokens: i64,
    #[serde(default)]
    pub output_tokens: i64,
    #[serde(default)]
    pub cache_read_input_tokens: i64,
    #[serde(default)]
    pub cache_creation_input_tokens: i64,
}

impl Entry {
    /// Parse tool_use_result as a JSON object (map). Returns None if absent/non-object.
    pub fn tool_use_result_map(&self) -> Option<HashMap<String, Value>> {
        let val = self.tool_use_result.as_ref()?;
        match val {
            Value::Object(map) => {
                let mut result = HashMap::new();
                for (k, v) in map {
                    result.insert(k.clone(), v.clone());
                }
                Some(result)
            }
            _ => None,
        }
    }
}

/// Parse a single JSONL line into an Entry.
/// Returns None if the JSON is invalid or the entry has no UUID.
pub fn parse_entry(line: &[u8]) -> Option<Entry> {
    let e: Entry = serde_json::from_slice(line).ok()?;
    if e.uuid.is_empty() && e.leaf_uuid.is_empty() {
        return None;
    }
    Some(e)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // --- parse_entry tests ---

    #[test]
    fn parse_entry_valid_json_returns_entry() {
        let line = json!({
            "type": "user",
            "uuid": "abc-123",
            "timestamp": "2025-01-15T10:30:00Z",
            "message": {"role": "user", "content": "hello"}
        });
        let bytes = serde_json::to_vec(&line).unwrap();
        let entry = parse_entry(&bytes);
        assert!(entry.is_some());
        let e = entry.unwrap();
        assert_eq!(e.entry_type, "user");
        assert_eq!(e.uuid, "abc-123");
    }

    #[test]
    fn parse_entry_invalid_json_returns_none() {
        let bytes = b"not valid json {{{";
        assert!(parse_entry(bytes).is_none());
    }

    #[test]
    fn parse_entry_without_uuid_or_leaf_uuid_returns_none() {
        let line = json!({
            "type": "user",
            "timestamp": "2025-01-15T10:30:00Z",
            "message": {"role": "user", "content": "hello"}
        });
        let bytes = serde_json::to_vec(&line).unwrap();
        assert!(parse_entry(&bytes).is_none());
    }

    #[test]
    fn parse_entry_with_leaf_uuid_only_returns_some() {
        let line = json!({
            "type": "user",
            "leafUuid": "leaf-456",
            "timestamp": "2025-01-15T10:30:00Z",
            "message": {"role": "user"}
        });
        let bytes = serde_json::to_vec(&line).unwrap();
        let entry = parse_entry(&bytes);
        assert!(entry.is_some());
        assert_eq!(entry.unwrap().leaf_uuid, "leaf-456");
    }

    // --- tool_use_result_map tests ---

    #[test]
    fn tool_use_result_map_returns_some_for_objects() {
        let e = Entry {
            tool_use_result: Some(json!({"key": "value", "count": 42})),
            ..Default::default()
        };
        let map = e.tool_use_result_map();
        assert!(map.is_some());
        let m = map.unwrap();
        assert_eq!(m.get("key").and_then(|v| v.as_str()), Some("value"));
        assert_eq!(m.get("count").and_then(|v| v.as_i64()), Some(42));
    }

    #[test]
    fn tool_use_result_map_returns_none_for_non_objects() {
        let e = Entry {
            tool_use_result: Some(json!("just a string")),
            ..Default::default()
        };
        assert!(e.tool_use_result_map().is_none());

        let e2 = Entry {
            tool_use_result: Some(json!([1, 2, 3])),
            ..Default::default()
        };
        assert!(e2.tool_use_result_map().is_none());
    }

    #[test]
    fn tool_use_result_map_returns_none_for_none() {
        let e = Entry {
            tool_use_result: None,
            ..Default::default()
        };
        assert!(e.tool_use_result_map().is_none());
    }
}
