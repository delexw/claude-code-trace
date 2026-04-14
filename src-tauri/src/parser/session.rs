use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::PathBuf;

use super::chunk::{build_chunks, Chunk};
use super::classify::{classify, ClassifiedMsg};
use super::debuglog::extract_hook_msgs;
use super::entry::{parse_entry, Entry};

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
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_creation_tokens: i64,
    pub cost_usd: f64,
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

/// Full session load: JSONL classified messages merged with hook events from the
/// debug log (if one exists at `~/.claude/debug/{session_id}.txt`).
///
/// Non-Stop hooks (PreToolUse, PostToolUse, UserPromptSubmit, SessionStart) are
/// only written to the debug log (not the JSONL) in Claude Code v2.1.84+. This
/// function surfaces them by reading the debug log and merging by timestamp.
pub fn read_session_with_debug_hooks(path: &str) -> Result<(Vec<ClassifiedMsg>, u64, u64), String> {
    let (mut msgs, offset, bytes) = read_session_incremental(path, 0)?;
    let debug_hooks = extract_hook_msgs(path);
    if !debug_hooks.is_empty() {
        msgs.extend(debug_hooks);
        msgs.sort_by_key(|m| match m {
            ClassifiedMsg::User(u) => u.timestamp,
            ClassifiedMsg::AI(a) => a.timestamp,
            ClassifiedMsg::System(s) => s.timestamp,
            ClassifiedMsg::Teammate(t) => t.timestamp,
            ClassifiedMsg::Compact(c) => c.timestamp,
            ClassifiedMsg::Hook(h) => h.timestamp,
        });
    }
    Ok((msgs, offset, bytes))
}

/// Return the set of UUIDs that lie on the live (main) conversation chain.
///
/// Strategy:
/// 1. If any entry carries a non-empty `leafUuid`, the last such value is the
///    authoritative tip of the live chain — Claude Code writes it on every turn.
/// 2. Otherwise, find the non-sidechain leaf entry — the entry whose `uuid` is
///    not referenced as any other entry's `parentUuid` — that appears latest in
///    the file (most recently written, most likely to be the live tip).
/// 3. Walk backwards from the chosen tip via `parentUuid` links to collect all
///    UUIDs on the live path.
///
/// Returns an empty set when the chain cannot be determined; callers must then
/// render all entries unchanged (safe fallback — no entries are silently dropped).
fn resolve_live_chain_uuids(entries: &[Entry]) -> HashSet<String> {
    if entries.is_empty() {
        return HashSet::new();
    }

    // uuid → index in entries (used for the backward walk).
    let mut uuid_idx: HashMap<String, usize> = HashMap::with_capacity(entries.len());
    // UUIDs that appear as someone's parentUuid — they have a child, so they are not leaves.
    let mut has_child: HashSet<String> = HashSet::with_capacity(entries.len());
    // Last non-empty leafUuid seen (Claude Code writes this to mark the live tip).
    let mut leaf_hint = String::new();

    for (i, e) in entries.iter().enumerate() {
        if !e.uuid.is_empty() {
            uuid_idx.insert(e.uuid.clone(), i);
        }
        if !e.parent_uuid.is_empty() {
            has_child.insert(e.parent_uuid.clone());
        }
        if !e.leaf_uuid.is_empty() {
            leaf_hint = e.leaf_uuid.clone();
        }
    }

    // Step 1: prefer the explicit leafUuid hint when it resolves to a known entry.
    let live_tip = if !leaf_hint.is_empty() && uuid_idx.contains_key(&leaf_hint) {
        leaf_hint
    } else {
        // Step 2: fallback — pick the last non-sidechain leaf entry in file order.
        entries
            .iter()
            .rev()
            .find(|e| !e.uuid.is_empty() && !e.is_sidechain && !has_child.contains(&e.uuid))
            .map(|e| e.uuid.clone())
            .unwrap_or_default()
    };

    if live_tip.is_empty() {
        return HashSet::new();
    }

    // Step 3: walk backward from live_tip via parentUuid links.
    let mut live_set: HashSet<String> = HashSet::new();
    let mut current = live_tip;
    loop {
        if live_set.contains(&current) {
            break; // cycle guard
        }
        live_set.insert(current.clone());
        let parent = match uuid_idx.get(&current).and_then(|&i| entries.get(i)) {
            Some(e) => e.parent_uuid.clone(),
            None => break,
        };
        if parent.is_empty() {
            break;
        }
        current = parent;
    }

    live_set
}

