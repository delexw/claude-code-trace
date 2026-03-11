use serde::Serialize;
use super::chunk::*;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum LastOutputType {
    Text,
    ToolResult,
    ToolCalls,
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolCallSummary {
    pub name: String,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct LastOutput {
    pub output_type: LastOutputType,
    pub text: String,
    pub tool_name: String,
    pub tool_result: String,
    pub is_error: bool,
    pub tool_calls: Vec<ToolCallSummary>,
}

/// Find the last meaningful output from display items.
pub fn find_last_output(items: &[DisplayItem]) -> Option<LastOutput> {
    // First pass: last output text
    for item in items.iter().rev() {
        if item.item_type == DisplayItemType::Output && !item.text.is_empty() {
            return Some(LastOutput {
                output_type: LastOutputType::Text,
                text: item.text.clone(),
                tool_name: String::new(),
                tool_result: String::new(),
                is_error: false,
                tool_calls: Vec::new(),
            });
        }
    }

    // Second pass: last tool result
    for item in items.iter().rev() {
        if (item.item_type == DisplayItemType::ToolCall || item.item_type == DisplayItemType::Subagent)
            && !item.tool_result.is_empty()
        {
            return Some(LastOutput {
                output_type: LastOutputType::ToolResult,
                text: String::new(),
                tool_name: item.tool_name.clone(),
                tool_result: item.tool_result.clone(),
                is_error: item.tool_error,
                tool_calls: Vec::new(),
            });
        }
    }

    // Third pass: tool call names as fallback
    let mut calls = Vec::new();
    for item in items {
        if item.item_type == DisplayItemType::ToolCall || item.item_type == DisplayItemType::Subagent {
            calls.push(ToolCallSummary {
                name: item.tool_name.clone(),
                summary: item.tool_summary.clone(),
            });
            if calls.len() >= 5 {
                break;
            }
        }
    }
    if !calls.is_empty() {
        return Some(LastOutput {
            output_type: LastOutputType::ToolCalls,
            text: String::new(),
            tool_name: String::new(),
            tool_result: String::new(),
            is_error: false,
            tool_calls: calls,
        });
    }

    None
}
