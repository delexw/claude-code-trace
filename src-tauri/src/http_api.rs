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

use crate::convert::*;
use crate::parser::chunk::build_chunks;
use crate::parser::debuglog::*;
use crate::parser::ongoing::OngoingChecker;
use crate::parser::session::{extract_session_meta, read_session_with_debug_hooks};
use crate::parser::subagent::{discover_and_link_all, inject_orphan_subagents};
use crate::parser::team::reconstruct_teams;
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
        .route("/api/project-dirs", get(api_get_project_dirs))
        .route("/api/sessions", post(api_discover_sessions))
        .route("/api/session", get(api_get_session_by_id))
        .route("/api/session/load", post(api_load_session))
        .route("/api/session/meta", get(api_get_session_meta))
        .route("/api/session/watch", post(api_watch_session))
        .route("/api/session/unwatch", post(api_unwatch_session))
        .route("/api/picker/watch", post(api_watch_picker))
        .route("/api/picker/unwatch", post(api_unwatch_picker))
        .route("/api/git-info", get(api_get_git_info))
        .route("/api/debug-log", get(api_get_debug_log))
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
    let default_dir = crate::commands::settings::platform_default_dir();
    ok_json(&crate::commands::settings::SettingsResponse {
        projects_dir: guard.projects_dir.clone(),
        default_dir,
    })
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
    let default_dir = crate::commands::settings::platform_default_dir();
    ok_json(&crate::commands::settings::SettingsResponse {
        projects_dir: guard.projects_dir.clone(),
        default_dir,
    })
}

// ---------------------------------------------------------------------------
// Project dirs
// ---------------------------------------------------------------------------

