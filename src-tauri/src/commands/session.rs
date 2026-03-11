use tauri::{AppHandle, State};

use crate::convert::*;
use crate::parser::chunk::build_chunks;
use crate::parser::ongoing::{is_ongoing, ONGOING_STALENESS_THRESHOLD};
use crate::parser::session::{extract_session_meta, read_session_incremental, SessionMeta};
use crate::parser::subagent::{discover_subagents, discover_team_sessions, link_subagents, SubagentProcess};
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
    let chunks = build_chunks(&classified);

    if chunks.is_empty() {
        return Err(format!("session {} has no messages", path));
    }

    // Discover and link subagent execution traces.
    let subagents: Vec<SubagentProcess> = discover_subagents(&path).unwrap_or_default();
    let team_procs = discover_team_sessions(&path, &chunks).unwrap_or_default();
    let mut all_procs: Vec<SubagentProcess> = subagents;
    all_procs.extend(team_procs);
    let color_map = link_subagents(&mut all_procs, &chunks, &path);

    let mut ongoing = is_ongoing(&chunks);
    if !ongoing {
        for proc in &all_procs {
            if is_ongoing(&proc.chunks) {
                ongoing = true;
                break;
            }
        }
    }
    if ongoing {
        if let Ok(info) = std::fs::metadata(&path) {
            if let Ok(modified) = info.modified() {
                let elapsed = std::time::SystemTime::now()
                    .duration_since(modified)
                    .unwrap_or_default();
                if elapsed > ONGOING_STALENESS_THRESHOLD {
                    ongoing = false;
                }
            }
        }
    }

    let teams = reconstruct_teams(&chunks, &all_procs);
    let messages = chunks_to_messages(&chunks, &all_procs, &color_map);
    let meta = extract_session_meta(&path);

    Ok(LoadResult {
        messages,
        teams,
        path: path.clone(),
        ongoing,
        meta,
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
    {
        let mut guard = state.session_watcher.lock().map_err(|e| e.to_string())?;
        if let Some(handle) = guard.take() {
            handle.stop();
        }
    }

    // Read initial state.
    let (classified, new_offset, _) = read_session_incremental(&path, 0)?;

    // Start watcher.
    let handle = start_session_watcher(path, classified, new_offset, app);

    {
        let mut guard = state.session_watcher.lock().map_err(|e| e.to_string())?;
        *guard = Some(handle);
    }

    Ok(())
}

/// Return all project directories under ~/.claude/projects/.
/// Each subdirectory corresponds to an encoded project path.
#[tauri::command]
pub async fn get_project_dirs() -> Result<Vec<String>, String> {
    let home = dirs::home_dir().ok_or("no home directory")?;
    let projects_dir = home.join(".claude").join("projects");
    if !projects_dir.exists() {
        return Ok(Vec::new());
    }
    let mut dirs = Vec::new();
    let entries = std::fs::read_dir(&projects_dir).map_err(|e| e.to_string())?;
    for entry in entries {
        if let Ok(entry) = entry {
            if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                dirs.push(entry.path().to_string_lossy().to_string());
            }
        }
    }
    Ok(dirs)
}

/// Stop watching the current session.
#[tauri::command]
pub async fn unwatch_session(state: State<'_, AppState>) -> Result<(), String> {
    let mut guard = state.session_watcher.lock().map_err(|e| e.to_string())?;
    if let Some(handle) = guard.take() {
        handle.stop();
    }
    Ok(())
}
