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
#[derive(Debug, Clone, Serialize)]
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

impl Default for ContentBlock {
    fn default() -> Self {
        Self {
            block_type: String::new(),
            text: String::new(),
            tool_id: String::new(),
            tool_name: String::new(),
            tool_input: None,
            content: String::new(),
            is_error: false,
            teammate_id: String::new(),
            teammate_color: String::new(),
        }
    }
}

/// Classified message types.
#[derive(Debug, Clone)]
pub enum ClassifiedMsg {
    User(UserMsg),
    AI(AIMsg),
    System(SystemMsg),
    Teammate(TeammateMsg),
    Compact(CompactMsg),
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

pub const SYSTEM_OUTPUT_TAGS: &[&str] = &[
    LOCAL_COMMAND_STDERR_TAG,
    LOCAL_COMMAND_STDOUT_TAG,
    "<local-command-caveat>",
    "<system-reminder>",
    BASH_STDOUT_TAG,
    BASH_STDERR_TAG,
    TASK_NOTIFICATION_TAG,
];

const NOISE_ENTRY_TYPES: &[&str] = &["system", "file-history-snapshot", "queue-operation", "progress"];

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
        if trimmed.starts_with(LOCAL_COMMAND_STDOUT_TAG) || trimmed.starts_with(LOCAL_COMMAND_STDERR_TAG) {
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
                is_error: status == "killed",
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
        let excluded = SYSTEM_OUTPUT_TAGS.iter().any(|tag| trimmed.starts_with(tag));
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

    // Fallback: remaining user messages -> AI message (meta).
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
    let names: Vec<String> = arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect();
    if names.is_empty() { None } else { Some(names) }
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
                    text: b.get("thinking").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                    ..Default::default()
                });
            }
            "text" => {
                content_blocks.push(ContentBlock {
                    block_type: "text".to_string(),
                    text: b.get("text").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                    ..Default::default()
                });
            }
            "tool_use" => {
                let id = b.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let name = b.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
                if !id.is_empty() && !name.is_empty() {
                    calls.push(ToolCall { id: id.clone(), name: name.clone() });
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
                    text: b.get("text").and_then(|v| v.as_str()).unwrap_or("").to_string(),
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

    let has_tool_result = blocks.iter().any(|b| {
        b.get("type").and_then(|v| v.as_str()) == Some("tool_result")
    });

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
            let tool_id = b.get("tool_use_id").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let content = stringify_content(&b.get("content").cloned());
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
