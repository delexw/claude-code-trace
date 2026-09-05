# API client verification: shared local token for the HTTP API

## Context

claude-code-trace runs one axum HTTP server (`src-tauri/src/http_api.rs`, default `127.0.0.1:11423`,
Docker `0.0.0.0:1421`) exposing 18 `/api/*` routes plus an SSE stream. Today the **only** gate is a
CORS `Origin` allowlist, which protects browsers only. Any local process, browser extension, or (in
Docker, bound to `0.0.0.0`) any LAN host can call every endpoint. Reachable side effects include
persisting `settings.json`, adding CORS origins to the server's own allowlist, shelling out to
`git -C <cwd>`, reading arbitrary files (`/api/debug-log`, `/api/session/load`) and running
`osascript` (`/api/focus`). `CHANGELOG.md` itself calls it "its unauthenticated local server".

Goal: the API is accessible only from **accepted clients**. Decisions made with the user:

| Question    | Decision                                                                                                                                                |
| ----------- | ------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Mechanism   | One shared local secret token. Resolution: `CCTRACE_API_TOKEN` env > token file `config_dir()/claude-code-trace/api-token` (0600) > generate + persist. |
| Default     | Enforced on every `/api/*` route. Escape hatch `CCTRACE_API_AUTH=off` (loud stderr warning).                                                            |
| Settings UI | New "API access" section: masked token, Show, Copy, Regenerate. New Tauri command + HTTP route.                                                         |

Accepted clients: the bundled web UI (Vite dev on 1420 and the Docker same-origin bundle), the Python
TUI, and any external tool the user hands the token to. The Tauri desktop webview uses IPC, not HTTP,
so it is unaffected.

GitNexus MCP tools required by `CLAUDE.md` are not available in this session; impact was assessed by
reading callers directly (counts below). Re-run `gitnexus_impact` / `gitnexus_detect_changes` if the
MCP server is available at implementation time.

## Design

### Token carriers accepted by the server

Checked in this order, constant-time compare (`subtle::ConstantTimeEq`):

1. `X-CCTrace-Token: <token>` header (primary; used by web UI fetch and TUI)
2. `Authorization: Bearer <token>` (for curl / external tools)
3. `?token=<token>` query param (browser `EventSource` cannot set headers)
4. `cctrace_token` cookie (Docker same-origin browser, set by the server — see below)

Missing/wrong → `401 {"error": "missing or invalid API token — send X-CCTrace-Token / Authorization: Bearer, or see Settings > API access"}`. `OPTIONS` always passes (CORS preflight).

### Backend layout (Rust)

- **New `src-tauri/src/auth.rs`**
  - `enum ApiAuth { Disabled, Env(String), File(String) }` with `token()` and `source() -> "disabled"|"env"|"file"`.
  - `resolve_api_auth_from(env_auth, env_token, path) -> Result<ApiAuth, String>` (pure, testable) and
    `resolve_api_auth()` wrapper reading env + `token_file_path()`. On file error: eprintln and fall back to
    an in-memory random token (fail closed, never `Disabled`). Print the token _file path_ (not the token) at startup; loud warning when disabled.
  - `generate_token()`: 32 random bytes (`rand`) → 64 lowercase hex.
  - `load_or_create_token_file(path)`: read+trim if present; else `create_new(true)` (O_EXCL) with `mode(0o600)` on unix; on `AlreadyExists` re-read (converges with the Vite-side creator, see below).
  - `rotate_token_file(path)`: truncate-write new token, re-apply 0600.
  - `require_api_token` middleware (`from_fn_with_state`), `presented_tokens(req)`, `query_param`, `cookie_value`, `token_eq`.
  - `attach_token_cookie` middleware + `host_allowed(host)` for the Docker static fallback (below).
