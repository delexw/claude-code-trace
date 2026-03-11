use serde::Serialize;
use serde_json::Value;
use std::collections::{HashMap, HashSet};

use crate::parser::chunk::*;
use crate::parser::last_output::{find_last_output, LastOutput, LastOutputType};
use crate::parser::ongoing::{is_ongoing, ONGOING_STALENESS_THRESHOLD};
use crate::parser::subagent::SubagentProcess;
use crate::parser::team::TeamSnapshot;
use crate::parser::taxonomy::ToolCategory;

/// Team color pool for synthetic color assignment.
const TEAM_COLOR_POOL: &[&str] = &[
    "blue", "green", "red", "yellow", "purple", "cyan", "orange", "pink",
];

/// Frontend display item.
#[derive(Debug, Clone, Serialize)]
pub struct FrontendDisplayItem {
    pub item_type: String,
    pub text: String,
    pub tool_name: String,
    pub tool_summary: String,
    pub tool_category: String,
    pub tool_input: String,
    pub tool_result: String,
    pub tool_error: bool,
    pub duration_ms: i64,
    pub token_count: usize,
    pub subagent_type: String,
    pub subagent_desc: String,
    pub team_member_name: String,
    pub teammate_id: String,
    pub team_color: String,
    pub subagent_ongoing: bool,
}

/// Frontend last output.
#[derive(Debug, Clone, Serialize)]
pub struct FrontendLastOutput {
    pub output_type: String,
    pub text: String,
    pub tool_name: String,
    pub tool_result: String,
    pub is_error: bool,
    pub tool_calls: Vec<FrontendToolCallSummary>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FrontendToolCallSummary {
    pub name: String,
    pub summary: String,
}

/// DisplayMessage struct sent to frontend.
#[derive(Debug, Clone, Serialize)]
pub struct DisplayMessage {
    pub role: String,
    pub model: String,
    pub content: String,
    pub timestamp: String,
    pub thinking_count: usize,
    pub tool_call_count: usize,
    pub output_count: usize,
    pub tokens_raw: i64,
    pub context_tokens: i64,
    pub duration_ms: i64,
    pub items: Vec<FrontendDisplayItem>,
    pub last_output: Option<FrontendLastOutput>,
    pub is_error: bool,
    pub teammate_spawns: usize,
    pub teammate_messages: usize,
    pub subagent_label: String,
}

/// LoadResult holds everything needed to bootstrap the frontend.
#[derive(Debug, Clone, Serialize)]
pub struct LoadResult {
    pub messages: Vec<DisplayMessage>,
    pub teams: Vec<TeamSnapshot>,
    pub path: String,
    pub ongoing: bool,
    pub meta: crate::parser::session::SessionMeta,
}

/// Format a timestamp as "3:04:05 PM".
fn format_time(ts: &chrono::DateTime<chrono::Utc>) -> String {
    use chrono::Local;
    let local = ts.with_timezone(&Local);
    local.format("%-I:%M:%S %p").to_string()
}

/// Shorten model name: "claude-opus-4-6" -> "opus4.6"
fn short_model(m: &str) -> String {
    let m = m.strip_prefix("claude-").unwrap_or(m);
    let parts: Vec<&str> = m.splitn(2, '-').collect();
    if parts.len() == 2 {
        let family = parts[0];
        let v_parts: Vec<&str> = parts[1].splitn(3, '-').collect();
        let version = if v_parts.len() >= 2 {
            format!("{}-{}", v_parts[0], v_parts[1])
        } else {
            v_parts[0].to_string()
        };
        format!("{}{}", family, version.replace('-', "."))
    } else {
        m.to_string()
    }
}

fn count_output_items(items: &[DisplayItem]) -> usize {
    items.iter().filter(|it| it.item_type == DisplayItemType::Output).count()
}

fn display_item_type_str(t: &DisplayItemType) -> &'static str {
    match t {
        DisplayItemType::Thinking => "Thinking",
        DisplayItemType::Output => "Output",
        DisplayItemType::ToolCall => "ToolCall",
        DisplayItemType::Subagent => "Subagent",
        DisplayItemType::TeammateMessage => "TeammateMessage",
    }
}

