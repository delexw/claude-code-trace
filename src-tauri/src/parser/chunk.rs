use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;

use super::classify::*;
use super::summary::tool_summary;
use super::taxonomy::{categorize_tool_name, mcp_display_name, ToolCategory};

const CONCURRENT_TASK_DURATION_THRESHOLD: i64 = 60_000;

/// DisplayItemType discriminates the display item categories.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum DisplayItemType {
    Thinking,
    Output,
    ToolCall,
    Subagent,
    TeammateMessage,
    HookEvent,
}

/// DisplayItem is a structured element within an AI chunk's detail view.
#[derive(Debug, Clone, Serialize)]
pub struct DisplayItem {
    pub item_type: DisplayItemType,
    pub text: String,
    pub tool_name: String,
    pub tool_id: String,
    pub tool_input: Option<Value>,
    pub tool_summary: String,
    pub tool_result: String,
    pub tool_error: bool,
    pub duration_ms: i64,
    pub token_count: usize,
    pub tool_category: ToolCategory,
    pub subagent_type: String,
    pub subagent_desc: String,
    pub team_member_name: String,
    pub teammate_id: String,
    pub teammate_color: String,
    pub hook_event: String,
    pub hook_name: String,
    pub hook_command: String,
    pub is_orphan: bool,
    /// True when the session was suspended via a "defer" PreToolUse permission decision
    /// before a tool_result arrived for this tool_use block.
    pub is_deferred: bool,
}

impl Default for DisplayItem {
    fn default() -> Self {
        Self {
            item_type: DisplayItemType::Output,
            text: String::new(),
            tool_name: String::new(),
            tool_id: String::new(),
            tool_input: None,
            tool_summary: String::new(),
            tool_result: String::new(),
            tool_error: false,
            duration_ms: 0,
            token_count: 0,
            tool_category: ToolCategory::Other,
            subagent_type: String::new(),
            subagent_desc: String::new(),
            team_member_name: String::new(),
            teammate_id: String::new(),
            teammate_color: String::new(),
            hook_event: String::new(),
            hook_name: String::new(),
            hook_command: String::new(),
            is_orphan: false,
            is_deferred: false,
        }
    }
}

/// ChunkType discriminates the chunk categories.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum ChunkType {
    User,
    AI,
    System,
    Compact,
}

/// Chunk is the output of the pipeline. Each chunk represents one visible unit.
#[derive(Debug, Clone, Serialize)]
pub struct Chunk {
    pub chunk_type: ChunkType,
    pub timestamp: DateTime<Utc>,
    pub user_text: String,
    pub model: String,
    pub text: String,
    pub thinking_count: usize,
    pub tool_calls: Vec<ToolCall>,
    pub items: Vec<DisplayItem>,
    pub usage: Usage,
    pub stop_reason: String,
    pub duration_ms: i64,
    pub output: String,
    pub is_error: bool,
}

impl Default for Chunk {
    fn default() -> Self {
        Self {
            chunk_type: ChunkType::User,
            timestamp: Utc::now(),
            user_text: String::new(),
            model: String::new(),
            text: String::new(),
            thinking_count: 0,
            tool_calls: Vec::new(),
            items: Vec::new(),
            usage: Usage::default(),
            stop_reason: String::new(),
            duration_ms: 0,
            output: String::new(),
            is_error: false,
        }
    }
}

