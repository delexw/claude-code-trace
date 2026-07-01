# Spec: CLI Launcher and Service Installer

**Locations**: `bin/cctrace.mjs`, `bin/python-venv.mjs`, `bin/install-service.mjs`, `bin/wait-for-backend.mjs`

The CLI entrypoint `cctrace` selects which mode to run and orchestrates the necessary processes.
It also supports installing the server as a persistent OS service.

---

## Mode Selection

```mermaid
flowchart TD
    START["cctrace [flags]"]
    START --> ARG{flags?}
    ARG -->|"--app (default)"| DESKTOP["Tauri desktop app\n(tauri dev / tauri bundle)"]
    ARG -->|"--web"| WEB["Browser mode\n(Vite + Rust HTTP)"]
    ARG -->|"--tui"| TUI["Terminal UI\n(Python / Textual)"]
    ARG -->|"--headless"| HEADLESS["API only\n(no frontend)"]
    ARG -->|"install"| SVCINSTALL["Service installer\n(OS-level)"]
```

---

## Web Mode Flow

```mermaid
sequenceDiagram
    participant CLI as cctrace --web
    participant BE as Rust Backend (11423)
    participant VITE as Vite dev server (1420)
    participant USER as User

    CLI ->> BE: check port 11423\n(already running?)
    alt not running
        CLI ->> USER: "Start in background? (y/n)"
        alt yes
            CLI ->> BE: spawn detached process\n(tauri dev --headless)
        else no
            CLI ->> VITE: start Vite (foreground)
            CLI ->> BE: start Rust (foreground)
        end
    end

    CLI ->> USER: open browser at\nhttp://localhost:1420
```

---

## TUI Mode Flow

```mermaid
sequenceDiagram
    participant CLI as cctrace --tui
    participant BE as Rust Backend (11423)
    participant VENV as ensureTuiVenv()
    participant WAIT as wait-for-backend.mjs
    participant TUI as TUI process

    CLI ->> BE: check port 11423
    alt not running
        CLI ->> BE: spawn headless backend\n(tauri dev --headless)
    end

    CLI ->> VENV: ensure tui-py/.venv exists + deps installed
    alt .venv missing
        VENV ->> VENV: discover python3 interpreters on PATH
        VENV ->> VENV: create venv with first one that yields working pip
    end
    VENV ->> VENV: <venv>/bin/python -m pip install -r requirements.txt
    VENV -->> CLI: path to <venv>/bin/python

    CLI ->> WAIT: node bin/wait-for-backend.mjs
    WAIT ->> BE: poll GET /api/settings\n(every 200 ms, up to 30 s)
    BE -->> WAIT: 200 OK
    WAIT -->> CLI: backend ready

    CLI ->> TUI: <venv>/bin/python tui-py/main.py\n(stdio: inherit)

    Note over CLI,TUI: graceful shutdown
    TUI ->> CLI: exit signal
    CLI ->> BE: kill backend process group (if spawned)
```

The backend spawn (`npx tauri dev -- -- --headless`) is only reached when no
backend was already running on 11423 — if one was, `--tui` connects to it and
never kills it on exit, since it may be shared with the desktop app or
another client.

When `--tui` does spawn its own backend, it uses `detached: true` and kills
via `process.kill(-backend.pid, "SIGTERM")` on TUI exit — signalling the
whole process group, not just the immediate `npx` child. `npx tauri dev`
nests further processes (the Tauri CLI, Vite, the Rust backend); a plain
`backend.kill()` only reaches `npx` and leaves Vite/the Rust backend as
orphans still holding their ports, breaking a later `cctrace --web` with
`EADDRINUSE` on 1420.

### Why a dedicated venv

The TUI's Python deps install into a dedicated `tui-py/.venv` (created on first
launch, reused after) rather than via a bare `pip` + `python3`. A bare `pip` and
a bare `python3` can resolve to **different** interpreters on a developer's
machine — an asdf shim, an unrelated active virtualenv (which may even lack
pip), system python, etc. — so deps could install where the app can't import
them. The venv guarantees install and launch use the same isolated interpreter.

