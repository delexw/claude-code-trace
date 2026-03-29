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
}

#[derive(Debug, Clone)]
pub struct HookMsg {
    pub timestamp: DateTime<Utc>,
    pub hook_event: String,
    pub hook_name: String,
    pub command: String,
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
    // so that future hook types (e.g. TaskCreated added in v2.1.84) are also rescued
    // without needing to enumerate data.type values.
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
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                return Some(ClassifiedMsg::Hook(HookMsg {
                    timestamp: ts,
                    hook_event,
                    hook_name,
                    command,
                }));
            }
        }
    }

    // Rescue hook-related system entries before the NOISE_ENTRY_TYPES filter drops them.
    if e.entry_type == "system" {
        match e.subtype.as_str() {
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
                }));
            }
            // hook_progress: written in verbose/stream-json mode for mid-session hooks.
            "hook_progress" => {
                return Some(ClassifiedMsg::Hook(HookMsg {
                    timestamp: ts,
                    hook_event: e.hook_event.clone(),
                    hook_name: e.hook_name.clone(),
                    command: String::new(),
                }));
            }
            // hookEvent present on any system entry (forward-compat for future hook types).
            _ if !e.hook_event.is_empty() => {
                return Some(ClassifiedMsg::Hook(HookMsg {
                    timestamp: ts,
                    hook_event: e.hook_event.clone(),
                    hook_name: e.hook_name.clone(),
                    command: String::new(),
                }));
            }
            _ => {}
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
                cache_creation_tokens: e.message.usage.cache_creation_input_tokens,
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
            bt == "text" || bt == "image"
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
                    tool_input: b.get("input").cloned(),
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
            let raw_content = stringify_content(&b.get("content").cloned());
            let content = resolve_persisted_output(&raw_content);
            let is_error = b.get("is_error").and_then(|v| v.as_bool()).unwrap_or(false);
            Some(ContentBlock {
                block_type: "tool_result".to_string(),
                tool_id,
                content,
                is_error,
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
}
