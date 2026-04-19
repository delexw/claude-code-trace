use chrono::{DateTime, NaiveDateTime, Utc};
use lazy_static::lazy_static;
use regex::Regex;
use serde::Serialize;
use std::fs;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum DebugLevel {
    Debug,
    Warn,
    Error,
}

impl std::fmt::Display for DebugLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DebugLevel::Debug => write!(f, "DEBUG"),
            DebugLevel::Warn => write!(f, "WARN"),
            DebugLevel::Error => write!(f, "ERROR"),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct DebugEntry {
    pub timestamp: DateTime<Utc>,
    pub level: DebugLevel,
    pub category: String,
    pub message: String,
    pub extra: String,
    pub line_num: usize,
    pub count: usize,
}

lazy_static! {
    static ref HOOK_MSG_RE: Regex =
        Regex::new(r"^Hook ([^ (]+) \(([^)]+)\) (success|error|blocked):$").unwrap();
    static ref DEBUG_LINE_RE: Regex = Regex::new(
        r"^(\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z)\s+\[(DEBUG|WARN|ERROR)\]\s+(.*)$"
    )
    .unwrap();
    static ref DEBUG_CATEGORY_RE: Regex = Regex::new(r"^\[([^\]]+)\]\s*(.*)$").unwrap();
}

fn parse_level(s: &str) -> DebugLevel {
    match s {
        "WARN" => DebugLevel::Warn,
        "ERROR" => DebugLevel::Error,
        _ => DebugLevel::Debug,
    }
}

/// Read a debug log file from the beginning.
pub fn read_debug_log(path: &str) -> Result<(Vec<DebugEntry>, u64), String> {
    read_debug_log_incremental(path, 0)
}

fn read_debug_log_incremental(path: &str, offset: u64) -> Result<(Vec<DebugEntry>, u64), String> {
    let f = fs::File::open(path).map_err(|e| format!("opening {path}: {e}"))?;
    let mut reader = BufReader::new(f);
    reader
        .seek(SeekFrom::Start(offset))
        .map_err(|e| format!("seeking: {e}"))?;

    let mut entries = Vec::new();
    let mut bytes_read = offset;
    let mut line_num = if offset == 0 {
        0
    } else {
        count_lines_before_offset(path, offset)
    };

    let mut line = String::new();
    loop {
        line.clear();
        let n = reader
            .read_line(&mut line)
            .map_err(|e| format!("reading: {e}"))?;
        if n == 0 {
            break;
        }
        bytes_read += n as u64;
        line_num += 1;

        let trimmed = line.trim_end();
        if let Some(caps) = DEBUG_LINE_RE.captures(trimmed) {
            let ts_str = caps.get(1).unwrap().as_str();
            let ts = NaiveDateTime::parse_from_str(ts_str, "%Y-%m-%dT%H:%M:%S%.3fZ")
                .map(|naive| DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc))
                .unwrap_or_else(|_| Utc::now());
            let level = parse_level(caps.get(2).unwrap().as_str());
            let body = caps.get(3).unwrap().as_str();

            let (category, message) = if let Some(cm) = DEBUG_CATEGORY_RE.captures(body) {
                (
                    cm.get(1).unwrap().as_str().to_string(),
                    cm.get(2).unwrap().as_str().to_string(),
                )
            } else {
                (String::new(), body.to_string())
            };

            entries.push(DebugEntry {
                timestamp: ts,
                level,
                category,
                message,
                extra: String::new(),
                line_num,
                count: 1,
            });
        } else if !entries.is_empty() && !trimmed.is_empty() {
            let last = entries.last_mut().unwrap();
            if last.extra.is_empty() {
                last.extra = trimmed.to_string();
            } else {
                last.extra.push('\n');
                last.extra.push_str(trimmed);
            }
        }
    }

    Ok((entries, bytes_read))
}

fn count_lines_before_offset(path: &str, offset: u64) -> usize {
    if offset == 0 {
        return 0;
    }
    let f = match fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return 0,
    };
    let mut reader = BufReader::new(f);
    let mut count = 0;
    let mut read = 0u64;
    let mut buf = [0u8; 32 * 1024];
    use std::io::Read;
    while read < offset {
        let to_read = std::cmp::min(buf.len() as u64, offset - read) as usize;
        let n = match reader.read(&mut buf[..to_read]) {
            Ok(0) => break,
            Ok(n) => n,
            Err(_) => break,
        };
        for &byte in &buf[..n] {
            if byte == b'\n' {
                count += 1;
            }
        }
        read += n as u64;
    }
    count
}

