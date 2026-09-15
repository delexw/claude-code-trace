//! Client verification for the local HTTP API.
//!
//! Every `/api/*` route requires the caller to present a signed credential
//! that identifies it as a registered, non-revoked *client* (see
//! `crate::clients`). Without this, any local process — or, when Docker binds
//! `0.0.0.0`, any LAN host — could read session transcripts, rewrite
//! `settings.json`, or trigger the `git` / `osascript` side effects some
//! endpoints have; and with a single shared secret nobody could tell clients
//! apart or revoke just one.
//!
//! Resolution at startup (see [`resolve_auth`]):
//!
//! 1. `CCTRACE_API_AUTH=off` disables verification entirely (loud warning).
//! 2. Otherwise the HS256 signing key lives in `<config root>/api-secret`
//!    (mode `0600`), created on first run; the registry in `clients.json`;
//!    and the built-in clients' credentials in `clients/<name>.jwt`, rewritten
//!    whenever they are missing or no longer valid (reissue, new key).
//! 3. If the config dir is unusable the server runs on an in-memory key
//!    (`ephemeral`): still fail-closed, but only the same-origin web UI (which
//!    receives its credential from this very process) can authenticate.
//!
//! Accepted carriers on a request, in this order: `X-CCTrace-Token` header,
//! `Authorization: Bearer`, `?token=` query (browser `EventSource` cannot set
//! headers), and the `cctrace_token` cookie the server itself sets on the
//! Docker same-origin UI (see [`attach_credential_cookie`]).

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::extract::{Request, State};
use axum::http::{header, HeaderMap, HeaderValue, Method, StatusCode, Uri};
use axum::middleware::Next;
use axum::response::Response;
use rand::Rng;
use serde::Serialize;
use uuid::Uuid;

use crate::clients::{self, Client, ClientRegistry};
use crate::http_api::{err_response, HttpState};
use crate::jwt::{self, Claims};

/// Request header carrying the credential (primary carrier for the web UI and TUI).
pub const TOKEN_HEADER: &str = "x-cctrace-token";
/// Cookie the server sets for the same-origin (Docker) browser UI.
pub const TOKEN_COOKIE: &str = "cctrace_token";
/// Query parameter carrier, needed by browser `EventSource` for `/api/events`.
pub const TOKEN_QUERY: &str = "token";
/// Env var that disables verification when set to `off`.
pub const ENV_AUTH: &str = "CCTRACE_API_AUTH";

/// Where the signing key came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeySource {
    /// `<config root>/api-secret`.
    File,
    /// In-memory only: the config dir could not be used. Credentials minted
    /// this run die with the process, and the TUI/dev server cannot read them.
    Ephemeral,
}

/// Live verification mode, read on every request.
#[derive(Clone, PartialEq, Eq)]
pub enum AuthMode {
    /// `CCTRACE_API_AUTH=off`: every request is accepted, no identity attached.
    Disabled,
    Enabled {
        key: Vec<u8>,
        source: KeySource,
    },
}

/// Never print the signing key, even at debug level.
impl std::fmt::Debug for AuthMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuthMode::Disabled => f.write_str("Disabled"),
            AuthMode::Enabled { key, source } => f
                .debug_struct("Enabled")
                .field("key", &format_args!("<redacted {} bytes>", key.len()))
                .field("source", source)
                .finish(),
        }
    }
}

impl AuthMode {
    pub fn is_enabled(&self) -> bool {
        matches!(self, AuthMode::Enabled { .. })
    }

    pub fn key(&self) -> Option<&[u8]> {
        match self {
            AuthMode::Disabled => None,
            AuthMode::Enabled { key, .. } => Some(key),
        }
    }

    /// Stable string for the frontend: `"disabled"`, `"file"`, or `"ephemeral"`.
    pub fn source(&self) -> &'static str {
        match self {
            AuthMode::Disabled => "disabled",
            AuthMode::Enabled {
                source: KeySource::File,
                ..
            } => "file",
            AuthMode::Enabled {
                source: KeySource::Ephemeral,
                ..
            } => "ephemeral",
        }
    }
}

/// The authenticated caller, attached to the request extensions by
/// [`require_client`] so handlers (e.g. `/api/whoami`) can tell who is asking.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ClientIdentity {
    pub id: Uuid,
    pub name: String,
}

