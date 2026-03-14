use chrono::{DateTime, Utc};
use lazy_static::lazy_static;
use regex::Regex;
use serde_json::Value;
use std::collections::HashSet;
use std::time::Duration;

use super::chunk::*;

/// Maximum time since last file modification before a session is dead.
pub const ONGOING_STALENESS_THRESHOLD: Duration = Duration::from_secs(60);

/// Returns true if the session is still ongoing (not stale).
/// Returns false if `is_ongoing` is false, or if `mod_time` exceeds the staleness threshold.
pub fn apply_staleness(is_ongoing: bool, mod_time: impl Into<DateTime<Utc>>) -> bool {
    if !is_ongoing {
        return false;
    }
    let elapsed = Utc::now()
        .signed_duration_since(mod_time.into())
        .to_std()
        .unwrap_or(Duration::ZERO);
    elapsed <= ONGOING_STALENESS_THRESHOLD
}

lazy_static! {
    static ref APPROVE_PATTERN: Regex = Regex::new(r#""approve"\s*:\s*true"#).unwrap();
}

/// Checks if a tool_use block is a SendMessage shutdown_response with approve: true.
pub fn is_shutdown_approval(tool_name: &str, tool_input: &Option<Value>) -> bool {
    if tool_name != "SendMessage" {
        return false;
    }
    let input = match tool_input {
        Some(v) => v,
        None => return false,
    };
    if let Some(obj) = input.as_object() {
        let msg_type = obj.get("type").and_then(|v| v.as_str()).unwrap_or("");
        let approve = obj.get("approve").and_then(|v| v.as_bool());
        if msg_type == "shutdown_response" && approve == Some(true) {
            return true;
        }
    }
    // Fallback to regex.
    APPROVE_PATTERN.is_match(&input.to_string())
}

// ---------------------------------------------------------------------------
// ActivityType — classifies each item in a chunk for ongoing analysis
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
enum ActivityType {
    TextOutput,
    Thinking,
    ToolUse,
    ToolResult,
    Interruption,
    ExitPlanMode,
}

impl ActivityType {
    fn is_ending(&self) -> bool {
        matches!(
            self,
            ActivityType::TextOutput | ActivityType::Interruption | ActivityType::ExitPlanMode
        )
    }

    fn is_ai_activity(&self) -> bool {
        matches!(
            self,
            ActivityType::Thinking | ActivityType::ToolUse | ActivityType::ToolResult
        )
    }
}

// ---------------------------------------------------------------------------
// ActivityLog — collects indexed activities from chunks and checks ongoing
// ---------------------------------------------------------------------------

struct ActivityLog {
    entries: Vec<(ActivityType, usize)>,
    has_items: bool,
}

impl ActivityLog {
    /// Scan all AI chunks and build an ordered activity log.
    fn from_chunks(chunks: &[Chunk]) -> Self {
        let mut entries: Vec<(ActivityType, usize)> = Vec::new();
        let mut idx = 0;
        let mut has_items = false;
        let mut shutdown_tool_ids: HashSet<String> = HashSet::new();

        for chunk in chunks {
            if chunk.chunk_type != ChunkType::AI || chunk.items.is_empty() {
                continue;
            }
            has_items = true;

            for item in &chunk.items {
                match item.item_type {
                    DisplayItemType::Thinking => {
                        entries.push((ActivityType::Thinking, idx));
                        idx += 1;
                    }
                    DisplayItemType::Output => {
                        if !item.text.trim().is_empty() {
                            entries.push((ActivityType::TextOutput, idx));
                            idx += 1;
                        }
                    }
                    DisplayItemType::ToolCall => {
                        if item.tool_name == "ExitPlanMode" {
                            entries.push((ActivityType::ExitPlanMode, idx));
                            idx += 1;
                        } else if is_shutdown_approval(&item.tool_name, &item.tool_input) {
                            shutdown_tool_ids.insert(item.tool_id.clone());
                            entries.push((ActivityType::Interruption, idx));
                            idx += 1;
                        } else {
                            entries.push((ActivityType::ToolUse, idx));
                            idx += 1;
                        }
                        if !item.tool_result.is_empty() {
                            if shutdown_tool_ids.contains(&item.tool_id) {
                                entries.push((ActivityType::Interruption, idx));
                            } else {
                                entries.push((ActivityType::ToolResult, idx));
                            }
                            idx += 1;
                        }
                    }
                    DisplayItemType::Subagent => {
                        entries.push((ActivityType::ToolUse, idx));
                        idx += 1;
                        if !item.tool_result.is_empty() {
                            entries.push((ActivityType::ToolResult, idx));
                            idx += 1;
                        }
                    }
                    DisplayItemType::TeammateMessage | DisplayItemType::HookEvent => {}
                }
            }
        }

        Self { entries, has_items }
    }

    /// True if the last meaningful activity is not an ending (text/interruption).
    fn is_ongoing(&self) -> bool {
        if self.entries.is_empty() {
            return false;
        }

        let last_ending_idx = self
            .entries
            .iter()
            .rev()
            .find(|(at, _)| at.is_ending())
            .map(|(_, idx)| *idx);

        match last_ending_idx {
            None => self.entries.iter().any(|(at, _)| at.is_ai_activity()),
            Some(lei) => self
                .entries
                .iter()
                .any(|(at, idx)| *idx > lei && at.is_ai_activity()),
        }
    }
}

// ---------------------------------------------------------------------------
// OngoingChecker — determines whether a session (or subagent) is ongoing
// ---------------------------------------------------------------------------

/// Checks ongoing status by combining chunk analysis, subagent state, and file staleness.
pub struct OngoingChecker<'a> {
    chunks: &'a [Chunk],
    procs: &'a [super::subagent::SubagentProcess],
    session_path: &'a str,
}

