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
        return format!("{} - {} lines", short, lines);
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
            return format!("{} - {} line{}", short, old_lines, s);
        }
        return format!("{} - {} -> {} lines", short, old_lines, new_lines);
    }
    short
}

fn summary_bash(f: &serde_json::Map<String, Value>) -> String {
    let desc = get_str(f, "description");
    let cmd = get_str(f, "command");

    if !desc.is_empty() && !cmd.is_empty() {
        return truncate(&format!("{}: {}", desc, cmd), 60);
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
        return format!("{} in {}", pat_str, glob);
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
        format!("{} - ", sub_type)
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
            return truncate(&format!("{}{}", hostname, path), 50);
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
        return format!("{} - {}", mode, base);
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
        parts.push(format!("#{}", id));
    }
    let status = get_str(f, "status");
    if !status.is_empty() {
        parts.push(status.to_string());
    }
    let owner = get_str(f, "owner");
    if !owner.is_empty() {
        parts.push(format!("-> {}", owner));
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
        return format!("Shutdown {}", recipient);
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
    fields
        .get(key)
        .and_then(|v| v.as_str())
        .unwrap_or("")
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
    fields
        .get(key)
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0) as i64
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
    format!("{}{}", truncated, ELLIPSIS)
}

/// TruncateWord shortens a string to max_len runes, breaking at the nearest
/// preceding word boundary (space).
pub fn truncate_word(s: &str, max_len: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max_len {
        return s.to_string();
    }
    let cutoff = max_len - 1;
    let search_start = if cutoff > 20 { cutoff - 20 } else { 0 };
    for i in (search_start..=cutoff).rev() {
        if chars[i] == ' ' {
            let truncated: String = chars[..i].iter().collect();
            return format!("{}{}", truncated, ELLIPSIS);
        }
    }
    let truncated: String = chars[..cutoff].iter().collect();
    format!("{}{}", truncated, ELLIPSIS)
}
