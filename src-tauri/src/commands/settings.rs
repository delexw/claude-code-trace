use std::sync::Arc;

use serde::Serialize;
use tauri::State;

use crate::state::AppState;

/// Response for the frontend — always includes both the configured and default paths.
#[derive(Serialize)]
pub struct SettingsResponse {
    /// The user-configured path, or null if using the default.
    pub projects_dir: Option<String>,
    /// The platform default path (e.g. ~/.claude/projects). Always present.
    pub default_dir: String,
    /// The resolved effective path (configured > CLAUDE_PROJECTS_DIR env > default).
    pub effective_dir: String,
    /// Whether the effective directory actually exists on disk.
    pub effective_dir_exists: bool,
}

pub fn platform_default_dir() -> String {
    dirs::home_dir()
        .map(|h| {
            h.join(".claude")
                .join("projects")
                .to_string_lossy()
                .to_string()
        })
        .unwrap_or_default()
}

pub fn build_response_pub(settings: &crate::settings::Settings) -> SettingsResponse {
    let effective = crate::parser::session::claude_projects_dir(settings.projects_dir.as_deref())
        .unwrap_or_else(|_| std::path::PathBuf::from(platform_default_dir()));
    let effective_dir_exists = effective.exists();
    SettingsResponse {
        projects_dir: settings.projects_dir.clone(),
        default_dir: platform_default_dir(),
        effective_dir: effective.to_string_lossy().to_string(),
        effective_dir_exists,
    }
}

#[tauri::command]
pub async fn get_settings(state: State<'_, Arc<AppState>>) -> Result<SettingsResponse, String> {
    let guard = state.settings.lock().map_err(|e| e.to_string())?;
    Ok(build_response_pub(&guard))
}

#[tauri::command]
pub async fn set_projects_dir(
    path: Option<String>,
    state: State<'_, Arc<AppState>>,
) -> Result<SettingsResponse, String> {
    if let Some(ref p) = path {
        let pb = std::path::PathBuf::from(p);
        if !pb.exists() {
            return Err(format!("path does not exist: {p}"));
        }
        if !pb.is_dir() {
            return Err(format!("path is not a directory: {p}"));
        }
    }

    let mut guard = state.settings.lock().map_err(|e| e.to_string())?;
    guard.projects_dir = path;
    crate::settings::save_settings(&guard)?;
    Ok(build_response_pub(&guard))
}
