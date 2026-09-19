# Running Claude Code Trace in Docker

This guide walks through running the web UI inside a Docker container. The
image bundles both the Rust backend and the static React frontend behind a
single HTTP port, and reads your Claude Code sessions from a mounted volume.

> **Scope.** Docker is only supported for **web mode**. The desktop (Tauri)
> and TUI modes require OS-level integrations (native windowing, ttys) that
> don't translate well to containers. If you need the desktop app, install
> from source or grab a pre-built release instead.

## Prerequisites

- Docker 20.10+ (or Docker Desktop / Podman / OrbStack equivalent)
- Your host has Claude Code sessions under `~/.claude/projects`

## Quick start

The recommended deployment command is interactive:

```bash
./script/redeploy.sh
```

After building the image, it asks whether to configure or update the Jev API key. If you answer
**Yes**, the script reads the key without echoing it and writes it to the dedicated
`claude-code-trace-secrets` Docker volume. The running container mounts that volume read-only and
loads the key server-side; the browser never sends or receives it. Answer **No** to preserve an
existing key or to run without Jev analysis.

The following manual commands do not prompt for an API key:

```bash
# Build
docker build -t claude-code-trace .

# Run — expose port 1421, mount ~/.claude read-only
docker run --rm \
  -p 1421:1421 \
  -v "$HOME/.claude:/home/app/.claude:ro" \
  claude-code-trace
```

Open http://localhost:1421 in a browser. The session picker will populate
from the mounted directory.

Press `Ctrl-C` to stop the container.

## docker compose

A `docker-compose.yml` is included for convenience:

```bash
docker compose up --build
```

To change the host port or the Claude data location, set env vars:

```bash
CCTRACE_HOST_PORT=9090 CLAUDE_HOME=/srv/claude docker compose up
```

## Runtime configuration

All runtime knobs are environment variables, so you can override them with
`-e VAR=value` on `docker run` or under `environment:` in compose.

| Variable                  | Default     | What it does                                                                             |
| ------------------------- | ----------- | ---------------------------------------------------------------------------------------- |
| `CCTRACE_HTTP_HOST`       | `0.0.0.0`   | Bind host for the HTTP server                                                            |
| `CCTRACE_HTTP_PORT`       | `1421`      | Bind port for the HTTP server                                                            |
| `CCTRACE_STATIC_DIR`      | `/app/dist` | Directory of static frontend assets to serve                                             |
| `CCTRACE_RUNTIME`         | `docker`    | Marks the shipped container runtime so host subscription CLI providers stay unavailable  |
| `CCTRACE_ALLOWED_ORIGINS` | (unset)     | Extra CORS origins, comma-separated (see below)                                          |
| `CCTRACE_API_AUTH`        | (unset)     | `off` disables client verification (see "API access" below)                              |
| `CCTRACE_CONFIG_DIR`      | (unset)     | Relocate `settings.json` + client secrets (default `$XDG_CONFIG_HOME/claude-code-trace`) |
| `JEV_API_KEY_FILE`        | (unset)     | Server-side file containing the Jev API key; Compose uses `/run/secrets/jev_api_key`     |

Outside Docker (i.e. the normal desktop/web app) these variables are not
set, and the server falls back to the historical defaults
(`127.0.0.1:11423`, no static assets). So adding these vars has no effect on
native installations.

The Docker image does not bundle or authenticate the Codex or Claude Code CLIs, so their
subscription-backed recommendation providers are hidden and rejected in this runtime. Configure an
OpenAI-compatible endpoint instead. Native installations and ordinary web mode keep the subscription
provider choices because their backend runs directly on the authenticated host.

The browser never submits a recommendation-provider API key. In web and Docker modes,
OpenAI-compatible providers are restricted to loopback HTTP(S) URLs (`localhost`, `127.0.0.0/8`, or
`::1`) without URL credentials, query parameters, or fragments. Use the desktop app when a remote
provider requires a securely stored API token.

The shipped Compose configuration sets `JEV_API_KEY_FILE` and mounts the
`claude-code-trace-secrets` volume at `/run/secrets`. Use `./script/redeploy.sh` to populate or
update it. Docker administrators can still read container-managed secrets, so protect access to
the Docker daemon; the key is not exposed through the web API, browser storage, Compose
environment, or tracked files.

In the default Compose setup, the frontend and API are served same-origin
(both on port 1421 via `CCTRACE_STATIC_DIR`), so `CCTRACE_ALLOWED_ORIGINS`
isn't needed for the bundled UI to work — it only matters if you're reaching
this container's API from a _different_ origin (e.g. a reverse proxy on
another hostname). The app's Settings UI can also configure extra allowed
origins; those are stored in `~/.config/claude-code-trace/settings.json`
inside the container, which the shipped `docker-compose.yml` persists via a
named `cctrace-config` volume (and the Dockerfile's own `VOLUME` declaration
gives plain `docker run` an anonymous volume for the same path) — so they
survive a container recreate either way. Both mechanisms compose (the
allowlist is a union), so you can use either or both.

## API access (client verification)