impl<'a> OngoingChecker<'a> {
    pub fn new(
        chunks: &'a [Chunk],
        procs: &'a [super::subagent::SubagentProcess],
        session_path: &'a str,
    ) -> Self {
        Self {
            chunks,
            procs,
            session_path,
        }
    }

    /// Full ongoing check: chunks → subagents → file staleness.
    pub fn is_ongoing(&self) -> bool {
        // If the main session chunks are still in progress, gate on the main file freshness.
        if self.chunks_ongoing() {
            return self.is_file_fresh();
        }
        // If any subagent is still running, the session is ongoing.
        // Subagent staleness is already checked per-file inside is_subagent_ongoing,
        // so we don't re-check the main session file here.
        self.any_subagent_ongoing()
    }

    /// Check if a subagent process is ongoing (chunk-based + file staleness).
    pub fn is_subagent_ongoing(proc: &super::subagent::SubagentProcess) -> bool {
        Self::is_chunks_ongoing(&proc.chunks) && apply_staleness(true, proc.file_mod_time)
    }

    /// Cascading check: returns true if `proc` or any of its descendant subagents are ongoing.
    pub fn is_subagent_ongoing_deep(
        proc: &super::subagent::SubagentProcess,
        all_procs: &[super::subagent::SubagentProcess],
    ) -> bool {
        if Self::is_subagent_ongoing(proc) {
            return true;
        }
        // Collect tool_ids from this proc's chunks to find child processes.
        let child_tool_ids: HashSet<&str> = proc
            .chunks
            .iter()
            .flat_map(|c| c.items.iter())
            .filter(|it| {
                it.item_type == DisplayItemType::Subagent
                    || ((it.tool_name == "Task" || it.tool_name == "Agent")
                        && it.item_type == DisplayItemType::ToolCall)
            })
            .map(|it| it.tool_id.as_str())
            .collect();
        if child_tool_ids.is_empty() {
            return false;
        }
        all_procs
            .iter()
            .filter(|p| child_tool_ids.contains(p.parent_task_id.as_str()))
            .any(|p| Self::is_subagent_ongoing_deep(p, all_procs))
    }

    /// Reports whether the chunks indicate the session is still in progress.
    pub fn is_chunks_ongoing(chunks: &[Chunk]) -> bool {
        if chunks.is_empty() {
            return false;
        }

        // Trailing user prompt means Claude is processing.
        if chunks.last().map(|c| &c.chunk_type) == Some(&ChunkType::User) {
            return true;
        }

        let activity_log = ActivityLog::from_chunks(chunks);

        if activity_log.has_items {
            if activity_log.is_ongoing() {
                return true;
            }
            return Self::has_pending_agents(chunks);
        }

        // Fallback for old-style chunks.
        for c in chunks.iter().rev() {
            if c.chunk_type == ChunkType::AI {
                return c.stop_reason != "end_turn";
            }
        }

        false
    }

    // -- Private helpers --

    fn chunks_ongoing(&self) -> bool {
        Self::is_chunks_ongoing(self.chunks)
    }

