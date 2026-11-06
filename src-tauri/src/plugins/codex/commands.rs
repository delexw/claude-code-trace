use std::sync::Arc;

use tauri::State;

use crate::state::AppState;

#[tauri::command]
pub async fn list_codex_sessions(
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<super::reader::CodexSession>, String> {
    let settings = state.settings.lock().map_err(|e| e.to_string())?.clone();
    tokio::task::spawn_blocking(move || super::visible_sessions(&settings))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_codex_settings(
    state: State<'_, Arc<AppState>>,
) -> Result<super::CodexSettings, String> {
    let settings = state.settings.lock().map_err(|e| e.to_string())?.clone();
    tokio::task::spawn_blocking(move || super::settings_view(&settings))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn set_codex_root(
    path: Option<String>,
    state: State<'_, Arc<AppState>>,
) -> Result<super::CodexSettings, String> {
    let settings = {
        let mut guard = state.settings.lock().map_err(|e| e.to_string())?;
        super::set_root(&mut guard, path)?;
        crate::settings::save_settings(&guard)?;
        guard.clone()
    };
    tokio::task::spawn_blocking(move || super::settings_view(&settings))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn load_codex_session(
    path: String,
    start: Option<usize>,
    limit: Option<usize>,
) -> Result<super::reader::CodexWindow, String> {
    tokio::task::spawn_blocking(move || super::load_session(&path, start, limit))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn prepare_codex_efficiency_payload(
    path: String,
    payload_mode: Option<crate::efficiency::settings::PayloadMode>,
) -> Result<crate::efficiency::PreparedEfficiencyPayload, String> {
    tokio::task::spawn_blocking(move || super::prepare_efficiency_payload(&path, payload_mode))
        .await
        .map_err(|e| e.to_string())?
}