Candidate interpreters are **discovered dynamically** by scanning PATH (plus
well-known install dirs) for `python3` / `python3.<minor>` — no hardcoded version
list — and each is validated by actually creating the venv and confirming pip is
present (some interpreters can `import ensurepip` yet still fail to seed pip).
The first candidate that yields a working venv is used.

---

## Port Management

```mermaid
flowchart LR
    CHECK_1420["Check port 1420\n(Vite)"]
    CHECK_1420 -->|"occupied"| DYNPORT["VITE_PORT=0\n(dynamic)"]
    CHECK_1420 -->|"free"| USE_1420["use 1420"]

    CHECK_11423["Check port 11423\n(Rust API)"]
    CHECK_11423 -->|"occupied"| REUSE["connect to existing\nbackend silently"]
    CHECK_11423 -->|"free"| SPAWN["spawn new backend"]
```

If the backend is already running (e.g., launched by a previous session), the CLI connects to it
silently without prompting — this is the "graceful reconnect" behaviour.

---

## Graceful Shutdown

```mermaid
flowchart LR
    SIGINT["SIGINT / SIGTERM"]
    SIGINT --> KILL_TUI["kill TUI process\n(if spawned)"]
    KILL_TUI --> KILL_BE["kill backend\n(if spawned by this CLI)"]
    KILL_BE --> EXIT["exit 0"]

    TUI_EXIT["TUI exits normally"]
    TUI_EXIT --> KILL_BE
```

Exit codes propagate: if TUI exits with a non-zero code, the CLI exits with the same code.

---

## Service Installer (`install-service.mjs`)

Installs `cctrace --web` as a persistent background service that starts at login.

```mermaid
flowchart TD
    INSTALL["cctrace install"]
    INSTALL --> PLATFORM{OS?}
    PLATFORM -->|"macOS"| LAUNCHD["launchd\n~/Library/LaunchAgents/\ncom.claude-code-trace.web-server.plist"]
    PLATFORM -->|"Linux"| SYSTEMD["systemd user unit\n~/.config/systemd/user/\nclaude-code-trace-web.service"]
    PLATFORM -->|"Windows"| STARTUP["Startup folder VBS script\n%APPDATA%/Microsoft/Windows/\nStart Menu/Programs/Startup/"]

    LAUNCHD --> CAPTURE["capture current PATH\nfor node/npx/cargo resolution"]
    LAUNCHD --> LOAD["launchctl load -w <plist>"]

    SYSTEMD --> ENABLE["systemctl --user enable\n+ start"]

    STARTUP --> VBS["silent VBScript launcher\n(no console window)"]
```

### macOS launchd Plist Key Settings

| Key                        | Value                                 |
| -------------------------- | ------------------------------------- |
| `Label`                    | `com.claude-code-trace.web-server`    |
| `ProgramArguments`         | `[node, /path/to/cctrace.mjs, --web]` |
| `RunAtLoad`                | `true`                                |
| `KeepAlive.SuccessfulExit` | `false` (restart on crash)            |
| `StandardOutPath`          | `~/.claude/claude-code-trace-web.log` |
| `StandardErrorPath`        | same log file                         |

---

## Backend Health Check (`wait-for-backend.mjs`)

```mermaid
flowchart LR
    START["start"]
    START --> POLL["GET http://127.0.0.1:11423/api/settings"]
    POLL -->|"200 OK"| READY["exit 0 (backend ready)"]
    POLL -->|"error / timeout"| WAIT["wait 200 ms"]
    WAIT --> COUNTER["attempt++"]
    COUNTER --> MAX{"attempt > 150?\n(~30 s)"}
    MAX -->|"yes"| FAIL["exit 1 (timeout)"]
    MAX -->|"no"| POLL
```

---

## Related Specs

- [04-http-api.md](04-http-api.md) — the backend this launcher starts
- [06-tui.md](06-tui.md) — the TUI process this launcher orchestrates
- [00-overview.md](00-overview.md) — system context
