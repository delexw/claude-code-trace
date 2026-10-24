//! HTTP routes for the Claude source plugin: listing, loading and watching
//! Claude Code sessions, plus the per-session git info, debug log and
//! terminal focus.

use std::sync::Arc;

use axum::extract::{Query, State};
use axum::response::{Json, Response};
use axum::routing::{get, post};
use axum::Router;
use chrono::{DateTime, Utc};
use serde::Deserialize;

use crate::http_api::{app_state, err_response, ok_json, HttpState};
use crate::parser::debuglog::*;
use crate::parser::session::extract_session_meta;
use crate::state::AppState;
use crate::watcher::{start_picker_watcher, start_session_watcher};

pub(super) fn routes() -> Router<Arc<HttpState>> {
    Router::new()
        .route("/api/project-dirs", get(api_get_project_dirs))
        .route("/api/sessions", post(api_discover_sessions))
        .route("/api/session", get(api_get_session_by_id))
        .route("/api/session/load", post(api_load_session))
        .route("/api/session/message", post(api_load_message))
        .route("/api/session/meta", get(api_get_session_meta))
        .route("/api/session/watch", post(api_watch_session))
        .route("/api/session/unwatch", post(api_unwatch_session))
        .route("/api/picker/watch", post(api_watch_picker))
        .route("/api/picker/unwatch", post(api_unwatch_picker))
        .route("/api/git-info", get(api_get_git_info))
        .route("/api/debug-log", get(api_get_debug_log))
        .route("/api/focus", post(api_focus_session_window))
}

// ---------------------------------------------------------------------------
// Project dirs
// ---------------------------------------------------------------------------

