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
    /// Whether the HTTP API requires a client credential (false only under
    /// `CCTRACE_API_AUTH=off`). See `crate::auth`.
    pub api_auth_enabled: bool,
    /// Where the signing key comes from: `"file"` (persisted config dir),
    /// `"ephemeral"` (config dir unusable at startup — credentials minted this
    /// run die with the process), or `"disabled"`.
    pub api_auth_source: &'static str,
    /// The accepted clients (never their credentials). See `crate::clients`.
    pub clients: Vec<crate::clients::Client>,
    /// Whether the picker also lists Antigravity (Gemini IDE) brain folders
    /// as read-only envelopes. Off by default — see `Settings.enable_antigravity`.
    pub enable_antigravity: bool,
    /// Resolved Antigravity brain root (override > `$GEMINI_HOME` > `~/.gemini`
    /// for the `antigravity-ide` flavor). Empty when no home is known.
    pub antigravity_brain_root: String,
    /// Whether the resolved Antigravity brain root exists on disk. The toggle
    /// in Settings can still be enabled without this being true — it just
    /// means the picker will surface "no folders" until the user installs
    /// Antigravity.
    pub antigravity_brain_root_exists: bool,
    /// The user's explicit brain-root override, verbatim. `None` when flavor
    /// auto-detection is in force. Distinct from `antigravity_brain_root`
    /// above, which is the *resolved* root — this is what the Settings field
    /// is seeded with and edited to.
    pub antigravity_root: Option<String>,
    /// Every flavor root the resolver probed, newest first, with whether it
    /// exists. `antigravity_brain_root` above is simply the first entry — the
    /// rest let the Settings UI say "also found" instead of silently ignoring
    /// an install the user knows about.
    pub antigravity_brain_roots: Vec<crate::commands::antigravity::BrainRoot>,
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
    auth: &crate::auth::AuthMode,
    clients: Vec<crate::clients::Client>,
) -> SettingsResponse {
    let effective = crate::parser::session::claude_projects_dir(settings.projects_dir.as_deref())
        .unwrap_or_else(|_| std::path::PathBuf::from(platform_default_dir()));
    let effective_dir_exists = effective.exists();
    // The Antigravity root is read-only metadata — letting the toggle be
    // turned on even when the root does not exist (Antigravity not installed
    // yet) is intentional, so we never gate on `exists` here.
    //
    // Resolution lives in `commands::antigravity` so the picker, the HTTP API
    // and this response can never disagree about which directory is read.
    let antigravity_roots = crate::commands::antigravity::resolve_roots(settings);
    let (antigravity_brain_root, antigravity_brain_root_exists) = antigravity_roots
        .first()
        .map(|r| (r.path.clone(), r.exists))
        .unwrap_or_default();
    SettingsResponse {
        projects_dir: settings.projects_dir.clone(),
        default_dir: platform_default_dir(),
        effective_dir: effective.to_string_lossy().to_string(),
        effective_dir_exists,
        wsl_distros: settings.wsl_distros.clone(),
        allowed_origins: settings.allowed_origins.clone(),
        can_focus: crate::commands::terminal::can_focus(),
        api_auth_enabled: auth.is_enabled(),
        api_auth_source: auth.source(),
        clients,
        enable_antigravity: settings.enable_antigravity,
        antigravity_brain_root,
        antigravity_brain_root_exists,
        antigravity_root: settings.antigravity_root.clone(),
        antigravity_brain_roots: antigravity_roots,
    }
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub async fn get_settings(state: State<'_, Arc<AppState>>) -> Result<SettingsResponse, String> {
    let guard = state.settings.lock().map_err(|e| e.to_string())?;
    Ok(build_response_pub(
        &guard,
        &state.auth_snapshot(),
        state.clients_snapshot(),
    ))
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
    Ok(build_response_pub(
        &guard,
        &state.auth_snapshot(),
        state.clients_snapshot(),
    ))
}

/// Points the Antigravity brain reader at an explicit directory, or clears
/// the override so flavor auto-detection resumes. Mirrors `set_projects_dir`
/// — the override is a brain root, not a `.gemini` base, and it replaces
/// flavor probing entirely.
///
/// Rejects a path that does not exist so a typo surfaces as an error instead
/// of silently emptying the picker. Pass `None` (or an empty string) to go
/// back to auto-detection.
#[cfg(feature = "desktop")]
#[tauri::command]
pub async fn set_antigravity_root(
    path: Option<String>,
    state: State<'_, Arc<AppState>>,
) -> Result<SettingsResponse, String> {
    if let Some(ref p) = path.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        let pb = std::path::PathBuf::from(p);
        if !pb.exists() {
            return Err(format!("path does not exist: {p}"));
        }
        if !pb.is_dir() {
            return Err(format!("path is not a directory: {p}"));
        }
    }

    let mut guard = state.settings.lock().map_err(|e| e.to_string())?;
    guard.antigravity_root = path
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned);
    crate::settings::save_settings(&guard)?;
    Ok(build_response_pub(
        &guard,
        &state.auth_snapshot(),
        state.clients_snapshot(),
    ))
}

