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
    /// Unpersisted one-off token: the token file could not be read or created
    /// (read-only config dir, empty file…). Verification stays on — fail closed
    /// — but nothing else can learn this token, so the UI must say so instead
    /// of blaming `CCTRACE_API_TOKEN`.
    Ephemeral(String),
}

impl ApiAuth {
    /// The token requests must present, or `None` when verification is off.
    pub fn token(&self) -> Option<&str> {
        match self {
            ApiAuth::Disabled => None,
            ApiAuth::Env(t) | ApiAuth::File(t) | ApiAuth::Ephemeral(t) => Some(t.as_str()),
        }
    }

    /// Stable string for the frontend: `"disabled"`, `"env"`, `"file"`, or
    /// `"ephemeral"`.
    pub fn source(&self) -> &'static str {
        match self {
            ApiAuth::Disabled => "disabled",
            ApiAuth::Env(_) => "env",
            ApiAuth::File(_) => "file",
            ApiAuth::Ephemeral(_) => "ephemeral",
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

/// Trimmed contents of the token file, or `None` when missing or empty.
pub(crate) fn read_token_file(path: &Path) -> Option<String> {
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

/// How long to keep re-reading a token file that exists but is still empty:
/// the creator writes right after its `O_EXCL` create, so the window is tiny.
const EMPTY_FILE_RETRIES: u32 = 5;
const EMPTY_FILE_RETRY_DELAY: std::time::Duration = std::time::Duration::from_millis(20);

/// Re-read a token file that another creator has just made, tolerating the
/// moment between its `create_new` and its `write_all`.
fn read_token_file_with_retry(path: &Path) -> Option<String> {
    for attempt in 0..=EMPTY_FILE_RETRIES {
        if let Some(t) = read_token_file(path) {
            return Some(t);
        }
        if attempt < EMPTY_FILE_RETRIES {
            std::thread::sleep(EMPTY_FILE_RETRY_DELAY);
        }
    }
    None
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
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => read_token_file_with_retry(path)
            .ok_or_else(|| format!("api-token file exists but is empty: {}", path.display())),
        Err(e) => Err(format!("cannot create {}: {e}", path.display())),
    }
}

/// Replace the token file with a fresh token (Settings → Regenerate) and
/// return the new token.
///
/// The new content is written to a sibling temp file (created `0600`) and then
/// renamed over the target, so concurrent readers — the TUI re-reads the file
/// per request and the Vite dev server watches it — see either the old token
/// or the new one, never an empty file. The rename also replaces any looser
/// permissions the old file may have had.
pub fn rotate_token_file(path: &Path) -> Result<String, String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("api-token path has no parent: {}", path.display()))?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let token = generate_token();
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("api-token");
    let tmp = parent.join(format!("{file_name}.{}.tmp", &token[..8]));
    let mut opts = OpenOptions::new();
    opts.write(true).create_new(true);
    let mut write = || -> Result<(), String> {
        let mut f = open_options_0600(&mut opts)
            .open(&tmp)
            .map_err(|e| format!("cannot write {}: {e}", tmp.display()))?;
        f.write_all(token.as_bytes())
            .and_then(|()| f.write_all(b"\n"))
            .and_then(|()| f.sync_all())
            .map_err(|e| e.to_string())?;
        fs::rename(&tmp, path).map_err(|e| format!("cannot replace {}: {e}", path.display()))
    };
    if let Err(e) = write() {
        let _ = fs::remove_file(&tmp);
        return Err(e);
    }
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

/// [`resolve_api_auth_from`] that never fails: a token file that cannot be read
/// or created yields an [`ApiAuth::Ephemeral`] one-off token (fail *closed* —
/// never unauthenticated). Returns the error text alongside for logging.
pub fn resolve_api_auth_with(
    env_auth: Option<String>,
    env_token: Option<String>,
    path: Option<&Path>,
) -> (ApiAuth, Option<String>) {
    match resolve_api_auth_from(env_auth, env_token, path) {
        Ok(auth) => (auth, None),
        Err(e) => (ApiAuth::Ephemeral(generate_token()), Some(e)),
    }
}

/// Resolve the live auth mode from the environment and the token file, logging
/// where the token lives (never the token itself).
pub fn resolve_api_auth() -> ApiAuth {
    let path = token_file_path();
    let (auth, error) = resolve_api_auth_with(
        std::env::var(ENV_AUTH).ok(),
        std::env::var(ENV_TOKEN).ok(),
        path.as_deref(),
    );
    match &auth {
        ApiAuth::Disabled => eprintln!(
            "HTTP API: WARNING — client verification is DISABLED ({ENV_AUTH}=off). \
             Every local process can call the API."
        ),
        ApiAuth::Env(_) => {
            eprintln!("HTTP API: client verification on (token from {ENV_TOKEN})")
        }
        ApiAuth::File(_) => {
            if let Some(p) = &path {
                eprintln!(
                    "HTTP API: client verification on (token file: {})",
                    p.display()
                );
            }
        }
        ApiAuth::Ephemeral(_) => eprintln!(
            "HTTP API: could not persist the api-token ({}); using a one-off token for this run \
             that no client can read. Fix the config directory and restart, or set {ENV_TOKEN}.",
            error.unwrap_or_default()
        ),
    }
    auth
}

// ---------------------------------------------------------------------------
// Request-side carriers
// ---------------------------------------------------------------------------

/// Decode `%XX` escapes (as produced by `encodeURIComponent`). A `+` is left
/// literal — `encodeURIComponent` never emits a raw `+`, so one in the query
/// is part of the token. Malformed escapes are passed through unchanged.
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(b) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| s.to_string())
}