/// Returns the debug log file path for a given session JSONL path.
pub fn debug_log_path(session_path: &str) -> String {
    let base = Path::new(session_path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    if base.is_empty() {
        return String::new();
    }
    let home = match dirs::home_dir() {
        Some(h) => h,
        None => return String::new(),
    };
    let debug_path = home
        .join(".claude")
        .join("debug")
        .join(format!("{base}.txt"));
    if debug_path.exists() {
        debug_path.to_string_lossy().to_string()
    } else {
        String::new()
    }
}

/// Filter entries by minimum level.
pub fn filter_by_level(entries: &[DebugEntry], min_level: &DebugLevel) -> Vec<DebugEntry> {
    if *min_level == DebugLevel::Debug {
        return entries.to_vec();
    }
    entries
        .iter()
        .filter(|e| e.level >= *min_level)
        .cloned()
        .collect()
}

/// Filter entries by text query (case-insensitive).
pub fn filter_by_text(entries: &[DebugEntry], query: &str) -> Vec<DebugEntry> {
    if query.is_empty() {
        return entries.to_vec();
    }
    let q = query.to_lowercase();
    entries
        .iter()
        .filter(|e| {
            e.message.to_lowercase().contains(&q)
                || e.category.to_lowercase().contains(&q)
                || e.extra.to_lowercase().contains(&q)
        })
        .cloned()
        .collect()
}

/// Extract hook execution events from a session's debug log as ClassifiedMsg::Hook.
///
/// Claude Code writes one `[DEBUG] Hook {name} ({event}) success:` line per hook
/// execution in `~/.claude/debug/{session_id}.txt` (only when run with `--debug`).
/// This function reads those lines and returns HookMsg entries that can be merged
/// into the session's classified message list to surface non-Stop hooks (PreToolUse,
/// PostToolUse, UserPromptSubmit, SessionStart, PreCompact, etc.) that are not recorded in JSONL.
///
/// Stop hooks are excluded because they are already captured via `stop_hook_summary`
/// system entries in the JSONL file.
pub fn extract_hook_msgs(session_path: &str) -> Vec<super::classify::ClassifiedMsg> {
    let debug_path = debug_log_path(session_path);
    if debug_path.is_empty() {
        return Vec::new();
    }
    let (entries, _) = match read_debug_log(&debug_path) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    entries
        .iter()
        .filter_map(|e| {
            let caps = HOOK_MSG_RE.captures(&e.message)?;
            let hook_name_full = caps.get(1)?.as_str(); // e.g. "PreToolUse:Agent"
            let hook_event = caps.get(2)?.as_str(); // e.g. "PreToolUse"
                                                    // Stop hooks are already captured via stop_hook_summary in the JSONL.
            if hook_event == "Stop" {
                return None;
            }
            // Extract the tool/matcher name after the colon (e.g. "Agent" from "PreToolUse:Agent").
            let hook_name = hook_name_full
                .find(':')
                .map(|i| &hook_name_full[i + 1..])
                .unwrap_or(hook_name_full)
                .to_string();
            let command = e.extra.clone();
            Some(super::classify::ClassifiedMsg::Hook(
                super::classify::HookMsg {
                    timestamp: e.timestamp,
                    hook_event: hook_event.to_string(),
                    hook_name,
                    command,
                    metadata: None,
                },
            ))
        })
        .collect()
}

/// Collapse consecutive duplicate entries.
pub fn collapse_duplicates(entries: Vec<DebugEntry>) -> Vec<DebugEntry> {
    if entries.is_empty() {
        return entries;
    }
    let mut result = Vec::new();
    let mut current = entries[0].clone();

    for entry in entries.into_iter().skip(1) {
        if entry.message == current.message && entry.extra.is_empty() && current.extra.is_empty() {
            current.count += 1;
        } else {
            result.push(current);
            current = entry;
        }
    }
    result.push(current);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::classify::ClassifiedMsg;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn write_debug_file(content: &str) -> NamedTempFile {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        f
    }

    #[test]
    fn extract_hook_msgs_returns_empty_for_missing_debug_log() {
        // session_path with no corresponding debug file → empty
        let result = extract_hook_msgs("/nonexistent/path/fake-uuid.jsonl");
        assert!(result.is_empty());
    }

    #[test]
    fn extract_hook_msgs_parses_pretooluse_and_posttooluse() {
        // Write a fake debug log to a temp file with known session UUID pattern.
        // We test the inner parsing by calling read_debug_log directly + applying
        // the same regex, since extract_hook_msgs relies on debug_log_path() which
        // looks up ~/.claude/debug/{session_id}.txt.
        let content = "\
2026-03-03T01:01:41.147Z [DEBUG] Hook PreToolUse:Agent (PreToolUse) success:\n\
[entire] PreToolUse[Task] hook invoked\n\
2026-03-03T01:01:48.664Z [DEBUG] Hook PostToolUse:Agent (PostToolUse) success:\n\
[entire] PostToolUse[Task] hook invoked\n\
2026-03-03T01:02:38.628Z [DEBUG] Hook Stop (Stop) success:\n\
stop output\n\
2026-03-03T01:03:00.000Z [DEBUG] Hook UserPromptSubmit (UserPromptSubmit) success:\n\
prompt captured\n\
";
        let f = write_debug_file(content);
        let (entries, _) = read_debug_log(f.path().to_str().unwrap()).unwrap();

        let hooks: Vec<ClassifiedMsg> = entries
            .iter()
            .filter_map(|e| {
                let caps = HOOK_MSG_RE.captures(&e.message)?;
                let hook_name_full = caps.get(1)?.as_str();
                let hook_event = caps.get(2)?.as_str();
                if hook_event == "Stop" {
                    return None;
                }
                let hook_name = hook_name_full
                    .find(':')
                    .map(|i| &hook_name_full[i + 1..])
                    .unwrap_or(hook_name_full)
                    .to_string();
                Some(ClassifiedMsg::Hook(crate::parser::classify::HookMsg {
                    timestamp: e.timestamp,
                    hook_event: hook_event.to_string(),
                    hook_name,
                    command: e.extra.clone(),
                    metadata: None,
                }))
            })
            .collect();

        assert_eq!(hooks.len(), 3); // PreToolUse, PostToolUse, UserPromptSubmit (Stop excluded)

        let events: Vec<&str> = hooks
            .iter()
            .map(|m| match m {
                ClassifiedMsg::Hook(h) => h.hook_event.as_str(),
                _ => "",
            })
            .collect();
        assert!(events.contains(&"PreToolUse"));
        assert!(events.contains(&"PostToolUse"));
        assert!(events.contains(&"UserPromptSubmit"));
        assert!(!events.contains(&"Stop"), "Stop should be excluded");
    }

    #[test]
    fn extract_hook_msgs_extracts_tool_name_from_colon_format() {
        let content =
            "2026-03-03T01:01:41.147Z [DEBUG] Hook PreToolUse:Read (PreToolUse) success:\n";
        let f = write_debug_file(content);
        let (entries, _) = read_debug_log(f.path().to_str().unwrap()).unwrap();

        let caps = HOOK_MSG_RE.captures(&entries[0].message).unwrap();
        let hook_name_full = caps.get(1).unwrap().as_str();
        let hook_name = hook_name_full
            .find(':')
            .map(|i| &hook_name_full[i + 1..])
            .unwrap_or(hook_name_full);
        assert_eq!(hook_name, "Read");
    }

    #[test]
    fn extract_hook_msgs_handles_hook_without_colon() {
        // e.g. "Hook UserPromptSubmit (UserPromptSubmit) success:"
        let content =
            "2026-03-03T01:01:41.147Z [DEBUG] Hook UserPromptSubmit (UserPromptSubmit) success:\n";
        let f = write_debug_file(content);
        let (entries, _) = read_debug_log(f.path().to_str().unwrap()).unwrap();

        let caps = HOOK_MSG_RE.captures(&entries[0].message).unwrap();
        let hook_name_full = caps.get(1).unwrap().as_str();
        let hook_name = hook_name_full
            .find(':')
            .map(|i| &hook_name_full[i + 1..])
            .unwrap_or(hook_name_full);
        assert_eq!(hook_name, "UserPromptSubmit");
    }

    #[test]
    fn extract_hook_msgs_parses_pre_compact_hook_event() {
        // v2.1.105: PreCompact fires before session compaction. The debug log captures it
        // with the same format as other hooks. The generic regex must match it.
        let content = "\
2026-04-13T10:00:00.000Z [DEBUG] Hook PreCompact (PreCompact) success:\n\
compaction allowed\n\
";
        let f = write_debug_file(content);
        let (entries, _) = read_debug_log(f.path().to_str().unwrap()).unwrap();

        let caps = HOOK_MSG_RE.captures(&entries[0].message).unwrap();
        assert_eq!(caps.get(2).unwrap().as_str(), "PreCompact");

        // Must not be excluded (only Stop is excluded).
        let hook_event = caps.get(2).unwrap().as_str();
        assert_ne!(hook_event, "Stop");

        let hook_name_full = caps.get(1).unwrap().as_str();
        let hook_name = hook_name_full
            .find(':')
            .map(|i| &hook_name_full[i + 1..])
            .unwrap_or(hook_name_full);
        assert_eq!(hook_name, "PreCompact");
    }

    #[test]
    fn hook_msg_re_matches_blocked_status_for_pre_compact() {
        // v2.1.105 PreCompact hooks that block compaction may log with "blocked:" status.
        let line = "Hook PreCompact (PreCompact) blocked:";
        let caps = HOOK_MSG_RE.captures(line).unwrap();
        assert_eq!(caps.get(1).unwrap().as_str(), "PreCompact");
        assert_eq!(caps.get(2).unwrap().as_str(), "PreCompact");
        assert_eq!(caps.get(3).unwrap().as_str(), "blocked");
    }
}