/// Toggles the Antigravity (Gemini IDE) brain-folder discovery. Persists to
/// `settings.json`. When turned off the picker hides every Antigravity
/// envelope, even if folders exist on disk. Does not validate the brain
/// root — turning it on for a system without Antigravity installed simply
/// results in an empty picker section.
#[cfg(feature = "desktop")]
#[tauri::command]
pub async fn set_antigravity_enabled(
    enabled: bool,
    state: State<'_, Arc<AppState>>,
) -> Result<SettingsResponse, String> {
    let mut guard = state.settings.lock().map_err(|e| e.to_string())?;
    guard.enable_antigravity = enabled;
    crate::settings::save_settings(&guard)?;
    Ok(build_response_pub(
        &guard,
        &state.auth_snapshot(),
        state.clients_snapshot(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_response_serializes_can_focus() {
        let response = build_response_pub(
            &crate::settings::Settings::default(),
            &crate::auth::AuthMode::Disabled,
            vec![],
        );
        let json = serde_json::to_value(&response).expect("serializes");
        assert_eq!(
            json.get("can_focus").and_then(|v| v.as_bool()),
            Some(crate::commands::terminal::can_focus())
        );
    }

    #[test]
    fn settings_response_reports_antigravity_root_from_os_home() {
        // On a host with a discoverable home, the resolved brain root
        // points at `<home>/.gemini/antigravity-ide/brain`.
        let response = build_response_pub(
            &crate::settings::Settings::default(),
            &crate::auth::AuthMode::Disabled,
            vec![],
        );
        assert!(!response.enable_antigravity);
        if let Some(home) = dirs::home_dir() {
            // The first candidate is always the newest flavor, which is what
            // the response reports as the active root.
            let expected = home
                .join(".gemini")
                .join(crate::commands::antigravity::ANTIGRAVITY_FLAVORS[0])
                .join("brain");
            assert_eq!(
                response.antigravity_brain_root,
                expected.to_string_lossy().to_string()
            );
        } else {
            // No home → empty resolved path.
            assert!(response.antigravity_brain_root.is_empty());
        }
    }

    #[test]
    fn settings_response_reports_antigravity_root_exists_flag() {
        // The `exists` flag is just a snapshot of `path.exists()` — we only
        // verify the type, not the value (depends on host install state).
        let response = build_response_pub(
            &crate::settings::Settings::default(),
            &crate::auth::AuthMode::Disabled,
            vec![],
        );
        let json = serde_json::to_value(&response).expect("serializes");
        assert!(json.get("antigravity_brain_root_exists").is_some());
        assert_eq!(
            json.get("enable_antigravity").and_then(|v| v.as_bool()),
            Some(false)
        );
    }

    #[test]
    fn settings_response_reports_auth_mode_and_clients_without_credentials() {
        let mut reg = crate::clients::ClientRegistry::default();
        reg.ensure_builtins(1);
        let response = build_response_pub(
            &crate::settings::Settings::default(),
            &crate::auth::AuthMode::Enabled {
                key: b"k".to_vec(),
                source: crate::auth::KeySource::File,
            },
            reg.clients.clone(),
        );
        let json = serde_json::to_value(&response).expect("serializes");
        assert_eq!(json["api_auth_enabled"], true);
        assert_eq!(json["api_auth_source"], "file");
        assert_eq!(json["clients"].as_array().unwrap().len(), 2);
        assert_eq!(json["clients"][0]["name"], "web-ui");
        // Exactly the registry fields — a credential or key can never ride along.
        let mut keys: Vec<&str> = json["clients"][0]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(keys, ["builtin", "created_at", "id", "issued_at", "name"]);
        assert!(json.get("api_token").is_none());
    }

    #[test]
    fn settings_response_reports_disabled_auth() {
        let response = build_response_pub(
            &crate::settings::Settings::default(),
            &crate::auth::AuthMode::Disabled,
            vec![],
        );
        let json = serde_json::to_value(&response).expect("serializes");
        assert_eq!(json["api_auth_enabled"], false);
        assert_eq!(json["api_auth_source"], "disabled");
    }
}