- **`src-tauri/src/state.rs`**: add `pub api_auth: RwLock<ApiAuth>`; change `AppState::new()` → `AppState::new(api_auth: ApiAuth)` (8 call sites: `lib.rs` ×2, tests ×6; tests pass `ApiAuth::Disabled` or `ApiAuth::File("test-token")`). Never touch the real token file inside `new()`. Add `api_auth_snapshot()` and `regenerate_api_token() -> Result<String, String>` (`File` → rotate file + swap lock; `Env` → error "set by CCTRACE_API_TOKEN"; `Disabled` → error).
- **`src-tauri/src/http_api.rs`**
  - Extract `build_router(state, static_dir) -> Router` from `run_server` so tests can `tower::ServiceExt::oneshot` it.
  - Apply auth with `.route_layer(from_fn_with_state(state, auth::require_api_token))` on the API routes only. `route_layer` wraps path-router entries, not the fallback, so static assets stay public and unknown paths still 404.
  - Static fallback (Docker): wrap `ServeDir` in a sub-router with `.layer(from_fn_with_state(state, auth::attach_token_cookie))`, then `router.fallback_service(static_ui)`.
  - CORS stays outermost (`.layer(build_cors(..))` last). `build_cors`: `allow_headers([CONTENT_TYPE, AUTHORIZATION, "x-cctrace-token"])`.
  - `err_response` → `pub(crate)`.
  - New route `POST /api/settings/token/regenerate` → `api_regenerate_api_token` (protected like every route; appends the new-token `Set-Cookie` so a Docker tab rotates in place).
- **Cookie issuance rule (security-critical)**: `attach_token_cookie` sets `cctrace_token=<tok>; Path=/; HttpOnly; SameSite=Strict` only on `text/html` responses AND only when the request `Host` (port stripped) is `localhost`/`127.0.0.1`/`::1` or matches the host of an entry in the existing origin allowlist union (`resolve_allowed_origins()` + live `Settings.allowed_origins`, same predicate pattern as `build_cors`). Without the Host check a DNS-rebinding page against the `0.0.0.0` Docker port would receive the cookie and be fully authenticated. Consequence: Docker users reaching the UI via LAN IP or reverse-proxy hostname must add it to `CCTRACE_ALLOWED_ORIGINS` (already the documented knob) or pass the token explicitly.
- **`src-tauri/src/commands/settings.rs`**: `SettingsResponse` gains `api_auth_enabled: bool`, `api_token_source: &'static str`, `api_token: Option<String>`. `build_response_pub(&Settings, &ApiAuth)` (10 call sites, mechanical).
- **New `src-tauri/src/commands/api_token.rs`** (mirror `commands/cors.rs`): `regenerate_api_token_impl(&AppState) -> Result<SettingsResponse, String>` + `#[cfg(feature="desktop")] #[tauri::command] regenerate_api_token`. Register in `lib.rs` `generate_handler![]` and add `allow-regenerate-api-token` to `src-tauri/permissions/default.toml` (both the `[default]` list and a `[[permission]]` block; `tests/acl_consistency.rs` enforces).
- **`src-tauri/src/lib.rs`**: `mod auth;` and `run_headless`/`run_desktop` do `let auth = auth::resolve_api_auth(); Arc::new(AppState::new(auth))`.
- **`src-tauri/Cargo.toml`**: add `subtle = "2"`, `rand = "0.8"` (already in lockfile at 0.8.7 via tauri); dev-dep `tower = { version = "0.5", features = ["util"] }` (0.5.3 in lockfile).

### Web UI getting the token

- **Dev/web mode (Vite 1420 → API 11423, cross-origin)**: new `bin/api-token.mjs` (plain Node, shared by Vite and tests) with `configDir()` (win32 `%APPDATA%`, darwin `~/Library/Application Support`, else `$XDG_CONFIG_HOME` or `~/.config` — matches `dirs::config_dir()`), `apiTokenPath()`, `resolveApiToken({env})` with identical precedence (`CCTRACE_API_AUTH=off` → null, env token, read file, else create 64-hex with `flag: "wx", mode: 0o600`, on `EEXIST` re-read). Tauri dev starts Vite _before_ the Rust binary, so Vite may be the first-run creator; O_EXCL on both sides makes them converge.
  `vite.config.ts` adds plugin `cctrace-api-token` with `apply: "serve"` (never bakes a token into the production/Docker bundle): `config()` returns `define: { "import.meta.env.VITE_API_TOKEN": ... }`; `configureServer` watches the token file and calls `server.restart()` on change (Regenerate). Add `VITE_API_TOKEN?: string` to `src/vite-env.d.ts`.
