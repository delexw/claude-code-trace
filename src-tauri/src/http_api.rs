use std::convert::Infallible;
use std::sync::Arc;

use chrono::{DateTime, Utc};

use axum::extract::{Path as UrlPath, Query, State};
use axum::http::{header, HeaderName, HeaderValue, Method};
use axum::middleware::from_fn_with_state;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Json, Response};
use axum::routing::{get, post};
use axum::Extension;
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
///   same-origin browser UI as a cookie (see `auth::attach_credential_cookie`).
/// - CORS is added last, so it runs first: preflights are answered before the
///   auth check, and 401 responses carry CORS headers so the browser can read
///   the `{"error"}` body.
fn build_router(state: Arc<HttpState>, static_dir: Option<String>) -> Router {
    let mut router = Router::new()
        .route("/api/settings", get(api_get_settings))
        .route("/api/settings/dir", post(api_set_projects_dir))
        .route("/api/settings/origins", post(api_set_allowed_origins))
        .route(
            "/api/analytics/settings",
            get(api_get_analytics_settings).post(api_set_analytics_settings),
        )
        .route(
            "/api/analytics/jev/key",
            post(api_set_jev_api_key).delete(api_clear_jev_api_key),
        )
        .route("/api/analytics/jev/test", post(api_test_jev_connection))
        .route(
            "/api/analytics/recommendation/key",
            post(api_set_recommendation_api_key).delete(api_clear_recommendation_api_key),
        )
        .route(
            "/api/analytics/recommendation/test",
            post(api_test_recommendation_provider),
        )
        .route(
            "/api/efficiency/prepare",
            post(api_prepare_efficiency_payload),
        )
        .route("/api/efficiency/start", post(api_start_efficiency_analysis))
        .route("/api/efficiency/jobs", get(api_list_efficiency_jobs))
        .route(
            "/api/efficiency/job/{id}/cancel",
            post(api_cancel_efficiency_analysis),
        )
        .route(
            "/api/efficiency/result",
            get(api_get_session_efficiency).delete(api_delete_session_efficiency),
        )
        .route(
            "/api/efficiency/summaries",
            get(api_list_efficiency_summaries),
        )
        .route(
            "/api/clients",
            get(api_list_clients).post(api_register_client),
        )
        .route("/api/clients/{id}/reissue", post(api_reissue_client))
        .route("/api/clients/{id}/revoke", post(api_revoke_client))
        .route("/api/whoami", get(api_whoami))
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
        .route_layer(from_fn_with_state(state.clone(), auth::require_client));

    if let Some(dir) = static_dir {
        let serve = ServeDir::new(&dir).append_index_html_on_directories(true);
        let static_ui = Router::new()
            .fallback_service(serve)
            .layer(from_fn_with_state(
                state.clone(),
                auth::attach_credential_cookie,
            ));
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
        &app_state.auth_snapshot(),
        app_state.clients_snapshot(),
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
        &app_state.auth_snapshot(),
        app_state.clients_snapshot(),
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
        &app_state.auth_snapshot(),
        app_state.clients_snapshot(),
    ))
}

// ---------------------------------------------------------------------------
// Analytics / Jev efficiency analysis
// ---------------------------------------------------------------------------

