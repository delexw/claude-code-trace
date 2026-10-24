use std::sync::Arc;

use tauri::{AppHandle, State};

use crate::convert::*;
use crate::parser::session::{extract_session_meta, SessionMeta};
use crate::session_load::{LoadOptions, MessageRange};
use crate::state::AppState;
use crate::watcher::start_session_watcher;

/// Load a session file and return display messages.
///
/// `start`/`limit` request a windowed slice for virtualized clients; omit both
/// (the default) to load the whole session. The returned `count` is always the
/// total message count so the client can size the list.
#[tauri::command]
pub async fn load_session(
    path: String,
    start: Option<usize>,
    limit: Option<usize>,
    state: State<'_, Arc<AppState>>,
) -> Result<LoadResult, String> {
    if path.is_empty() {
        return Err("no session path provided".to_string());
    }

    let opts = LoadOptions::window(MessageRange {
        start: start.unwrap_or(0),
        limit,
    });
    let result = state.load_session_windowed(&path, opts)?;

    // Set initial ongoing status so the picker has it immediately.
    state.set_watched_ongoing(path.clone(), result.ongoing);

    Ok(result)
}

/// Return the full (heavy-body) message at `index` for the detail view. List
/// windows carry lightened messages (no tool bodies), so the detail view fetches
/// the full message on demand.
#[tauri::command]
pub async fn load_message(
    path: String,
    index: usize,
    state: State<'_, Arc<AppState>>,
) -> Result<Option<DisplayMessage>, String> {
    if path.is_empty() {
        return Err("no session path provided".to_string());
    }
    state.full_message_at(&path, index)
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
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    // Stop existing watcher if any.
    state.stop_session_watcher()?;

    // Start watcher.
    let handle = start_session_watcher(path, state.inner().clone(), Some(app));
    state.set_session_watcher(handle)?;

    Ok(())
}

/// Return all project directories to scan for sessions: every subdirectory of
/// the Claude projects base directory plus the projects of each configured WSL
/// distro. Each entry corresponds to an encoded project path.
#[tauri::command]
pub async fn get_project_dirs(state: State<'_, Arc<AppState>>) -> Result<Vec<String>, String> {
    let (configured, wsl_distros) = {
        let guard = state.settings.lock().map_err(|e| e.to_string())?;
        (guard.projects_dir.clone(), guard.wsl_distros.clone())
    };
    Ok(crate::wsl::collect_project_dirs(
        configured.as_deref(),
        &wsl_distros,
    ))
}

/// Stop watching the current session.
#[tauri::command]
pub async fn unwatch_session(state: State<'_, Arc<AppState>>) -> Result<(), String> {
    state.clear_watched_ongoing();
    state.clear_session_build_cache();
    state.stop_session_watcher()
}
