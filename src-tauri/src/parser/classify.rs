use chrono::{DateTime, NaiveDateTime, TimeZone, Utc};
use serde::Serialize;
use serde_json::Value;

use super::entry::Entry;
use super::patterns::*;
use super::sanitize::*;

/// Usage holds token counts for a single API response.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Usage {
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_creation_tokens: i64,
}

impl Usage {
    pub fn total_tokens(&self) -> i64 {
        self.input_tokens + self.output_tokens + self.cache_read_tokens + self.cache_creation_tokens
    }
}

/// ToolCall is a tool invocation extracted from an assistant message.
#[derive(Debug, Clone, Serialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
}

/// ContentBlock represents a single content block from a message.
#[derive(Debug, Clone, Serialize, Default)]
pub struct ContentBlock {
    pub block_type: String,
    pub text: String,
    pub tool_id: String,
    pub tool_name: String,
    pub tool_input: Option<Value>,
    pub content: String,
    pub is_error: bool,
    pub teammate_id: String,
    pub teammate_color: String,
    /// For hook_event blocks: all key-value pairs from the hook attachment JSON.
    pub hook_metadata: Option<Value>,
    /// For tool_result blocks: raw content value before stringification.
    pub content_json: Option<Value>,
}

/// Classified message types.
#[derive(Debug, Clone)]
pub enum ClassifiedMsg {
    User(UserMsg),
    AI(AIMsg),
    System(SystemMsg),
    Teammate(TeammateMsg),
    Compact(CompactMsg),
    Hook(HookMsg),
}

#[derive(Debug, Clone)]
pub struct UserMsg {
    pub timestamp: DateTime<Utc>,
    pub text: String,
    pub permission_mode: String,
}

#[derive(Debug, Clone)]
pub struct AIMsg {
    pub timestamp: DateTime<Utc>,
    pub model: String,
    pub text: String,
    pub thinking_count: usize,
    pub tool_calls: Vec<ToolCall>,
    pub blocks: Vec<ContentBlock>,
    pub usage: Usage,
    pub stop_reason: String,
    pub is_meta: bool,
}

#[derive(Debug, Clone)]
pub struct SystemMsg {
    pub timestamp: DateTime<Utc>,
    pub output: String,
    pub is_error: bool,
}

#[derive(Debug, Clone)]
pub struct TeammateMsg {
    pub timestamp: DateTime<Utc>,
    pub text: String,
    pub teammate_id: String,
    pub color: String,
}

#[derive(Debug, Clone)]
pub struct CompactMsg {
    pub timestamp: DateTime<Utc>,
    pub text: String,
    /// True for away_summary (session recap); false for actual compaction events.
    pub is_recap: bool,
}

#[derive(Debug, Clone)]
pub struct HookMsg {
    pub timestamp: DateTime<Utc>,
    pub hook_event: String,
    pub hook_name: String,
    pub command: String,
    /// All key-value pairs from the hook attachment JSON (stdout, stderr, command,
    /// exitCode, durationMs, additionalContext, decision, reason, …).
    pub metadata: Option<Value>,
}

pub const SYSTEM_OUTPUT_TAGS: &[&str] = &[
    LOCAL_COMMAND_STDERR_TAG,
    LOCAL_COMMAND_STDOUT_TAG,
    "<local-command-caveat>",
    "<system-reminder>",
    BASH_STDOUT_TAG,
    BASH_STDERR_TAG,
    TASK_NOTIFICATION_TAG,
];

const NOISE_ENTRY_TYPES: &[&str] = &[
    "system",
    "file-history-snapshot",
    "queue-operation",
    "progress",
    // v2.1.118+: /fork writes a compact pointer entry (type:"fork-context-ref") instead of
    // duplicating the full parent conversation. The pointer has no message content — drop it
    // silently so it never appears in the conversation display.
    "fork-context-ref",
    // v2.1.154+: Dynamic Workflow lifecycle entries carry workflow state but no displayable
    // message content. Drop silently so they never appear in the conversation transcript.
    "workflow-start",
    "workflow-progress",
    "workflow-complete",
    "workflow-cancelled",
    "workflow-error",
];

const HARD_NOISE_TAGS: &[&str] = &["<local-command-caveat>", "<system-reminder>"];

const EMPTY_STDOUT: &str = "<local-command-stdout></local-command-stdout>";
const EMPTY_STDERR: &str = "<local-command-stderr></local-command-stderr>";

/// Parse an ISO 8601 timestamp. Returns epoch on failure.
pub fn parse_timestamp(s: &str) -> DateTime<Utc> {
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return dt.with_timezone(&Utc);
    }
    // Try without timezone
    if let Ok(naive) = NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.f") {
        return Utc.from_utc_datetime(&naive);
    }
    Utc::now() // fallback; ideally epoch but using now for simplicity
}

