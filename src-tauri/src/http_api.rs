use std::convert::Infallible;
use std::sync::Arc;

use chrono::{DateTime, Utc};

use axum::extract::{Query, State};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Json, Response};
use axum::routing::{get, post};
use axum::Router;
use serde::Deserialize;
use tauri::{AppHandle, Manager};
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;

use crate::parser::debuglog::*;
use crate::parser::session::extract_session_meta;
use crate::state::AppState;
use crate::watcher::{start_picker_watcher, start_session_watcher};

/// Shared state for axum handlers.
#[derive(Clone)]
pub struct HttpState {
    pub app_state: Arc<AppState>,
    pub app: Option<AppHandle>,
}

/// Default bind host. Overridable via the `CCTRACE_HTTP_HOST` env var.
pub const DEFAULT_HTTP_HOST: &str = "127.0.0.1";
/// Default bind port. Overridable via the `CCTRACE_HTTP_PORT` env var.
pub const DEFAULT_HTTP_PORT: u16 = 11423;

/// Pick the host from a raw env value, normalizing empty/missing to the default.
fn pick_host(raw: Option<String>) -> String {
    raw.filter(|s| !s.is_empty())
        .unwrap_or_else(|| DEFAULT_HTTP_HOST.to_string())
}

/// Pick the port from a raw env value, silently dropping invalid values.
fn pick_port(raw: Option<String>) -> u16 {
    raw.and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(DEFAULT_HTTP_PORT)
}

/// Resolve the bind address from env vars, falling back to the defaults.
pub fn resolve_bind_addr() -> (String, u16) {
    (
        pick_host(std::env::var("CCTRACE_HTTP_HOST").ok()),
        pick_port(std::env::var("CCTRACE_HTTP_PORT").ok()),
    )
}

/// Optional directory of static frontend assets to serve alongside the API.
/// When `CCTRACE_STATIC_DIR` is set to a non-empty path, the HTTP server
/// will serve the frontend bundle as a fallback for all non-API routes.
/// This is used by the Docker image to run the full web UI from a single
/// process on a single port.
pub fn resolve_static_dir() -> Option<String> {
    std::env::var("CCTRACE_STATIC_DIR")
        .ok()
        .filter(|s| !s.is_empty())
}

/// Start the HTTP API server from a Tauri AppHandle (desktop/web mode).
pub async fn start_http_server(app: AppHandle) {
    let app_state: Arc<AppState> = app.state::<Arc<AppState>>().inner().clone();
    run_server(Arc::new(HttpState {
        app_state,
        app: Some(app),
    }))
    .await;
}

/// Start the HTTP server without Tauri (headless mode).
pub async fn start_http_server_headless(state: Arc<AppState>) {
    run_server(Arc::new(HttpState {
        app_state: state,
        app: None,
    }))
    .await;
}

