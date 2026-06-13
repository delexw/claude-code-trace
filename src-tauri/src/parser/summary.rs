use super::taxonomy::parse_mcp_tool_name;
use serde_json::Value;

/// Unicode horizontal ellipsis used for text truncation.
const ELLIPSIS: &str = "\u{2026}";

/// Generates a human-readable summary for a tool call.
/// Returns the tool name as fallback when input is nil or unparseable.
pub fn tool_summary(name: &str, input: &Option<Value>) -> String {
    let fields = match input {
        Some(Value::Object(m)) => m,
        _ => return name.to_string(),
    };

    match name {
        "Read" => summary_read(fields),
        "Write" => summary_write(fields),
        "Edit" => summary_edit(fields),
        "Bash" => summary_bash(fields),
        "Grep" => summary_grep(fields),
        "Glob" => summary_glob(fields),
        "Task" | "Agent" => summary_task(fields),
        "LSP" => summary_lsp(fields),
        "WebFetch" => summary_web_fetch(fields),
        "WebSearch" => summary_web_search(fields),
        "TodoWrite" => summary_todo_write(fields),
        "NotebookEdit" => summary_notebook_edit(fields),
        "TaskCreate" => summary_task_create(fields),
        "TaskUpdate" => summary_task_update(fields),
        "SendMessage" => summary_send_message(fields),
        "ToolSearch" => summary_tool_search(fields),
        "CronCreate" => summary_cron_create(fields),
        "CronDelete" => summary_cron_delete(fields),
        "CronList" => "List scheduled jobs".to_string(),
        "TaskList" => "List tasks".to_string(),
        "TaskGet" => summary_task_get(fields),
        "TaskStop" => summary_task_stop(fields),
        "TaskOutput" => summary_task_output(fields),
        "TeamCreate" => summary_team_create(fields),
        "TeamDelete" => summary_team_delete(fields),
        "AskUserQuestion" => summary_ask_user(fields),
        "Skill" => summary_skill(fields),
        "Monitor" => summary_monitor(fields),
        "EnterPlanMode" | "ExitPlanMode" | "EnterWorktree" | "ExitWorktree" => name.to_string(),
        "Cd" => summary_cd(fields),
        _ if name.starts_with("mcp__") => summary_mcp(name, fields),
        _ => summary_default(name, fields),
    }
}

fn summary_read(f: &serde_json::Map<String, Value>) -> String {
    let fp = get_str(f, "file_path");
    if fp.is_empty() {
        return "Read".to_string();
    }
    let short = short_path(fp, 2);

    let limit = get_num(f, "limit");
    if limit > 0 {
        let mut offset = get_num(f, "offset");
        if offset == 0 {
            offset = 1;
        }
        return format!("{} - lines {}-{}", short, offset, offset + limit - 1);
    }
    short
}

fn summary_write(f: &serde_json::Map<String, Value>) -> String {
    let fp = get_str(f, "file_path");
    if fp.is_empty() {
        return "Write".to_string();
    }
    let short = short_path(fp, 2);

    let content = get_str(f, "content");
    if !content.is_empty() {
        let lines = content.split('\n').count();
        return format!("{short} - {lines} lines");
    }
    short
}

fn summary_edit(f: &serde_json::Map<String, Value>) -> String {
    let fp = get_str(f, "file_path");
    if fp.is_empty() {
        return "Edit".to_string();
    }
    let short = short_path(fp, 2);

    let old_str = get_str(f, "old_string");
    let new_str = get_str(f, "new_string");
    if !old_str.is_empty() && !new_str.is_empty() {
        let old_lines = old_str.split('\n').count();
        let new_lines = new_str.split('\n').count();
        if old_lines == new_lines {
            let s = if old_lines > 1 { "s" } else { "" };
            return format!("{short} - {old_lines} line{s}");
        }
        return format!("{short} - {old_lines} -> {new_lines} lines");
    }
    short
}

fn summary_bash(f: &serde_json::Map<String, Value>) -> String {
    let desc = get_str(f, "description");
    let cmd = get_str(f, "command");

    if !desc.is_empty() && !cmd.is_empty() {
        return truncate(&format!("{desc}: {cmd}"), 60);
    }
    if !desc.is_empty() {
        return truncate(desc, 60);
    }
    if !cmd.is_empty() {
        return truncate(cmd, 60);
    }
    "Bash".to_string()
}

