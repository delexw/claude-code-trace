use std::collections::HashSet;

use axum::http::{HeaderValue, Uri};

#[cfg(feature = "desktop")]
use std::sync::Arc;
#[cfg(feature = "desktop")]
use tauri::State;

#[cfg(feature = "desktop")]
use crate::commands::settings::{build_response_pub, SettingsResponse};
#[cfg(feature = "desktop")]
use crate::state::AppState;

/// Validate a single origin: must parse as an `http(s)://host[:port]` URI
/// with no path, query, or invalid header characters. Returns a message
/// naming the offending origin on failure.
fn validate_origin(origin: &str) -> Result<(), String> {
    let invalid = || {
        format!("invalid origin: {origin} (expected e.g. http://example.com:8080, no path/query)")
    };

    let uri: Uri = origin.parse().map_err(|_| invalid())?;
    let scheme_ok = matches!(uri.scheme_str(), Some("http") | Some("https"));
    let has_authority = uri.authority().is_some_and(|a| !a.as_str().is_empty());
    // `Uri` normalizes a missing path and a bare trailing slash to the same
    // `"/"`, but a real browser never sends a trailing slash in an `Origin`
    // header — so a stored `"http://example.com/"` would silently never
    // match anything. Reject it explicitly rather than accepting a value
    // that can never work.
    let path_ok = matches!(uri.path(), "" | "/") && !origin.ends_with('/');
    let query_ok = uri.query().is_none();
    if !scheme_ok || !has_authority || !path_ok || !query_ok {
        return Err(invalid());
    }

    HeaderValue::from_str(origin)
        .map(|_| ())
        .map_err(|_| invalid())
}

/// Trim, drop empty entries, de-duplicate (preserving order), and validate
/// each remaining entry as a well-formed CORS origin. Rejects the whole batch
/// on the first invalid entry rather than silently dropping it — mirrors how
/// `set_projects_dir` rejects before mutating anything.
pub fn sanitize_and_validate_origins(origins: Vec<String>) -> Result<Vec<String>, String> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for raw in origins {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }
        validate_origin(trimmed)?;
        if seen.insert(trimmed.to_string()) {
            out.push(trimmed.to_string());
        }
    }
    Ok(out)
}

/// Persist the set of extra CORS origins allowed to call the local HTTP API.
#[cfg(feature = "desktop")]
#[tauri::command]
pub async fn set_allowed_origins(
    origins: Vec<String>,
    state: State<'_, Arc<AppState>>,
) -> Result<SettingsResponse, String> {
    let validated = sanitize_and_validate_origins(origins)?;
    let mut guard = state.settings.lock().map_err(|e| e.to_string())?;
    guard.allowed_origins = validated;
    crate::settings::save_settings(&guard)?;
    Ok(build_response_pub(&guard))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_trims_and_drops_empty() {
        let out = sanitize_and_validate_origins(vec![
            "  http://a.example  ".to_string(),
            "".to_string(),
            "   ".to_string(),
            "http://b.example".to_string(),
        ])
        .unwrap();
        assert_eq!(out, vec!["http://a.example", "http://b.example"]);
    }

    #[test]
    fn sanitize_dedupes_preserving_order() {
        let out = sanitize_and_validate_origins(vec![
            "http://a.example".to_string(),
            "http://b.example".to_string(),
            "http://a.example".to_string(),
        ])
        .unwrap();
        assert_eq!(out, vec!["http://a.example", "http://b.example"]);
    }

    #[test]
    fn accepts_valid_http_and_https_origins_with_and_without_port() {
        assert!(sanitize_and_validate_origins(vec!["http://example.com".to_string()]).is_ok());
        assert!(sanitize_and_validate_origins(vec!["https://example.com".to_string()]).is_ok());
        assert!(sanitize_and_validate_origins(vec!["http://example.com:8080".to_string()]).is_ok());
    }

    #[test]
    fn rejects_bare_hostname_with_no_scheme() {
        assert!(sanitize_and_validate_origins(vec!["example.com".to_string()]).is_err());
    }

    #[test]
    fn rejects_origin_with_trailing_path() {
        assert!(sanitize_and_validate_origins(vec!["http://example.com/foo".to_string()]).is_err());
    }

    #[test]
    fn rejects_origin_with_trailing_slash() {
        assert!(sanitize_and_validate_origins(vec!["http://example.com/".to_string()]).is_err());
    }

    #[test]
    fn rejects_origin_with_embedded_newline() {
        assert!(sanitize_and_validate_origins(vec![
            "http://example.com\nEvil-Header: 1".to_string()
        ])
        .is_err());
    }

    #[test]
    fn rejects_first_invalid_entry_without_partial_acceptance() {
        let result = sanitize_and_validate_origins(vec![
            "http://good.example".to_string(),
            "bad".to_string(),
        ]);
        assert!(result.is_err());
    }
}
