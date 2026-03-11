use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::PathBuf;

use super::chunk::{build_chunks, Chunk};
use super::classify::{classify, ClassifiedMsg};
use super::entry::parse_entry;

/// SessionInfo holds metadata about a discovered session file for the picker.
#[derive(Debug, Clone, Serialize)]
pub struct SessionInfo {
    pub path: String,
    pub session_id: String,
    pub mod_time: DateTime<Utc>,
    pub first_message: String,
    pub turn_count: i32,
    pub is_ongoing: bool,
    pub total_tokens: i64,
    pub duration_ms: i64,
    pub model: String,
    pub cwd: String,
    pub git_branch: String,
    pub permission_mode: String,
}

/// SessionMeta holds session-level metadata extracted from a JSONL file.
#[derive(Debug, Clone, Default, Serialize)]
pub struct SessionMeta {
    pub cwd: String,
    pub git_branch: String,
    pub permission_mode: String,
}

/// Extract session metadata from a JSONL file.
pub fn extract_session_meta(path: &str) -> SessionMeta {
    let meta = scan_session_metadata(path);
    SessionMeta {
        cwd: meta.cwd,
        git_branch: meta.git_branch,
        permission_mode: meta.permission_mode,
    }
}

/// Read a JSONL session file and return the fully processed chunk list.
pub fn read_session(path: &str) -> Result<Vec<Chunk>, String> {
    let (msgs, _, _) = read_session_incremental(path, 0)?;
    Ok(build_chunks(&msgs))
}

/// Read new lines from a session file starting at the given byte offset.
/// Returns (new classified messages, updated offset, bytes read).
pub fn read_session_incremental(path: &str, offset: u64) -> Result<(Vec<ClassifiedMsg>, u64, u64), String> {
    let f = fs::File::open(path).map_err(|e| format!("opening {}: {}", path, e))?;
    let mut reader = BufReader::new(f);
    reader.seek(SeekFrom::Start(offset)).map_err(|e| format!("seeking: {}", e))?;

    let mut msgs = Vec::new();
    let mut bytes_read: u64 = 0;
    let mut line = String::new();

    loop {
        line.clear();
        let n = reader.read_line(&mut line).map_err(|e| format!("reading: {}", e))?;
        if n == 0 {
            break;
        }
        bytes_read += n as u64;

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if let Some(entry) = parse_entry(trimmed.as_bytes()) {
            if let Some(msg) = classify(entry) {
                msgs.push(msg);
            }
        }
    }

    Ok((msgs, offset + bytes_read, bytes_read))
}

/// Return the Claude CLI projects directory for an absolute path.
pub fn project_dir_for_path(abs_path: &str) -> Result<String, String> {
    let home = dirs::home_dir().ok_or("no home directory")?;
    let resolved = fs::canonicalize(abs_path).unwrap_or_else(|_| PathBuf::from(abs_path));
    let encoded = encode_path(&resolved.to_string_lossy());
    Ok(home
        .join(".claude")
        .join("projects")
        .join(encoded)
        .to_string_lossy()
        .to_string())
}

fn encode_path(abs_path: &str) -> String {
    abs_path
        .replace(std::path::MAIN_SEPARATOR, "-")
        .replace('/', "-")
        .replace('.', "-")
        .replace('_', "-")
}

/// Return the projects directory for the current working directory.
/// If inside a git worktree, resolves to the main repo root so sessions
/// are found under the original project path.
pub fn current_project_dir() -> Result<String, String> {
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    let cwd_str = cwd.to_string_lossy().to_string();

    // Resolve git root for worktree support.
    let resolved = super::project::resolve_git_root(&cwd_str);
    project_dir_for_path(&resolved)
}