fn summary_grep(f: &serde_json::Map<String, Value>) -> String {
    let pattern = get_str(f, "pattern");
    if pattern.is_empty() {
        return "Grep".to_string();
    }
    let pat_str = format!("\"{}\"", truncate(pattern, 30));

    let glob = get_str(f, "glob");
    if !glob.is_empty() {
        return format!("{pat_str} in {glob}");
    }
    let p = get_str(f, "path");
    if !p.is_empty() {
        return format!("{} in {}", pat_str, file_base(p));
    }
    pat_str
}

fn summary_glob(f: &serde_json::Map<String, Value>) -> String {
    let pattern = get_str(f, "pattern");
    if pattern.is_empty() {
        return "Glob".to_string();
    }
    let pat_str = format!("\"{}\"", truncate(pattern, 30));

    let p = get_str(f, "path");
    if !p.is_empty() {
        return format!("{} in {}", pat_str, file_base(p));
    }
    pat_str
}

fn summary_task(f: &serde_json::Map<String, Value>) -> String {
    let mut desc = get_str(f, "description").to_string();
    if desc.is_empty() {
        desc = get_str(f, "prompt").to_string();
    }
    let sub_type = get_str(f, "subagentType");

    let type_prefix = if !sub_type.is_empty() {
        format!("{sub_type} - ")
    } else {
        String::new()
    };
    if !desc.is_empty() {
        return format!("{}{}", type_prefix, truncate(&desc, 40));
    }
    if !sub_type.is_empty() {
        return sub_type.to_string();
    }
    "Task".to_string()
}

fn summary_lsp(f: &serde_json::Map<String, Value>) -> String {
    let op = get_str(f, "operation");
    if op.is_empty() {
        return "LSP".to_string();
    }
    let fp = get_str(f, "filePath");
    if !fp.is_empty() {
        return format!("{} - {}", op, file_base(fp));
    }
    op.to_string()
}

fn summary_web_fetch(f: &serde_json::Map<String, Value>) -> String {
    let raw_url = get_str(f, "url");
    if raw_url.is_empty() {
        return "WebFetch".to_string();
    }
    // Simple URL parse: extract hostname+path
    if let Some(after) = raw_url.find("://").map(|i| &raw_url[i + 3..]) {
        let (host_port, path) = match after.find('/') {
            Some(i) => (&after[..i], &after[i..]),
            None => (after, ""),
        };
        let hostname = match host_port.find(':') {
            Some(i) => &host_port[..i],
            None => host_port,
        };
        if !hostname.is_empty() {
            return truncate(&format!("{hostname}{path}"), 50);
        }
    }
    truncate(raw_url, 50)
}

fn summary_web_search(f: &serde_json::Map<String, Value>) -> String {
    let q = get_str(f, "query");
    if q.is_empty() {
        return "WebSearch".to_string();
    }
    format!("\"{}\"", truncate(q, 40))
}

fn summary_todo_write(f: &serde_json::Map<String, Value>) -> String {
    match f.get("todos") {
        Some(Value::Array(arr)) => {
            let s = if arr.len() == 1 { "" } else { "s" };
            format!("{} item{}", arr.len(), s)
        }
        _ => "TodoWrite".to_string(),
    }
}

fn summary_notebook_edit(f: &serde_json::Map<String, Value>) -> String {
    let nb_path = get_str(f, "notebook_path");
    if nb_path.is_empty() {
        return "NotebookEdit".to_string();
    }
    let base = file_base(nb_path);
    let mode = get_str(f, "edit_mode");
    if !mode.is_empty() {
        return format!("{mode} - {base}");
    }
    base
}

fn summary_task_create(f: &serde_json::Map<String, Value>) -> String {
    let subj = get_str(f, "subject");
    if !subj.is_empty() {
        return truncate(subj, 50);
    }
    "Create task".to_string()
}

fn summary_task_update(f: &serde_json::Map<String, Value>) -> String {
    let mut parts = Vec::new();
    let id = get_str(f, "taskId");
    if !id.is_empty() {
        parts.push(format!("#{id}"));
    }
    let status = get_str(f, "status");
    if !status.is_empty() {
        parts.push(status.to_string());
    }
    let owner = get_str(f, "owner");
    if !owner.is_empty() {
        parts.push(format!("-> {owner}"));
    }
    if !parts.is_empty() {
        return parts.join(" ");
    }
    "Update task".to_string()
}

