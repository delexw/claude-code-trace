use serde::Serialize;
use serde_json::Value;
use std::collections::{HashMap, HashSet};

use crate::parser::chunk::*;
use crate::parser::last_output::{find_last_output, LastOutput, LastOutputType};
use crate::parser::ongoing::OngoingChecker;
use crate::parser::subagent::{orphan_description_from_prompt, ProcGraph, SubagentProcess};
use crate::parser::taxonomy::ToolCategory;
use crate::parser::team::TeamSnapshot;

/// Team color pool for synthetic color assignment.
const TEAM_COLOR_POOL: &[&str] = &[
    "blue", "green", "red", "yellow", "purple", "cyan", "orange", "pink",
];

/// Frontend display item.
#[derive(Debug, Clone, Serialize)]
pub struct FrontendDisplayItem {
    pub id: String,
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
    pub agent_id: String,
    pub subagent_messages: Vec<DisplayMessage>,
    pub hook_event: String,
    pub hook_name: String,
    pub hook_command: String,
    /// All key-value pairs from the hook attachment JSON (pretty-printed).
    pub hook_metadata: String,
    /// Tool result as pretty-printed JSON when the content is an object or array.
    pub tool_result_json: String,
    pub is_orphan: bool,
    pub subagent_prompt: String,
    pub is_deferred: bool,
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
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_creation_tokens: i64,
    pub context_tokens: i64,
    pub duration_ms: i64,
    pub items: Vec<FrontendDisplayItem>,
    pub last_output: Option<FrontendLastOutput>,
    pub is_error: bool,
    pub teammate_spawns: usize,
    pub teammate_messages: usize,
    pub subagent_label: String,
}

/// Session-wide token totals (includes sidechains/subagents).
#[derive(Debug, Clone, Default, Serialize)]
pub struct SessionTotals {
    pub total_tokens: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_creation_tokens: i64,
    pub cost_usd: f64,
    pub model: String,
}

/// LoadResult holds everything needed to bootstrap the frontend.
#[derive(Debug, Clone, Serialize)]
pub struct LoadResult {
    pub messages: Vec<DisplayMessage>,
    pub teams: Vec<TeamSnapshot>,
    pub path: String,
    pub ongoing: bool,
    pub meta: crate::parser::session::SessionMeta,
    pub session_totals: SessionTotals,
}

/// Format a timestamp as "yyyy-mm-dd hh:mm:ss".
fn format_time(ts: &chrono::DateTime<chrono::Utc>) -> String {
    use chrono::Local;
    let local = ts.with_timezone(&Local);
    local.format("%Y-%m-%d %H:%M:%S").to_string()
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
    items
        .iter()
        .filter(|it| it.item_type == DisplayItemType::Output)
        .count()
}