/// Build chunks from classified messages.
pub fn build_chunks(msgs: &[ClassifiedMsg]) -> Vec<Chunk> {
    let mut chunks = Vec::new();
    let mut ai_buf: Vec<AIMsg> = Vec::new();

    let flush = |buf: &mut Vec<AIMsg>, chunks: &mut Vec<Chunk>| {
        if buf.is_empty() {
            return;
        }
        chunks.push(merge_ai_buffer(buf));
        buf.clear();
    };

    for msg in msgs {
        match msg {
            ClassifiedMsg::User(m) => {
                flush(&mut ai_buf, &mut chunks);
                chunks.push(Chunk {
                    chunk_type: ChunkType::User,
                    timestamp: m.timestamp,
                    user_text: m.text.clone(),
                    ..Default::default()
                });
            }
            ClassifiedMsg::System(m) => {
                flush(&mut ai_buf, &mut chunks);
                chunks.push(Chunk {
                    chunk_type: ChunkType::System,
                    timestamp: m.timestamp,
                    output: m.output.clone(),
                    is_error: m.is_error,
                    ..Default::default()
                });
            }
            ClassifiedMsg::AI(m) => {
                ai_buf.push(m.clone());
            }
            ClassifiedMsg::Teammate(m) => {
                // Fold into AI buffer as synthetic AIMsg with teammate block.
                ai_buf.push(AIMsg {
                    timestamp: m.timestamp,
                    is_meta: true,
                    blocks: vec![ContentBlock {
                        block_type: "teammate".to_string(),
                        text: m.text.clone(),
                        teammate_id: m.teammate_id.clone(),
                        teammate_color: m.color.clone(),
                        ..Default::default()
                    }],
                    ..AIMsg {
                        timestamp: m.timestamp,
                        model: String::new(),
                        text: String::new(),
                        thinking_count: 0,
                        tool_calls: Vec::new(),
                        blocks: Vec::new(),
                        usage: Usage::default(),
                        stop_reason: String::new(),
                        is_meta: true,
                    }
                });
            }
            ClassifiedMsg::Hook(m) => {
                // Fold hook events into the AI buffer as synthetic AIMsg with a hook block.
                ai_buf.push(AIMsg {
                    timestamp: m.timestamp,
                    model: String::new(),
                    text: String::new(),
                    thinking_count: 0,
                    tool_calls: Vec::new(),
                    blocks: vec![ContentBlock {
                        block_type: "hook_event".to_string(),
                        text: m.command.clone(),
                        tool_name: m.hook_name.clone(),
                        tool_id: m.hook_event.clone(),
                        ..Default::default()
                    }],
                    usage: Usage::default(),
                    stop_reason: String::new(),
                    is_meta: true,
                });
            }
            ClassifiedMsg::Compact(m) => {
                flush(&mut ai_buf, &mut chunks);
                chunks.push(Chunk {
                    chunk_type: ChunkType::Compact,
                    timestamp: m.timestamp,
                    output: m.text.clone(),
                    ..Default::default()
                });
            }
        }
    }
    flush(&mut ai_buf, &mut chunks);
    chunks
}

struct PendingTool {
    index: usize,
    timestamp: DateTime<Utc>,
}