fn summary_send_message(f: &serde_json::Map<String, Value>) -> String {
    let msg_type = get_str(f, "type");
    let recipient = get_str(f, "recipient");
    let summary = get_str(f, "summary");

    if msg_type == "shutdown_request" && !recipient.is_empty() {
        return format!("Shutdown {recipient}");
    }
    if msg_type == "shutdown_response" {
        return "Shutdown response".to_string();
    }
    if msg_type == "broadcast" {
        return format!("Broadcast: {}", truncate(summary, 30));
    }
    if !recipient.is_empty() {
        return format!("To {}: {}", recipient, truncate(summary, 30));
    }
    "Send message".to_string()
}

fn summary_tool_search(f: &serde_json::Map<String, Value>) -> String {
    let q = get_str(f, "query");
    if q.is_empty() {
        return "ToolSearch".to_string();
    }
    truncate(q, 50)
}

fn summary_cron_create(f: &serde_json::Map<String, Value>) -> String {
    let prompt = get_str(f, "prompt");
    let cron = get_str(f, "cron");
    if !prompt.is_empty() && !cron.is_empty() {
        return format!("{} ({})", truncate(prompt, 40), cron);
    }
    if !prompt.is_empty() {
        return truncate(prompt, 50);
    }
    if !cron.is_empty() {
        return cron.to_string();
    }
    "Create cron job".to_string()
}

fn summary_cron_delete(f: &serde_json::Map<String, Value>) -> String {
    let id = get_str(f, "id");
    if !id.is_empty() {
        return format!("Delete job {id}");
    }
    "Delete cron job".to_string()
}

fn summary_task_get(f: &serde_json::Map<String, Value>) -> String {
    let id = get_str(f, "taskId");
    if !id.is_empty() {
        return format!("Get task #{id}");
    }
    "Get task".to_string()
}

fn summary_task_stop(f: &serde_json::Map<String, Value>) -> String {
    let id = get_str(f, "taskId");
    if !id.is_empty() {
        return format!("Stop task #{id}");
    }
    "Stop task".to_string()
}

fn summary_task_output(f: &serde_json::Map<String, Value>) -> String {
    let id = get_str(f, "taskId");
    if !id.is_empty() {
        return format!("Output of #{id}");
    }
    "Task output".to_string()
}

fn summary_team_create(f: &serde_json::Map<String, Value>) -> String {
    let name = get_str(f, "name");
    if !name.is_empty() {
        return truncate(name, 50);
    }
    "Create team".to_string()
}

fn summary_team_delete(f: &serde_json::Map<String, Value>) -> String {
    let name = get_str(f, "name");
    if !name.is_empty() {
        return format!("Delete {name}");
    }
    "Delete team".to_string()
}

fn summary_ask_user(f: &serde_json::Map<String, Value>) -> String {
    if let Some(Value::Array(questions)) = f.get("questions") {
        if let Some(Value::Object(q)) = questions.first() {
            let header = q.get("header").and_then(|v| v.as_str()).unwrap_or("");
            let question = q.get("question").and_then(|v| v.as_str()).unwrap_or("");
            if !header.is_empty() {
                return truncate(header, 50);
            }
            if !question.is_empty() {
                return truncate(question, 50);
            }
        }
    }
    "Ask user".to_string()
}

fn summary_mcp(name: &str, f: &serde_json::Map<String, Value>) -> String {
    let tool_part = match parse_mcp_tool_name(name) {
        Some((_, tool)) => tool.replace('_', " "),
        None => return name.to_string(),
    };

    // Try to extract a meaningful detail from common MCP parameter names.
    let detail_keys = [
        "url",
        "query",
        "selector",
        "expression",
        "fileKey",
        "nodeId",
        "issue_key",
        "page_url",
        "description",
        "prompt",
        "name",
        "path",
        "file",
        "command",
    ];

    for key in &detail_keys {
        let v = get_str(f, key);
        if !v.is_empty() {
            return format!("{} - {}", tool_part, truncate(v, 40));
        }
    }

    // Fall back to first string value.
    if !f.is_empty() {
        let mut keys: Vec<&String> = f.keys().collect();
        keys.sort();
        for k in keys {
            if let Some(Value::String(s)) = f.get(k.as_str()) {
                if !s.is_empty() {
                    return format!("{} - {}", tool_part, truncate(s, 40));
                }
            }
        }
    }

    tool_part
}

