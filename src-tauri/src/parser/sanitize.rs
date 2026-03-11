use regex::Regex;
use serde_json::Value;

use super::patterns::*;

/// Extract text content from message.content (string or array of text blocks).
pub fn extract_text(content: &Option<Value>) -> String {
    match content {
        None => String::new(),
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(blocks)) => {
            let mut parts = Vec::new();
            for block in blocks {
                if let Some(block_type) = block.get("type").and_then(|v| v.as_str()) {
                    if block_type == "text" {
                        if let Some(text) = block.get("text").and_then(|v| v.as_str()) {
                            if !text.is_empty() {
                                parts.push(text.to_string());
                            }
                        }
                    }
                }
            }
            parts.join("\n")
        }
        _ => String::new(),
    }
}

/// SanitizeContent removes noise XML tags and converts command tags into
/// a human-readable slash command format for display.
pub fn sanitize_content(s: &str) -> String {
    // Command output messages: extract the inner content.
    if is_command_output(s) {
        let out = extract_command_output(s);
        if !out.is_empty() {
            return out;
        }
    }

    // Command messages: convert to "/name args" form.
    if s.starts_with("<command-name>") || s.starts_with("<command-message>") {
        if let Some(display) = extract_command_display(s) {
            if !display.is_empty() {
                return display;
            }
        }
    }

    // Strip noise tags.
    let mut result = s.to_string();
    for pat in NOISE_TAG_PATTERNS.iter() {
        result = pat.replace_all(&result, "").to_string();
    }

    // Strip remaining command tags.
    for pat in COMMAND_TAG_PATTERNS.iter() {
        result = pat.replace_all(&result, "").to_string();
    }

    // Strip bash-input tags but keep inner content (the command text).
    result = RE_BASH_INPUT.replace_all(&result, "$1").to_string();

    result.trim().to_string()
}

/// Converts <command-name>/foo</command-name><command-args>bar</command-args>
/// into "/foo bar".
fn extract_command_display(s: &str) -> Option<String> {
    let m = RE_COMMAND_NAME.captures(s)?;
    let name = format!("/{}", m.get(1)?.as_str().trim());

    if let Some(am) = RE_COMMAND_ARGS.captures(s) {
        let args = am.get(1).map(|m| m.as_str().trim()).unwrap_or("");
        if !args.is_empty() {
            return Some(format!("{} {}", name, args));
        }
    }
    Some(name)
}

/// Returns true when content starts with a local-command output tag.
/// (Only local-command tags, not bash/task tags.)
pub fn is_command_output(s: &str) -> bool {
    s.starts_with(LOCAL_COMMAND_STDOUT_TAG) || s.starts_with(LOCAL_COMMAND_STDERR_TAG)
}

/// Returns the inner text from <local-command-stdout> or <local-command-stderr>.
/// Returns empty string if neither tag is found.
pub fn extract_command_output(s: &str) -> String {
    if let Some(caps) = RE_STDOUT.captures(s) {
        if let Some(m) = caps.get(1) {
            return m.as_str().trim().to_string();
        }
    }
    if let Some(caps) = RE_STDERR.captures(s) {
        if let Some(m) = caps.get(1) {
            return m.as_str().trim().to_string();
        }
    }
    String::new()
}

/// Returns the inner text from <bash-stdout> or <bash-stderr> wrapper tags.
/// Tries stdout first, falls back to stderr.
pub fn extract_bash_output(s: &str) -> String {
    if let Some(caps) = RE_BASH_STDOUT.captures(s) {
        if let Some(m) = caps.get(1) {
            return m.as_str().trim().to_string();
        }
    }
    if let Some(caps) = RE_BASH_STDERR.captures(s) {
        if let Some(m) = caps.get(1) {
            return m.as_str().trim().to_string();
        }
    }
    String::new()
}

/// Pulls the human-readable summary from a <task-notification> XML wrapper.
pub fn extract_task_notification(s: &str) -> String {
    if let Some(caps) = RE_TASK_NOTIFY_SUMMARY.captures(s) {
        if let Some(m) = caps.get(1) {
            return m.as_str().trim().to_string();
        }
    }
    // Fallback: strip all XML-like tags and return what's left.
    let re_tags = Regex::new(r"<[^>]+>").unwrap();
    let stripped = re_tags.replace_all(s, " ");
    let re_spaces = Regex::new(r"\s+").unwrap();
    re_spaces
        .replace_all(stripped.trim(), " ")
        .trim()
        .to_string()
}

/// Converts tool_result content (string or array of text blocks) to a string.
pub fn stringify_content(raw: &Option<Value>) -> String {
    let val = match raw {
        Some(v) => v,
        None => return String::new(),
    };

    // Try string first.
    if let Value::String(s) = val {
        return s.clone();
    }

    // Try array of text blocks.
    if let Value::Array(blocks) = val {
        let parts: Vec<&str> = blocks
            .iter()
            .filter_map(|b| {
                let text = b.get("text")?.as_str()?;
                if !text.is_empty() {
                    Some(text)
                } else {
                    None
                }
            })
            .collect();
        if !parts.is_empty() {
            return parts.join("\n");
        }
    }

    // Last resort: raw JSON string.
    val.to_string()
}