async fn api_get_analytics_settings() -> Response {
    match crate::commands::efficiency::get_analytics_settings_impl() {
        Ok(settings) => ok_json(&settings),
        Err(error) => err_response(axum::http::StatusCode::INTERNAL_SERVER_ERROR, error),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AnalyticsSettingsBody {
    default_payload_mode: crate::efficiency::settings::PayloadMode,
    recommendation_provider: crate::efficiency::settings::RecommendationProvider,
}

async fn api_set_analytics_settings(Json(body): Json<AnalyticsSettingsBody>) -> Response {
    match crate::commands::efficiency::set_analytics_settings_impl(
        body.default_payload_mode,
        body.recommendation_provider,
    ) {
        Ok(settings) => ok_json(&settings),
        Err(error) => err_response(axum::http::StatusCode::INTERNAL_SERVER_ERROR, error),
    }
}

#[derive(Deserialize)]
struct ApiKeyBody {
    key: String,
}

async fn api_set_jev_api_key(Json(body): Json<ApiKeyBody>) -> Response {
    match crate::credentials::api_tokens::store(
        crate::credentials::api_tokens::ApiToken::Jev,
        &body.key,
    )
    .and_then(|()| crate::commands::efficiency::get_analytics_settings_impl())
    {
        Ok(settings) => ok_json(&settings),
        Err(error) => err_response(axum::http::StatusCode::BAD_REQUEST, error),
    }
}

async fn api_clear_jev_api_key() -> Response {
    match crate::credentials::api_tokens::delete(crate::credentials::api_tokens::ApiToken::Jev)
        .and_then(|()| crate::commands::efficiency::get_analytics_settings_impl())
    {
        Ok(settings) => ok_json(&settings),
        Err(error) => err_response(axum::http::StatusCode::INTERNAL_SERVER_ERROR, error),
    }
}

async fn api_test_jev_connection() -> Response {
    let key = match crate::credentials::api_tokens::resolve(
        crate::credentials::api_tokens::ApiToken::Jev,
    ) {
        Ok(Some(key)) => key,
        Ok(None) => {
            return err_response(
                axum::http::StatusCode::BAD_REQUEST,
                "Jev API key required".to_string(),
            )
        }
        Err(error) => return err_response(axum::http::StatusCode::INTERNAL_SERVER_ERROR, error),
    };
    match crate::efficiency::jev::test_connection(&key).await {
        Ok(()) => ok_json(&serde_json::json!({ "status": "connected" })),
        Err(error) => err_response(axum::http::StatusCode::BAD_GATEWAY, error),
    }
}

async fn api_set_recommendation_api_key(Json(body): Json<ApiKeyBody>) -> Response {
    match crate::credentials::api_tokens::store(
        crate::credentials::api_tokens::ApiToken::RecommendationProvider,
        &body.key,
    ) {
        Ok(()) => ok_json(&serde_json::json!({ "configured": true })),
        Err(error) => err_response(axum::http::StatusCode::BAD_REQUEST, error),
    }
}

async fn api_clear_recommendation_api_key() -> Response {
    match crate::credentials::api_tokens::delete(
        crate::credentials::api_tokens::ApiToken::RecommendationProvider,
    ) {
        Ok(()) => ok_json(&serde_json::json!({ "configured": false })),
        Err(error) => err_response(axum::http::StatusCode::INTERNAL_SERVER_ERROR, error),
    }
}

#[derive(Deserialize)]
struct RecommendationProviderBody {
    provider: crate::efficiency::settings::RecommendationProvider,
}

async fn api_test_recommendation_provider(
    Json(body): Json<RecommendationProviderBody>,
) -> Response {
    match crate::commands::efficiency::test_recommendation_provider_impl(body.provider).await {
        Ok(()) => ok_json(&serde_json::json!({ "status": "connected" })),
        Err(error) => err_response(axum::http::StatusCode::BAD_GATEWAY, error),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PrepareEfficiencyBody {
    path: String,
    payload_mode: Option<crate::efficiency::settings::PayloadMode>,
}

async fn api_prepare_efficiency_payload(Json(body): Json<PrepareEfficiencyBody>) -> Response {
    match crate::commands::efficiency::prepare_session_efficiency_payload_impl(
        &body.path,
        body.payload_mode,
    ) {
        Ok(payload) => ok_json(&payload),
        Err(error) => err_response(axum::http::StatusCode::BAD_REQUEST, error),
    }
}

#[derive(Deserialize)]
struct StartEfficiencyBody {
    path: String,
    payload: crate::efficiency::PreparedEfficiencyPayload,
}

async fn api_start_efficiency_analysis(
    State(state): State<Arc<HttpState>>,
    Json(body): Json<StartEfficiencyBody>,
) -> Response {
    match crate::commands::efficiency::start_session_efficiency_analysis_impl(
        state.app_state.clone(),
        state.app.clone(),
        body.path,
        body.payload,
    ) {
        Ok(job) => ok_json(&job),
        Err(error) => err_response(axum::http::StatusCode::BAD_REQUEST, error),
    }
}

async fn api_list_efficiency_jobs(State(state): State<Arc<HttpState>>) -> Response {
    match crate::commands::efficiency::list_efficiency_analysis_jobs_impl(&state.app_state) {
        Ok(jobs) => ok_json(&jobs),
        Err(error) => err_response(axum::http::StatusCode::INTERNAL_SERVER_ERROR, error),
    }
}

async fn api_cancel_efficiency_analysis(
    State(state): State<Arc<HttpState>>,
    UrlPath(id): UrlPath<String>,
) -> Response {
    match crate::commands::efficiency::cancel_efficiency_analysis_impl(&state.app_state, &id) {
        Ok(()) => ok_json(&serde_json::json!({ "cancelled": true })),
        Err(error) => err_response(axum::http::StatusCode::NOT_FOUND, error),
    }
}

#[derive(Deserialize)]
struct EfficiencyPathQuery {
    path: String,
}

async fn api_get_session_efficiency(Query(query): Query<EfficiencyPathQuery>) -> Response {
    match crate::efficiency::cache::read(&query.path) {
        Ok(analysis) => ok_json(&analysis),
        Err(error) => err_response(axum::http::StatusCode::INTERNAL_SERVER_ERROR, error),
    }
}

async fn api_delete_session_efficiency(Query(query): Query<EfficiencyPathQuery>) -> Response {
    match crate::efficiency::cache::delete(&query.path) {
        Ok(()) => ok_json(&serde_json::json!({ "deleted": true })),
        Err(error) => err_response(axum::http::StatusCode::INTERNAL_SERVER_ERROR, error),
    }
}

async fn api_list_efficiency_summaries() -> Response {
    match crate::efficiency::cache::list_summaries() {
        Ok(summaries) => ok_json(&summaries),
        Err(error) => err_response(axum::http::StatusCode::INTERNAL_SERVER_ERROR, error),
    }
}

// ---------------------------------------------------------------------------
// Accepted clients
// ---------------------------------------------------------------------------

/// Registered clients — never their credentials.
async fn api_list_clients(State(state): State<Arc<HttpState>>) -> Response {
    ok_json(&crate::commands::clients::list_clients_impl(app_state(
        &state,
    )))
}

#[derive(Deserialize)]
struct RegisterClientBody {
    name: String,
}

/// Register a client and return its credential exactly once.
async fn api_register_client(
    State(state): State<Arc<HttpState>>,
    Json(body): Json<RegisterClientBody>,
) -> Response {
    match crate::commands::clients::register_client_impl(app_state(&state), &body.name) {
        Ok(issued) => ok_json(&issued),
        Err(e) => err_response(axum::http::StatusCode::BAD_REQUEST, e),
    }
}

/// Reissue a client's credential, invalidating every older one. When the
/// client is `web-ui` the new credential is also set as the same-origin
/// cookie so the Docker browser tab that asked keeps working.
async fn api_reissue_client(
    State(state): State<Arc<HttpState>>,
    UrlPath(id): UrlPath<String>,
) -> Response {
    match crate::commands::clients::reissue_client_impl(app_state(&state), &id) {
        Ok(issued) => {
            let mut response = ok_json(&issued);
            if issued.client.name == crate::clients::WEB_UI {
                if let Some(cookie) = auth::credential_cookie_header(&issued.credential) {
                    response.headers_mut().append(header::SET_COOKIE, cookie);
                }
            }
            response
        }
        Err(e) => err_response(axum::http::StatusCode::BAD_REQUEST, e),
    }
}

async fn api_revoke_client(
    State(state): State<Arc<HttpState>>,
    UrlPath(id): UrlPath<String>,
) -> Response {
    match crate::commands::clients::revoke_client_impl(app_state(&state), &id) {
        Ok(client) => ok_json(&client),
        Err(e) => err_response(axum::http::StatusCode::BAD_REQUEST, e),
    }
}

/// Who is calling? `client` is `null` only when verification is disabled
/// (`CCTRACE_API_AUTH=off`); every other caller was identified by the
/// middleware.
async fn api_whoami(identity: Option<Extension<auth::ClientIdentity>>) -> Response {
    let identity = identity.map(|Extension(i)| i);
    ok_json(&serde_json::json!({
        "auth_enabled": identity.is_some(),
        "client": identity,
    }))
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
        &app_state.auth_snapshot(),
        app_state.clients_snapshot(),
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
        let state = Arc::new(crate::state::AppState::for_tests(
            crate::auth::AuthMode::Disabled,
        ));
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
    // Client verification (JWT middleware) — full router via `oneshot`
    // -----------------------------------------------------------------------

    use crate::auth::{AuthMode, KeySource};
    use crate::clients::{ClientRegistry, TUI, WEB_UI};
    use crate::jwt::Claims;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    const KEY: &[u8] = b"0123456789abcdef0123456789abcdef";

    fn enabled() -> AuthMode {
        AuthMode::Enabled {
            key: KEY.to_vec(),
            source: KeySource::Ephemeral,
        }
    }

    fn test_state(auth: AuthMode) -> Arc<HttpState> {
        Arc::new(HttpState {
            app_state: Arc::new(crate::state::AppState::for_tests(auth)),
            app: None,
        })
    }

    fn api_router(auth: AuthMode) -> Router {
        build_router(test_state(auth), None)
    }

    /// A valid credential for the built-in client `name`, minted with the
    /// test key at the client's current `issued_at`.
    fn credential_for(state: &HttpState, name: &str) -> String {
        let client = state
            .app_state
            .clients_snapshot()
            .into_iter()
            .find(|c| c.name == name)
            .expect("built-in client registered");
        crate::auth::issue_credential(&client, KEY, client.issued_at)
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

    fn post_json(uri: &str, credential: &str, body: serde_json::Value) -> Request<Body> {
        Request::builder()
            .method(Method::POST)
            .uri(uri)
            .header("x-cctrace-token", credential)
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap()
    }

    fn post_empty(uri: &str, credential: &str) -> Request<Body> {
        Request::builder()
            .method(Method::POST)
            .uri(uri)
            .header("x-cctrace-token", credential)
            .body(Body::empty())
            .unwrap()
    }

    async fn body_json(resp: Response) -> serde_json::Value {
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    async fn whoami(router: Router, headers: &[(&str, &str)]) -> (StatusCode, serde_json::Value) {
        let resp = router
            .oneshot(get_with("/api/whoami", headers))
            .await
            .unwrap();
        let status = resp.status();
        (status, body_json(resp).await)
    }

    #[tokio::test]
    async fn anonymous_request_is_401_json_with_guidance() {
        let resp = api_router(enabled())
            .oneshot(get("/api/settings"))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
        let json = body_json(resp).await;
        let msg = json["error"].as_str().unwrap();
        assert!(msg.contains("client credential"), "{msg}");
        assert!(msg.contains("Accepted clients"), "{msg}");
    }

    #[tokio::test]
    async fn every_carrier_identifies_the_calling_client() {
        let state = test_state(enabled());
        let tui = credential_for(&state, TUI);
        let web = credential_for(&state, WEB_UI);
        let router = build_router(state.clone(), None);

        let (status, json) = whoami(router.clone(), &[("x-cctrace-token", &tui)]).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(json["auth_enabled"], true);
        assert_eq!(json["client"]["name"], "tui");
        assert!(json["client"]["id"].as_str().is_some());

        let bearer = format!("Bearer {tui}");
        let (status, json) = whoami(router.clone(), &[("authorization", &bearer)]).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(json["client"]["name"], "tui");

        let resp = router
            .clone()
            .oneshot(get(&format!("/api/whoami?token={web}")))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(body_json(resp).await["client"]["name"], "web-ui");

        let cookie = format!("other=1; cctrace_token={web}");
        let (status, json) = whoami(router, &[("cookie", &cookie)]).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(json["client"]["name"], "web-ui");
    }

    #[tokio::test]
    async fn cookie_carrier_is_ignored_for_same_site_form_posts() {
        let state = test_state(enabled());
        let web = credential_for(&state, WEB_UI);
        let web_id = state
            .app_state
            .clients_snapshot()
            .into_iter()
            .find(|c| c.name == WEB_UI)
            .unwrap()
            .id;
        let router = build_router(state.clone(), None);
        let cookie = format!("cctrace_token={web}");

        // A page on another port of the same host auto-submits a form: the
        // browser attaches the cookie (same-site) and labels the fetch.
        let forged = Request::builder()
            .method(Method::POST)
            .uri(format!("/api/clients/{web_id}/revoke"))
            .header("cookie", &cookie)
            .header("sec-fetch-site", "same-site")
            .body(Body::empty())
            .unwrap();
        let resp = router.clone().oneshot(forged).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
        assert!(
            !state
                .app_state
                .clients_snapshot()
                .iter()
                .find(|c| c.name == WEB_UI)
                .unwrap()
                .is_revoked(),
            "web-ui was not revoked by the forged request"
        );

        // The UI's own fetches are same-origin and keep working.
        let (status, json) = whoami(
            router.clone(),
            &[("cookie", &cookie), ("sec-fetch-site", "same-origin")],
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(json["client"]["name"], "web-ui");

        // The header carrier is unaffected: forms cannot set custom headers.
        let (status, _) = whoami(
            router,
            &[("x-cctrace-token", &web), ("sec-fetch-site", "cross-site")],
        )
        .await;
        assert_eq!(status, StatusCode::OK);
    }

    #[tokio::test]
    async fn settings_report_auth_mode_and_clients_but_no_credentials() {
        let state = test_state(enabled());
        let tui = credential_for(&state, TUI);
        let resp = build_router(state, None)
            .oneshot(get_with("/api/settings", &[("x-cctrace-token", &tui)]))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        assert_eq!(json["api_auth_enabled"], true);
        assert_eq!(json["api_auth_source"], "ephemeral");
        let names: Vec<&str> = json["clients"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["name"].as_str().unwrap())
            .collect();
        assert_eq!(names, ["web-ui", "tui"]);
        assert!(!json.to_string().contains(&tui));
    }

    #[tokio::test]
    async fn forged_foreign_and_unknown_credentials_are_rejected() {
        let state = test_state(enabled());
        let tui = credential_for(&state, TUI);
        let router = build_router(state.clone(), None);

        // Same claims, signed with a different key.
        let client = state
            .app_state
            .clients_snapshot()
            .into_iter()
            .find(|c| c.name == TUI)
            .unwrap();
        let forged =
            crate::auth::issue_credential(&client, b"not the server key", client.issued_at);
        let (status, _) = whoami(router.clone(), &[("x-cctrace-token", &forged)]).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);

        // Correct key, but the client id is not registered.
        let unknown = crate::jwt::sign(
            &Claims {
                sub: uuid::Uuid::new_v4().to_string(),
                name: "tui".into(),
                iat: client.issued_at,
            },
            KEY,
        );
        let (status, _) = whoami(router.clone(), &[("x-cctrace-token", &unknown)]).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);

        // Correct key and client, but minted before the client's issued_at.
        let stale = crate::auth::issue_credential(&client, KEY, client.issued_at - 1);
        let (status, _) = whoami(router.clone(), &[("x-cctrace-token", &stale)]).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);

        // Tampered payload.
        let mut parts: Vec<String> = tui.split('.').map(str::to_owned).collect();
        parts[1].push('x');
        let (status, _) = whoami(router.clone(), &[("x-cctrace-token", &parts.join("."))]).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);

        // Garbage.
        let (status, _) = whoami(router, &[("x-cctrace-token", "nope")]).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn disabled_mode_accepts_anonymous_callers_without_identity() {
        let router = api_router(AuthMode::Disabled);
        let (status, json) = whoami(router.clone(), &[]).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(json["auth_enabled"], false);
        assert!(json["client"].is_null());

        let resp = router.oneshot(get("/api/settings")).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        assert_eq!(json["api_auth_enabled"], false);
        assert_eq!(json["api_auth_source"], "disabled");
    }

    #[tokio::test]
    async fn unknown_api_path_is_404_not_401() {
        let resp = api_router(enabled())
            .oneshot(get("/api/does-not-exist"))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn cors_preflight_passes_without_credential_and_allows_token_header() {
        let req = Request::builder()
            .method(Method::OPTIONS)
            .uri("/api/settings")
            .header("origin", "http://localhost:1420")
            .header("access-control-request-method", "GET")
            .header("access-control-request-headers", "x-cctrace-token")
            .body(Body::empty())
            .unwrap();
        let resp = api_router(enabled()).oneshot(req).await.unwrap();
        assert_ne!(resp.status(), StatusCode::UNAUTHORIZED);
        let allow = resp
            .headers()
            .get("access-control-allow-headers")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_ascii_lowercase();
        assert!(allow.contains("x-cctrace-token"), "{allow}");
        assert!(allow.contains("authorization"), "{allow}");
    }

    #[tokio::test]
    async fn unauthorized_response_carries_cors_headers_for_allowed_origin() {
        let resp = api_router(enabled())
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
    async fn sse_route_accepts_percent_encoded_credential_in_query() {
        let state = test_state(enabled());
        let tui = credential_for(&state, TUI);
        // base64url never contains `%`, so encode a few characters by hand to
        // prove the decoder runs; `.` and `-` are the only punctuation present.
        let encoded = tui.replace('.', "%2E").replacen('-', "%2D", 1);
        assert_ne!(encoded, tui);
        let resp = build_router(state, None)
            .oneshot(get(&format!("/api/events?token={encoded}")))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    // -- client management over HTTP -----------------------------------------

    #[tokio::test]
    async fn register_list_reissue_revoke_roundtrip_over_http() {
        let state = test_state(enabled());
        let tui = credential_for(&state, TUI);
        let router = build_router(state, None);

        // Register.
        let resp = router
            .clone()
            .oneshot(post_json(
                "/api/clients",
                &tui,
                serde_json::json!({ "name": "ci-script" }),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let issued = body_json(resp).await;
        let id = issued["client"]["id"].as_str().unwrap().to_owned();
        let cred = issued["credential"].as_str().unwrap().to_owned();
        assert_eq!(issued["client"]["name"], "ci-script");
        assert_eq!(issued["client"]["builtin"], false);

        // The new client is identified by name.
        let (status, json) = whoami(router.clone(), &[("x-cctrace-token", &cred)]).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(json["client"]["name"], "ci-script");
        assert_eq!(json["client"]["id"], id);

        // List never leaks credentials.
        let resp = router
            .clone()
            .oneshot(get_with("/api/clients", &[("x-cctrace-token", &cred)]))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let list = body_json(resp).await;
        assert_eq!(list.as_array().unwrap().len(), 3);
        assert!(!list.to_string().contains(&cred));

        // Duplicate name → 400.
        let resp = router
            .clone()
            .oneshot(post_json(
                "/api/clients",
                &tui,
                serde_json::json!({ "name": "CI-Script" }),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        assert!(body_json(resp).await["error"]
            .as_str()
            .unwrap()
            .contains("already exists"));

        // Reissue: the old credential dies, the new one works.
        let resp = router
            .clone()
            .oneshot(post_empty(&format!("/api/clients/{id}/reissue"), &tui))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert!(set_cookie(&resp).is_none(), "only web-ui gets a cookie");
        let reissued = body_json(resp).await;
        let cred2 = reissued["credential"].as_str().unwrap().to_owned();
        assert_ne!(cred2, cred);
        let (status, _) = whoami(router.clone(), &[("x-cctrace-token", &cred)]).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        let (status, _) = whoami(router.clone(), &[("x-cctrace-token", &cred2)]).await;
        assert_eq!(status, StatusCode::OK);

        // Revoke: nothing of this client works any more.
        let resp = router
            .clone()
            .oneshot(post_empty(&format!("/api/clients/{id}/revoke"), &tui))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let revoked = body_json(resp).await;
        assert!(revoked["revoked_at"].as_i64().is_some());
        let (status, _) = whoami(router.clone(), &[("x-cctrace-token", &cred2)]).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);

        // Bad ids → 400, and management routes are themselves gated.
        let resp = router
            .clone()
            .oneshot(post_empty("/api/clients/not-a-uuid/revoke", &tui))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let resp = router
            .oneshot(post_empty(&format!("/api/clients/{id}/reissue"), &cred2))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn reissuing_web_ui_sets_the_new_cookie_and_swaps_the_live_credential() {
        let state = test_state(enabled());
        let old_web = credential_for(&state, WEB_UI);
        let web_id = state
            .app_state
            .clients_snapshot()
            .into_iter()
            .find(|c| c.name == WEB_UI)
            .unwrap()
            .id;
        let router = build_router(state.clone(), None);

        let resp = router
            .clone()
            .oneshot(post_empty(
                &format!("/api/clients/{web_id}/reissue"),
                &old_web,
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let cookie = set_cookie(&resp).expect("web-ui reissue sets the cookie");
        let issued = body_json(resp).await;
        let new_web = issued["credential"].as_str().unwrap().to_owned();
        assert!(cookie.starts_with(&format!("cctrace_token={new_web};")));
        assert!(cookie.contains("HttpOnly"));
        assert_eq!(
            state.app_state.web_ui_credential().as_deref(),
            Some(new_web.as_str())
        );

        let (status, _) = whoami(router.clone(), &[("x-cctrace-token", &old_web)]).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        let cookie_hdr = format!("cctrace_token={new_web}");
        let (status, json) = whoami(router, &[("cookie", &cookie_hdr)]).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(json["client"]["name"], "web-ui");
    }

    #[tokio::test]
    async fn disabled_mode_refuses_to_manage_clients() {
        let resp = api_router(AuthMode::Disabled)
            .oneshot(post_json(
                "/api/clients",
                "",
                serde_json::json!({ "name": "x" }),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        assert!(body_json(resp).await["error"]
            .as_str()
            .unwrap()
            .contains("disabled"));
    }

    #[tokio::test]
    async fn credential_registered_by_another_process_is_accepted_after_disk_refresh() {
        let root = tempfile::tempdir().unwrap();
        let state = test_state(enabled());
        state
            .app_state
            .set_config_root(Some(root.path().to_path_buf()));

        // "Another process" shares the registry file: start from this one's
        // clients, add a new client, persist.
        let mut other = ClientRegistry {
            clients: state.app_state.clients_snapshot(),
        };
        let client = other.register("sidecar", crate::clients::now()).unwrap();
        other
            .save(&crate::auth::registry_path(root.path()))
            .unwrap();
        let cred = crate::auth::issue_credential(&client, KEY, client.issued_at);

        let (status, json) = whoami(
            build_router(state.clone(), None),
            &[("x-cctrace-token", &cred)],
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(json["client"]["name"], "sidecar");
        assert!(state
            .app_state
            .clients_snapshot()
            .iter()
            .any(|c| c.name == "sidecar"));
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

    fn static_router(auth: AuthMode, dir: &tempfile::TempDir) -> Router {
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
    async fn static_fallback_serves_without_credential() {
        let dir = static_dir();
        let resp = static_router(enabled(), &dir)
            .oneshot(get_with("/app.js", &[("host", "localhost:1421")]))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert!(set_cookie(&resp).is_none(), "assets never carry the cookie");
    }

    #[tokio::test]
    async fn static_index_sets_web_ui_cookie_for_loopback_host() {
        let dir = static_dir();
        let state = test_state(enabled());
        let web = state.app_state.web_ui_credential().unwrap();
        let resp = build_router(state, Some(dir.path().to_string_lossy().to_string()))
            .oneshot(get_with("/", &[("host", "localhost:1421")]))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let cookie = set_cookie(&resp).unwrap();
        assert!(cookie.starts_with(&format!("cctrace_token={web};")));
        assert!(cookie.contains("HttpOnly"));
        assert!(cookie.contains("SameSite=Strict"));
        assert_eq!(crate::jwt::verify(&web, KEY).unwrap().name, "web-ui");
    }

    /// Any date the file cannot postdate, so `ServeDir` answers `304`.
    const FAR_FUTURE: &str = "Wed, 21 Oct 2099 07:28:00 GMT";

    #[tokio::test]
    async fn static_index_is_marked_no_cache() {
        let dir = static_dir();
        let resp = static_router(enabled(), &dir)
            .oneshot(get_with("/", &[("host", "localhost:1421")]))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        // The credential rides on the shell, so a browser must never serve it
        // from cache without asking.
        assert_eq!(
            resp.headers().get(header::CACHE_CONTROL).unwrap(),
            "no-cache"
        );
    }

    #[tokio::test]
    async fn revalidated_index_still_sets_web_ui_cookie() {
        let dir = static_dir();
        let state = test_state(enabled());
        let web = state.app_state.web_ui_credential().unwrap();
        let resp = build_router(state, Some(dir.path().to_string_lossy().to_string()))
            .oneshot(get_with(
                "/",
                &[
                    ("host", "localhost:1421"),
                    ("if-modified-since", FAR_FUTURE),
                ],
            ))
            .await
            .unwrap();
        // A `304` carries no `Content-Type`; the cookie must ride along anyway,
        // or a browser with a cached shell is left with no credential.
        assert_eq!(resp.status(), StatusCode::NOT_MODIFIED);
        assert!(set_cookie(&resp)
            .unwrap()
            .starts_with(&format!("cctrace_token={web};")));
    }

    #[tokio::test]
    async fn revalidated_asset_gets_no_cookie() {
        let dir = static_dir();
        let resp = static_router(enabled(), &dir)
            .oneshot(get_with(
                "/app.js",
                &[
                    ("host", "localhost:1421"),
                    ("if-modified-since", FAR_FUTURE),
                ],
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_MODIFIED);
        assert!(set_cookie(&resp).is_none(), "assets never carry the cookie");
        assert!(resp.headers().get(header::CACHE_CONTROL).is_none());
    }

    #[tokio::test]
    async fn static_index_sets_no_cookie_for_unknown_host() {
        let dir = static_dir();
        let resp = static_router(enabled(), &dir)
            .oneshot(get_with("/", &[("host", "attacker.example:1421")]))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert!(set_cookie(&resp).is_none());
    }

    #[tokio::test]
    async fn static_index_sets_cookie_for_settings_allowlisted_host() {
        let dir = static_dir();
        let state = test_state(enabled());
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
        let resp = static_router(AuthMode::Disabled, &dir)
            .oneshot(get_with("/", &[("host", "localhost:1421")]))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert!(set_cookie(&resp).is_none());
    }

    #[tokio::test]
    async fn static_index_sets_no_cookie_once_web_ui_is_revoked() {
        let dir = static_dir();
        let state = test_state(enabled());
        let web_id = state
            .app_state
            .clients_snapshot()
            .into_iter()
            .find(|c| c.name == WEB_UI)
            .unwrap()
            .id;
        state.app_state.revoke_client(web_id).unwrap();
        let resp = build_router(state, Some(dir.path().to_string_lossy().to_string()))
            .oneshot(get_with("/", &[("host", "localhost:1421")]))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert!(set_cookie(&resp).is_none());
    }

    #[tokio::test]
    async fn api_routes_stay_gated_when_static_fallback_is_mounted() {
        let dir = static_dir();
        let resp = static_router(enabled(), &dir)
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