/// Discover all session .jsonl files in a project directory.
pub fn discover_project_sessions(project_dir: &str) -> Result<Vec<SessionInfo>, String> {
    let entries = fs::read_dir(project_dir).map_err(|e| format!("reading {}: {}", project_dir, e))?;

    let mut sessions = Vec::new();

    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.ends_with(".jsonl") || name.starts_with("agent_") {
            continue;
        }
        if entry.file_type().map(|t| t.is_dir()).unwrap_or(true) {
            continue;
        }

        let metadata = entry.metadata();
        let mod_time = metadata
            .as_ref()
            .ok()
            .and_then(|m| m.modified().ok())
            .map(|t| DateTime::<Utc>::from(t))
            .unwrap_or_else(Utc::now);

        let path = entry.path().to_string_lossy().to_string();
        let meta = scan_session_metadata(&path);

        if meta.turn_count == 0 {
            continue;
        }

        let mut is_ongoing = meta.is_ongoing;
        if is_ongoing {
            if let Ok(m) = entry.metadata() {
                if let Ok(modified) = m.modified() {
                    let elapsed = std::time::SystemTime::now()
                        .duration_since(modified)
                        .unwrap_or_default();
                    if elapsed > std::time::Duration::from_secs(120) {
                        is_ongoing = false;
                    }
                }
            }
        }

        let session_id = name.trim_end_matches(".jsonl").to_string();

        sessions.push(SessionInfo {
            path,
            session_id,
            mod_time,
            first_message: meta.first_msg,
            turn_count: meta.turn_count,
            is_ongoing,
            total_tokens: meta.total_tokens,
            duration_ms: meta.duration_ms,
            model: meta.model,
            cwd: meta.cwd,
            git_branch: meta.git_branch,
            permission_mode: meta.permission_mode,
        });
    }

    sessions.sort_by(|a, b| b.mod_time.cmp(&a.mod_time));
    Ok(sessions)
}

/// Discover sessions across multiple project directories.
pub fn discover_all_project_sessions(project_dirs: &[String]) -> Result<Vec<SessionInfo>, String> {
    let mut all = Vec::new();
    for dir in project_dirs {
        if let Ok(sessions) = discover_project_sessions(dir) {
            all.extend(sessions);
        }
    }
    all.sort_by(|a, b| b.mod_time.cmp(&a.mod_time));
    Ok(all)
}

/// Convert scanned metadata into a SessionInfo struct.
/// Public for use by SessionCache.
pub fn session_info_from_metadata(path: &str, mod_time: std::time::SystemTime, meta: SessionMetadata) -> SessionInfo {
    let mod_time_chrono: DateTime<Utc> = mod_time.into();
    let mut is_ongoing = meta.is_ongoing;
    if is_ongoing {
        let elapsed = std::time::SystemTime::now()
            .duration_since(mod_time)
            .unwrap_or_default();
        if elapsed > std::time::Duration::from_secs(120) {
            is_ongoing = false;
        }
    }
    let name = std::path::Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");
    let session_id = name.trim_end_matches(".jsonl").to_string();
    SessionInfo {
        path: path.to_string(),
        session_id,
        mod_time: mod_time_chrono,
        first_message: meta.first_msg,
        turn_count: meta.turn_count,
        is_ongoing,
        total_tokens: meta.total_tokens,
        duration_ms: meta.duration_ms,
        model: meta.model,
        cwd: meta.cwd,
        git_branch: meta.git_branch,
        permission_mode: meta.permission_mode,
    }
}

/// Discover sessions using a custom scan function (for caching).
pub fn discover_project_sessions_with_scan<F>(
    project_dir: &str,
    scan: F,
) -> Result<Vec<SessionInfo>, String>
where
    F: Fn(&str, std::time::SystemTime, u64) -> Option<SessionInfo>,
{
    let entries = fs::read_dir(project_dir).map_err(|e| format!("reading {}: {}", project_dir, e))?;

    let mut sessions = Vec::new();

    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.ends_with(".jsonl") || name.starts_with("agent_") {
            continue;
        }
        if entry.file_type().map(|t| t.is_dir()).unwrap_or(true) {
            continue;
        }

        let metadata = match entry.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };
        let mod_time = metadata.modified().unwrap_or(std::time::SystemTime::now());
        let size = metadata.len();

        let path = entry.path().to_string_lossy().to_string();
        if let Some(info) = scan(&path, mod_time, size) {
            sessions.push(info);
        }
    }

    sessions.sort_by(|a, b| b.mod_time.cmp(&a.mod_time));
    Ok(sessions)
}

// Internal metadata scan result.
pub(crate) struct SessionMetadata {
    pub(crate) first_msg: String,
    pub(crate) turn_count: i32,
    pub(crate) is_ongoing: bool,
    pub(crate) total_tokens: i64,
    pub(crate) duration_ms: i64,
    pub(crate) model: String,
    pub(crate) cwd: String,
    pub(crate) git_branch: String,
    pub(crate) permission_mode: String,
}

impl Default for SessionMetadata {
    fn default() -> Self {
        Self {
            first_msg: String::new(),
            turn_count: 0,
            is_ongoing: false,
            total_tokens: 0,
            duration_ms: 0,
            model: String::new(),
            cwd: String::new(),
            git_branch: String::new(),
            permission_mode: String::new(),
        }
    }
}