fn tool_category_str(c: &ToolCategory) -> &'static str {
    match c {
        ToolCategory::Read => "Read",
        ToolCategory::Edit => "Edit",
        ToolCategory::Write => "Write",
        ToolCategory::Bash => "Bash",
        ToolCategory::Grep => "Grep",
        ToolCategory::Glob => "Glob",
        ToolCategory::Task => "Task",
        ToolCategory::Tool => "Tool",
        ToolCategory::Web => "Web",
        ToolCategory::Other => "Other",
    }
}

fn last_output_type_str(t: &LastOutputType) -> &'static str {
    match t {
        LastOutputType::Text => "Text",
        LastOutputType::ToolResult => "ToolResult",
        LastOutputType::ToolCalls => "ToolCalls",
    }
}

fn pretty_json(val: &Option<Value>) -> String {
    match val {
        Some(v) => serde_json::to_string_pretty(v).unwrap_or_else(|_| {
            serde_json::to_string(v).unwrap_or_default()
        }),
        None => String::new(),
    }
}

fn convert_last_output(lo: &LastOutput) -> FrontendLastOutput {
    FrontendLastOutput {
        output_type: last_output_type_str(&lo.output_type).to_string(),
        text: lo.text.clone(),
        tool_name: lo.tool_name.clone(),
        tool_result: lo.tool_result.clone(),
        is_error: lo.is_error,
        tool_calls: lo
            .tool_calls
            .iter()
            .map(|tc| FrontendToolCallSummary {
                name: tc.name.clone(),
                summary: tc.summary.clone(),
            })
            .collect(),
    }
}

/// Check if a subagent is ongoing (chunk-based + file staleness).
fn is_subagent_ongoing(proc: &SubagentProcess) -> bool {
    if !is_ongoing(&proc.chunks) {
        return false;
    }
    let elapsed = chrono::Utc::now()
        .signed_duration_since(proc.file_mod_time)
        .to_std()
        .unwrap_or(std::time::Duration::ZERO);
    if elapsed > ONGOING_STALENESS_THRESHOLD {
        return false;
    }
    true
}

/// Convert display items with subagent linking and color assignment.
fn convert_display_items(
    items: &[DisplayItem],
    subagents: &[SubagentProcess],
    color_by_tool_id: &HashMap<String, String>,
) -> Vec<FrontendDisplayItem> {
    if items.is_empty() {
        return Vec::new();
    }

    // Build ParentTaskID -> SubagentProcess index.
    let proc_by_task_id: HashMap<&str, &SubagentProcess> = subagents
        .iter()
        .filter(|p| !p.parent_task_id.is_empty())
        .map(|p| (p.parent_task_id.as_str(), p))
        .collect();

    let mut out: Vec<FrontendDisplayItem> = items
        .iter()
        .map(|it| {
            let mut fdi = FrontendDisplayItem {
                item_type: display_item_type_str(&it.item_type).to_string(),
                text: it.text.clone(),
                tool_name: it.tool_name.clone(),
                tool_summary: it.tool_summary.clone(),
                tool_category: tool_category_str(&it.tool_category).to_string(),
                tool_input: pretty_json(&it.tool_input),
                tool_result: it.tool_result.clone(),
                tool_error: it.tool_error,
                duration_ms: it.duration_ms,
                token_count: it.token_count,
                subagent_type: it.subagent_type.clone(),
                subagent_desc: it.subagent_desc.clone(),
                team_member_name: it.team_member_name.clone(),
                teammate_id: it.teammate_id.clone(),
                team_color: it.teammate_color.clone(),
                subagent_ongoing: false,
            };

            // Link subagent process if available.
            if it.item_type == DisplayItemType::Subagent {
                if let Some(proc) = proc_by_task_id.get(it.tool_id.as_str()) {
                    fdi.subagent_ongoing = is_subagent_ongoing(proc);
                    if !proc.teammate_color.is_empty() {
                        fdi.team_color = proc.teammate_color.clone();
                    }
                }
                // Fallback: apply team color from toolUseResult data.
                if fdi.team_color.is_empty() {
                    if let Some(color) = color_by_tool_id.get(&it.tool_id) {
                        fdi.team_color = color.clone();
                    }
                }
            }

            fdi
        })
        .collect();

    // Assign pool colors to subagents without a team color.
    let claimed: HashSet<String> = out.iter().map(|di| di.team_color.clone()).filter(|c| !c.is_empty()).collect();
    let pool_colors: Vec<&str> = TEAM_COLOR_POOL
        .iter()
        .filter(|name| !claimed.contains(**name))
        .copied()
        .collect();

    if !pool_colors.is_empty() {
        let mut pool_idx = 0;
        for di in &mut out {
            if di.item_type == "Subagent" && di.team_color.is_empty() {
                di.team_color = pool_colors[pool_idx % pool_colors.len()].to_string();
                pool_idx += 1;
            }
        }
    }

    out
}