/// Classify maps a raw Entry to a ClassifiedMsg. Returns None for noise.
pub fn classify(e: Entry) -> Option<ClassifiedMsg> {
    if e.is_sidechain {
        return None;
    }

    let ts = parse_timestamp(&e.timestamp);

    // Rescue hook events from noise filter before discarding all "progress" entries.
    // All existing hooks use data.type="hook_progress", but guard on hookEvent presence
    // so that future hook types (e.g. TaskCreated added in v2.1.84, PreCompact in v2.1.105)
    // are also rescued without needing to enumerate data.type values.
    if e.entry_type == "progress" {
        if let Some(ref data) = e.data {
            let is_hook = data.get("type").and_then(|v| v.as_str()) == Some("hook_progress")
                || data.get("hookEvent").is_some();
            if is_hook {
                let hook_event = data
                    .get("hookEvent")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let hook_name = data
                    .get("hookName")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let command = data
                    .get("command")
                    .map(resolve_hook_output)
                    .unwrap_or_default();
                return Some(ClassifiedMsg::Hook(HookMsg {
                    timestamp: ts,
                    hook_event,
                    hook_name,
                    command,
                    metadata: None,
                }));
            }
        }
    }

    // Rescue hook-related system entries before the NOISE_ENTRY_TYPES filter drops them.
    if e.entry_type == "system" {
        match e.subtype.as_str() {
            // away_summary: written by v2.1.108+ when the user returns after being idle,
            // or when /recap is invoked manually. The recap text lives in the top-level
            // `content` field (not `message.content`). Display as a CompactMsg so it
            // appears inline in the transcript like a context summary.
            "away_summary" => {
                return Some(ClassifiedMsg::Compact(CompactMsg {
                    timestamp: ts,
                    text: e.content.clone(),
                    is_recap: true,
                }));
            }
            // stop_hook_summary: written every time Stop hooks run (success or failure).
            // hookInfos contains [{command, durationMs}, ...] for each hook that ran.
            "stop_hook_summary" if e.hook_count > 0 => {
                let hook_name = e
                    .hook_infos
                    .as_ref()
                    .and_then(|v| v.as_array())
                    .and_then(|arr| arr.first())
                    .and_then(|info| info.get("command"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                return Some(ClassifiedMsg::Hook(HookMsg {
                    timestamp: ts,
                    hook_event: "Stop".to_string(),
                    hook_name,
                    command: String::new(),
                    metadata: None,
                }));
            }
            // hook_progress: written in verbose/stream-json mode for mid-session hooks.
            "hook_progress" => {
                return Some(ClassifiedMsg::Hook(HookMsg {
                    timestamp: ts,
                    hook_event: e.hook_event.clone(),
                    hook_name: e.hook_name.clone(),
                    command: String::new(),
                    metadata: None,
                }));
            }
            // hookEvent present on any system entry (forward-compat for future hook types).
            _ if !e.hook_event.is_empty() => {
                return Some(ClassifiedMsg::Hook(HookMsg {
                    timestamp: ts,
                    hook_event: e.hook_event.clone(),
                    hook_name: e.hook_name.clone(),
                    command: String::new(),
                    metadata: None,
                }));
            }
            _ => {}
        }
    }

    // Rescue hook attachment entries for all non-Stop hook events (PreToolUse, PostToolUse,
    // UserPromptSubmit, Notification, SessionStart, PreCompact, etc.).
    // Claude Code writes these as: {type:"attachment", attachment:{type:"hook_success"|
    // "hook_non_blocking_error"|"hook_blocking_error"|"hook_cancelled"|..., hookEvent, hookName}}
    if e.entry_type == "attachment" {
        if let Some(ref att) = e.attachment {
            let hook_event = att.get("hookEvent").and_then(|v| v.as_str()).unwrap_or("");
            if !hook_event.is_empty() {
                let hook_name = att
                    .get("hookName")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                // For blocking errors, extract the error message as the command context.
                // From v2.1.89 these fields may be a {path, preview} file-reference object
                // instead of a plain string when output exceeds 50 K characters.
                let command = {
                    let blocking = att
                        .get("blockingError")
                        .and_then(|v| v.get("blockingError"))
                        .map(resolve_hook_output)
                        .unwrap_or_default();
                    if !blocking.trim().is_empty() {
                        blocking
                    } else {
                        att.get("stderr")
                            .map(resolve_hook_output)
                            .unwrap_or_default()
                    }
                }
                .trim()
                .to_string();
                // Store the whole attachment object so all key-value pairs are available:
                // type, hookName, hookEvent, command, stdout, stderr, exitCode, durationMs,
                // additionalContext (when stdout is parsed JSON), decision, reason, etc.
                // Any nested JSON strings (e.g. stdout) are left for the frontend to expand.
                let metadata = Some(att.clone());
                return Some(ClassifiedMsg::Hook(HookMsg {
                    timestamp: ts,
                    hook_event: hook_event.to_string(),
                    hook_name,
                    command,
                    metadata,
                }));
            }
        }
    }

    // Hard noise: structural metadata types.
    if NOISE_ENTRY_TYPES.contains(&e.entry_type.as_str()) {
        return None;
    }

    // Summary entries -> CompactMsg.
    if e.entry_type == "summary" {
        return Some(ClassifiedMsg::Compact(CompactMsg {
            timestamp: ts,
            text: e.summary.clone(),
            is_recap: false,
        }));
    }

    // Synthetic assistant messages.
    if e.entry_type == "assistant" && e.message.model == "<synthetic>" {
        return None;
    }

    let content_str = extract_text(&e.message.content);

    // Filter user-type noise.
    if e.entry_type == "user" && is_user_noise(&e.message.content, &content_str) {
        return None;
    }

    // "Stop hook feedback:" entries: isMeta user messages injected by Claude Code when
    // a Stop hook exits non-zero.  Format: "Stop hook feedback:\n[command]: output\n"
    // Classify as HookMsg so they appear with the other hook items, not as AI meta noise.
    if e.entry_type == "user" && e.is_meta {
        let trimmed = content_str.trim();
        if trimmed.starts_with("Stop hook feedback:") {
            let (hook_name, command) = parse_hook_feedback(trimmed);
            return Some(ClassifiedMsg::Hook(HookMsg {
                timestamp: ts,
                hook_event: "Stop".to_string(),
                hook_name,
                command,
                metadata: None,
            }));
        }
    }

    // Teammate messages.
    if e.entry_type == "user" {
        let trimmed = content_str.trim();
        if TEAMMATE_MESSAGE_RE.is_match(trimmed) {
            let inner = extract_teammate_content(trimmed);
            if TEAMMATE_PROTOCOL_RE.is_match(&inner) {
                return None;
            }
            let teammate_id = extract_teammate_id(trimmed);
            let color = extract_teammate_color(trimmed);
            let text = sanitize_content(&inner);
            return Some(ClassifiedMsg::Teammate(TeammateMsg {
                timestamp: ts,
                text,
                teammate_id,
                color,
            }));
        }
    }

    // System message: user entry starting with command output tag.
    if e.entry_type == "user" {
        let trimmed = content_str.trim();
        if trimmed.starts_with(LOCAL_COMMAND_STDOUT_TAG)
            || trimmed.starts_with(LOCAL_COMMAND_STDERR_TAG)
        {
            return Some(ClassifiedMsg::System(SystemMsg {
                timestamp: ts,
                output: extract_command_output(&content_str),
                is_error: false,
            }));
        }
        if trimmed.starts_with(BASH_STDOUT_TAG) || trimmed.starts_with(BASH_STDERR_TAG) {
            let stderr_content = RE_BASH_STDERR
                .captures(&content_str)
                .and_then(|c| c.get(1))
                .map(|m| m.as_str().trim().to_string())
                .unwrap_or_default();
            return Some(ClassifiedMsg::System(SystemMsg {
                timestamp: ts,
                output: extract_bash_output(&content_str),
                is_error: !stderr_content.is_empty(),
            }));
        }
        if trimmed.starts_with(TASK_NOTIFICATION_TAG) {
            let status = RE_TASK_NOTIFY_STATUS
                .captures(&content_str)
                .and_then(|c| c.get(1))
                .map(|m| m.as_str().trim().to_string())
                .unwrap_or_default();
            return Some(ClassifiedMsg::System(SystemMsg {
                timestamp: ts,
                output: extract_task_notification(&content_str),
                is_error: status == "failed",
            }));
        }
    }

    // ToolSearch results.
    if e.entry_type == "user" && content_str.trim() == "Tool loaded." {
        if let Some(names) = extract_tool_search_matches(&e.tool_use_result) {
            if !names.is_empty() {
                return Some(ClassifiedMsg::System(SystemMsg {
                    timestamp: ts,
                    output: format!("Loaded: {}", names.join(", ")),
                    is_error: false,
                }));
            }
        }
    }

    // Compact summary: user entries with isCompactSummary:true are injected by Claude Code
    // after a compaction event and contain an AI-generated summary of the compacted messages.
    // Classify as CompactMsg so they render as a styled separator rather than a user turn.
    if e.entry_type == "user" && e.is_compact_summary {
        return Some(ClassifiedMsg::Compact(CompactMsg {
            timestamp: ts,
            text: sanitize_content(&content_str),
            is_recap: false,
        }));
    }

    // User message.
    if e.entry_type == "user" && !e.is_meta {
        let trimmed = content_str.trim();
        let excluded = SYSTEM_OUTPUT_TAGS
            .iter()
            .any(|tag| trimmed.starts_with(tag));
        if !excluded && has_user_content(&e.message.content, &content_str) {
            return Some(ClassifiedMsg::User(UserMsg {
                timestamp: ts,
                text: sanitize_content(&content_str),
                permission_mode: e.permission_mode.clone(),
            }));
        }
    }

    // AI message (assistant).
    if e.entry_type == "assistant" {
        // v2.1.166+: when fallbackModel retries a failed turn, Claude Code writes a stub assistant
        // entry with null or empty content for the failed primary attempt before writing the
        // successful fallback response. Skip these stubs to avoid emitting empty AI turns.
        let content_is_empty = match &e.message.content {
            None => true,
            Some(Value::Array(arr)) => arr.is_empty(),
            _ => false,
        };
        if content_is_empty {
            return None;
        }
        let (thinking, tool_calls, blocks) = extract_assistant_details(&e.message.content);
        let stop_reason = e.message.stop_reason.clone().unwrap_or_default();
        return Some(ClassifiedMsg::AI(AIMsg {
            timestamp: ts,
            model: e.message.model.clone(),
            text: sanitize_content(&extract_text(&e.message.content)),
            thinking_count: thinking,
            tool_calls,
            blocks,
            usage: Usage {
                input_tokens: e.message.usage.input_tokens,
                output_tokens: e.message.usage.output_tokens,
                cache_read_tokens: e.message.usage.cache_read_input_tokens,
                cache_creation_tokens: e.message.usage.effective_cache_creation_input_tokens(),
            },
            stop_reason,
            is_meta: false,
        }));
    }

    // Unknown entry types with no message role (e.g. rate_limit_event, CwdChanged,
    // FileChanged, --channels injected entries) are structural metadata — drop them.
    if e.message.role.is_empty() {
        return None;
    }

    // Fallback: entries with an unrecognised type but a message role -> meta AI message.
    let blocks = extract_meta_blocks(&e.message.content, &content_str);
    Some(ClassifiedMsg::AI(AIMsg {
        timestamp: ts,
        model: String::new(),
        text: content_str,
        thinking_count: 0,
        tool_calls: Vec::new(),
        blocks,
        usage: Usage::default(),
        stop_reason: String::new(),
        is_meta: true,
    }))
}

fn extract_teammate_id(s: &str) -> String {
    TEAMMATE_ID_RE
        .captures(s)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
        .unwrap_or_default()
}

fn extract_teammate_color(s: &str) -> String {
    TEAMMATE_COLOR_RE
        .captures(s)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
        .unwrap_or_default()
}

fn extract_teammate_content(s: &str) -> String {
    TEAMMATE_CONTENT_RE
        .captures(s)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().trim().to_string())
        .unwrap_or_else(|| s.to_string())
}

/// Parse a "Stop hook feedback:\n[command]: output\n" string into (hook_name, command).
fn parse_hook_feedback(s: &str) -> (String, String) {
    // Skip the first line ("Stop hook feedback:"), then parse "[command]: output" lines.
    let body = s
        .split_once('\n')
        .map(|x| x.1)
        .unwrap_or("")
        .trim()
        .to_string();
    // Format: "[~/.claude/script.sh]: error message"
    if let Some(rest) = body.strip_prefix('[') {
        if let Some(bracket_end) = rest.find("]: ") {
            let hook_name = rest[..bracket_end].to_string();
            let command = rest[bracket_end + 3..].trim().to_string();
            return (hook_name, command);
        }
    }
    (String::new(), body)
}

fn is_user_noise(raw: &Option<Value>, content_str: &str) -> bool {
    let trimmed = content_str.trim();

    for tag in HARD_NOISE_TAGS {
        let close_tag = tag.replace('<', "</");
        if trimmed.starts_with(tag) && trimmed.ends_with(&close_tag) {
            return true;
        }
    }

    if trimmed == EMPTY_STDOUT || trimmed == EMPTY_STDERR {
        return true;
    }

    if trimmed.starts_with("[Request interrupted by user") {
        return true;
    }

    // Check array interruption
    if let Some(Value::Array(blocks)) = raw {
        if blocks.len() == 1 {
            if let Some(block) = blocks.first() {
                let bt = block.get("type").and_then(|v| v.as_str()).unwrap_or("");
                let text = block.get("text").and_then(|v| v.as_str()).unwrap_or("");
                if bt == "text" && text.starts_with("[Request interrupted by user") {
                    return true;
                }
            }
        }
    }
    false
}

fn has_user_content(raw: &Option<Value>, str_content: &str) -> bool {
    match raw {
        Some(Value::String(_)) => !str_content.trim().is_empty(),
        Some(Value::Array(blocks)) => blocks.iter().any(|b| {
            let bt = b.get("type").and_then(|v| v.as_str()).unwrap_or("");
            bt == "text" || bt == "image" || bt == "document"
        }),
        _ => false,
    }
}

fn extract_tool_search_matches(raw: &Option<Value>) -> Option<Vec<String>> {
    let val = raw.as_ref()?;
    let matches = val.get("matches")?;
    let arr = matches.as_array()?;
    let names: Vec<String> = arr
        .iter()
        .filter_map(|v| v.as_str().map(|s| s.to_string()))
        .collect();
    if names.is_empty() {
        None
    } else {
        Some(names)
    }
}

/// Normalizes a `tool_use.input` value to handle the pre-v2.1.92 streaming bug where
/// array/object fields were emitted as JSON-encoded strings instead of native JSON types.
///
/// For example, `"env": "[\"KEY=val\"]"` is parsed back to `"env": ["KEY=val"]`.
/// Values that are already arrays or objects, or strings that don't parse as
/// arrays/objects, are left unchanged.
fn normalize_tool_input(input: Value) -> Value {
    match input {
        Value::Object(mut map) => {
            for val in map.values_mut() {
                if let Value::String(s) = val {
                    let trimmed = s.trim_start();
                    if trimmed.starts_with('[') || trimmed.starts_with('{') {
                        if let Ok(parsed) = serde_json::from_str::<Value>(s) {
                            if matches!(parsed, Value::Array(_) | Value::Object(_)) {
                                *val = parsed;
                            }
                        }
                    }
                }
            }
            Value::Object(map)
        }
        other => other,
    }
}

fn extract_assistant_details(content: &Option<Value>) -> (usize, Vec<ToolCall>, Vec<ContentBlock>) {
    let blocks = match content {
        Some(Value::Array(arr)) => arr,
        _ => return (0, Vec::new(), Vec::new()),
    };

    let mut thinking = 0;
    let mut calls = Vec::new();
    let mut content_blocks = Vec::new();

    for b in blocks {
        let bt = b.get("type").and_then(|v| v.as_str()).unwrap_or("");
        match bt {
            "thinking" => {
                thinking += 1;
                content_blocks.push(ContentBlock {
                    block_type: "thinking".to_string(),
                    text: b
                        .get("thinking")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    ..Default::default()
                });
            }
            "text" => {
                content_blocks.push(ContentBlock {
                    block_type: "text".to_string(),
                    text: b
                        .get("text")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    ..Default::default()
                });
            }
            "tool_use" => {
                let id = b
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let name = b
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                if !id.is_empty() && !name.is_empty() {
                    calls.push(ToolCall {
                        id: id.clone(),
                        name: name.clone(),
                    });
                }
                content_blocks.push(ContentBlock {
                    block_type: "tool_use".to_string(),
                    tool_id: id,
                    tool_name: name,
                    tool_input: b.get("input").cloned().map(normalize_tool_input),
                    ..Default::default()
                });
            }
            _ => {
                content_blocks.push(ContentBlock {
                    block_type: bt.to_string(),
                    text: b
                        .get("text")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    ..Default::default()
                });
            }
        }
    }

    (thinking, calls, content_blocks)
}

fn extract_meta_blocks(content: &Option<Value>, text_fallback: &str) -> Vec<ContentBlock> {
    let blocks = match content {
        Some(Value::Array(arr)) => arr,
        _ => {
            return vec![ContentBlock {
                block_type: "text".to_string(),
                text: text_fallback.to_string(),
                ..Default::default()
            }];
        }
    };

    let has_tool_result = blocks
        .iter()
        .any(|b| b.get("type").and_then(|v| v.as_str()) == Some("tool_result"));

    if !has_tool_result {
        return vec![ContentBlock {
            block_type: "text".to_string(),
            text: text_fallback.to_string(),
            ..Default::default()
        }];
    }

    blocks
        .iter()
        .filter_map(|b| {
            let bt = b.get("type").and_then(|v| v.as_str())?;
            if bt != "tool_result" {
                return None;
            }
            let tool_id = b
                .get("tool_use_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let raw_content_val = b.get("content").cloned();
            let raw_content = stringify_content(&raw_content_val);
            let content = resolve_persisted_output(&raw_content);
            let is_error = b.get("is_error").and_then(|v| v.as_bool()).unwrap_or(false);
            // Keep the raw Value so callers can access all key-value pairs when the
            // tool result is a JSON object (not just the stringified form).
            let content_json = raw_content_val.filter(|v| v.is_object() || v.is_array());
            Some(ContentBlock {
                block_type: "tool_result".to_string(),
                tool_id,
                content,
                is_error,
                content_json,
                ..Default::default()
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::super::entry::{Entry, EntryMessage};
    use super::*;
    use serde_json::json;

    fn make_entry(entry_type: &str, content: Option<Value>) -> Entry {
        Entry {
            entry_type: entry_type.to_string(),
            uuid: "test-uuid".to_string(),
            timestamp: "2025-01-15T10:30:00Z".to_string(),
            message: EntryMessage {
                content,
                ..Default::default()
            },
            ..Default::default()
        }
    }

    // --- parse_timestamp tests ---

    #[test]
    fn parse_timestamp_valid_rfc3339() {
        let ts = parse_timestamp("2025-01-15T10:30:00Z");
        assert_eq!(
            ts.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            "2025-01-15T10:30:00Z"
        );
    }

    #[test]
    fn parse_timestamp_valid_naive() {
        let ts = parse_timestamp("2025-01-15T10:30:00.000");
        assert_eq!(ts.format("%Y-%m-%d").to_string(), "2025-01-15");
    }

    #[test]
    fn parse_timestamp_invalid_returns_recent() {
        let before = chrono::Utc::now();
        let ts = parse_timestamp("not-a-date");
        let after = chrono::Utc::now();
        // Should return approximately now (fallback)
        assert!(ts >= before && ts <= after);
    }

    // --- classify tests ---

    #[test]
    fn classify_returns_none_for_sidechain() {
        let mut e = make_entry("user", Some(json!("hello")));
        e.is_sidechain = true;
        assert!(classify(e).is_none());
    }

    #[test]
    fn classify_returns_none_for_noise_entry_types() {
        for noise_type in &[
            "system",
            "file-history-snapshot",
            "queue-operation",
            "progress",
        ] {
            let e = make_entry(noise_type, Some(json!("content")));
            assert!(
                classify(e).is_none(),
                "Expected None for entry_type={}",
                noise_type
            );
        }
    }

    #[test]
    fn classify_returns_compact_for_summary() {
        let mut e = make_entry("summary", None);
        e.summary = "Session summary text".to_string();
        match classify(e) {
            Some(ClassifiedMsg::Compact(c)) => {
                assert_eq!(c.text, "Session summary text");
            }
            other => panic!("Expected Compact, got {:?}", other),
        }
    }

    #[test]
    fn classify_returns_none_for_synthetic_assistant() {
        let mut e = make_entry("assistant", Some(json!([{"type": "text", "text": "hi"}])));
        e.message.model = "<synthetic>".to_string();
        assert!(classify(e).is_none());
    }

    // --- Issue #124: v2.1.166+ fallbackModel retry stub entries ---

    #[test]
    fn classify_returns_none_for_assistant_with_null_content() {
        // v2.1.166+: when the primary model fails and fallbackModel retries the turn, Claude Code
        // writes a stub assistant entry with null content for the failed attempt. classify must
        // return None so no empty AI turn appears in the conversation display.
        let mut e = make_entry("assistant", None);
        e.message.model = "claude-opus-4-7".to_string();
        assert!(
            classify(e).is_none(),
            "assistant entry with null content must be dropped (fallback retry stub)"
        );
    }

    #[test]
    fn classify_returns_none_for_assistant_with_empty_content_array() {
        // v2.1.166+: the failed primary attempt may also carry an empty content array []
        // instead of null. classify must return None in this case too.
        let mut e = make_entry("assistant", Some(json!([])));
        e.message.model = "claude-opus-4-7".to_string();
        assert!(
            classify(e).is_none(),
            "assistant entry with empty content array must be dropped (fallback retry stub)"
        );
    }

    #[test]
    fn classify_returns_ai_for_fallback_model_assistant_with_real_content() {
        // v2.1.166+: the successful fallback response has a different model ID but real content.
        // classify must emit an AIMsg with the fallback model's ID so per-turn model display
        // is accurate — not the session's primary model.
        let content = json!([{"type": "text", "text": "Here is the answer"}]);
        let mut e = make_entry("assistant", Some(content));
        e.message.model = "claude-haiku-4-5".to_string();
        e.message.stop_reason = Some("end_turn".to_string());
        match classify(e) {
            Some(ClassifiedMsg::AI(ai)) => {
                assert_eq!(
                    ai.model, "claude-haiku-4-5",
                    "fallback model must be preserved per-entry, not overridden by primary model"
                );
                assert!(ai.text.contains("Here is the answer"));
            }
            other => panic!(
                "Expected AI for fallback response with content, got {:?}",
                other
            ),
        }
    }

    #[test]
    fn classify_returns_user_msg_for_regular_user() {
        let e = make_entry("user", Some(json!("Hello Claude")));
        match classify(e) {
            Some(ClassifiedMsg::User(u)) => {
                assert!(u.text.contains("Hello Claude"));
            }
            other => panic!("Expected User, got {:?}", other),
        }
    }

    #[test]
    fn classify_returns_ai_msg_for_assistant_with_tool_calls_and_thinking() {
        let content = json!([
            {"type": "thinking", "thinking": "Let me think..."},
            {"type": "text", "text": "Here is my response"},
            {"type": "tool_use", "id": "tool1", "name": "Bash", "input": {"command": "ls"}}
        ]);
        let mut e = make_entry("assistant", Some(content));
        e.message.model = "claude-sonnet-4-20250514".to_string();
        e.message.stop_reason = Some("tool_use".to_string());
        match classify(e) {
            Some(ClassifiedMsg::AI(ai)) => {
                assert_eq!(ai.thinking_count, 1);
                assert_eq!(ai.tool_calls.len(), 1);
                assert_eq!(ai.tool_calls[0].name, "Bash");
                assert_eq!(ai.model, "claude-sonnet-4-20250514");
                assert_eq!(ai.stop_reason, "tool_use");
            }
            other => panic!("Expected AI, got {:?}", other),
        }
    }

    #[test]
    fn classify_returns_system_msg_for_stdout_tag() {
        let content = format!("<local-command-stdout>file1.txt\nfile2.txt</local-command-stdout>");
        let e = make_entry("user", Some(json!(content)));
        match classify(e) {
            Some(ClassifiedMsg::System(_)) => {}
            other => panic!("Expected System, got {:?}", other),
        }
    }

    #[test]
    fn classify_returns_system_msg_for_bash_stdout() {
        let content = "<bash-stdout>output here</bash-stdout>";
        let e = make_entry("user", Some(json!(content)));
        match classify(e) {
            Some(ClassifiedMsg::System(s)) => {
                assert!(!s.output.is_empty());
            }
            other => panic!("Expected System, got {:?}", other),
        }
    }

    #[test]
    fn classify_returns_system_msg_for_task_notification() {
        let content = "<task-notification><summary>Task done</summary><status>completed</status></task-notification>";
        let e = make_entry("user", Some(json!(content)));
        match classify(e) {
            Some(ClassifiedMsg::System(s)) => {
                assert!(!s.is_error);
            }
            other => panic!("Expected System, got {:?}", other),
        }
    }

    #[test]
    fn classify_returns_teammate_for_teammate_message() {
        let content = r##"<teammate-message teammate_id="worker1" color="#ff0000">Hello from worker</teammate-message>"##;
        let e = make_entry("user", Some(json!(content)));
        match classify(e) {
            Some(ClassifiedMsg::Teammate(t)) => {
                assert_eq!(t.teammate_id, "worker1");
                assert_eq!(t.color, "#ff0000");
                assert!(t.text.contains("Hello from worker"));
            }
            other => panic!("Expected Teammate, got {:?}", other),
        }
    }

    #[test]
    fn classify_returns_none_for_teammate_protocol_messages() {
        let content = r##"<teammate-message teammate_id="worker1" color="#ff0000">{"type": "idle_notification"}</teammate-message>"##;
        let e = make_entry("user", Some(json!(content)));
        assert!(classify(e).is_none());
    }

    #[test]
    fn classify_returns_none_for_user_noise_system_reminder_only() {
        let content = "<system-reminder>some reminder</system-reminder>";
        let e = make_entry("user", Some(json!(content)));
        assert!(classify(e).is_none());
    }

    #[test]
    fn classify_returns_none_for_empty_stdout() {
        let content = "<local-command-stdout></local-command-stdout>";
        let e = make_entry("user", Some(json!(content)));
        assert!(classify(e).is_none());
    }

    #[test]
    fn classify_task_notification_killed_not_error() {
        let content = "<task-notification><summary>Background command \"Start sso-server\" was stopped</summary><status>killed</status></task-notification>";
        let e = make_entry("user", Some(json!(content)));
        match classify(e) {
            Some(ClassifiedMsg::System(s)) => {
                assert!(!s.is_error, "killed (user-stopped) should not be an error");
            }
            other => panic!("Expected System, got {:?}", other),
        }
    }

    #[test]
    fn classify_task_notification_failed_is_error() {
        let content = "<task-notification><summary>Background command failed</summary><status>failed</status></task-notification>";
        let e = make_entry("user", Some(json!(content)));
        match classify(e) {
            Some(ClassifiedMsg::System(s)) => {
                assert!(s.is_error, "failed status should be an error");
            }
            other => panic!("Expected System with is_error, got {:?}", other),
        }
    }

    // --- Hook event compat tests (v2.1.84+) ---

    #[test]
    fn classify_rescues_hook_progress_with_any_hook_event() {
        // Existing behaviour: hook_progress entries are rescued regardless of hookEvent value.
        let e = Entry {
            entry_type: "progress".to_string(),
            uuid: "uuid-hook".to_string(),
            timestamp: "2025-01-15T10:30:00Z".to_string(),
            data: Some(json!({
                "type": "hook_progress",
                "hookEvent": "PostToolUse",
                "hookName": "my-hook",
                "command": "echo done"
            })),
            ..Default::default()
        };
        match classify(e) {
            Some(ClassifiedMsg::Hook(h)) => {
                assert_eq!(h.hook_event, "PostToolUse");
                assert_eq!(h.hook_name, "my-hook");
            }
            other => panic!("Expected Hook, got {:?}", other),
        }
    }

    #[test]
    fn classify_rescues_progress_with_hook_event_field_regardless_of_data_type() {
        // Forward-compat: a future hook type (e.g. TaskCreated, v2.1.84) may arrive with a
        // data.type other than "hook_progress" but still carry a hookEvent field.
        // The rescue must fire based on hookEvent presence, not data.type alone.
        let e = Entry {
            entry_type: "progress".to_string(),
            uuid: "uuid-task-created".to_string(),
            timestamp: "2025-01-15T10:30:00Z".to_string(),
            data: Some(json!({
                "type": "task_hook",
                "hookEvent": "TaskCreated",
                "hookName": "on-task",
                "command": "echo task"
            })),
            ..Default::default()
        };
        match classify(e) {
            Some(ClassifiedMsg::Hook(h)) => {
                assert_eq!(h.hook_event, "TaskCreated");
            }
            other => panic!(
                "Expected Hook for hookEvent-bearing progress entry, got {:?}",
                other
            ),
        }
    }

    #[test]
    fn classify_drops_progress_entry_without_hook_event() {
        // Non-hook progress entries (agent_progress, bash_progress, etc.) must remain noise.
        let e = Entry {
            entry_type: "progress".to_string(),
            uuid: "uuid-agent-progress".to_string(),
            timestamp: "2025-01-15T10:30:00Z".to_string(),
            data: Some(json!({"type": "agent_progress", "message": "thinking..."})),
            ..Default::default()
        };
        assert!(
            classify(e).is_none(),
            "Non-hook progress entry must be dropped"
        );
    }

    #[test]
    fn classify_rescues_system_hook_progress_subtype() {
        // Verbose/stream-json mode: hooks arrive as type:"system", subtype:"hook_progress".
        // These must be rescued before the noise filter discards all "system" entries.
        let e = Entry {
            entry_type: "system".to_string(),
            uuid: "uuid-sys-hook".to_string(),
            timestamp: "2025-01-15T10:30:00Z".to_string(),
            subtype: "hook_progress".to_string(),
            hook_event: "PreToolUse".to_string(),
            hook_name: "pre-commit".to_string(),
            ..Default::default()
        };
        match classify(e) {
            Some(ClassifiedMsg::Hook(h)) => {
                assert_eq!(h.hook_event, "PreToolUse");
                assert_eq!(h.hook_name, "pre-commit");
            }
            other => panic!(
                "Expected Hook for system/hook_progress entry, got {:?}",
                other
            ),
        }
    }

    #[test]
    fn classify_rescues_system_entry_with_hook_event_field() {
        // Forward-compat: any system entry carrying a hookEvent field is treated as a hook.
        let e = Entry {
            entry_type: "system".to_string(),
            uuid: "uuid-sys-hook2".to_string(),
            timestamp: "2025-01-15T10:30:00Z".to_string(),
            hook_event: "PostToolUse".to_string(),
            hook_name: "post-hook".to_string(),
            ..Default::default()
        };
        match classify(e) {
            Some(ClassifiedMsg::Hook(h)) => {
                assert_eq!(h.hook_event, "PostToolUse");
            }
            other => panic!(
                "Expected Hook for system entry with hookEvent, got {:?}",
                other
            ),
        }
    }

    #[test]
    fn classify_drops_plain_system_entry_without_hook_fields() {
        // Regular system entries (no subtype/hookEvent) must still be dropped as noise.
        let e = Entry {
            entry_type: "system".to_string(),
            uuid: "uuid-plain-sys".to_string(),
            timestamp: "2025-01-15T10:30:00Z".to_string(),
            ..Default::default()
        };
        assert!(
            classify(e).is_none(),
            "Plain system entry must remain noise"
        );
    }

    #[test]
    fn classify_rescues_stop_hook_summary_as_hook() {
        // stop_hook_summary is written every time Stop hooks run (even on success).
        // It must be rescued and shown as a HookMsg so hooks always appear in the transcript.
        let e = Entry {
            entry_type: "system".to_string(),
            uuid: "uuid-stop-hook-summary".to_string(),
            timestamp: "2025-01-15T10:30:00Z".to_string(),
            subtype: "stop_hook_summary".to_string(),
            hook_count: 1,
            hook_infos: Some(json!([{
                "command": "~/.claude/stop-hook-git-check.sh",
                "durationMs": 59
            }])),
            ..Default::default()
        };
        match classify(e) {
            Some(ClassifiedMsg::Hook(h)) => {
                assert_eq!(h.hook_event, "Stop");
                assert_eq!(h.hook_name, "~/.claude/stop-hook-git-check.sh");
            }
            other => panic!("Expected Hook for stop_hook_summary entry, got {:?}", other),
        }
    }

    #[test]
    fn classify_drops_stop_hook_summary_with_zero_hooks() {
        // stop_hook_summary with hookCount=0 means no hooks ran; drop silently.
        let e = Entry {
            entry_type: "system".to_string(),
            uuid: "uuid-stop-hook-empty".to_string(),
            timestamp: "2025-01-15T10:30:00Z".to_string(),
            subtype: "stop_hook_summary".to_string(),
            hook_count: 0,
            ..Default::default()
        };
        assert!(
            classify(e).is_none(),
            "stop_hook_summary with hookCount=0 must be dropped"
        );
    }

    #[test]
    fn classify_rescues_stop_hook_feedback_user_entry_as_hook() {
        // "Stop hook feedback:" user entries (isMeta=true) are injected by Claude Code when
        // a Stop hook exits non-zero.  Classify as HookMsg instead of fallthrough meta AI.
        let e = Entry {
            entry_type: "user".to_string(),
            uuid: "uuid-hook-feedback".to_string(),
            timestamp: "2025-01-15T10:30:00Z".to_string(),
            is_meta: true,
            message: super::super::entry::EntryMessage {
                role: "user".to_string(),
                content: Some(json!(
                    "Stop hook feedback:\n[~/.claude/stop-hook-git-check.sh]: There are untracked files.\n"
                )),
                ..Default::default()
            },
            ..Default::default()
        };
        match classify(e) {
            Some(ClassifiedMsg::Hook(h)) => {
                assert_eq!(h.hook_event, "Stop");
                assert_eq!(h.hook_name, "~/.claude/stop-hook-git-check.sh");
                assert!(
                    h.command.contains("untracked"),
                    "command should contain hook output"
                );
            }
            other => panic!(
                "Expected Hook for stop hook feedback entry, got {:?}",
                other
            ),
        }
    }

    #[test]
    fn classify_rescues_attachment_hook_success() {
        // PreToolUse/PostToolUse/UserPromptSubmit/etc. hooks are written as attachment entries.
        let e = Entry {
            entry_type: "attachment".to_string(),
            uuid: "uuid-att-hook".to_string(),
            timestamp: "2025-01-15T10:30:00Z".to_string(),
            attachment: Some(json!({
                "type": "hook_success",
                "hookEvent": "PreToolUse",
                "hookName": "my-pre-hook",
                "toolUseID": "tool-123",
                "content": "Success"
            })),
            ..Default::default()
        };
        match classify(e) {
            Some(ClassifiedMsg::Hook(h)) => {
                assert_eq!(h.hook_event, "PreToolUse");
                assert_eq!(h.hook_name, "my-pre-hook");
            }
            other => panic!(
                "Expected Hook for attachment/hook_success entry, got {:?}",
                other
            ),
        }
    }

    #[test]
    fn classify_rescues_attachment_hook_blocking_error_with_message() {
        // hook_blocking_error extracts the error message into command field.
        let e = Entry {
            entry_type: "attachment".to_string(),
            uuid: "uuid-att-block".to_string(),
            timestamp: "2025-01-15T10:30:00Z".to_string(),
            attachment: Some(json!({
                "type": "hook_blocking_error",
                "hookEvent": "PostToolUse",
                "hookName": "post-lint",
                "blockingError": {"blockingError": "Lint failed: unused variable"}
            })),
            ..Default::default()
        };
        match classify(e) {
            Some(ClassifiedMsg::Hook(h)) => {
                assert_eq!(h.hook_event, "PostToolUse");
                assert_eq!(h.hook_name, "post-lint");
                assert!(h.command.contains("Lint failed"));
            }
            other => panic!(
                "Expected Hook for attachment/hook_blocking_error, got {:?}",
                other
            ),
        }
    }

    #[test]
    fn classify_drops_attachment_without_hook_event() {
        // Non-hook attachments (file attachments, etc.) must not be shown as hooks.
        let e = Entry {
            entry_type: "attachment".to_string(),
            uuid: "uuid-att-file".to_string(),
            timestamp: "2025-01-15T10:30:00Z".to_string(),
            attachment: Some(json!({
                "type": "file",
                "filename": "README.md",
                "content": "# readme"
            })),
            ..Default::default()
        };
        assert!(classify(e).is_none(), "Non-hook attachment must be dropped");
    }

    // --- Hook output compat tests (v2.1.89+) ---

    #[test]
    fn classify_progress_hook_with_structured_command_returns_preview() {
        // v2.1.89: hook stdout >50K is stored as {path, preview} object instead of a plain string.
        // When the file is absent (tmp file already cleaned up), the preview must be returned.
        let e = Entry {
            entry_type: "progress".to_string(),
            uuid: "uuid-hook-large".to_string(),
            timestamp: "2025-01-15T10:30:00Z".to_string(),
            data: Some(json!({
                "type": "hook_progress",
                "hookEvent": "PostToolUse",
                "hookName": "my-large-hook",
                "command": {"path": "/tmp/nonexistent_hook_12345.txt", "preview": "large output preview"}
            })),
            ..Default::default()
        };
        match classify(e) {
            Some(ClassifiedMsg::Hook(h)) => {
                assert_eq!(h.hook_event, "PostToolUse");
                assert_eq!(h.hook_name, "my-large-hook");
                assert_eq!(h.command, "large output preview");
            }
            other => panic!("Expected Hook with preview, got {:?}", other),
        }
    }

    #[test]
    fn classify_progress_hook_with_structured_command_reads_file_when_present() {
        // When the file is still on disk, the full content must be returned.
        let dir = std::env::temp_dir();
        let path = dir.join("test_classify_hook_large.txt");
        std::fs::write(&path, "full large hook output").unwrap();
        let e = Entry {
            entry_type: "progress".to_string(),
            uuid: "uuid-hook-file".to_string(),
            timestamp: "2025-01-15T10:30:00Z".to_string(),
            data: Some(json!({
                "type": "hook_progress",
                "hookEvent": "PreToolUse",
                "hookName": "pre-hook",
                "command": {"path": path.to_str().unwrap(), "preview": "truncated..."}
            })),
            ..Default::default()
        };
        match classify(e) {
            Some(ClassifiedMsg::Hook(h)) => {
                assert_eq!(h.command, "full large hook output");
            }
            other => panic!("Expected Hook, got {:?}", other),
        }
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn classify_attachment_blocking_error_with_structured_blocking_error_field() {
        // v2.1.89: blockingError.blockingError may be a {path, preview} object.
        let e = Entry {
            entry_type: "attachment".to_string(),
            uuid: "uuid-att-large-block".to_string(),
            timestamp: "2025-01-15T10:30:00Z".to_string(),
            attachment: Some(json!({
                "type": "hook_blocking_error",
                "hookEvent": "PostToolUse",
                "hookName": "post-lint",
                "blockingError": {
                    "blockingError": {"path": "/tmp/nonexistent_block.txt", "preview": "lint error preview"}
                }
            })),
            ..Default::default()
        };
        match classify(e) {
            Some(ClassifiedMsg::Hook(h)) => {
                assert_eq!(h.hook_event, "PostToolUse");
                assert_eq!(h.command, "lint error preview");
            }
            other => panic!("Expected Hook, got {:?}", other),
        }
    }

    #[test]
    fn classify_attachment_blocking_error_with_structured_stderr_field() {
        // v2.1.89: stderr may also be a {path, preview} object when output is large.
        let e = Entry {
            entry_type: "attachment".to_string(),
            uuid: "uuid-att-large-stderr".to_string(),
            timestamp: "2025-01-15T10:30:00Z".to_string(),
            attachment: Some(json!({
                "type": "hook_non_blocking_error",
                "hookEvent": "PreToolUse",
                "hookName": "pre-check",
                "stderr": {"path": "/tmp/nonexistent_stderr.txt", "preview": "stderr preview text"}
            })),
            ..Default::default()
        };
        match classify(e) {
            Some(ClassifiedMsg::Hook(h)) => {
                assert_eq!(h.hook_event, "PreToolUse");
                assert_eq!(h.command, "stderr preview text");
            }
            other => panic!("Expected Hook, got {:?}", other),
        }
    }

    // --- Unknown / structural entry type tests (compat: v2.1.79-v2.1.83) ---

    #[test]
    fn classify_drops_rate_limit_event_silently() {
        // rate_limit_event has a uuid but no message role — must be dropped, not shown as AI.
        let mut e = Entry {
            entry_type: "rate_limit_event".to_string(),
            uuid: "uuid-rate".to_string(),
            timestamp: "2025-01-15T10:30:00Z".to_string(),
            ..Default::default()
        };
        // No message.role set (default empty string)
        assert!(
            classify(e).is_none(),
            "rate_limit_event must be dropped silently"
        );
    }

    #[test]
    fn classify_drops_unknown_structural_entry_with_no_role() {
        // CwdChanged / FileChanged / --channels structural entries: unknown type, no message.role
        let e = Entry {
            entry_type: "CwdChanged".to_string(),
            uuid: "uuid-cwd".to_string(),
            timestamp: "2025-01-15T10:30:00Z".to_string(),
            ..Default::default()
        };
        assert!(
            classify(e).is_none(),
            "Unknown structural entry with no message role must be dropped"
        );
    }

    #[test]
    fn classify_keeps_unknown_entry_with_message_role_as_meta() {
        // An unknown entry type that carries actual message content should still be shown.
        let mut e = Entry {
            entry_type: "channel_message".to_string(),
            uuid: "uuid-chan".to_string(),
            timestamp: "2025-01-15T10:30:00Z".to_string(),
            message: super::super::entry::EntryMessage {
                role: "user".to_string(),
                content: Some(json!("hello from channel")),
                ..Default::default()
            },
            ..Default::default()
        };
        match classify(e) {
            Some(ClassifiedMsg::AI(ai)) => {
                assert!(ai.is_meta, "Unknown entry with role should be meta AI");
            }
            other => panic!(
                "Expected meta AI for unknown entry with role, got {:?}",
                other
            ),
        }
    }

    // --- normalize_tool_input tests (compat: pre-v2.1.92 streaming bug) ---

    #[test]
    fn normalize_tool_input_leaves_native_array_unchanged() {
        let input = json!({"command": "ls", "env": ["KEY=val"]});
        let result = normalize_tool_input(input.clone());
        assert_eq!(result, input);
    }

    #[test]
    fn normalize_tool_input_leaves_native_object_unchanged() {
        let input = json!({"options": {"flag": true}});
        let result = normalize_tool_input(input.clone());
        assert_eq!(result, input);
    }

    #[test]
    fn normalize_tool_input_parses_json_encoded_array_string() {
        // Pre-v2.1.92: array field emitted as JSON-encoded string
        let input = json!({"command": "ls", "env": "[\"KEY=val\"]"});
        let result = normalize_tool_input(input);
        assert_eq!(result["env"], json!(["KEY=val"]));
        assert_eq!(result["command"], json!("ls"));
    }

    #[test]
    fn normalize_tool_input_parses_json_encoded_object_string() {
        // Pre-v2.1.92: object field emitted as JSON-encoded string
        let input = json!({"options": "{\"flag\": true, \"count\": 3}"});
        let result = normalize_tool_input(input);
        assert_eq!(result["options"], json!({"flag": true, "count": 3}));
    }

    #[test]
    fn normalize_tool_input_leaves_plain_string_unchanged() {
        let input = json!({"command": "ls -la", "description": "List files"});
        let result = normalize_tool_input(input.clone());
        assert_eq!(result, input);
    }

    #[test]
    fn normalize_tool_input_leaves_string_that_looks_like_array_but_invalid_json_unchanged() {
        let input = json!({"bad": "[not valid json"});
        let result = normalize_tool_input(input.clone());
        assert_eq!(result, input);
    }

    #[test]
    fn normalize_tool_input_leaves_non_object_input_unchanged() {
        // input that is an array or scalar at the top level is returned as-is
        let input = json!(["a", "b"]);
        let result = normalize_tool_input(input.clone());
        assert_eq!(result, input);
    }

    #[test]
    fn classify_tool_use_block_with_pre_v2_1_92_encoded_array_is_normalized() {
        // Integration test: full classify path normalizes legacy encoded array
        let content = json!([{
            "type": "tool_use",
            "id": "tool1",
            "name": "Bash",
            "input": {
                "command": "ls",
                "env": "[\"KEY=val\"]"
            }
        }]);
        let mut e = make_entry("assistant", Some(content));
        e.message.model = "claude-sonnet-4-20250514".to_string();
        e.message.stop_reason = Some("tool_use".to_string());
        match classify(e) {
            Some(ClassifiedMsg::AI(ai)) => {
                let block = ai
                    .blocks
                    .iter()
                    .find(|b| b.block_type == "tool_use")
                    .expect("should have tool_use block");
                let env = block
                    .tool_input
                    .as_ref()
                    .and_then(|v| v.get("env"))
                    .expect("env field should exist");
                assert_eq!(
                    *env,
                    json!(["KEY=val"]),
                    "env should be a native array, not a string"
                );
            }
            other => panic!("Expected AI, got {:?}", other),
        }
    }

    // --- Issue #49: session recap (away_summary) entries are displayed as CompactMsg ---

    #[test]
    fn classify_away_summary_returns_compact_msg() {
        // v2.1.108+: recap entries use type:"system", subtype:"away_summary", content:"<text>"
        let e = Entry {
            entry_type: "system".to_string(),
            uuid: "uuid-recap".to_string(),
            timestamp: "2026-04-14T10:00:00Z".to_string(),
            subtype: "away_summary".to_string(),
            content: "Working on a bug fix in entry.rs.".to_string(),
            ..Default::default()
        };
        match classify(e) {
            Some(ClassifiedMsg::Compact(c)) => {
                assert_eq!(c.text, "Working on a bug fix in entry.rs.");
                assert!(c.is_recap, "away_summary must produce is_recap=true");
            }
            other => panic!("Expected Compact for away_summary, got {:?}", other),
        }
    }

    #[test]
    fn classify_away_summary_with_empty_content_returns_compact_msg() {
        // Empty content is preserved consistently with how "summary" entries are handled.
        let e = Entry {
            entry_type: "system".to_string(),
            uuid: "uuid-recap-empty".to_string(),
            timestamp: "2026-04-14T10:00:00Z".to_string(),
            subtype: "away_summary".to_string(),
            content: String::new(),
            ..Default::default()
        };
        match classify(e) {
            Some(ClassifiedMsg::Compact(c)) => {
                assert_eq!(c.text, "");
                assert!(c.is_recap, "away_summary must produce is_recap=true");
            }
            other => panic!("Expected Compact for empty away_summary, got {:?}", other),
        }
    }

    // --- Issue #35: PermissionDenied hook event is handled generically ---

    #[test]
    fn permission_denied_hook_attachment_produces_hook_msg() {
        // PermissionDenied fires as an attachment entry; the generic hookEvent
        // pattern must recognise it without an explicit match arm.
        let mut e = make_entry("user", None);
        e.entry_type = "attachment".to_string();
        e.attachment = Some(json!({
            "type": "hook_success",
            "hookEvent": "PermissionDenied",
            "hookName": "~/.claude/hooks/deny.sh"
        }));
        match classify(e) {
            Some(ClassifiedMsg::Hook(h)) => {
                assert_eq!(h.hook_event, "PermissionDenied");
                assert_eq!(h.hook_name, "~/.claude/hooks/deny.sh");
            }
            other => panic!("Expected Hook, got {:?}", other),
        }
    }

    // --- Issue #48: PreCompact hook event type (v2.1.105) is handled generically ---

    #[test]
    fn pre_compact_progress_entry_produces_hook_msg() {
        // PreCompact fires as a progress entry before session compaction.
        // The generic hookEvent rescue must recognise it without an explicit match arm.
        let mut e = make_entry("user", None);
        e.entry_type = "progress".to_string();
        e.data = Some(json!({
            "type": "hook_progress",
            "hookEvent": "PreCompact",
            "hookName": "PreCompact:my-hook",
            "command": "~/.claude/hooks/compact-guard.sh"
        }));
        match classify(e) {
            Some(ClassifiedMsg::Hook(h)) => {
                assert_eq!(h.hook_event, "PreCompact");
                assert_eq!(h.hook_name, "PreCompact:my-hook");
            }
            other => panic!(
                "Expected Hook for PreCompact progress entry, got {:?}",
                other
            ),
        }
    }

    #[test]
    fn pre_compact_attachment_with_block_decision_produces_hook_msg_with_metadata() {
        // When a PreCompact hook blocks compaction via {"decision":"block"}, Claude Code
        // writes an attachment entry. The decision field must be preserved in metadata.
        let mut e = make_entry("user", None);
        e.entry_type = "attachment".to_string();
        e.attachment = Some(json!({
            "type": "hook_blocking_error",
            "hookEvent": "PreCompact",
            "hookName": "PreCompact:compact-guard",
            "decision": "block",
            "reason": "uncommitted changes detected",
            "exitCode": 2,
            "durationMs": 50
        }));
        match classify(e) {
            Some(ClassifiedMsg::Hook(h)) => {
                assert_eq!(h.hook_event, "PreCompact");
                assert_eq!(h.hook_name, "PreCompact:compact-guard");
                let meta = h
                    .metadata
                    .expect("metadata must be present for attachment hooks");
                assert_eq!(meta.get("decision").and_then(|v| v.as_str()), Some("block"));
                assert_eq!(
                    meta.get("reason").and_then(|v| v.as_str()),
                    Some("uncommitted changes detected")
                );
            }
            other => panic!("Expected Hook for PreCompact attachment, got {:?}", other),
        }
    }

    // --- Issue #41: missing file_path in tool_use_result is handled gracefully ---

    #[test]
    fn tool_use_result_without_file_path_does_not_crash() {
        // tool_use_result is Option<Value>; absent file_path must not panic.
        let mut e = make_entry("user", Some(json!("Tool loaded.")));
        e.tool_use_result = Some(json!({
            "type": "tool_result",
            "tool_use_id": "toolu_abc",
            "content": "File written successfully"
            // file_path deliberately absent
        }));
        // classify must not panic; it returns None (tool-loaded noise) or a SystemMsg.
        let _ = classify(e);
    }

    // --- Issue #48: PreCompact hook event (v2.1.105) is handled generically ---

    #[test]
    fn classify_rescues_pre_compact_progress_hook_event() {
        // v2.1.105: PreCompact fires as a progress entry before session compaction.
        // The generic hookEvent presence check must rescue it without an explicit match arm.
        let e = Entry {
            entry_type: "progress".to_string(),
            uuid: "uuid-pre-compact".to_string(),
            timestamp: "2026-04-13T10:00:00Z".to_string(),
            data: Some(json!({
                "type": "hook_progress",
                "hookEvent": "PreCompact",
                "hookName": "my-compact-hook",
                "command": "echo compacting"
            })),
            ..Default::default()
        };
        match classify(e) {
            Some(ClassifiedMsg::Hook(h)) => {
                assert_eq!(h.hook_event, "PreCompact");
                assert_eq!(h.hook_name, "my-compact-hook");
            }
            other => panic!(
                "Expected Hook for PreCompact progress entry, got {:?}",
                other
            ),
        }
    }

    #[test]
    fn classify_rescues_pre_compact_attachment_with_decision_block() {
        // v2.1.105: PreCompact hooks can block compaction by returning {"decision":"block"}.
        // The attachment entry must be rescued and its metadata must preserve the decision field.
        let e = Entry {
            entry_type: "attachment".to_string(),
            uuid: "uuid-pre-compact-block".to_string(),
            timestamp: "2026-04-13T10:00:01Z".to_string(),
            attachment: Some(json!({
                "type": "hook_blocking_error",
                "hookEvent": "PreCompact",
                "hookName": "my-compact-hook",
                "decision": "block",
                "reason": "Not ready to compact"
            })),
            ..Default::default()
        };
        match classify(e) {
            Some(ClassifiedMsg::Hook(h)) => {
                assert_eq!(h.hook_event, "PreCompact");
                assert_eq!(h.hook_name, "my-compact-hook");
                let meta = h.metadata.expect("metadata must be present");
                assert_eq!(
                    meta.get("decision").and_then(|v| v.as_str()),
                    Some("block"),
                    "decision:block payload must be preserved in metadata"
                );
                assert_eq!(
                    meta.get("reason").and_then(|v| v.as_str()),
                    Some("Not ready to compact")
                );
            }
            other => panic!(
                "Expected Hook for PreCompact attachment with decision:block, got {:?}",
                other
            ),
        }
    }

    // --- Issue #117: v2.1.152+ MessageDisplay hook event is handled by existing catch-alls ---

    #[test]
    fn classify_message_display_progress_entry_produces_hook_msg() {
        // v2.1.152+: MessageDisplay fires as a progress/hook_progress entry.
        // The generic hookEvent presence check rescues it without an explicit match arm.
        let e = Entry {
            entry_type: "progress".to_string(),
            uuid: "uuid-msg-display-progress".to_string(),
            timestamp: "2026-05-27T10:00:00Z".to_string(),
            data: Some(json!({
                "type": "hook_progress",
                "hookEvent": "MessageDisplay",
                "hookName": "my-display-hook",
                "command": "echo transforming"
            })),
            ..Default::default()
        };
        match classify(e) {
            Some(ClassifiedMsg::Hook(h)) => {
                assert_eq!(h.hook_event, "MessageDisplay");
                assert_eq!(h.hook_name, "my-display-hook");
            }
            other => panic!(
                "Expected Hook for MessageDisplay progress entry, got {:?}",
                other
            ),
        }
    }

    #[test]
    fn classify_message_display_system_hook_progress_entry_produces_hook_msg() {
        // v2.1.152+: MessageDisplay may also arrive as type:"system", subtype:"hook_progress"
        // in verbose/stream-json mode. The hook_progress subtype rescue must handle it.
        let e = Entry {
            entry_type: "system".to_string(),
            uuid: "uuid-msg-display-sys".to_string(),
            timestamp: "2026-05-27T10:01:00Z".to_string(),
            subtype: "hook_progress".to_string(),
            hook_event: "MessageDisplay".to_string(),
            hook_name: "display-transform".to_string(),
            ..Default::default()
        };
        match classify(e) {
            Some(ClassifiedMsg::Hook(h)) => {
                assert_eq!(h.hook_event, "MessageDisplay");
                assert_eq!(h.hook_name, "display-transform");
            }
            other => panic!(
                "Expected Hook for system/hook_progress MessageDisplay entry, got {:?}",
                other
            ),
        }
    }

    #[test]
    fn classify_message_display_attachment_entry_produces_hook_msg() {
        // v2.1.152+: MessageDisplay hook results surface as attachment entries.
        // The generic attachment hookEvent rescue must handle it without an explicit arm.
        let e = Entry {
            entry_type: "attachment".to_string(),
            uuid: "uuid-msg-display-att".to_string(),
            timestamp: "2026-05-27T10:02:00Z".to_string(),
            attachment: Some(json!({
                "type": "hook_success",
                "hookEvent": "MessageDisplay",
                "hookName": "my-display-hook",
                "stdout": "transformed message text"
            })),
            ..Default::default()
        };
        match classify(e) {
            Some(ClassifiedMsg::Hook(h)) => {
                assert_eq!(h.hook_event, "MessageDisplay");
                assert_eq!(h.hook_name, "my-display-hook");
                let meta = h
                    .metadata
                    .expect("metadata must be captured for attachment hooks");
                assert_eq!(
                    meta.get("hookEvent").and_then(|v| v.as_str()),
                    Some("MessageDisplay")
                );
            }
            other => panic!(
                "Expected Hook for MessageDisplay attachment entry, got {:?}",
                other
            ),
        }
    }

    // --- Issue #60: fork-context-ref entry (v2.1.118+) is silently dropped ---

    #[test]
    fn classify_drops_fork_context_ref_entry() {
        // v2.1.118+: /fork writes a compact pointer entry (type:"fork-context-ref") instead
        // of duplicating the full parent conversation. classify must drop it silently.
        let e = Entry {
            entry_type: "fork-context-ref".to_string(),
            uuid: "fork-ref-uuid-001".to_string(),
            timestamp: "2026-04-20T10:00:00Z".to_string(),
            ..Default::default()
        };
        assert!(
            classify(e).is_none(),
            "fork-context-ref entry must be silently dropped"
        );
    }

    #[test]
    fn classify_drops_fork_context_ref_even_with_message_role() {
        // Extra safety: even if a fork-context-ref variant carries a message role, it must
        // still be dropped because "fork-context-ref" is in NOISE_ENTRY_TYPES.
        let e = Entry {
            entry_type: "fork-context-ref".to_string(),
            uuid: "fork-ref-uuid-002".to_string(),
            timestamp: "2026-04-20T10:00:00Z".to_string(),
            message: super::super::entry::EntryMessage {
                role: "user".to_string(),
                content: Some(serde_json::json!("should not be shown")),
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(
            classify(e).is_none(),
            "fork-context-ref entry must be dropped even when message.role is set"
        );
    }

    // --- Issue #115: v2.1.154+ Dynamic Workflow entry types ---

    #[test]
    fn classify_drops_all_workflow_lifecycle_entry_types_as_noise() {
        // v2.1.154+: workflow-start, workflow-progress, workflow-complete, workflow-cancelled,
        // and workflow-error are structural metadata with no displayable content; all must be
        // silently dropped from the transcript.
        for wf_type in &[
            "workflow-start",
            "workflow-progress",
            "workflow-complete",
            "workflow-cancelled",
            "workflow-error",
        ] {
            let e = Entry {
                entry_type: wf_type.to_string(),
                uuid: format!("uuid-{wf_type}"),
                timestamp: "2026-05-28T10:00:00Z".to_string(),
                ..Default::default()
            };
            assert!(
                classify(e).is_none(),
                "workflow entry type={wf_type} must be silently dropped"
            );
        }
    }

    #[test]
    fn classify_workflow_tool_use_in_assistant_message_is_extracted() {
        // v2.1.154+: the Workflow tool appears as a tool_use block in assistant messages.
        // extract_assistant_details must extract it without filtering by tool name.
        let content = json!([{
            "type": "tool_use",
            "id": "toolu_workflow_001",
            "name": "Workflow",
            "input": {
                "workflowName": "my-workflow",
                "description": "Run background agents"
            }
        }]);
        let mut e = make_entry("assistant", Some(content));
        e.message.model = "claude-sonnet-4-20250514".to_string();
        e.message.stop_reason = Some("tool_use".to_string());
        match classify(e) {
            Some(ClassifiedMsg::AI(ai)) => {
                assert_eq!(ai.tool_calls.len(), 1);
                assert_eq!(ai.tool_calls[0].name, "Workflow");
                let block = ai
                    .blocks
                    .iter()
                    .find(|b| b.block_type == "tool_use")
                    .expect("must have tool_use block");
                assert_eq!(block.tool_name, "Workflow");
                let input = block.tool_input.as_ref().expect("input must be captured");
                assert_eq!(
                    input.get("workflowName").and_then(|v| v.as_str()),
                    Some("my-workflow")
                );
            }
            other => panic!("Expected AI with Workflow tool_use, got {:?}", other),
        }
    }

    #[test]
    fn classify_workflow_start_with_data_fields_is_still_dropped() {
        // Even a workflow-start entry that carries workflowId / workflowName / workflowStatus
        // fields must be silently dropped — it is structural metadata, not transcript content.
        let e = Entry {
            entry_type: "workflow-start".to_string(),
            uuid: "uuid-wf-start-data".to_string(),
            timestamp: "2026-05-28T10:00:00Z".to_string(),
            workflow_id: "wf-123".to_string(),
            workflow_name: "my-workflow".to_string(),
            workflow_status: "running".to_string(),
            ..Default::default()
        };
        assert!(
            classify(e).is_none(),
            "workflow-start with data fields must still be dropped"
        );
    }

    // --- Issue #37: document content block is recognised as user content ---

    #[test]
    fn has_user_content_true_for_document_block() {
        let content = Some(json!([{
            "type": "document",
            "source": {"type": "base64", "media_type": "application/pdf", "data": "abc"}
        }]));
        let e = make_entry("user", content);
        // classify should produce a UserMsg (not None) because document counts as user content.
        match classify(e) {
            Some(ClassifiedMsg::User(_)) => {}
            other => panic!("Expected UserMsg for document block, got {:?}", other),
        }
    }

    // --- compact_boundary / isCompactSummary classification ---

    #[test]
    fn classify_compact_summary_user_entry_as_compact_msg() {
        // User entries with isCompactSummary:true are injected by Claude Code after compaction.
        // They must be classified as CompactMsg so they render as a styled separator, not as
        // a regular user turn that would confusingly appear alongside the pre-compact messages.
        let e = Entry {
            entry_type: "user".to_string(),
            uuid: "compact-summary-uuid".to_string(),
            timestamp: "2026-04-26T12:21:02Z".to_string(),
            is_compact_summary: true,
            message: super::super::entry::EntryMessage {
                role: "user".to_string(),
                content: Some(json!(
                    "This session is being continued from a previous conversation that ran out of context. The summary below covers the earlier portion of the conversation.\n\nSummary:\n1. Primary Request: ..."
                )),
                ..Default::default()
            },
            ..Default::default()
        };
        match classify(e) {
            Some(ClassifiedMsg::Compact(c)) => {
                assert!(
                    c.text.contains("This session is being continued"),
                    "compact summary text must be preserved"
                );
                assert!(!c.is_recap, "isCompactSummary must produce is_recap=false");
            }
            other => panic!(
                "Expected Compact for isCompactSummary user entry, got {:?}",
                other
            ),
        }
    }

    // --- Issue #124: v2.1.166+ fallbackModel retry stubs (empty-content assistant entries) ---

    #[test]
    fn classify_drops_assistant_with_null_content() {
        // v2.1.166+: when the primary model fails before emitting any content, Claude Code
        // writes an assistant stub with message.content:null before the fallback response.
        // classify must return None for this stub so no blank AI turn appears in the UI.
        let e = Entry {
            entry_type: "assistant".to_string(),
            uuid: "stub-null-content".to_string(),
            timestamp: "2026-06-06T10:00:00Z".to_string(),
            message: super::super::entry::EntryMessage {
                role: "assistant".to_string(),
                model: "claude-opus-4-7".to_string(),
                content: None,
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(
            classify(e).is_none(),
            "assistant entry with null content must be dropped (fallback retry stub)"
        );
    }

    #[test]
    fn classify_drops_assistant_with_empty_array_content() {
        // v2.1.166+: the stub may also carry an empty array instead of null.
        // Both forms must be silently dropped.
        let e = Entry {
            entry_type: "assistant".to_string(),
            uuid: "stub-empty-array-content".to_string(),
            timestamp: "2026-06-06T10:01:00Z".to_string(),
            message: super::super::entry::EntryMessage {
                role: "assistant".to_string(),
                model: "claude-opus-4-7".to_string(),
                content: Some(json!([])),
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(
            classify(e).is_none(),
            "assistant entry with empty-array content must be dropped (fallback retry stub)"
        );
    }

    #[test]
    fn classify_keeps_fallback_success_entry_with_content() {
        // v2.1.166+: the successful fallback response has real content and must be kept.
        // Its model (claude-haiku-4-5) may differ from the primary model and must be
        // preserved as authoritative for that entry.
        let content = json!([{"type": "text", "text": "Hello from fallback model"}]);
        let e = Entry {
            entry_type: "assistant".to_string(),
            uuid: "fallback-success-entry".to_string(),
            timestamp: "2026-06-06T10:01:01Z".to_string(),
            message: super::super::entry::EntryMessage {
                role: "assistant".to_string(),
                model: "claude-haiku-4-5".to_string(),
                content: Some(content),
                stop_reason: Some("end_turn".to_string()),
                ..Default::default()
            },
            ..Default::default()
        };
        match classify(e) {
            Some(ClassifiedMsg::AI(ai)) => {
                assert_eq!(
                    ai.model, "claude-haiku-4-5",
                    "fallback response model must be entry-authoritative"
                );
                assert!(
                    ai.text.contains("Hello from fallback model"),
                    "fallback response text must be preserved"
                );
            }
            other => panic!("Expected AI for fallback success entry, got {:?}", other),
        }
    }

    #[test]
    fn classify_fallback_model_differs_from_primary_model() {
        // v2.1.166+: adjacent turns may have different model IDs when fallbackModel is used.
        // Each entry's model is authoritative for that turn — no session-level assumption.
        let primary_content = json!([{"type": "text", "text": "Response from primary"}]);
        let primary = Entry {
            entry_type: "assistant".to_string(),
            uuid: "primary-turn".to_string(),
            timestamp: "2026-06-06T10:00:00Z".to_string(),
            message: super::super::entry::EntryMessage {
                role: "assistant".to_string(),
                model: "claude-opus-4-7".to_string(),
                content: Some(primary_content),
                stop_reason: Some("end_turn".to_string()),
                ..Default::default()
            },
            ..Default::default()
        };
        let fallback_content = json!([{"type": "text", "text": "Response from fallback"}]);
        let fallback = Entry {
            entry_type: "assistant".to_string(),
            uuid: "fallback-turn".to_string(),
            timestamp: "2026-06-06T10:00:05Z".to_string(),
            message: super::super::entry::EntryMessage {
                role: "assistant".to_string(),
                model: "claude-haiku-4-5".to_string(),
                content: Some(fallback_content),
                stop_reason: Some("end_turn".to_string()),
                ..Default::default()
            },
            ..Default::default()
        };

        match (classify(primary), classify(fallback)) {
            (Some(ClassifiedMsg::AI(p)), Some(ClassifiedMsg::AI(f))) => {
                assert_eq!(p.model, "claude-opus-4-7");
                assert_eq!(f.model, "claude-haiku-4-5");
            }
            other => panic!("Expected two AI messages, got {:?}", other),
        }
    }

    #[test]
    fn classify_regular_user_entry_not_affected_by_compact_summary_flag() {
        // Regular user entries (isCompactSummary defaults to false) must still produce UserMsg.
        let e = make_entry("user", Some(json!("Hello Claude")));
        match classify(e) {
            Some(ClassifiedMsg::User(u)) => {
                assert!(u.text.contains("Hello Claude"));
            }
            other => panic!("Expected User, got {:?}", other),
        }
    }
}