pub(crate) fn scan_session_metadata(path: &str) -> SessionMetadata {
    use super::classify::parse_timestamp;
    use super::patterns::RE_COMMAND_NAME;
    use super::sanitize::{extract_text, is_command_output, sanitize_content};

    let f = match fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return SessionMetadata::default(),
    };
    let reader = BufReader::new(f);

    let mut meta = SessionMetadata::default();
    let mut command_fallback = String::new();
    let mut preview_found = false;
    let mut lines_read = 0;
    const MAX_PREVIEW_LINES: usize = 200;

    // Turn counting: user message increments, then first qualifying AI response increments.
    let mut awaiting_ai_group = false;

    // Token deduplication: track per-requestId usage, sum once at end.
    #[derive(Clone)]
    struct TokenSnapshot {
        input: i64,
        output: i64,
        cache_read: i64,
        cache_create: i64,
    }
    let mut request_tokens: HashMap<String, TokenSnapshot> = HashMap::new();

    // Ongoing detection state (one-pass, ported from jsonl.ts).
    let mut activity_index: usize = 0;
    let mut last_ending_index: Option<usize> = None;
    let mut has_any_ongoing_activity = false;
    let mut has_activity_after_last_ending = false;
    let mut shutdown_tool_ids: HashSet<String> = HashSet::new();
    let mut pending_tool_ids: HashSet<String> = HashSet::new();

    // Duration tracking.
    let mut first_ts: Option<DateTime<Utc>> = None;
    let mut last_ts: Option<DateTime<Utc>> = None;

    for line_result in reader.lines() {
        let line = match line_result {
            Ok(l) => l,
            Err(_) => break,
        };
        lines_read += 1;

        let raw: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let uuid = raw.get("uuid").and_then(|v| v.as_str()).unwrap_or("");
        if uuid.is_empty() {
            continue;
        }

        let entry_type = raw.get("type").and_then(|v| v.as_str()).unwrap_or("");
        let is_sidechain = raw
            .get("isSidechain")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let is_meta_flag = raw
            .get("isMeta")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        // Track timestamps for duration.
        if let Some(ts_str) = raw.get("timestamp").and_then(|v| v.as_str()) {
            let ts = parse_timestamp(ts_str);
            if first_ts.is_none() {
                first_ts = Some(ts);
            }
            last_ts = Some(ts);
        }

        // --- Session-level metadata (cwd, branch: first seen; mode: last seen) ---
        if meta.cwd.is_empty() {
            if let Some(cwd) = raw.get("cwd").and_then(|v| v.as_str()) {
                if !cwd.is_empty() {
                    meta.cwd = cwd.to_string();
                }
            }
        }
        if meta.git_branch.is_empty() {
            if let Some(branch) = raw.get("gitBranch").and_then(|v| v.as_str()) {
                if !branch.is_empty() {
                    meta.git_branch = branch.to_string();
                }
            }
        }
        if let Some(mode) = raw.get("permissionMode").and_then(|v| v.as_str()) {
            if !mode.is_empty() {
                meta.permission_mode = mode.to_string();
            }
        }

        // --- Turn counting (matches isParsedUserChunkMessage + AI pairing) ---
        if is_user_chunk_for_turn_count(&raw, entry_type, is_meta_flag, is_sidechain) {
            meta.turn_count += 1;
            awaiting_ai_group = true;
        } else if awaiting_ai_group && entry_type == "assistant" && !is_sidechain {
            let model_str = raw
                .get("message")
                .and_then(|m| m.get("model"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if model_str != "<synthetic>" {
                meta.turn_count += 1;
                awaiting_ai_group = false;
            }
        }

        // --- Token accumulation (dedup streaming entries by requestId) ---
        if entry_type == "assistant" && !is_sidechain {
            let model_str = raw
                .get("message")
                .and_then(|m| m.get("model"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if model_str != "<synthetic>" {
                if let Some(usage) = raw.get("message").and_then(|m| m.get("usage")) {
                    let snap = TokenSnapshot {
                        input: usage
                            .get("input_tokens")
                            .and_then(|v| v.as_i64())
                            .unwrap_or(0),
                        output: usage
                            .get("output_tokens")
                            .and_then(|v| v.as_i64())
                            .unwrap_or(0),
                        cache_read: usage
                            .get("cache_read_input_tokens")
                            .and_then(|v| v.as_i64())
                            .unwrap_or(0),
                        cache_create: usage
                            .get("cache_creation_input_tokens")
                            .and_then(|v| v.as_i64())
                            .unwrap_or(0),
                    };

                    let request_id = raw
                        .get("requestId")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    if !request_id.is_empty() {
                        // Last-entry-wins: streaming entries share a requestId.
                        request_tokens.insert(request_id.to_string(), snap);
                    } else {
                        // No requestId: sum directly.
                        meta.total_tokens +=
                            snap.input + snap.output + snap.cache_read + snap.cache_create;
                    }
                }

                // Model extraction (first real assistant entry).
                if meta.model.is_empty() && !model_str.is_empty() {
                    meta.model = model_str.to_string();
                }
            }
        }

        // --- Ongoing detection ---
        if entry_type == "assistant" && !is_sidechain {
            scan_ongoing_assistant(
                &raw,
                &mut activity_index,
                &mut last_ending_index,
                &mut has_any_ongoing_activity,
                &mut has_activity_after_last_ending,
                &mut shutdown_tool_ids,
                &mut pending_tool_ids,
            );
        } else if entry_type == "user" {
            scan_ongoing_user(
                &raw,
                &mut activity_index,
                &mut last_ending_index,
                &mut has_any_ongoing_activity,
                &mut has_activity_after_last_ending,
                &mut shutdown_tool_ids,
                &mut pending_tool_ids,
            );
        }

        // --- Preview extraction ---
        if preview_found || lines_read > MAX_PREVIEW_LINES || entry_type != "user" {
            continue;
        }

        let content = raw
            .get("message")
            .and_then(|m| m.get("content"))
            .cloned();
        let text = extract_text(&content);
        if text.is_empty() {
            continue;
        }

        if is_command_output(&text) || text.starts_with("[Request interrupted by user") {
            continue;
        }

        if text.starts_with("<command-name>") {
            if command_fallback.is_empty() {
                if let Some(caps) = RE_COMMAND_NAME.captures(&text) {
                    command_fallback =
                        format!("/{}", caps.get(1).map_or("command", |m| m.as_str()).trim());
                } else {
                    command_fallback = "/command".to_string();
                }
            }
            continue;
        }

        let sanitized = sanitize_content(&text);
        let sanitized = sanitized.trim();
        if !sanitized.is_empty() {
            let msg: String = sanitized.chars().take(500).collect();
            meta.first_msg = msg;
            preview_found = true;
        }
    }

    if meta.first_msg.is_empty() {
        meta.first_msg = command_fallback;
    }
    if !meta.first_msg.is_empty() {
        meta.first_msg = meta.first_msg.replace('\n', " ");
    }
    if meta.permission_mode.is_empty() {
        meta.permission_mode = "default".to_string();
    }

    // Finalize token totals: sum the last-seen usage per requestId.
    for snap in request_tokens.values() {
        meta.total_tokens += snap.input + snap.output + snap.cache_read + snap.cache_create;
    }

    // Finalize ongoing detection.
    if last_ending_index.is_none() {
        meta.is_ongoing = has_any_ongoing_activity;
    } else {
        meta.is_ongoing = has_activity_after_last_ending;
    }
    // Pending tool calls override.
    if !meta.is_ongoing && !pending_tool_ids.is_empty() {
        meta.is_ongoing = true;
    }

    // Finalize duration.
    if let (Some(first), Some(last)) = (first_ts, last_ts) {
        meta.duration_ms = last.signed_duration_since(first).num_milliseconds();
    }

    meta
}

/// Mirrors claude-devtools' isParsedUserChunkMessage.
fn is_user_chunk_for_turn_count(
    raw: &Value,
    entry_type: &str,
    is_meta: bool,
    is_sidechain: bool,
) -> bool {
    use super::classify::SYSTEM_OUTPUT_TAGS;
    use super::patterns::TEAMMATE_MESSAGE_RE;
    use super::sanitize::extract_text;

    if entry_type != "user" || is_meta || is_sidechain {
        return false;
    }

    let content = raw
        .get("message")
        .and_then(|m| m.get("content"))
        .cloned();
    let text = extract_text(&content);
    let trimmed = text.trim();

    // Teammate messages.
    if TEAMMATE_MESSAGE_RE.is_match(trimmed) {
        return false;
    }

    // System output tags.
    for tag in SYSTEM_OUTPUT_TAGS {
        if trimmed.starts_with(tag) {
            return false;
        }
    }

    // Must have actual content.
    has_user_content_raw(&content, &text)
}

fn has_user_content_raw(raw: &Option<Value>, str_content: &str) -> bool {
    match raw {
        Some(Value::String(_)) => !str_content.trim().is_empty(),
        Some(Value::Array(blocks)) => blocks.iter().any(|b| {
            let bt = b.get("type").and_then(|v| v.as_str()).unwrap_or("");
            bt == "text" || bt == "image"
        }),
        _ => false,
    }
}

/// Process an assistant entry for ongoing detection (ported from jsonl.ts:438-470).
fn scan_ongoing_assistant(
    raw: &Value,
    activity_index: &mut usize,
    last_ending_index: &mut Option<usize>,
    has_any: &mut bool,
    has_after: &mut bool,
    shutdown_ids: &mut HashSet<String>,
    pending_tool_ids: &mut HashSet<String>,
) {
    use super::ongoing::is_shutdown_approval;

    let blocks = match raw
        .get("message")
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_array())
    {
        Some(b) => b,
        None => return,
    };

    for b in blocks {
        let bt = b.get("type").and_then(|v| v.as_str()).unwrap_or("");
        match bt {
            "thinking" => {
                let thinking = b.get("thinking").and_then(|v| v.as_str()).unwrap_or("");
                if !thinking.trim().is_empty() {
                    *has_any = true;
                    if last_ending_index.is_some() {
                        *has_after = true;
                    }
                    *activity_index += 1;
                }
            }
            "tool_use" => {
                let id = b
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                if id.is_empty() {
                    continue;
                }
                let name = b
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                if name == "ExitPlanMode" {
                    *last_ending_index = Some(*activity_index);
                    *has_after = false;
                    *activity_index += 1;
                } else if is_shutdown_approval(&name, &b.get("input").cloned()) {
                    shutdown_ids.insert(id);
                    *last_ending_index = Some(*activity_index);
                    *has_after = false;
                    *activity_index += 1;
                } else {
                    pending_tool_ids.insert(id);
                    *has_any = true;
                    if last_ending_index.is_some() {
                        *has_after = true;
                    }
                    *activity_index += 1;
                }
            }
            "text" => {
                let text = b.get("text").and_then(|v| v.as_str()).unwrap_or("");
                if !text.trim().is_empty() {
                    *last_ending_index = Some(*activity_index);
                    *has_after = false;
                    *activity_index += 1;
                }
            }
            _ => {}
        }
    }
}

/// Process a user entry for ongoing detection (ported from jsonl.ts:471-499).
fn scan_ongoing_user(
    raw: &Value,
    activity_index: &mut usize,
    last_ending_index: &mut Option<usize>,
    has_any: &mut bool,
    has_after: &mut bool,
    shutdown_ids: &mut HashSet<String>,
    pending_tool_ids: &mut HashSet<String>,
) {
    // Check for user-rejected tool use at the entry level.
    let is_rejection = is_tool_use_rejection(raw);

    // String-content user entries (e.g. "[Request interrupted by user...]").
    let content = raw.get("message").and_then(|m| m.get("content"));
    if let Some(Value::String(text)) = content {
        if text.starts_with("[Request interrupted by user") {
            pending_tool_ids.clear();
            *last_ending_index = Some(*activity_index);
            *has_after = false;
            *activity_index += 1;
        }
        return;
    }

    let blocks = match content.and_then(|c| c.as_array()) {
        Some(b) => b,
        None => return,
    };

    for b in blocks {
        let bt = b.get("type").and_then(|v| v.as_str()).unwrap_or("");
        match bt {
            "tool_result" => {
                let tool_use_id = b
                    .get("tool_use_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                if tool_use_id.is_empty() {
                    continue;
                }
                pending_tool_ids.remove(&tool_use_id);
                if shutdown_ids.contains(&tool_use_id) || is_rejection {
                    // Ending event.
                    *last_ending_index = Some(*activity_index);
                    *has_after = false;
                    *activity_index += 1;
                } else {
                    // Ongoing activity.
                    *has_any = true;
                    if last_ending_index.is_some() {
                        *has_after = true;
                    }
                    *activity_index += 1;
                }
            }
            "text" => {
                let text = b.get("text").and_then(|v| v.as_str()).unwrap_or("");
                if text.starts_with("[Request interrupted by user") {
                    pending_tool_ids.clear();
                    *last_ending_index = Some(*activity_index);
                    *has_after = false;
                    *activity_index += 1;
                }
            }
            _ => {}
        }
    }
}

const TOOL_USE_REJECTED_MSG: &str = "User rejected tool use";

fn is_tool_use_rejection(raw: &Value) -> bool {
    raw.get("toolUseResult")
        .and_then(|v| v.as_str())
        .map(|s| s == TOOL_USE_REJECTED_MSG)
        .unwrap_or(false)
}
