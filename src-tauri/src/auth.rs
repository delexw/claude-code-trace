//! Shared-token client verification for the local HTTP API.
//!
//! Every `/api/*` route requires callers to present a secret token so that only
//! *accepted clients* — the bundled web UI, the Python TUI, and any tool the
//! user has handed the token to — can reach the backend. Without this, any
//! local process (or, when Docker binds `0.0.0.0`, any LAN host) could read
//! session transcripts, rewrite `settings.json`, or trigger the `git` /
//! `osascript` side effects some endpoints have.
//!
//! Resolution at startup (see [`resolve_api_auth`]):
//!
//! 1. `CCTRACE_API_AUTH=off` disables verification entirely (loud warning).
//! 2. `CCTRACE_API_TOKEN=<token>` uses that token verbatim (not rotatable).
//! 3. Otherwise the token lives in `config_dir()/claude-code-trace/api-token`
//!    (mode `0600` on unix). It is generated on first run and re-read on every
//!    later run, so clients that share the same OS user can read it too.
//!
//! Accepted carriers on a request, in this order: `X-CCTrace-Token` header,
//! `Authorization: Bearer`, `?token=` query (browser `EventSource` cannot set
//! headers), and the `cctrace_token` cookie the server itself sets on the
//! Docker same-origin UI (see [`attach_token_cookie`]).

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::extract::{Request, State};
use axum::http::{header, HeaderValue, Method, StatusCode, Uri};
use axum::middleware::Next;
use axum::response::Response;
use rand::RngCore;
use subtle::ConstantTimeEq;

use crate::http_api::{err_response, HttpState};

/// Request header carrying the token (primary carrier for the web UI and TUI).
pub const TOKEN_HEADER: &str = "x-cctrace-token";
/// Cookie the server sets for the same-origin (Docker) browser UI.
pub const TOKEN_COOKIE: &str = "cctrace_token";
/// Query parameter carrier, needed by browser `EventSource` for `/api/events`.
pub const TOKEN_QUERY: &str = "token";
/// Env var that supplies the token verbatim (overrides the token file).
pub const ENV_TOKEN: &str = "CCTRACE_API_TOKEN";
/// Env var that disables verification when set to `off`.
pub const ENV_AUTH: &str = "CCTRACE_API_AUTH";

/// Where the live token came from — drives both the middleware and the
/// Settings UI (an env-provided token cannot be rotated at runtime).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ApiAuth {
    /// `CCTRACE_API_AUTH=off`: every request is accepted.
    Disabled,
    /// Token supplied by `CCTRACE_API_TOKEN`.
    Env(String),
    /// Token read from (or generated into) the token file.
    File(String),
}

impl ApiAuth {
    /// The token requests must present, or `None` when verification is off.
    pub fn token(&self) -> Option<&str> {
        match self {
            ApiAuth::Disabled => None,
            ApiAuth::Env(t) | ApiAuth::File(t) => Some(t.as_str()),
        }
    }

    /// Stable string for the frontend: `"disabled"`, `"env"`, or `"file"`.
    pub fn source(&self) -> &'static str {
        match self {
            ApiAuth::Disabled => "disabled",
            ApiAuth::Env(_) => "env",
            ApiAuth::File(_) => "file",
        }
    }

    pub fn is_enabled(&self) -> bool {
        !matches!(self, ApiAuth::Disabled)
    }
}

/// `config_dir()/claude-code-trace/api-token` — sibling of `settings.json`.
pub fn token_file_path() -> Option<PathBuf> {
    dirs::config_dir().map(|c| c.join("claude-code-trace").join("api-token"))
}

/// 32 random bytes as 64 lowercase hex chars: safe in headers, cookies, and
/// URLs without any escaping.
pub fn generate_token() -> String {
    let mut buf = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut buf);
    buf.iter().map(|b| format!("{b:02x}")).collect()
}

/// Constant-time equality so a wrong token can't be narrowed down byte by byte.
pub fn token_eq(presented: &str, expected: &str) -> bool {
    presented.as_bytes().ct_eq(expected.as_bytes()).into()
}