- **Docker same-origin**: no frontend code; the server cookie above is sent automatically by `fetch` and `EventSource`.
- **New `src/lib/apiToken.ts`** (avoids an invoke↔listen import cycle): module-level token initialised from `import.meta.env.VITE_API_TOKEN`, `getApiToken`, `setApiToken`, `authHeaders()`, `withTokenQuery(url)`.
- **`src/lib/invoke.ts`**: `fetchJson` spreads `...authHeaders()`; throw `ApiAuthError` on 401; add route `regenerate_api_token: { method: "POST", path: "/api/settings/token/regenerate" }`.
- **`src/lib/listen.ts`**: `new EventSource(withTokenQuery(`${API_BASE}/api/events`))`; keep a listener registry so `reconnectSse()` can close/recreate/re-attach after Regenerate.
- **`src/App.tsx`** (~line 121, the `get_settings` catch): if `err instanceof ApiAuthError` set an `authError` state and render a top-level banner ("This browser isn't authorised to talk to the local API. Restart `cctrace --web` (dev) or open the UI via localhost / an allowed origin (Docker).") instead of opening Settings.
- **`src/components/SettingsModal.tsx`**: extend the local `SettingsResponse` interface; new "API access" section after the CORS textarea. `disabled` source → hint only. Otherwise read-only `<input type={showToken ? "text" : "password"} aria-label="API token">`, Show/Hide, Copy (`navigator.clipboard.writeText`, as in `SessionActions.tsx:25`), Regenerate as a two-click inline confirm. Disabled with hint when source is `env`. On success: `applyResponse(res)`, `setApiToken(res.api_token)`, `if (!isTauri) reconnectSse()`, notice "Token regenerated — update TUI/scripts". Independent of `handleSave`. Styles reuse `settings-modal__*` classes in `src/styles/global.css`.

### TUI (Python)

- **New `tui-py/auth.py`**: `config_dir()`, `token_path()`, `resolve_api_token(env=os.environ, path=None)` (env > file `.strip()`, never creates — the backend does), `auth_headers()`.
- **`tui-py/api.py`**: `_get`/`_post` pass `headers=auth_headers()`; on 401 raise `ApiAuthError(httpx.HTTPStatusError)` naming the token file path.
- **`tui-py/sse.py`**: `SSEClient(url, headers=None)` → `client.stream("GET", url, headers=...)`.
- **`tui-py/app.py:188`**: pass `headers=auth.auth_headers()`. Existing `picker_error` path already surfaces the exception text.

### Scripts

- `bin/wait-for-backend.mjs`: unchanged behaviour (a resolved `fetch`, even 401, means "ready"); add a one-line comment. `bin/install-service.mjs`, `bin/cctrace.mjs`: no change (same user → same token file).

### Docs / changelog

- `specs/04-http-api.md`: env table rows `CCTRACE_API_TOKEN`, `CCTRACE_API_AUTH`; new "Authentication" section before "CORS Policy" (carriers, 401 shape, file path per OS, cookie issuance + Host rule, regenerate route); add route to the endpoint reference.
- `docs/docker.md`: runtime table rows; token file lives in the persisted `/home/app/.config` volume; LAN/reverse-proxy access needs the host in `CCTRACE_ALLOWED_ORIGINS` or an explicit token.
- `README.md`: short paragraph: where the token lives, `curl -H "X-CCTrace-Token: $(cat ~/.config/claude-code-trace/api-token)"`, `CCTRACE_API_AUTH=off`.
- `specs/05-frontend-web.md`, `specs/06-tui.md`: one line each on token acquisition.
- `CHANGELOG.md`: new Unreleased "### Added" entry in the repo's style; call out the behaviour change for scripts and Docker LAN access.

## Implementation order (each step builds and tests green)

