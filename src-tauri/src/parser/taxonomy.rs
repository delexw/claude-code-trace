use serde::Serialize;

/// ToolCategory classifies tool calls into broad functional groups.
/// Used by the GUI to assign per-category icons and colors.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum ToolCategory {
    Read,
    Edit,
    Write,
    Bash,
    Grep,
    Glob,
    Task,
    Tool,
    Web,
    Other,
}

/// CategorizeToolName maps a raw tool name to a ToolCategory.
pub fn categorize_tool_name(name: &str) -> ToolCategory {
    match name {
        // Claude Code tools
        "Read" => ToolCategory::Read,
        "Edit" => ToolCategory::Edit,
        "Write" | "NotebookEdit" => ToolCategory::Write,
        "Bash" => ToolCategory::Bash,
        "Grep" => ToolCategory::Grep,
        "Glob" => ToolCategory::Glob,
        "Task" | "Agent" => ToolCategory::Task,
        "Skill" => ToolCategory::Tool,
        "WebFetch" | "WebSearch" => ToolCategory::Web,

        // Codex tools
        "shell_command" | "exec_command" | "write_stdin" | "shell" => ToolCategory::Bash,
        "apply_patch" => ToolCategory::Edit,

        // Gemini tools
        "read_file" => ToolCategory::Read,
        "write_file" | "edit_file" => ToolCategory::Write,
        "run_command" | "execute_command" => ToolCategory::Bash,
        "search_files" | "grep" => ToolCategory::Grep,

        // OpenCode tools (lowercase variants)
        "read" => ToolCategory::Read,
        "edit" => ToolCategory::Edit,
        "write" => ToolCategory::Write,
        "bash" => ToolCategory::Bash,
        "glob" => ToolCategory::Glob,
        "task" => ToolCategory::Task,

        // Copilot tools
        "view" => ToolCategory::Read,
        "report_intent" => ToolCategory::Tool,

        // Cursor tools
        "Shell" => ToolCategory::Bash,
        "StrReplace" => ToolCategory::Edit,
        "LS" => ToolCategory::Read,

        _ => ToolCategory::Other,
    }
}
