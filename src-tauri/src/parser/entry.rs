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
    // hook_event is stored as a plain String (not an enum) so that new hook event names
    // introduced by future Claude Code releases (e.g. MessageDisplay added in v2.1.152) are
    // captured as-is rather than rejected. Callers that need to distinguish specific event
    // types should match on the string value with a wildcard fallback arm.
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
    // Present in Stop and SubagentStop hook result entries (v2.1.163+). When a hook returns
    // hookSpecificOutput.additionalContext, Claude Code persists the payload at the top level
    // so the feedback text can be injected back into the session without being labeled a hook
    // error.
    #[serde(default, rename = "hookSpecificOutput")]
    pub hook_specific_output: Option<Value>,
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
    // Present in sidechain entries when Claude Code writes deeply nested sub-agent attribution
    // fields (v2.1.172+). With 5-level sub-agent nesting, entries carry `agentDepth` (1-indexed
    // depth in the agent tree) and `parentAgentName` (the spawning agent's name). Both are
    // optional — older sessions and non-sidechain entries omit them entirely.
    // Declared here so that when Claude Code adds them, the fields are captured rather than
    // silently dropped by the parser.
    #[serde(default, rename = "agentDepth")]
    pub agent_depth: Option<u32>,
    #[serde(default, rename = "parentAgentName")]
    pub parent_agent_name: String,
    // Present in type:"rewind-pointer" entries (v2.1.191+). When /rewind is used to resume a
    // conversation from before /clear was run, Claude Code may write a rewind-pointer entry.
    // rewindToUuid identifies the last pre-clear message UUID so the chain resolver can
    // understand where the post-rewind conversation re-roots.
    #[serde(default, rename = "rewindToUuid", deserialize_with = "null_as_default")]
    pub rewind_to_uuid: String,
    // Present in summary or compact_boundary entries (v2.1.191+). When true, the compaction
    // checkpoint is persisted and the pre-clear state can be resumed via /rewind.
    #[serde(default)]
    pub rewindable: bool,
    // Present in summary or compact_boundary entries when rewindable:true (v2.1.191+).
    // Points to the UUID of the last pre-clear message — the anchor for /rewind. Enables
    // the chain resolver to re-root a post-rewind conversation at the correct entry.
    #[serde(
        default,
        rename = "checkpointUuid",
        deserialize_with = "null_as_default"
    )]
    pub checkpoint_uuid: String,
    // Background-agent session fields added to all entry types in Claude Code v2.1.141+.
    // When a session is written by the Claude Code SDK or a background agent, every entry
    // carries these fields so the session can be attributed and versioned independently
    // of the main interactive session.
    //
    // `version` is the Claude Code version string that wrote the entry (e.g. "2.1.141").
    // It acts as a schema-version discriminant: parsers can gate forward-compat logic on it
    // without requiring a separate metadata file.
    #[serde(default)]
    pub version: String,
    // `entrypoint` identifies how Claude Code was invoked when the entry was written.
    // Common values: "sdk-ts" (TypeScript SDK / background agent), "cli" (interactive CLI).
    #[serde(default)]
    pub entrypoint: String,
    // `sessionId` is the owning session's UUID. Present on all background-agent entries and
    // on type:"last-prompt" / type:"queue-operation" structural metadata entries.
    #[serde(default, rename = "sessionId")]
    pub session_id: String,
    // `agentId` identifies the specific background-agent instance that wrote the entry.
    // Distinct from `agentName` (a human-readable label): agentId is an opaque identifier
    // assigned by Claude Code at agent creation time.
    #[serde(default, rename = "agentId")]
    pub agent_id: String,
    // `userType` classifies the actor that submitted the prompt. Common values:
    // "external" (SDK / background agent), "human" (interactive CLI user).
    #[serde(default, rename = "userType")]
    pub user_type: String,
    // `attributionSkill` names the Claude Code skill that spawned this background-agent
    // session. Absent when the agent was launched directly rather than via a skill.
    #[serde(default, rename = "attributionSkill")]
    pub attribution_skill: Option<String>,
    // Present in type:"last-prompt" entries (v2.1.195+). Claude Code writes a last-prompt
    // entry to persist the most recent prompt text for background-agent checkpoint/resume.
    // The entry's `leafUuid` points to the last message in the conversation at the time
    // the checkpoint was written.
    #[serde(default, rename = "lastPrompt")]
    pub last_prompt: String,
    // Present in auto-mode denial entries (v2.1.193+). When Claude Code's auto-mode denies a
    // tool call, it writes a denial entry to the transcript for replay and /permissions recent
    // denials display. `reason` holds the human-readable denial explanation; `tool_name` names
    // the tool that was blocked (may also appear in progress data for the same event).
    #[serde(default)]
    pub reason: String,
    #[serde(default, rename = "toolName")]
    pub tool_name: String,
    // Present in background subagent permission prompt entries (v2.1.186+). When a background
    // subagent surfaces a permission prompt in the main session JSONL instead of being
    // auto-denied, `source_agent_name` is the requesting subagent's display name and
    // `requesting_agent_uuid` is its session UUID, allowing the UI to attribute the prompt.
    #[serde(default, rename = "sourceAgentName")]
    pub source_agent_name: String,
    #[serde(default, rename = "requestingAgentUuid")]
    pub requesting_agent_uuid: String,
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
    // v2.1.179+: mid-stream connection drops flush a partial assistant entry where usage may be
    // null. null_as_default converts null → EntryUsage::default() so the entry is preserved.
    #[serde(default, deserialize_with = "null_as_default")]
    pub usage: EntryUsage,
    // Present in assistant message objects (v2.1.212+). Claude Code records the active reasoning
    // effort level on each assistant message in session transcripts as
    // effort:{level:"low"|"normal"|"high"}. Previously this only appeared in hook input JSON at
    // the top-level entry (v2.1.133+, issue #86); v2.1.212 promotes it to every assistant turn.
    #[serde(default)]
    pub effort: Option<Value>,
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
    // v2.1.179+: partial flush entries may include null for individual token counts.
    // null_as_default maps null → 0 so arithmetic on partial usage is safe.
    #[serde(default, deserialize_with = "null_as_default")]
    pub input_tokens: i64,
    #[serde(default, deserialize_with = "null_as_default")]
    pub output_tokens: i64,
    #[serde(default, deserialize_with = "null_as_default")]
    pub cache_read_input_tokens: i64,
    /// Flat format (pre-v2.1.152). May be 0 when the API uses the nested format.
    #[serde(default, deserialize_with = "null_as_default")]
    pub cache_creation_input_tokens: i64,
    /// Nested format (v2.1.152+). Takes precedence when non-zero.
    #[serde(default)]
    pub cache_creation: Option<CacheCreationUsage>,
    /// Per-iteration usage breakdown. The advisor tool call (a single logical turn) actually
    /// spans multiple model invocations under the hood — the caller's own message plus a
    /// nested call to the advisor model — and Claude Code records each as one entry here.
    #[serde(default)]
    pub iterations: Vec<IterationUsage>,
}

