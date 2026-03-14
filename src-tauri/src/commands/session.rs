use tauri::{AppHandle, State};

use crate::convert::*;
use crate::parser::chunk::build_chunks;
use crate::parser::ongoing::OngoingChecker;
use crate::parser::session::{extract_session_meta, read_session_incremental, SessionMeta};
use crate::parser::subagent::{discover_and_link_all, inject_orphan_subagents};
use crate::parser::team::reconstruct_teams;
use crate::state::AppState;
use crate::watcher::start_session_watcher;

/// Load a session file and return display messages.
#[tauri::command]
pub async fn load_session(path: String) -> Result<LoadResult, String> {
    if path.is_empty() {
        return Err("no session path provided".to_string());
    }

    let (classified, _new_offset, _) = read_session_incremental(&path, 0)?;
    let mut chunks = build_chunks(&classified);

    // Discover and link subagent execution traces.
    let (mut all_procs, color_map) = discover_and_link_all(&path, &chunks);

    // Inject orphan subagents (no parent tool_use in main session yet).
    inject_orphan_subagents(&mut chunks, &mut all_procs);

    let ongoing = OngoingChecker::new(&chunks, &all_procs, &path).is_ongoing();

    let teams = reconstruct_teams(&chunks, &all_procs);
    let messages = chunks_to_messages(&chunks, &all_procs, &color_map);
    let meta = extract_session_meta(&path);

    // Scan main session + subagent files with global requestId dedup.
    let scanned = crate::parser::session::scan_session_metadata(&path);
    let session_totals = SessionTotals {
        total_tokens: scanned.total_tokens,
        input_tokens: scanned.input_tokens,
        output_tokens: scanned.output_tokens,
        cache_read_tokens: scanned.cache_read_tokens,
        cache_creation_tokens: scanned.cache_creation_tokens,
        cost_usd: scanned.cost_usd,
        model: scanned.model,
    };

    Ok(LoadResult {
        messages,
        teams,
        path: path.clone(),
        ongoing,
        meta,
        session_totals,
    })
}

/// Get session metadata without loading the full session.
#[tauri::command]
pub async fn get_session_meta(path: String) -> Result<SessionMeta, String> {
    if path.is_empty() {
        return Err("no session path provided".to_string());
    }
    Ok(extract_session_meta(&path))
}

/// Start watching a session file. Emits "session-update" events.
#[tauri::command]
pub async fn watch_session(
    path: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    // Stop existing watcher if any.
    state.stop_session_watcher()?;

    // Read initial state.
    let (classified, new_offset, _) = read_session_incremental(&path, 0)?;

    // Start watcher.
    let handle = start_session_watcher(path, classified, new_offset, app);
    state.set_session_watcher(handle)?;

    Ok(())
}

/// Return all project directories under the Claude projects base directory.
/// Each subdirectory corresponds to an encoded project path.
#[tauri::command]
pub async fn get_project_dirs(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    let configured = state
        .settings
        .lock()
        .map_err(|e| e.to_string())?
        .projects_dir
        .clone();
    let projects_dir = crate::parser::session::claude_projects_dir(configured.as_deref())?;
    if !projects_dir.exists() {
        return Ok(Vec::new());
    }
    let mut dirs = Vec::new();
    let entries = std::fs::read_dir(&projects_dir).map_err(|e| e.to_string())?;
    for entry in entries.flatten() {
        if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            dirs.push(entry.path().to_string_lossy().to_string());
        }
    }
    Ok(dirs)
}

/// Stop watching the current session.
#[tauri::command]
pub async fn unwatch_session(state: State<'_, AppState>) -> Result<(), String> {
    state.stop_session_watcher()
}