    fn any_subagent_ongoing(&self) -> bool {
        self.procs.iter().any(Self::is_subagent_ongoing)
    }

    fn is_file_fresh(&self) -> bool {
        if let Ok(info) = std::fs::metadata(self.session_path) {
            if let Ok(modified) = info.modified() {
                return apply_staleness(true, modified);
            }
        }
        true // If we can't read metadata, assume fresh
    }

    fn has_pending_agents(chunks: &[Chunk]) -> bool {
        for chunk in chunks {
            if chunk.chunk_type != ChunkType::AI {
                continue;
            }
            for item in &chunk.items {
                match item.item_type {
                    DisplayItemType::Subagent => {
                        if item.tool_result.is_empty() {
                            return true;
                        }
                    }
                    DisplayItemType::ToolCall => {
                        if (item.tool_name == "Task" || item.tool_name == "Agent")
                            && item.tool_result.is_empty()
                        {
                            return true;
                        }
                    }
                    _ => {}
                }
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};
    use serde_json::json;

    fn make_chunk(chunk_type: ChunkType) -> Chunk {
        Chunk {
            chunk_type,
            ..Default::default()
        }
    }

    fn make_item(item_type: DisplayItemType) -> DisplayItem {
        DisplayItem {
            item_type,
            ..Default::default()
        }
    }

    // --- apply_staleness tests ---

    #[test]
    fn apply_staleness_false_when_not_ongoing() {
        assert!(!apply_staleness(false, Utc::now()));
    }

    #[test]
    fn apply_staleness_true_when_ongoing_and_recent() {
        let recent = Utc::now() - Duration::seconds(10);
        assert!(apply_staleness(true, recent));
    }

    #[test]
    fn apply_staleness_false_when_ongoing_and_stale() {
        let stale = Utc::now() - Duration::seconds(61);
        assert!(!apply_staleness(true, stale));
    }

    // --- is_shutdown_approval tests ---

    #[test]
    fn is_shutdown_approval_true_for_valid_shutdown() {
        let input = Some(json!({
            "type": "shutdown_response",
            "approve": true
        }));
        assert!(is_shutdown_approval("SendMessage", &input));
    }

    #[test]
    fn is_shutdown_approval_false_for_other_tools() {
        let input = Some(json!({
            "type": "shutdown_response",
            "approve": true
        }));
        assert!(!is_shutdown_approval("Bash", &input));
    }

    #[test]
    fn is_shutdown_approval_false_for_missing_fields() {
        let input = Some(json!({"type": "other_type"}));
        assert!(!is_shutdown_approval("SendMessage", &input));
    }

    #[test]
    fn is_shutdown_approval_false_for_none_input() {
        assert!(!is_shutdown_approval("SendMessage", &None));
    }

    // --- ActivityType tests ---

    #[test]
    fn activity_type_is_ending() {
        assert!(ActivityType::TextOutput.is_ending());
        assert!(ActivityType::Interruption.is_ending());
        assert!(ActivityType::ExitPlanMode.is_ending());
        assert!(!ActivityType::Thinking.is_ending());
        assert!(!ActivityType::ToolUse.is_ending());
        assert!(!ActivityType::ToolResult.is_ending());
    }

    #[test]
    fn activity_type_is_ai_activity() {
        assert!(ActivityType::Thinking.is_ai_activity());
        assert!(ActivityType::ToolUse.is_ai_activity());
        assert!(ActivityType::ToolResult.is_ai_activity());
        assert!(!ActivityType::TextOutput.is_ai_activity());
        assert!(!ActivityType::Interruption.is_ai_activity());
        assert!(!ActivityType::ExitPlanMode.is_ai_activity());
    }

    // --- ActivityLog tests ---

    #[test]
    fn activity_log_empty_chunks() {
        let log = ActivityLog::from_chunks(&[]);
        assert!(!log.has_items);
        assert!(!log.is_ongoing());
    }

    #[test]
    fn activity_log_tool_use_without_result_is_ongoing() {
        let mut chunk = make_chunk(ChunkType::AI);
        chunk.items.push(DisplayItem {
            item_type: DisplayItemType::ToolCall,
            tool_name: "Bash".to_string(),
            tool_result: String::new(),
            ..Default::default()
        });
        let log = ActivityLog::from_chunks(&[chunk]);
        assert!(log.has_items);
        assert!(log.is_ongoing());
    }

    #[test]
    fn activity_log_text_after_tool_not_ongoing() {
        let mut chunk = make_chunk(ChunkType::AI);
        chunk.items.push(DisplayItem {
            item_type: DisplayItemType::ToolCall,
            tool_name: "Bash".to_string(),
            tool_result: "done".to_string(),
            ..Default::default()
        });
        chunk.items.push(DisplayItem {
            item_type: DisplayItemType::Output,
            text: "Here is the result".to_string(),
            ..Default::default()
        });
        let log = ActivityLog::from_chunks(&[chunk]);
        assert!(log.has_items);
        assert!(!log.is_ongoing());
    }

    // --- OngoingChecker::is_chunks_ongoing tests ---

    #[test]
    fn chunks_ongoing_empty_returns_false() {
        assert!(!OngoingChecker::is_chunks_ongoing(&[]));
    }

    #[test]
    fn chunks_ongoing_trailing_user_returns_true() {
        let chunks = vec![make_chunk(ChunkType::AI), make_chunk(ChunkType::User)];
        assert!(OngoingChecker::is_chunks_ongoing(&chunks));
    }

    #[test]
    fn chunks_ongoing_end_turn_returns_false() {
        let mut ai_chunk = make_chunk(ChunkType::AI);
        ai_chunk.stop_reason = "end_turn".to_string();
        assert!(!OngoingChecker::is_chunks_ongoing(&[ai_chunk]));
    }

    #[test]
    fn chunks_ongoing_pending_agent_returns_true() {
        let mut ai_chunk = make_chunk(ChunkType::AI);
        ai_chunk.items.push(DisplayItem {
            item_type: DisplayItemType::Subagent,
            tool_name: "Task".to_string(),
            tool_result: String::new(),
            ..Default::default()
        });
        assert!(OngoingChecker::is_chunks_ongoing(&[ai_chunk]));
    }

    #[test]
    fn chunks_ongoing_text_at_end_returns_false() {
        let mut ai_chunk = make_chunk(ChunkType::AI);
        ai_chunk.items.push(DisplayItem {
            item_type: DisplayItemType::ToolCall,
            tool_name: "Bash".to_string(),
            tool_result: "done".to_string(),
            ..Default::default()
        });
        ai_chunk.items.push(DisplayItem {
            item_type: DisplayItemType::Output,
            text: "Here is the result".to_string(),
            ..Default::default()
        });
        assert!(!OngoingChecker::is_chunks_ongoing(&[ai_chunk]));
    }

    #[test]
    fn chunks_ongoing_tool_after_text_returns_true() {
        let mut ai_chunk = make_chunk(ChunkType::AI);
        ai_chunk.items.push(DisplayItem {
            item_type: DisplayItemType::Output,
            text: "Let me check".to_string(),
            ..Default::default()
        });
        ai_chunk.items.push(DisplayItem {
            item_type: DisplayItemType::ToolCall,
            tool_name: "Bash".to_string(),
            tool_result: String::new(),
            ..Default::default()
        });
        assert!(OngoingChecker::is_chunks_ongoing(&[ai_chunk]));
    }

    // --- OngoingChecker::is_ongoing tests ---

    #[test]
    fn checker_not_ongoing_when_chunks_finished() {
        let mut chunk = make_chunk(ChunkType::AI);
        chunk.stop_reason = "end_turn".to_string();
        let chunks = vec![chunk];

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session.jsonl");
        std::fs::write(&path, "data").unwrap();

        let checker = OngoingChecker::new(&chunks, &[], path.to_str().unwrap());
        assert!(!checker.is_ongoing());
    }

    #[test]
    fn checker_ongoing_when_trailing_user_chunk() {
        let chunks = vec![make_chunk(ChunkType::User)];

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session.jsonl");
        std::fs::write(&path, "data").unwrap();

        let checker = OngoingChecker::new(&chunks, &[], path.to_str().unwrap());
        assert!(checker.is_ongoing());
    }

    #[test]
    fn checker_ongoing_from_subagent_even_when_chunks_done() {
        let mut chunk = make_chunk(ChunkType::AI);
        chunk.stop_reason = "end_turn".to_string();
        let chunks = vec![chunk];

        let dir = tempfile::tempdir().unwrap();
        let session_path = dir.path().join("session.jsonl");
        std::fs::write(&session_path, "data").unwrap();

        let proc = super::super::subagent::SubagentProcess {
            chunks: vec![make_chunk(ChunkType::User)],
            file_mod_time: Utc::now(),
            ..Default::default()
        };

        let procs = [proc];
        let checker = OngoingChecker::new(&chunks, &procs, session_path.to_str().unwrap());
        assert!(checker.is_ongoing());
    }

    #[test]
    fn checker_not_ongoing_when_file_stale() {
        let chunks = vec![make_chunk(ChunkType::User)];

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session.jsonl");
        std::fs::write(&path, "data").unwrap();

        let stale_time = std::time::SystemTime::now() - std::time::Duration::from_secs(60);
        filetime::set_file_mtime(&path, filetime::FileTime::from_system_time(stale_time)).unwrap();

        let checker = OngoingChecker::new(&chunks, &[], path.to_str().unwrap());
        assert!(!checker.is_ongoing());
    }

    // --- is_shutdown_approval edge cases ---

    #[test]
    fn is_shutdown_approval_approve_false_returns_false() {
        let input = Some(json!({"type": "shutdown_response", "approve": false}));
        assert!(!is_shutdown_approval("SendMessage", &input));
    }

    #[test]
    fn is_shutdown_approval_regex_fallback() {
        // Array value — not an object, so struct check is skipped, falls back to regex.
        let input = Some(json!([{"type": "shutdown_response", "approve": true}]));
        assert!(is_shutdown_approval("SendMessage", &input));
    }

    // --- ActivityLog edge cases ---

    #[test]
    fn activity_log_thinking_only_is_ongoing() {
        let mut chunk = make_chunk(ChunkType::AI);
        chunk.items.push(make_item(DisplayItemType::Thinking));
        let log = ActivityLog::from_chunks(&[chunk]);
        assert!(log.has_items);
        assert!(log.is_ongoing());
    }

    #[test]
    fn activity_log_empty_output_text_skipped() {
        let mut chunk = make_chunk(ChunkType::AI);
        chunk.items.push(DisplayItem {
            item_type: DisplayItemType::Output,
            text: "   ".to_string(), // whitespace only
            ..Default::default()
        });
        let log = ActivityLog::from_chunks(&[chunk]);
        assert!(log.has_items);
        // No entries recorded (empty text skipped), so not ongoing.
        assert!(!log.is_ongoing());
    }

    #[test]
    fn activity_log_exit_plan_mode_ends_session() {
        let mut chunk = make_chunk(ChunkType::AI);
        chunk.items.push(DisplayItem {
            item_type: DisplayItemType::ToolCall,
            tool_name: "Bash".to_string(),
            tool_result: "ok".to_string(),
            ..Default::default()
        });
        chunk.items.push(DisplayItem {
            item_type: DisplayItemType::ToolCall,
            tool_name: "ExitPlanMode".to_string(),
            ..Default::default()
        });
        let log = ActivityLog::from_chunks(&[chunk]);
        assert!(!log.is_ongoing());
    }

    #[test]
    fn activity_log_shutdown_approval_ends_session() {
        let mut chunk = make_chunk(ChunkType::AI);
        chunk.items.push(DisplayItem {
            item_type: DisplayItemType::ToolCall,
            tool_name: "SendMessage".to_string(),
            tool_input: Some(json!({"type": "shutdown_response", "approve": true})),
            tool_id: "t1".to_string(),
            tool_result: "ack".to_string(),
            ..Default::default()
        });
        let log = ActivityLog::from_chunks(&[chunk]);
        // Shutdown approval + its result are both Interruption → ending.
        assert!(!log.is_ongoing());
    }

    #[test]
    fn activity_log_subagent_with_result_not_ongoing() {
        let mut chunk = make_chunk(ChunkType::AI);
        chunk.items.push(DisplayItem {
            item_type: DisplayItemType::Subagent,
            tool_name: "Task".to_string(),
            tool_result: "done".to_string(),
            ..Default::default()
        });
        // ToolUse + ToolResult with no ending after → ongoing by activity analysis,
        // but no pending agents either.
        let log = ActivityLog::from_chunks(&[chunk]);
        // ToolResult is not an ending, and there's AI activity after nothing → ongoing.
        assert!(log.is_ongoing());
    }

    #[test]
    fn activity_log_subagent_followed_by_text_not_ongoing() {
        let mut chunk = make_chunk(ChunkType::AI);
        chunk.items.push(DisplayItem {
            item_type: DisplayItemType::Subagent,
            tool_name: "Task".to_string(),
            tool_result: "done".to_string(),
            ..Default::default()
        });
        chunk.items.push(DisplayItem {
            item_type: DisplayItemType::Output,
            text: "All done".to_string(),
            ..Default::default()
        });
        let log = ActivityLog::from_chunks(&[chunk]);
        assert!(!log.is_ongoing());
    }

    #[test]
    fn activity_log_skips_teammate_and_hook_items() {
        let mut chunk = make_chunk(ChunkType::AI);
        chunk
            .items
            .push(make_item(DisplayItemType::TeammateMessage));
        chunk.items.push(make_item(DisplayItemType::HookEvent));
        let log = ActivityLog::from_chunks(&[chunk]);
        assert!(log.has_items); // chunk had items
        assert!(log.entries.is_empty()); // but none produced activities
        assert!(!log.is_ongoing());
    }

    #[test]
    fn activity_log_skips_user_chunks() {
        let user_chunk = make_chunk(ChunkType::User);
        let log = ActivityLog::from_chunks(&[user_chunk]);
        assert!(!log.has_items);
    }

    // --- OngoingChecker::is_subagent_ongoing tests ---

    #[test]
    fn subagent_ongoing_with_trailing_user_and_fresh_file() {
        let proc = super::super::subagent::SubagentProcess {
            chunks: vec![make_chunk(ChunkType::User)],
            file_mod_time: Utc::now(),
            ..Default::default()
        };
        assert!(OngoingChecker::is_subagent_ongoing(&proc));
    }

    #[test]
    fn subagent_not_ongoing_when_file_stale() {
        let proc = super::super::subagent::SubagentProcess {
            chunks: vec![make_chunk(ChunkType::User)],
            file_mod_time: Utc::now() - Duration::seconds(120),
            ..Default::default()
        };
        assert!(!OngoingChecker::is_subagent_ongoing(&proc));
    }

    #[test]
    fn subagent_not_ongoing_when_chunks_finished() {
        let mut chunk = make_chunk(ChunkType::AI);
        chunk.stop_reason = "end_turn".to_string();
        let proc = super::super::subagent::SubagentProcess {
            chunks: vec![chunk],
            file_mod_time: Utc::now(),
            ..Default::default()
        };
        assert!(!OngoingChecker::is_subagent_ongoing(&proc));
    }

    // --- OngoingChecker::is_subagent_ongoing_deep tests ---

    #[test]
    fn deep_ongoing_parent_done_child_ongoing() {
        // Parent proc is finished (subagent has result + text output after),
        // but a child process is still ongoing.
        let mut parent_chunk = make_chunk(ChunkType::AI);
        parent_chunk.stop_reason = "end_turn".to_string();
        parent_chunk.items.push(DisplayItem {
            item_type: DisplayItemType::Subagent,
            tool_name: "Agent".to_string(),
            tool_id: "child-task-1".to_string(),
            tool_result: "done".to_string(),
            ..Default::default()
        });
        parent_chunk.items.push(DisplayItem {
            item_type: DisplayItemType::Output,
            text: "All done".to_string(),
            ..Default::default()
        });
        let parent = super::super::subagent::SubagentProcess {
            id: "parent-id".to_string(),
            chunks: vec![parent_chunk],
            file_mod_time: Utc::now(),
            ..Default::default()
        };

        let child = super::super::subagent::SubagentProcess {
            id: "child-id".to_string(),
            parent_task_id: "child-task-1".to_string(),
            chunks: vec![make_chunk(ChunkType::User)], // trailing user = ongoing
            file_mod_time: Utc::now(),
            ..Default::default()
        };

        let all = vec![parent.clone(), child];
        assert!(!OngoingChecker::is_subagent_ongoing(&parent));
        assert!(OngoingChecker::is_subagent_ongoing_deep(&parent, &all));
    }

    #[test]
    fn deep_ongoing_grandchild_ongoing() {
        // Parent → child (done) → grandchild (ongoing).
        let mut parent_chunk = make_chunk(ChunkType::AI);
        parent_chunk.stop_reason = "end_turn".to_string();
        parent_chunk.items.push(DisplayItem {
            item_type: DisplayItemType::Subagent,
            tool_name: "Agent".to_string(),
            tool_id: "task-c1".to_string(),
            tool_result: "done".to_string(),
            ..Default::default()
        });
        parent_chunk.items.push(DisplayItem {
            item_type: DisplayItemType::Output,
            text: "Done".to_string(),
            ..Default::default()
        });
        let parent = super::super::subagent::SubagentProcess {
            id: "p".to_string(),
            chunks: vec![parent_chunk],
            file_mod_time: Utc::now(),
            ..Default::default()
        };

        let mut child_chunk = make_chunk(ChunkType::AI);
        child_chunk.stop_reason = "end_turn".to_string();
        child_chunk.items.push(DisplayItem {
            item_type: DisplayItemType::Subagent,
            tool_name: "Agent".to_string(),
            tool_id: "task-gc1".to_string(),
            tool_result: "done".to_string(),
            ..Default::default()
        });
        child_chunk.items.push(DisplayItem {
            item_type: DisplayItemType::Output,
            text: "Done".to_string(),
            ..Default::default()
        });
        let child = super::super::subagent::SubagentProcess {
            id: "c".to_string(),
            parent_task_id: "task-c1".to_string(),
            chunks: vec![child_chunk],
            file_mod_time: Utc::now(),
            ..Default::default()
        };

        let grandchild = super::super::subagent::SubagentProcess {
            id: "gc".to_string(),
            parent_task_id: "task-gc1".to_string(),
            chunks: vec![make_chunk(ChunkType::User)],
            file_mod_time: Utc::now(),
            ..Default::default()
        };

        let all = vec![parent.clone(), child, grandchild];
        assert!(OngoingChecker::is_subagent_ongoing_deep(&parent, &all));
    }

    #[test]
    fn deep_ongoing_all_done() {
        let mut parent_chunk = make_chunk(ChunkType::AI);
        parent_chunk.stop_reason = "end_turn".to_string();
        parent_chunk.items.push(DisplayItem {
            item_type: DisplayItemType::Subagent,
            tool_name: "Agent".to_string(),
            tool_id: "task-c1".to_string(),
            tool_result: "done".to_string(),
            ..Default::default()
        });
        parent_chunk.items.push(DisplayItem {
            item_type: DisplayItemType::Output,
            text: "Done".to_string(),
            ..Default::default()
        });
        let parent = super::super::subagent::SubagentProcess {
            id: "p".to_string(),
            chunks: vec![parent_chunk],
            file_mod_time: Utc::now(),
            ..Default::default()
        };

        let mut child_chunk = make_chunk(ChunkType::AI);
        child_chunk.stop_reason = "end_turn".to_string();
        let child = super::super::subagent::SubagentProcess {
            id: "c".to_string(),
            parent_task_id: "task-c1".to_string(),
            chunks: vec![child_chunk],
            file_mod_time: Utc::now(),
            ..Default::default()
        };

        let all = vec![parent.clone(), child];
        assert!(!OngoingChecker::is_subagent_ongoing_deep(&parent, &all));
    }

    // --- OngoingChecker::is_chunks_ongoing edge cases ---

    #[test]
    fn chunks_ongoing_pending_agent_tool_call_returns_true() {
        let mut chunk = make_chunk(ChunkType::AI);
        chunk.items.push(DisplayItem {
            item_type: DisplayItemType::ToolCall,
            tool_name: "Agent".to_string(),
            tool_result: String::new(),
            ..Default::default()
        });
        // Text ending before it.
        chunk.items.insert(
            0,
            DisplayItem {
                item_type: DisplayItemType::Output,
                text: "Launching agent".to_string(),
                ..Default::default()
            },
        );
        assert!(OngoingChecker::is_chunks_ongoing(&[chunk]));
    }

    #[test]
    fn chunks_ongoing_old_style_no_items_not_end_turn() {
        // AI chunk with no structured items and non-end_turn stop reason → ongoing.
        let mut chunk = make_chunk(ChunkType::AI);
        chunk.stop_reason = "max_tokens".to_string();
        assert!(OngoingChecker::is_chunks_ongoing(&[chunk]));
    }

    #[test]
    fn chunks_ongoing_multiple_chunks_last_ai_decides() {
        // First AI finished, second AI still going.
        let mut ai1 = make_chunk(ChunkType::AI);
        ai1.stop_reason = "end_turn".to_string();
        let mut ai2 = make_chunk(ChunkType::AI);
        ai2.items.push(DisplayItem {
            item_type: DisplayItemType::ToolCall,
            tool_name: "Read".to_string(),
            tool_result: String::new(),
            ..Default::default()
        });
        assert!(OngoingChecker::is_chunks_ongoing(&[ai1, ai2]));
    }
}
