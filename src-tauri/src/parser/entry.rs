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
