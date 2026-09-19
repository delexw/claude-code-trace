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

fn normalized_action_key(tool: &str, summary: &str) -> String {
    format!(
        "{}:{}",
        tool.to_lowercase(),
        summary.split_whitespace().collect::<Vec<_>>().join(" ")
    )
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

    for (message_index, message) in messages.iter().enumerate() {
        if message.context_tokens > 0 {
            min_context = min_context.min(message.context_tokens);
            max_context = max_context.max(message.context_tokens);
        }
        for item in &message.items {
            if item.item_type != "ToolCall" {
                continue;
            }
            let summary_source = if item.tool_summary.trim().is_empty() {
                &item.tool_input
            } else {
                &item.tool_summary
            };
            let summary = truncate(summary_source, MAX_SUMMARY_CHARS);
            let key = normalized_action_key(&item.tool_name, &summary);
            *counts.entry(key.clone()).or_default() += 1;
            if item.tool_error {
                failed_tool_calls += 1;
            }
            if item.tool_category == "Task" || item.tool_name == "Agent" {
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

    let selected_excerpts = messages
        .iter()
        .filter(|message| {
            mode == PayloadMode::FullTranscript
                || message.role == "user"
                || message.is_error
                || message.items.iter().any(|item| item.tool_error)
        })
        .take(if mode == PayloadMode::FullTranscript {
            usize::MAX
        } else {
            MAX_EXCERPTS
        })
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
