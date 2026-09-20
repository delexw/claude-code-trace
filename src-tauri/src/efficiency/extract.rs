use std::collections::HashMap;

use crate::convert::DisplayMessage;

use super::settings::PayloadMode;
use super::{EfficiencyAction, EfficiencyInput, EfficiencySignals, EfficiencyTask};

const MAX_SUMMARY_CHARS: usize = 500;
const MAX_EXCERPT_CHARS: usize = 1_200;
const MAX_FULL_TRANSCRIPT_ENTRY_CHARS: usize = 5_000;
const MAX_EXCERPTS: usize = 24;

fn truncate(value: &str, limit: usize) -> String {
    if value.chars().count() <= limit {
        return value.to_string();
    }
    let mut result: String = value.chars().take(limit).collect();
    result.push('…');
    result
}

fn normalized_action_key(tool: &str, identity: &str) -> String {
    format!(
        "{}:{}",
        tool.to_lowercase(),
        identity.split_whitespace().collect::<Vec<_>>().join(" ")
    )
}

/// The text describing one action to Jev.
///
/// `tool_summary` is built for the UI and is capped well below what the model
/// needs — a Bash call is cut to 60 characters, so two different commands that
/// share a prefix arrive indistinguishable. When the display summary was
/// truncated (or is absent) fall back to the raw tool input, which is what
/// actually distinguishes one call from another.
fn action_detail(tool_summary: &str, tool_input: &str) -> String {
    let summary = tool_summary.trim();
    let lost_detail = summary.is_empty() || summary.ends_with('\u{2026}');
    if !lost_detail {
        return summary.to_string();
    }
    let input = tool_input.trim();
    if input.is_empty() {
        return summary.to_string();
    }
    truncate(input, MAX_SUMMARY_CHARS)
}

/// Pick `cap` entries spread evenly across `candidates`, always keeping the
/// first and the last. Used so a long session is sampled end to end instead of
/// being cut off after the first N matches.
fn evenly_spaced(candidates: &[usize], cap: usize) -> Vec<usize> {
    if cap == 0 {
        return Vec::new();
    }
    if candidates.len() <= cap {
        return candidates.to_vec();
    }
    if cap == 1 {
        return candidates.last().copied().into_iter().collect();
    }
    let last = candidates.len() - 1;
    (0..cap).map(|i| candidates[i * last / (cap - 1)]).collect()
}

fn has_failure(message: &DisplayMessage) -> bool {
    message.is_error || message.items.iter().any(|item| item.tool_error)
}

/// Choose which messages to excerpt for Jev.
///
/// The old filter kept user messages and failures, then took the first 24. That
/// meant a successful assistant message never qualified, so Jev was asked whether
/// the task completed while seeing only requests and errors — and on a long
/// session it only ever saw the opening.
///
/// The outcome (the final assistant message with content) is reserved first so it
/// can never be squeezed out. Requests and failures come next, sampled across the
/// whole timeline. Any leftover capacity is filled with successful assistant
/// messages, so the sample is not purely negative.
fn excerpt_indices(messages: &[DisplayMessage], cap: usize) -> Vec<usize> {
    let has_text = |m: &DisplayMessage| !m.content.trim().is_empty();
    let outcome = messages
        .iter()
        .rposition(|m| m.role == "claude" && has_text(m));

    let mut kept: Vec<usize> = outcome.into_iter().collect();
    if kept.len() >= cap {
        return kept;
    }

    let problems: Vec<usize> = messages
        .iter()
        .enumerate()
        .filter(|(i, m)| {
            !kept.contains(i)
                && (m.role == "user" || has_failure(m))
                && (has_text(m) || has_failure(m))
        })
        .map(|(i, _)| i)
        .collect();
    kept.extend(evenly_spaced(&problems, cap - kept.len()));

    if kept.len() < cap {
        let successes: Vec<usize> = messages
            .iter()
            .enumerate()
            .filter(|(i, m)| !kept.contains(i) && m.role == "claude" && has_text(m))
            .map(|(i, _)| i)
            .collect();
        kept.extend(evenly_spaced(&successes, cap - kept.len()));
    }

    kept.sort_unstable();
    kept.dedup();
    kept
}

