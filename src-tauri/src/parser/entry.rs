use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;

/// Deserializes a JSON string field, treating `null` as the type's default
/// value. Serde's `#[serde(default)]` only applies when the field is absent;
/// this helper also handles the `"field": null` case.
fn null_as_default<'de, D, T>(d: D) -> Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Default + Deserialize<'de>,
{
    Ok(Option::<T>::deserialize(d)?.unwrap_or_default())
}

/// Entry represents a raw JSONL line from a Claude Code session file.
#[derive(Debug, Deserialize, Default)]
pub struct Entry {
    #[serde(default, rename = "type")]
    pub entry_type: String,
    #[serde(default)]
    pub uuid: String,
    #[serde(default, rename = "parentUuid", deserialize_with = "null_as_default")]
    pub parent_uuid: String,
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
    // Top-level fields present in system/stop_hook_summary entries.
    #[serde(default, rename = "hookCount")]
    pub hook_count: u32,
    #[serde(default, rename = "hookInfos")]
    pub hook_infos: Option<Value>,
    #[serde(default, rename = "preventedContinuation")]
    pub prevented_continuation: bool,
    // Present in type:"attachment" entries. Hook results for PreToolUse, PostToolUse, etc.
    // are written as attachment entries: {type:"attachment", attachment:{type:"hook_success"|
    // "hook_non_blocking_error"|"hook_blocking_error"|"hook_cancelled", hookEvent, hookName, ...}}
    #[serde(default)]
    pub attachment: Option<Value>,
    // Present in type:"system", subtype:"away_summary" entries (v2.1.108+). Claude Code writes
    // a recap entry when the user returns after being idle; the recap text is at top-level
    // `content`, not inside `message.content`.
    #[serde(default)]
    pub content: String,
    // Present in type:"system", subtype:"compact_boundary" entries. Claude Code writes this to
    // mark where a compaction occurred. parentUuid is null (breaks the chain), but
    // logicalParentUuid points to the last message before compaction so we can follow the
    // chain back and include pre-compaction messages in the conversation view.
    #[serde(
        default,
        rename = "logicalParentUuid",
        deserialize_with = "null_as_default"
    )]
    pub logical_parent_uuid: String,
    // Present in type:"user" entries when Claude Code wrote the AI-generated summary of a
    // compacted conversation. We classify these as CompactMsg instead of regular user messages.
    #[serde(default, rename = "isCompactSummary")]
    pub is_compact_summary: bool,
    // Present in forked session entries (pre-v2.1.118). When /fork branched a conversation,
    // each duplicated parent entry carried forkedFrom:{sessionId,messageUuid} to identify
    // its origin. Entries without this field are newly added in the fork itself.
    #[serde(default, rename = "forkedFrom")]
    pub forked_from: Option<Value>,
    // Present in type:"fork-context-ref" entries (v2.1.118+). The session being forked from.
    #[serde(default, rename = "forkedSessionId")]
    pub forked_session_id: String,
    // Present in type:"fork-context-ref" entries (v2.1.118+). The message uuid in the parent
    // session up to which the fork context should be read.
    #[serde(default, rename = "upToMessageId")]
    pub up_to_message_id: String,
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
/// Returns None if the JSON is invalid, the entry has no UUID, or the entry
/// has no type (guards against empty entries written by async PostToolUse
/// hooks in Claude Code pre-v2.1.119).
pub fn parse_entry(line: &[u8]) -> Option<Entry> {
    let e: Entry = serde_json::from_slice(line).ok()?;
    if (e.uuid.is_empty() && e.leaf_uuid.is_empty()) || e.entry_type.is_empty() {
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

    #[test]
    fn parse_entry_captures_content_field_for_away_summary() {
        // v2.1.108+: {type:"system",subtype:"away_summary",content:"<text>",uuid:"...",timestamp:"..."}
        let line = json!({
            "type": "system",
            "subtype": "away_summary",
            "uuid": "recap-uuid-123",
            "timestamp": "2026-04-14T10:00:00Z",
            "isMeta": false,
            "content": "Working on issue #49 — fixing recap entry parsing."
        });
        let bytes = serde_json::to_vec(&line).unwrap();
        let entry = parse_entry(&bytes).expect("must parse away_summary entry");
        assert_eq!(entry.entry_type, "system");
        assert_eq!(entry.subtype, "away_summary");
        assert_eq!(
            entry.content,
            "Working on issue #49 — fixing recap entry parsing."
        );
    }

    // --- Issue #60: forkedFrom field compat (v2.1.118+) ---

    #[test]
    fn parse_entry_forked_from_field_is_captured() {
        // v2.1.118+: when /fork branches a conversation, each inherited parent entry
        // carries forkedFrom:{sessionId,messageUuid}. The field must be captured.
        let line = json!({
            "type": "user",
            "uuid": "fork-entry-uuid",
            "timestamp": "2026-04-26T10:00:00Z",
            "message": {"role": "user", "content": "Hello"},
            "forkedFrom": {
                "sessionId": "parent-session-id",
                "messageUuid": "fork-entry-uuid"
            }
        });
        let bytes = serde_json::to_vec(&line).unwrap();
        let entry = parse_entry(&bytes).expect("must parse forked entry");
        assert!(entry.forked_from.is_some(), "forkedFrom must be captured");
        let ff = entry.forked_from.as_ref().unwrap();
        assert_eq!(
            ff.get("sessionId").and_then(|v| v.as_str()),
            Some("parent-session-id")
        );
        assert_eq!(
            ff.get("messageUuid").and_then(|v| v.as_str()),
            Some("fork-entry-uuid")
        );
    }

    #[test]
    fn parse_entry_without_forked_from_is_not_inherited() {
        // Regular entries (not inherited from a fork parent) must have forked_from = None.
        let line = json!({
            "type": "user",
            "uuid": "regular-uuid",
            "timestamp": "2026-04-26T10:00:00Z",
            "message": {"role": "user", "content": "Hello"}
        });
        let bytes = serde_json::to_vec(&line).unwrap();
        let entry = parse_entry(&bytes).expect("must parse regular entry");
        assert!(
            entry.forked_from.is_none(),
            "regular entry must not have forkedFrom"
        );
    }

    #[test]
    fn parse_entry_with_uuid_but_empty_type_returns_none() {
        // Async PostToolUse hooks in Claude Code pre-v2.1.119 could write entries
        // that have a uuid but no type field.  These must be silently skipped.
        let line = json!({
            "uuid": "dead-beef-1234",
            "timestamp": "2025-01-15T10:30:00Z",
            "message": {"role": "user", "content": "hook output"}
        });
        let bytes = serde_json::to_vec(&line).unwrap();
        assert!(parse_entry(&bytes).is_none());
    }

    #[test]
    fn parse_entry_completely_empty_object_returns_none() {
        // A completely empty JSONL entry {} has no uuid and no type — must be skipped.
        let bytes = b"{}";
        assert!(parse_entry(bytes).is_none());
    }

    #[test]
    fn parse_entry_handles_null_parent_uuid() {
        // Subagent JSONL files write parentUuid: null for the first entry.
        // parse_entry must succeed and treat null as an empty string.
        let line = json!({
            "type": "user",
            "uuid": "e65f5102-fdbe-424d-814f-a04e1ab466c6",
            "parentUuid": null,
            "isSidechain": true,
            "timestamp": "2026-04-12T21:18:39.485Z",
            "message": {"role": "user", "content": "Base directory for this skill: /skills/test"}
        });
        let bytes = serde_json::to_vec(&line).unwrap();
        let entry = parse_entry(&bytes).expect("must parse despite null parentUuid");
        assert_eq!(entry.parent_uuid, "");
        assert_eq!(entry.entry_type, "user");
    }

    // --- Issue #60: fork-context-ref entry (v2.1.118+) ---

    #[test]
    fn parse_entry_captures_fork_context_ref_fields() {
        // v2.1.118+: /fork writes a type:"fork-context-ref" pointer entry with forkedSessionId
        // and upToMessageId instead of duplicating the full parent conversation.
        let line = json!({
            "type": "fork-context-ref",
            "uuid": "fork-ref-uuid-001",
            "forkedSessionId": "parent-session-abc",
            "upToMessageId": "leaf-uuid-xyz",
            "timestamp": "2026-04-20T10:00:00Z"
        });
        let bytes = serde_json::to_vec(&line).unwrap();
        let entry = parse_entry(&bytes).expect("must parse fork-context-ref entry");
        assert_eq!(entry.entry_type, "fork-context-ref");
        assert_eq!(entry.forked_session_id, "parent-session-abc");
        assert_eq!(entry.up_to_message_id, "leaf-uuid-xyz");
    }

    #[test]
    fn parse_entry_fork_context_ref_without_uuid_returns_none() {
        // A fork-context-ref entry with no uuid (and no leafUuid) must be silently dropped.
        let line = json!({
            "type": "fork-context-ref",
            "forkedSessionId": "parent-session-abc",
            "upToMessageId": "leaf-uuid-xyz",
            "timestamp": "2026-04-20T10:00:00Z"
        });
        let bytes = serde_json::to_vec(&line).unwrap();
        assert!(
            parse_entry(&bytes).is_none(),
            "fork-context-ref with no uuid must return None"
        );
    }

    // --- compact_boundary and isCompactSummary fields ---

    #[test]
    fn parse_entry_captures_logical_parent_uuid_for_compact_boundary() {
        // compact_boundary entries have parentUuid:null but logicalParentUuid pointing to the
        // last pre-compaction message so the live chain can follow back to that message.
        let line = json!({
            "type": "system",
            "subtype": "compact_boundary",
            "uuid": "boundary-uuid-001",
            "parentUuid": null,
            "logicalParentUuid": "last-pre-compact-uuid",
            "timestamp": "2026-04-26T12:21:02Z",
            "isMeta": false
        });
        let bytes = serde_json::to_vec(&line).unwrap();
        let entry = parse_entry(&bytes).expect("must parse compact_boundary entry");
        assert_eq!(entry.entry_type, "system");
        assert_eq!(entry.subtype, "compact_boundary");
        assert_eq!(entry.parent_uuid, "");
        assert_eq!(entry.logical_parent_uuid, "last-pre-compact-uuid");
    }

    #[test]
    fn parse_entry_captures_is_compact_summary_flag() {
        // Compact summary user entries have isCompactSummary:true so classify() can
        // render them as a CompactMsg separator instead of a regular user message.
        let line = json!({
            "type": "user",
            "uuid": "compact-summary-uuid-001",
            "parentUuid": "boundary-uuid-001",
            "isCompactSummary": true,
            "timestamp": "2026-04-26T12:21:02Z",
            "message": {"role": "user", "content": "This session is being continued..."}
        });
        let bytes = serde_json::to_vec(&line).unwrap();
        let entry = parse_entry(&bytes).expect("must parse compact summary entry");
        assert!(entry.is_compact_summary, "isCompactSummary must be true");
    }

    #[test]
    fn parse_entry_logical_parent_uuid_defaults_to_empty() {
        // Regular entries without logicalParentUuid must have an empty string.
        let line = json!({
            "type": "user",
            "uuid": "regular-uuid",
            "timestamp": "2026-04-26T10:00:00Z",
            "message": {"role": "user", "content": "Hello"}
        });
        let bytes = serde_json::to_vec(&line).unwrap();
        let entry = parse_entry(&bytes).expect("must parse regular entry");
        assert_eq!(
            entry.logical_parent_uuid, "",
            "regular entry must have empty logical_parent_uuid"
        );
        assert!(!entry.is_compact_summary);
    }
}
