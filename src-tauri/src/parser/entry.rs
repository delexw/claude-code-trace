use serde::Deserialize;
use serde_json::Value;
use std::borrow::Cow;
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
    // Present in hook-related entries (v2.1.133+). Claude Code injects the active effort level
    // into hook input JSON as effort:{level:"low"|"normal"|"high"}.
    #[serde(default)]
    pub effort: Option<Value>,
    // Present in hook output entries (v2.1.141+). Hooks may emit this field to send desktop
    // notifications, window titles, or bells without a controlling terminal.
    #[serde(default, rename = "terminalSequence")]
    pub terminal_sequence: Option<String>,
    // Present in Stop and SubagentStop hook input payloads (v2.1.145+). Claude Code includes
    // currently-running background task descriptors and session-scoped cron jobs so hooks can
    // inspect or block on them before the session exits.
    #[serde(default, rename = "background_tasks")]
    pub background_tasks: Option<Value>,
    #[serde(default, rename = "session_crons")]
    pub session_crons: Option<Value>,
    // Present in workflow lifecycle entries (v2.1.154+). Claude Code's dynamic workflow
    // system writes workflow-start, workflow-progress, workflow-complete, workflow-cancelled,
    // and workflow-error entries carrying these fields.
    #[serde(default, rename = "workflowId")]
    pub workflow_id: String,
    #[serde(default, rename = "workflowName")]
    pub workflow_name: String,
    #[serde(default, rename = "workflowRunUrl")]
    pub workflow_run_url: String,
    #[serde(default, rename = "workflowStatus")]
    pub workflow_status: String,
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

/// Nested cache-creation breakdown returned by the Anthropic API since Claude Code v2.1.152.
/// The API may report cache writes as `usage.cache_creation.input_tokens` instead of (or in
/// addition to) the flat `usage.cache_creation_input_tokens` field.
#[derive(Debug, Deserialize, Default)]
pub struct CacheCreationUsage {
    #[serde(default)]
    pub input_tokens: i64,
}

#[derive(Debug, Deserialize, Default)]
pub struct EntryUsage {
    #[serde(default)]
    pub input_tokens: i64,
    #[serde(default)]
    pub output_tokens: i64,
    #[serde(default)]
    pub cache_read_input_tokens: i64,
    /// Flat format (pre-v2.1.152). May be 0 when the API uses the nested format.
    #[serde(default)]
    pub cache_creation_input_tokens: i64,
    /// Nested format (v2.1.152+). Takes precedence when non-zero.
    #[serde(default)]
    pub cache_creation: Option<CacheCreationUsage>,
}

impl EntryUsage {
    /// Returns the effective cache-creation token count, handling both the flat and nested
    /// API formats. Takes the max so old sessions (flat only) and new sessions (nested only)
    /// both read correctly.
    pub fn effective_cache_creation_input_tokens(&self) -> i64 {
        let nested = self
            .cache_creation
            .as_ref()
            .map(|c| c.input_tokens)
            .unwrap_or(0);
        self.cache_creation_input_tokens.max(nested)
    }
}