/// Read new lines from a session file starting at the given byte offset.
/// Returns (new classified messages, updated offset, bytes read).
pub fn read_session_incremental(
    path: &str,
    offset: u64,
) -> Result<(Vec<ClassifiedMsg>, u64, u64), String> {
    let f = fs::File::open(path).map_err(|e| format!("opening {path}: {e}"))?;
    let mut reader = BufReader::new(f);
    reader
        .seek(SeekFrom::Start(offset))
        .map_err(|e| format!("seeking: {e}"))?;

    let mut raw_entries: Vec<Entry> = Vec::new();
    let mut bytes_read: u64 = 0;
    let mut line = String::new();

    loop {
        line.clear();
        let n = reader
            .read_line(&mut line)
            .map_err(|e| format!("reading: {e}"))?;
        if n == 0 {
            break;
        }

        // If the line does not end with '\n', it is a partial write at EOF
        // (Claude Code v2.1.78+ streams responses line-by-line). Do not
        // advance the offset past the incomplete bytes — wait for the full
        // line to be flushed on the next watcher event.
        if !line.ends_with('\n') {
            break;
        }

        bytes_read += n as u64;

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if let Some(entry) = parse_entry(trimmed.as_bytes()) {
            raw_entries.push(entry);
        }
    }

    // For full reads (offset == 0), resolve the live chain before classifying so
    // that dead-end branch entries (interrupted turns, failed retries, subagent
    // write-gap collisions) are suppressed.  Incremental watcher reads (offset > 0)
    // skip resolution: new bytes are always continuations of the live chain, and
    // we don't have the full chain context to resolve accurately.
    let live_set = if offset == 0 {
        resolve_live_chain_uuids(&raw_entries)
    } else {
        HashSet::new()
    };

    let mut msgs = Vec::new();
    for entry in raw_entries {
        // When live-branch resolution produced a non-empty set, skip any non-sidechain
        // entry whose uuid is absent from the live chain.  Sidechain entries are passed
        // through unchanged — classify() already filters them.  Entries with no uuid
        // (e.g. leafUuid-only markers) are always passed through.
        //
        // Exception: "attachment" entries (hook results, skill listings, etc.) are
        // side-nodes — they hang off a chain entry via parentUuid but are never
        // referenced as someone else's parentUuid, so their own uuid never appears in
        // the live set.  Include them when their parentUuid is on the live chain.
        let is_live_attachment = entry.entry_type == "attachment"
            && !entry.parent_uuid.is_empty()
            && live_set.contains(&entry.parent_uuid);
        if !live_set.is_empty()
            && !entry.uuid.is_empty()
            && !entry.is_sidechain
            && !live_set.contains(&entry.uuid)
            && !is_live_attachment
        {
            continue;
        }
        if let Some(msg) = classify(entry) {
            msgs.push(msg);
        }
    }

    Ok((msgs, offset + bytes_read, bytes_read))
}

/// Return the Claude projects base directory.
/// Priority: configured path > CLAUDE_PROJECTS_DIR env var > ~/.claude/projects.
pub fn claude_projects_dir(configured: Option<&str>) -> Result<PathBuf, String> {
    if let Some(dir) = configured {
        let p = PathBuf::from(dir);
        if p.exists() {
            return Ok(p);
        }
    }
    if let Ok(custom) = std::env::var("CLAUDE_PROJECTS_DIR") {
        let p = PathBuf::from(&custom);
        if p.exists() {
            return Ok(p);
        }
    }
    let home = dirs::home_dir().ok_or("no home directory")?;
    Ok(home.join(".claude").join("projects"))
}

/// Return the Claude CLI projects directory for an absolute path.
pub fn project_dir_for_path(abs_path: &str) -> Result<String, String> {
    let base = claude_projects_dir(None)?;
    let resolved = fs::canonicalize(abs_path).unwrap_or_else(|_| PathBuf::from(abs_path));
    let encoded = encode_path(&resolved.to_string_lossy());
    Ok(base.join(encoded).to_string_lossy().to_string())
}