async fn api_get_project_dirs(State(state): State<Arc<HttpState>>) -> Response {
    let app_state = app_state(&state);
    let configured = match app_state.settings.lock() {
        Ok(g) => g.projects_dir.clone(),
        Err(e) => {
            return err_response(axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
        }
    };
    let projects_dir = match crate::parser::session::claude_projects_dir(configured.as_deref()) {
        Ok(d) => d,
        Err(e) => return err_response(axum::http::StatusCode::INTERNAL_SERVER_ERROR, e),
    };
    if !projects_dir.exists() {
        return ok_json(&Vec::<String>::new());
    }
    let entries = match std::fs::read_dir(&projects_dir) {
        Ok(e) => e,
        Err(e) => {
            return err_response(axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
        }
    };
    let dirs: Vec<String> = entries
        .flatten()
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .map(|e| e.path().to_string_lossy().to_string())
        .collect();
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
}

fn load_session_by_path(
    app_state: &AppState,
    path: String,
    since: Option<DateTime<Utc>>,
    before: Option<DateTime<Utc>>,
) -> Response {
    let (classified, _new_offset, _) = match read_session_with_debug_hooks(&path) {
        Ok(v) => v,
        Err(e) => return err_response(axum::http::StatusCode::INTERNAL_SERVER_ERROR, e),
    };
    let mut chunks = build_chunks(&classified);
    let (mut all_procs, color_map) = discover_and_link_all(&path, &chunks);
    inject_orphan_subagents(&mut chunks, &mut all_procs);
    if since.is_some() || before.is_some() {
        chunks.retain(|c| {
            since.map_or(true, |s| c.timestamp >= s) && before.map_or(true, |b| c.timestamp < b)
        });
    }

    let ongoing = OngoingChecker::new(&chunks, &all_procs, &path).is_ongoing();
    app_state.set_watched_ongoing(path.clone(), ongoing);

    let teams = reconstruct_teams(&chunks, &all_procs);
    let messages = chunks_to_messages(&chunks, &all_procs, &color_map);
    let meta = extract_session_meta(&path);

    let scanned = crate::parser::session::scan_session_metadata(&path);
    let session_totals = SessionTotals {
        total_tokens: scanned.total_tokens,
        input_tokens: scanned.input_tokens,
        output_tokens: scanned.output_tokens,
        cache_read_tokens: scanned.cache_read_tokens,
        cache_creation_tokens: scanned.cache_creation_tokens,
        cost_usd: scanned.cost_usd,
        model: scanned.model,
    };

    ok_json(&LoadResult {
        messages,
        teams,
        path,
        ongoing,
        meta,
        session_totals,
    })
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
    load_session_by_path(app_state(&state), body.path, None, None)
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
    let configured = match app_state.settings.lock() {
        Ok(g) => g.projects_dir.clone(),
        Err(e) => {
            return err_response(axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
        }
    };
    let projects_dir = match crate::parser::session::claude_projects_dir(configured.as_deref()) {
        Ok(d) => d,
        Err(e) => return err_response(axum::http::StatusCode::INTERNAL_SERVER_ERROR, e),
    };

    // Search all project subdirs for <id>.jsonl
    let filename = format!("{}.jsonl", q.id);
    let found_path = std::fs::read_dir(&projects_dir).ok().and_then(|entries| {
        entries.flatten().find_map(|entry| {
            if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                return None;
            }
            let candidate = entry.path().join(&filename);
            if candidate.exists() {
                Some(candidate.to_string_lossy().to_string())
            } else {
                None
            }
        })
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
    use crate::parser::chunk::{Chunk, ChunkType};
    use chrono::TimeZone;

    fn chunk_at(year: i32, month: u32, day: u32) -> Chunk {
        Chunk {
            chunk_type: ChunkType::User,
            timestamp: Utc.with_ymd_and_hms(year, month, day, 0, 0, 0).unwrap(),
            ..Chunk::default()
        }
    }

    fn apply_filter(
        chunks: &mut Vec<Chunk>,
        since: Option<DateTime<Utc>>,
        before: Option<DateTime<Utc>>,
    ) {
        if since.is_some() || before.is_some() {
            chunks.retain(|c| {
                since.map_or(true, |s| c.timestamp >= s) && before.map_or(true, |b| c.timestamp < b)
            });
        }
    }

    #[test]
    fn since_future_excludes_all() {
        let mut chunks = vec![chunk_at(2025, 1, 1), chunk_at(2025, 6, 1)];
        let since = Utc.with_ymd_and_hms(2099, 1, 1, 0, 0, 0).unwrap();
        apply_filter(&mut chunks, Some(since), None);
        assert!(chunks.is_empty());
    }

    #[test]
    fn before_ancient_excludes_all() {
        let mut chunks = vec![chunk_at(2025, 1, 1), chunk_at(2025, 6, 1)];
        let before = Utc.with_ymd_and_hms(2000, 1, 1, 0, 0, 0).unwrap();
        apply_filter(&mut chunks, None, Some(before));
        assert!(chunks.is_empty());
    }

    #[test]
    fn since_filters_older_keeps_newer() {
        let mut chunks = vec![
            chunk_at(2025, 1, 1),
            chunk_at(2025, 6, 1),
            chunk_at(2026, 1, 1),
        ];
        let since = Utc.with_ymd_and_hms(2025, 6, 1, 0, 0, 0).unwrap();
        apply_filter(&mut chunks, Some(since), None);
        assert_eq!(chunks.len(), 2);
        assert!(chunks.iter().all(|c| c.timestamp >= since));
    }

    #[test]
    fn before_filters_newer_keeps_older() {
        let mut chunks = vec![
            chunk_at(2025, 1, 1),
            chunk_at(2025, 6, 1),
            chunk_at(2026, 1, 1),
        ];
        let before = Utc.with_ymd_and_hms(2025, 6, 1, 0, 0, 0).unwrap();
        apply_filter(&mut chunks, None, Some(before));
        assert_eq!(chunks.len(), 1);
        assert!(chunks.iter().all(|c| c.timestamp < before));
    }

    #[test]
    fn since_and_before_window() {
        let mut chunks = vec![
            chunk_at(2025, 1, 1),
            chunk_at(2025, 6, 1),
            chunk_at(2025, 9, 1),
            chunk_at(2026, 1, 1),
        ];
        let since = Utc.with_ymd_and_hms(2025, 6, 1, 0, 0, 0).unwrap();
        let before = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        apply_filter(&mut chunks, Some(since), Some(before));
        assert_eq!(chunks.len(), 2);
    }

    #[test]
    fn no_filter_keeps_all() {
        let mut chunks = vec![chunk_at(2025, 1, 1), chunk_at(2025, 6, 1)];
        apply_filter(&mut chunks, None, None);
        assert_eq!(chunks.len(), 2);
    }

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
}