async fn api_get_project_dirs(State(state): State<Arc<HttpState>>) -> Response {
    let app_state = app_state(&state);
    let (configured, wsl_distros) = match app_state.settings.lock() {
        Ok(g) => (g.projects_dir.clone(), g.wsl_distros.clone()),
        Err(e) => {
            return err_response(axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
        }
    };
    let dirs = crate::wsl::collect_project_dirs(configured.as_deref(), &wsl_distros);
    ok_json(&dirs)
}

// ---------------------------------------------------------------------------
// Sessions
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct DiscoverBody {
    dirs: Vec<String>,
}

async fn api_discover_sessions(
    State(state): State<Arc<HttpState>>,
    Json(body): Json<DiscoverBody>,
) -> Response {
    let app_state = app_state(&state);
    let mut sessions =
        match AppState::discover_sessions_cached(&state.app_state, &body.dirs, state.app.clone()) {
            Ok(s) => s,
            Err(e) => return err_response(axum::http::StatusCode::INTERNAL_SERVER_ERROR, e),
        };
    app_state.apply_watched_ongoing(&mut sessions);
    ok_json(&sessions)
}

#[derive(Deserialize)]
struct PathBody {
    path: String,
    /// Optional window for virtualized clients — first message index.
    #[serde(default)]
    start: Option<usize>,
    /// Optional window size; omit to load to the end.
    #[serde(default)]
    limit: Option<usize>,
}

/// Load a session by timestamp window (used by the by-id range endpoint).
fn load_session_by_path(
    app_state: &AppState,
    path: String,
    since: Option<DateTime<Utc>>,
    before: Option<DateTime<Utc>>,
) -> Response {
    let opts = crate::session_load::LoadOptions::filtered(crate::session_load::TimeFilter {
        since,
        before,
    });
    load_session_with(app_state, path, opts)
}

/// Shared tail for every HTTP session-load path: build via the single pipeline,
/// record ongoing status, and serialize. Keeps the endpoints thin pass-throughs.
fn load_session_with(
    app_state: &AppState,
    path: String,
    opts: crate::session_load::LoadOptions,
) -> Response {
    let result = match app_state.load_session_windowed(&path, opts) {
        Ok(r) => r,
        Err(e) => return err_response(axum::http::StatusCode::INTERNAL_SERVER_ERROR, e),
    };
    app_state.set_watched_ongoing(path, result.ongoing);
    ok_json(&result)
}

async fn api_load_session(
    State(state): State<Arc<HttpState>>,
    Json(body): Json<PathBody>,
) -> Response {
    if body.path.is_empty() {
        return err_response(
            axum::http::StatusCode::BAD_REQUEST,
            "no session path provided".to_string(),
        );
    }
    let opts = crate::session_load::LoadOptions::window(crate::session_load::MessageRange {
        start: body.start.unwrap_or(0),
        limit: body.limit,
    });
    load_session_with(app_state(&state), body.path, opts)
}

#[derive(Deserialize)]
struct MessageBody {
    path: String,
    index: usize,
}

/// Return the full (heavy-body) message at `index` for the detail view.
async fn api_load_message(
    State(state): State<Arc<HttpState>>,
    Json(body): Json<MessageBody>,
) -> Response {
    if body.path.is_empty() {
        return err_response(
            axum::http::StatusCode::BAD_REQUEST,
            "no session path provided".to_string(),
        );
    }
    match app_state(&state).full_message_at(&body.path, body.index) {
        Ok(Some(msg)) => ok_json(&msg),
        Ok(None) => err_response(
            axum::http::StatusCode::NOT_FOUND,
            "message not found".to_string(),
        ),
        Err(e) => err_response(axum::http::StatusCode::INTERNAL_SERVER_ERROR, e),
    }
}

#[derive(Deserialize)]
struct SessionIdQuery {
    id: String,
    since: Option<String>,
    before: Option<String>,
}

async fn api_get_session_by_id(
    State(state): State<Arc<HttpState>>,
    Query(q): Query<SessionIdQuery>,
) -> Response {
    if q.id.is_empty() {
        return err_response(
            axum::http::StatusCode::BAD_REQUEST,
            "no session id provided".to_string(),
        );
    }

    let app_state = app_state(&state);
    let (configured, wsl_distros) = match app_state.settings.lock() {
        Ok(g) => (g.projects_dir.clone(), g.wsl_distros.clone()),
        Err(e) => {
            return err_response(axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
        }
    };

    // Search every project dir (host + WSL distros) for <id>.jsonl
    let project_dirs = crate::wsl::collect_project_dirs(configured.as_deref(), &wsl_distros);
    let filename = format!("{}.jsonl", q.id);
    let found_path = project_dirs.iter().find_map(|dir| {
        let candidate = std::path::Path::new(dir).join(&filename);
        if candidate.exists() {
            Some(candidate.to_string_lossy().to_string())
        } else {
            None
        }
    });

    let path = match found_path {
        Some(p) => p,
        None => {
            return err_response(
                axum::http::StatusCode::NOT_FOUND,
                format!("session not found: {}", q.id),
            )
        }
    };

    let since = match q.since.as_deref().map(|s| s.parse::<DateTime<Utc>>()) {
        Some(Err(_)) => {
            return err_response(
                axum::http::StatusCode::BAD_REQUEST,
                "invalid `since` timestamp — expected ISO 8601 UTC (e.g. 2025-01-15T10:00:00Z)"
                    .to_string(),
            )
        }
        Some(Ok(dt)) => Some(dt),
        None => None,
    };
    let before =
        match q.before.as_deref().map(|s| s.parse::<DateTime<Utc>>()) {
            Some(Err(_)) => return err_response(
                axum::http::StatusCode::BAD_REQUEST,
                "invalid `before` timestamp — expected ISO 8601 UTC (e.g. 2025-01-15T10:00:00Z)"
                    .to_string(),
            ),
            Some(Ok(dt)) => Some(dt),
            None => None,
        };

    load_session_by_path(app_state, path, since, before)
}

#[derive(Deserialize)]
struct MetaQuery {
    path: String,
}

async fn api_get_session_meta(Query(q): Query<MetaQuery>) -> Response {
    if q.path.is_empty() {
        return err_response(
            axum::http::StatusCode::BAD_REQUEST,
            "no session path provided".to_string(),
        );
    }
    ok_json(&extract_session_meta(&q.path))
}

// ---------------------------------------------------------------------------
// Watch / unwatch
// ---------------------------------------------------------------------------

async fn api_watch_session(
    State(state): State<Arc<HttpState>>,
    Json(body): Json<PathBody>,
) -> Response {
    let app_state = app_state(&state);
    if let Err(e) = app_state.stop_session_watcher() {
        return err_response(axum::http::StatusCode::INTERNAL_SERVER_ERROR, e);
    }
    let handle = start_session_watcher(body.path, state.app_state.clone(), state.app.clone());
    if let Err(e) = app_state.set_session_watcher(handle) {
        return err_response(axum::http::StatusCode::INTERNAL_SERVER_ERROR, e);
    }
    ok_json(&serde_json::json!({ "ok": true }))
}

async fn api_unwatch_session(State(state): State<Arc<HttpState>>) -> Response {
    let app_state = app_state(&state);
    app_state.clear_watched_ongoing();
    app_state.clear_session_build_cache();
    match app_state.stop_session_watcher() {
        Ok(()) => ok_json(&serde_json::json!({ "ok": true })),
        Err(e) => err_response(axum::http::StatusCode::INTERNAL_SERVER_ERROR, e),
    }
}

#[derive(Deserialize)]
struct WatchPickerBody {
    #[serde(rename = "projectDirs")]
    project_dirs: Vec<String>,
}

async fn api_watch_picker(
    State(state): State<Arc<HttpState>>,
    Json(body): Json<WatchPickerBody>,
) -> Response {
    let app_state = app_state(&state);
    if let Err(e) = app_state.stop_picker_watcher() {
        return err_response(axum::http::StatusCode::INTERNAL_SERVER_ERROR, e);
    }
    let handle = start_picker_watcher(
        body.project_dirs,
        state.app_state.clone(),
        state.app.clone(),
    );
    if let Err(e) = app_state.set_picker_watcher(handle) {
        return err_response(axum::http::StatusCode::INTERNAL_SERVER_ERROR, e);
    }
    ok_json(&serde_json::json!({ "ok": true }))
}

async fn api_unwatch_picker(State(state): State<Arc<HttpState>>) -> Response {
    let app_state = app_state(&state);
    match app_state.stop_picker_watcher() {
        Ok(()) => ok_json(&serde_json::json!({ "ok": true })),
        Err(e) => err_response(axum::http::StatusCode::INTERNAL_SERVER_ERROR, e),
    }
}

// ---------------------------------------------------------------------------
// Git info
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct GitQuery {
    cwd: String,
}

async fn api_get_git_info(Query(q): Query<GitQuery>) -> Response {
    ok_json(&super::commands::git::get_git_info(q.cwd))
}

// ---------------------------------------------------------------------------
// Debug log
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct DebugQuery {
    path: String,
    #[serde(rename = "minLevel")]
    min_level: Option<String>,
    #[serde(rename = "filterText")]
    filter_text: Option<String>,
}

async fn api_get_debug_log(Query(q): Query<DebugQuery>) -> Response {
    let debug_path = debug_log_path(&q.path);
    if debug_path.is_empty() {
        return ok_json(&Vec::<DebugEntry>::new());
    }
    let (entries, _offset) = match read_debug_log(&debug_path) {
        Ok(v) => v,
        Err(e) => return err_response(axum::http::StatusCode::INTERNAL_SERVER_ERROR, e),
    };
    let level = match q.min_level.as_deref() {
        Some("WARN") | Some("warn") => DebugLevel::Warn,
        Some("ERROR") | Some("error") => DebugLevel::Error,
        _ => DebugLevel::Debug,
    };
    let filtered = filter_by_level(&entries, &level);
    let filtered = filter_by_text(&filtered, q.filter_text.as_deref().unwrap_or(""));
    let collapsed = collapse_duplicates(filtered);
    ok_json(&collapsed)
}

// ---------------------------------------------------------------------------
// Focus
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct FocusBody {
    #[serde(rename = "sessionId")]
    session_id: String,
}

async fn api_focus_session_window(Json(body): Json<FocusBody>) -> Response {
    match super::commands::terminal::focus_session_window_impl(&body.session_id) {
        Ok(()) => ok_json(&serde_json::json!({ "ok": true })),
        Err(e) => err_response(axum::http::StatusCode::BAD_REQUEST, e.user_message()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Timestamp-window filtering is owned and tested by `crate::session_load`
    // (`TimeFilter`). Here we only cover the HTTP-layer concern of parsing the
    // `since`/`before` query strings.

    #[test]
    fn invalid_since_parse_fails() {
        assert!("notadate".parse::<DateTime<Utc>>().is_err());
    }

    // -----------------------------------------------------------------------
    // Focus
    // -----------------------------------------------------------------------

    // Full route registration (via `run_server`) needs a live `AppState`/bound
    // port; instead this calls the handler directly to verify it's wired to
    // `focus_session_window_impl` and shapes errors like the other POST
    // handlers in this file (BAD_REQUEST + `{ "error": ... }` envelope).
    #[tokio::test]
    async fn focus_route_reports_bad_request_for_a_session_that_is_not_live() {
        let body = FocusBody {
            session_id: "does-not-exist".to_string(),
        };
        let resp = api_focus_session_window(Json(body)).await;
        assert_eq!(resp.status(), axum::http::StatusCode::BAD_REQUEST);
    }
}