fn read_token_file(path: &Path) -> Option<String> {
    fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn open_options_0600(opts: &mut OpenOptions) -> &mut OpenOptions {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    opts
}

#[cfg(unix)]
fn enforce_0600(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).map_err(|e| e.to_string())
}

#[cfg(not(unix))]
fn enforce_0600(_path: &Path) -> Result<(), String> {
    // Windows: `%APPDATA%` is already per-user; no POSIX mode bits to set.
    Ok(())
}

/// Read the token file, or create it with a fresh token if it does not exist.
///
/// Creation uses `create_new` (O_EXCL) so that two creators racing on first run
/// — the Rust backend and the Vite dev server's `bin/api-token.mjs`, which
/// Tauri starts *before* the backend — converge on a single token: the loser
/// sees `AlreadyExists` and re-reads what the winner wrote.
pub fn load_or_create_token_file(path: &Path) -> Result<String, String> {
    if let Some(existing) = read_token_file(path) {
        return Ok(existing);
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let mut opts = OpenOptions::new();
    opts.write(true).create_new(true);
    match open_options_0600(&mut opts).open(path) {
        Ok(mut f) => {
            let token = generate_token();
            f.write_all(token.as_bytes())
                .and_then(|()| f.write_all(b"\n"))
                .map_err(|e| e.to_string())?;
            Ok(token)
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => read_token_file(path)
            .ok_or_else(|| format!("api-token file exists but is empty: {}", path.display())),
        Err(e) => Err(format!("cannot create {}: {e}", path.display())),
    }
}

/// Overwrite the token file with a fresh token (Settings → Regenerate) and
/// return the new token. Re-applies `0600` in case the file pre-existed with
/// looser permissions.
pub fn rotate_token_file(path: &Path) -> Result<String, String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let token = generate_token();
    let mut opts = OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    let mut f = open_options_0600(&mut opts)
        .open(path)
        .map_err(|e| format!("cannot write {}: {e}", path.display()))?;
    f.write_all(token.as_bytes())
        .and_then(|()| f.write_all(b"\n"))
        .map_err(|e| e.to_string())?;
    enforce_0600(path)?;
    Ok(token)
}

/// Pure resolution core. `env_auth` is the raw `CCTRACE_API_AUTH` value,
/// `env_token` the raw `CCTRACE_API_TOKEN` value, `path` the token file.
pub fn resolve_api_auth_from(
    env_auth: Option<String>,
    env_token: Option<String>,
    path: Option<&Path>,
) -> Result<ApiAuth, String> {
    if env_auth
        .as_deref()
        .is_some_and(|v| v.trim().eq_ignore_ascii_case("off"))
    {
        return Ok(ApiAuth::Disabled);
    }
    if let Some(t) = env_token
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
    {
        return Ok(ApiAuth::Env(t));
    }
    let path = path.ok_or("no config directory available for the api-token file")?;
    load_or_create_token_file(path).map(ApiAuth::File)
}

/// Resolve the live auth mode from the environment and the token file, logging
/// where the token lives (never the token itself). Fails *closed*: if the file
/// cannot be read or created, the server runs with an unpersisted random token
/// rather than unauthenticated.
pub fn resolve_api_auth() -> ApiAuth {
    let path = token_file_path();
    let resolved = resolve_api_auth_from(
        std::env::var(ENV_AUTH).ok(),
        std::env::var(ENV_TOKEN).ok(),
        path.as_deref(),
    );
    match resolved {
        Ok(ApiAuth::Disabled) => {
            eprintln!(
                "HTTP API: WARNING — client verification is DISABLED ({ENV_AUTH}=off). \
                 Every local process can call the API."
            );
            ApiAuth::Disabled
        }
        Ok(ApiAuth::Env(t)) => {
            eprintln!("HTTP API: client verification on (token from {ENV_TOKEN})");
            ApiAuth::Env(t)
        }
        Ok(ApiAuth::File(t)) => {
            if let Some(p) = &path {
                eprintln!(
                    "HTTP API: client verification on (token file: {})",
                    p.display()
                );
            }
            ApiAuth::File(t)
        }
        Err(e) => {
            eprintln!(
                "HTTP API: could not persist the api-token ({e}); using a one-off token for this \
                 run. Set {ENV_TOKEN} to share a token with clients."
            );
            ApiAuth::Env(generate_token())
        }
    }
}

// ---------------------------------------------------------------------------
// Request-side carriers
// ---------------------------------------------------------------------------

/// Value of `name` in a raw query string. Tokens are hex, so no
/// percent-decoding is needed.
fn query_param(query: &str, name: &str) -> Option<String> {
    query
        .split('&')
        .filter_map(|kv| kv.split_once('='))
        .find(|(k, _)| *k == name)
        .map(|(_, v)| v.to_string())
}

/// Value of `name` in a `Cookie:` header (`a=1; b=2`).
fn cookie_value(header: &str, name: &str) -> Option<String> {
    header
        .split(';')
        .map(str::trim)
        .filter_map(|kv| kv.split_once('='))
        .find(|(k, _)| *k == name)
        .map(|(_, v)| v.trim().to_string())
}

/// Every token candidate the client presented, in precedence order.
fn presented_tokens(req: &Request) -> Vec<String> {
    let mut out = Vec::new();
    let headers = req.headers();
    if let Some(v) = headers.get(TOKEN_HEADER).and_then(|v| v.to_str().ok()) {
        out.push(v.trim().to_string());
    }
    if let Some(v) = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
    {
        if let Some(rest) = v
            .strip_prefix("Bearer ")
            .or_else(|| v.strip_prefix("bearer "))
        {
            out.push(rest.trim().to_string());
        }
    }
    if let Some(q) = req.uri().query() {
        out.extend(query_param(q, TOKEN_QUERY));
    }
    if let Some(c) = headers.get(header::COOKIE).and_then(|v| v.to_str().ok()) {
        out.extend(cookie_value(c, TOKEN_COOKIE));
    }
    out.retain(|t| !t.is_empty());
    out
}

/// Whether a request carries the expected token via any accepted carrier.
fn request_has_token(req: &Request, expected: &str) -> bool {
    presented_tokens(req).iter().any(|p| token_eq(p, expected))
}

/// axum middleware: reject `/api/*` requests that don't carry the live token.
/// `OPTIONS` always passes so CORS preflights (which browsers send without
/// custom headers) are never blocked. Fails closed on a poisoned lock.
pub async fn require_api_token(
    State(state): State<Arc<HttpState>>,
    req: Request,
    next: Next,
) -> Response {
    if req.method() == Method::OPTIONS {
        return next.run(req).await;
    }
    let expected = match state.app_state.api_auth.read() {
        Ok(guard) => guard.token().map(str::to_owned),
        Err(_) => {
            return err_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "auth state unavailable".to_string(),
            )
        }
    };
    match expected {
        None => next.run(req).await,
        Some(tok) if request_has_token(&req, &tok) => next.run(req).await,
        Some(_) => err_response(
            StatusCode::UNAUTHORIZED,
            format!(
                "missing or invalid API token — send an X-CCTrace-Token header or \
                 Authorization: Bearer, or copy the token from Settings > API access \
                 ({ENV_AUTH}=off disables verification)"
            ),
        ),
    }
}