/// Extracts the effective cache-creation token count from a raw `usage` JSON value,
/// handling both the flat `cache_creation_input_tokens` field and the nested
/// `cache_creation.input_tokens` form introduced in Claude Code v2.1.152.
pub(crate) fn cache_creation_from_value(usage: &Value) -> i64 {
    let flat = usage
        .get("cache_creation_input_tokens")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let nested = usage
        .get("cache_creation")
        .and_then(|v| v.get("input_tokens"))
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    flat.max(nested)
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

/// Parses 4 ASCII hex bytes into a u16. Returns None if bytes are fewer than 4
/// or contain non-hex characters.
fn hex4_to_u16(bytes: &[u8]) -> Option<u16> {
    if bytes.len() < 4 {
        return None;
    }
    let s = std::str::from_utf8(&bytes[..4]).ok()?;
    u16::from_str_radix(s, 16).ok()
}

/// Replaces lone UTF-16 surrogates (U+D800–U+DFFF) in JSON `\uXXXX` escape
/// sequences with the Unicode replacement character U+FFFD. JSONL files
/// written by Claude Code before v2.1.132 may contain lone surrogates when
/// the tool-error truncation logic split a multi-byte emoji at an offset
/// boundary. serde_json rejects lone surrogates per RFC 8259; this pass makes
/// such lines parseable before they reach the deserializer.
fn sanitize_lone_surrogates(s: &str) -> Cow<'_, str> {
    let bytes = s.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    // Delay allocation until the first lone surrogate is found.
    let mut result: Option<Vec<u8>> = None;

    while i < len {
        // Match \uXXXX (6 bytes: backslash, u, 4 hex digits).
        if bytes[i] == b'\\' && i + 5 < len && bytes[i + 1] == b'u' {
            if let Some(cp) = hex4_to_u16(&bytes[i + 2..i + 6]) {
                if (0xD800..=0xDBFF).contains(&cp) {
                    // High surrogate — valid only when immediately followed by \uDCxx–\uDFxx.
                    let is_valid_pair = i + 11 < len
                        && bytes[i + 6] == b'\\'
                        && bytes[i + 7] == b'u'
                        && hex4_to_u16(&bytes[i + 8..i + 12])
                            .is_some_and(|c| (0xDC00..=0xDFFF).contains(&c));
                    if is_valid_pair {
                        if let Some(ref mut buf) = result {
                            buf.extend_from_slice(&bytes[i..i + 12]);
                        }
                        i += 12;
                    } else {
                        result
                            .get_or_insert_with(|| bytes[..i].to_vec())
                            .extend_from_slice(b"\\uFFFD");
                        i += 6;
                    }
                    continue;
                } else if (0xDC00..=0xDFFF).contains(&cp) {
                    // Lone low surrogate.
                    result
                        .get_or_insert_with(|| bytes[..i].to_vec())
                        .extend_from_slice(b"\\uFFFD");
                    i += 6;
                    continue;
                }
            }
        }
        if let Some(ref mut buf) = result {
            buf.push(bytes[i]);
        }
        i += 1;
    }

    match result {
        None => Cow::Borrowed(s),
        Some(buf) => Cow::Owned(
            String::from_utf8(buf)
                .unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned()),
        ),
    }
}