impl EntryUsage {
    /// The model used for the advisor's own reasoning, when this entry's turn included an
    /// advisor invocation. `None` for ordinary turns with no advisor call.
    pub fn advisor_model(&self) -> Option<String> {
        self.iterations
            .iter()
            .find(|it| it.iteration_type == "advisor_message")
            .and_then(|it| it.model.clone())
    }
}

/// One entry in `usage.iterations` — a single model invocation within a turn.
#[derive(Debug, Deserialize, Default, Clone)]
pub struct IterationUsage {
    #[serde(default, rename = "type")]
    pub iteration_type: String,
    #[serde(default)]
    pub model: Option<String>,
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

    // --- EntryUsage::advisor_model tests ---

    #[test]
    fn advisor_model_found_in_iterations() {
        let json_str = r#"{"iterations":[{"type":"message"},{"type":"advisor_message","model":"claude-opus-4-8"},{"type":"message"}]}"#;
        let usage: EntryUsage = serde_json::from_str(json_str).unwrap();
        assert_eq!(usage.advisor_model(), Some("claude-opus-4-8".to_string()));
    }

    #[test]
    fn advisor_model_absent_when_no_advisor_iteration() {
        let json_str = r#"{"iterations":[{"type":"message"}]}"#;
        let usage: EntryUsage = serde_json::from_str(json_str).unwrap();
        assert_eq!(usage.advisor_model(), None);
    }