// ---------------------------------------------------------------------------
// Same-origin cookie for the Docker static UI
// ---------------------------------------------------------------------------

/// Host header without its port; IPv6 literals lose their brackets too.
fn strip_port(host: &str) -> &str {
    let host = host.trim();
    if let Some(rest) = host.strip_prefix('[') {
        return rest.split(']').next().unwrap_or(rest);
    }
    match host.rsplit_once(':') {
        // `a:b:c` with more than one colon is a bare IPv6 literal, not host:port.
        Some((h, _)) if !h.contains(':') => h,
        _ => host,
    }
}

/// Host component of an `http(s)://host[:port]` origin string.
fn host_of_origin(origin: &str) -> Option<String> {
    let uri: Uri = origin.parse().ok()?;
    uri.host().map(|h| h.trim_matches(['[', ']']).to_string())
}

/// Is this request `Host` one we're willing to hand the token cookie to?
///
/// Loopback names always qualify. Anything else must match the host part of
/// an allowlisted CORS origin (defaults + `CCTRACE_ALLOWED_ORIGINS` +
/// Settings UI). Without this check, a DNS-rebinding page pointed at a
/// `0.0.0.0`-bound Docker port would be issued the cookie and become a fully
/// authenticated same-origin client.
pub fn host_allowed(host: &str, origins: &[String]) -> bool {
    let h = strip_port(host);
    if h.is_empty() {
        return false;
    }
    if h.eq_ignore_ascii_case("localhost") || h == "127.0.0.1" || h == "::1" {
        return true;
    }
    origins
        .iter()
        .filter_map(|o| host_of_origin(o))
        .any(|oh| oh.eq_ignore_ascii_case(h))
}