/// Parse a single JSONL line into an Entry.
/// Returns None if the JSON is invalid, the entry has no UUID, or the entry
/// has no type (guards against empty entries written by async PostToolUse
/// hooks in Claude Code pre-v2.1.119).
pub fn parse_entry(line: &[u8]) -> Option<Entry> {
    let s = std::str::from_utf8(line).ok()?;
    let sanitized = sanitize_lone_surrogates(s);
    let e: Entry = serde_json::from_str(&sanitized).ok()?;
    if (e.uuid.is_empty() && e.leaf_uuid.is_empty()) || e.entry_type.is_empty() {
        return None;
    }
    Some(e)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // --- cache_creation_from_value / effective_cache_creation_input_tokens tests ---

    #[test]
    fn cache_creation_flat_format() {
        let usage = json!({"cache_creation_input_tokens": 200});
        assert_eq!(cache_creation_from_value(&usage), 200);
    }

    #[test]
    fn cache_creation_nested_format() {
        let usage = json!({"cache_creation": {"input_tokens": 300}});
        assert_eq!(cache_creation_from_value(&usage), 300);
    }

    #[test]
    fn cache_creation_both_formats_returns_max() {
        // Both fields present: take the larger value.
        let usage =
            json!({"cache_creation_input_tokens": 100, "cache_creation": {"input_tokens": 300}});
        assert_eq!(cache_creation_from_value(&usage), 300);
    }

    #[test]
    fn cache_creation_neither_format_returns_zero() {
        let usage = json!({"input_tokens": 50});
        assert_eq!(cache_creation_from_value(&usage), 0);
    }

    #[test]
    fn entry_usage_deserializes_nested_format() {
        let json_str = r#"{"input_tokens":10,"output_tokens":5,"cache_read_input_tokens":0,"cache_creation":{"input_tokens":400}}"#;
        let usage: EntryUsage = serde_json::from_str(json_str).unwrap();
        assert_eq!(usage.cache_creation_input_tokens, 0);
        assert_eq!(usage.effective_cache_creation_input_tokens(), 400);
    }

    #[test]
    fn entry_usage_deserializes_flat_format() {
        let json_str = r#"{"input_tokens":10,"output_tokens":5,"cache_read_input_tokens":0,"cache_creation_input_tokens":200}"#;
        let usage: EntryUsage = serde_json::from_str(json_str).unwrap();
        assert_eq!(usage.cache_creation_input_tokens, 200);
        assert_eq!(usage.effective_cache_creation_input_tokens(), 200);
    }

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
    fn parse_entry_truncated_json_returns_none() {
        // Simulates a line partially written before an unclean shutdown (kill -9, OOM).
        // The JSON object is cut mid-string — must return None, not panic or error.
        let bytes = b"{\"type\":\"assistant\",\"uuid\":\"a1\",\"message\":{\"role\":\"assistant\"";
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

    // --- Issue #86: v2.1.133+ effort.level and v2.1.141+ terminalSequence compat ---

    #[test]
    fn parse_entry_captures_effort_field_v2_1_133() {
        // v2.1.133+: hook input JSON includes effort:{level:"..."} at the top level.
        // The parser must capture the nested object so callers can inspect effort.level.
        let line = json!({
            "type": "system",
            "subtype": "hook_progress",
            "uuid": "hook-effort-uuid",
            "timestamp": "2026-05-07T10:00:00Z",
            "hookEvent": "PreToolUse",
            "hookName": "my-hook",
            "effort": {"level": "high"}
        });
        let bytes = serde_json::to_vec(&line).unwrap();
        let entry = parse_entry(&bytes).expect("must parse hook entry with effort field");
        let effort = entry.effort.expect("effort must be captured");
        assert_eq!(
            effort.get("level").and_then(|v| v.as_str()),
            Some("high"),
            "effort.level must be 'high'"
        );
    }

    #[test]
    fn parse_entry_effort_defaults_to_none_when_absent() {
        // Entries from before v2.1.133 (or non-hook entries) have no effort field.
        let line = json!({
            "type": "user",
            "uuid": "no-effort-uuid",
            "timestamp": "2026-05-07T10:00:00Z",
            "message": {"role": "user", "content": "Hello"}
        });
        let bytes = serde_json::to_vec(&line).unwrap();
        let entry = parse_entry(&bytes).expect("must parse entry without effort field");
        assert!(entry.effort.is_none(), "effort must be None when absent");
    }

    #[test]
    fn parse_entry_captures_terminal_sequence_field_v2_1_141() {
        // v2.1.141+: hook output entries may carry terminalSequence at the top level,
        // allowing hooks to emit desktop notifications and bells.
        let line = json!({
            "type": "attachment",
            "uuid": "hook-out-uuid",
            "timestamp": "2026-05-13T10:00:00Z",
            "attachment": {"type": "hook_success", "hookEvent": "PostToolUse"},
            "terminalSequence": "\x1b]9;Notification\x07"
        });
        let bytes = serde_json::to_vec(&line).unwrap();
        let entry =
            parse_entry(&bytes).expect("must parse hook output entry with terminalSequence");
        assert_eq!(
            entry.terminal_sequence.as_deref(),
            Some("\x1b]9;Notification\x07"),
            "terminalSequence must be captured"
        );
    }

    #[test]
    fn parse_entry_terminal_sequence_defaults_to_none_when_absent() {
        // Entries from before v2.1.141 (or hooks that don't emit terminal sequences) have no
        // terminalSequence field.
        let line = json!({
            "type": "attachment",
            "uuid": "old-hook-out-uuid",
            "timestamp": "2026-05-01T10:00:00Z",
            "attachment": {"type": "hook_success", "hookEvent": "PostToolUse"}
        });
        let bytes = serde_json::to_vec(&line).unwrap();
        let entry =
            parse_entry(&bytes).expect("must parse hook output entry without terminalSequence");
        assert!(
            entry.terminal_sequence.is_none(),
            "terminalSequence must be None when absent"
        );
    }

    // --- Issue #106: v2.1.145+ Stop/SubagentStop gain background_tasks and session_crons ---

    #[test]
    fn parse_entry_captures_background_tasks_and_session_crons_v2_1_145() {
        // v2.1.145+: Stop and SubagentStop hook input payloads include background_tasks
        // (array of running task descriptors) and session_crons (array of registered cron jobs).
        // Both must be captured as Value so callers can inspect them.
        let line = json!({
            "type": "system",
            "subtype": "hook_progress",
            "uuid": "stop-hook-uuid-145",
            "timestamp": "2026-05-19T10:00:00Z",
            "hookEvent": "Stop",
            "hookName": "on-stop",
            "background_tasks": [{"id": "task-1", "description": "running bg job"}],
            "session_crons": [{"id": "cron-1", "schedule": "*/5 * * * *"}]
        });
        let bytes = serde_json::to_vec(&line).unwrap();
        let entry =
            parse_entry(&bytes).expect("must parse Stop hook entry with new v2.1.145 fields");

        let tasks = entry
            .background_tasks
            .expect("background_tasks must be captured");
        assert!(tasks.is_array(), "background_tasks must be an array");
        assert_eq!(tasks.as_array().unwrap().len(), 1);
        assert_eq!(tasks[0].get("id").and_then(|v| v.as_str()), Some("task-1"));

        let crons = entry.session_crons.expect("session_crons must be captured");
        assert!(crons.is_array(), "session_crons must be an array");
        assert_eq!(crons.as_array().unwrap().len(), 1);
        assert_eq!(
            crons[0].get("schedule").and_then(|v| v.as_str()),
            Some("*/5 * * * *")
        );
    }

    #[test]
    fn parse_entry_background_tasks_and_session_crons_default_to_none_when_absent() {
        // Stop/SubagentStop hook entries from before v2.1.145 have no background_tasks or
        // session_crons fields — both must default to None.
        let line = json!({
            "type": "system",
            "subtype": "hook_progress",
            "uuid": "stop-hook-uuid-old",
            "timestamp": "2026-05-01T10:00:00Z",
            "hookEvent": "Stop",
            "hookName": "on-stop"
        });
        let bytes = serde_json::to_vec(&line).unwrap();
        let entry = parse_entry(&bytes).expect("must parse old Stop hook entry");
        assert!(
            entry.background_tasks.is_none(),
            "background_tasks must be None when absent"
        );
        assert!(
            entry.session_crons.is_none(),
            "session_crons must be None when absent"
        );
    }

    // --- Issue #115: v2.1.154+ Dynamic Workflow fields ---

    #[test]
    fn parse_entry_captures_workflow_fields_v2_1_154() {
        // v2.1.154+: workflow lifecycle entries carry workflowId, workflowName,
        // workflowRunUrl, and workflowStatus at the top level.
        let line = json!({
            "type": "workflow-start",
            "uuid": "wf-start-uuid-001",
            "timestamp": "2026-05-28T10:00:00Z",
            "workflowId": "wf-abc-123",
            "workflowName": "my-workflow",
            "workflowRunUrl": "https://example.com/workflows/wf-abc-123",
            "workflowStatus": "running"
        });
        let bytes = serde_json::to_vec(&line).unwrap();
        let entry = parse_entry(&bytes).expect("must parse workflow-start entry");
        assert_eq!(entry.entry_type, "workflow-start");
        assert_eq!(entry.workflow_id, "wf-abc-123");
        assert_eq!(entry.workflow_name, "my-workflow");
        assert_eq!(
            entry.workflow_run_url,
            "https://example.com/workflows/wf-abc-123"
        );
        assert_eq!(entry.workflow_status, "running");
    }

    #[test]
    fn parse_entry_workflow_fields_default_to_empty_when_absent() {
        // Regular entries from before v2.1.154 have no workflow fields — must default to "".
        let line = json!({
            "type": "user",
            "uuid": "regular-uuid-no-wf",
            "timestamp": "2026-05-28T10:00:00Z",
            "message": {"role": "user", "content": "Hello"}
        });
        let bytes = serde_json::to_vec(&line).unwrap();
        let entry = parse_entry(&bytes).expect("must parse regular entry");
        assert_eq!(entry.workflow_id, "");
        assert_eq!(entry.workflow_name, "");
        assert_eq!(entry.workflow_run_url, "");
        assert_eq!(entry.workflow_status, "");
    }

    #[test]
    fn parse_entry_workflow_entry_with_unknown_fields_succeeds() {
        // Workflow entries may carry additional fields not yet known. The parser must
        // not reject them — no #[serde(deny_unknown_fields)] is set on Entry.
        let line = json!({
            "type": "workflow-progress",
            "uuid": "wf-progress-uuid-001",
            "timestamp": "2026-05-28T10:01:00Z",
            "workflowId": "wf-xyz-999",
            "workflowName": "my-workflow",
            "workflowStatus": "running",
            "futureWorkflowField": "some-value",
            "nestedWorkflowData": {"agentCount": 10, "completedAgents": 3}
        });
        let bytes = serde_json::to_vec(&line).unwrap();
        let entry =
            parse_entry(&bytes).expect("must parse workflow-progress despite unknown fields");
        assert_eq!(entry.entry_type, "workflow-progress");
        assert_eq!(entry.workflow_id, "wf-xyz-999");
    }

    #[test]
    fn parse_entry_all_workflow_lifecycle_types_succeed() {
        // All five workflow lifecycle types must parse without panicking or returning None.
        for wf_type in &[
            "workflow-start",
            "workflow-progress",
            "workflow-complete",
            "workflow-cancelled",
            "workflow-error",
        ] {
            let line = json!({
                "type": wf_type,
                "uuid": format!("uuid-{}", wf_type),
                "timestamp": "2026-05-28T10:00:00Z",
                "workflowId": "wf-123",
                "workflowName": "test-workflow",
                "workflowStatus": "running"
            });
            let bytes = serde_json::to_vec(&line).unwrap();
            let entry = parse_entry(&bytes)
                .unwrap_or_else(|| panic!("must parse {wf_type} entry without panicking"));
            assert_eq!(entry.entry_type, *wf_type);
        }
    }

    #[test]
    fn parse_entry_subagent_stop_with_background_tasks_and_session_crons() {
        // SubagentStop hook input also gains these fields in v2.1.145+.
        let line = json!({
            "type": "system",
            "subtype": "hook_progress",
            "uuid": "subagent-stop-uuid-145",
            "timestamp": "2026-05-19T11:00:00Z",
            "hookEvent": "SubagentStop",
            "hookName": "on-subagent-stop",
            "background_tasks": [],
            "session_crons": [{"id": "c1"}, {"id": "c2"}]
        });
        let bytes = serde_json::to_vec(&line).unwrap();
        let entry =
            parse_entry(&bytes).expect("must parse SubagentStop hook entry with new fields");

        let tasks = entry
            .background_tasks
            .expect("background_tasks must be captured");
        assert!(
            tasks.as_array().unwrap().is_empty(),
            "empty array must be preserved"
        );

        let crons = entry.session_crons.expect("session_crons must be captured");
        assert_eq!(crons.as_array().unwrap().len(), 2);
    }

    // --- Issue #85: lone UTF-16 surrogate sanitization ---

    #[test]
    fn sanitize_lone_surrogates_no_surrogates_returns_borrowed() {
        let s = r#"{\"key\":\"hello world\"}"#;
        let result = sanitize_lone_surrogates(s);
        assert!(
            matches!(result, Cow::Borrowed(_)),
            "no surrogates -- must return borrowed (no allocation)"
        );
        assert_eq!(result.as_ref(), s);
    }

    #[test]
    fn sanitize_lone_high_surrogate_replaced_with_fffd() {
        // \uD83D is a lone high surrogate (no following \uDCxx).
        // The function outputs the literal JSON escape \uFFFD (6 ASCII chars).
        let s = r#"{\"key\":\"emoji \uD83D truncated\"}"#;
        let result = sanitize_lone_surrogates(s);
        assert!(
            result.as_ref().contains(r"\uFFFD"),
            "replacement escape must be present"
        );
        assert!(
            !result.as_ref().contains(r"\uD83D"),
            "lone surrogate must be removed"
        );
    }

    #[test]
    fn sanitize_lone_low_surrogate_replaced_with_fffd() {
        // \uDC36 is a lone low surrogate (not preceded by a high surrogate).
        let s = r#"{\"key\":\"broken \uDC36 emoji\"}"#;
        let result = sanitize_lone_surrogates(s);
        assert!(
            result.as_ref().contains(r"\uFFFD"),
            "replacement escape must be present"
        );
        assert!(
            !result.as_ref().contains(r"\uDC36"),
            "lone surrogate must be removed"
        );
    }

    #[test]
    fn sanitize_valid_surrogate_pair_unchanged() {
        // \uD83D\uDC36 is a valid surrogate pair (dog face emoji).
        let s = r#"{\"key\":\"dog \uD83D\uDC36 emoji\"}"#;
        let result = sanitize_lone_surrogates(s);
        assert!(
            matches!(result, Cow::Borrowed(_)),
            "valid pair must return borrowed (no modification)"
        );
        assert_eq!(result.as_ref(), s);
    }

    #[test]
    fn sanitize_multiple_lone_surrogates_all_replaced() {
        let s = r#"{\"a\":\"\uD83D\",\"b\":\"\uDC00\"}"#;
        let result = sanitize_lone_surrogates(s);
        assert!(
            !result.as_ref().contains(r"\uD83D"),
            "first lone surrogate must be removed"
        );
        assert!(
            !result.as_ref().contains(r"\uDC00"),
            "second lone surrogate must be removed"
        );
        let fffd_count = result.as_ref().match_indices(r"\uFFFD").count();
        assert_eq!(
            fffd_count, 2,
            "both lone surrogates must be replaced with \\uFFFD"
        );
    }

    #[test]
    fn sanitize_high_surrogate_at_end_of_string_replaced() {
        // \uD83D at end of value -- no room for a low surrogate, must be replaced.
        let s = r#"\"\uD83D\""#;
        let result = sanitize_lone_surrogates(s);
        assert!(
            result.as_ref().contains(r"\uFFFD"),
            "replacement escape must be present"
        );
        assert!(
            !result.as_ref().contains(r"\uD83D"),
            "lone surrogate must be removed"
        );
    }

    #[test]
    fn parse_entry_with_lone_high_surrogate_succeeds() {
        // Simulates a JSONL line from Claude Code < v2.1.132 where tool error
        // truncation left a lone \uD83D (high surrogate, no low surrogate follows).
        // serde_json rejects this without sanitization; parse_entry must succeed.
        let line = r#"{"type":"user","uuid":"emoji-lone-high","timestamp":"2026-05-01T10:00:00Z","message":{"role":"user","content":"truncated emoji: \uD83D end"}}"#.as_bytes();
        let entry = parse_entry(line);
        assert!(
            entry.is_some(),
            "parse_entry must succeed despite lone high surrogate"
        );
        let e = entry.unwrap();
        assert_eq!(e.uuid, "emoji-lone-high");
        assert_eq!(e.entry_type, "user");
    }

    #[test]
    fn parse_entry_with_lone_low_surrogate_succeeds() {
        // Lone low surrogate \uDC36 without a preceding high surrogate.
        let line = r#"{"type":"user","uuid":"emoji-lone-low","timestamp":"2026-05-01T10:00:00Z","message":{"role":"user","content":"lone low: \uDC36"}}"#.as_bytes();
        let entry = parse_entry(line);
        assert!(
            entry.is_some(),
            "parse_entry must succeed despite lone low surrogate"
        );
        assert_eq!(entry.unwrap().uuid, "emoji-lone-low");
    }

    #[test]
    fn parse_entry_with_valid_surrogate_pair_succeeds() {
        // Valid surrogate pair \uD83D\uDC36 (dog face) must parse successfully.
        let line = r#"{"type":"user","uuid":"emoji-valid-pair","timestamp":"2026-05-01T10:00:00Z","message":{"role":"user","content":"dog: \uD83D\uDC36"}}"#.as_bytes();
        let entry = parse_entry(line);
        assert!(
            entry.is_some(),
            "parse_entry must succeed with a valid surrogate pair"
        );
        assert_eq!(entry.unwrap().uuid, "emoji-valid-pair");
    }

    #[test]
    fn parse_entry_unknown_fields_are_silently_ignored() {
        // Future Claude Code versions may add more fields. The parser must never crash on
        // unknown top-level fields — they must be silently dropped.
        let line = json!({
            "type": "system",
            "uuid": "future-uuid",
            "timestamp": "2026-05-17T10:00:00Z",
            "unknownFutureField": "some-value",
            "anotherNewField": {"nested": true}
        });
        let bytes = serde_json::to_vec(&line).unwrap();
        let entry = parse_entry(&bytes).expect("must parse despite unknown fields");
        assert_eq!(entry.entry_type, "system");
        assert_eq!(entry.uuid, "future-uuid");
    }
}