pub fn extract_input(
    messages: &[DisplayMessage],
    total_tokens: i64,
    mode: PayloadMode,
) -> EfficiencyInput {
    let first_user_message = messages
        .iter()
        .find(|message| message.role == "user")
        .map(|message| truncate(&message.content, MAX_EXCERPT_CHARS))
        .unwrap_or_default();

    let mut raw_actions = Vec::new();
    let mut counts = HashMap::<String, usize>::new();
    let mut subagent_count = 0;
    let mut failed_tool_calls = 0;
    let mut min_context = i64::MAX;
    let mut max_context = 0;
    let mut thinking_blocks = 0;
    let mut thinking_chars = 0_i64;

    for (message_index, message) in messages.iter().enumerate() {
        if message.context_tokens > 0 {
            min_context = min_context.min(message.context_tokens);
            max_context = max_context.max(message.context_tokens);
        }
        for item in &message.items {
            if item.item_type == "Thinking" {
                thinking_blocks += 1;
                thinking_chars += item.text.chars().count() as i64;
            }
            // A Task/Agent spawn is a `Subagent` item, not a `ToolCall`. Skipping
            // those hid every subagent from the payload: a session could run 100 of
            // them and still report subagentCount 0, so `subagentsUseful` was
            // answered on no evidence and the work was invisible to every metric.
            if item.item_type != "ToolCall" && item.item_type != "Subagent" {
                continue;
            }
            let summary = action_detail(&item.tool_summary, &item.tool_input);
            // Key on the full input, never the display summary: the summary is
            // truncated for the UI and collapses genuinely different calls.
            let identity = if item.tool_input.trim().is_empty() {
                summary.clone()
            } else {
                item.tool_input.clone()
            };
            let key = normalized_action_key(&item.tool_name, &identity);
            *counts.entry(key.clone()).or_default() += 1;
            if item.tool_error {
                failed_tool_calls += 1;
            }
            // Count real spawns only. Keying on the Task *category* counted
            // TaskCreate/TaskUpdate/TaskList/TeamCreate/SendMessage instead.
            if item.item_type == "Subagent" {
                subagent_count += 1;
            }
            raw_actions.push((message_index, item, summary, key));
        }
    }

    let actions = raw_actions
        .into_iter()
        .map(|(message_index, item, summary, key)| EfficiencyAction {
            index: message_index,
            tool: item.tool_name.clone(),
            category: item.tool_category.clone(),
            summary,
            duration_ms: item.duration_ms,
            error: item.tool_error,
            repeated_similar_call_count: counts.get(&key).copied().unwrap_or(1).saturating_sub(1),
        })
        .collect::<Vec<_>>();

    let chosen: Vec<usize> = if mode == PayloadMode::FullTranscript {
        (0..messages.len()).collect()
    } else {
        excerpt_indices(messages, MAX_EXCERPTS)
    };
    let selected_excerpts = chosen
        .into_iter()
        .map(|index| &messages[index])
        .map(|message| {
            let mut excerpt = format!("{}: {}", message.role, message.content);
            if mode == PayloadMode::FullTranscript {
                for item in &message.items {
                    excerpt.push_str(&format!(
                        "\n{} {}\n{}",
                        item.tool_name, item.tool_input, item.tool_result
                    ));
                }
            }
            truncate(
                &excerpt,
                if mode == PayloadMode::FullTranscript {
                    MAX_FULL_TRANSCRIPT_ENTRY_CHARS
                } else {
                    MAX_EXCERPT_CHARS
                },
            )
        })
        .collect();

    let duration_ms = messages.iter().map(|message| message.duration_ms).sum();
    let repeated_tool_calls = actions
        .iter()
        .filter(|action| action.repeated_similar_call_count > 0)
        .count();

    EfficiencyInput {
        task: EfficiencyTask {
            first_user_message,
            turns: messages.len(),
            duration_ms,
            total_tokens,
        },
        signals: EfficiencySignals {
            tool_calls: actions.len(),
            failed_tool_calls,
            repeated_tool_calls,
            subagent_count,
            context_growth: if min_context == i64::MAX {
                0
            } else {
                max_context.saturating_sub(min_context)
            },
            thinking_blocks,
            thinking_chars,
        },
        actions,
        selected_excerpts,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::convert::{DisplayMessage, FrontendDisplayItem};

    fn message(role: &str, content: &str, tool: Option<&str>) -> DisplayMessage {
        DisplayMessage {
            role: role.into(),
            model: String::new(),
            content: content.into(),
            timestamp: String::new(),
            thinking_count: 0,
            tool_call_count: usize::from(tool.is_some()),
            output_count: 0,
            tokens_raw: 0,
            input_tokens: 0,
            output_tokens: 0,
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
            context_tokens: 100,
            duration_ms: 10,
            items: tool
                .map(|name| {
                    vec![FrontendDisplayItem {
                        id: "1".into(),
                        item_type: "ToolCall".into(),
                        text: String::new(),
                        tool_name: name.into(),
                        tool_summary: "src/main.rs".into(),
                        tool_category: "Read".into(),
                        tool_input: String::new(),
                        tool_result: String::new(),
                        tool_error: false,
                        duration_ms: 3,
                        token_count: 0,
                        subagent_type: String::new(),
                        subagent_desc: String::new(),
                        team_member_name: String::new(),
                        teammate_id: String::new(),
                        team_color: String::new(),
                        subagent_ongoing: false,
                        agent_id: String::new(),
                        subagent_messages: vec![],
                        hook_event: String::new(),
                        hook_name: String::new(),
                        hook_command: String::new(),
                        hook_metadata: String::new(),
                        tool_result_json: String::new(),
                        is_orphan: false,
                        subagent_prompt: String::new(),
                        is_deferred: false,
                        hook_source_agent_name: String::new(),
                        hook_requesting_agent_uuid: String::new(),
                        advisor_model: String::new(),
                    }]
                })
                .unwrap_or_default(),
            last_output: None,
            is_error: false,
            teammate_spawns: 0,
            teammate_messages: 0,
            subagent_label: String::new(),
        }
    }

    fn bash_message(description: &str, command: &str) -> DisplayMessage {
        let mut m = message("claude", "", Some("Bash"));
        let input = serde_json::json!({"description": description, "command": command});
        m.items[0].tool_summary =
            crate::parser::summary::tool_summary("Bash", &Some(input.clone()));
        m.items[0].tool_input = input.to_string();
        m
    }

    fn subagent_message(subagent_type: &str, desc: &str) -> DisplayMessage {
        let mut m = message("claude", "", Some("Task"));
        m.items[0].item_type = "Subagent".into();
        m.items[0].tool_category = "Task".into();
        m.items[0].subagent_type = subagent_type.into();
        m.items[0].subagent_desc = desc.into();
        m.items[0].tool_summary = format!("{subagent_type} - {desc}");
        m.items[0].tool_input = format!("{{\"subagentType\":\"{subagent_type}\"}}");
        m
    }

    fn plain(role: &str, content: &str) -> DisplayMessage {
        message(role, content, None)
    }

    fn thinking_message(texts: &[&str]) -> DisplayMessage {
        let mut m = plain("claude", "");
        m.thinking_count = texts.len();
        m.items = texts
            .iter()
            .map(|text| {
                let mut item = message("claude", "", Some("Read")).items.remove(0);
                item.item_type = "Thinking".into();
                item.text = (*text).into();
                item.tool_name = String::new();
                item.tool_summary = String::new();
                item.tool_category = String::new();
                item
            })
            .collect();
        m
    }

    #[test]
    fn thinking_volume_reaches_jev_without_the_thinking_text() {
        let input = extract_input(
            &[
                plain("user", "do it"),
                thinking_message(&["weighing the options", "still weighing"]),
                plain("claude", "done"),
                thinking_message(&["one more thought"]),
            ],
            0,
            PayloadMode::Minimized,
        );

        assert_eq!(input.signals.thinking_blocks, 3);
        assert_eq!(input.signals.thinking_chars, 20 + 14 + 16);
        let payload = serde_json::to_string(&input).unwrap();
        assert!(
            !payload.contains("weighing the options"),
            "thinking text must never leave the device; got {payload}"
        );
    }

    #[test]
    fn a_thinking_block_is_not_counted_as_a_tool_call() {
        let input = extract_input(&[thinking_message(&["hmm"])], 0, PayloadMode::Minimized);
        assert_eq!(input.signals.tool_calls, 0);
        assert!(input.actions.is_empty());
    }

    #[test]
    fn redacted_thinking_still_counts_as_a_block() {
        // Claude Code writes the block with an encrypted signature and empty text,
        // so a chars-only signal would report a heavy-thinking session as zero.
        let input = extract_input(&[thinking_message(&["", ""])], 0, PayloadMode::Minimized);
        assert_eq!(input.signals.thinking_blocks, 2);
        assert_eq!(input.signals.thinking_chars, 0);
    }

    #[test]
    fn subagent_spawns_reach_jev_and_are_counted() {
        // They are `Subagent` items, not `ToolCall`s; skipping them reported
        // subagentCount 0 for a session full of subagent work.
        let input = extract_input(
            &[
                plain("user", "do it"),
                subagent_message("Explore", "find the parser"),
                subagent_message("Plan", "design the fix"),
            ],
            0,
            PayloadMode::Minimized,
        );
        assert_eq!(input.signals.subagent_count, 2);
        assert_eq!(input.actions.len(), 2);
        assert!(input.actions[0].summary.contains("Explore"));
    }

    #[test]
    fn task_bookkeeping_tools_are_not_counted_as_subagents() {
        // SendMessage/TaskCreate share the Task *category* but spawn nothing.
        let mut m = message("claude", "", Some("SendMessage"));
        m.items[0].tool_category = "Task".into();
        let input = extract_input(&[m], 0, PayloadMode::Minimized);
        assert_eq!(input.actions.len(), 1);
        assert_eq!(input.signals.subagent_count, 0);
    }

    #[test]
    fn the_final_assistant_message_always_reaches_jev() {
        // "Was the task completed?" cannot be judged when the ending is filtered out.
        let mut msgs = vec![plain("user", "start")];
        for i in 0..60 {
            msgs.push(plain("user", &format!("follow-up {i}")));
        }
        msgs.push(plain("claude", "All done: the suite passes."));
        let input = extract_input(&msgs, 0, PayloadMode::Minimized);
        assert!(input.selected_excerpts.len() <= MAX_EXCERPTS);
        assert!(
            input
                .selected_excerpts
                .iter()
                .any(|e| e.contains("All done: the suite passes.")),
            "the outcome must survive the cap; got {:?}",
            input.selected_excerpts
        );
    }

    #[test]
    fn excerpts_span_the_whole_session_not_just_the_opening() {
        let mut msgs = Vec::new();
        for i in 0..80 {
            msgs.push(plain("user", &format!("request {i}")));
        }
        msgs.push(plain("claude", "finished"));
        let input = extract_input(&msgs, 0, PayloadMode::Minimized);
        let joined = input.selected_excerpts.join("\n");
        assert!(joined.contains("request 0"), "opening must be sampled");
        assert!(
            joined.contains("request 79"),
            "the late half must be sampled, not cut off after the first 24"
        );
    }

    #[test]
    fn successful_assistant_messages_fill_spare_capacity() {
        // The old filter kept only requests and failures, so Jev saw every failure
        // and no successes.
        let msgs = vec![
            plain("user", "do it"),
            plain("claude", "step one worked"),
            plain("claude", "step two worked"),
        ];
        let input = extract_input(&msgs, 0, PayloadMode::Minimized);
        let joined = input.selected_excerpts.join("\n");
        assert!(joined.contains("step one worked"));
        assert!(joined.contains("step two worked"));
    }

    #[test]
    fn different_bash_commands_are_not_counted_as_repeated() {
        // The display summary is capped at 60 chars, so a long shared description
        // used to collapse two unrelated commands into one key and report them as
        // repeated work. The key now comes from the full tool input.
        let description = "Check the efficiency module for the repeated-work logic";
        let input = extract_input(
            &[
                bash_message(
                    description,
                    "grep -n redundant src-tauri/src/efficiency/jev.rs",
                ),
                bash_message(
                    description,
                    "grep -n threshold src-tauri/src/efficiency/score.rs",
                ),
            ],
            0,
            PayloadMode::Minimized,
        );
        assert_eq!(input.actions.len(), 2);
        assert_eq!(input.actions[0].repeated_similar_call_count, 0);
        assert_eq!(input.actions[1].repeated_similar_call_count, 0);
        assert_eq!(input.signals.repeated_tool_calls, 0);
    }

    #[test]
    fn identical_bash_commands_are_still_counted_as_repeated() {
        let input = extract_input(
            &[
                bash_message("Run the suite", "cargo test"),
                bash_message("Run the suite", "cargo test"),
            ],
            0,
            PayloadMode::Minimized,
        );
        assert_eq!(input.actions[0].repeated_similar_call_count, 1);
        assert_eq!(input.signals.repeated_tool_calls, 2);
    }

    #[test]
    fn a_truncated_summary_is_replaced_by_the_full_input() {
        let description = "Check the efficiency module for the repeated-work logic";
        let command = "grep -n redundant src-tauri/src/efficiency/jev.rs";
        let input = extract_input(
            &[bash_message(description, command)],
            0,
            PayloadMode::Minimized,
        );
        let summary = &input.actions[0].summary;
        assert!(
            summary.contains(command),
            "the command Jev needs to tell calls apart must survive; got {summary:?}"
        );
    }

    #[test]
    fn a_short_summary_stays_human_readable() {
        let input = extract_input(
            &[bash_message("List files", "ls -la")],
            0,
            PayloadMode::Minimized,
        );
        assert_eq!(input.actions[0].summary, "List files: ls -la");
    }

    #[test]
    fn extracts_facts_and_counts_repeated_calls() {
        let input = extract_input(
            &[
                message("user", "Fix it", None),
                message("claude", "", Some("Read")),
                message("claude", "", Some("Read")),
            ],
            42,
            PayloadMode::Minimized,
        );
        assert_eq!(input.task.first_user_message, "Fix it");
        assert_eq!(input.signals.tool_calls, 2);
        assert_eq!(input.signals.repeated_tool_calls, 2);
        assert_eq!(input.actions[0].repeated_similar_call_count, 1);
    }
}