Every `/api/*` route requires the signed credential of a registered **client**, so a container
exposed on a LAN port isn't an open door to your session transcripts, and each caller is identified
by name. Nothing to configure for the bundled UI: it runs as the built-in `web-ui` client, the server
sets its credential as an `HttpOnly` cookie on the HTML shell, and the browser sends it back
automatically.

The signing key (`api-secret`), the client registry (`clients.json`) and the built-in clients'
credentials (`clients/web-ui.jwt`, `clients/tui.jwt`) live in `/home/app/.config/claude-code-trace/`
(mode `0600`), i.e. inside the persisted config volume, so they survive container recreation.
`CCTRACE_API_AUTH=off` turns verification off entirely (not recommended when the port is reachable
from other machines).

The cookie is only issued when the request `Host` is `localhost`/`127.0.0.1` or matches an allowed
origin. So if you open the UI via a LAN IP or a reverse-proxy hostname, add that origin to
`CCTRACE_ALLOWED_ORIGINS` (e.g. `http://192.168.1.20:1421`) or the Settings UI — the same knob that
was already needed for cross-origin API access.

Scripts hitting the container's API directly need their own client. Register one in the UI
(Settings → **Accepted clients** → Add client; the credential is shown once) or over the API, using
the `tui` client's file as the bootstrap credential:

```bash
TUI=$(docker compose exec cctrace cat /home/app/.config/claude-code-trace/clients/tui.jwt)
curl -H "X-CCTrace-Token: $TUI" -H "Content-Type: application/json" \
  -d '{"name":"backup-script"}' http://localhost:1421/api/clients
# → {"client":{...},"credential":"eyJ..."}   ← store the credential; it is not kept
curl -H "X-CCTrace-Token: eyJ..." http://localhost:1421/api/whoami
```

Revoke or reissue any client from the same Settings section without affecting the others.

## Volumes

| Container path      | Purpose                                                                                                                         |
| ------------------- | ------------------------------------------------------------------------------------------------------------------------------- |
| `/home/app/.claude` | Source of session JSONL files (mount your host's `~/.claude` here)                                                              |
| `/home/app/.config` | `settings.json`, `api-secret`, `clients.json`, `clients/*.jwt` — persisted read-write so settings and clients survive recreates |

The app only needs to **read** session logs, so mounting `/home/app/.claude`
read-only (`:ro`) is recommended and is what the shipped `docker-compose.yml`
does. `/home/app/.config` needs to stay read-write since the app saves to it.

If you keep session logs somewhere other than `~/.claude/projects` — e.g.
`/srv/claude` — mount that directory instead, or point the app at a
different `projectsDir` via the Settings UI (the chosen path is remembered
in `XDG_CONFIG_HOME/claude-code-trace/settings.json`, which lives inside
`/home/app/.config` in the image).

## Networking model

The container runs a single axum HTTP server that:

1. Serves `/api/*` — the JSON + Server-Sent Events backend.
2. Serves everything else from `/app/dist` — the compiled React bundle
   (including SPA deep-link support via `append_index_html_on_directories`).

The frontend is built with `VITE_API_BASE=""`, which makes it use
same-origin relative URLs. Nothing in the container talks to `localhost`
from the browser's perspective, so you only need to expose **one port**.

## Under the hood

Tauri v2's webview runtime links against `libwebkit2gtk-4.1-0` on Linux,
which needs an X display even when no window is shown. The image uses
`xvfb-run` to provide a virtual display at runtime — this is invisible to
the user but means headless mode works without a host display server.

## Troubleshooting

**The session picker is empty.** Check that your host mount actually points
at a directory containing `~/.claude/projects`:

```bash
docker run --rm -v "$HOME/.claude:/home/app/.claude:ro" \
  claude-code-trace ls -la /home/app/.claude/projects
```

**Port 1421 is already in use.** Pick a different host port:

```bash
docker run --rm -p 9090:1421 \
  -v "$HOME/.claude:/home/app/.claude:ro" \
  claude-code-trace
```

**`cannot open display` / webkit errors on startup.** The entrypoint uses
`xvfb-run`; if you override the entrypoint make sure you keep it (or
provide your own virtual display). Running the binary directly with
`docker run --entrypoint /usr/local/bin/claude-code-trace ...` will fail.

**"Not an accepted client" in the browser, but it works in a private window.**
The credential cookie rides on the HTML shell, so a browser holding an older
cached copy of that page never receives one. The shell is now served
`Cache-Control: no-cache` to prevent this, but a copy cached before that
change is still cached: reload once with `Cmd`/`Ctrl`+`Shift`+`R`. Confirm the
server side with `curl -sI http://localhost:1421/ | grep -i set-cookie` — if
that prints a `cctrace_token=...` line, the backend is issuing the cookie and
the problem is on the browser's side. If it prints nothing, the request `Host`
is not allowlisted (see [API access](#api-access-client-verification)) or
`web-ui` has been revoked.

**File watchers don't see changes.** On some Docker-for-Mac / WSL setups,
`notify`-style filesystem watchers over bind mounts are unreliable. This
affects the "live tailing" feature. A reload usually picks up new content;
for aggressive tailing, prefer running the native desktop app on the host.