/// Value of `name` in a raw query string, percent-decoded. Generated tokens
/// are plain hex, but a `CCTRACE_API_TOKEN` may contain URL-reserved
/// characters that the web client escapes in the SSE URL.
fn query_param(query: &str, name: &str) -> Option<String> {
    query
        .split('&')
        .filter_map(|kv| kv.split_once('='))
        .find(|(k, _)| *k == name)
        .map(|(_, v)| percent_decode(v))
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
    let Some(expected) = expected else {
        return next.run(req).await;
    };
    if request_has_token(&req, &expected) {
        return next.run(req).await;
    }
    // Mismatch. Another cctrace process sharing the token file (a background
    // `--web` service plus the desktop app, say) — or the user by hand — may
    // have rotated it since this process started. Re-read the file once and
    // re-check before rejecting, so the live token heals instead of every
    // client getting 401 until a restart. Only on a mismatch, so the happy
    // path never touches the disk.
    if state.app_state.refresh_api_token_from_file() {
        let fresh = state
            .app_state
            .api_auth
            .read()
            .ok()
            .and_then(|g| g.token().map(str::to_owned));
        if fresh.is_some_and(|f| request_has_token(&req, &f)) {
            return next.run(req).await;
        }
    }
    err_response(
        StatusCode::UNAUTHORIZED,
        format!(
            "missing or invalid API token — send an X-CCTrace-Token header or \
             Authorization: Bearer, or copy the token from Settings > API access \
             ({ENV_AUTH}=off disables verification)"
        ),
    )
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

/// Static (defaults + env) ∪ live (Settings UI) CORS origins — the same union
/// `http_api::build_cors` checks.
fn allowed_origins(state: &HttpState) -> Vec<String> {
    let mut origins = crate::http_api::resolve_allowed_origins();
    if let Ok(g) = state.app_state.settings.lock() {
        origins.extend(g.allowed_origins.iter().cloned());
    }
    origins
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
    // Only the HTML shell can receive the cookie, so defer the allowlist work
    // (env read, settings lock) until the response type is known — every
    // JS/CSS/image request through the fallback skips it entirely.
    let host = req
        .headers()
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
        .map(str::to_owned);
    let mut resp = next.run(req).await;
    if is_html(&resp) && host.is_some_and(|h| host_allowed(&h, &allowed_origins(&state))) {
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
    fn percent_decode_handles_encoded_reserved_chars() {
        assert_eq!(percent_decode("my%2Fsecret%2Bkey%20x"), "my/secret+key x");
        assert_eq!(percent_decode("plainhex0123"), "plainhex0123");
    }

    #[test]
    fn percent_decode_leaves_plus_and_malformed_escapes_alone() {
        assert_eq!(percent_decode("a+b"), "a+b");
        assert_eq!(percent_decode("bad%zz%4"), "bad%zz%4");
        assert_eq!(percent_decode("%"), "%");
    }

    #[test]
    fn query_param_percent_decodes_the_token() {
        assert_eq!(
            query_param("token=my%2Fsecret%2Bkey", "token"),
            Some("my/secret+key".to_string())
        );
    }

    #[test]
    fn request_has_token_matches_encoded_query_against_raw_env_token() {
        let req = req_with(&[], "/api/events?token=my%2Fsecret%2Bkey");
        assert!(request_has_token(&req, "my/secret+key"));
    }

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
    fn resolve_with_falls_back_to_an_ephemeral_token_when_the_file_is_unusable() {
        // A path *under a regular file* can never be created.
        let dir = tempfile::tempdir().unwrap();
        let blocker = dir.path().join("not-a-dir");
        fs::write(&blocker, "x").unwrap();
        let path = blocker.join("api-token");
        let (auth, err) = resolve_api_auth_with(None, None, Some(&path));
        let ApiAuth::Ephemeral(t) = auth else {
            panic!("expected Ephemeral, got {auth:?}");
        };
        assert_eq!(t.len(), 64);
        assert!(err.is_some());
    }

    #[test]
    fn resolve_with_reports_no_error_on_success() {
        let (_d, p) = tmp_token_path();
        let (auth, err) = resolve_api_auth_with(None, None, Some(&p));
        assert!(matches!(auth, ApiAuth::File(_)));
        assert!(err.is_none());
    }

    #[test]
    fn load_or_create_errors_on_a_persistently_empty_file() {
        let (_d, p) = tmp_token_path();
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(&p, "\n").unwrap();
        let err = load_or_create_token_file(&p).unwrap_err();
        assert!(err.contains("empty"), "{err}");
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
    fn rotate_changes_content_and_leaves_no_temp_file_behind() {
        let (_d, p) = tmp_token_path();
        let a = load_or_create_token_file(&p).unwrap();
        let b = rotate_token_file(&p).unwrap();
        assert_ne!(a, b);
        assert_eq!(fs::read_to_string(&p).unwrap().trim(), b);
        let siblings: Vec<_> = fs::read_dir(p.parent().unwrap())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
            .collect();
        assert_eq!(siblings, vec!["api-token"], "{siblings:?}");
    }

    #[test]
    fn rotate_creates_the_file_when_missing() {
        let (_d, p) = tmp_token_path();
        let t = rotate_token_file(&p).unwrap();
        assert_eq!(fs::read_to_string(&p).unwrap().trim(), t);
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
        assert_eq!(ApiAuth::Ephemeral("x".into()).token(), Some("x"));
        assert_eq!(ApiAuth::Ephemeral("x".into()).source(), "ephemeral");
        assert!(ApiAuth::Ephemeral("x".into()).is_enabled());
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
