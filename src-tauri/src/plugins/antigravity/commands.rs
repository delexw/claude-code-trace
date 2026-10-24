//! Tauri commands for the Antigravity source plugin (desktop build).

use std::sync::Arc;

use tauri::State;

use super::roots::AntigravityBrainFolder;
use super::transcript::AntigravityWindow;
use super::AntigravitySettings;
use crate::state::AppState;

/// The conversations to list, newest first; empty while the plugin is off.
#[tauri::command]
pub async fn list_antigravity_folders(
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<AntigravityBrainFolder>, String> {
    let settings = state.settings.lock().map_err(|e| e.to_string())?.clone();
    Ok(super::visible_folders(&settings))
}

#[tauri::command]
pub async fn get_antigravity_settings(
    state: State<'_, Arc<AppState>>,
) -> Result<AntigravitySettings, String> {
    let guard = state.settings.lock().map_err(|e| e.to_string())?;
    Ok(super::settings_view(&guard))
}

#[tauri::command]
pub async fn set_antigravity_root(
    path: Option<String>,
    state: State<'_, Arc<AppState>>,
) -> Result<AntigravitySettings, String> {
    let mut guard = state.settings.lock().map_err(|e| e.to_string())?;
    super::set_root(&mut guard, path)?;
    crate::settings::save_settings(&guard)?;
    Ok(super::settings_view(&guard))
}

#[tauri::command]
pub async fn load_antigravity_session(
    path: String,
    start: Option<usize>,
    limit: Option<usize>,
) -> Result<AntigravityWindow, String> {
    super::load_session(&path, start, limit)
}