/// Monitor tool summary: prefer `label`, then `command`, then fall back to "Monitor".
fn summary_monitor(f: &serde_json::Map<String, Value>) -> String {
    let label = get_str(f, "label");
    if !label.is_empty() {
        return label.to_string();
    }
    let command = get_str(f, "command");
    if !command.is_empty() {
        return truncate(command, 60);
    }
    "Monitor".to_string()
}

fn summary_skill(f: &serde_json::Map<String, Value>) -> String {
    let skill = get_str(f, "skill");
    if skill.is_empty() {
        return "Skill".to_string();
    }
    skill.to_string()
}

fn summary_cd(f: &serde_json::Map<String, Value>) -> String {
    let path = get_str(f, "path");
    if path.is_empty() {
        return "Cd".to_string();
    }
    short_path(path, 2)
}

fn summary_default(name: &str, f: &serde_json::Map<String, Value>) -> String {
    if f.is_empty() {
        return name.to_string();
    }

    // Try common parameter names in order.
    for key in &["name", "path", "file", "query", "command"] {
        let v = get_str(f, key);
        if !v.is_empty() {
            return truncate(v, 50);
        }
    }

    // Fall back to first string value (sorted keys for deterministic output).
    let mut keys: Vec<&String> = f.keys().collect();
    keys.sort();
    for k in keys {
        if let Some(Value::String(s)) = f.get(k.as_str()) {
            if !s.is_empty() {
                return truncate(s, 40);
            }
        }
    }
    name.to_string()
}

// --- Helpers ---

/// Returns the last n segments of a file path.
pub fn short_path(full_path: &str, n: usize) -> String {
    let normalized = full_path.replace('\\', "/");
    let segments: Vec<&str> = normalized.split('/').filter(|s| !s.is_empty()).collect();
    if segments.len() <= n {
        return segments.join("/");
    }
    segments[segments.len() - n..].join("/")
}

/// Extracts the file name from a path.
fn file_base(p: &str) -> String {
    std::path::Path::new(p)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(p)
        .to_string()
}

/// Extracts a string field from a JSON map. Returns "" if missing or wrong type.
fn get_str<'a>(fields: &'a serde_json::Map<String, Value>, key: &str) -> &'a str {
    fields.get(key).and_then(|v| v.as_str()).unwrap_or("")
}

/// Also for HashMap<String, Value> usage in other modules.
pub fn get_string_from_map(fields: &std::collections::HashMap<String, Value>, key: &str) -> String {
    match fields.get(key) {
        Some(Value::String(s)) => s.clone(),
        _ => String::new(),
    }
}

/// Extracts a numeric field from a JSON map. Returns 0 if missing or wrong type.
fn get_num(fields: &serde_json::Map<String, Value>, key: &str) -> i64 {
    fields.get(key).and_then(|v| v.as_f64()).unwrap_or(0.0) as i64
}

/// Truncate shortens a string to max_len runes, appending an ellipsis if truncated.
/// Collapses newlines to spaces since summaries are single-line display strings.
pub fn truncate(s: &str, max_len: usize) -> String {
    let s = s.replace('\n', " ");
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max_len {
        return s;
    }
    let truncated: String = chars[..max_len - 1].iter().collect();
    format!("{truncated}{ELLIPSIS}")
}

