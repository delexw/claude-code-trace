use std::convert::Infallible;
use std::sync::Arc;

use chrono::{DateTime, Utc};

use axum::extract::{Query, State};
use axum::http::{header, HeaderName, HeaderValue, Method};
use axum::middleware::from_fn_with_state;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Json, Response};
use axum::routing::{get, post};
use axum::Router;
use serde::Deserialize;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::services::ServeDir;

use crate::auth;
use crate::parser::debuglog::*;
use crate::parser::session::extract_session_meta;
use crate::state::AppState;
use crate::watcher::{start_picker_watcher, start_session_watcher};
use crate::AppHandle;

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

/// Origins the browser UI is served from. In web/dev mode the frontend runs on
/// `localhost:1420` and calls the API on port 11423 — a distinct origin — so
/// these are allowlisted for CORS. The Tauri desktop webview talks to the
/// backend over the IPC bridge (never HTTP), and the Docker image serves the UI
/// same-origin, so neither needs an entry here. Extra origins can be added
/// statically via `CCTRACE_ALLOWED_ORIGINS` or live via the Settings UI
/// (`Settings.allowed_origins`, checked per-request in `build_cors`); both
/// compose with these defaults rather than replacing them.
const DEFAULT_ALLOWED_ORIGINS: [&str; 2] = ["http://localhost:1420", "http://127.0.0.1:1420"];

/// Split a raw `CCTRACE_ALLOWED_ORIGINS` value into individual origins,
/// dropping empty entries and surrounding whitespace.
fn parse_extra_origins(raw: Option<String>) -> Vec<String> {
    raw.into_iter()
        .flat_map(|s| {
            s.split(',')
                .map(|p| p.trim().to_string())
                .filter(|p| !p.is_empty())
                .collect::<Vec<_>>()
        })
        .collect()
}

/// Resolve the *static* half of the CORS origin allowlist: the built-in
/// dev/web origins plus any added via the `CCTRACE_ALLOWED_ORIGINS` env var
/// (comma-separated). Resolved once at startup — the env var never changes at
/// runtime. The *live* half (origins configured via the Settings UI) is
/// checked per-request in `build_cors`'s predicate; the two are unioned, not
/// one replacing the other.
pub(crate) fn resolve_allowed_origins() -> Vec<String> {
    let mut origins: Vec<String> = DEFAULT_ALLOWED_ORIGINS
        .iter()
        .map(|s| s.to_string())
        .collect();
    origins.extend(parse_extra_origins(
        std::env::var("CCTRACE_ALLOWED_ORIGINS").ok(),
    ));
    origins
}

/// Exact-match check against the union of the static (default/env) and live
/// (Settings-UI-configured) allowlists. No prefix/substring/wildcard
/// matching — this is the function PR 206 hardened against a real
/// cross-origin data leak; keep it exact.
fn origin_allowed(origin: &str, static_origins: &[String], live_origins: &[String]) -> bool {
    static_origins.iter().any(|o| o == origin) || live_origins.iter().any(|o| o == origin)
}

/// Build a CORS layer scoped to the allowlisted origins. This replaces a
/// permissive `*` policy under which any website the user visited could read
/// local Claude session data (prompts, code, tool output) cross-origin while
/// the app was running.
///
/// The origin check is a live predicate rather than a static list so that
/// origins added via the Settings UI (`Settings.allowed_origins`) take effect
/// immediately, with no server restart — consistent with every other setting
/// in this app. The static defaults/env var are resolved once; the
/// Settings-UI list is re-read from `app_state` on every request.
fn build_cors(app_state: Arc<AppState>) -> CorsLayer {
    let static_origins = resolve_allowed_origins();
    CorsLayer::new()
        .allow_origin(AllowOrigin::predicate(
            move |origin: &HeaderValue, _parts: &axum::http::request::Parts| {
                let Ok(origin_str) = origin.to_str() else {
                    return false;
                };
                // Fail closed on a poisoned lock — this is a security
                // allowlist, not a feature that should fail open.
                let live_origins = app_state
                    .settings
                    .lock()
                    .map(|g| g.allowed_origins.clone())
                    .unwrap_or_default();
                origin_allowed(origin_str, &static_origins, &live_origins)
            },
        ))
        .allow_methods([Method::GET, Method::POST])
        // The token header must be preflight-allowed or browsers never send it.
        .allow_headers([
            header::CONTENT_TYPE,
            header::AUTHORIZATION,
            HeaderName::from_static(auth::TOKEN_HEADER),
        ])
}