// ---------------------------------------------------------------------------
// Files
// ---------------------------------------------------------------------------

pub fn secret_path(root: &Path) -> PathBuf {
    root.join("api-secret")
}

pub fn registry_path(root: &Path) -> PathBuf {
    root.join("clients.json")
}

/// `<config root>/clients/<name>.jwt` — where a built-in client's credential is
/// kept so the Vite dev server (`web-ui`) and the TUI (`tui`) can read it.
pub fn builtin_credential_path(root: &Path, name: &str) -> PathBuf {
    root.join("clients").join(format!("{name}.jwt"))
}

/// 32 random bytes as 64 lowercase hex chars.
pub fn generate_secret_hex() -> String {
    let mut buf = [0u8; 32];
    rand::rng().fill_bytes(&mut buf);
    buf.iter().map(|b| format!("{b:02x}")).collect()
}

fn hex_to_bytes(hex: &str) -> Option<Vec<u8>> {
    // Byte-offset slicing below is only sound on ASCII; anything else is not
    // hex anyway (and must not panic at startup).
    if !hex.is_ascii() || hex.len() % 2 != 0 || hex.is_empty() {
        return None;
    }
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).ok())
        .collect()
}

/// Trimmed contents of a small secret file, or `None` when missing or empty.
pub(crate) fn read_trimmed(path: &Path) -> Option<String> {
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

const EMPTY_FILE_RETRIES: u32 = 5;
const EMPTY_FILE_RETRY_DELAY: std::time::Duration = std::time::Duration::from_millis(20);

/// Re-read a file another creator has just made, tolerating the moment
/// between its `create_new` and its `write_all`.
fn read_trimmed_with_retry(path: &Path) -> Option<String> {
    for attempt in 0..=EMPTY_FILE_RETRIES {
        if let Some(t) = read_trimmed(path) {
            return Some(t);
        }
        if attempt < EMPTY_FILE_RETRIES {
            std::thread::sleep(EMPTY_FILE_RETRY_DELAY);
        }
    }
    None
}

/// Read the signing key file, or create it with a fresh key if it does not
/// exist. `create_new` (O_EXCL) makes two backends racing on first run converge
/// on one key: the loser re-reads what the winner wrote.
pub(crate) fn load_or_create_secret(path: &Path) -> Result<String, String> {
    if let Some(existing) = read_trimmed(path) {
        return Ok(existing);
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let mut opts = OpenOptions::new();
    opts.write(true).create_new(true);
    match open_options_0600(&mut opts).open(path) {
        Ok(mut f) => {
            let secret = generate_secret_hex();
            f.write_all(secret.as_bytes())
                .and_then(|()| f.write_all(b"\n"))
                .map_err(|e| e.to_string())?;
            Ok(secret)
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => read_trimmed_with_retry(path)
            .ok_or_else(|| format!("{} exists but is empty", path.display())),
        Err(e) => Err(format!("cannot create {}: {e}", path.display())),
    }
}

/// Write `bytes` to `path` via a sibling temp file (created `0600`) + rename,
/// so concurrent readers see either the old contents or the new, never a
/// truncated file. Used for the registry and the built-in credential files.
pub(crate) fn write_private_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("path has no parent: {}", path.display()))?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("secret");
    let tmp = parent.join(format!("{file_name}.{}.tmp", &generate_secret_hex()[..8]));
    let mut opts = OpenOptions::new();
    opts.write(true).create_new(true);
    let mut write = || -> Result<(), String> {
        let mut f = open_options_0600(&mut opts)
            .open(&tmp)
            .map_err(|e| format!("cannot write {}: {e}", tmp.display()))?;
        f.write_all(bytes)
            .and_then(|()| f.sync_all())
            .map_err(|e| e.to_string())?;
        fs::rename(&tmp, path).map_err(|e| format!("cannot replace {}: {e}", path.display()))
    };
    if let Err(e) = write() {
        let _ = fs::remove_file(&tmp);
        return Err(e);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Startup resolution
// ---------------------------------------------------------------------------

/// Everything `AppState` needs to enforce client verification.
#[derive(Clone)]
pub struct ResolvedAuth {
    pub mode: AuthMode,
    pub registry: ClientRegistry,
    /// The `web-ui` client's current credential (issued as the same-origin
    /// cookie), or `None` when disabled / revoked.
    pub web_ui_credential: Option<String>,
    /// `Some` when the registry and built-in credentials are persisted.
    pub config_root: Option<PathBuf>,
    /// Non-fatal problems worth logging.
    pub warnings: Vec<String>,
}

/// Mint a credential for `client` at `iat`.
pub fn issue_credential(client: &Client, key: &[u8], iat: i64) -> String {
    jwt::sign(
        &Claims {
            sub: client.id.to_string(),
            name: client.name.clone(),
            iat,
        },
        key,
    )
}

/// Does `credential` (typically read from `clients/<name>.jwt`) still
/// authenticate `client` under `key`?
fn credential_is_current(credential: &str, client: &Client, key: &[u8]) -> bool {
    jwt::verify(credential, key).is_ok_and(|c| {
        c.sub == client.id.to_string() && c.iat >= client.issued_at && !client.is_revoked()
    })
}

/// Make sure every built-in client has a valid credential file under `root`,
/// (re)writing files that are missing or stale. Returns the `web-ui`
/// credential when that client is active.
fn ensure_builtin_credentials(
    root: &Path,
    registry: &ClientRegistry,
    key: &[u8],
    warnings: &mut Vec<String>,
) -> Option<String> {
    let mut web_ui = None;
    for client in registry.clients.iter().filter(|c| c.builtin) {
        if client.is_revoked() {
            continue;
        }
        let path = builtin_credential_path(root, &client.name);
        let credential = match read_trimmed(&path) {
            Some(existing) if credential_is_current(&existing, client, key) => existing,
            _ => {
                let fresh = issue_credential(client, key, clients::now().max(client.issued_at));
                if let Err(e) = write_private_atomic(&path, format!("{fresh}\n").as_bytes()) {
                    warnings.push(format!("could not write {}: {e}", path.display()));
                }
                fresh
            }
        };
        if client.name == clients::WEB_UI {
            web_ui = Some(credential);
        }
    }
    web_ui
}

/// Pure core of [`resolve_auth`]: `env_auth` is the raw `CCTRACE_API_AUTH`
/// value, `root` the config directory (or `None` when there is none).
pub fn resolve_auth_from(env_auth: Option<String>, root: Option<&Path>) -> ResolvedAuth {
    let mut warnings = Vec::new();
    if env_auth
        .as_deref()
        .is_some_and(|v| v.trim().eq_ignore_ascii_case("off"))
    {
        return ResolvedAuth {
            mode: AuthMode::Disabled,
            registry: ClientRegistry::default(),
            web_ui_credential: None,
            config_root: root.map(Path::to_path_buf),
            warnings,
        };
    }

    // Signing key: file-backed when possible, ephemeral otherwise (fail closed).
    let (key, source) = match root {
        Some(root) => match load_or_create_secret(&secret_path(root)) {
            Ok(hex) => match hex_to_bytes(&hex) {
                Some(bytes) if bytes.len() >= 16 => (bytes, KeySource::File),
                _ => {
                    warnings.push(format!(
                        "{} is not a hex key; using a one-off key for this run",
                        secret_path(root).display()
                    ));
                    (
                        hex_to_bytes(&generate_secret_hex()).unwrap_or_default(),
                        KeySource::Ephemeral,
                    )
                }
            },
            Err(e) => {
                warnings.push(format!("could not persist the signing key ({e})"));
                (
                    hex_to_bytes(&generate_secret_hex()).unwrap_or_default(),
                    KeySource::Ephemeral,
                )
            }
        },
        None => {
            warnings.push("no config directory available".into());
            (
                hex_to_bytes(&generate_secret_hex()).unwrap_or_default(),
                KeySource::Ephemeral,
            )
        }
    };

    // Registry + built-ins.
    let persisted_root = root.filter(|_| source == KeySource::File);
    let mut registry = match persisted_root {
        Some(root) => ClientRegistry::load(&registry_path(root)).unwrap_or_else(|e| {
            // Never overwrite a registry we could not read: move it aside so
            // the user's clients can be recovered, then start fresh (every
            // previously issued credential stops working — say so loudly).
            let path = registry_path(root);
            let backup = path.with_file_name(format!("clients.json.corrupt-{}", clients::now()));
            match fs::rename(&path, &backup) {
                Ok(()) => warnings.push(format!(
                    "{e}; moved it to {} and started a fresh client registry — every previously \
                     issued credential is now invalid",
                    backup.display()
                )),
                Err(re) => warnings.push(format!(
                    "{e}; could not move it aside ({re}); starting a fresh client registry — \
                     every previously issued credential is now invalid"
                )),
            }
            ClientRegistry::default()
        }),
        None => ClientRegistry::default(),
    };
    if registry.ensure_builtins(clients::now()) {
        if let Some(root) = persisted_root {
            if let Err(e) = registry.save(&registry_path(root)) {
                warnings.push(format!("could not save the client registry: {e}"));
            }
        }
    }

    let web_ui_credential = match persisted_root {
        Some(root) => ensure_builtin_credentials(root, &registry, &key, &mut warnings),
        None => registry
            .find_by_name(clients::WEB_UI)
            .filter(|c| !c.is_revoked())
            .map(|c| issue_credential(c, &key, clients::now())),
    };

    ResolvedAuth {
        mode: AuthMode::Enabled { key, source },
        registry,
        web_ui_credential,
        config_root: persisted_root.map(Path::to_path_buf),
        warnings,
    }
}

/// Resolve the live verification setup from the environment and the config
/// dir, logging where things live (never any secret).
pub fn resolve_auth() -> ResolvedAuth {
    let root = crate::settings::config_root();
    let resolved = resolve_auth_from(std::env::var(ENV_AUTH).ok(), root.as_deref());
    for w in &resolved.warnings {
        eprintln!("HTTP API: WARNING — {w}");
    }
    match &resolved.mode {
        AuthMode::Disabled => eprintln!(
            "HTTP API: WARNING — client verification is DISABLED ({ENV_AUTH}=off). \
             Every local process can call the API."
        ),
        AuthMode::Enabled {
            source: KeySource::File,
            ..
        } => {
            if let Some(root) = &resolved.config_root {
                eprintln!(
                    "HTTP API: client verification on ({} clients registered; config: {})",
                    resolved.registry.clients.len(),
                    root.display()
                );
            }
        }
        AuthMode::Enabled {
            source: KeySource::Ephemeral,
            ..
        } => eprintln!(
            "HTTP API: client verification on with a ONE-OFF key: the config directory is \
             unusable, so only this process's own web UI can authenticate. Fix the config \
             directory and restart."
        ),
    }
    resolved
}

// ---------------------------------------------------------------------------
// Request-side carriers
// ---------------------------------------------------------------------------

/// Decode `%XX` escapes (as produced by `encodeURIComponent`). A `+` is left
/// literal. Malformed escapes are passed through unchanged.
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            // Work on bytes: slicing the `&str` at byte offsets would panic
            // when a multi-byte character follows the `%` (an attacker-chosen
            // query string reaches this code unauthenticated).
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok();
            if let Some(b) = hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
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

/// Value of `name` in a raw query string, percent-decoded.
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

/// Every credential candidate the client presented, in precedence order.
fn presented_credentials(req: &Request) -> Vec<String> {
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
    if cookie_carrier_allowed(headers) {
        if let Some(c) = headers.get(header::COOKIE).and_then(|v| v.to_str().ok()) {
            out.extend(cookie_value(c, TOKEN_COOKIE));
        }
    }
    out.retain(|t| !t.is_empty());
    out
}

/// The cookie is only meant for the same-origin UI. `SameSite=Strict` already
/// keeps cross-*site* pages from riding on it, but a page on another port of
/// the same host is same-*site*: a `<form method=POST>` auto-submitted from
/// `localhost:<other>` would carry the cookie to a body-less mutating route
/// such as `/api/clients/{id}/revoke` (a simple request, so CORS never blocks
/// it). Browsers label such requests `Sec-Fetch-Site: same-site` (or
/// `cross-site`); only `same-origin` / `none` (typed URL) — or no header at
/// all, for non-browser and older clients — may use the cookie carrier.
fn cookie_carrier_allowed(headers: &HeaderMap) -> bool {
    match headers.get("sec-fetch-site").and_then(|v| v.to_str().ok()) {
        None => true,
        Some(site) => matches!(site.trim(), "same-origin" | "none"),
    }
}

/// axum middleware: reject `/api/*` requests that don't carry a valid
/// credential of a registered, non-revoked client. Attaches the caller's
/// [`ClientIdentity`] to the request on success. `OPTIONS` always passes so
/// CORS preflights (sent without custom headers) are never blocked.
pub async fn require_client(
    State(state): State<Arc<HttpState>>,
    mut req: Request,
    next: Next,
) -> Response {
    if req.method() == Method::OPTIONS {
        return next.run(req).await;
    }
    let key = match state.app_state.auth.read() {
        Ok(guard) => guard.key().map(<[u8]>::to_vec),
        Err(_) => {
            return err_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "auth state unavailable".to_string(),
            )
        }
    };
    let Some(key) = key else {
        return next.run(req).await; // disabled
    };
    for presented in presented_credentials(&req) {
        let Ok(claims) = jwt::verify(&presented, &key) else {
            continue;
        };
        if let Some(identity) = state.app_state.authenticate(&claims) {
            req.extensions_mut().insert(identity);
            return next.run(req).await;
        }
    }
    err_response(
        StatusCode::UNAUTHORIZED,
        "invalid or revoked client credential — register a client (or reissue this one) in \
         Settings > Accepted clients and send its credential as an X-CCTrace-Token header or \
         Authorization: Bearer"
            .to_string(),
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

/// Is this request `Host` one we're willing to hand the web UI's credential to?
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

/// `Set-Cookie` value carrying the web UI's credential. `HttpOnly` keeps page
/// scripts from reading it; `SameSite=Strict` keeps cross-site pages from
/// riding on it. No `Secure` flag: plain `http://` on localhost is the normal
/// deployment.
pub fn credential_cookie_header(credential: &str) -> Option<HeaderValue> {
    HeaderValue::from_str(&format!(
        "{TOKEN_COOKIE}={credential}; Path=/; HttpOnly; SameSite=Strict"
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

/// Does this request path resolve to the SPA shell? `/` and any other
/// directory, which `ServeDir` answers with that directory's `index.html`,
/// plus an explicit `.html` request. A `304` has no `Content-Type` to
/// recognise the shell by (see [`is_shell_response`]), so its path is all we
/// have.
fn is_shell_path(path: &str) -> bool {
    path.ends_with('/') || path.ends_with(".html")
}

/// Is this the SPA shell — the one response that carries the credential?
///
/// A `200` is identified by its content type. A `304 Not Modified` carries no
/// body and no `Content-Type` (it is only headers), so a revalidated shell is
/// identified by its request path instead. Missing this case is what made the
/// UI break after the browser had cached `index.html`: the shell came back
/// `304`, no `Set-Cookie` rode along, and — the cookie being session-scoped —
/// the tab was left with no credential at all.
fn is_shell_response(resp: &Response, shell_path: bool) -> bool {
    is_html(resp) || (resp.status() == StatusCode::NOT_MODIFIED && shell_path)
}

/// axum middleware for the static-asset fallback: attach the `web-ui` client's
/// credential as a cookie to the SPA shell when the request `Host` is
/// allowlisted, so the Docker same-origin UI authenticates with zero frontend
/// code. No cookie when `web-ui` is revoked or verification is off.
///
/// The shell is also marked `no-cache`, because the credential rides on it: a
/// browser that serves the shell straight out of its cache never asks the
/// server anything, so it never receives a cookie either. `no-cache` still
/// allows storing and revalidating (a `304` keeps the transfer cheap and now
/// carries the cookie too) — it only forbids using the stored copy blind.
pub async fn attach_credential_cookie(
    State(state): State<Arc<HttpState>>,
    req: Request,
    next: Next,
) -> Response {
    // Only the shell can receive the cookie, so defer the allowlist work until
    // the response is known — asset requests skip it entirely.
    let host = req
        .headers()
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
        .map(str::to_owned);
    let shell_path = is_shell_path(req.uri().path());
    let mut resp = next.run(req).await;
    if !is_shell_response(&resp, shell_path) {
        return resp;
    }
    resp.headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    if host.is_some_and(|h| host_allowed(&h, &allowed_origins(&state))) {
        if let Some(cookie) = state
            .app_state
            .web_ui_credential()
            .as_deref()
            .and_then(credential_cookie_header)
        {
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

    const KEY: &[u8] = b"0123456789abcdef0123456789abcdef";

    fn tmp_root() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    // -- key file --------------------------------------------------------------

    #[test]
    fn non_ascii_input_never_panics_the_decoders() {
        assert_eq!(hex_to_bytes("aéb"), None);
        assert_eq!(hex_to_bytes("éé"), None);
        assert_eq!(percent_decode("%aé"), "%aé");
        assert_eq!(percent_decode("é%41"), "éA");
        assert_eq!(percent_decode("%"), "%");
        assert_eq!(percent_decode("%2"), "%2");
    }

    #[test]
    fn debug_output_never_contains_the_key() {
        let mode = AuthMode::Enabled {
            key: KEY.to_vec(),
            source: KeySource::File,
        };
        let shown = format!("{mode:?}");
        assert!(
            !shown.contains(std::str::from_utf8(KEY).unwrap()),
            "{shown}"
        );
        assert!(shown.contains("redacted"), "{shown}");
    }

    #[test]
    fn corrupt_registry_is_moved_aside_not_overwritten() {
        let root = tmp_root();
        let path = registry_path(root.path());
        fs::write(&path, "{ definitely not json").unwrap();

        let resolved = resolve_auth_from(None, Some(root.path()));
        assert!(resolved.mode.is_enabled());
        assert_eq!(resolved.registry.clients.len(), 2, "fresh built-ins");
        assert!(
            resolved.warnings.iter().any(|w| w.contains("moved it to")),
            "{:?}",
            resolved.warnings
        );
        let backups: Vec<_> = fs::read_dir(root.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with("clients.json.corrupt-")
            })
            .collect();
        assert_eq!(backups.len(), 1);
        assert_eq!(
            fs::read_to_string(backups[0].path()).unwrap(),
            "{ definitely not json",
            "original contents preserved for recovery"
        );
        assert!(ClientRegistry::load(&path).is_ok(), "new registry parses");
    }

    #[test]
    fn cookie_carrier_is_refused_for_same_site_and_cross_site_fetches() {
        let mut headers = HeaderMap::new();
        assert!(cookie_carrier_allowed(&headers), "no header: non-browser");
        headers.insert("sec-fetch-site", HeaderValue::from_static("same-origin"));
        assert!(cookie_carrier_allowed(&headers));
        headers.insert("sec-fetch-site", HeaderValue::from_static("none"));
        assert!(cookie_carrier_allowed(&headers));
        headers.insert("sec-fetch-site", HeaderValue::from_static("same-site"));
        assert!(!cookie_carrier_allowed(&headers));
        headers.insert("sec-fetch-site", HeaderValue::from_static("cross-site"));
        assert!(!cookie_carrier_allowed(&headers));
    }

    #[test]
    fn generate_secret_is_64_lowercase_hex_and_unique() {
        let a = generate_secret_hex();
        assert_eq!(a.len(), 64);
        assert!(a
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
        assert_ne!(a, generate_secret_hex());
        assert_eq!(hex_to_bytes(&a).unwrap().len(), 32);
    }

    #[test]
    fn the_signing_key_comes_from_a_csprng() {
        // The signing key's only security property is that it is unguessable,
        // and nothing else in the build checks that its source is cryptographic.
        fn require_crypto<R: rand::CryptoRng>(_: &R) {}
        require_crypto(&rand::rng());
    }

    #[test]
    fn hex_to_bytes_rejects_odd_or_non_hex() {
        assert!(hex_to_bytes("abc").is_none());
        assert!(hex_to_bytes("zz").is_none());
        assert!(hex_to_bytes("").is_none());
        assert_eq!(hex_to_bytes("00ff"), Some(vec![0, 255]));
    }

    #[test]
    fn load_or_create_secret_creates_then_reads_back_stably() {
        let dir = tmp_root();
        let path = dir.path().join("nested").join("api-secret");
        let a = load_or_create_secret(&path).unwrap();
        let b = load_or_create_secret(&path).unwrap();
        assert_eq!(a, b);
        assert_eq!(fs::read_to_string(&path).unwrap().trim(), a);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
    }

    #[test]
    fn load_or_create_secret_errors_on_a_persistently_empty_file() {
        let dir = tmp_root();
        let path = dir.path().join("api-secret");
        fs::write(&path, "\n").unwrap();
        assert!(load_or_create_secret(&path).unwrap_err().contains("empty"));
    }

    #[test]
    fn write_private_atomic_leaves_no_temp_file_and_is_0600() {
        let dir = tmp_root();
        let path = dir.path().join("sub").join("clients.json");
        write_private_atomic(&path, b"{}").unwrap();
        write_private_atomic(&path, b"{\"clients\":[]}").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "{\"clients\":[]}");
        let siblings: Vec<_> = fs::read_dir(path.parent().unwrap())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
            .collect();
        assert_eq!(siblings, vec!["clients.json"], "{siblings:?}");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
    }

    // -- resolution ------------------------------------------------------------

    #[test]
    fn resolve_off_disables_and_touches_nothing() {
        let dir = tmp_root();
        let r = resolve_auth_from(Some(" OFF ".into()), Some(dir.path()));
        assert_eq!(r.mode, AuthMode::Disabled);
        assert!(r.registry.clients.is_empty());
        assert!(r.web_ui_credential.is_none());
        assert!(!secret_path(dir.path()).exists());
        assert!(!registry_path(dir.path()).exists());
    }

    #[test]
    fn resolve_creates_key_registry_and_builtin_credentials() {
        let dir = tmp_root();
        let r = resolve_auth_from(None, Some(dir.path()));
        assert_eq!(r.mode.source(), "file");
        assert!(r.warnings.is_empty(), "{:?}", r.warnings);
        assert_eq!(r.config_root.as_deref(), Some(dir.path()));
        assert_eq!(r.registry.clients.len(), 2);
        assert!(secret_path(dir.path()).exists());
        assert!(registry_path(dir.path()).exists());

        let key = r.mode.key().unwrap();
        for name in clients::BUILTIN_NAMES {
            let path = builtin_credential_path(dir.path(), name);
            let credential = read_trimmed(&path).expect("credential file written");
            let claims = jwt::verify(&credential, key).unwrap();
            let client = r.registry.find_by_name(name).unwrap();
            assert_eq!(claims.sub, client.id.to_string());
            assert_eq!(claims.name, name);
            assert!(claims.iat >= client.issued_at);
        }
        assert_eq!(
            r.web_ui_credential.as_deref(),
            read_trimmed(&builtin_credential_path(dir.path(), clients::WEB_UI)).as_deref()
        );
    }

    #[test]
    fn resolve_is_stable_across_restarts() {
        let dir = tmp_root();
        let first = resolve_auth_from(None, Some(dir.path()));
        let second = resolve_auth_from(None, Some(dir.path()));
        assert_eq!(first.mode, second.mode, "same key");
        assert_eq!(first.registry, second.registry, "same clients");
        assert_eq!(first.web_ui_credential, second.web_ui_credential);
        assert_eq!(
            read_trimmed(&builtin_credential_path(dir.path(), clients::TUI)),
            read_trimmed(&builtin_credential_path(dir.path(), clients::TUI)),
        );
    }

    #[test]
    fn resolve_rewrites_a_stale_builtin_credential() {
        let dir = tmp_root();
        let first = resolve_auth_from(None, Some(dir.path()));
        let path = builtin_credential_path(dir.path(), clients::TUI);
        fs::write(&path, "garbage\n").unwrap();
        let second = resolve_auth_from(None, Some(dir.path()));
        let credential = read_trimmed(&path).unwrap();
        assert_ne!(credential, "garbage");
        assert!(credential_is_current(
            &credential,
            second.registry.find_by_name(clients::TUI).unwrap(),
            first.mode.key().unwrap()
        ));
    }

    #[test]
    fn resolve_without_config_dir_is_ephemeral_but_enabled() {
        let r = resolve_auth_from(None, None);
        assert_eq!(r.mode.source(), "ephemeral");
        assert!(r.mode.is_enabled());
        assert!(r.config_root.is_none());
        assert_eq!(
            r.registry.clients.len(),
            2,
            "built-ins still exist in memory"
        );
        assert!(
            r.web_ui_credential.is_some(),
            "own web UI can still authenticate"
        );
        assert!(!r.warnings.is_empty());
    }

    #[test]
    fn resolve_falls_back_to_ephemeral_when_the_dir_is_unusable() {
        let dir = tmp_root();
        let blocker = dir.path().join("not-a-dir");
        fs::write(&blocker, "x").unwrap();
        let r = resolve_auth_from(None, Some(&blocker.join("cfg")));
        assert_eq!(r.mode.source(), "ephemeral");
        assert!(r.config_root.is_none());
    }

    #[test]
    fn issue_and_check_credential_currency() {
        let mut reg = ClientRegistry::default();
        let client = reg.register("script", 100).unwrap();
        let cred = issue_credential(&client, KEY, 100);
        assert!(credential_is_current(&cred, &client, KEY));
        let reissued = reg.reissue(client.id, 200).unwrap();
        assert!(
            !credential_is_current(&cred, &reissued, KEY),
            "older iat is stale"
        );
        let revoked = reg.revoke(client.id, 300).unwrap();
        let fresh = issue_credential(&revoked, KEY, 400);
        assert!(
            !credential_is_current(&fresh, &revoked, KEY),
            "revoked is never current"
        );
    }

    // -- carriers --------------------------------------------------------------

    #[test]
    fn percent_decode_handles_encoded_reserved_chars() {
        assert_eq!(percent_decode("a%2Fb%2Bc%20d"), "a/b+c d");
        assert_eq!(percent_decode("a+b"), "a+b");
        assert_eq!(percent_decode("bad%zz%4"), "bad%zz%4");
    }

    #[test]
    fn query_param_and_cookie_value_extract_by_name() {
        assert_eq!(
            query_param("a=1&token=x.y.z&b=2", "token"),
            Some("x.y.z".to_string())
        );
        assert_eq!(query_param("tokens=abc", "token"), None);
        assert_eq!(
            cookie_value("theme=dark; cctrace_token=abc; other=1", "cctrace_token"),
            Some("abc".to_string())
        );
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
    fn presented_credentials_collects_all_carriers_in_order() {
        let req = req_with(
            &[
                ("x-cctrace-token", "h"),
                ("authorization", "Bearer b"),
                ("cookie", "cctrace_token=c"),
            ],
            "/api/x?token=q",
        );
        assert_eq!(presented_credentials(&req), vec!["h", "b", "q", "c"]);
        let basic = req_with(&[("authorization", "Basic abc")], "/api/x");
        assert!(presented_credentials(&basic).is_empty());
    }

    // -- cookie host gate ------------------------------------------------------

    #[test]
    fn strip_port_handles_plain_ipv4_and_ipv6_hosts() {
        assert_eq!(strip_port("localhost:1421"), "localhost");
        assert_eq!(strip_port("127.0.0.1:80"), "127.0.0.1");
        assert_eq!(strip_port("[::1]:1421"), "::1");
        assert_eq!(strip_port("::1"), "::1");
    }

    #[test]
    fn host_allowed_rules() {
        assert!(host_allowed("localhost:1421", &[]));
        assert!(host_allowed("[::1]:1421", &[]));
        let origins = vec![
            "https://cctrace.example.com".to_string(),
            "http://192.168.1.20:1421".to_string(),
        ];
        assert!(host_allowed("cctrace.example.com:1421", &origins));
        assert!(host_allowed("192.168.1.20:9090", &origins));
        assert!(!host_allowed("evil.example", &origins));
        assert!(!host_allowed("cctrace.example.com.evil.net", &origins));
        assert!(!host_allowed("", &origins));
    }

    #[test]
    fn shell_paths_are_the_index_and_directories() {
        assert!(is_shell_path("/"));
        assert!(is_shell_path("/index.html"));
        assert!(is_shell_path("/sub/"));
        assert!(!is_shell_path("/app.js"));
        assert!(!is_shell_path("/assets/index-a1b2c3.css"));
        assert!(!is_shell_path("/favicon.ico"));
    }

    #[test]
    fn credential_cookie_header_is_httponly_strict_and_injection_safe() {
        let v = credential_cookie_header("a.b.c").unwrap();
        let s = v.to_str().unwrap();
        assert!(s.starts_with("cctrace_token=a.b.c;"));
        assert!(s.contains("HttpOnly") && s.contains("SameSite=Strict") && s.contains("Path=/"));
        assert!(credential_cookie_header("abc\r\nSet-Cookie: evil=1").is_none());
    }
}