fn merge_ai_buffer(buf: &[AIMsg]) -> Chunk {
    let mut texts: Vec<String> = Vec::new();
    let mut thinking = 0usize;
    let mut tool_calls: Vec<ToolCall> = Vec::new();
    let mut model = String::new();
    let mut stop = String::new();
    let mut items: Vec<DisplayItem> = Vec::new();
    let mut pending: HashMap<String, PendingTool> = HashMap::new();
    let mut has_blocks = false;

    for m in buf {
        if !m.text.is_empty() {
            texts.push(m.text.clone());
        }
        thinking += m.thinking_count;
        tool_calls.extend(m.tool_calls.iter().cloned());

        if model.is_empty() && !m.is_meta && !m.model.is_empty() {
            model = m.model.clone();
        }
        if !m.is_meta && !m.stop_reason.is_empty() {
            stop = m.stop_reason.clone();
        }

        if m.blocks.is_empty() {
            continue;
        }
        has_blocks = true;

        if !m.is_meta {
            for b in &m.blocks {
                match b.block_type.as_str() {
                    "thinking" => {
                        items.push(DisplayItem {
                            item_type: DisplayItemType::Thinking,
                            text: b.text.clone(),
                            ..Default::default()
                        });
                    }
                    "text" => {
                        items.push(DisplayItem {
                            item_type: DisplayItemType::Output,
                            text: b.text.clone(),
                            ..Default::default()
                        });
                    }
                    "tool_use" => {
                        let summary = tool_summary(&b.tool_name, &b.tool_input);
                        let category = categorize_tool_name(&b.tool_name);
                        let display_name = mcp_display_name(&b.tool_name);

                        if b.tool_name == "Task" || b.tool_name == "Agent" {
                            let info = extract_subagent_info(&b.tool_input);
                            items.push(DisplayItem {
                                item_type: DisplayItemType::Subagent,
                                tool_name: b.tool_name.clone(),
                                tool_id: b.tool_id.clone(),
                                tool_input: b.tool_input.clone(),
                                tool_summary: summary,
                                tool_category: category,
                                subagent_type: info.0,
                                subagent_desc: info.1,
                                team_member_name: info.2,
                                ..Default::default()
                            });
                        } else {
                            items.push(DisplayItem {
                                item_type: DisplayItemType::ToolCall,
                                tool_name: display_name,
                                tool_id: b.tool_id.clone(),
                                tool_input: b.tool_input.clone(),
                                tool_summary: summary,
                                tool_category: category,
                                ..Default::default()
                            });
                        }
                        pending.insert(
                            b.tool_id.clone(),
                            PendingTool {
                                index: items.len() - 1,
                                timestamp: m.timestamp,
                            },
                        );
                    }
                    _ => {}
                }
            }
        } else {
            for b in &m.blocks {
                match b.block_type.as_str() {
                    "tool_result" => {
                        if let Some(p) = pending.remove(&b.tool_id) {
                            items[p.index].tool_result = b.content.clone();
                            items[p.index].tool_error = b.is_error;
                            let dur = m.timestamp.signed_duration_since(p.timestamp);
                            items[p.index].duration_ms = dur.num_milliseconds();
                        } else {
                            items.push(DisplayItem {
                                item_type: DisplayItemType::Output,
                                text: b.content.clone(),
                                ..Default::default()
                            });
                        }
                    }
                    "teammate" => {
                        items.push(DisplayItem {
                            item_type: DisplayItemType::TeammateMessage,
                            text: b.text.clone(),
                            teammate_id: b.teammate_id.clone(),
                            teammate_color: b.teammate_color.clone(),
                            ..Default::default()
                        });
                    }
                    "hook_event" => {
                        items.push(DisplayItem {
                            item_type: DisplayItemType::HookEvent,
                            hook_event: b.tool_id.clone(),
                            hook_name: b.tool_name.clone(),
                            hook_command: b.text.clone(),
                            ..Default::default()
                        });
                    }
                    _ => {}
                }
            }
        }
    }

    // Any tool_use blocks still in pending have no matching tool_result — the session was
    // suspended via a "defer" PreToolUse permission decision before the tool ran.
    for p in pending.values() {
        items[p.index].is_deferred = true;
    }

    let first = buf.first().map(|m| m.timestamp).unwrap_or_else(Utc::now);
    let last = buf.last().map(|m| m.timestamp).unwrap_or(first);
    let dur = last.signed_duration_since(first).num_milliseconds();
    let ts = first;

    let final_items = if has_blocks {
        suppress_inflated_durations(&mut items);
        items
    } else {
        Vec::new()
    };

    // Usage snapshot: last non-meta assistant message's usage.
    let mut usage = Usage::default();
    for m in buf.iter().rev() {
        if !m.is_meta && m.usage.total_tokens() > 0 {
            usage = m.usage.clone();
            break;
        }
    }

    Chunk {
        chunk_type: ChunkType::AI,
        timestamp: ts,
        model,
        text: texts.join("\n"),
        thinking_count: thinking,
        tool_calls,
        items: final_items,
        usage,
        stop_reason: stop,
        duration_ms: dur,
        ..Default::default()
    }
}

fn suppress_inflated_durations(items: &mut [DisplayItem]) {
    let max_task_dur = items
        .iter()
        .filter(|it| it.item_type == DisplayItemType::Subagent)
        .map(|it| it.duration_ms)
        .max()
        .unwrap_or(0);

    if max_task_dur == 0 {
        return;
    }

    for item in items.iter_mut() {
        if item.item_type == DisplayItemType::Subagent
            || item.item_type == DisplayItemType::TeammateMessage
        {
            continue;
        }
        if item.duration_ms > CONCURRENT_TASK_DURATION_THRESHOLD {
            item.duration_ms = 0;
        }
    }
}

/// Extract subagent info from Task tool input: (type, description, member_name)
fn extract_subagent_info(input: &Option<Value>) -> (String, String, String) {
    let map = match input {
        Some(Value::Object(m)) => m,
        _ => return (String::new(), String::new(), String::new()),
    };

    let subagent_type = map
        .get("subagent_type")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let mut description = map
        .get("description")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    if description.is_empty() {
        if let Some(prompt) = map.get("prompt").and_then(|v| v.as_str()) {
            description = super::summary::truncate(prompt, 80);
        }
    }
    let member_name = map
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    (subagent_type, description, member_name)
}