/// Start the HTTP API server from a Tauri AppHandle (desktop/web mode).
#[cfg(feature = "desktop")]
pub async fn start_http_server(app: AppHandle) {
    use tauri::Manager;
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

/// Assemble the full router: token-gated `/api/*` routes, the optional static
/// UI fallback (Docker), and CORS outermost.
///
/// Layer order matters:
/// - `route_layer` applies the auth middleware to the registered API routes
///   only — the static fallback stays public (the SPA shell must load before
///   it can authenticate) and unknown paths still 404 rather than 401.
/// - The static fallback gets its own middleware that hands the token to the
///   same-origin browser UI as a cookie (see `auth::attach_token_cookie`).
/// - CORS is added last, so it runs first: preflights are answered before the
///   auth check, and 401 responses carry CORS headers so the browser can read
///   the `{"error"}` body.
fn build_router(state: Arc<HttpState>, static_dir: Option<String>) -> Router {
    let mut router = Router::new()
        .route("/api/settings", get(api_get_settings))
        .route("/api/settings/dir", post(api_set_projects_dir))
        .route("/api/settings/origins", post(api_set_allowed_origins))
        .route(
            "/api/settings/token/regenerate",
            post(api_regenerate_api_token),
        )
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
        .route("/api/events", get(api_events))
        .route_layer(from_fn_with_state(state.clone(), auth::require_api_token));

    if let Some(dir) = static_dir {
        let serve = ServeDir::new(&dir).append_index_html_on_directories(true);
        let static_ui = Router::new()
            .fallback_service(serve)
            .layer(from_fn_with_state(state.clone(), auth::attach_token_cookie));
        router = router.fallback_service(static_ui);
        eprintln!("HTTP API: serving static assets from {dir}");
    }

    let cors_state = state.app_state.clone();
    router.layer(build_cors(cors_state)).with_state(state)
}

async fn run_server(state: Arc<HttpState>) {
    let router = build_router(state, resolve_static_dir());

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

pub(crate) fn err_response(status: axum::http::StatusCode, msg: String) -> Response {
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
    ok_json(&crate::commands::settings::build_response_pub(
        &guard,
        &app_state.api_auth_snapshot(),
    ))
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
    ok_json(&crate::commands::settings::build_response_pub(
        &guard,
        &app_state.api_auth_snapshot(),
    ))
}

#[derive(Deserialize)]
struct SetOriginsBody {
    origins: Vec<String>,
}

async fn api_set_allowed_origins(
    State(state): State<Arc<HttpState>>,
    Json(body): Json<SetOriginsBody>,
) -> Response {
    let app_state = app_state(&state);

    let validated = match crate::commands::cors::sanitize_and_validate_origins(body.origins) {
        Ok(v) => v,
        Err(e) => return err_response(axum::http::StatusCode::BAD_REQUEST, e),
    };

    let mut guard = match app_state.settings.lock() {
        Ok(g) => g,
        Err(e) => {
            return err_response(axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
        }
    };
    guard.allowed_origins = validated;
    if let Err(e) = crate::settings::save_settings(&guard) {
        return err_response(axum::http::StatusCode::INTERNAL_SERVER_ERROR, e);
    }
    ok_json(&crate::commands::settings::build_response_pub(
        &guard,
        &app_state.api_auth_snapshot(),
    ))
}

/// Rotate the shared API token. Reachable only by a caller that already holds
/// the current token (the route is gated like every other). The new token is
/// also set as the same-origin cookie so a Docker browser tab rotates in place.
async fn api_regenerate_api_token(State(state): State<Arc<HttpState>>) -> Response {
    let app_state = app_state(&state);
    match crate::commands::api_token::regenerate_api_token_impl(app_state) {
        Ok(settings) => {
            let mut response = ok_json(&settings);
            if let Some(cookie) = settings
                .api_token
                .as_deref()
                .and_then(auth::token_cookie_header)
            {
                response.headers_mut().append(header::SET_COOKIE, cookie);
            }
            response
        }
        Err(e) => err_response(axum::http::StatusCode::BAD_REQUEST, e),
    }
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
    ok_json(&crate::commands::settings::build_response_pub(
        &guard,
        &app_state.api_auth_snapshot(),
    ))
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
    // CORS allowlist
    // -----------------------------------------------------------------------

    #[test]
    fn parse_extra_origins_is_empty_when_missing() {
        assert!(parse_extra_origins(None).is_empty());
    }

    #[test]
    fn parse_extra_origins_splits_and_trims() {
        assert_eq!(
            parse_extra_origins(Some(" http://a.example , http://b.example ".to_string())),
            vec!["http://a.example", "http://b.example"],
        );
    }

    #[test]
    fn parse_extra_origins_drops_empty_entries() {
        assert!(parse_extra_origins(Some(" , ,".to_string())).is_empty());
    }

    #[test]
    fn default_origins_are_the_dev_web_ui() {
        assert_eq!(
            DEFAULT_ALLOWED_ORIGINS,
            ["http://localhost:1420", "http://127.0.0.1:1420"],
        );
    }

    #[test]
    fn default_origins_parse_to_valid_header_values() {
        for origin in DEFAULT_ALLOWED_ORIGINS {
            assert!(HeaderValue::from_str(origin).is_ok(), "{origin}");
        }
    }

    #[test]
    fn build_cors_constructs_without_panicking() {
        let state = Arc::new(crate::state::AppState::new(crate::auth::ApiAuth::Disabled));
        let _ = build_cors(state);
    }

    #[test]
    fn origin_allowed_matches_static_default() {
        let static_origins = resolve_allowed_origins();
        assert!(origin_allowed(
            "http://localhost:1420",
            &static_origins,
            &[]
        ));
    }

    #[test]
    fn origin_allowed_matches_live_settings_origin() {
        let live = vec!["https://cctrace.example.com".to_string()];
        assert!(origin_allowed("https://cctrace.example.com", &[], &live));
    }

    #[test]
    fn origin_allowed_denies_unrelated_origin() {
        let static_origins = resolve_allowed_origins();
        let live = vec!["https://cctrace.example.com".to_string()];
        assert!(!origin_allowed(
            "https://evil.example",
            &static_origins,
            &live
        ));
    }

    #[test]
    fn origin_allowed_denies_when_both_lists_empty() {
        assert!(!origin_allowed("http://localhost:1420", &[], &[]));
    }

    #[test]
    fn origin_allowed_denies_substring_or_prefix_match() {
        let static_origins = vec!["http://localhost:1420".to_string()];
        // Guards against ever "helpfully" loosening the exact-match check to
        // `.starts_with()`/`.contains()` — both would let this through.
        assert!(!origin_allowed(
            "http://localhost:1420.evil.com",
            &static_origins,
            &[]
        ));
    }

    // -----------------------------------------------------------------------
    // Client verification (token middleware) — full router via `oneshot`
    // -----------------------------------------------------------------------

    use crate::auth::ApiAuth;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    const TOKEN: &str = "test-token-0123456789abcdef";

    fn test_state(auth: ApiAuth) -> Arc<HttpState> {
        Arc::new(HttpState {
            app_state: Arc::new(crate::state::AppState::new(auth)),
            app: None,
        })
    }

    fn api_router(auth: ApiAuth) -> Router {
        build_router(test_state(auth), None)
    }

    fn get(uri: &str) -> Request<Body> {
        Request::builder().uri(uri).body(Body::empty()).unwrap()
    }

    fn get_with(uri: &str, headers: &[(&str, &str)]) -> Request<Body> {
        let mut b = Request::builder().uri(uri);
        for (k, v) in headers {
            b = b.header(*k, *v);
        }
        b.body(Body::empty()).unwrap()
    }

    async fn body_json(resp: Response) -> serde_json::Value {
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[tokio::test]
    async fn api_route_without_token_is_401_with_json_error() {
        let resp = api_router(ApiAuth::File(TOKEN.into()))
            .oneshot(get("/api/settings"))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
        let json = body_json(resp).await;
        assert!(json["error"].as_str().unwrap().contains("API token"));
    }

    #[tokio::test]
    async fn api_route_with_wrong_token_is_401() {
        let resp = api_router(ApiAuth::File(TOKEN.into()))
            .oneshot(get_with("/api/settings", &[("x-cctrace-token", "nope")]))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn api_route_with_header_token_is_200() {
        let resp = api_router(ApiAuth::File(TOKEN.into()))
            .oneshot(get_with("/api/settings", &[("x-cctrace-token", TOKEN)]))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        assert_eq!(json["api_token"], TOKEN);
        assert_eq!(json["api_token_source"], "file");
    }

    #[tokio::test]
    async fn api_route_with_bearer_token_is_200() {
        let resp = api_router(ApiAuth::Env(TOKEN.into()))
            .oneshot(get_with(
                "/api/settings",
                &[("authorization", &format!("Bearer {TOKEN}"))],
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn api_route_with_query_token_is_200() {
        let resp = api_router(ApiAuth::File(TOKEN.into()))
            .oneshot(get(&format!("/api/settings?token={TOKEN}")))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn api_route_with_cookie_token_is_200() {
        let resp = api_router(ApiAuth::File(TOKEN.into()))
            .oneshot(get_with(
                "/api/settings",
                &[("cookie", &format!("theme=dark; cctrace_token={TOKEN}"))],
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn sse_route_accepts_query_token() {
        let resp = api_router(ApiAuth::File(TOKEN.into()))
            .oneshot(get(&format!("/api/events?token={TOKEN}")))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let ct = resp.headers().get(header::CONTENT_TYPE).unwrap();
        assert!(ct.to_str().unwrap().starts_with("text/event-stream"));
    }

    #[tokio::test]
    async fn auth_disabled_allows_requests_without_token() {
        let resp = api_router(ApiAuth::Disabled)
            .oneshot(get("/api/settings"))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        assert_eq!(json["api_auth_enabled"], false);
        assert!(json["api_token"].is_null());
    }

    #[tokio::test]
    async fn unknown_path_is_404_not_401() {
        let resp = api_router(ApiAuth::File(TOKEN.into()))
            .oneshot(get("/api/does-not-exist"))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn cors_preflight_passes_without_token_and_allows_token_header() {
        let req = Request::builder()
            .method(Method::OPTIONS)
            .uri("/api/settings")
            .header("origin", "http://localhost:1420")
            .header("access-control-request-method", "GET")
            .header("access-control-request-headers", "x-cctrace-token")
            .body(Body::empty())
            .unwrap();
        let resp = api_router(ApiAuth::File(TOKEN.into()))
            .oneshot(req)
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let allowed = resp
            .headers()
            .get("access-control-allow-headers")
            .unwrap()
            .to_str()
            .unwrap()
            .to_ascii_lowercase();
        assert!(allowed.contains("x-cctrace-token"), "{allowed}");
    }

    #[tokio::test]
    async fn unauthorized_response_still_carries_cors_headers() {
        let resp = api_router(ApiAuth::File(TOKEN.into()))
            .oneshot(get_with(
                "/api/settings",
                &[("origin", "http://localhost:1420")],
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(
            resp.headers()
                .get("access-control-allow-origin")
                .and_then(|v| v.to_str().ok()),
            Some("http://localhost:1420")
        );
    }

    #[tokio::test]
    async fn regenerate_rotates_live_token_so_old_token_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("api-token");
        std::fs::write(&path, format!("{TOKEN}\n")).unwrap();
        let state = test_state(ApiAuth::File(TOKEN.into()));

        let new_token = state
            .app_state
            .regenerate_api_token_at(Some(&path))
            .unwrap();

        let old = build_router(state.clone(), None)
            .oneshot(get_with("/api/settings", &[("x-cctrace-token", TOKEN)]))
            .await
            .unwrap();
        assert_eq!(old.status(), StatusCode::UNAUTHORIZED);

        let fresh = build_router(state, None)
            .oneshot(get_with(
                "/api/settings",
                &[("x-cctrace-token", &new_token)],
            ))
            .await
            .unwrap();
        assert_eq!(fresh.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn mismatch_re_reads_a_rotated_token_file_before_rejecting() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("api-token");
        std::fs::write(&path, "rotated-by-other-process\n").unwrap();
        let state = test_state(ApiAuth::File(TOKEN.into()));
        state.app_state.set_api_token_path(Some(path));

        // The token in the file (rotated by another process) is accepted…
        let fresh = build_router(state.clone(), None)
            .oneshot(get_with(
                "/api/settings",
                &[("x-cctrace-token", "rotated-by-other-process")],
            ))
            .await
            .unwrap();
        assert_eq!(fresh.status(), StatusCode::OK);
        // …and from then on the stale in-memory token is rejected.
        let stale = build_router(state.clone(), None)
            .oneshot(get_with("/api/settings", &[("x-cctrace-token", TOKEN)]))
            .await
            .unwrap();
        assert_eq!(stale.status(), StatusCode::UNAUTHORIZED);
        // A wrong token still fails even though the file exists.
        let wrong = build_router(state, None)
            .oneshot(get_with("/api/settings", &[("x-cctrace-token", "nope")]))
            .await
            .unwrap();
        assert_eq!(wrong.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn sse_route_accepts_percent_encoded_env_token_in_query() {
        let resp = api_router(ApiAuth::Env("my/secret+key".into()))
            .oneshot(get("/api/events?token=my%2Fsecret%2Bkey"))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn regenerate_route_rejects_ephemeral_source() {
        let router = api_router(ApiAuth::Ephemeral(TOKEN.into()));
        let req = Request::builder()
            .method(Method::POST)
            .uri("/api/settings/token/regenerate")
            .header("x-cctrace-token", TOKEN)
            .body(Body::empty())
            .unwrap();
        let resp = router.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let json = body_json(resp).await;
        assert!(json["error"]
            .as_str()
            .unwrap()
            .contains("could not be persisted"));
        assert!(!json["error"]
            .as_str()
            .unwrap()
            .contains("CCTRACE_API_TOKEN"));
    }

    #[tokio::test]
    async fn regenerate_route_requires_token_and_rejects_env_source() {
        let router = api_router(ApiAuth::Env(TOKEN.into()));
        let post = |headers: &[(&str, &str)]| {
            let mut b = Request::builder()
                .method(Method::POST)
                .uri("/api/settings/token/regenerate");
            for (k, v) in headers {
                b = b.header(*k, *v);
            }
            b.body(Body::empty()).unwrap()
        };
        let denied = router.clone().oneshot(post(&[])).await.unwrap();
        assert_eq!(denied.status(), StatusCode::UNAUTHORIZED);

        let rejected = router
            .oneshot(post(&[("x-cctrace-token", TOKEN)]))
            .await
            .unwrap();
        assert_eq!(rejected.status(), StatusCode::BAD_REQUEST);
        let json = body_json(rejected).await;
        assert!(json["error"]
            .as_str()
            .unwrap()
            .contains("CCTRACE_API_TOKEN"));
    }

    // -- static fallback + cookie -------------------------------------------

    fn static_dir() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("index.html"),
            "<!doctype html><title>x</title>",
        )
        .unwrap();
        std::fs::write(dir.path().join("app.js"), "console.log(1)").unwrap();
        dir
    }

    fn static_router(auth: ApiAuth, dir: &tempfile::TempDir) -> Router {
        build_router(
            test_state(auth),
            Some(dir.path().to_string_lossy().to_string()),
        )
    }

    fn set_cookie(resp: &Response) -> Option<String> {
        resp.headers()
            .get(header::SET_COOKIE)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned)
    }

    #[tokio::test]
    async fn static_fallback_serves_without_token() {
        let dir = static_dir();
        let resp = static_router(ApiAuth::File(TOKEN.into()), &dir)
            .oneshot(get_with("/app.js", &[("host", "localhost:1421")]))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        // Non-HTML assets never carry the cookie.
        assert!(set_cookie(&resp).is_none());
    }

    #[tokio::test]
    async fn static_index_sets_token_cookie_for_localhost_host() {
        let dir = static_dir();
        let resp = static_router(ApiAuth::File(TOKEN.into()), &dir)
            .oneshot(get_with("/", &[("host", "localhost:1421")]))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let cookie = set_cookie(&resp).expect("Set-Cookie present");
        assert!(
            cookie.starts_with(&format!("cctrace_token={TOKEN};")),
            "{cookie}"
        );
        assert!(cookie.contains("HttpOnly"));
        assert!(cookie.contains("SameSite=Strict"));
    }

    #[tokio::test]
    async fn static_index_does_not_set_cookie_for_unknown_host() {
        let dir = static_dir();
        let resp = static_router(ApiAuth::File(TOKEN.into()), &dir)
            .oneshot(get_with("/", &[("host", "attacker.example:1421")]))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK, "the page still loads");
        assert!(set_cookie(&resp).is_none(), "but no token cookie is issued");
    }

    #[tokio::test]
    async fn static_index_does_not_set_cookie_without_host_header() {
        let dir = static_dir();
        let resp = static_router(ApiAuth::File(TOKEN.into()), &dir)
            .oneshot(get("/"))
            .await
            .unwrap();
        assert!(set_cookie(&resp).is_none());
    }

    #[tokio::test]
    async fn static_index_sets_cookie_for_settings_allowlisted_host() {
        let dir = static_dir();
        let state = test_state(ApiAuth::File(TOKEN.into()));
        state.app_state.settings.lock().unwrap().allowed_origins =
            vec!["https://cctrace.example.com".to_string()];
        let resp = build_router(state, Some(dir.path().to_string_lossy().to_string()))
            .oneshot(get_with("/", &[("host", "cctrace.example.com")]))
            .await
            .unwrap();
        assert!(set_cookie(&resp).is_some());
    }

    #[tokio::test]
    async fn static_index_sets_no_cookie_when_auth_disabled() {
        let dir = static_dir();
        let resp = static_router(ApiAuth::Disabled, &dir)
            .oneshot(get_with("/", &[("host", "localhost:1421")]))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert!(set_cookie(&resp).is_none());
    }

    #[tokio::test]
    async fn api_routes_stay_gated_when_static_fallback_is_mounted() {
        let dir = static_dir();
        let resp = static_router(ApiAuth::File(TOKEN.into()), &dir)
            .oneshot(get_with("/api/settings", &[("host", "localhost:1421")]))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
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
