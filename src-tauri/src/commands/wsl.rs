use std::collections::HashSet;

#[cfg(feature = "desktop")]
use std::sync::Arc;
#[cfg(feature = "desktop")]
use tauri::State;

#[cfg(feature = "desktop")]
use crate::commands::settings::{build_response_pub, SettingsResponse};
#[cfg(feature = "desktop")]
use crate::state::AppState;

/// Trim, drop empty entries, and de-duplicate distro names while preserving order.
pub fn sanitize_distros(distros: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    distros
        .into_iter()
        .map(|d| d.trim().to_string())
        .filter(|d| !d.is_empty() && seen.insert(d.clone()))
        .collect()
}

/// List installed WSL distributions. Empty on non-Windows hosts or when WSL is
/// not installed.
#[cfg(feature = "desktop")]
#[tauri::command]
pub async fn list_wsl_distros() -> Result<Vec<String>, String> {
    Ok(crate::wsl::list_distros())
}

/// Persist the set of WSL distros whose projects should be discovered.
#[cfg(feature = "desktop")]
#[tauri::command]
pub async fn set_wsl_distros(
    distros: Vec<String>,
    state: State<'_, Arc<AppState>>,
) -> Result<SettingsResponse, String> {
    let mut guard = state.settings.lock().map_err(|e| e.to_string())?;
    guard.wsl_distros = sanitize_distros(distros);
    crate::settings::save_settings(&guard)?;
    Ok(build_response_pub(&guard))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_trims_and_drops_empty() {
        let out = sanitize_distros(vec![
            "  Ubuntu  ".to_string(),
            "".to_string(),
            "   ".to_string(),
            "Debian".to_string(),
        ]);
        assert_eq!(out, vec!["Ubuntu", "Debian"]);
    }

    #[test]
    fn sanitize_dedupes_preserving_order() {
        let out = sanitize_distros(vec![
            "Ubuntu".to_string(),
            "Debian".to_string(),
            "Ubuntu".to_string(),
        ]);
        assert_eq!(out, vec!["Ubuntu", "Debian"]);
    }
}
