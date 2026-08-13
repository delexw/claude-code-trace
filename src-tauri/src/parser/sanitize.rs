use regex::Regex;
use serde_json::Value;
use std::fs;

use super::patterns::*;

/// Returns true when a content block's `type` discriminant names a non-text
/// attachment: the known "image"/"document" shapes (v2.1.97+), or a future
/// attachment-shaped block whose exact type name isn't documented yet. Claude
/// Code v2.1.223's release notes mention a "diagnostics attachment" block
/// without specifying its `type` string (issue #235); matching on the
/// "attachment"/"diagnostic" substrings lets the parser recognize it (and any
/// similarly-named future attachment) without hardcoding a guessed literal.
/// None of the other known block discriminants (`text`, `tool_use`,
/// `tool_result`, `thinking`, `server_tool_use`, `advisor_tool_result`)
/// contain either substring, so this cannot misfire on them.
pub fn is_attachment_block_type(block_type: &str) -> bool {
    matches!(block_type, "image" | "document")
        || block_type.contains("attachment")
        || block_type.contains("diagnostic")
}

/// Extract text content from message.content (string or array of text blocks).
pub fn extract_text(content: &Option<Value>) -> String {
    match content {
        None => String::new(),
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(blocks)) => {
            let mut parts = Vec::new();
            for block in blocks {
                let block_type = block.get("type").and_then(|v| v.as_str()).unwrap_or("");
                match block_type {
                    "text" => {
                        if let Some(text) = block.get("text").and_then(|v| v.as_str()) {
                            if !text.is_empty() {
                                parts.push(text.to_string());
                            }
                        }
                    }
                    // Attachment blocks added in Claude Code v2.1.97: produce a
                    // placeholder so the user message is not silently empty in the UI.
                    "image" => {
                        parts.push("[Image attached]".to_string());
                    }
                    "document" => {
                        let media_type = block
                            .get("source")
                            .and_then(|s| s.get("media_type"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("document");
                        parts.push(format!("[Document attached: {media_type}]"));
                    }
                    // Issue #235: an unrecognized attachment-shaped block (e.g. a
                    // "diagnostics attachment") still needs to surface as visible
                    // content when it's the entry's only block -- see the fallback
                    // below. It's intentionally *not* rendered inline alongside real
                    // text/image/document blocks, matching how other unknown block
                    // types (streaming deltas, etc.) are silently skipped when mixed
                    // with real content.
                    _ => {}
                }
            }
            // If nothing renderable was found, but the array held at least one
            // attachment-shaped block we don't otherwise parse, show a generic
            // placeholder instead of silently rendering the message as empty.
            if parts.is_empty() {
                if let Some(bt) = blocks
                    .iter()
                    .filter_map(|b| b.get("type").and_then(|v| v.as_str()))
                    .find(|bt| is_attachment_block_type(bt))
                {
                    parts.push(format!("[Attachment: {bt}]"));
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

    // v2.1.224+: inbound cross-session SendMessage is wrapped in <cross-session-message
    // from="..." from-name="...">...</cross-session-message> (issue #237). Unwrap it here,
    // before any other processing, so the raw tag never leaks into the displayed text.
    if let Some(text) = extract_cross_session_message(s) {
        return text;
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
            return Some(format!("{name} {args}"));
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

/// Unwraps a <cross-session-message from="..." from-name="...">...</cross-session-message>
/// tag (v2.1.224+ cross-session SendMessage, issue #237), prefixing the inner text with the
/// sender's display name so attribution is not lost. Returns None when `s` isn't wrapped in
/// this tag.
pub fn extract_cross_session_message(s: &str) -> Option<String> {
    let caps = CROSS_SESSION_MESSAGE_RE.captures(s)?;
    let inner = caps.get(1).map(|m| m.as_str().trim()).unwrap_or("");
    let from_name = CROSS_SESSION_FROM_NAME_RE
        .captures(s)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().trim())
        .filter(|n| !n.is_empty());
    Some(match from_name {
        Some(name) => format!("[{name}]: {inner}"),
        None => inner.to_string(),
    })
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

/// Resolves a hook output field that may be either a plain string or a structured
/// file-reference object introduced in Claude Code v2.1.89.
///
/// From v2.1.89, hook stdout over 50,000 characters is saved to a temporary file
/// and the field contains `{"path": "/tmp/...", "preview": "...(truncated)"}` instead
/// of a plain string. This function reads the full file when it is still on disk,
/// otherwise falls back to the `preview` string from the object.
pub fn resolve_hook_output(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Object(map) => {
            if let Some(path) = map.get("path").and_then(|p| p.as_str()) {
                if let Ok(content) = fs::read_to_string(path) {
                    return content;
                }
            }
            map.get("preview")
                .and_then(|p| p.as_str())
                .unwrap_or("")
                .to_string()
        }
        _ => String::new(),
    }
}

/// Detects `<persisted-output>` markers in tool result content and resolves
/// the file reference by reading the persisted file from disk.
/// Returns the full file content if found, otherwise the original string.
pub fn resolve_persisted_output(s: &str) -> String {
    if !s.contains("<persisted-output>") {
        return s.to_string();
    }
    if let Some(caps) = RE_PERSISTED_OUTPUT_PATH.captures(s) {
        if let Some(m) = caps.get(1) {
            let path = m.as_str().trim();
            if let Ok(content) = fs::read_to_string(path) {
                return content;
            }
        }
    }
    // File not found or no path match — return original content as fallback.
    s.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ---- extract_text tests ----

    #[test]
    fn extract_text_none() {
        assert_eq!(extract_text(&None), "");
    }

    #[test]
    fn extract_text_string() {
        let v = Some(json!("hello world"));
        assert_eq!(extract_text(&v), "hello world");
    }

    #[test]
    fn extract_text_array_with_text_blocks() {
        let v = Some(json!([
            {"type": "text", "text": "first"},
            {"type": "text", "text": "second"}
        ]));
        assert_eq!(extract_text(&v), "first\nsecond");
    }

    #[test]
    fn extract_text_image_block_produces_placeholder() {
        // Issue #37: image attachment blocks persisted from v2.1.97 must not silently drop.
        let v = Some(
            json!([{"type": "image", "source": {"type": "base64", "media_type": "image/png", "data": "abc"}}]),
        );
        assert_eq!(extract_text(&v), "[Image attached]");
    }

    #[test]
    fn extract_text_document_block_produces_placeholder() {
        // Issue #37: document attachment blocks must show media_type in placeholder.
        let v = Some(
            json!([{"type": "document", "source": {"type": "base64", "media_type": "application/pdf", "data": "abc"}}]),
        );
        assert_eq!(extract_text(&v), "[Document attached: application/pdf]");
    }

    #[test]
    fn extract_text_document_block_missing_media_type() {
        let v = Some(json!([{"type": "document", "source": {}}]));
        assert_eq!(extract_text(&v), "[Document attached: document]");
    }

    #[test]
    fn extract_text_mixed_text_and_image() {
        let v = Some(json!([
            {"type": "text", "text": "see this"},
            {"type": "image", "source": {"type": "base64", "media_type": "image/png", "data": ""}}
        ]));
        assert_eq!(extract_text(&v), "see this\n[Image attached]");
    }

    #[test]
    fn extract_text_array_skips_non_text() {
        // Unknown block types (not image/document) are silently skipped.
        let v = Some(json!([
            {"type": "thinking_delta", "text": "ignored"},
            {"type": "text", "text": "only this"}
        ]));
        assert_eq!(extract_text(&v), "only this");
    }

    // --- Issue #235: unrecognized "diagnostics attachment"-shaped content block ---

    #[test]
    fn extract_text_diagnostics_attachment_only_produces_placeholder() {
        // v2.1.223 introduced a "diagnostics attachment" content block whose exact
        // `type` string isn't documented. When it's the entry's only block, it must
        // not render as an empty message.
        let v = Some(json!([{"type": "diagnostics_attachment", "diagnostics": {"code": "E1"}}]));
        assert_eq!(extract_text(&v), "[Attachment: diagnostics_attachment]");
    }

    #[test]
    fn extract_text_unknown_attachment_variant_produces_placeholder() {
        // Any type string containing "attachment" or "diagnostic" is recognized,
        // not just the exact literal guessed above -- the real schema is unknown.
        let v = Some(json!([{"type": "diagnosticAttachment"}]));
        assert_eq!(extract_text(&v), "[Attachment: diagnosticAttachment]");
    }

    #[test]
    fn extract_text_diagnostics_attachment_alongside_text_is_not_duplicated() {
        // When real text is present, the unrecognized attachment stays silent --
        // matching how unknown block types are skipped when mixed with real content
        // (extract_text_array_skips_non_text).
        let v = Some(json!([
            {"type": "text", "text": "see attached diagnostics"},
            {"type": "diagnostics_attachment", "diagnostics": {"code": "E1"}}
        ]));
        assert_eq!(extract_text(&v), "see attached diagnostics");
    }

    #[test]
    fn extract_text_malformed_diagnostics_attachment_does_not_panic() {
        // A malformed attachment (missing every expected field) must still produce
        // a placeholder rather than panicking or silently emptying the message --
        // the exact failure mode the upstream CLI bug this issue tracks exhibited.
        let v = Some(json!([{"type": "diagnostics_attachment"}]));
        assert_eq!(extract_text(&v), "[Attachment: diagnostics_attachment]");
    }

    #[test]
    fn is_attachment_block_type_matches_known_and_future_shapes() {
        assert!(is_attachment_block_type("image"));
        assert!(is_attachment_block_type("document"));
        assert!(is_attachment_block_type("diagnostics_attachment"));
        assert!(is_attachment_block_type("diagnostic_info"));
        assert!(!is_attachment_block_type("text"));
        assert!(!is_attachment_block_type("tool_use"));
        assert!(!is_attachment_block_type("tool_result"));
        assert!(!is_attachment_block_type("thinking"));
    }

    #[test]
    fn extract_text_empty_array() {
        let v = Some(json!([]));
        assert_eq!(extract_text(&v), "");
    }

    #[test]
    fn extract_text_number_returns_empty() {
        let v = Some(json!(42));
        assert_eq!(extract_text(&v), "");
    }

    // ---- sanitize_content tests ----

    #[test]
    fn sanitize_plain_text() {
        assert_eq!(sanitize_content("Hello, world!"), "Hello, world!");
    }

    #[test]
    fn sanitize_command_output_stdout() {
        let s = "<local-command-stdout>output text</local-command-stdout>";
        assert_eq!(sanitize_content(s), "output text");
    }

    #[test]
    fn sanitize_command_message() {
        let s = "<command-name>/commit</command-name><command-args>fix bug</command-args>";
        assert_eq!(sanitize_content(s), "/commit fix bug");
    }

    #[test]
    fn sanitize_command_name_only() {
        let s = "<command-name>/help</command-name>";
        assert_eq!(sanitize_content(s), "/help");
    }

    #[test]
    fn sanitize_noise_tags_removed() {
        let s = "before<system-reminder>noise</system-reminder>after";
        assert_eq!(sanitize_content(s), "beforeafter");
    }

    #[test]
    fn sanitize_local_command_caveat_removed() {
        let s = "text<local-command-caveat>caveat</local-command-caveat>end";
        assert_eq!(sanitize_content(s), "textend");
    }

    // ---- Issue #237: v2.1.224+ cross-session SendMessage compat ----

    #[test]
    fn extract_cross_session_message_with_from_name() {
        let s = "<cross-session-message from=\"session-on-laptop-b\" from-name=\"laptop-b\">\nHey, can you check the build?\n</cross-session-message>";
        assert_eq!(
            extract_cross_session_message(s),
            Some("[laptop-b]: Hey, can you check the build?".to_string())
        );
    }

    #[test]
    fn extract_cross_session_message_without_from_name() {
        // from-name is expected to always be present in practice, but the extractor must
        // not panic or lose content if it's ever missing.
        let s = "<cross-session-message from=\"session-on-laptop-b\">\nStatus check\n</cross-session-message>";
        assert_eq!(
            extract_cross_session_message(s),
            Some("Status check".to_string())
        );
    }

    #[test]
    fn extract_cross_session_message_returns_none_for_unrelated_content() {
        assert_eq!(extract_cross_session_message("just a normal message"), None);
    }

    #[test]
    fn sanitize_cross_session_message_unwraps_tag_with_attribution() {
        let s = "<cross-session-message from=\"session-abc\" from-name=\"desktop-a\">\nDeploy finished on my end.\n</cross-session-message>";
        assert_eq!(
            sanitize_content(s),
            "[desktop-a]: Deploy finished on my end."
        );
    }

    // ---- extract_bash_output tests ----

    #[test]
    fn extract_bash_output_stdout() {
        let s = "<bash-stdout>hello bash</bash-stdout>";
        assert_eq!(extract_bash_output(s), "hello bash");
    }

    #[test]
    fn extract_bash_output_stderr() {
        let s = "<bash-stderr>error msg</bash-stderr>";
        assert_eq!(extract_bash_output(s), "error msg");
    }

    #[test]
    fn extract_bash_output_neither() {
        assert_eq!(extract_bash_output("just plain text"), "");
    }

    #[test]
    fn extract_bash_output_prefers_stdout() {
        let s = "<bash-stdout>out</bash-stdout><bash-stderr>err</bash-stderr>";
        assert_eq!(extract_bash_output(s), "out");
    }

    // ---- stringify_content tests ----

    #[test]
    fn stringify_content_none() {
        assert_eq!(stringify_content(&None), "");
    }

    #[test]
    fn stringify_content_string() {
        let v = Some(json!("simple string"));
        assert_eq!(stringify_content(&v), "simple string");
    }

    #[test]
    fn stringify_content_array_text_blocks() {
        let v = Some(json!([
            {"text": "block1"},
            {"text": "block2"}
        ]));
        assert_eq!(stringify_content(&v), "block1\nblock2");
    }

    #[test]
    fn stringify_content_array_empty_text() {
        let v = Some(json!([{"text": ""}]));
        // Empty text blocks are filtered, falls through to raw JSON
        let result = stringify_content(&v);
        assert!(!result.is_empty());
    }

    #[test]
    fn stringify_content_non_string_value() {
        let v = Some(json!(42));
        assert_eq!(stringify_content(&v), "42");
    }

    #[test]
    fn stringify_content_object_value() {
        let v = Some(json!({"key": "value"}));
        let result = stringify_content(&v);
        assert!(result.contains("key"));
        assert!(result.contains("value"));
    }

    // ---- resolve_hook_output tests ----

    #[test]
    fn resolve_hook_output_plain_string_returned_as_is() {
        let v = json!("hook output text");
        assert_eq!(resolve_hook_output(&v), "hook output text");
    }

    #[test]
    fn resolve_hook_output_object_returns_preview_when_file_missing() {
        let v =
            json!({"path": "/tmp/nonexistent_hook_file_xyz.txt", "preview": "preview text here"});
        assert_eq!(resolve_hook_output(&v), "preview text here");
    }

    #[test]
    fn resolve_hook_output_object_reads_file_when_exists() {
        let dir = std::env::temp_dir();
        let path = dir.join("test_hook_output_file.txt");
        std::fs::write(&path, "full hook output content").unwrap();
        let v = json!({"path": path.to_str().unwrap(), "preview": "truncated preview"});
        let result = resolve_hook_output(&v);
        assert_eq!(result, "full hook output content");
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn resolve_hook_output_non_string_non_object_returns_empty() {
        assert_eq!(resolve_hook_output(&json!(42)), "");
        assert_eq!(resolve_hook_output(&json!(null)), "");
        assert_eq!(resolve_hook_output(&json!([1, 2])), "");
    }

    #[test]
    fn resolve_hook_output_object_without_preview_returns_empty() {
        let v = json!({"path": "/tmp/missing.txt"});
        assert_eq!(resolve_hook_output(&v), "");
    }

    // ---- resolve_persisted_output tests ----

    #[test]
    fn resolve_persisted_output_returns_original_when_no_tag() {
        assert_eq!(resolve_persisted_output("plain text"), "plain text");
    }

    #[test]
    fn resolve_persisted_output_returns_original_when_file_missing() {
        let s = "<persisted-output>\nOutput too large (100KB). Full output saved to: /tmp/nonexistent_file_12345.txt\n\nPreview:\nsome preview\n</persisted-output>";
        let result = resolve_persisted_output(s);
        assert_eq!(result, s);
    }

    #[test]
    fn resolve_persisted_output_reads_file_when_exists() {
        let dir = std::env::temp_dir();
        let path = dir.join("test_persisted_output.txt");
        std::fs::write(&path, "full file content here").unwrap();

        let s = format!(
            "<persisted-output>\nOutput too large (100KB). Full output saved to: {}\n\nPreview:\ntruncated...\n</persisted-output>",
            path.display()
        );
        let result = resolve_persisted_output(&s);
        assert_eq!(result, "full file content here");

        std::fs::remove_file(&path).ok();
    }

    // ---- extract_command_output tests ----

    #[test]
    fn extract_command_output_stdout() {
        let s = "<local-command-stdout>output here</local-command-stdout>";
        assert_eq!(extract_command_output(s), "output here");
    }

    #[test]
    fn extract_command_output_stderr() {
        let s = "<local-command-stderr>err output</local-command-stderr>";
        assert_eq!(extract_command_output(s), "err output");
    }

    #[test]
    fn extract_command_output_neither() {
        assert_eq!(extract_command_output("no tags here"), "");
    }

    // ---- is_command_output tests ----

    #[test]
    fn is_command_output_with_stdout() {
        assert!(is_command_output(
            "<local-command-stdout>stuff</local-command-stdout>"
        ));
    }

    #[test]
    fn is_command_output_with_stderr() {
        assert!(is_command_output(
            "<local-command-stderr>stuff</local-command-stderr>"
        ));
    }

    #[test]
    fn is_command_output_plain_text() {
        assert!(!is_command_output("just text"));
    }

    #[test]
    fn is_command_output_bash_tags() {
        // bash-stdout is NOT a local-command tag
        assert!(!is_command_output("<bash-stdout>stuff</bash-stdout>"));
    }
}