1. Cargo deps (`subtle`, `rand`, dev `tower`).
2. `auth.rs` pure core + unit tests (`tempfile`), `mod auth;`.
3. `AppState` field/constructor/regenerate; update 8 call sites; `lib.rs` startup resolution + warnings.
4. `SettingsResponse` fields; `build_response_pub(&Settings, &ApiAuth)`; update 10 call sites.
5. Middleware, `build_router` extraction, `route_layer`, static sub-router cookie, CORS headers; `oneshot` router tests.
6. `commands/api_token.rs`, `generate_handler!`, `permissions/default.toml`, HTTP route.
7. `bin/api-token.mjs` + test, Vite plugin, `vite-env.d.ts`.
8. `src/lib/apiToken.ts`, `invoke.ts`, `listen.ts` + vitest.
9. `SettingsModal.tsx` section, `App.tsx` banner, CSS + vitest.
10. `tui-py/auth.py`, `api.py`, `sse.py`, `app.py` + pytest, ruff.
11. Scripts comment, specs, docs, README, CHANGELOG.
12. Full check: `npx oxfmt && npx oxlint && npx tsc --noEmit && npx vitest run && cargo fmt --manifest-path src-tauri/Cargo.toml && cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings && cargo test --manifest-path src-tauri/Cargo.toml && ruff check tui-py && ruff format --check tui-py && (cd tui-py && pytest)`.

## Tests

**Rust**: token generation (64 hex, unique); `token_eq` equal/different/different-length; `query_param`/`cookie_value` parsing; resolution precedence (env off > env token > file > create), created file is 0600 (unix), rotate keeps 0600 and changes content; router `oneshot`: no token → 401 JSON, header/bearer/query/cookie → 200, wrong token → 401, disabled → 200, OPTIONS preflight not 401, `access-control-allow-headers` includes `x-cctrace-token`, static fallback serves without token, index sets cookie for localhost Host, not for unknown Host, yes for allowed-origin Host; regenerate rotates live token and old token is rejected, regenerate with env source errors, regenerate response sets cookie; `SettingsResponse` JSON has the new fields; `acl_consistency` covers the new command.
**Vitest**: `bin/api-token.test.mjs` (`configDir` per platform; env off → null; env wins; reads file; creates with `wx`; EEXIST re-reads — mock `node:fs` as `python-venv.test.mjs` does); `invoke.test.ts` (header sent when token set, omitted when unset, `setApiToken` takes effect, 401 → `ApiAuthError`, regenerate route); `listen.test.ts` (`?token=` present/absent, `reconnectSse` re-attaches); `SettingsModal.test.tsx` (masked field, Show, two-click Regenerate invokes and updates, disabled for env, hint-only for disabled).
**Pytest**: `config_dir` per platform, `resolve_api_token` precedence, `auth_headers` empty without token, `_get`/`_post` pass headers, 401 → `ApiAuthError`, `SSEClient` forwards headers.

## Verification (end-to-end)

1. `cargo run --manifest-path src-tauri/Cargo.toml -- --headless` then `curl -i 127.0.0.1:11423/api/settings` → 401; `curl -H "X-CCTrace-Token: $(cat ~/.config/claude-code-trace/api-token)" ...` → 200; `curl "127.0.0.1:11423/api/events?token=..."` streams; `CCTRACE_API_AUTH=off` → 200 without token plus stderr warning.
2. `cctrace --web`: picker loads, live tailing works (SSE token in query), Settings shows masked token, Copy works, Regenerate updates the field, SSE reconnects, Vite restarts on file change and the tab still works after reload. TUI (`cctrace --tui`) connects using the same file.
3. Docker: `docker compose up --build`, open `http://localhost:1421` → works via cookie; `curl http://localhost:1421/api/settings` → 401; `curl -H "Host: evil.example" http://localhost:1421/` → no `Set-Cookie`; with `CCTRACE_API_TOKEN=abc` env the curl with that token → 200 and Regenerate is disabled in Settings.
4. Full check command from step 12 passes; CI (`.github/workflows/ci.yml`) runs the same set.

## Known trade-offs to state in the CHANGELOG

- Existing scripts against `/api/*` get 401 until they send the token or set `CCTRACE_API_AUTH=off`.
- Docker LAN/reverse-proxy access requires the host in `CCTRACE_ALLOWED_ORIGINS` for the cookie to be issued.
- In dev mode the SSE token travels in the URL (no access logging exists today; noted in the spec).
- After Regenerate, other clients (TUI, scripts) must re-read the file; the current browser tab is updated in memory.
