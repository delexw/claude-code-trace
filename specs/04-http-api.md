# Spec: HTTP API & SSE

**Location**: `src-tauri/src/http_api.rs`

The Rust backend exposes a full HTTP API so that browsers and the TUI can access the same
functionality as the Tauri desktop frontend. The server runs on port **11423** by default
(configurable via `CCTRACE_HTTP_PORT`).

---

## Architecture

```mermaid
graph TB
    subgraph Rust["Rust Backend (Axum)"]
        ROUTER["Axum Router\n/api/*"]
        MW["Middleware\nAPI token check (accepted clients only)\nCORS (allowlisted origins)\nJSON responses"]
        STATE["Arc&lt;AppState&gt;"]
    end

    subgraph Endpoints["REST Endpoints"]
        GET_SETTINGS["GET /api/settings"]
        GET_DIRS["GET /api/project-dirs"]
        POST_SESSIONS["POST /api/sessions"]
        POST_LOAD["POST /api/session/load"]
        POST_MESSAGE["POST /api/session/message"]
        GET_META["GET /api/session/meta"]
        POST_WATCH["POST /api/session/watch"]
        POST_UNWATCH["POST /api/session/unwatch"]
        POST_PICKER_WATCH["POST /api/picker/watch"]
        POST_PICKER_UNWATCH["POST /api/picker/unwatch"]
        GET_GIT["GET /api/git-info"]
        GET_DEBUG["GET /api/debug-log"]
        GET_SETTINGS_W["POST /api/settings/set"]
        POST_TOKEN["POST /api/settings/token/regenerate"]
        GET_SSE["GET /api/events\n(SSE stream)"]
        STATIC["GET /*\n(optional static assets)"]
    end

    ROUTER --> MW --> STATE
    ROUTER --> GET_SETTINGS
    ROUTER --> GET_DIRS
    ROUTER --> POST_SESSIONS
    ROUTER --> POST_LOAD
    ROUTER --> POST_MESSAGE
    ROUTER --> GET_META
    ROUTER --> POST_WATCH
    ROUTER --> POST_UNWATCH
    ROUTER --> POST_PICKER_WATCH
    ROUTER --> POST_PICKER_UNWATCH
    ROUTER --> GET_GIT
    ROUTER --> GET_DEBUG
    ROUTER --> GET_SETTINGS_W
    ROUTER --> POST_TOKEN
    ROUTER --> GET_SSE
    ROUTER --> STATIC
```

---

## Endpoint Reference

### `GET /api/settings`