/// Convert chunks to display messages (port of Go chunksToMessages).
pub fn chunks_to_messages(
    chunks: &[Chunk],
    subagents: &[SubagentProcess],
    color_by_tool_id: &HashMap<String, String>,
) -> Vec<DisplayMessage> {
    let mut msgs = Vec::new();

    for c in chunks {
        match c.chunk_type {
            ChunkType::User => {
                msgs.push(DisplayMessage {
                    role: "user".to_string(),
                    model: String::new(),
                    content: c.user_text.clone(),
                    timestamp: format_time(&c.timestamp),
                    thinking_count: 0,
                    tool_call_count: 0,
                    output_count: 0,
                    tokens_raw: 0,
                    context_tokens: 0,
                    duration_ms: 0,
                    items: Vec::new(),
                    last_output: None,
                    is_error: false,
                    teammate_spawns: 0,
                    teammate_messages: 0,
                    subagent_label: String::new(),
                });
            }
            ChunkType::AI => {
                let mut team_spawns = 0;
                let mut teammate_ids: HashSet<String> = HashSet::new();
                for it in &c.items {
                    if it.item_type == DisplayItemType::Subagent && is_team_task(it) {
                        team_spawns += 1;
                    }
                    if it.item_type == DisplayItemType::TeammateMessage && !it.teammate_id.is_empty() {
                        teammate_ids.insert(it.teammate_id.clone());
                    }
                }

                let lo = find_last_output(&c.items);
                let frontend_lo = lo.as_ref().map(|l| convert_last_output(l));

                msgs.push(DisplayMessage {
                    role: "claude".to_string(),
                    model: short_model(&c.model),
                    content: c.text.clone(),
                    timestamp: format_time(&c.timestamp),
                    thinking_count: c.thinking_count,
                    tool_call_count: c.tool_calls.len(),
                    output_count: count_output_items(&c.items),
                    tokens_raw: c.usage.total_tokens(),
                    context_tokens: c.usage.input_tokens + c.usage.cache_read_tokens + c.usage.cache_creation_tokens,
                    duration_ms: c.duration_ms,
                    items: convert_display_items(&c.items, subagents, color_by_tool_id),
                    last_output: frontend_lo,
                    is_error: false,
                    teammate_spawns: team_spawns,
                    teammate_messages: teammate_ids.len(),
                    subagent_label: String::new(),
                });
            }
            ChunkType::System => {
                msgs.push(DisplayMessage {
                    role: "system".to_string(),
                    model: String::new(),
                    content: c.output.clone(),
                    timestamp: format_time(&c.timestamp),
                    thinking_count: 0,
                    tool_call_count: 0,
                    output_count: 0,
                    tokens_raw: 0,
                    context_tokens: 0,
                    duration_ms: 0,
                    items: Vec::new(),
                    last_output: None,
                    is_error: c.is_error,
                    teammate_spawns: 0,
                    teammate_messages: 0,
                    subagent_label: String::new(),
                });
            }
            ChunkType::Compact => {
                msgs.push(DisplayMessage {
                    role: "compact".to_string(),
                    model: String::new(),
                    content: c.output.clone(),
                    timestamp: format_time(&c.timestamp),
                    thinking_count: 0,
                    tool_call_count: 0,
                    output_count: 0,
                    tokens_raw: 0,
                    context_tokens: 0,
                    duration_ms: 0,
                    items: Vec::new(),
                    last_output: None,
                    is_error: false,
                    teammate_spawns: 0,
                    teammate_messages: 0,
                    subagent_label: String::new(),
                });
            }
        }
    }

    msgs
}

/// Check if any chunk contains team Task items.
pub fn has_team_task_items(chunks: &[Chunk]) -> bool {
    for c in chunks {
        for item in &c.items {
            if item.item_type == DisplayItemType::Subagent && is_team_task(item) {
                return true;
            }
        }
    }
    false
}