fn display_item_type_str(t: &DisplayItemType) -> &'static str {
    match t {
        DisplayItemType::Thinking => "Thinking",
        DisplayItemType::Output => "Output",
        DisplayItemType::ToolCall => "ToolCall",
        DisplayItemType::Subagent => "Subagent",
        DisplayItemType::TeammateMessage => "TeammateMessage",
        DisplayItemType::HookEvent => "HookEvent",
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
        ToolCategory::Cron => "Cron",
        ToolCategory::Mcp => "Mcp",
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
        Some(v) => serde_json::to_string_pretty(v)
            .unwrap_or_else(|_| serde_json::to_string(v).unwrap_or_default()),
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

/// Convert display items with subagent linking and color assignment.
fn convert_display_items(
    items: &[DisplayItem],
    graph: &ProcGraph,
    color_by_tool_id: &HashMap<String, String>,
    pool_idx: &mut usize,
    visited: &HashSet<String>,
) -> Vec<FrontendDisplayItem> {
    if items.is_empty() {
        return Vec::new();
    }

    let mut out: Vec<FrontendDisplayItem> = items
        .iter()
        .enumerate()
        .map(|(idx, it)| {
            let id = if !it.tool_id.is_empty() {
                it.tool_id.clone()
            } else {
                format!("{}-{idx}", display_item_type_str(&it.item_type))
            };
            let mut fdi = FrontendDisplayItem {
                id,
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
                agent_id: String::new(),
                subagent_messages: Vec::new(),
                hook_event: it.hook_event.clone(),
                hook_name: it.hook_name.clone(),
                hook_command: it.hook_command.clone(),
                hook_metadata: pretty_json(&it.hook_metadata),
                tool_result_json: pretty_json(&it.tool_result_json),
                is_orphan: it.is_orphan,
                subagent_prompt: String::new(),
                is_deferred: it.is_deferred,
            };

            // Link subagent process if available (Subagent items and ToolCall items like Skill).
            if it.item_type == DisplayItemType::Subagent || !it.tool_id.is_empty() {
                if let Some(proc) = graph.get(it.tool_id.as_str()) {
                    fdi.subagent_ongoing = OngoingChecker::is_subagent_ongoing_deep(proc, graph);
                    fdi.agent_id = proc.id.clone();
                    fdi.subagent_prompt = proc.prompt.clone();
                    // Fallback: derive description from the agent's own prompt when the
                    // parent tool call carried no description (e.g. Skill-forked agents).
                    if fdi.subagent_desc.is_empty() && !proc.prompt.is_empty() {
                        fdi.subagent_desc = orphan_description_from_prompt(&proc.prompt);
                    }
                    if !proc.teammate_color.is_empty() {
                        fdi.team_color = proc.teammate_color.clone();
                    }
                    // Skip agents already on the current path to break circular references.
                    if !visited.contains(&proc.id) {
                        let mut child_visited = visited.clone();
                        child_visited.insert(proc.id.clone());
                        fdi.subagent_messages = chunks_to_messages_inner(
                            &proc.chunks,
                            graph,
                            color_by_tool_id,
                            &child_visited,
                        );
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

    // Assign pool colors to agent items without a team color.
    for di in &mut out {
        if di.team_color.is_empty()
            && (!di.subagent_messages.is_empty() || di.item_type == "Subagent")
        {
            di.team_color = TEAM_COLOR_POOL[*pool_idx % TEAM_COLOR_POOL.len()].to_string();
            *pool_idx += 1;
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
    let graph = ProcGraph::new(subagents);
    chunks_to_messages_inner(chunks, &graph, color_by_tool_id, &HashSet::new())
}

fn chunks_to_messages_inner(
    chunks: &[Chunk],
    graph: &ProcGraph,
    color_by_tool_id: &HashMap<String, String>,
    visited: &HashSet<String>,
) -> Vec<DisplayMessage> {
    let mut msgs = Vec::new();
    let mut pool_idx: usize = 0;

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
                    input_tokens: 0,
                    output_tokens: 0,
                    cache_read_tokens: 0,
                    cache_creation_tokens: 0,
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
                    if it.item_type == DisplayItemType::TeammateMessage
                        && !it.teammate_id.is_empty()
                    {
                        teammate_ids.insert(it.teammate_id.clone());
                    }
                }

                let lo = find_last_output(&c.items);
                let frontend_lo = lo.as_ref().map(convert_last_output);

                msgs.push(DisplayMessage {
                    role: "claude".to_string(),
                    model: short_model(&c.model),
                    content: c.text.clone(),
                    timestamp: format_time(&c.timestamp),
                    thinking_count: c.thinking_count,
                    tool_call_count: c.tool_calls.len(),
                    output_count: count_output_items(&c.items),
                    tokens_raw: c.usage.total_tokens(),
                    input_tokens: c.usage.input_tokens,
                    output_tokens: c.usage.output_tokens,
                    cache_read_tokens: c.usage.cache_read_tokens,
                    cache_creation_tokens: c.usage.cache_creation_tokens,
                    context_tokens: c.usage.input_tokens
                        + c.usage.cache_read_tokens
                        + c.usage.cache_creation_tokens,
                    duration_ms: c.duration_ms,
                    items: convert_display_items(
                        &c.items,
                        graph,
                        color_by_tool_id,
                        &mut pool_idx,
                        visited,
                    ),
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
                    input_tokens: 0,
                    output_tokens: 0,
                    cache_read_tokens: 0,
                    cache_creation_tokens: 0,
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
                    input_tokens: 0,
                    output_tokens: 0,
                    cache_read_tokens: 0,
                    cache_creation_tokens: 0,
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

#[cfg(test)]
mod tests {
    use super::*;

    // ---- short_model tests ----

    #[test]
    fn short_model_opus() {
        assert_eq!(short_model("claude-opus-4-6"), "opus4.6");
    }

    #[test]
    fn short_model_sonnet() {
        assert_eq!(short_model("claude-sonnet-4-6"), "sonnet4.6");
    }

    #[test]
    fn short_model_haiku_with_date() {
        assert_eq!(short_model("claude-haiku-4-5-20251001"), "haiku4.5");
    }

    #[test]
    fn short_model_bare_name() {
        assert_eq!(short_model("opus"), "opus");
    }

    #[test]
    fn short_model_empty() {
        assert_eq!(short_model(""), "");
    }

    // ---- tool_category_str tests ----

    #[test]
    fn tool_category_str_all_variants() {
        assert_eq!(tool_category_str(&ToolCategory::Read), "Read");
        assert_eq!(tool_category_str(&ToolCategory::Edit), "Edit");
        assert_eq!(tool_category_str(&ToolCategory::Write), "Write");
        assert_eq!(tool_category_str(&ToolCategory::Bash), "Bash");
        assert_eq!(tool_category_str(&ToolCategory::Grep), "Grep");
        assert_eq!(tool_category_str(&ToolCategory::Glob), "Glob");
        assert_eq!(tool_category_str(&ToolCategory::Task), "Task");
        assert_eq!(tool_category_str(&ToolCategory::Tool), "Tool");
        assert_eq!(tool_category_str(&ToolCategory::Web), "Web");
        assert_eq!(tool_category_str(&ToolCategory::Cron), "Cron");
        assert_eq!(tool_category_str(&ToolCategory::Other), "Other");
    }

    // ---- display_item_type_str tests ----

    #[test]
    fn display_item_type_str_all_variants() {
        assert_eq!(
            display_item_type_str(&DisplayItemType::Thinking),
            "Thinking"
        );
        assert_eq!(display_item_type_str(&DisplayItemType::Output), "Output");
        assert_eq!(
            display_item_type_str(&DisplayItemType::ToolCall),
            "ToolCall"
        );
        assert_eq!(
            display_item_type_str(&DisplayItemType::Subagent),
            "Subagent"
        );
        assert_eq!(
            display_item_type_str(&DisplayItemType::TeammateMessage),
            "TeammateMessage"
        );
        assert_eq!(
            display_item_type_str(&DisplayItemType::HookEvent),
            "HookEvent"
        );
    }

    // ---- format_time test ----

    #[test]
    fn format_time_produces_expected_format() {
        use chrono::TimeZone;
        let dt = chrono::Utc
            .with_ymd_and_hms(2025, 6, 15, 12, 30, 45)
            .unwrap();
        let result = format_time(&dt);
        // The exact output depends on local timezone, but the format should match.
        let parts: Vec<&str> = result.split(' ').collect();
        assert_eq!(parts.len(), 2);
        // Date part: YYYY-MM-DD
        assert_eq!(parts[0].len(), 10);
        assert_eq!(parts[0].chars().filter(|c| *c == '-').count(), 2);
        // Time part: HH:MM:SS
        assert_eq!(parts[1].len(), 8);
        assert_eq!(parts[1].chars().filter(|c| *c == ':').count(), 2);
    }

    // ---- TEAM_COLOR_POOL test ----

    #[test]
    fn team_color_pool_has_8_entries() {
        assert_eq!(TEAM_COLOR_POOL.len(), 8);
    }

    // ---- subagent_prompt propagation test ----

    #[test]
    fn subagent_prompt_is_propagated_from_process() {
        use crate::parser::chunk::{DisplayItem, DisplayItemType};
        use crate::parser::subagent::SubagentProcess;

        let tool_id = "toolu_test123".to_string();
        let items = vec![DisplayItem {
            item_type: DisplayItemType::Subagent,
            tool_id: tool_id.clone(),
            tool_name: "Agent".to_string(),
            subagent_type: "general-purpose".to_string(),
            subagent_desc: "test agent".to_string(),
            ..Default::default()
        }];

        let proc = SubagentProcess {
            id: "agent-abc".to_string(),
            parent_task_id: tool_id,
            prompt: "Base directory for this skill: /path/to/skill\n\n# My Skill".to_string(),
            ..Default::default()
        };
        let subagents = vec![proc];
        let graph = ProcGraph::new(&subagents);
        let color_map = std::collections::HashMap::new();
        let mut pool_idx = 0;

        let result =
            convert_display_items(&items, &graph, &color_map, &mut pool_idx, &HashSet::new());

        assert_eq!(result.len(), 1);
        assert_eq!(
            result[0].subagent_prompt,
            "Base directory for this skill: /path/to/skill\n\n# My Skill"
        );
        assert_eq!(result[0].agent_id, "agent-abc");
    }

    #[test]
    fn subagent_desc_derived_from_skill_prompt_when_empty() {
        use crate::parser::chunk::{DisplayItem, DisplayItemType};
        use crate::parser::subagent::SubagentProcess;

        let tool_id = "orphan-ae9be0e043273ded1".to_string();
        // Orphan item with empty subagent_desc (as inject_orphan_subagents may produce
        // in edge cases, e.g. Skill-forked agents with no skill_progress linking).
        let items = vec![DisplayItem {
            item_type: DisplayItemType::Subagent,
            tool_id: tool_id.clone(),
            tool_name: "Agent".to_string(),
            subagent_type: "general-purpose".to_string(),
            subagent_desc: String::new(),
            ..Default::default()
        }];

        let proc = SubagentProcess {
            id: "ae9be0e043273ded1".to_string(),
            parent_task_id: tool_id,
            prompt:
                "Base directory for this skill: /Users/yang/.claude/skills/rollbar-reader\n\n# Rollbar Reader"
                    .to_string(),
            ..Default::default()
        };
        let subagents = vec![proc];
        let graph = ProcGraph::new(&subagents);
        let color_map = std::collections::HashMap::new();
        let mut pool_idx = 0;

        let result =
            convert_display_items(&items, &graph, &color_map, &mut pool_idx, &HashSet::new());

        assert_eq!(result.len(), 1);
        assert_eq!(
            result[0].subagent_desc, "rollbar-reader",
            "skill name must be derived from prompt when subagent_desc is empty"
        );
    }

    // ---- cycle detection test ----

    #[test]
    fn circular_subagent_references_do_not_overflow() {
        use crate::parser::chunk::{Chunk, ChunkType, DisplayItem, DisplayItemType};
        use crate::parser::subagent::SubagentProcess;
        use chrono::Utc;

        // agent-a spawns agent-b, and agent-b's chunks also reference agent-a's tool ID —
        // a mutual cycle that previously caused infinite recursion and stack overflow.
        let tool_a = "toolu_aaa".to_string();
        let tool_b = "toolu_bbb".to_string();

        let chunk_a = Chunk {
            chunk_type: ChunkType::AI,
            items: vec![DisplayItem {
                item_type: DisplayItemType::Subagent,
                tool_id: tool_b.clone(),
                tool_name: "Agent".to_string(),
                ..Default::default()
            }],
            timestamp: Utc::now(),
            ..Default::default()
        };
        let chunk_b = Chunk {
            chunk_type: ChunkType::AI,
            items: vec![DisplayItem {
                item_type: DisplayItemType::Subagent,
                tool_id: tool_a.clone(),
                tool_name: "Agent".to_string(),
                ..Default::default()
            }],
            timestamp: Utc::now(),
            ..Default::default()
        };

        let proc_a = SubagentProcess {
            id: "agent-a".to_string(),
            parent_task_id: tool_a.clone(),
            chunks: vec![chunk_a],
            ..Default::default()
        };
        let proc_b = SubagentProcess {
            id: "agent-b".to_string(),
            parent_task_id: tool_b.clone(),
            chunks: vec![chunk_b],
            ..Default::default()
        };

        let subagents = vec![proc_a, proc_b];
        let color_map = std::collections::HashMap::new();

        // Main session chunk referencing agent-a — triggers the cycle.
        let main_chunk = Chunk {
            chunk_type: ChunkType::AI,
            items: vec![DisplayItem {
                item_type: DisplayItemType::Subagent,
                tool_id: tool_a.clone(),
                tool_name: "Agent".to_string(),
                ..Default::default()
            }],
            timestamp: Utc::now(),
            ..Default::default()
        };

        // Must not overflow the stack.
        let msgs = chunks_to_messages(&[main_chunk], &subagents, &color_map);

        // main → agent-a (expanded)
        assert_eq!(msgs.len(), 1);
        let main_items = &msgs[0].items;
        assert_eq!(main_items.len(), 1);
        assert_eq!(main_items[0].agent_id, "agent-a");

        // agent-a → agent-b (expanded, first visit)
        let a_msgs = &main_items[0].subagent_messages;
        assert_eq!(a_msgs.len(), 1);
        let a_items = &a_msgs[0].items;
        assert_eq!(a_items.len(), 1);
        assert_eq!(a_items[0].agent_id, "agent-b");

        // agent-b → agent-a: the link item exists but subagent_messages is empty (cycle cut)
        let b_msgs = &a_items[0].subagent_messages;
        assert_eq!(b_msgs.len(), 1);
        let b_items = &b_msgs[0].items;
        assert_eq!(b_items.len(), 1);
        assert_eq!(b_items[0].agent_id, "agent-a");
        assert!(
            b_items[0].subagent_messages.is_empty(),
            "cycle must be cut here"
        );
    }

    #[test]
    fn deferred_flag_is_propagated_to_frontend_display_item() {
        use crate::parser::chunk::{DisplayItem, DisplayItemType};

        let items = vec![DisplayItem {
            item_type: DisplayItemType::ToolCall,
            tool_id: "toolu_defer".to_string(),
            tool_name: "Read".to_string(),
            is_deferred: true,
            ..Default::default()
        }];

        let subagents = vec![];
        let graph = ProcGraph::new(&subagents);
        let color_map = std::collections::HashMap::new();
        let mut pool_idx = 0;

        let result =
            convert_display_items(&items, &graph, &color_map, &mut pool_idx, &HashSet::new());

        assert_eq!(result.len(), 1);
        assert!(
            result[0].is_deferred,
            "is_deferred must be propagated to FrontendDisplayItem"
        );
    }

    #[test]
    fn non_deferred_flag_propagates_as_false() {
        use crate::parser::chunk::{DisplayItem, DisplayItemType};

        let items = vec![DisplayItem {
            item_type: DisplayItemType::ToolCall,
            tool_id: "toolu_normal".to_string(),
            tool_name: "Bash".to_string(),
            tool_result: "output".to_string(),
            is_deferred: false,
            ..Default::default()
        }];

        let subagents = vec![];
        let graph = ProcGraph::new(&subagents);
        let color_map = std::collections::HashMap::new();
        let mut pool_idx = 0;

        let result =
            convert_display_items(&items, &graph, &color_map, &mut pool_idx, &HashSet::new());

        assert_eq!(result.len(), 1);
        assert!(!result[0].is_deferred);
    }

    #[test]
    fn subagent_prompt_is_empty_when_no_process_linked() {
        use crate::parser::chunk::{DisplayItem, DisplayItemType};

        let items = vec![DisplayItem {
            item_type: DisplayItemType::Subagent,
            tool_id: "toolu_orphan".to_string(),
            tool_name: "Agent".to_string(),
            ..Default::default()
        }];

        let subagents = vec![];
        let graph = ProcGraph::new(&subagents);
        let color_map = std::collections::HashMap::new();
        let mut pool_idx = 0;

        let result =
            convert_display_items(&items, &graph, &color_map, &mut pool_idx, &HashSet::new());

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].subagent_prompt, "");
    }

    /// End-to-end test using the real JSONL session 3f97c7ca to verify that
    /// Skill-forked agent summaries (detail-item__summary) are populated.
    /// The session has no assistant messages — all 6 agents are orphans linked
    /// via inject_orphan_subagents + the convert_display_items fallback.
    #[test]
    fn skill_forked_agents_have_subagent_desc_populated() {
        use crate::parser::chunk::build_chunks;
        use crate::parser::session::read_session_with_debug_hooks;
        use crate::parser::subagent::{discover_and_link_all, inject_orphan_subagents};

        let session_path = concat!(
            env!("HOME"),
            "/.claude/projects",
            "/-Users-yang-liu--dovepaw-workspaces--oncall-analyzer-oa-0104cf01",
            "/3f97c7ca-41e5-4a0d-b20e-beea73a63aa1.jsonl"
        );

        // Skip if the session doesn't exist on this machine.
        if !std::path::Path::new(session_path).exists() {
            return;
        }

        let (classified, _, _) = read_session_with_debug_hooks(session_path).unwrap();
        let mut chunks = build_chunks(&classified);
        let (mut all_procs, color_map) = discover_and_link_all(session_path, &chunks);
        inject_orphan_subagents(&mut chunks, &mut all_procs);

        let messages = chunks_to_messages(&chunks, &all_procs, &color_map);

        // Collect all Subagent items across all messages.
        let agent_items: Vec<_> = messages
            .iter()
            .flat_map(|m| m.items.iter())
            .filter(|it| it.item_type == "Subagent")
            .collect();

        assert!(
            !agent_items.is_empty(),
            "session must produce at least one Subagent item"
        );

        let expected_skills = [
            "rollbar-reader",
            "pagerduty-oncall",
            "datadog-analyser",
            "slack-explorer",
            "cloudflare-traffic-investigator",
            "pir",
        ];

        for item in &agent_items {
            assert!(
                !item.subagent_desc.is_empty(),
                "subagent_desc must not be empty (agent_id={})",
                item.agent_id
            );
            assert!(
                expected_skills.contains(&item.subagent_desc.as_str()),
                "subagent_desc '{}' must be a known skill name (agent_id={})",
                item.subagent_desc,
                item.agent_id
            );
        }
    }
}