async fn run_server(state: Arc<HttpState>) {
    let mut router = Router::new()
        .route("/api/settings", get(api_get_settings))
        .route("/api/settings/dir", post(api_set_projects_dir))
        .route(
            "/api/wsl/distros",
            get(api_list_wsl_distros).post(api_set_wsl_distros),
        )
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
        .route("/api/events", get(api_events));

    if let Some(dir) = resolve_static_dir() {
        let serve = ServeDir::new(&dir).append_index_html_on_directories(true);
        router = router.fallback_service(serve);
        eprintln!("HTTP API: serving static assets from {dir}");
    }

    let router = router.layer(CorsLayer::permissive()).with_state(state);

    let (host, port) = resolve_bind_addr();
    let addr = format!("{host}:{port}");
    let listener = match tokio::net::TcpListener::bind(&addr).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("HTTP API: failed to bind {addr}: {e}");
            return;
        }
    };
    eprintln!("HTTP API: listening on http://{addr}");

    if let Err(e) = axum::serve(listener, router).await {
        eprintln!("HTTP API: server error: {e}");
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn app_state(state: &HttpState) -> &AppState {
    &state.app_state
}

fn err_response(status: axum::http::StatusCode, msg: String) -> Response {
    (status, Json(serde_json::json!({ "error": msg }))).into_response()
}

fn ok_json<T: serde::Serialize>(val: &T) -> Response {
    Json(val).into_response()
}

// ---------------------------------------------------------------------------
// Settings
// ---------------------------------------------------------------------------

async fn api_get_settings(State(state): State<Arc<HttpState>>) -> Response {
    let app_state = app_state(&state);
    let guard = match app_state.settings.lock() {
        Ok(g) => g,
        Err(e) => {
            return err_response(axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
        }
    };
    ok_json(&crate::commands::settings::build_response_pub(&guard))
}

#[derive(Deserialize)]
struct SetDirBody {
    path: Option<String>,
}

async fn api_set_projects_dir(
    State(state): State<Arc<HttpState>>,
    Json(body): Json<SetDirBody>,
) -> Response {
    let app_state = app_state(&state);

    if let Some(ref p) = body.path {
        let pb = std::path::PathBuf::from(p);
        if !pb.exists() {
            return err_response(
                axum::http::StatusCode::BAD_REQUEST,
                format!("path does not exist: {p}"),
            );
        }
        if !pb.is_dir() {
            return err_response(
                axum::http::StatusCode::BAD_REQUEST,
                format!("path is not a directory: {p}"),
            );
        }
    }

    let mut guard = match app_state.settings.lock() {
        Ok(g) => g,
        Err(e) => {
            return err_response(axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
        }
    };
    guard.projects_dir = body.path;
    if let Err(e) = crate::settings::save_settings(&guard) {
        return err_response(axum::http::StatusCode::INTERNAL_SERVER_ERROR, e);
    }
    ok_json(&crate::commands::settings::build_response_pub(&guard))
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
// WSL distros
// ---------------------------------------------------------------------------

async fn api_list_wsl_distros() -> Response {
    ok_json(&crate::wsl::list_distros())
}

#[derive(Deserialize)]
struct SetWslBody {
    distros: Vec<String>,
}

async fn api_set_wsl_distros(
    State(state): State<Arc<HttpState>>,
    Json(body): Json<SetWslBody>,
) -> Response {
    let app_state = app_state(&state);
    let mut guard = match app_state.settings.lock() {
        Ok(g) => g,
        Err(e) => {
            return err_response(axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
        }
    };
    guard.wsl_distros = crate::commands::wsl::sanitize_distros(body.distros);
    if let Err(e) = crate::settings::save_settings(&guard) {
        return err_response(axum::http::StatusCode::INTERNAL_SERVER_ERROR, e);
    }
    ok_json(&crate::commands::settings::build_response_pub(&guard))
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
    let mut sessions = match app_state.discover_sessions_cached(&body.dirs) {
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
    ok_json(&crate::commands::git::get_git_info(q.cwd))
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
    match crate::commands::terminal::focus_session_window_impl(&body.session_id) {
        Ok(()) => ok_json(&serde_json::json!({ "ok": true })),
        Err(e) => err_response(axum::http::StatusCode::BAD_REQUEST, e.user_message()),
    }
}

// ---------------------------------------------------------------------------
// SSE events
// ---------------------------------------------------------------------------

async fn api_events(
    State(state): State<Arc<HttpState>>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>> {
    let app_state = app_state(&state);
    let rx = app_state.event_tx.subscribe();

    let stream = BroadcastStream::new(rx).filter_map(|result| {
        result
            .ok()
            .map(|sse_event| Ok(Event::default().event(sse_event.event).data(sse_event.data)))
    });

    Sse::new(stream).keep_alive(KeepAlive::default())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

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
    // Bind address / static dir resolution
    // -----------------------------------------------------------------------

    #[test]
    fn pick_host_uses_default_when_missing() {
        assert_eq!(pick_host(None), DEFAULT_HTTP_HOST);
    }

    #[test]
    fn pick_host_uses_default_when_empty() {
        assert_eq!(pick_host(Some(String::new())), DEFAULT_HTTP_HOST);
    }

    #[test]
    fn pick_host_uses_provided_value() {
        assert_eq!(pick_host(Some("0.0.0.0".to_string())), "0.0.0.0");
    }

    #[test]
    fn pick_port_uses_default_when_missing() {
        assert_eq!(pick_port(None), DEFAULT_HTTP_PORT);
    }

    #[test]
    fn pick_port_uses_default_when_unparsable() {
        assert_eq!(
            pick_port(Some("not-a-number".to_string())),
            DEFAULT_HTTP_PORT
        );
    }

    #[test]
    fn pick_port_uses_parsed_value() {
        assert_eq!(pick_port(Some("8080".to_string())), 8080);
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
