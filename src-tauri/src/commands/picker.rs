use tauri::{AppHandle, State};

use crate::parser::session::SessionInfo;
use crate::state::AppState;
use crate::watcher::start_picker_watcher;

/// Discover sessions across project directories.
/// Uses the session cache for efficient rescanning.
/// Returns a flat array of SessionInfo sorted by mod_time descending.
#[tauri::command]
pub async fn discover_sessions(
    project_dirs: Vec<String>,
    state: State<'_, AppState>,
) -> Result<Vec<SessionInfo>, String> {
    let cache = state.session_cache.lock().map_err(|e| e.to_string())?;
    cache.discover_all_project_sessions(&project_dirs)
}

/// Start watching project directories for new/changed sessions.
/// Emits "picker-refresh" events.
#[tauri::command]
pub async fn watch_picker(
    project_dirs: Vec<String>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    // Stop existing picker watcher if any.
    {
        let mut guard = state.picker_watcher.lock().map_err(|e| e.to_string())?;
        if let Some(handle) = guard.take() {
            handle.stop();
        }
    }

    let handle = start_picker_watcher(project_dirs, app);

    {
        let mut guard = state.picker_watcher.lock().map_err(|e| e.to_string())?;
        *guard = Some(handle);
    }

    Ok(())
}

/// Stop watching project directories.
#[tauri::command]
pub async fn unwatch_picker(state: State<'_, AppState>) -> Result<(), String> {
    let mut guard = state.picker_watcher.lock().map_err(|e| e.to_string())?;
    if let Some(handle) = guard.take() {
        handle.stop();
    }
    Ok(())
}