/// Check whether a DisplayItem is a team task (has team_name and name in input).
pub fn is_team_task(item: &DisplayItem) -> bool {
    match &item.tool_input {
        Some(Value::Object(map)) => map.contains_key("team_name") && map.contains_key("name"),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::classify::{AIMsg, ClassifiedMsg, ContentBlock, Usage};
    use chrono::Utc;

    fn make_ai_msg(blocks: Vec<ContentBlock>, is_meta: bool) -> AIMsg {
        AIMsg {
            timestamp: Utc::now(),
            model: if is_meta {
                String::new()
            } else {
                "claude-test".to_string()
            },
            text: String::new(),
            thinking_count: 0,
            tool_calls: Vec::new(),
            blocks,
            usage: Usage::default(),
            stop_reason: String::new(),
            is_meta,
        }
    }

    fn tool_use_block(id: &str, name: &str) -> ContentBlock {
        ContentBlock {
            block_type: "tool_use".to_string(),
            tool_id: id.to_string(),
            tool_name: name.to_string(),
            ..Default::default()
        }
    }

    fn tool_result_block(id: &str, content: &str) -> ContentBlock {
        ContentBlock {
            block_type: "tool_result".to_string(),
            tool_id: id.to_string(),
            content: content.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn tool_use_with_matching_result_is_not_deferred() {
        // A complete pair: tool_use followed by a tool_result in a meta entry.
        let tool_id = "toolu_001";
        let msgs = vec![
            ClassifiedMsg::AI(make_ai_msg(vec![tool_use_block(tool_id, "Bash")], false)),
            ClassifiedMsg::AI(make_ai_msg(
                vec![tool_result_block(tool_id, "output")],
                true,
            )),
        ];
        let chunks = build_chunks(&msgs);
        assert_eq!(chunks.len(), 1);
        let items = &chunks[0].items;
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].item_type, DisplayItemType::ToolCall);
        assert_eq!(items[0].tool_result, "output");
        assert!(
            !items[0].is_deferred,
            "matched tool_use should not be deferred"
        );
    }

    #[test]
    fn tool_use_without_result_is_marked_deferred() {
        // Session ends after tool_use with no tool_result — simulates "defer" suspension.
        let tool_id = "toolu_defer_001";
        let msgs = vec![ClassifiedMsg::AI(make_ai_msg(
            vec![tool_use_block(tool_id, "Read")],
            false,
        ))];
        let chunks = build_chunks(&msgs);
        assert_eq!(chunks.len(), 1);
        let items = &chunks[0].items;
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].item_type, DisplayItemType::ToolCall);
        assert!(items[0].tool_result.is_empty());
        assert!(
            items[0].is_deferred,
            "dangling tool_use should be marked deferred"
        );
    }

    #[test]
    fn multiple_tool_uses_only_unmatched_are_deferred() {
        // Two tool_use blocks: one gets a result, one doesn't.
        let id_complete = "toolu_complete";
        let id_deferred = "toolu_deferred";
        let msgs = vec![
            ClassifiedMsg::AI(make_ai_msg(
                vec![
                    tool_use_block(id_complete, "Bash"),
                    tool_use_block(id_deferred, "Read"),
                ],
                false,
            )),
            ClassifiedMsg::AI(make_ai_msg(
                vec![tool_result_block(id_complete, "done")],
                true,
            )),
        ];
        let chunks = build_chunks(&msgs);
        assert_eq!(chunks.len(), 1);
        let items = &chunks[0].items;
        assert_eq!(items.len(), 2);

        let complete = items.iter().find(|i| i.tool_name == "Bash").unwrap();
        let deferred = items.iter().find(|i| i.tool_name == "Read").unwrap();

        assert_eq!(complete.tool_result, "done");
        assert!(!complete.is_deferred);
        assert!(deferred.tool_result.is_empty());
        assert!(deferred.is_deferred);
    }

    #[test]
    fn deferred_subagent_tool_use_is_marked_deferred() {
        // A Task/Agent tool_use block without a matching result is also deferred.
        let tool_id = "toolu_agent_deferred";
        let msgs = vec![ClassifiedMsg::AI(make_ai_msg(
            vec![ContentBlock {
                block_type: "tool_use".to_string(),
                tool_id: tool_id.to_string(),
                tool_name: "Task".to_string(),
                ..Default::default()
            }],
            false,
        ))];
        let chunks = build_chunks(&msgs);
        assert_eq!(chunks.len(), 1);
        let items = &chunks[0].items;
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].item_type, DisplayItemType::Subagent);
        assert!(
            items[0].is_deferred,
            "dangling Task tool_use should be marked deferred"
        );
    }
}