/// TruncateWord shortens a string to max_len runes, breaking at the nearest
/// preceding word boundary (space).
pub fn truncate_word(s: &str, max_len: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max_len {
        return s.to_string();
    }
    let cutoff = max_len - 1;
    let search_start = cutoff.saturating_sub(20);
    for i in (search_start..=cutoff).rev() {
        if chars[i] == ' ' {
            let truncated: String = chars[..i].iter().collect();
            return format!("{truncated}{ELLIPSIS}");
        }
    }
    let truncated: String = chars[..cutoff].iter().collect();
    format!("{truncated}{ELLIPSIS}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ---- tool_summary tests ----

    #[test]
    fn summary_none_input() {
        assert_eq!(tool_summary("Read", &None), "Read");
    }

    #[test]
    fn summary_non_object_input() {
        assert_eq!(tool_summary("Read", &Some(json!("string"))), "Read");
    }

    #[test]
    fn summary_read_with_file_path() {
        let input = Some(json!({"file_path": "/home/user/project/src/main.rs"}));
        assert_eq!(tool_summary("Read", &input), "src/main.rs");
    }

    #[test]
    fn summary_read_with_limit_offset() {
        let input = Some(json!({"file_path": "/a/b/c.rs", "limit": 10, "offset": 5}));
        assert_eq!(tool_summary("Read", &input), "b/c.rs - lines 5-14");
    }

    #[test]
    fn summary_read_with_limit_no_offset() {
        let input = Some(json!({"file_path": "/a/b/c.rs", "limit": 20}));
        assert_eq!(tool_summary("Read", &input), "b/c.rs - lines 1-20");
    }

    #[test]
    fn summary_read_no_file_path() {
        let input = Some(json!({}));
        assert_eq!(tool_summary("Read", &input), "Read");
    }

    #[test]
    fn summary_write_with_content() {
        let input = Some(json!({"file_path": "/x/y/z.txt", "content": "line1\nline2\nline3"}));
        assert_eq!(tool_summary("Write", &input), "y/z.txt - 3 lines");
    }

    #[test]
    fn summary_write_no_content() {
        let input = Some(json!({"file_path": "/x/y/z.txt"}));
        assert_eq!(tool_summary("Write", &input), "y/z.txt");
    }

    #[test]
    fn summary_write_no_file_path() {
        let input = Some(json!({}));
        assert_eq!(tool_summary("Write", &input), "Write");
    }

    #[test]
    fn summary_edit_same_lines() {
        let input = Some(json!({
            "file_path": "/a/b/c.rs",
            "old_string": "old line",
            "new_string": "new line"
        }));
        assert_eq!(tool_summary("Edit", &input), "b/c.rs - 1 line");
    }

    #[test]
    fn summary_edit_multiple_same_lines() {
        let input = Some(json!({
            "file_path": "/a/b/c.rs",
            "old_string": "line1\nline2\nline3",
            "new_string": "new1\nnew2\nnew3"
        }));
        assert_eq!(tool_summary("Edit", &input), "b/c.rs - 3 lines");
    }

    #[test]
    fn summary_edit_different_lines() {
        let input = Some(json!({
            "file_path": "/a/b/c.rs",
            "old_string": "one line",
            "new_string": "line1\nline2\nline3"
        }));
        assert_eq!(tool_summary("Edit", &input), "b/c.rs - 1 -> 3 lines");
    }

    #[test]
    fn summary_edit_no_file_path() {
        let input = Some(json!({"old_string": "a", "new_string": "b"}));
        assert_eq!(tool_summary("Edit", &input), "Edit");
    }

    #[test]
    fn summary_bash_desc_and_cmd() {
        let input = Some(json!({"description": "List files", "command": "ls -la"}));
        assert_eq!(tool_summary("Bash", &input), "List files: ls -la");
    }

    #[test]
    fn summary_bash_desc_only() {
        let input = Some(json!({"description": "Run tests"}));
        assert_eq!(tool_summary("Bash", &input), "Run tests");
    }

    #[test]
    fn summary_bash_cmd_only() {
        let input = Some(json!({"command": "cargo build"}));
        assert_eq!(tool_summary("Bash", &input), "cargo build");
    }

    #[test]
    fn summary_bash_empty() {
        let input = Some(json!({}));
        assert_eq!(tool_summary("Bash", &input), "Bash");
    }

    #[test]
    fn summary_grep_pattern_only() {
        let input = Some(json!({"pattern": "TODO"}));
        assert_eq!(tool_summary("Grep", &input), "\"TODO\"");
    }

    #[test]
    fn summary_grep_with_glob() {
        let input = Some(json!({"pattern": "TODO", "glob": "*.rs"}));
        assert_eq!(tool_summary("Grep", &input), "\"TODO\" in *.rs");
    }

    #[test]
    fn summary_grep_with_path() {
        let input = Some(json!({"pattern": "TODO", "path": "/home/user/src/main.rs"}));
        assert_eq!(tool_summary("Grep", &input), "\"TODO\" in main.rs");
    }

    #[test]
    fn summary_grep_empty() {
        let input = Some(json!({}));
        assert_eq!(tool_summary("Grep", &input), "Grep");
    }

    #[test]
    fn summary_glob_pattern_only() {
        let input = Some(json!({"pattern": "**/*.rs"}));
        assert_eq!(tool_summary("Glob", &input), "\"**/*.rs\"");
    }

    #[test]
    fn summary_glob_with_path() {
        let input = Some(json!({"pattern": "*.rs", "path": "/home/user/src"}));
        assert_eq!(tool_summary("Glob", &input), "\"*.rs\" in src");
    }

    #[test]
    fn summary_glob_empty() {
        let input = Some(json!({}));
        assert_eq!(tool_summary("Glob", &input), "Glob");
    }

    #[test]
    fn summary_task_with_description() {
        let input = Some(json!({"description": "Analyze the code"}));
        assert_eq!(tool_summary("Task", &input), "Analyze the code");
    }

    #[test]
    fn summary_task_with_prompt() {
        let input = Some(json!({"prompt": "Do something"}));
        assert_eq!(tool_summary("Agent", &input), "Do something");
    }

    #[test]
    fn summary_task_with_subagent_type() {
        let input = Some(json!({"subagentType": "researcher", "description": "Find info"}));
        assert_eq!(tool_summary("Task", &input), "researcher - Find info");
    }

    #[test]
    fn summary_task_subagent_type_only() {
        let input = Some(json!({"subagentType": "researcher"}));
        assert_eq!(tool_summary("Task", &input), "researcher");
    }

    #[test]
    fn summary_task_empty() {
        let input = Some(json!({}));
        assert_eq!(tool_summary("Task", &input), "Task");
    }

    #[test]
    fn summary_web_fetch_with_url() {
        let input = Some(json!({"url": "https://example.com/path/to/page"}));
        assert_eq!(tool_summary("WebFetch", &input), "example.com/path/to/page");
    }

    #[test]
    fn summary_web_fetch_with_port() {
        let input = Some(json!({"url": "http://localhost:3000/api"}));
        assert_eq!(tool_summary("WebFetch", &input), "localhost/api");
    }

    #[test]
    fn summary_web_fetch_no_path() {
        let input = Some(json!({"url": "https://example.com"}));
        assert_eq!(tool_summary("WebFetch", &input), "example.com");
    }

    #[test]
    fn summary_web_fetch_empty() {
        let input = Some(json!({}));
        assert_eq!(tool_summary("WebFetch", &input), "WebFetch");
    }

    #[test]
    fn summary_web_search_with_query() {
        let input = Some(json!({"query": "rust testing"}));
        assert_eq!(tool_summary("WebSearch", &input), "\"rust testing\"");
    }

    #[test]
    fn summary_web_search_empty() {
        let input = Some(json!({}));
        assert_eq!(tool_summary("WebSearch", &input), "WebSearch");
    }

    // ---- MCP tool summary tests ----

    #[test]
    fn summary_mcp_figma_with_file_key() {
        let input = Some(json!({"fileKey": "abc123", "nodeId": "1:2"}));
        assert_eq!(
            tool_summary("mcp__figma__get_design_context", &input),
            "get design context - abc123"
        );
    }

    #[test]
    fn summary_mcp_chrome_devtools_with_url() {
        let input = Some(json!({"url": "https://example.com/page"}));
        assert_eq!(
            tool_summary("mcp__chrome-devtools__navigate_page", &input),
            "navigate page - https://example.com/page"
        );
    }

    #[test]
    fn summary_mcp_with_selector() {
        let input = Some(json!({"selector": "#login-btn"}));
        assert_eq!(
            tool_summary("mcp__chrome-devtools__click", &input),
            "click - #login-btn"
        );
    }

    #[test]
    fn summary_mcp_jira_with_issue_key() {
        let input = Some(json!({"issue_key": "EC-10457"}));
        assert_eq!(
            tool_summary("mcp__atlassian__jira_get_issue", &input),
            "jira get issue - EC-10457"
        );
    }

    #[test]
    fn summary_mcp_empty_input() {
        let input = Some(json!({}));
        assert_eq!(
            tool_summary("mcp__figma__get_screenshot", &input),
            "get screenshot"
        );
    }

    #[test]
    fn summary_mcp_no_input() {
        assert_eq!(
            tool_summary("mcp__figma__get_screenshot", &None),
            "mcp__figma__get_screenshot"
        );
    }

    #[test]
    fn summary_mcp_falls_back_to_first_string() {
        let input = Some(json!({"some_custom_param": "value123"}));
        assert_eq!(
            tool_summary("mcp__custom__do_thing", &input),
            "do thing - value123"
        );
    }

    #[test]
    fn summary_default_with_common_keys() {
        let input = Some(json!({"name": "my-tool", "extra": "data"}));
        assert_eq!(tool_summary("SomeUnknown", &input), "my-tool");
    }

    #[test]
    fn summary_default_with_path_key() {
        let input = Some(json!({"path": "/some/path"}));
        assert_eq!(tool_summary("SomeUnknown", &input), "/some/path");
    }

    #[test]
    fn summary_default_empty_fields() {
        let input = Some(json!({}));
        assert_eq!(tool_summary("SomeUnknown", &input), "SomeUnknown");
    }

    #[test]
    fn summary_default_falls_back_to_first_string() {
        let input = Some(json!({"zzz_field": "hello"}));
        assert_eq!(tool_summary("SomeUnknown", &input), "hello");
    }

    // ---- short_path tests ----

    #[test]
    fn short_path_fewer_segments() {
        assert_eq!(short_path("file.rs", 2), "file.rs");
    }

    #[test]
    fn short_path_exact_segments() {
        assert_eq!(short_path("src/main.rs", 2), "src/main.rs");
    }

    #[test]
    fn short_path_more_segments() {
        assert_eq!(
            short_path("/home/user/project/src/main.rs", 2),
            "src/main.rs"
        );
    }

    #[test]
    fn short_path_single_segment() {
        assert_eq!(short_path("/home/user/project/src/main.rs", 1), "main.rs");
    }

    #[test]
    fn short_path_backslashes() {
        assert_eq!(
            short_path("C:\\Users\\project\\src\\main.rs", 2),
            "src/main.rs"
        );
    }

    // ---- truncate tests ----

    #[test]
    fn truncate_within_limit() {
        assert_eq!(truncate("short", 10), "short");
    }

    #[test]
    fn truncate_over_limit() {
        let result = truncate("this is a long string", 10);
        assert_eq!(result.chars().count(), 10);
        assert!(result.ends_with('\u{2026}'));
    }

    #[test]
    fn truncate_collapses_newlines() {
        assert_eq!(truncate("line1\nline2", 20), "line1 line2");
    }

    // ---- truncate_word tests ----

    // --- Monitor tool summary tests (#39) ---

    #[test]
    fn summary_monitor_with_label() {
        let input = json!({"label": "background build"});
        assert_eq!(tool_summary("Monitor", &Some(input)), "background build");
    }

    #[test]
    fn summary_monitor_with_command() {
        let input = json!({"command": "npm run watch"});
        assert_eq!(tool_summary("Monitor", &Some(input)), "npm run watch");
    }

    #[test]
    fn summary_monitor_label_takes_priority_over_command() {
        let input = json!({"label": "my label", "command": "some command"});
        assert_eq!(tool_summary("Monitor", &Some(input)), "my label");
    }

    #[test]
    fn summary_monitor_no_fields_returns_monitor() {
        let input = json!({});
        assert_eq!(tool_summary("Monitor", &Some(input)), "Monitor");
    }

    #[test]
    fn truncate_word_within_limit() {
        assert_eq!(truncate_word("short text", 20), "short text");
    }

    #[test]
    fn truncate_word_breaks_at_space() {
        let result = truncate_word("hello world this is a test string for truncation", 25);
        assert!(result.ends_with('\u{2026}'));
        // Should break at a word boundary
        assert!(!result.contains("trun"));
    }

    #[test]
    fn truncate_word_no_space_falls_back() {
        let result = truncate_word("abcdefghijklmnopqrstuvwxyz", 10);
        assert!(result.ends_with('\u{2026}'));
        assert_eq!(result.chars().count(), 10);
    }

    // --- Cd tool summary tests (#136) ---

    #[test]
    fn summary_cd_with_path() {
        let input = json!({"path": "/home/user/project-b"});
        let result = tool_summary("Cd", &Some(input));
        assert!(
            result.contains("project-b"),
            "Cd summary must include the target directory; got {:?}",
            result
        );
    }

    #[test]
    fn summary_cd_no_path_falls_back_to_cd() {
        let input = json!({});
        assert_eq!(tool_summary("Cd", &Some(input)), "Cd");
    }

    #[test]
    fn summary_cd_nil_input_falls_back_to_cd() {
        assert_eq!(tool_summary("Cd", &None), "Cd");
    }
}
