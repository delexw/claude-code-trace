//! Settings → API access → Regenerate: rotate the shared HTTP API client token.
//!
//! Mirrors `commands/cors.rs`: a plain impl function shared by the Tauri IPC
//! command and the HTTP route (`POST /api/settings/token/regenerate`), with
//! only the `#[tauri::command]` wrapper gated on the `desktop` feature.

use crate::commands::settings::{build_response_pub, SettingsResponse};
use crate::state::AppState;

#[cfg(feature = "desktop")]
use std::sync::Arc;
#[cfg(feature = "desktop")]
use tauri::State;

/// Rotate the token file, swap the live token, and return the refreshed
/// settings (which carry the new token for the UI to display).
pub fn regenerate_api_token_impl(state: &AppState) -> Result<SettingsResponse, String> {
    state.regenerate_api_token()?;
    let guard = state.settings.lock().map_err(|e| e.to_string())?;
    Ok(build_response_pub(&guard, &state.api_auth_snapshot()))
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub async fn regenerate_api_token(
    state: State<'_, Arc<AppState>>,
) -> Result<SettingsResponse, String> {
    regenerate_api_token_impl(&state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::ApiAuth;

    // File-mode rotation is covered in `state.rs` against a temp path; the
    // impl here would rotate the developer's real token file, so only the
    // rejection paths are exercised.

    #[test]
    fn env_token_cannot_be_regenerated() {
        let state = AppState::new(ApiAuth::Env("fixed".into()));
        let err = regenerate_api_token_impl(&state).unwrap_err();
        assert!(err.contains("CCTRACE_API_TOKEN"), "{err}");
    }

    #[test]
    fn disabled_auth_cannot_be_regenerated() {
        let state = AppState::new(ApiAuth::Disabled);
        let err = regenerate_api_token_impl(&state).unwrap_err();
        assert!(err.contains("disabled"), "{err}");
    }
}
