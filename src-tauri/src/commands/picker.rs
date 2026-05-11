use std::sync::Arc;

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
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<SessionInfo>, String> {
    let mut sessions = state.discover_sessions_cached(&project_dirs)?;
    // The session watcher has the most accurate ongoing detection, so apply
    // its verdict over the picker's lightweight metadata scan.
    state.apply_watched_ongoing(&mut sessions);
    Ok(sessions)
}

/// Start watching project directories for new/changed sessions.
/// Emits "picker-refresh" events.
#[tauri::command]
pub async fn watch_picker(
    project_dirs: Vec<String>,
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    // Stop existing picker watcher if any.
    state.stop_picker_watcher()?;

    let handle = start_picker_watcher(project_dirs, state.inner().clone(), Some(app));
    state.set_picker_watcher(handle)?;

    Ok(())
}

/// Stop watching project directories.
#[tauri::command]
pub async fn unwatch_picker(state: State<'_, Arc<AppState>>) -> Result<(), String> {
    state.stop_picker_watcher()
}