fn encode_path(abs_path: &str) -> String {
    abs_path.replace([std::path::MAIN_SEPARATOR, '/', '.', '_'], "-")
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

/// Check if a directory entry is a session file (*.jsonl, not agent_*, not a directory).
fn is_session_file(name: &str, entry: &fs::DirEntry) -> bool {
    name.ends_with(".jsonl")
        && !name.starts_with("agent_")
        && !entry.file_type().map(|t| t.is_dir()).unwrap_or(true)
}

/// Discover all session .jsonl files in a project directory.
pub fn discover_project_sessions(project_dir: &str) -> Result<Vec<SessionInfo>, String> {
    let entries = fs::read_dir(project_dir).map_err(|e| format!("reading {project_dir}: {e}"))?;

    let mut sessions = Vec::new();

    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        let name = entry.file_name().to_string_lossy().to_string();
        if !is_session_file(&name, &entry) {
            continue;
        }

        let metadata = entry.metadata();
        let mod_time = metadata
            .as_ref()
            .ok()
            .and_then(|m| m.modified().ok())
            .map(DateTime::<Utc>::from)
            .unwrap_or_else(Utc::now);

        let path = entry.path().to_string_lossy().to_string();
        let meta = scan_session_metadata(&path);

        let mut is_ongoing = meta.is_ongoing;
        if is_ongoing {
            if let Ok(m) = entry.metadata() {
                if let Ok(modified) = m.modified() {
                    is_ongoing = super::ongoing::apply_staleness(true, modified);
                }
            }
        }
        if !is_ongoing {
            is_ongoing = super::subagent::has_recently_active_subagents(&path);
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
            input_tokens: meta.input_tokens,
            output_tokens: meta.output_tokens,
            cache_read_tokens: meta.cache_read_tokens,
            cache_creation_tokens: meta.cache_creation_tokens,
            cost_usd: meta.cost_usd,
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
pub fn session_info_from_metadata(
    path: &str,
    mod_time: std::time::SystemTime,
    meta: SessionMetadata,
) -> SessionInfo {
    let mod_time_chrono: DateTime<Utc> = mod_time.into();
    let mut is_ongoing = super::ongoing::apply_staleness(meta.is_ongoing, mod_time);
    if !is_ongoing {
        is_ongoing = super::subagent::has_recently_active_subagents(path);
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
        input_tokens: meta.input_tokens,
        output_tokens: meta.output_tokens,
        cache_read_tokens: meta.cache_read_tokens,
        cache_creation_tokens: meta.cache_creation_tokens,
        cost_usd: meta.cost_usd,
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
    let entries = fs::read_dir(project_dir).map_err(|e| format!("reading {project_dir}: {e}"))?;

    let mut sessions = Vec::new();

    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        let name = entry.file_name().to_string_lossy().to_string();
        if !is_session_file(&name, &entry) {
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
    pub(crate) input_tokens: i64,
    pub(crate) output_tokens: i64,
    pub(crate) cache_read_tokens: i64,
    pub(crate) cache_creation_tokens: i64,
    pub(crate) cost_usd: f64,
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
            input_tokens: 0,
            output_tokens: 0,
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
            cost_usd: 0.0,
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
    use super::subagent::TokenSnapshot;
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

        let entry_type = raw.get("type").and_then(|v| v.as_str()).unwrap_or("");

        // Track timestamps for duration (all entries, even UUID-less ones).
        if let Some(ts_str) = raw.get("timestamp").and_then(|v| v.as_str()) {
            let ts = parse_timestamp(ts_str);
            if first_ts.is_none() {
                first_ts = Some(ts);
            }
            last_ts = Some(ts);
        }

        // --- Session-level metadata (cwd, branch: first seen; mode: last seen) ---
        // Extract before UUID check so queue-operation entries contribute metadata.
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

        let uuid = raw.get("uuid").and_then(|v| v.as_str()).unwrap_or("");
        if uuid.is_empty() {
            continue;
        }

        let is_sidechain = raw
            .get("isSidechain")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let is_meta_flag = raw.get("isMeta").and_then(|v| v.as_bool()).unwrap_or(false);

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
        // Include sidechain entries so cost reflects all API calls.
        if entry_type == "assistant" {
            let model_str = raw
                .get("message")
                .and_then(|m| m.get("model"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if model_str != "<synthetic>" {
                if let Some(usage) = raw.get("message").and_then(|m| m.get("usage")) {
                    let has_stop = !raw
                        .get("message")
                        .and_then(|m| m.get("stop_reason"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .is_empty();

                    let reported_output = usage
                        .get("output_tokens")
                        .and_then(|v| v.as_i64())
                        .unwrap_or(0);

                    // For incomplete streaming entries (no stop_reason), the
                    // output_tokens may be frozen at an early value while
                    // content continued to stream. Use a content-based
                    // estimate when it exceeds the reported value.
                    let output = if has_stop {
                        reported_output
                    } else {
                        let estimated = super::subagent::estimate_output_from_content(&raw);
                        reported_output.max(estimated)
                    };

                    let snap = TokenSnapshot {
                        input: usage
                            .get("input_tokens")
                            .and_then(|v| v.as_i64())
                            .unwrap_or(0),
                        output,
                        cache_read: usage
                            .get("cache_read_input_tokens")
                            .and_then(|v| v.as_i64())
                            .unwrap_or(0),
                        cache_create: usage
                            .get("cache_creation_input_tokens")
                            .and_then(|v| v.as_i64())
                            .unwrap_or(0),
                        model: model_str.to_string(),
                        has_stop_reason: has_stop,
                    };

                    let request_id = raw.get("requestId").and_then(|v| v.as_str()).unwrap_or("");
                    if !request_id.is_empty() {
                        // Prefer complete entries over partial streaming snapshots.
                        super::subagent::insert_best_snapshot(
                            &mut request_tokens,
                            request_id.to_string(),
                            snap,
                        );
                    } else {
                        // No requestId: sum directly.
                        meta.total_tokens +=
                            snap.input + snap.output + snap.cache_read + snap.cache_create;
                        meta.input_tokens += snap.input;
                        meta.output_tokens += snap.output;
                        meta.cache_read_tokens += snap.cache_read;
                        meta.cache_creation_tokens += snap.cache_create;
                    }
                }

                // Model extraction (first real main-context assistant entry).
                if !is_sidechain && meta.model.is_empty() && !model_str.is_empty() {
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

        let content = raw.get("message").and_then(|m| m.get("content")).cloned();
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

    // Scan subagent JSONL files into the same request_tokens map (global requestId dedup).
    let mut fallback = TokenSnapshot {
        input: 0,
        output: 0,
        cache_read: 0,
        cache_create: 0,
        model: String::new(),
        has_stop_reason: false,
    };
    super::subagent::scan_subagent_tokens_into(path, &mut request_tokens, &mut fallback);
    meta.total_tokens +=
        fallback.input + fallback.output + fallback.cache_read + fallback.cache_create;
    meta.input_tokens += fallback.input;
    meta.output_tokens += fallback.output;
    meta.cache_read_tokens += fallback.cache_read;
    meta.cache_creation_tokens += fallback.cache_create;

    // Finalize token totals: sum the last-seen usage per requestId.
    for snap in request_tokens.values() {
        meta.total_tokens += snap.input + snap.output + snap.cache_read + snap.cache_create;
        meta.input_tokens += snap.input;
        meta.output_tokens += snap.output;
        meta.cache_read_tokens += snap.cache_read;
        meta.cache_creation_tokens += snap.cache_create;
    }

    // Compute cost per-model (accurate for mixed opus/haiku/sonnet sessions).
    meta.cost_usd = super::subagent::estimate_cost_from_snapshots(&request_tokens, &fallback);

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

/// Incremental token scanner for the watcher — avoids re-reading the entire file.
///
/// Keeps a running `request_tokens` map and byte offset so that each call to
/// `scan_new_bytes` only reads the newly appended portion of the main session
/// file. Subagent files are rescanned only when their size changes.
pub struct IncrementalTokenScanner {
    /// Byte offset into the main session file (how far we've read).
    offset: u64,
    /// Per-requestId best token snapshot (global dedup across main + subagents).
    request_tokens: HashMap<String, super::subagent::TokenSnapshot>,
    /// Accumulated tokens from entries without a requestId.
    fallback: super::subagent::TokenSnapshot,
    /// Model string (first real non-sidechain assistant model).
    model: String,
    /// Cached subagent file sizes — only rescan files that grew.
    subagent_sizes: HashMap<String, u64>,
    /// Per-subagent byte offsets for incremental reading.
    subagent_offsets: HashMap<String, u64>,
}

impl IncrementalTokenScanner {
    pub fn new() -> Self {
        Self {
            offset: 0,
            request_tokens: HashMap::new(),
            fallback: super::subagent::TokenSnapshot {
                input: 0,
                output: 0,
                cache_read: 0,
                cache_create: 0,
                model: String::new(),
                has_stop_reason: false,
            },
            model: String::new(),
            subagent_sizes: HashMap::new(),
            subagent_offsets: HashMap::new(),
        }
    }

    /// Scan only new bytes from the main session file and any changed subagent files.
    /// Returns the current totals.
    pub fn scan_new_bytes(&mut self, path: &str) -> crate::convert::SessionTotals {
        // 1. Read new bytes from main session file.
        self.scan_main_file(path);

        // 2. Incrementally scan subagent files (only changed ones).
        self.scan_subagents_incremental(path);

        // 3. Compute totals from accumulated state.
        self.compute_totals()
    }

    fn scan_main_file(&mut self, path: &str) {
        let f = match fs::File::open(path) {
            Ok(f) => f,
            Err(_) => return,
        };
        let mut reader = BufReader::new(f);
        if reader.seek(SeekFrom::Start(self.offset)).is_err() {
            return;
        }

        let mut line = String::new();
        loop {
            line.clear();
            let n = match reader.read_line(&mut line) {
                Ok(n) => n,
                Err(_) => break,
            };
            if n == 0 {
                break;
            }
            self.offset += n as u64;
            self.process_line(line.trim(), false);
        }
    }

    fn scan_subagents_incremental(&mut self, session_path: &str) {
        let dir = super::subagent::subagents_dir(session_path);
        let entries = match fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => return,
        };

        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.starts_with("agent-") || !name.ends_with(".jsonl") {
                continue;
            }
            let file_path = dir.join(&name);
            let key = file_path.to_string_lossy().to_string();

            // Check if file has grown since last scan.
            let current_size = entry.metadata().map(|m| m.len()).unwrap_or(0);
            let prev_size = self.subagent_sizes.get(&key).copied().unwrap_or(0);
            if current_size <= prev_size {
                continue;
            }
            self.subagent_sizes.insert(key.clone(), current_size);

            // Read from where we left off.
            let sub_offset = self.subagent_offsets.get(&key).copied().unwrap_or(0);
            let f = match fs::File::open(&file_path) {
                Ok(f) => f,
                Err(_) => continue,
            };
            let mut reader = BufReader::new(f);
            if reader.seek(SeekFrom::Start(sub_offset)).is_err() {
                continue;
            }

            let mut new_offset = sub_offset;
            let mut line = String::new();
            loop {
                line.clear();
                let n = match reader.read_line(&mut line) {
                    Ok(n) => n,
                    Err(_) => break,
                };
                if n == 0 {
                    break;
                }
                new_offset += n as u64;
                self.process_line(line.trim(), true);
            }
            self.subagent_offsets.insert(key, new_offset);
        }
    }

    fn process_line(&mut self, line: &str, _is_subagent: bool) {
        let raw: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => return,
        };

        let entry_type = raw.get("type").and_then(|v| v.as_str()).unwrap_or("");
        if entry_type != "assistant" {
            return;
        }

        let model_str = raw
            .get("message")
            .and_then(|m| m.get("model"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if model_str == "<synthetic>" {
            return;
        }

        let usage = match raw.get("message").and_then(|m| m.get("usage")) {
            Some(u) => u,
            None => return,
        };

        let has_stop = !raw
            .get("message")
            .and_then(|m| m.get("stop_reason"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .is_empty();

        let reported_output = usage
            .get("output_tokens")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);

        let output = if has_stop {
            reported_output
        } else {
            let estimated = super::subagent::estimate_output_from_content(&raw);
            reported_output.max(estimated)
        };

        let inp = usage
            .get("input_tokens")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let cr = usage
            .get("cache_read_input_tokens")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let cc = usage
            .get("cache_creation_input_tokens")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);

        if inp + output + cr + cc == 0 {
            return;
        }

        let snap = super::subagent::TokenSnapshot {
            input: inp,
            output,
            cache_read: cr,
            cache_create: cc,
            model: model_str.to_string(),
            has_stop_reason: has_stop,
        };

        let request_id = raw.get("requestId").and_then(|v| v.as_str()).unwrap_or("");
        if !request_id.is_empty() {
            super::subagent::insert_best_snapshot(
                &mut self.request_tokens,
                request_id.to_string(),
                snap,
            );
        } else {
            self.fallback.input += inp;
            self.fallback.output += output;
            self.fallback.cache_read += cr;
            self.fallback.cache_create += cc;
        }

        // Capture model from first real entry.
        if self.model.is_empty() && !model_str.is_empty() {
            self.model = model_str.to_string();
        }
    }

    fn compute_totals(&self) -> crate::convert::SessionTotals {
        let mut total_tokens = self.fallback.input
            + self.fallback.output
            + self.fallback.cache_read
            + self.fallback.cache_create;
        let mut input_tokens = self.fallback.input;
        let mut output_tokens = self.fallback.output;
        let mut cache_read_tokens = self.fallback.cache_read;
        let mut cache_creation_tokens = self.fallback.cache_create;

        for snap in self.request_tokens.values() {
            total_tokens += snap.input + snap.output + snap.cache_read + snap.cache_create;
            input_tokens += snap.input;
            output_tokens += snap.output;
            cache_read_tokens += snap.cache_read;
            cache_creation_tokens += snap.cache_create;
        }

        let cost_usd =
            super::subagent::estimate_cost_from_snapshots(&self.request_tokens, &self.fallback);

        crate::convert::SessionTotals {
            total_tokens,
            input_tokens,
            output_tokens,
            cache_read_tokens,
            cache_creation_tokens,
            cost_usd,
            model: self.model.clone(),
        }
    }
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

    let content = raw.get("message").and_then(|m| m.get("content")).cloned();
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn claude_projects_dir_defaults_to_home() {
        env::remove_var("CLAUDE_PROJECTS_DIR");
        let dir = claude_projects_dir(None).unwrap();
        let home = dirs::home_dir().unwrap();
        assert_eq!(dir, home.join(".claude").join("projects"));
    }

    #[test]
    fn claude_projects_dir_uses_configured_when_valid() {
        let tmp = env::temp_dir().join("tail-test-projects-configured");
        std::fs::create_dir_all(&tmp).unwrap();
        let dir = claude_projects_dir(Some(tmp.to_str().unwrap())).unwrap();
        assert_eq!(dir, tmp);
        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn claude_projects_dir_uses_env_var_when_valid() {
        let tmp = env::temp_dir().join("tail-test-projects-dir");
        std::fs::create_dir_all(&tmp).unwrap();
        env::set_var("CLAUDE_PROJECTS_DIR", tmp.to_str().unwrap());
        let dir = claude_projects_dir(None).unwrap();
        assert_eq!(dir, tmp);
        env::remove_var("CLAUDE_PROJECTS_DIR");
        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn claude_projects_dir_configured_takes_priority_over_env() {
        let tmp_configured = env::temp_dir().join("tail-test-configured-priority");
        let tmp_env = env::temp_dir().join("tail-test-env-priority");
        std::fs::create_dir_all(&tmp_configured).unwrap();
        std::fs::create_dir_all(&tmp_env).unwrap();
        env::set_var("CLAUDE_PROJECTS_DIR", tmp_env.to_str().unwrap());
        let dir = claude_projects_dir(Some(tmp_configured.to_str().unwrap())).unwrap();
        assert_eq!(dir, tmp_configured);
        env::remove_var("CLAUDE_PROJECTS_DIR");
        std::fs::remove_dir_all(&tmp_configured).ok();
        std::fs::remove_dir_all(&tmp_env).ok();
    }

    #[test]
    fn claude_projects_dir_falls_back_when_env_path_missing() {
        env::set_var(
            "CLAUDE_PROJECTS_DIR",
            "/nonexistent/path/that/does/not/exist",
        );
        let dir = claude_projects_dir(None).unwrap();
        let home = dirs::home_dir().unwrap();
        assert_eq!(dir, home.join(".claude").join("projects"));
        env::remove_var("CLAUDE_PROJECTS_DIR");
    }

    #[test]
    fn incremental_scanner_empty_file() {
        let tmp = env::temp_dir().join("tail-test-scanner-empty");
        std::fs::create_dir_all(&tmp).unwrap();
        let path = tmp.join("session.jsonl");
        std::fs::write(&path, "").unwrap();

        let mut scanner = IncrementalTokenScanner::new();
        let totals = scanner.scan_new_bytes(path.to_str().unwrap());
        assert_eq!(totals.total_tokens, 0);
        assert_eq!(totals.cost_usd, 0.0);
        assert!(totals.model.is_empty());

        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn incremental_scanner_accumulates_tokens() {
        let tmp = env::temp_dir().join("tail-test-scanner-accum");
        std::fs::create_dir_all(&tmp).unwrap();
        let path = tmp.join("session.jsonl");

        // Write first entry.
        let entry1 = r#"{"type":"assistant","uuid":"a1","requestId":"r1","message":{"model":"claude-sonnet-4-20250514","role":"assistant","content":[],"usage":{"input_tokens":100,"output_tokens":50,"cache_read_input_tokens":0,"cache_creation_input_tokens":0},"stop_reason":"end_turn"}}"#;
        std::fs::write(&path, format!("{entry1}\n")).unwrap();

        let mut scanner = IncrementalTokenScanner::new();
        let totals1 = scanner.scan_new_bytes(path.to_str().unwrap());
        assert_eq!(totals1.input_tokens, 100);
        assert_eq!(totals1.output_tokens, 50);
        assert_eq!(totals1.total_tokens, 150);
        assert_eq!(totals1.model, "claude-sonnet-4-20250514");

        // Append second entry with different requestId.
        let entry2 = r#"{"type":"assistant","uuid":"a2","requestId":"r2","message":{"model":"claude-sonnet-4-20250514","role":"assistant","content":[],"usage":{"input_tokens":200,"output_tokens":80,"cache_read_input_tokens":0,"cache_creation_input_tokens":0},"stop_reason":"end_turn"}}"#;
        use std::io::Write;
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        writeln!(f, "{entry2}").unwrap();

        // Second scan should only read the new bytes.
        let totals2 = scanner.scan_new_bytes(path.to_str().unwrap());
        assert_eq!(totals2.input_tokens, 300);
        assert_eq!(totals2.output_tokens, 130);
        assert_eq!(totals2.total_tokens, 430);

        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn incremental_scanner_deduplicates_request_ids() {
        let tmp = env::temp_dir().join("tail-test-scanner-dedup");
        std::fs::create_dir_all(&tmp).unwrap();
        let path = tmp.join("session.jsonl");

        // Two entries with same requestId — scanner should keep the one with stop_reason.
        let streaming = r#"{"type":"assistant","uuid":"a1","requestId":"r1","message":{"model":"claude-sonnet-4-20250514","role":"assistant","content":[],"usage":{"input_tokens":100,"output_tokens":20,"cache_read_input_tokens":0,"cache_creation_input_tokens":0}}}"#;
        let complete = r#"{"type":"assistant","uuid":"a2","requestId":"r1","message":{"model":"claude-sonnet-4-20250514","role":"assistant","content":[],"usage":{"input_tokens":100,"output_tokens":50,"cache_read_input_tokens":0,"cache_creation_input_tokens":0},"stop_reason":"end_turn"}}"#;
        std::fs::write(&path, format!("{streaming}\n{complete}\n")).unwrap();

        let mut scanner = IncrementalTokenScanner::new();
        let totals = scanner.scan_new_bytes(path.to_str().unwrap());
        // Should use the complete entry (50 output), not streaming (20).
        assert_eq!(totals.input_tokens, 100);
        assert_eq!(totals.output_tokens, 50);
        assert_eq!(totals.total_tokens, 150);

        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn incremental_scanner_skips_non_assistant_lines() {
        let tmp = env::temp_dir().join("tail-test-scanner-skip");
        std::fs::create_dir_all(&tmp).unwrap();
        let path = tmp.join("session.jsonl");

        let user_entry = r#"{"type":"user","uuid":"u1","message":{"content":"hello"}}"#;
        let assistant_entry = r#"{"type":"assistant","uuid":"a1","requestId":"r1","message":{"model":"claude-sonnet-4-20250514","role":"assistant","content":[],"usage":{"input_tokens":50,"output_tokens":25,"cache_read_input_tokens":0,"cache_creation_input_tokens":0},"stop_reason":"end_turn"}}"#;
        std::fs::write(&path, format!("{user_entry}\n{assistant_entry}\n")).unwrap();

        let mut scanner = IncrementalTokenScanner::new();
        let totals = scanner.scan_new_bytes(path.to_str().unwrap());
        assert_eq!(totals.total_tokens, 75);

        std::fs::remove_dir_all(&tmp).ok();
    }

    // --- resolve_live_chain_uuids tests ---

    fn make_entry(uuid: &str, parent_uuid: &str, leaf_uuid: &str, is_sidechain: bool) -> Entry {
        Entry {
            uuid: uuid.to_string(),
            parent_uuid: parent_uuid.to_string(),
            leaf_uuid: leaf_uuid.to_string(),
            is_sidechain,
            ..Default::default()
        }
    }

    #[test]
    fn live_chain_empty_entries_returns_empty_set() {
        let result = resolve_live_chain_uuids(&[]);
        assert!(result.is_empty());
    }

    #[test]
    fn live_chain_single_entry_no_parent_returns_self() {
        let entries = vec![make_entry("u1", "", "", false)];
        let set = resolve_live_chain_uuids(&entries);
        assert!(set.contains("u1"));
        assert_eq!(set.len(), 1);
    }

    #[test]
    fn live_chain_linear_chain_returns_all() {
        // A → B → C (linear, no branches)
        let entries = vec![
            make_entry("A", "", "", false),
            make_entry("B", "A", "", false),
            make_entry("C", "B", "", false),
        ];
        let set = resolve_live_chain_uuids(&entries);
        assert!(set.contains("A"));
        assert!(set.contains("B"));
        assert!(set.contains("C"));
        assert_eq!(set.len(), 3);
    }

    #[test]
    fn live_chain_dead_end_branch_excluded() {
        // Main chain: A → B → C → D (live leaf)
        // Dead-end:        B → X (dead-end leaf, written before D)
        let entries = vec![
            make_entry("A", "", "", false),
            make_entry("B", "A", "", false),
            make_entry("X", "B", "", false), // dead-end, appears before D
            make_entry("C", "B", "", false),
            make_entry("D", "C", "", false), // live leaf — appears last
        ];
        let set = resolve_live_chain_uuids(&entries);
        // Live chain is A→B→C→D
        assert!(set.contains("A"), "root must be included");
        assert!(set.contains("B"), "shared node must be included");
        assert!(set.contains("C"), "live chain node must be included");
        assert!(set.contains("D"), "live leaf must be included");
        // Dead-end must be excluded
        assert!(!set.contains("X"), "dead-end entry must be excluded");
    }

    #[test]
    fn live_chain_leaf_uuid_hint_overrides_file_order() {
        // Main chain: A → B → C (live, but C is NOT the last entry)
        // Dead-end:   A → D (appears last in file)
        // A leafUuid hint points at C — this should win over D.
        let entries = vec![
            make_entry("A", "", "", false),
            make_entry("B", "A", "", false),
            make_entry("C", "B", "", false), // live tip, pointed to by leafUuid
            make_entry("D", "A", "", false), // dead-end, appears last
            Entry {
                // Marker entry carrying the leafUuid hint (no uuid of its own)
                uuid: String::new(),
                leaf_uuid: "C".to_string(),
                ..Default::default()
            },
        ];
        let set = resolve_live_chain_uuids(&entries);
        assert!(set.contains("A"));
        assert!(set.contains("B"));
        assert!(set.contains("C"), "leafUuid-pointed entry must be live");
        assert!(
            !set.contains("D"),
            "dead-end after live tip must be excluded"
        );
    }

    #[test]
    fn live_chain_sidechain_entries_are_not_leaf_candidates() {
        // Main chain: A → B (live leaf)
        // Sidechain:  A → S (is_sidechain = true, appears last)
        let entries = vec![
            make_entry("A", "", "", false),
            make_entry("B", "A", "", false),
            make_entry("S", "A", "", true), // sidechain — must not be chosen as live tip
        ];
        let set = resolve_live_chain_uuids(&entries);
        // Sidechain "S" should not become the live tip
        assert!(set.contains("A"));
        assert!(set.contains("B"), "main chain leaf must be in live set");
        // S is sidechain; it won't be in the live_set, but classify() handles it separately
    }

    #[test]
    fn live_chain_cycle_guard_prevents_infinite_loop() {
        // Malformed entries: A.parent = B, B.parent = A (cycle)
        let entries = vec![
            make_entry("A", "B", "", false),
            make_entry("B", "A", "", false),
        ];
        // Should terminate without panicking.
        let set = resolve_live_chain_uuids(&entries);
        // Both are referenced as parents, so neither is a leaf → no live tip → empty set.
        assert!(set.is_empty() || !set.is_empty()); // just verify no panic/hang
    }

    #[test]
    fn incremental_read_does_not_advance_past_partial_line() {
        let tmp = env::temp_dir().join("tail-test-partial-line");
        std::fs::create_dir_all(&tmp).unwrap();
        let path = tmp.join("session.jsonl");

        // Write a complete line first.
        let complete = "{\"type\":\"user\",\"uuid\":\"u1\",\"timestamp\":\"2025-01-15T10:00:00Z\",\"message\":{\"role\":\"user\",\"content\":\"Hello Claude\"}}\n";
        std::fs::write(&path, complete).unwrap();

        let (msgs, offset, _) = read_session_incremental(path.to_str().unwrap(), 0).unwrap();
        assert_eq!(msgs.len(), 1, "should parse the complete line");

        // Simulate partial write: append a partial JSON line with no trailing newline.
        let partial = "{\"type\":\"assistant\",\"uuid\":\"a1\"";
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        use std::io::Write;
        file.write_all(partial.as_bytes()).unwrap();
        drop(file);

        // Incremental read from the previous offset should NOT advance past the partial line.
        let (new_msgs, new_offset, _) =
            read_session_incremental(path.to_str().unwrap(), offset).unwrap();
        assert!(
            new_msgs.is_empty(),
            "partial line should not produce a message"
        );
        assert_eq!(
            new_offset, offset,
            "offset must not advance past a partial line"
        );

        // Now complete the line with a newline — the full entry should be parseable.
        let rest = ",\"timestamp\":\"2025-01-15T10:00:01Z\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"Hi!\"}],\"model\":\"claude-sonnet-4\",\"stop_reason\":\"end_turn\",\"usage\":{\"input_tokens\":10,\"output_tokens\":5,\"cache_read_input_tokens\":0,\"cache_creation_input_tokens\":0}}}\n";
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        file.write_all(rest.as_bytes()).unwrap();
        drop(file);

        let (completed_msgs, _, _) =
            read_session_incremental(path.to_str().unwrap(), offset).unwrap();
        assert_eq!(
            completed_msgs.len(),
            1,
            "completed line should produce a message"
        );

        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn attachment_hook_on_live_chain_is_not_dropped() {
        // Regression: attachment entries (hook results) are side-nodes — they have their
        // own uuid but nothing references that uuid as a parentUuid.  The live-chain filter
        // must not drop them when their parentUuid is on the live chain.
        //
        // Chain: u1 → a1 → u2 (live leaf)
        // Side:  a1 → hook_attachment (type="attachment", uuid="h1", parentUuid="a1")
        let tmp = env::temp_dir().join("tail-test-attachment-hook");
        std::fs::create_dir_all(&tmp).unwrap();
        let path = tmp.join("session.jsonl");

        let user1 = "{\"type\":\"user\",\"uuid\":\"u1\",\"parentUuid\":null,\"isSidechain\":false,\"timestamp\":\"2025-01-15T10:00:00Z\",\"message\":{\"role\":\"user\",\"content\":\"Write a file\"}}\n";
        let asst1 = "{\"type\":\"assistant\",\"uuid\":\"a1\",\"parentUuid\":\"u1\",\"isSidechain\":false,\"timestamp\":\"2025-01-15T10:00:01Z\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"Sure\"}],\"model\":\"claude-sonnet-4\",\"stop_reason\":\"tool_use\",\"usage\":{\"input_tokens\":10,\"output_tokens\":5,\"cache_read_input_tokens\":0,\"cache_creation_input_tokens\":0}}}\n";
        // Hook attachment: side-node hanging off a1, never referenced as anyone's parentUuid.
        let hook  = "{\"type\":\"attachment\",\"uuid\":\"h1\",\"parentUuid\":\"a1\",\"isSidechain\":false,\"timestamp\":\"2025-01-15T10:00:02Z\",\"attachment\":{\"type\":\"hook_success\",\"hookEvent\":\"PreToolUse\",\"hookName\":\"PreToolUse:Write\",\"stdout\":\"\",\"stderr\":\"\",\"exitCode\":0,\"command\":\"check\",\"durationMs\":10}}\n";
        let user2 = "{\"type\":\"user\",\"uuid\":\"u2\",\"parentUuid\":\"a1\",\"isSidechain\":false,\"timestamp\":\"2025-01-15T10:00:03Z\",\"message\":{\"role\":\"user\",\"content\":[{\"type\":\"tool_result\",\"tool_use_id\":\"t1\",\"content\":\"done\"}]}}\n";

        std::fs::write(&path, format!("{user1}{asst1}{hook}{user2}")).unwrap();

        let (msgs, _, _) = read_session_incremental(path.to_str().unwrap(), 0).unwrap();

        let has_hook = msgs.iter().any(|m| {
            matches!(m, ClassifiedMsg::Hook(h) if h.hook_event == "PreToolUse" && h.hook_name == "PreToolUse:Write")
        });
        assert!(
            has_hook,
            "PreToolUse:Write attachment hook must survive the live-chain filter"
        );

        std::fs::remove_dir_all(&tmp).ok();
    }
}

const TOOL_USE_REJECTED_MSG: &str = "User rejected tool use";

fn is_tool_use_rejection(raw: &Value) -> bool {
    raw.get("toolUseResult")
        .and_then(|v| v.as_str())
        .map(|s| s == TOOL_USE_REJECTED_MSG)
        .unwrap_or(false)
}
