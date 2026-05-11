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
/// Non-Stop hooks (PreToolUse, PostToolUse, UserPromptSubmit, SessionStart, PreCompact,
/// etc.) are only written to the debug log (not the JSONL) in Claude Code v2.1.84+. This
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
    // When parentUuid is empty but logicalParentUuid is set (compact_boundary entries),
    // follow logicalParentUuid instead so that pre-compaction messages are included.
    let mut live_set: HashSet<String> = HashSet::new();
    let mut current = live_tip;
    loop {
        if live_set.contains(&current) {
            break; // cycle guard
        }
        live_set.insert(current.clone());
        let parent = match uuid_idx.get(&current).and_then(|&i| entries.get(i)) {
            Some(e) if !e.parent_uuid.is_empty() => e.parent_uuid.clone(),
            Some(e) if !e.logical_parent_uuid.is_empty() => e.logical_parent_uuid.clone(),
            _ => break,
        };
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
        let n = match reader.read_line(&mut line) {
            Ok(n) => n,
            Err(_) => break, // unreadable bytes (e.g. invalid UTF-8) — return what we have
        };
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
    // Track seen (agentName, teamName, summary_text) tuples to deduplicate `summary`
    // entries from pre-v2.1.128 sessions where idle sub-agents fired the same summary
    // repeatedly while the sub-agent's transcript was static.
    let mut seen_summaries: HashSet<(String, String, String)> = HashSet::new();
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
        // Skip duplicate summary entries — keep only the first occurrence of each
        // (agentName, teamName, summary_text) triple.
        if entry.entry_type == "summary" {
            let key = (
                entry.agent_name.clone(),
                entry.team_name.clone(),
                entry.summary.clone(),
            );
            if !seen_summaries.insert(key) {
                continue;
            }
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

    sessions.sort_by_key(|b| std::cmp::Reverse(b.mod_time));
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
    all.sort_by_key(|b| std::cmp::Reverse(b.mod_time));
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

    sessions.sort_by_key(|b| std::cmp::Reverse(b.mod_time));
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
            Err(_) => continue, // unreadable line (e.g. invalid UTF-8) — skip and continue
        };
        lines_read += 1;

        let raw: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let entry_type = raw.get("type").and_then(|v| v.as_str()).unwrap_or("");

        // Entries with `forkedFrom` were inherited from a parent session (v2.1.118+).
        // Detect early so the flag is available for all per-entry decisions below.
        let is_inherited = raw.get("forkedFrom").is_some();

        // Track timestamps for duration. Skip inherited entries so the fork's duration
        // reflects only its own activity, not the parent conversation's timeline.
        if !is_inherited {
            if let Some(ts_str) = raw.get("timestamp").and_then(|v| v.as_str()) {
                let ts = parse_timestamp(ts_str);
                if first_ts.is_none() {
                    first_ts = Some(ts);
                }
                last_ts = Some(ts);
            }
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
        // Skip inherited entries so the turn count reflects the fork's own activity.
        if !is_inherited
            && is_user_chunk_for_turn_count(&raw, entry_type, is_meta_flag, is_sidechain)
        {
            meta.turn_count += 1;
            awaiting_ai_group = true;
        } else if !is_inherited && awaiting_ai_group && entry_type == "assistant" && !is_sidechain {
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
        // Skip inherited entries — their tokens were already counted in the parent session.
        if !is_inherited && entry_type == "assistant" {
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
        // Skip inherited entries — they represent past activity in the parent session.
        if !is_inherited && entry_type == "assistant" && !is_sidechain {
            scan_ongoing_assistant(
                &raw,
                &mut activity_index,
                &mut last_ending_index,
                &mut has_any_ongoing_activity,
                &mut has_activity_after_last_ending,
                &mut shutdown_tool_ids,
                &mut pending_tool_ids,
            );
        } else if !is_inherited && entry_type == "user" {
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
        // Skip inherited entries so first_message reflects the fork's own first prompt.
        if preview_found || lines_read > MAX_PREVIEW_LINES || entry_type != "user" || is_inherited {
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
    // Pending tool calls override — only when no text/ending response was seen after them.
    // If last_ending_index is set, remaining pending IDs appeared before the ending marker;
    // they are orphaned (e.g. from rewound timelines in pre-v2.1.122 /branch fork sessions)
    // rather than genuinely awaiting results, so we must not treat them as ongoing.
    if !meta.is_ongoing && !pending_tool_ids.is_empty() && last_ending_index.is_none() {
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

impl Default for IncrementalTokenScanner {
    fn default() -> Self {
        Self::new()
    }
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

        // Skip inherited entries (v2.1.118+ fork format) — tokens were counted in the parent.
        if raw.get("forkedFrom").is_some() {
            return;
        }

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
        } else if !text.trim().is_empty() {
            // A regular user text message means the conversation continued past any
            // pending tool_uses. Those tool_uses are orphans from a rewound timeline
            // (pre-v2.1.122 /branch fork sessions) — clear them so the session is not
            // falsely marked as ongoing.
            pending_tool_ids.clear();
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

    // If the user entry has continuation content (text, image, or document — not only
    // tool_results), any still-pending tool_use IDs are orphans from a rewound timeline:
    // the conversation moved forward without supplying their results. Pre-v2.1.122 /branch
    // fork sessions can produce this when the source session had rewound timeline entries.
    let has_continuation_content = blocks.iter().any(|b| {
        matches!(
            b.get("type").and_then(|v| v.as_str()).unwrap_or(""),
            "text" | "image" | "document"
        )
    });
    if has_continuation_content {
        pending_tool_ids.clear();
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
        assert!(set.is_empty());
    }

    // --- compact_boundary / logicalParentUuid chain extension ---

    fn make_compact_boundary(uuid: &str, logical_parent_uuid: &str) -> Entry {
        Entry {
            uuid: uuid.to_string(),
            parent_uuid: String::new(), // null → empty
            logical_parent_uuid: logical_parent_uuid.to_string(),
            entry_type: "system".to_string(),
            subtype: "compact_boundary".to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn live_chain_follows_logical_parent_uuid_through_compact_boundary() {
        // Pre-compact: A → B → C (C is the last pre-compact message)
        // compact_boundary: D (parentUuid=null, logicalParentUuid=C)
        // Post-compact: D → E → F (F is the live leaf)
        let entries = vec![
            make_entry("A", "", "", false),
            make_entry("B", "A", "", false),
            make_entry("C", "B", "", false),
            make_compact_boundary("D", "C"),
            make_entry("E", "D", "", false),
            make_entry("F", "E", "", false), // live leaf
        ];
        let set = resolve_live_chain_uuids(&entries);

        // Post-compact chain must be present.
        assert!(set.contains("F"), "live leaf must be in live set");
        assert!(set.contains("E"), "post-compact entry must be in live set");
        assert!(set.contains("D"), "compact_boundary must be in live set");

        // Pre-compact chain must also be present (followed via logicalParentUuid).
        assert!(
            set.contains("C"),
            "last pre-compact entry must be in live set"
        );
        assert!(
            set.contains("B"),
            "mid pre-compact entry must be in live set"
        );
        assert!(
            set.contains("A"),
            "first pre-compact entry must be in live set"
        );
        assert_eq!(set.len(), 6);
    }

    #[test]
    fn live_chain_multiple_compactions_includes_all_pre_compact_messages() {
        // Two compactions: first compacts A→B→C, second compacts post-compact messages.
        // Pre-compact1: A → B → C
        // compact_boundary1: D (logicalParentUuid=C)
        // Post-compact1 / pre-compact2: D → E → F
        // compact_boundary2: G (logicalParentUuid=F)
        // Post-compact2: G → H → I (I is the live leaf)
        let entries = vec![
            make_entry("A", "", "", false),
            make_entry("B", "A", "", false),
            make_entry("C", "B", "", false),
            make_compact_boundary("D", "C"),
            make_entry("E", "D", "", false),
            make_entry("F", "E", "", false),
            make_compact_boundary("G", "F"),
            make_entry("H", "G", "", false),
            make_entry("I", "H", "", false), // live leaf
        ];
        let set = resolve_live_chain_uuids(&entries);
        assert_eq!(
            set.len(),
            9,
            "all entries across both compactions must be in live set"
        );
        for id in &["A", "B", "C", "D", "E", "F", "G", "H", "I"] {
            assert!(set.contains(*id), "{id} must be in live set");
        }
    }

    #[test]
    fn live_chain_compact_boundary_with_dead_end_branch_excluded() {
        // Main chain: A → B → compact_boundary(C, logicalParent=B) → D → E (live)
        // Dead-end branch: B → X (dead-end)
        let entries = vec![
            make_entry("A", "", "", false),
            make_entry("B", "A", "", false),
            make_entry("X", "B", "", false), // dead-end branch
            make_compact_boundary("C", "B"),
            make_entry("D", "C", "", false),
            make_entry("E", "D", "", false), // live leaf
        ];
        let set = resolve_live_chain_uuids(&entries);
        assert!(set.contains("E"), "live leaf must be in live set");
        assert!(set.contains("D"));
        assert!(set.contains("C"), "compact_boundary must be in live set");
        assert!(
            set.contains("B"),
            "pre-compact entry must be in live set via logicalParentUuid"
        );
        assert!(set.contains("A"), "root must be in live set");
        assert!(!set.contains("X"), "dead-end branch must be excluded");
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

    // --- Issue #66: corrupt/truncated JSONL lines from unclean shutdowns ---

    #[test]
    fn read_session_incremental_skips_corrupt_mid_file_line() {
        // A corrupt (invalid JSON but valid UTF-8) line in the middle of the file
        // must be skipped. Valid entries before AND after it must still be parsed.
        let tmp = env::temp_dir().join("tail-test-corrupt-mid-file");
        std::fs::create_dir_all(&tmp).unwrap();
        let path = tmp.join("session.jsonl");

        let u1 = "{\"type\":\"user\",\"uuid\":\"u1\",\"parentUuid\":null,\"isSidechain\":false,\"timestamp\":\"2025-01-15T10:00:00Z\",\"message\":{\"role\":\"user\",\"content\":\"Hello\"}}\n";
        let corrupt = "{truncated-invalid-json-cut-mid-write\n"; // valid UTF-8, invalid JSON
        let u2 = "{\"type\":\"user\",\"uuid\":\"u2\",\"parentUuid\":\"u1\",\"isSidechain\":false,\"timestamp\":\"2025-01-15T10:00:01Z\",\"message\":{\"role\":\"user\",\"content\":\"World\"}}\n";

        std::fs::write(&path, format!("{u1}{corrupt}{u2}")).unwrap();

        let result = read_session_incremental(path.to_str().unwrap(), 0);
        assert!(
            result.is_ok(),
            "corrupt mid-file line must not cause a session load error"
        );

        let (msgs, _, _) = result.unwrap();
        let user_count = msgs
            .iter()
            .filter(|m| matches!(m, ClassifiedMsg::User(_)))
            .count();
        assert_eq!(
            user_count, 2,
            "both valid user entries must be parsed; corrupt line must be skipped"
        );

        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn read_session_incremental_skips_multiple_corrupt_lines() {
        // Multiple corrupt lines scattered throughout the file must all be skipped.
        let tmp = env::temp_dir().join("tail-test-multiple-corrupt");
        std::fs::create_dir_all(&tmp).unwrap();
        let path = tmp.join("session.jsonl");

        let u1 = "{\"type\":\"user\",\"uuid\":\"u1\",\"parentUuid\":null,\"isSidechain\":false,\"timestamp\":\"2025-01-15T10:00:00Z\",\"message\":{\"role\":\"user\",\"content\":\"Hello\"}}\n";
        let corrupt1 = "{\"truncated line 1\n";
        let corrupt2 = "not json at all\n";
        let corrupt3 = "{\"type\":\"assistant\",\"uuid\":\"a1\",\"message\":{\"role\":\"assistan\n"; // cut mid-value
        let u2 = "{\"type\":\"user\",\"uuid\":\"u2\",\"parentUuid\":\"u1\",\"isSidechain\":false,\"timestamp\":\"2025-01-15T10:00:05Z\",\"message\":{\"role\":\"user\",\"content\":\"World\"}}\n";

        std::fs::write(&path, format!("{u1}{corrupt1}{corrupt2}{corrupt3}{u2}")).unwrap();

        let result = read_session_incremental(path.to_str().unwrap(), 0);
        assert!(
            result.is_ok(),
            "multiple corrupt lines must not cause a load error"
        );

        let (msgs, _, _) = result.unwrap();
        let user_count = msgs
            .iter()
            .filter(|m| matches!(m, ClassifiedMsg::User(_)))
            .count();
        assert_eq!(
            user_count, 2,
            "both valid user entries must survive; all corrupt lines must be skipped"
        );

        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn scan_session_metadata_skips_corrupt_lines() {
        // scan_session_metadata must skip corrupt JSON lines and continue processing
        // valid entries that appear after them in the file.
        let tmp = env::temp_dir().join("tail-test-meta-corrupt");
        std::fs::create_dir_all(&tmp).unwrap();
        let path = tmp.join("session.jsonl");

        let valid_user = "{\"type\":\"user\",\"uuid\":\"u1\",\"timestamp\":\"2025-01-15T10:00:00Z\",\"message\":{\"role\":\"user\",\"content\":\"Hello world\"}}\n";
        let corrupt = "{\"type\":\"user\",\"uuid\":\"u2\",\"timestamp\":\"2025-01-15T10:00:01Z\",\"message\":{\"role\":\"user\",\"content\":\"CORRUPT truncated\n"; // truncated
        let valid_assistant = "{\"type\":\"assistant\",\"uuid\":\"a1\",\"requestId\":\"r1\",\"message\":{\"model\":\"claude-sonnet-4-20250514\",\"role\":\"assistant\",\"content\":[],\"usage\":{\"input_tokens\":10,\"output_tokens\":5,\"cache_read_input_tokens\":0,\"cache_creation_input_tokens\":0},\"stop_reason\":\"end_turn\"}}\n";

        std::fs::write(&path, format!("{valid_user}{corrupt}{valid_assistant}")).unwrap();

        let meta = scan_session_metadata(path.to_str().unwrap());
        assert!(
            meta.first_msg.contains("Hello world"),
            "first_msg must come from the valid user entry before the corrupt line; got: {:?}",
            meta.first_msg
        );
        assert_eq!(
            meta.input_tokens, 10,
            "tokens from the valid assistant entry after the corrupt line must be counted (got {})",
            meta.input_tokens
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

    // --- Issue #60: forked session compat (v2.1.118+) ---

    #[test]
    fn forked_session_turn_count_excludes_inherited_entries() {
        // Entries with forkedFrom trigger is_inherited=true → skipped by turn counter.
        // 2 inherited pairs + 1 new pair → turn_count must be 2 (new user + new assistant).
        let tmp = env::temp_dir().join("tail-test-fork-turn-count");
        std::fs::create_dir_all(&tmp).unwrap();
        let path = tmp.join("session.jsonl");

        let inh_u1 = "{\"type\":\"user\",\"uuid\":\"pu1\",\"forkedFrom\":{\"sessionId\":\"p\",\"messageUuid\":\"pu1\"},\"message\":{\"role\":\"user\",\"content\":\"q\"}}\n";
        let inh_a1 = "{\"type\":\"assistant\",\"uuid\":\"pa1\",\"forkedFrom\":{\"sessionId\":\"p\",\"messageUuid\":\"pa1\"},\"message\":{\"role\":\"assistant\",\"content\":[]}}\n";
        let inh_u2 = "{\"type\":\"user\",\"uuid\":\"pu2\",\"forkedFrom\":{\"sessionId\":\"p\",\"messageUuid\":\"pu2\"},\"message\":{\"role\":\"user\",\"content\":\"q\"}}\n";
        let inh_a2 = "{\"type\":\"assistant\",\"uuid\":\"pa2\",\"forkedFrom\":{\"sessionId\":\"p\",\"messageUuid\":\"pa2\"},\"message\":{\"role\":\"assistant\",\"content\":[]}}\n";
        let new_u  = "{\"type\":\"user\",\"uuid\":\"fu1\",\"message\":{\"role\":\"user\",\"content\":\"fork question\"}}\n";
        let new_a  = "{\"type\":\"assistant\",\"uuid\":\"fa1\",\"message\":{\"role\":\"assistant\",\"content\":[]}}\n";
        std::fs::write(
            &path,
            format!("{inh_u1}{inh_a1}{inh_u2}{inh_a2}{new_u}{new_a}"),
        )
        .unwrap();

        let meta = scan_session_metadata(path.to_str().unwrap());
        assert_eq!(
            meta.turn_count, 2,
            "turn_count must only reflect the fork's own turns"
        );

        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn forked_session_first_message_excludes_inherited_entries() {
        // first_msg must come from the fork's own first user entry, not inherited ones.
        let tmp = env::temp_dir().join("tail-test-fork-first-msg");
        std::fs::create_dir_all(&tmp).unwrap();
        let path = tmp.join("session.jsonl");

        let inh_u = "{\"type\":\"user\",\"uuid\":\"pu1\",\"forkedFrom\":{\"sessionId\":\"p\",\"messageUuid\":\"pu1\"},\"message\":{\"role\":\"user\",\"content\":\"Inherited parent question\"}}\n";
        let new_u = "{\"type\":\"user\",\"uuid\":\"fu1\",\"message\":{\"role\":\"user\",\"content\":\"New fork question\"}}\n";
        std::fs::write(&path, format!("{inh_u}{new_u}")).unwrap();

        let meta = scan_session_metadata(path.to_str().unwrap());
        assert!(
            meta.first_msg.contains("New fork question"),
            "first_msg must be the fork's own first message, got: {:?}",
            meta.first_msg
        );
        assert!(
            !meta.first_msg.contains("Inherited"),
            "first_msg must not be from the inherited parent, got: {:?}",
            meta.first_msg
        );

        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn forked_session_tokens_exclude_inherited_entries() {
        // Inherited assistant: 100 in + 50 out; new: 10 in + 5 out → totals must be 15.
        let tmp = env::temp_dir().join("tail-test-fork-tokens");
        std::fs::create_dir_all(&tmp).unwrap();
        let path = tmp.join("session.jsonl");

        let inh_a = "{\"type\":\"assistant\",\"uuid\":\"pa1\",\"requestId\":\"r1\",\"forkedFrom\":{\"sessionId\":\"p\",\"messageUuid\":\"pa1\"},\"message\":{\"usage\":{\"input_tokens\":100,\"output_tokens\":50,\"cache_read_input_tokens\":0,\"cache_creation_input_tokens\":0},\"stop_reason\":\"end_turn\"}}\n";
        let new_a = "{\"type\":\"assistant\",\"uuid\":\"fa1\",\"requestId\":\"r2\",\"message\":{\"usage\":{\"input_tokens\":10,\"output_tokens\":5,\"cache_read_input_tokens\":0,\"cache_creation_input_tokens\":0},\"stop_reason\":\"end_turn\"}}\n";
        std::fs::write(&path, format!("{inh_a}{new_a}")).unwrap();

        let meta = scan_session_metadata(path.to_str().unwrap());
        assert_eq!(
            meta.input_tokens, 10,
            "input_tokens must only count the fork's own entries"
        );
        assert_eq!(
            meta.output_tokens, 5,
            "output_tokens must only count the fork's own entries"
        );
        assert_eq!(
            meta.total_tokens, 15,
            "total_tokens must only count the fork's own entries (not 215)"
        );

        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn forked_session_duration_excludes_inherited_timestamps() {
        // Inherited entry from Jan 2026; new entries 5 s apart in Apr 2026 → duration ≈ 5 s.
        let tmp = env::temp_dir().join("tail-test-fork-duration");
        std::fs::create_dir_all(&tmp).unwrap();
        let path = tmp.join("session.jsonl");

        let inh      = "{\"type\":\"user\",\"uuid\":\"pu1\",\"timestamp\":\"2026-01-01T10:00:00Z\",\"forkedFrom\":{\"sessionId\":\"p\",\"messageUuid\":\"pu1\"},\"message\":{\"role\":\"user\",\"content\":\"q\"}}\n";
        let new_start = "{\"type\":\"user\",\"uuid\":\"fu1\",\"timestamp\":\"2026-04-26T10:00:00Z\",\"message\":{\"role\":\"user\",\"content\":\"q\"}}\n";
        let new_end   = "{\"type\":\"assistant\",\"uuid\":\"fa1\",\"timestamp\":\"2026-04-26T10:00:05Z\",\"message\":{\"role\":\"assistant\",\"content\":[]}}\n";
        std::fs::write(&path, format!("{inh}{new_start}{new_end}")).unwrap();

        let meta = scan_session_metadata(path.to_str().unwrap());
        assert!(
            meta.duration_ms >= 5000,
            "duration_ms must span the fork's own entries (got {} ms)",
            meta.duration_ms
        );
        assert!(
            meta.duration_ms < 60_000,
            "duration_ms must not include inherited timestamps (got {} ms, expected ~5000)",
            meta.duration_ms
        );

        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn forked_session_incremental_scanner_excludes_inherited_tokens() {
        // IncrementalTokenScanner must also skip forkedFrom entries: 200+100 inherited, 20+10 new.
        let tmp = env::temp_dir().join("tail-test-fork-incremental");
        std::fs::create_dir_all(&tmp).unwrap();
        let path = tmp.join("session.jsonl");

        let inh_a = "{\"type\":\"assistant\",\"uuid\":\"pa1\",\"requestId\":\"r1\",\"forkedFrom\":{\"sessionId\":\"p\",\"messageUuid\":\"pa1\"},\"message\":{\"usage\":{\"input_tokens\":200,\"output_tokens\":100,\"cache_read_input_tokens\":0,\"cache_creation_input_tokens\":0},\"stop_reason\":\"end_turn\"}}\n";
        let new_a = "{\"type\":\"assistant\",\"uuid\":\"fa1\",\"requestId\":\"r2\",\"message\":{\"usage\":{\"input_tokens\":20,\"output_tokens\":10,\"cache_read_input_tokens\":0,\"cache_creation_input_tokens\":0},\"stop_reason\":\"end_turn\"}}\n";
        std::fs::write(&path, format!("{inh_a}{new_a}")).unwrap();

        let mut scanner = IncrementalTokenScanner::new();
        let totals = scanner.scan_new_bytes(path.to_str().unwrap());
        assert_eq!(
            totals.input_tokens, 20,
            "IncrementalTokenScanner must skip forkedFrom entries (got {})",
            totals.input_tokens
        );
        assert_eq!(
            totals.output_tokens, 10,
            "IncrementalTokenScanner must skip forkedFrom entries (got {})",
            totals.output_tokens
        );

        std::fs::remove_dir_all(&tmp).ok();
    }

    // --- Issue #67: pre-v2.1.122 /branch fork sessions with orphaned tool_use blocks ---

    #[test]
    fn orphaned_tool_use_before_user_text_does_not_mark_session_ongoing() {
        // Pre-v2.1.122 /branch fork sessions can contain assistant entries with tool_use
        // blocks from rewound timelines — the corresponding tool_result is absent. A
        // subsequent user text entry continues the conversation: those pending tool_use IDs
        // must be cleared so scan_session_metadata does not falsely mark the session ongoing.
        let tmp = env::temp_dir().join("tail-test-issue67-ongoing");
        std::fs::create_dir_all(&tmp).unwrap();
        let path = tmp.join("session.jsonl");

        // Orphaned assistant: tool_use with no matching tool_result anywhere in the file.
        let orphan = "{\"type\":\"assistant\",\"uuid\":\"a1\",\"parentUuid\":null,\"timestamp\":\"2026-04-28T10:00:00Z\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"tool_use\",\"id\":\"toolu_orphan\",\"name\":\"Bash\",\"input\":{\"command\":\"ls\"}}],\"model\":\"claude-sonnet-4-6\",\"stop_reason\":\"tool_use\",\"usage\":{\"input_tokens\":10,\"output_tokens\":5,\"cache_read_input_tokens\":0,\"cache_creation_input_tokens\":0}}}\n";
        // User entry with text continuing the conversation (no tool_result for toolu_orphan).
        let user_text = "{\"type\":\"user\",\"uuid\":\"u1\",\"parentUuid\":\"a1\",\"timestamp\":\"2026-04-28T10:00:01Z\",\"message\":{\"role\":\"user\",\"content\":\"hello\"}}\n";
        // Final assistant with text ending.
        let final_asst = "{\"type\":\"assistant\",\"uuid\":\"a2\",\"parentUuid\":\"u1\",\"timestamp\":\"2026-04-28T10:00:02Z\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"hi\"}],\"model\":\"claude-sonnet-4-6\",\"stop_reason\":\"end_turn\",\"usage\":{\"input_tokens\":20,\"output_tokens\":3,\"cache_read_input_tokens\":0,\"cache_creation_input_tokens\":0}}}\n";

        std::fs::write(&path, format!("{orphan}{user_text}{final_asst}")).unwrap();

        let meta = scan_session_metadata(path.to_str().unwrap());
        assert!(
            !meta.is_ongoing,
            "session with orphaned tool_use before user text must not be marked ongoing (got is_ongoing=true)"
        );

        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn orphaned_tool_use_with_array_content_user_does_not_mark_ongoing() {
        // Same as above but the user entry uses array content instead of a string.
        let tmp = env::temp_dir().join("tail-test-issue67-array");
        std::fs::create_dir_all(&tmp).unwrap();
        let path = tmp.join("session.jsonl");

        let orphan = "{\"type\":\"assistant\",\"uuid\":\"a1\",\"parentUuid\":null,\"timestamp\":\"2026-04-28T10:00:00Z\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"tool_use\",\"id\":\"toolu_orphan2\",\"name\":\"Read\",\"input\":{\"file_path\":\"/tmp/x\"}}],\"model\":\"claude-sonnet-4-6\",\"stop_reason\":\"tool_use\",\"usage\":{\"input_tokens\":10,\"output_tokens\":5,\"cache_read_input_tokens\":0,\"cache_creation_input_tokens\":0}}}\n";
        // User entry with array content containing a text block (new user turn).
        let user_array = "{\"type\":\"user\",\"uuid\":\"u1\",\"parentUuid\":\"a1\",\"timestamp\":\"2026-04-28T10:00:01Z\",\"message\":{\"role\":\"user\",\"content\":[{\"type\":\"text\",\"text\":\"what now?\"}]}}\n";
        let final_asst = "{\"type\":\"assistant\",\"uuid\":\"a2\",\"parentUuid\":\"u1\",\"timestamp\":\"2026-04-28T10:00:02Z\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"here\"}],\"model\":\"claude-sonnet-4-6\",\"stop_reason\":\"end_turn\",\"usage\":{\"input_tokens\":20,\"output_tokens\":3,\"cache_read_input_tokens\":0,\"cache_creation_input_tokens\":0}}}\n";

        std::fs::write(&path, format!("{orphan}{user_array}{final_asst}")).unwrap();

        let meta = scan_session_metadata(path.to_str().unwrap());
        assert!(
            !meta.is_ongoing,
            "session with orphaned tool_use before array-content user entry must not be ongoing"
        );

        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn genuine_deferred_tool_use_still_marks_session_ongoing() {
        // A session that genuinely ends mid-tool-use (no subsequent user message) should
        // still be marked ongoing. The orphan fix must not affect this case.
        let tmp = env::temp_dir().join("tail-test-issue67-deferred");
        std::fs::create_dir_all(&tmp).unwrap();
        let path = tmp.join("session.jsonl");

        // Only an assistant entry with tool_use — no user message follows.
        let asst = "{\"type\":\"assistant\",\"uuid\":\"a1\",\"parentUuid\":null,\"timestamp\":\"2026-04-28T10:00:00Z\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"tool_use\",\"id\":\"toolu_defer\",\"name\":\"Bash\",\"input\":{\"command\":\"sleep 10\"}}],\"model\":\"claude-sonnet-4-6\",\"stop_reason\":\"tool_use\",\"usage\":{\"input_tokens\":10,\"output_tokens\":5,\"cache_read_input_tokens\":0,\"cache_creation_input_tokens\":0}}}\n";

        std::fs::write(&path, format!("{asst}")).unwrap();

        let meta = scan_session_metadata(path.to_str().unwrap());
        assert!(
            meta.is_ongoing,
            "session ending mid-tool-use (no subsequent user message) must remain ongoing"
        );

        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn read_session_incremental_orphaned_tool_use_before_user_is_orphan_in_chunks() {
        // End-to-end: a fork session with an orphaned tool_use followed by a user text message
        // must produce an AI chunk where the tool_use item is is_orphan=true, not is_deferred.
        // Verify the session does not appear ongoing.
        use crate::parser::chunk::build_chunks;
        use crate::parser::ongoing::OngoingChecker;

        let tmp = env::temp_dir().join("tail-test-issue67-e2e");
        std::fs::create_dir_all(&tmp).unwrap();
        let path = tmp.join("session.jsonl");

        let orphan = "{\"type\":\"assistant\",\"uuid\":\"a1\",\"parentUuid\":null,\"isSidechain\":false,\"timestamp\":\"2026-04-28T10:00:00Z\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"tool_use\",\"id\":\"toolu_e2e\",\"name\":\"Bash\",\"input\":{\"command\":\"ls\"}}],\"model\":\"claude-sonnet-4-6\",\"stop_reason\":\"tool_use\",\"usage\":{\"input_tokens\":10,\"output_tokens\":5,\"cache_read_input_tokens\":0,\"cache_creation_input_tokens\":0}}}\n";
        let user_text = "{\"type\":\"user\",\"uuid\":\"u1\",\"parentUuid\":\"a1\",\"isSidechain\":false,\"timestamp\":\"2026-04-28T10:00:01Z\",\"message\":{\"role\":\"user\",\"content\":\"new question\"}}\n";
        let final_asst = "{\"type\":\"assistant\",\"uuid\":\"a2\",\"parentUuid\":\"u1\",\"isSidechain\":false,\"timestamp\":\"2026-04-28T10:00:02Z\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"answer\"}],\"model\":\"claude-sonnet-4-6\",\"stop_reason\":\"end_turn\",\"usage\":{\"input_tokens\":20,\"output_tokens\":5,\"cache_read_input_tokens\":0,\"cache_creation_input_tokens\":0}}}\n";

        std::fs::write(&path, format!("{orphan}{user_text}{final_asst}")).unwrap();

        let (msgs, _, _) = read_session_incremental(path.to_str().unwrap(), 0).unwrap();
        let chunks = build_chunks(&msgs);

        // Find the AI chunk that contains the orphaned tool_use.
        let orphan_chunk = chunks.iter().find(|c| {
            c.chunk_type == crate::parser::chunk::ChunkType::AI
                && c.items
                    .iter()
                    .any(|i| i.item_type == crate::parser::chunk::DisplayItemType::ToolCall)
        });
        assert!(
            orphan_chunk.is_some(),
            "must have an AI chunk with a tool_use item"
        );
        let item = orphan_chunk
            .unwrap()
            .items
            .iter()
            .find(|i| i.item_type == crate::parser::chunk::DisplayItemType::ToolCall)
            .unwrap();
        assert!(
            item.is_orphan,
            "tool_use without result before user message must be is_orphan"
        );
        assert!(!item.is_deferred, "must not be is_deferred");

        assert!(
            !OngoingChecker::is_chunks_ongoing(&chunks),
            "session must not appear ongoing"
        );

        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn forked_session_conversation_view_includes_all_entries() {
        // read_session_incremental must include inherited entries — they provide fork context.
        // Chain pu1→pa1→fu1→fa1; fa1 is the live leaf so the live-chain filter keeps all 4.
        let tmp = env::temp_dir().join("tail-test-fork-conv-view");
        std::fs::create_dir_all(&tmp).unwrap();
        let path = tmp.join("session.jsonl");

        let inh_u = "{\"type\":\"user\",\"uuid\":\"pu1\",\"parentUuid\":null,\"forkedFrom\":{\"sessionId\":\"p\",\"messageUuid\":\"pu1\"},\"message\":{\"role\":\"user\",\"content\":\"Inherited question\"}}\n";
        let inh_a = "{\"type\":\"assistant\",\"uuid\":\"pa1\",\"parentUuid\":\"pu1\",\"forkedFrom\":{\"sessionId\":\"p\",\"messageUuid\":\"pa1\"},\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"inherited answer\"}]}}\n";
        let new_u = "{\"type\":\"user\",\"uuid\":\"fu1\",\"parentUuid\":\"pa1\",\"message\":{\"role\":\"user\",\"content\":\"Fork question\"}}\n";
        let new_a = "{\"type\":\"assistant\",\"uuid\":\"fa1\",\"parentUuid\":\"fu1\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"fork answer\"}]}}\n";
        std::fs::write(&path, format!("{inh_u}{inh_a}{new_u}{new_a}")).unwrap();

        let (msgs, _, _) = read_session_incremental(path.to_str().unwrap(), 0).unwrap();
        let user_count = msgs
            .iter()
            .filter(|m| matches!(m, ClassifiedMsg::User(_)))
            .count();
        let ai_count = msgs
            .iter()
            .filter(|m| matches!(m, ClassifiedMsg::AI(_)))
            .count();
        assert_eq!(
            user_count, 2,
            "both inherited and new user messages must appear"
        );
        assert_eq!(
            ai_count, 2,
            "both inherited and new AI messages must appear"
        );

        std::fs::remove_dir_all(&tmp).ok();
    }

    // --- Issue #67: orphaned tool_use blocks from pre-v2.1.122 /branch fork sessions ---

    #[test]
    fn orphaned_tool_use_before_end_turn_does_not_mark_session_ongoing() {
        // Pre-v2.1.122 /branch forks may contain tool_use blocks from rewound timelines
        // with no matching tool_result in any subsequent user message. If the fork's own
        // conversation ends normally (text response), the session must NOT be marked ongoing.
        let tmp = env::temp_dir().join("tail-test-orphan-tool-use-ongoing");
        std::fs::create_dir_all(&tmp).unwrap();
        let path = tmp.join("session.jsonl");

        // Orphaned assistant entry: tool_use block with no subsequent tool_result.
        let orphan_a = "{\"type\":\"assistant\",\"uuid\":\"a1\",\"parentUuid\":\"u1\",\"timestamp\":\"2026-05-01T10:00:00Z\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"tool_use\",\"id\":\"toolu_orphan\",\"name\":\"Bash\",\"input\":{\"command\":\"ls\"}}],\"model\":\"claude-sonnet-4-20250514\",\"stop_reason\":\"tool_use\",\"usage\":{\"input_tokens\":50,\"output_tokens\":20,\"cache_read_input_tokens\":0,\"cache_creation_input_tokens\":0}}}\n";
        // Fork's own user message (no tool_result for toolu_orphan).
        let new_u = "{\"type\":\"user\",\"uuid\":\"u2\",\"parentUuid\":\"a1\",\"timestamp\":\"2026-05-01T10:00:01Z\",\"message\":{\"role\":\"user\",\"content\":\"What else can you do?\"}}\n";
        // Fork's own assistant response — ends the conversation with a text block.
        let new_a = "{\"type\":\"assistant\",\"uuid\":\"a2\",\"parentUuid\":\"u2\",\"timestamp\":\"2026-05-01T10:00:02Z\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"I can help with many things.\"}],\"model\":\"claude-sonnet-4-20250514\",\"stop_reason\":\"end_turn\",\"usage\":{\"input_tokens\":100,\"output_tokens\":30,\"cache_read_input_tokens\":0,\"cache_creation_input_tokens\":0}}}\n";

        std::fs::write(&path, format!("{orphan_a}{new_u}{new_a}")).unwrap();

        let meta = scan_session_metadata(path.to_str().unwrap());
        assert!(
            !meta.is_ongoing,
            "session with orphaned tool_use before end_turn must not be marked ongoing"
        );

        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn genuine_pending_tool_use_at_session_end_marks_session_ongoing() {
        // A session that genuinely ends mid-tool-call (Claude invoked a tool but the result
        // has not yet been written to the JSONL) must still be marked as ongoing.
        let tmp = env::temp_dir().join("tail-test-genuine-pending-tool-use");
        std::fs::create_dir_all(&tmp).unwrap();
        let path = tmp.join("session.jsonl");

        let asst = "{\"type\":\"assistant\",\"uuid\":\"a1\",\"parentUuid\":\"u1\",\"timestamp\":\"2026-05-01T10:00:00Z\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"tool_use\",\"id\":\"toolu_genuine\",\"name\":\"Bash\",\"input\":{\"command\":\"ls\"}}],\"model\":\"claude-sonnet-4-20250514\",\"stop_reason\":\"tool_use\",\"usage\":{\"input_tokens\":50,\"output_tokens\":20,\"cache_read_input_tokens\":0,\"cache_creation_input_tokens\":0}}}\n";
        // No subsequent user message — session ends with the pending tool call.

        std::fs::write(&path, asst).unwrap();

        let meta = scan_session_metadata(path.to_str().unwrap());
        assert!(
            meta.is_ongoing,
            "session genuinely ending with pending tool_use must be marked ongoing"
        );

        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn orphaned_tool_use_in_conversation_is_rendered_as_orphan() {
        // When an orphaned tool_use block (no matching tool_result) is followed by a user
        // message (conversation continued), build_chunks must mark the DisplayItem as
        // is_orphan=true so the activity log skips it and the session is not shown as ongoing.
        let tmp = env::temp_dir().join("tail-test-orphan-tool-use-orphan");
        std::fs::create_dir_all(&tmp).unwrap();
        let path = tmp.join("session.jsonl");

        let orphan_a = "{\"type\":\"assistant\",\"uuid\":\"a1\",\"parentUuid\":null,\"timestamp\":\"2026-05-01T10:00:00Z\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"tool_use\",\"id\":\"toolu_orphan\",\"name\":\"Bash\",\"input\":{\"command\":\"ls\"}}],\"model\":\"claude-sonnet-4-20250514\",\"stop_reason\":\"tool_use\",\"usage\":{\"input_tokens\":50,\"output_tokens\":20,\"cache_read_input_tokens\":0,\"cache_creation_input_tokens\":0}}}\n";
        let new_u = "{\"type\":\"user\",\"uuid\":\"u2\",\"parentUuid\":\"a1\",\"timestamp\":\"2026-05-01T10:00:01Z\",\"message\":{\"role\":\"user\",\"content\":\"What else can you do?\"}}\n";
        let new_a = "{\"type\":\"assistant\",\"uuid\":\"a2\",\"parentUuid\":\"u2\",\"timestamp\":\"2026-05-01T10:00:02Z\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"I can help.\"}],\"model\":\"claude-sonnet-4-20250514\",\"stop_reason\":\"end_turn\",\"usage\":{\"input_tokens\":100,\"output_tokens\":10,\"cache_read_input_tokens\":0,\"cache_creation_input_tokens\":0}}}\n";

        std::fs::write(&path, format!("{orphan_a}{new_u}{new_a}")).unwrap();

        let chunks = read_session(path.to_str().unwrap()).expect("read_session must succeed");
        let ai_chunk = chunks
            .iter()
            .find(|c| matches!(c.chunk_type, crate::parser::chunk::ChunkType::AI))
            .expect("must have at least one AI chunk");

        let orphan_item = ai_chunk
            .items
            .iter()
            .find(|it| it.tool_name == "Bash")
            .expect("orphaned Bash tool_use must appear in AI chunk items");

        assert!(
            orphan_item.is_orphan,
            "orphaned tool_use before a user message must be marked is_orphan=true"
        );
        assert!(
            !orphan_item.is_deferred,
            "orphaned tool_use before a user message must not be is_deferred"
        );
        assert!(
            orphan_item.tool_result.is_empty(),
            "orphaned tool_use must have empty tool_result"
        );

        std::fs::remove_dir_all(&tmp).ok();
    }

    // --- Issue #78: duplicate summary entries from pre-v2.1.128 sessions ---

    #[test]
    fn duplicate_summary_entries_produce_single_compact_msg() {
        // Pre-v2.1.128: idle sub-agents could write the same summary entry repeatedly.
        // read_session_incremental must emit only one CompactMsg for each unique
        // (agentName, teamName, summary_text) triple.
        let tmp = env::temp_dir().join("tail-test-issue78-dup-summary");
        std::fs::create_dir_all(&tmp).unwrap();
        let path = tmp.join("session.jsonl");

        // Three summary entries with identical agentName and summary text.
        let s1 = "{\"type\":\"summary\",\"uuid\":\"s1\",\"parentUuid\":null,\"agentName\":\"agent1\",\"teamName\":\"\",\"summary\":\"Investigating the bug\",\"timestamp\":\"2026-05-01T10:00:00Z\"}\n";
        let s2 = "{\"type\":\"summary\",\"uuid\":\"s2\",\"parentUuid\":\"s1\",\"agentName\":\"agent1\",\"teamName\":\"\",\"summary\":\"Investigating the bug\",\"timestamp\":\"2026-05-01T10:00:01Z\"}\n";
        let s3 = "{\"type\":\"summary\",\"uuid\":\"s3\",\"parentUuid\":\"s2\",\"agentName\":\"agent1\",\"teamName\":\"\",\"summary\":\"Investigating the bug\",\"timestamp\":\"2026-05-01T10:00:02Z\"}\n";

        std::fs::write(&path, format!("{s1}{s2}{s3}")).unwrap();

        let (msgs, _, _) = read_session_incremental(path.to_str().unwrap(), 0).unwrap();
        let compact_count = msgs
            .iter()
            .filter(|m| matches!(m, ClassifiedMsg::Compact(_)))
            .count();
        assert_eq!(
            compact_count, 1,
            "three identical summary entries must produce exactly one CompactMsg (got {compact_count})"
        );
    }

    #[test]
    fn distinct_summary_entries_are_all_kept() {
        // When summary text changes (sub-agent's state genuinely advances), each unique
        // (agentName, teamName, summary_text) triple must produce its own CompactMsg.
        let tmp = env::temp_dir().join("tail-test-issue78-distinct-summary");
        std::fs::create_dir_all(&tmp).unwrap();
        let path = tmp.join("session.jsonl");

        let s1 = "{\"type\":\"summary\",\"uuid\":\"s1\",\"parentUuid\":null,\"agentName\":\"agent1\",\"teamName\":\"\",\"summary\":\"Starting the task\",\"timestamp\":\"2026-05-01T10:00:00Z\"}\n";
        let s2 = "{\"type\":\"summary\",\"uuid\":\"s2\",\"parentUuid\":\"s1\",\"agentName\":\"agent1\",\"teamName\":\"\",\"summary\":\"Half-way done\",\"timestamp\":\"2026-05-01T10:00:01Z\"}\n";
        let s3 = "{\"type\":\"summary\",\"uuid\":\"s3\",\"parentUuid\":\"s2\",\"agentName\":\"agent1\",\"teamName\":\"\",\"summary\":\"Task complete\",\"timestamp\":\"2026-05-01T10:00:02Z\"}\n";

        std::fs::write(&path, format!("{s1}{s2}{s3}")).unwrap();

        let (msgs, _, _) = read_session_incremental(path.to_str().unwrap(), 0).unwrap();
        let compact_count = msgs
            .iter()
            .filter(|m| matches!(m, ClassifiedMsg::Compact(_)))
            .count();
        assert_eq!(
            compact_count, 3,
            "three distinct summary entries must each produce a CompactMsg (got {compact_count})"
        );
    }

    #[test]
    fn duplicate_summary_entries_from_different_agents_are_kept_separately() {
        // Two agents can independently have the same summary text — their entries must NOT
        // be deduplicated against each other (different agentName → different key).
        let tmp = env::temp_dir().join("tail-test-issue78-multi-agent-summary");
        std::fs::create_dir_all(&tmp).unwrap();
        let path = tmp.join("session.jsonl");

        let s1 = "{\"type\":\"summary\",\"uuid\":\"s1\",\"parentUuid\":null,\"agentName\":\"agent1\",\"teamName\":\"\",\"summary\":\"Working\",\"timestamp\":\"2026-05-01T10:00:00Z\"}\n";
        let s2 = "{\"type\":\"summary\",\"uuid\":\"s2\",\"parentUuid\":\"s1\",\"agentName\":\"agent2\",\"teamName\":\"\",\"summary\":\"Working\",\"timestamp\":\"2026-05-01T10:00:01Z\"}\n";
        // Duplicate of s1 (same agentName + text) — must be deduplicated.
        let s3 = "{\"type\":\"summary\",\"uuid\":\"s3\",\"parentUuid\":\"s2\",\"agentName\":\"agent1\",\"teamName\":\"\",\"summary\":\"Working\",\"timestamp\":\"2026-05-01T10:00:02Z\"}\n";

        std::fs::write(&path, format!("{s1}{s2}{s3}")).unwrap();

        let (msgs, _, _) = read_session_incremental(path.to_str().unwrap(), 0).unwrap();
        let compact_count = msgs
            .iter()
            .filter(|m| matches!(m, ClassifiedMsg::Compact(_)))
            .count();
        assert_eq!(
            compact_count, 2,
            "agent1+agent2 summaries must produce 2 CompactMsgs (s1+s2 unique, s3 dup of s1); got {compact_count}"
        );
    }

    #[test]
    fn summary_dedup_does_not_affect_non_summary_entries() {
        // The deduplication logic must only touch summary entries and leave all other
        // entry types (user, assistant, system) completely unaffected.
        let tmp = env::temp_dir().join("tail-test-issue78-non-summary");
        std::fs::create_dir_all(&tmp).unwrap();
        let path = tmp.join("session.jsonl");

        let user = "{\"type\":\"user\",\"uuid\":\"u1\",\"parentUuid\":null,\"isSidechain\":false,\"timestamp\":\"2026-05-01T10:00:00Z\",\"message\":{\"role\":\"user\",\"content\":\"Hello\"}}\n";
        let summary = "{\"type\":\"summary\",\"uuid\":\"s1\",\"parentUuid\":\"u1\",\"agentName\":\"agent1\",\"teamName\":\"\",\"summary\":\"doing work\",\"timestamp\":\"2026-05-01T10:00:01Z\"}\n";
        let dup_summary = "{\"type\":\"summary\",\"uuid\":\"s2\",\"parentUuid\":\"s1\",\"agentName\":\"agent1\",\"teamName\":\"\",\"summary\":\"doing work\",\"timestamp\":\"2026-05-01T10:00:02Z\"}\n";

        std::fs::write(&path, format!("{user}{summary}{dup_summary}")).unwrap();

        let (msgs, _, _) = read_session_incremental(path.to_str().unwrap(), 0).unwrap();
        let user_count = msgs
            .iter()
            .filter(|m| matches!(m, ClassifiedMsg::User(_)))
            .count();
        let compact_count = msgs
            .iter()
            .filter(|m| matches!(m, ClassifiedMsg::Compact(_)))
            .count();
        assert_eq!(
            user_count, 1,
            "user entry must be unaffected by summary dedup"
        );
        assert_eq!(
            compact_count, 1,
            "duplicate summary must be deduplicated to one CompactMsg"
        );
    }
}

const TOOL_USE_REJECTED_MSG: &str = "User rejected tool use";

fn is_tool_use_rejection(raw: &Value) -> bool {
    raw.get("toolUseResult")
        .and_then(|v| v.as_str())
        .map(|s| s == TOOL_USE_REJECTED_MSG)
        .unwrap_or(false)
}