Returns current settings, the platform default projects directory, and the API client token
state (see [Authentication](#authentication)).

**Response**

```json
{
  "projects_dir": "/Users/you/.claude/projects",
  "default_dir": "/Users/you/.claude/projects",
  "api_auth_enabled": true,
  "api_token_source": "file",
  "api_token": "3f9a…(64 hex chars)"
}
```

`api_token_source` is `"file"` (rotatable from Settings), `"env"` (`CCTRACE_API_TOKEN`, read-only),
`"ephemeral"` (the token file was unusable at startup; one-off, read-only) or `"disabled"`
(`CCTRACE_API_AUTH=off`, `api_token` is `null`).

---

### `GET /api/project-dirs`

Returns all project directories found under `projects_dir`.

**Response**

```json
{ "dirs": ["/Users/you/.claude/projects/my-app"] }
```

---

### `POST /api/sessions`

Discovers sessions across provided project directories.

**Request**

```json
{ "dirs": ["/Users/you/.claude/projects/my-app"] }
```

**Response**

```json
{
  "sessions": [
    {
      "path": "...",
      "session_id": "abc123",
      "first_message": "Write a function...",
      "mod_time": "2025-05-01T10:00:00Z",
      "total_tokens": 12345,
      "cost_usd": 0.05,
      "model": "claude-opus-4-6",
      "turn_count": 12,
      "git_branch": "main",
      "is_ongoing": false
    }
  ]
}
```

Results are cached for 2 seconds (see [03-state-management.md](03-state-management.md)).

---

### `POST /api/session/load`

Fully parses a session JSONL file and returns a windowed slice of display-ready messages,
plus a lightweight full-session index so virtualized clients can size and scroll the list
without holding every message's heavy body in memory.

**Request**

```json
{ "path": "/path/to/session.jsonl", "start": 0, "limit": 200 }
```

`start`/`limit` are optional and request an index window into the message list — `start`
defaults to `0`, `limit` defaults to unset (load to the end). Omit both to load the whole
session.

**Response**

```json
{
  "messages": [/* DisplayMessage[] — only the requested window */],
  "count": 5000,
  "start": 0,
  "roles": ["user", "assistant", "..."],
  "context_tokens": 12345,
  "teams": [/* TeamSnapshot[] */],
  "ongoing": true,
  "meta": { "cwd": "...", "git_branch": "main", "permission_mode": "manual" },
  "session_totals": { "total_tokens": 5000, "cost_usd": 0.02, "model": "..." }
}
```

- `messages` — the windowed slice only, not the whole session.
- `count` — total number of messages in the full session.
- `start` — the index of the first message in the returned `messages` slice.
- `roles` — the role of every message in the full session (length == `count`); lets a
  virtualized client render placeholders and drive expand-all without holding the heavy
  message bodies for rows outside the window.
- `context_tokens` — latest Claude context-window fill, for the context gauge.

---

### `POST /api/session/message`

Returns the single full (heavy-body) `DisplayMessage` at `index`, for the Detail view to
fetch on demand — the windowed list from `session/load` carries lightened messages with no
tool-output bodies, so the detail view fetches the full message separately when the user
opens it.

**Request**

```json
{ "path": "/path/to/session.jsonl", "index": 42 }
```

**Response**

```json
/* DisplayMessage, or null if index is out of range */
```

---

### `POST /api/session/watch` / `POST /api/session/unwatch`

Starts or stops the session watcher. The watcher pushes `session-update` events over SSE.

**Watch request**: `{ "path": "..." }`
**Both responses**: `{ "ok": true }`

---

### `POST /api/picker/watch` / `POST /api/picker/unwatch`

Starts or stops the picker watcher. Pushes `picker-refresh` events over SSE.

**Watch request**: `{ "dirs": ["..."] }`
**Both responses**: `{ "ok": true }`

---

### `GET /api/git-info`

Returns git metadata for a working directory.

**Query params**: `?cwd=/path/to/dir`

**Response**

```json
{
  "branch": "main",
  "dirty": false,
  "worktree_dirs": ["/path/to/worktree"]
}
```

---

### `GET /api/debug-log`

Returns incremental debug log entries.

**Query params**: `?session_path=...&since=<timestamp>&level=warn`

**Response**

```json
{
  "entries": [{ "timestamp": "...", "level": "Warn", "category": "hook", "message": "..." }]
}
```

---

### `POST /api/settings/set`

Updates the projects directory setting.

**Request**: `{ "projects_dir": "/custom/path" }`
**Response**: `{ "ok": true }`

---

### `POST /api/settings/token/regenerate`

Rotates the shared API client token (see [Authentication](#authentication)): writes a fresh
64-hex token to the token file, swaps the live value the middleware checks, and returns the
same `SettingsResponse` as `GET /api/settings` with the new `api_token`. The response also
carries a `Set-Cookie: cctrace_token=<new>` so a same-origin (Docker) browser tab keeps
working without a reload.

Like every `/api/*` route it requires the _current_ token, so only an already-accepted client
can rotate it. Returns `400 {"error": ...}` when the token is not rotatable: it came from
`CCTRACE_API_TOKEN` (env tokens are read-only) or verification is disabled.

### `GET /api/events` — SSE Stream

The SSE endpoint streams real-time events to connected clients.

```mermaid
sequenceDiagram
    participant C as Client
    participant H as /api/events handler
    participant BC as broadcast::Receiver

    C ->> H: GET /api/events\n(long-lived connection)
    H ->> BC: subscribe (clone Receiver)
    H -->> C: Content-Type: text/event-stream

    loop while connected
        BC -->> H: SSEMessage { event, data }
        H -->> C: event: <name>\ndata: <json>\n\n
    end

    C ->> H: disconnect
    H ->> BC: drop (unsubscribe)
```

#### Event Types

| Event name       | Payload                | Trigger                                  |
| ---------------- | ---------------------- | ---------------------------------------- |
| `session-update` | `SessionUpdatePayload` | Session JSONL changed                    |
| `picker-refresh` | `{}` (empty)           | Any `.jsonl` file in project dir changed |

#### `session-update` Payload

A lightweight refresh **signal**, not a data dump: it carries the total
message `count` and the per-message `roles` index (so a virtualized client
can resize its list and refetch the visible window) plus session-level
fields, but never the heavy message bodies. Clients re-fetch the window
they're viewing (`SessionUpdatePayload` in `src-tauri/src/watcher.rs`).

```json
{
  "count": 42,
  "roles": [ /* string[], one per message, length == count */ ],
  "context_tokens": 12345,
  "teams": [ /* TeamSnapshot[] */ ],
  "ongoing": true,
  "permission_mode": "manual",
  "session_totals": { "total_tokens": ..., "cost_usd": ..., "model": "..." }
}
```

There is no `messages` field. The web frontend re-fetches its current
window via `POST /api/session/load` on receipt. The TUI (`tui-py/app.py`'s
`_on_session_update`) does the same — it re-fetches the whole session via
`load_session` rather than reading this payload, since it doesn't paginate
the way the web frontend does.

---

## Configuration

| Env var                   | Default                  | Description                                                           |
| ------------------------- | ------------------------ | --------------------------------------------------------------------- |
| `CCTRACE_HTTP_HOST`       | `127.0.0.1`              | Bind address                                                          |
| `CCTRACE_HTTP_PORT`       | `11423` (Docker: `1421`) | Listen port                                                           |
| `CCTRACE_STATIC_DIR`      | (unset)                  | Directory to serve as static files at `/`                             |
| `CCTRACE_ALLOWED_ORIGINS` | (unset)                  | Extra CORS origins, comma-separated (see CORS Policy below)           |
| `CCTRACE_API_TOKEN`       | (unset)                  | Fixed API client token; overrides the token file (see Authentication) |
| `CCTRACE_API_AUTH`        | (unset)                  | `off` disables the API token check entirely (loud stderr warning)     |

The default port for native binaries is `11423` (defined in `http_api.rs:38` as
`DEFAULT_HTTP_PORT`). The Docker image overrides this to `1421` via `CCTRACE_HTTP_PORT=1421` so
that the API and the bundled static frontend are served from a single, well-known port — this is
what the README and `docker-compose.yml` reference.

When `CCTRACE_STATIC_DIR` is set, the frontend build output is served directly, enabling the pure
headless / Docker deployment mode.

---

## Tauri IPC Mirror

Every HTTP endpoint has an exact Tauri command counterpart in `src-tauri/src/commands/`.
The commands share the same `AppState` and call the same parser functions.

```mermaid
flowchart LR
    subgraph IPC["Tauri IPC (desktop)"]
        T_SESS["load_session command"]
        T_MSG["load_message command"]
        T_WATCH["watch_session command"]
        T_PICK["discover_sessions command"]
    end

    subgraph HTTP["HTTP API (browser/TUI)"]
        H_SESS["POST /api/session/load"]
        H_MSG["POST /api/session/message"]
        H_WATCH["POST /api/session/watch"]
        H_PICK["POST /api/sessions"]
    end

    STATE["Arc&lt;AppState&gt;"]

    T_SESS --> STATE
    T_MSG --> STATE
    T_WATCH --> STATE
    T_PICK --> STATE
    H_SESS --> STATE
    H_MSG --> STATE
    H_WATCH --> STATE
    H_PICK --> STATE
```

---

## Authentication

**Location**: `src-tauri/src/auth.rs`

Every `/api/*` route is gated by a shared secret token so that only _accepted clients_ — the
bundled web UI, the Python TUI, and any tool the user has handed the token to — can reach the
backend. Without it, any local process (or, when Docker binds `0.0.0.0`, any LAN host) could read
session transcripts, rewrite `settings.json`, add CORS origins, or trigger the `git` / `osascript`
side effects of `/api/git-info` and `/api/focus`. The Tauri desktop webview is unaffected: it talks
over IPC, never HTTP.

### Token resolution (`auth::resolve_api_auth`, once at startup)

1. `CCTRACE_API_AUTH=off` → verification **disabled**; a warning is printed to stderr.
2. `CCTRACE_API_TOKEN=<token>` → that token, verbatim. Not rotatable at runtime.
3. Otherwise the token file `<config dir>/claude-code-trace/api-token` (sibling of
   `settings.json`) is read, or created with a fresh 64-hex token if missing. On unix the file is
   mode `0600`. Config dir per OS: `$XDG_CONFIG_HOME` or `~/.config` (Linux),
   `~/Library/Application Support` (macOS), `%APPDATA%` (Windows).

Creation uses `O_EXCL`: in `cctrace --web`, Tauri starts Vite _before_ the Rust binary, and the
Vite plugin (`bin/api-token.mjs`) may create the file first. Whichever side loses the race re-reads
the winner's token (retrying briefly while the winner is still writing), so both converge. If the
file cannot be read or created the server fails **closed**: it runs with an unpersisted one-off token
(`api_token_source: "ephemeral"`), never unauthenticated, and Settings says so rather than blaming
`CCTRACE_API_TOKEN`. The Vite side throws instead, so the dev server fails loudly rather than baking
a token no backend accepts.

`AppState.api_auth: RwLock<ApiAuth>` holds the live value; the middleware reads it on every request
so a rotation takes effect immediately. Rotation writes a sibling temp file and renames it over the
token file, so concurrent readers never see it empty. When a request's token does **not** match and
the live token came from the file, the middleware re-reads the file once before rejecting: if another
cctrace process (a background `--web` service next to the desktop app, say) rotated it, the live
token heals on the spot instead of every client getting 401 until a restart.

### Accepted carriers (`auth::require_api_token`)

Checked in this order with a constant-time comparison; any match passes:

| Carrier                         | Used by                                                     |
| ------------------------------- | ----------------------------------------------------------- |
| `X-CCTrace-Token: <token>`      | Web UI `fetch` (`src/lib/invoke.ts`), TUI (`tui-py/api.py`) |
| `Authorization: Bearer <token>` | curl / external tools                                       |
| `?token=<token>` query          | Browser `EventSource` on `/api/events` (cannot set headers) |
| `cctrace_token` cookie          | Docker same-origin UI (set by the server, see below)        |

A request with no valid token gets `401 {"error": "missing or invalid API token — ..."}`. `OPTIONS`
always passes so CORS preflights are never blocked, and CORS runs outermost so the 401 carries CORS
headers and the browser can read the error body. The middleware is applied with `route_layer`, so
it covers only the registered API routes: the static UI fallback stays public and unknown paths
still 404 rather than 401.

### How each client gets the token

- **Web/dev mode** (`cctrace --web`, Vite on 1420 → API on 11423): the `cctrace-api-token` Vite
  plugin in `vite.config.ts` reads the token file at _serve_ time and injects it as
  `import.meta.env.VITE_API_TOKEN` (never into a production build). `src/lib/apiToken.ts` holds it;
  `invoke.ts` sends the header and `listen.ts` appends `?token=`. The plugin watches the file and
  restarts the dev server when it changes.
- **Docker / same-origin** (`CCTRACE_STATIC_DIR` set): `auth::attach_token_cookie` wraps the static
  fallback and adds `Set-Cookie: cctrace_token=<token>; Path=/; HttpOnly; SameSite=Strict` to
  `text/html` responses — but **only when the request `Host` is allowlisted**: `localhost`,
  `127.0.0.1`, `::1`, or the host part of any allowed CORS origin (defaults, `CCTRACE_ALLOWED_ORIGINS`,
  Settings UI). Without that gate a DNS-rebinding page pointed at the `0.0.0.0`-bound port would be
  issued the cookie and become a fully authenticated client. Reaching a container via a LAN IP or
  reverse-proxy hostname therefore requires adding it to `CCTRACE_ALLOWED_ORIGINS` (or passing the
  token explicitly).
- **TUI**: `tui-py/auth.py` mirrors the resolution (env → file, never creates) and
  `auth_headers()` is re-evaluated per request, so a rotated token is picked up without a restart.
- **Other tools**: copy the token from Settings → API access, or read the file:
  `curl -H "X-CCTrace-Token: $(cat ~/.config/claude-code-trace/api-token)" http://127.0.0.1:11423/api/settings`.

### Settings UI

`GET /api/settings` reports `api_auth_enabled`, `api_token_source`
(`"file" | "env" | "ephemeral" | "disabled"`) and `api_token`. The Settings modal's **API access**
section shows the token masked with Show / Copy / Regenerate; Regenerate is a two-click confirm and
is disabled for `env` and `ephemeral` tokens.
`POST /api/settings/token/regenerate` (or the `regenerate_api_token` Tauri command) performs the
rotation.

Note: in dev mode the SSE token travels in the URL. The server keeps no access log today; if one is
added, the `token` query parameter must be redacted.

---

## CORS Policy

The HTTP server checks every request's `Origin` header against an allowlist — never a wildcard.
An origin is allowed only if it exactly matches one of (`http_api.rs::origin_allowed`):

- The built-in dev/web UI defaults: `http://localhost:1420`, `http://127.0.0.1:1420`
  (`DEFAULT_ALLOWED_ORIGINS`).
- Extra origins from the `CCTRACE_ALLOWED_ORIGINS` env var (comma-separated), resolved once at
  server startup.
- Extra origins configured via the Settings UI (`Settings.allowed_origins`, set through
  `POST /api/settings/origins`), checked live on every request — no restart required for a change
  to take effect.

The env var and the Settings-UI list are unioned, not exclusive — either or both can be used. The
env var is the better fit for Docker/deployment (where `settings.json` may not survive a container
restart without a volume mount); the Settings-UI field is for desktop/persistent-web use. Matching
is exact-string only — no prefix, substring, or wildcard matching.

The Tauri desktop webview never goes through this check — it talks to the backend over the IPC
bridge, not HTTP. The Docker image doesn't need an allowlist entry either — it serves the frontend
and API on the same origin (`CCTRACE_STATIC_DIR`), so browser requests there are same-origin and
CORS doesn't apply.

---

## Related Specs

- [03-state-management.md](03-state-management.md) — AppState used by every handler
- [05-frontend-web.md](05-frontend-web.md) — browser client using this API
- [06-tui.md](06-tui.md) — TUI client using this API
- [08-session-lifecycle.md](08-session-lifecycle.md) — SSE flow end-to-end