/// `Set-Cookie` value carrying the token for the same-origin browser UI.
/// `HttpOnly` keeps page scripts from reading it; `SameSite=Strict` keeps
/// cross-site pages from riding on it. No `Secure` flag: plain `http://` on
/// localhost is the normal deployment.
pub fn token_cookie_header(token: &str) -> Option<HeaderValue> {
    HeaderValue::from_str(&format!(
        "{TOKEN_COOKIE}={token}; Path=/; HttpOnly; SameSite=Strict"
    ))
    .ok()
}

fn is_html(resp: &Response) -> bool {
    resp.headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|ct| ct.starts_with("text/html"))
}

/// axum middleware for the static-asset fallback: attach the token cookie to
/// HTML responses (the SPA shell) when the request `Host` is allowlisted, so
/// the Docker same-origin UI authenticates with zero frontend code.
pub async fn attach_token_cookie(
    State(state): State<Arc<HttpState>>,
    req: Request,
    next: Next,
) -> Response {
    let host_ok = req
        .headers()
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
        .is_some_and(|h| {
            let mut origins = crate::http_api::resolve_allowed_origins();
            if let Ok(g) = state.app_state.settings.lock() {
                origins.extend(g.allowed_origins.iter().cloned());
            }
            host_allowed(h, &origins)
        });
    let mut resp = next.run(req).await;
    if host_ok && is_html(&resp) {
        let token = state
            .app_state
            .api_auth
            .read()
            .ok()
            .and_then(|g| g.token().map(str::to_owned));
        if let Some(cookie) = token.as_deref().and_then(token_cookie_header) {
            resp.headers_mut().append(header::SET_COOKIE, cookie);
        }
    }
    resp
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_token_path() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("api-token");
        (dir, path)
    }

    // -- token generation / comparison -------------------------------------

    #[test]
    fn generate_token_is_64_lowercase_hex() {
        let t = generate_token();
        assert_eq!(t.len(), 64);
        assert!(t
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
    }

    #[test]
    fn generate_token_is_unique() {
        assert_ne!(generate_token(), generate_token());
    }

    #[test]
    fn token_eq_matches_equal_strings() {
        assert!(token_eq("abc123", "abc123"));
    }

    #[test]
    fn token_eq_rejects_different_and_different_length() {
        assert!(!token_eq("abc123", "abc124"));
        assert!(!token_eq("abc12", "abc123"));
        assert!(!token_eq("", "abc123"));
    }

    // -- carriers -----------------------------------------------------------

    #[test]
    fn query_param_extracts_token() {
        assert_eq!(
            query_param("a=1&token=abc&b=2", "token"),
            Some("abc".to_string())
        );
    }

    #[test]
    fn query_param_ignores_other_keys_and_prefixes() {
        assert_eq!(query_param("tokens=abc&x=token", "token"), None);
    }

    #[test]
    fn cookie_value_parses_multi_cookie_header() {
        assert_eq!(
            cookie_value("theme=dark; cctrace_token=abc; other=1", "cctrace_token"),
            Some("abc".to_string())
        );
    }

    #[test]
    fn cookie_value_missing_returns_none() {
        assert_eq!(cookie_value("theme=dark", "cctrace_token"), None);
    }

    fn req_with(headers: &[(&str, &str)], uri: &str) -> Request {
        let mut b = Request::builder().uri(uri);
        for (k, v) in headers {
            b = b.header(*k, *v);
        }
        b.body(axum::body::Body::empty()).unwrap()
    }

    #[test]
    fn presented_tokens_collects_header_bearer_query_and_cookie_in_order() {
        let req = req_with(
            &[
                ("x-cctrace-token", "h"),
                ("authorization", "Bearer b"),
                ("cookie", "cctrace_token=c"),
            ],
            "/api/x?token=q",
        );
        assert_eq!(presented_tokens(&req), vec!["h", "b", "q", "c"]);
    }

    #[test]
    fn presented_tokens_ignores_non_bearer_authorization() {
        let req = req_with(&[("authorization", "Basic abc")], "/api/x");
        assert!(presented_tokens(&req).is_empty());
    }

    #[test]
    fn request_has_token_accepts_any_matching_carrier() {
        let req = req_with(&[("cookie", "cctrace_token=secret")], "/api/x");
        assert!(request_has_token(&req, "secret"));
        assert!(!request_has_token(&req, "other"));
    }

    // -- resolution ---------------------------------------------------------

    #[test]
    fn resolve_env_off_wins_over_everything() {
        let (_d, p) = tmp_token_path();
        let r = resolve_api_auth_from(Some("OFF".into()), Some("tok".into()), Some(&p)).unwrap();
        assert_eq!(r, ApiAuth::Disabled);
        assert!(!p.exists(), "disabled mode must not touch the token file");
    }

    #[test]
    fn resolve_env_token_wins_over_file() {
        let (_d, p) = tmp_token_path();
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(&p, "filetoken\n").unwrap();
        let r = resolve_api_auth_from(None, Some("  envtoken ".into()), Some(&p)).unwrap();
        assert_eq!(r, ApiAuth::Env("envtoken".into()));
    }

    #[test]
    fn resolve_blank_env_token_falls_through_to_file() {
        let (_d, p) = tmp_token_path();
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(&p, "filetoken\n").unwrap();
        let r = resolve_api_auth_from(Some("on".into()), Some("   ".into()), Some(&p)).unwrap();
        assert_eq!(r, ApiAuth::File("filetoken".into()));
    }

    #[test]
    fn resolve_creates_file_when_missing() {
        let (_d, p) = tmp_token_path();
        let r = resolve_api_auth_from(None, None, Some(&p)).unwrap();
        let ApiAuth::File(t) = r else {
            panic!("expected File");
        };
        assert_eq!(t.len(), 64);
        assert_eq!(fs::read_to_string(&p).unwrap().trim(), t);
    }

    #[test]
    fn resolve_without_config_dir_is_an_error() {
        assert!(resolve_api_auth_from(None, None, None).is_err());
    }

    #[test]
    fn load_or_create_reads_existing_trimmed() {
        let (_d, p) = tmp_token_path();
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(&p, "  existing \n").unwrap();
        assert_eq!(load_or_create_token_file(&p).unwrap(), "existing");
    }

    #[test]
    fn load_or_create_is_stable_across_calls() {
        let (_d, p) = tmp_token_path();
        let a = load_or_create_token_file(&p).unwrap();
        let b = load_or_create_token_file(&p).unwrap();
        assert_eq!(a, b);
    }

    #[cfg(unix)]
    #[test]
    fn created_file_has_mode_0600() {
        use std::os::unix::fs::PermissionsExt;
        let (_d, p) = tmp_token_path();
        load_or_create_token_file(&p).unwrap();
        let mode = fs::metadata(&p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[test]
    fn rotate_changes_content() {
        let (_d, p) = tmp_token_path();
        let a = load_or_create_token_file(&p).unwrap();
        let b = rotate_token_file(&p).unwrap();
        assert_ne!(a, b);
        assert_eq!(fs::read_to_string(&p).unwrap().trim(), b);
    }

    #[cfg(unix)]
    #[test]
    fn rotate_enforces_mode_0600_on_a_loose_file() {
        use std::os::unix::fs::PermissionsExt;
        let (_d, p) = tmp_token_path();
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(&p, "loose\n").unwrap();
        fs::set_permissions(&p, fs::Permissions::from_mode(0o644)).unwrap();
        rotate_token_file(&p).unwrap();
        let mode = fs::metadata(&p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }

    // -- ApiAuth ------------------------------------------------------------

    #[test]
    fn api_auth_token_and_source() {
        assert_eq!(ApiAuth::Disabled.token(), None);
        assert_eq!(ApiAuth::Disabled.source(), "disabled");
        assert!(!ApiAuth::Disabled.is_enabled());
        assert_eq!(ApiAuth::Env("e".into()).token(), Some("e"));
        assert_eq!(ApiAuth::Env("e".into()).source(), "env");
        assert_eq!(ApiAuth::File("f".into()).token(), Some("f"));
        assert_eq!(ApiAuth::File("f".into()).source(), "file");
        assert!(ApiAuth::File("f".into()).is_enabled());
    }

    // -- cookie host gate ---------------------------------------------------

    #[test]
    fn strip_port_handles_plain_ipv4_and_ipv6_hosts() {
        assert_eq!(strip_port("localhost:1421"), "localhost");
        assert_eq!(strip_port("localhost"), "localhost");
        assert_eq!(strip_port("127.0.0.1:80"), "127.0.0.1");
        assert_eq!(strip_port("[::1]:1421"), "::1");
        assert_eq!(strip_port("[::1]"), "::1");
        assert_eq!(strip_port("::1"), "::1");
    }

    #[test]
    fn host_allowed_accepts_loopback_without_allowlist() {
        assert!(host_allowed("localhost:1421", &[]));
        assert!(host_allowed("LOCALHOST", &[]));
        assert!(host_allowed("127.0.0.1:1421", &[]));
        assert!(host_allowed("[::1]:1421", &[]));
    }

    #[test]
    fn host_allowed_rejects_unknown_host() {
        let origins = vec!["http://localhost:1420".to_string()];
        assert!(!host_allowed("evil.example", &origins));
        assert!(!host_allowed("192.168.1.20:1421", &origins));
        assert!(!host_allowed("", &origins));
    }

    #[test]
    fn host_allowed_matches_host_of_allowlisted_origin_ignoring_port_and_scheme() {
        let origins = vec![
            "https://cctrace.example.com".to_string(),
            "http://192.168.1.20:1421".to_string(),
        ];
        assert!(host_allowed("cctrace.example.com:1421", &origins));
        assert!(host_allowed("CCTRACE.example.com", &origins));
        assert!(host_allowed("192.168.1.20:9090", &origins));
    }

    #[test]
    fn host_allowed_denies_suffix_and_prefix_tricks() {
        let origins = vec!["https://cctrace.example.com".to_string()];
        assert!(!host_allowed("cctrace.example.com.evil.net", &origins));
        assert!(!host_allowed("evil-cctrace.example.com", &origins));
    }

    #[test]
    fn token_cookie_header_has_httponly_and_samesite_strict() {
        let v = token_cookie_header("abc").unwrap();
        let s = v.to_str().unwrap();
        assert!(s.starts_with("cctrace_token=abc;"));
        assert!(s.contains("HttpOnly"));
        assert!(s.contains("SameSite=Strict"));
        assert!(s.contains("Path=/"));
    }

    #[test]
    fn token_cookie_header_rejects_header_injection() {
        assert!(token_cookie_header("abc\r\nSet-Cookie: evil=1").is_none());
    }
}