    #[test]
    fn advisor_model_absent_when_no_iterations_field() {
        let json_str = r#"{"input_tokens":10}"#;
        let usage: EntryUsage = serde_json::from_str(json_str).unwrap();
        assert_eq!(usage.advisor_model(), None);
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

    // --- Issue #125: v2.1.163+ hookSpecificOutput.additionalContext ---

    #[test]
    fn parse_entry_captures_hook_specific_output_v2_1_163() {
        // v2.1.163+: Stop and SubagentStop hooks can return hookSpecificOutput.additionalContext
        // to inject feedback back into the session. The field must be captured as a Value so
        // callers can inspect additionalContext and any future sub-fields.
        let line = json!({
            "type": "system",
            "subtype": "stop_hook_summary",
            "uuid": "stop-hook-output-uuid",
            "timestamp": "2026-06-04T10:00:00Z",
            "hookCount": 1,
            "hookInfos": [{"command": "~/.claude/hooks/stop.sh", "durationMs": 42}],
            "hookSpecificOutput": {
                "additionalContext": "All checks passed. You may continue."
            }
        });
        let bytes = serde_json::to_vec(&line).unwrap();
        let entry = parse_entry(&bytes)
            .expect("must parse stop_hook_summary entry with hookSpecificOutput");
        assert_eq!(entry.subtype, "stop_hook_summary");
        let hso = entry
            .hook_specific_output
            .expect("hookSpecificOutput must be captured");
        assert_eq!(
            hso.get("additionalContext").and_then(|v| v.as_str()),
            Some("All checks passed. You may continue."),
            "additionalContext must be accessible from hookSpecificOutput"
        );
    }

    #[test]
    fn parse_entry_hook_specific_output_defaults_to_none_when_absent() {
        // Entries from before v2.1.163 (or hooks that don't return feedback) have no
        // hookSpecificOutput field — must default to None.
        let line = json!({
            "type": "system",
            "subtype": "stop_hook_summary",
            "uuid": "stop-hook-old-uuid",
            "timestamp": "2026-05-01T10:00:00Z",
            "hookCount": 1,
            "hookInfos": [{"command": "~/.claude/hooks/stop.sh", "durationMs": 10}]
        });
        let bytes = serde_json::to_vec(&line).unwrap();
        let entry = parse_entry(&bytes).expect("must parse old stop_hook_summary entry");
        assert!(
            entry.hook_specific_output.is_none(),
            "hookSpecificOutput must be None when absent"
        );
    }

    #[test]
    fn parse_entry_hook_specific_output_on_hook_progress_entry() {
        // hookSpecificOutput may also appear on system/hook_progress entries for SubagentStop.
        let line = json!({
            "type": "system",
            "subtype": "hook_progress",
            "uuid": "subagent-stop-output-uuid",
            "timestamp": "2026-06-04T11:00:00Z",
            "hookEvent": "SubagentStop",
            "hookName": "on-subagent-stop",
            "hookSpecificOutput": {
                "additionalContext": "Subagent completed successfully."
            }
        });
        let bytes = serde_json::to_vec(&line).unwrap();
        let entry = parse_entry(&bytes)
            .expect("must parse SubagentStop hook_progress with hookSpecificOutput");
        assert_eq!(entry.hook_event, "SubagentStop");
        let hso = entry
            .hook_specific_output
            .expect("hookSpecificOutput must be captured for SubagentStop hook_progress");
        assert_eq!(
            hso.get("additionalContext").and_then(|v| v.as_str()),
            Some("Subagent completed successfully.")
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

    // --- Issue #124: v2.1.166+ fallbackModel — parse-level compat ---

    #[test]
    fn parse_entry_assistant_with_null_content_succeeds() {
        // v2.1.166+: fallback retry stub written before the successful response.
        // The stub has message.content:null (or absent). parse_entry must not panic or
        // return None — it should succeed so the caller (classify) can decide to drop it.
        let line = json!({
            "type": "assistant",
            "uuid": "fallback-stub-null",
            "timestamp": "2026-06-06T10:00:00Z",
            "message": {
                "role": "assistant",
                "model": "claude-opus-4-7",
                "content": null
            }
        });
        let bytes = serde_json::to_vec(&line).unwrap();
        let entry = parse_entry(&bytes).expect("must parse assistant stub with null content");
        assert_eq!(entry.entry_type, "assistant");
        assert_eq!(entry.message.model, "claude-opus-4-7");
        assert!(
            entry.message.content.is_none(),
            "content must be None for null"
        );
    }

    #[test]
    fn parse_entry_assistant_with_empty_array_content_succeeds() {
        // v2.1.166+: the stub may carry an empty array rather than null.
        let line = json!({
            "type": "assistant",
            "uuid": "fallback-stub-empty-arr",
            "timestamp": "2026-06-06T10:00:01Z",
            "message": {
                "role": "assistant",
                "model": "claude-opus-4-7",
                "content": []
            }
        });
        let bytes = serde_json::to_vec(&line).unwrap();
        let entry =
            parse_entry(&bytes).expect("must parse assistant stub with empty-array content");
        assert_eq!(entry.entry_type, "assistant");
        assert_eq!(entry.message.model, "claude-opus-4-7");
        match entry.message.content {
            Some(serde_json::Value::Array(arr)) => {
                assert!(arr.is_empty(), "content must be an empty array");
            }
            other => panic!("expected Some(Array([])), got {other:?}"),
        }
    }

    #[test]
    fn parse_entry_assistant_fallback_model_differs_from_prior_entry() {
        // v2.1.166+: sessions using fallbackModel will have assistant entries whose
        // message.model does not match the session's primary model. Each entry's model
        // must be captured as-is without being normalised or overwritten.
        let line = json!({
            "type": "assistant",
            "uuid": "fallback-success-uuid",
            "timestamp": "2026-06-06T10:00:02Z",
            "message": {
                "role": "assistant",
                "model": "claude-haiku-4-5",
                "content": [{"type": "text", "text": "Fallback response"}],
                "stop_reason": "end_turn"
            }
        });
        let bytes = serde_json::to_vec(&line).unwrap();
        let entry = parse_entry(&bytes).expect("must parse fallback response entry");
        assert_eq!(
            entry.message.model, "claude-haiku-4-5",
            "fallback model must be captured verbatim"
        );
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

    // --- Issue #117: v2.1.152+ MessageDisplay hook event compat ---

    #[test]
    fn parse_entry_captures_message_display_hook_event_as_string() {
        // v2.1.152+: MessageDisplay is a new hook event name that surfaces in JSONL entries.
        // hook_event is stored as a plain String so this new value is captured without rejection.
        let line = json!({
            "type": "system",
            "subtype": "hook_progress",
            "uuid": "msg-display-uuid-001",
            "timestamp": "2026-05-27T10:00:00Z",
            "hookEvent": "MessageDisplay",
            "hookName": "my-display-hook"
        });
        let bytes = serde_json::to_vec(&line).unwrap();
        let entry = parse_entry(&bytes).expect("must parse MessageDisplay hook entry");
        assert_eq!(entry.hook_event, "MessageDisplay");
        assert_eq!(entry.hook_name, "my-display-hook");
    }

    #[test]
    fn parse_entry_message_display_as_attachment_is_captured() {
        // MessageDisplay can also surface as an attachment entry (hook result).
        let line = json!({
            "type": "attachment",
            "uuid": "msg-display-att-uuid",
            "timestamp": "2026-05-27T11:00:00Z",
            "attachment": {
                "type": "hook_success",
                "hookEvent": "MessageDisplay",
                "hookName": "transform-hook"
            }
        });
        let bytes = serde_json::to_vec(&line).unwrap();
        let entry = parse_entry(&bytes).expect("must parse MessageDisplay attachment entry");
        let att = entry.attachment.expect("attachment must be captured");
        assert_eq!(
            att.get("hookEvent").and_then(|v| v.as_str()),
            Some("MessageDisplay")
        );
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

    // --- Issue #135: v2.1.172+ 5-level sub-agent nesting — agentDepth / parentAgentName ---

    #[test]
    fn parse_entry_captures_agent_depth_field_v2_1_172() {
        // v2.1.172+: sidechain entries may carry agentDepth (1-indexed nesting level)
        // so callers can distinguish depth-1 sub-agents from depth-2 through depth-5.
        for depth in 1u32..=5 {
            let line = json!({
                "type": "assistant",
                "uuid": format!("depth-{depth}-uuid"),
                "parentUuid": format!("parent-of-depth-{depth}"),
                "isSidechain": true,
                "timestamp": "2026-06-10T10:00:00Z",
                "agentDepth": depth,
                "agentName": format!("agent-depth-{depth}"),
                "message": {
                    "role": "assistant",
                    "content": [{"type": "text", "text": "working"}],
                    "model": "claude-sonnet-4-6",
                    "stop_reason": "end_turn",
                    "usage": {"input_tokens": 10, "output_tokens": 5}
                }
            });
            let bytes = serde_json::to_vec(&line).unwrap();
            let entry = parse_entry(&bytes)
                .unwrap_or_else(|| panic!("must parse depth-{depth} sidechain entry"));
            assert_eq!(
                entry.agent_depth,
                Some(depth),
                "agentDepth must be captured for depth {depth}"
            );
            assert!(entry.is_sidechain, "entry must carry isSidechain:true");
        }
    }

    #[test]
    fn parse_entry_captures_parent_agent_name_field_v2_1_172() {
        // v2.1.172+: sidechain entries may carry parentAgentName identifying which
        // agent spawned this one — enables proper attribution in the UI.
        let line = json!({
            "type": "user",
            "uuid": "nested-agent-uuid",
            "parentUuid": "root-agent-entry-uuid",
            "isSidechain": true,
            "timestamp": "2026-06-10T11:00:00Z",
            "agentDepth": 2,
            "parentAgentName": "orchestrator",
            "agentName": "sub-worker",
            "message": {"role": "user", "content": "do some work"}
        });
        let bytes = serde_json::to_vec(&line).unwrap();
        let entry = parse_entry(&bytes).expect("must parse entry with parentAgentName");
        assert_eq!(
            entry.parent_agent_name, "orchestrator",
            "parentAgentName must be captured"
        );
        assert_eq!(entry.agent_name, "sub-worker");
        assert_eq!(entry.agent_depth, Some(2));
    }

    #[test]
    fn parse_entry_agent_depth_defaults_to_none_when_absent() {
        // Entries from before v2.1.172 (or non-sidechain entries) have no agentDepth —
        // must default to None.
        let line = json!({
            "type": "user",
            "uuid": "no-depth-uuid",
            "timestamp": "2026-06-10T10:00:00Z",
            "message": {"role": "user", "content": "Hello"}
        });
        let bytes = serde_json::to_vec(&line).unwrap();
        let entry = parse_entry(&bytes).expect("must parse entry without agentDepth");
        assert!(
            entry.agent_depth.is_none(),
            "agentDepth must be None when absent"
        );
        assert!(
            entry.parent_agent_name.is_empty(),
            "parentAgentName must be empty when absent"
        );
    }

    // --- Issue #156: v2.1.179+ mid-stream connection drop — partial assistant entries ---

    #[test]
    fn parse_entry_assistant_with_null_usage_succeeds() {
        // v2.1.179+: partial flush entries may have usage:null when the token count was not yet
        // computed at the time of the connection drop. The entry must be preserved (not dropped).
        let line = json!({
            "type": "assistant",
            "uuid": "partial-null-usage-uuid",
            "timestamp": "2026-06-16T10:00:00Z",
            "message": {
                "role": "assistant",
                "model": "claude-sonnet-4-6",
                "content": [{"type": "text", "text": "Partial respon"}],
                "stop_reason": null,
                "usage": null
            }
        });
        let bytes = serde_json::to_vec(&line).unwrap();
        let entry =
            parse_entry(&bytes).expect("must parse partial assistant entry with null usage");
        assert_eq!(entry.entry_type, "assistant");
        assert_eq!(entry.message.usage.input_tokens, 0);
        assert_eq!(entry.message.usage.output_tokens, 0);
        assert_eq!(entry.message.usage.cache_read_input_tokens, 0);
        assert_eq!(entry.message.usage.cache_creation_input_tokens, 0);
        assert!(
            entry.message.stop_reason.is_none(),
            "null stop_reason must be None"
        );
    }

    #[test]
    fn parse_entry_assistant_with_null_individual_usage_fields_succeeds() {
        // v2.1.179+: partial flush may emit usage with some fields explicitly null (e.g. only
        // output_tokens was counted before the drop). Each null field must default to 0.
        let line = json!({
            "type": "assistant",
            "uuid": "partial-null-fields-uuid",
            "timestamp": "2026-06-16T10:01:00Z",
            "message": {
                "role": "assistant",
                "model": "claude-sonnet-4-6",
                "content": [{"type": "text", "text": "Truncated mid"}],
                "stop_reason": null,
                "usage": {
                    "input_tokens": null,
                    "output_tokens": null,
                    "cache_read_input_tokens": null,
                    "cache_creation_input_tokens": null
                }
            }
        });
        let bytes = serde_json::to_vec(&line).unwrap();
        let entry =
            parse_entry(&bytes).expect("must parse partial entry with null token count fields");
        assert_eq!(entry.message.usage.input_tokens, 0);
        assert_eq!(entry.message.usage.output_tokens, 0);
        assert_eq!(entry.message.usage.cache_read_input_tokens, 0);
        assert_eq!(entry.message.usage.cache_creation_input_tokens, 0);
    }

    #[test]
    fn parse_entry_assistant_with_partial_usage_succeeds() {
        // v2.1.179+: only some usage sub-fields may be present (e.g. input_tokens computed
        // but output_tokens not yet). Absent fields must default to 0.
        let line = json!({
            "type": "assistant",
            "uuid": "partial-usage-fields-uuid",
            "timestamp": "2026-06-16T10:02:00Z",
            "message": {
                "role": "assistant",
                "model": "claude-sonnet-4-6",
                "content": [{"type": "text", "text": "Partial answer"}],
                "stop_reason": null,
                "usage": {"input_tokens": 42}
            }
        });
        let bytes = serde_json::to_vec(&line).unwrap();
        let entry =
            parse_entry(&bytes).expect("must parse partial entry with only some usage fields");
        assert_eq!(entry.message.usage.input_tokens, 42);
        assert_eq!(
            entry.message.usage.output_tokens, 0,
            "absent output_tokens must default to 0"
        );
        assert_eq!(entry.message.usage.cache_read_input_tokens, 0);
    }

    #[test]
    fn parse_entry_assistant_with_unknown_stop_reason_captured() {
        // v2.1.179+: a connection drop may produce a stop_reason not in the known set
        // (end_turn, tool_use, max_tokens). The value must be preserved as Some("...") rather
        // than failing deserialization.
        let line = json!({
            "type": "assistant",
            "uuid": "unknown-stop-reason-uuid",
            "timestamp": "2026-06-16T10:03:00Z",
            "message": {
                "role": "assistant",
                "model": "claude-sonnet-4-6",
                "content": [{"type": "text", "text": "Interrupted response"}],
                "stop_reason": "connection_drop",
                "usage": {"input_tokens": 10, "output_tokens": 5}
            }
        });
        let bytes = serde_json::to_vec(&line).unwrap();
        let entry = parse_entry(&bytes).expect("must parse entry with unknown stop_reason");
        assert_eq!(
            entry.message.stop_reason.as_deref(),
            Some("connection_drop"),
            "unknown stop_reason must be preserved verbatim"
        );
    }

    #[test]
    fn parse_entry_assistant_with_absent_usage_still_succeeds() {
        // Regression: entries with entirely absent usage (pre-v2.1.179 normal case) must
        // still be parsed successfully, defaulting all token counts to 0.
        let line = json!({
            "type": "assistant",
            "uuid": "absent-usage-uuid",
            "timestamp": "2026-06-16T10:04:00Z",
            "message": {
                "role": "assistant",
                "model": "claude-sonnet-4-6",
                "content": [{"type": "text", "text": "Normal response"}],
                "stop_reason": "end_turn"
            }
        });
        let bytes = serde_json::to_vec(&line).unwrap();
        let entry = parse_entry(&bytes).expect("must parse entry with absent usage");
        assert_eq!(entry.message.usage.input_tokens, 0);
        assert_eq!(entry.message.usage.output_tokens, 0);
        assert_eq!(entry.message.stop_reason.as_deref(), Some("end_turn"));
    }

    // --- Issue #168: v2.1.193+ auto-mode denial entries — reason and toolName fields ---

    #[test]
    fn parse_entry_captures_reason_and_tool_name_for_auto_mode_denial() {
        // v2.1.193+: a new type:"auto-mode-denial" entry carries a top-level `reason` and
        // `toolName` explaining why the tool call was blocked. Both must be captured.
        let line = json!({
            "type": "auto-mode-denial",
            "uuid": "denial-uuid-001",
            "timestamp": "2026-06-25T10:00:00Z",
            "reason": "Bash is not allowed in auto mode",
            "toolName": "Bash"
        });
        let bytes = serde_json::to_vec(&line).unwrap();
        let entry = parse_entry(&bytes).expect("must parse auto-mode-denial entry");
        assert_eq!(entry.entry_type, "auto-mode-denial");
        assert_eq!(entry.reason, "Bash is not allowed in auto mode");
        assert_eq!(entry.tool_name, "Bash");
    }

    #[test]
    fn parse_entry_captures_reason_for_permission_denial_type() {
        // v2.1.193+: alternative type name "permission-denial" with only a reason (no toolName).
        let line = json!({
            "type": "permission-denial",
            "uuid": "denial-uuid-002",
            "timestamp": "2026-06-25T10:01:00Z",
            "reason": "Write access denied by permission policy"
        });
        let bytes = serde_json::to_vec(&line).unwrap();
        let entry = parse_entry(&bytes).expect("must parse permission-denial entry");
        assert_eq!(entry.entry_type, "permission-denial");
        assert_eq!(entry.reason, "Write access denied by permission policy");
        assert_eq!(
            entry.tool_name, "",
            "absent toolName must default to empty string"
        );
    }

    #[test]
    fn parse_entry_reason_and_tool_name_default_to_empty_when_absent() {
        // Regular entries have no reason or toolName — both must default to "".
        let line = json!({
            "type": "user",
            "uuid": "regular-uuid-no-denial",
            "timestamp": "2026-06-25T10:00:00Z",
            "message": {"role": "user", "content": "Hello"}
        });
        let bytes = serde_json::to_vec(&line).unwrap();
        let entry = parse_entry(&bytes).expect("must parse regular entry");
        assert_eq!(entry.reason, "", "reason must be empty when absent");
        assert_eq!(entry.tool_name, "", "toolName must be empty when absent");
    }

    #[test]
    fn parse_entry_sidechain_with_all_depth_fields_succeeds() {
        // Full payload as Claude Code v2.1.172+ might write for a depth-3 sidechain entry.
        // Verifies the complete set of attribution fields round-trips correctly.
        let line = json!({
            "type": "assistant",
            "uuid": "depth3-assistant-uuid",
            "parentUuid": "depth3-parent-uuid",
            "isSidechain": true,
            "timestamp": "2026-06-10T12:00:00Z",
            "agentDepth": 3,
            "agentName": "deep-worker",
            "parentAgentName": "mid-level-agent",
            "teamName": "",
            "message": {
                "role": "assistant",
                "content": [{"type": "text", "text": "deep work done"}],
                "model": "claude-haiku-4-5",
                "stop_reason": "end_turn",
                "usage": {"input_tokens": 50, "output_tokens": 20}
            }
        });
        let bytes = serde_json::to_vec(&line).unwrap();
        let entry = parse_entry(&bytes).expect("must parse full depth-3 sidechain entry");
        assert_eq!(entry.entry_type, "assistant");
        assert!(entry.is_sidechain);
        assert_eq!(entry.agent_depth, Some(3));
        assert_eq!(entry.agent_name, "deep-worker");
        assert_eq!(entry.parent_agent_name, "mid-level-agent");
        assert_eq!(entry.message.model, "claude-haiku-4-5");
    }

    // --- Issue #168: v2.1.193+ auto-mode denial fields ---

    #[test]
    fn parse_entry_captures_reason_and_tool_name_from_denial_entry() {
        // v2.1.193+: top-level denial entries carry `reason` and `toolName` fields.
        // Both must be captured by the parser so classify() can build the notice string.
        let line = json!({
            "type": "auto-mode-denial",
            "uuid": "denial-entry-uuid-001",
            "timestamp": "2026-06-25T12:00:00Z",
            "reason": "Tool not allowed in auto mode",
            "toolName": "Bash"
        });
        let bytes = serde_json::to_vec(&line).unwrap();
        let entry = parse_entry(&bytes).expect("must parse auto-mode-denial entry");
        assert_eq!(entry.entry_type, "auto-mode-denial");
        assert_eq!(
            entry.reason, "Tool not allowed in auto mode",
            "reason must be captured from top-level `reason` field"
        );
        assert_eq!(
            entry.tool_name, "Bash",
            "tool_name must be captured from top-level `toolName` field"
        );
    }

    #[test]
    fn parse_entry_captures_checkpoint_uuid_on_compact_boundary() {
        // v2.1.191+: compact_boundary entries may carry checkpointUuid identifying the last
        // pre-clear message UUID so the chain resolver can handle /rewind anchors.
        let line = json!({
            "type": "system",
            "subtype": "compact_boundary",
            "uuid": "compact-boundary-rewind-uuid",
            "parentUuid": null,
            "logicalParentUuid": "last-pre-clear-uuid",
            "rewindable": true,
            "checkpointUuid": "last-pre-clear-uuid",
            "timestamp": "2026-06-24T10:02:00Z"
        });
        let bytes = serde_json::to_vec(&line).unwrap();
        let entry = parse_entry(&bytes).expect("must parse compact_boundary with rewind fields");
        assert_eq!(entry.subtype, "compact_boundary");
        assert!(entry.rewindable, "rewindable flag must be captured");
        assert_eq!(
            entry.checkpoint_uuid, "last-pre-clear-uuid",
            "checkpointUuid must be captured"
        );
        assert_eq!(
            entry.logical_parent_uuid, "last-pre-clear-uuid",
            "logicalParentUuid must still be captured"
        );
    }

    #[test]
    fn parse_entry_rewind_fields_default_to_empty_when_absent() {
        // Entries from before v2.1.191 (or non-rewind entries) have no rewind fields.
        // All three fields must default to their zero values.
        let line = json!({
            "type": "user",
            "uuid": "regular-no-rewind-uuid",
            "timestamp": "2026-06-24T10:03:00Z",
            "message": {"role": "user", "content": "Hello"}
        });
        let bytes = serde_json::to_vec(&line).unwrap();
        let entry = parse_entry(&bytes).expect("must parse regular entry without rewind fields");
        assert_eq!(
            entry.rewind_to_uuid, "",
            "rewindToUuid must default to empty string"
        );
        assert!(!entry.rewindable, "rewindable must default to false");
        assert_eq!(
            entry.checkpoint_uuid, "",
            "checkpointUuid must default to empty string"
        );
    }

    // --- Issue #170: v2.1.195+ background-agent session fields ---

    #[test]
    fn parse_entry_captures_background_agent_fields_v2_1_195() {
        // Background-agent sessions written by Claude Code v2.1.141+ carry several new
        // top-level fields on every entry: version (schema discriminant), entrypoint
        // (invocation method), sessionId, agentId, userType, and attributionSkill.
        // All must be captured by the parser so downstream code can use them.
        let line = json!({
            "type": "user",
            "uuid": "bg-user-uuid-001",
            "parentUuid": null,
            "isSidechain": false,
            "timestamp": "2026-06-26T10:00:00Z",
            "version": "2.1.195",
            "entrypoint": "sdk-ts",
            "sessionId": "ba0f2078-81b4-40ed-bb4a-c9a3758b968d",
            "agentId": "a783ece79822ccf59",
            "userType": "external",
            "attributionSkill": "my-skill",
            "message": {"role": "user", "content": "run the background task"}
        });
        let bytes = serde_json::to_vec(&line).unwrap();
        let entry = parse_entry(&bytes).expect("must parse background-agent user entry");
        assert_eq!(entry.version, "2.1.195");
        assert_eq!(entry.entrypoint, "sdk-ts");
        assert_eq!(entry.session_id, "ba0f2078-81b4-40ed-bb4a-c9a3758b968d");
        assert_eq!(entry.agent_id, "a783ece79822ccf59");
        assert_eq!(entry.user_type, "external");
        assert_eq!(
            entry.attribution_skill.as_deref(),
            Some("my-skill"),
            "attributionSkill must be captured"
        );
    }

    #[test]
    fn parse_entry_background_agent_fields_default_to_empty_when_absent() {
        // Entries from interactive CLI sessions (pre-v2.1.141 or non-SDK invocations)
        // have no background-agent fields — all must default gracefully.
        let line = json!({
            "type": "user",
            "uuid": "interactive-uuid-001",
            "timestamp": "2026-06-26T10:00:00Z",
            "message": {"role": "user", "content": "Hello"}
        });
        let bytes = serde_json::to_vec(&line).unwrap();
        let entry = parse_entry(&bytes).expect("must parse interactive CLI entry");
        assert!(
            entry.version.is_empty(),
            "version must be empty when absent"
        );
        assert!(
            entry.entrypoint.is_empty(),
            "entrypoint must be empty when absent"
        );
        assert!(
            entry.session_id.is_empty(),
            "sessionId must be empty when absent"
        );
        assert!(
            entry.agent_id.is_empty(),
            "agentId must be empty when absent"
        );
        assert!(
            entry.user_type.is_empty(),
            "userType must be empty when absent"
        );
        assert!(
            entry.attribution_skill.is_none(),
            "attributionSkill must be None when absent"
        );
    }

    #[test]
    fn parse_entry_rewind_pointer_with_null_rewind_to_uuid_succeeds() {
        // rewindToUuid:null must be treated as empty string (same null_as_default contract
        // as parentUuid and logicalParentUuid).
        let line = json!({
            "type": "rewind-pointer",
            "uuid": "rewind-ptr-null-uuid",
            "rewindToUuid": null,
            "timestamp": "2026-06-24T10:04:00Z"
        });
        let bytes = serde_json::to_vec(&line).unwrap();
        let entry = parse_entry(&bytes).expect("must parse rewind-pointer with null rewindToUuid");
        assert_eq!(entry.rewind_to_uuid, "");
    }

    #[test]
    fn parse_entry_attribution_skill_defaults_to_none_when_absent() {
        // attributionSkill is absent for agents launched directly (not via a skill).
        let line = json!({
            "type": "assistant",
            "uuid": "bg-assist-no-skill",
            "timestamp": "2026-06-26T10:00:01Z",
            "version": "2.1.195",
            "agentId": "b894ece79822ccf60",
            "userType": "external",
            "message": {
                "role": "assistant",
                "model": "claude-sonnet-4-6",
                "content": [{"type": "text", "text": "working"}],
                "stop_reason": "end_turn",
                "usage": {"input_tokens": 10, "output_tokens": 5}
            }
        });
        let bytes = serde_json::to_vec(&line).unwrap();
        let entry = parse_entry(&bytes).expect("must parse background-agent entry without skill");
        assert!(
            entry.attribution_skill.is_none(),
            "attributionSkill must be None when absent"
        );
        assert_eq!(entry.agent_id, "b894ece79822ccf60");
        assert_eq!(entry.user_type, "external");
    }

    #[test]
    fn parse_entry_last_prompt_type_captures_leaf_uuid_and_prompt() {
        // v2.1.195+: Claude Code writes a type:"last-prompt" checkpoint entry to persist
        // the most recent background-agent prompt for resume. The entry has leafUuid (the
        // conversation cursor) and lastPrompt (the prompt text) but no uuid or message —
        // parse_entry must return Some (leafUuid is not empty) and capture both fields.
        let line = json!({
            "type": "last-prompt",
            "lastPrompt": "run the background task and report results",
            "leafUuid": "6515b150-20de-4361-a676-54fcca4fdbaa",
            "sessionId": "ba0f2078-81b4-40ed-bb4a-c9a3758b968d"
        });
        let bytes = serde_json::to_vec(&line).unwrap();
        let entry = parse_entry(&bytes).expect("must parse last-prompt entry");
        assert_eq!(entry.entry_type, "last-prompt");
        assert_eq!(entry.leaf_uuid, "6515b150-20de-4361-a676-54fcca4fdbaa");
        assert_eq!(
            entry.last_prompt,
            "run the background task and report results"
        );
        assert_eq!(entry.session_id, "ba0f2078-81b4-40ed-bb4a-c9a3758b968d");
        assert!(
            entry.uuid.is_empty(),
            "last-prompt entries have no uuid field"
        );
    }

    #[test]
    fn parse_entry_last_prompt_without_leaf_uuid_returns_none() {
        // A last-prompt entry with no leafUuid (and no uuid) must be discarded —
        // it has no anchor in the conversation chain.
        let line = json!({
            "type": "last-prompt",
            "lastPrompt": "some prompt",
            "sessionId": "ba0f2078-81b4-40ed-bb4a-c9a3758b968d"
        });
        let bytes = serde_json::to_vec(&line).unwrap();
        assert!(
            parse_entry(&bytes).is_none(),
            "last-prompt with no leafUuid must return None"
        );
    }

    #[test]
    fn parse_entry_background_agent_assistant_with_all_new_fields() {
        // Full assistant entry as Claude Code v2.1.195 writes for a background agent.
        // Verifies all new background-agent fields round-trip correctly alongside the
        // existing token-usage and model fields.
        let line = json!({
            "type": "assistant",
            "uuid": "bg-assist-full-uuid",
            "parentUuid": "bg-user-uuid-001",
            "isSidechain": false,
            "timestamp": "2026-06-26T10:00:01Z",
            "version": "2.1.195",
            "entrypoint": "sdk-ts",
            "sessionId": "ba0f2078-81b4-40ed-bb4a-c9a3758b968d",
            "agentId": "a783ece79822ccf59",
            "userType": "external",
            "cwd": "/workspace/project",
            "gitBranch": "main",
            "requestId": "req-bg-001",
            "message": {
                "role": "assistant",
                "model": "claude-opus-4-7",
                "content": [{"type": "text", "text": "Background task complete."}],
                "stop_reason": "end_turn",
                "usage": {
                    "input_tokens": 100,
                    "output_tokens": 20,
                    "cache_read_input_tokens": 50,
                    "cache_creation_input_tokens": 10
                }
            }
        });
        let bytes = serde_json::to_vec(&line).unwrap();
        let entry = parse_entry(&bytes).expect("must parse full background-agent assistant entry");
        assert_eq!(entry.entry_type, "assistant");
        assert_eq!(entry.version, "2.1.195");
        assert_eq!(entry.entrypoint, "sdk-ts");
        assert_eq!(entry.session_id, "ba0f2078-81b4-40ed-bb4a-c9a3758b968d");
        assert_eq!(entry.agent_id, "a783ece79822ccf59");
        assert_eq!(entry.user_type, "external");
        assert_eq!(entry.cwd, "/workspace/project");
        assert_eq!(entry.git_branch, "main");
        assert_eq!(entry.message.model, "claude-opus-4-7");
        assert_eq!(entry.message.usage.input_tokens, 100);
        assert_eq!(entry.message.usage.output_tokens, 20);
    }

    // --- Issue #171: v2.1.186+ background subagent permission prompt attribution fields ---

    #[test]
    fn parse_entry_captures_source_agent_name_and_requesting_agent_uuid_v2_1_186() {
        // v2.1.186+: background subagent permission prompts surfaced in the main session JSONL
        // carry sourceAgentName and requestingAgentUuid so the UI can attribute the request.
        let line = json!({
            "type": "progress",
            "uuid": "perm-prompt-uuid-001",
            "timestamp": "2026-06-28T08:00:00Z",
            "sourceAgentName": "background-explore-agent",
            "requestingAgentUuid": "session-uuid-bg-001",
            "data": {
                "type": "hook_progress",
                "hookEvent": "PreToolUse",
                "hookName": "permission_check",
                "command": ""
            }
        });
        let bytes = serde_json::to_vec(&line).unwrap();
        let entry = parse_entry(&bytes).expect("must parse progress entry with attribution");
        assert_eq!(entry.source_agent_name, "background-explore-agent");
        assert_eq!(entry.requesting_agent_uuid, "session-uuid-bg-001");
    }

    #[test]
    fn parse_entry_source_agent_name_defaults_to_empty_when_absent() {
        // Regular hook entries produced by the main agent have no attribution fields.
        let line = json!({
            "type": "attachment",
            "uuid": "regular-hook-uuid",
            "timestamp": "2026-06-28T08:01:00Z",
            "attachment": {
                "hookEvent": "PostToolUse",
                "hookName": "format",
                "type": "hook_success"
            }
        });
        let bytes = serde_json::to_vec(&line).unwrap();
        let entry = parse_entry(&bytes).expect("must parse attachment entry without attribution");
        assert_eq!(
            entry.source_agent_name, "",
            "sourceAgentName must default to empty when absent"
        );
        assert_eq!(
            entry.requesting_agent_uuid, "",
            "requestingAgentUuid must default to empty when absent"
        );
    }

    // --- Issue #209: v2.1.212+ assistant message effort level in session transcripts ---

    #[test]
    fn parse_entry_assistant_message_captures_effort_level_v2_1_212() {
        // v2.1.212+: every assistant entry carries effort:{level:"..."} on the message object.
        // The parser must capture it so callers can inspect the effort level.
        let line = json!({
            "type": "assistant",
            "uuid": "assistant-effort-uuid",
            "timestamp": "2026-07-15T10:00:00Z",
            "message": {
                "role": "assistant",
                "content": [{"type": "text", "text": "Hello"}],
                "model": "claude-opus-4-7",
                "stop_reason": "end_turn",
                "usage": {"input_tokens": 10, "output_tokens": 5},
                "effort": {"level": "normal"}
            }
        });
        let bytes = serde_json::to_vec(&line).unwrap();
        let entry = parse_entry(&bytes).expect("must parse assistant entry with effort on message");
        let effort = entry
            .message
            .effort
            .expect("message.effort must be captured");
        assert_eq!(
            effort.get("level").and_then(|v| v.as_str()),
            Some("normal"),
            "message.effort.level must be 'normal'"
        );
    }

    #[test]
    fn parse_entry_assistant_message_effort_low_and_high_captured() {
        // Verify all three effort level values parse correctly on the message object.
        for level in &["low", "high"] {
            let line = json!({
                "type": "assistant",
                "uuid": format!("effort-{level}-uuid"),
                "timestamp": "2026-07-15T10:00:00Z",
                "message": {
                    "role": "assistant",
                    "content": [],
                    "model": "claude-sonnet-4-6",
                    "stop_reason": "end_turn",
                    "usage": {"input_tokens": 1, "output_tokens": 1},
                    "effort": {"level": level}
                }
            });
            let bytes = serde_json::to_vec(&line).unwrap();
            let entry =
                parse_entry(&bytes).expect("must parse assistant entry with effort on message");
            let effort = entry
                .message
                .effort
                .expect("message.effort must be captured");
            assert_eq!(
                effort.get("level").and_then(|v| v.as_str()),
                Some(*level),
                "message.effort.level must be '{level}'"
            );
        }
    }

    #[test]
    fn parse_entry_assistant_message_effort_defaults_to_none_when_absent() {
        // Pre-v2.1.212 assistant entries carry no effort field on the message object.
        // The parser must tolerate its absence and default to None.
        let line = json!({
            "type": "assistant",
            "uuid": "old-assistant-uuid",
            "timestamp": "2026-01-01T00:00:00Z",
            "message": {
                "role": "assistant",
                "content": [{"type": "text", "text": "Hello"}],
                "model": "claude-opus-4-7",
                "stop_reason": "end_turn",
                "usage": {"input_tokens": 10, "output_tokens": 5}
            }
        });
        let bytes = serde_json::to_vec(&line).unwrap();
        let entry =
            parse_entry(&bytes).expect("must parse pre-v2.1.212 assistant entry without effort");
        assert!(
            entry.message.effort.is_none(),
            "message.effort must be None when absent (pre-v2.1.212 entries)"
        );
    }
}
