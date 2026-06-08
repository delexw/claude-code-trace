use super::chunk::*;
use serde::Serialize;

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
        if (item.item_type == DisplayItemType::ToolCall
            || item.item_type == DisplayItemType::Subagent)
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
        if item.item_type == DisplayItemType::ToolCall
            || item.item_type == DisplayItemType::Subagent
        {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn make_item(item_type: DisplayItemType) -> DisplayItem {
        DisplayItem {
            item_type,
            ..Default::default()
        }
    }

    #[test]
    fn returns_none_for_empty_items() {
        assert!(find_last_output(&[]).is_none());
    }

    #[test]
    fn returns_text_when_output_item_exists() {
        let items = vec![DisplayItem {
            item_type: DisplayItemType::Output,
            text: "Here is the answer".to_string(),
            ..Default::default()
        }];
        let result = find_last_output(&items).unwrap();
        assert_eq!(result.output_type, LastOutputType::Text);
        assert_eq!(result.text, "Here is the answer");
    }

    #[test]
    fn returns_tool_result_when_no_output_but_tool_call_with_result() {
        let items = vec![DisplayItem {
            item_type: DisplayItemType::ToolCall,
            tool_name: "Bash".to_string(),
            tool_result: "file1.txt\nfile2.txt".to_string(),
            tool_error: false,
            ..Default::default()
        }];
        let result = find_last_output(&items).unwrap();
        assert_eq!(result.output_type, LastOutputType::ToolResult);
        assert_eq!(result.tool_name, "Bash");
        assert_eq!(result.tool_result, "file1.txt\nfile2.txt");
        assert!(!result.is_error);
    }

    #[test]
    fn returns_tool_calls_when_no_output_and_no_results() {
        let items = vec![
            DisplayItem {
                item_type: DisplayItemType::ToolCall,
                tool_name: "Read".to_string(),
                tool_summary: "Reading file".to_string(),
                ..Default::default()
            },
            DisplayItem {
                item_type: DisplayItemType::ToolCall,
                tool_name: "Bash".to_string(),
                tool_summary: "Running ls".to_string(),
                ..Default::default()
            },
        ];
        let result = find_last_output(&items).unwrap();
        assert_eq!(result.output_type, LastOutputType::ToolCalls);
        assert_eq!(result.tool_calls.len(), 2);
        assert_eq!(result.tool_calls[0].name, "Read");
        assert_eq!(result.tool_calls[1].name, "Bash");
    }

    #[test]
    fn prefers_last_output_over_earlier_tool_result() {
        let items = vec![
            DisplayItem {
                item_type: DisplayItemType::ToolCall,
                tool_name: "Bash".to_string(),
                tool_result: "some result".to_string(),
                ..Default::default()
            },
            DisplayItem {
                item_type: DisplayItemType::Output,
                text: "Final answer".to_string(),
                ..Default::default()
            },
        ];
        let result = find_last_output(&items).unwrap();
        assert_eq!(result.output_type, LastOutputType::Text);
        assert_eq!(result.text, "Final answer");
    }

    #[test]
    fn caps_tool_calls_at_5() {
        let mut items = Vec::new();
        for i in 0..8 {
            items.push(DisplayItem {
                item_type: DisplayItemType::ToolCall,
                tool_name: format!("Tool{i}"),
                ..Default::default()
            });
        }
        let result = find_last_output(&items).unwrap();
        assert_eq!(result.output_type, LastOutputType::ToolCalls);
        assert_eq!(result.tool_calls.len(), 5);
    }

    #[test]
    fn returns_tool_result_for_subagent_with_result() {
        let items = vec![DisplayItem {
            item_type: DisplayItemType::Subagent,
            tool_name: "Task".to_string(),
            tool_result: "Task completed successfully".to_string(),
            ..Default::default()
        }];
        let result = find_last_output(&items).unwrap();
        assert_eq!(result.output_type, LastOutputType::ToolResult);
        assert_eq!(result.tool_name, "Task");
    }
}
