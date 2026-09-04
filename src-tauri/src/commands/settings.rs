use serde::Serialize;

#[cfg(feature = "desktop")]
use std::sync::Arc;
#[cfg(feature = "desktop")]
use tauri::State;

#[cfg(feature = "desktop")]
use crate::state::AppState;

/// Response for the frontend — always includes both the configured and default paths.
#[derive(Serialize, Debug)]
pub struct SettingsResponse {
    /// The user-configured path, or null if using the default.
    pub projects_dir: Option<String>,
    /// The platform default path (e.g. ~/.claude/projects). Always present.
    pub default_dir: String,
    /// The resolved effective path (configured > CLAUDE_PROJECTS_DIR env > default).
    pub effective_dir: String,
    /// Whether the effective directory actually exists on disk.
    pub effective_dir_exists: bool,
    /// WSL distributions whose projects are also discovered.
    pub wsl_distros: Vec<String>,
    /// Extra CORS origins allowed to call the local HTTP API, configured via
    /// the Settings UI. Unioned at request time with the built-in defaults
    /// and `CCTRACE_ALLOWED_ORIGINS` (see `http_api::build_cors`).
    pub allowed_origins: Vec<String>,
    /// Whether this backend can focus a session's terminal window (see
    /// `commands::terminal::can_focus`). Drives the frontend's Focus button —
    /// true for any local + macOS backend, whether the frontend is the Tauri
    /// app or a browser talking to the HTTP API.
    pub can_focus: bool,
    /// Whether the HTTP API requires the shared token (false only under
    /// `CCTRACE_API_AUTH=off`). See `crate::auth`.
    pub api_auth_enabled: bool,
    /// Where the live token comes from: `"file"` (rotatable from Settings),
    /// `"env"` (`CCTRACE_API_TOKEN`, read-only), `"ephemeral"` (the token file
    /// was unusable at startup; one-off token, read-only), or `"disabled"`.
    pub api_token_source: &'static str,
    /// The token accepted clients must present, or null when disabled. Only
    /// ever returned to a caller that already authenticated (or over IPC).
    pub api_token: Option<String>,
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

pub fn build_response_pub(
    settings: &crate::settings::Settings,
    auth: &crate::auth::ApiAuth,
) -> SettingsResponse {
    let effective = crate::parser::session::claude_projects_dir(settings.projects_dir.as_deref())
        .unwrap_or_else(|_| std::path::PathBuf::from(platform_default_dir()));
    let effective_dir_exists = effective.exists();
    SettingsResponse {
        projects_dir: settings.projects_dir.clone(),
        default_dir: platform_default_dir(),
        effective_dir: effective.to_string_lossy().to_string(),
        effective_dir_exists,
        wsl_distros: settings.wsl_distros.clone(),
        allowed_origins: settings.allowed_origins.clone(),
        can_focus: crate::commands::terminal::can_focus(),
        api_auth_enabled: auth.is_enabled(),
        api_token_source: auth.source(),
        api_token: auth.token().map(str::to_owned),
    }
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub async fn get_settings(state: State<'_, Arc<AppState>>) -> Result<SettingsResponse, String> {
    let guard = state.settings.lock().map_err(|e| e.to_string())?;
    Ok(build_response_pub(&guard, &state.api_auth_snapshot()))
}

#[cfg(feature = "desktop")]
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
    Ok(build_response_pub(&guard, &state.api_auth_snapshot()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_response_serializes_can_focus() {
        let response = build_response_pub(
            &crate::settings::Settings::default(),
            &crate::auth::ApiAuth::Disabled,
        );
        let json = serde_json::to_value(&response).expect("serializes");
        assert_eq!(
            json.get("can_focus").and_then(|v| v.as_bool()),
            Some(crate::commands::terminal::can_focus())
        );
    }

    #[test]
    fn settings_response_reports_file_token() {
        let response = build_response_pub(
            &crate::settings::Settings::default(),
            &crate::auth::ApiAuth::File("abc".into()),
        );
        let json = serde_json::to_value(&response).expect("serializes");
        assert_eq!(json["api_auth_enabled"], true);
        assert_eq!(json["api_token_source"], "file");
        assert_eq!(json["api_token"], "abc");
    }

    #[test]
    fn settings_response_reports_disabled_auth_with_null_token() {
        let response = build_response_pub(
            &crate::settings::Settings::default(),
            &crate::auth::ApiAuth::Disabled,
        );
        let json = serde_json::to_value(&response).expect("serializes");
        assert_eq!(json["api_auth_enabled"], false);
        assert_eq!(json["api_token_source"], "disabled");
        assert!(json["api_token"].is_null());
    }

    #[test]
    fn settings_response_reports_env_token_source() {
        let response = build_response_pub(
            &crate::settings::Settings::default(),
            &crate::auth::ApiAuth::Env("e".into()),
        );
        let json = serde_json::to_value(&response).expect("serializes");
        assert_eq!(json["api_token_source"], "env");
        assert_eq!(json["api_token"], "e");
    }
}
