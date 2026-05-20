# Spec: CLI Launcher and Service Installer

**Locations**: `bin/cctrace.mjs`, `bin/install-service.mjs`, `bin/wait-for-backend.mjs`

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
    participant DEPS as pip install
    participant WAIT as wait-for-backend.mjs
    participant TUI as TUI process

    CLI ->> BE: check port 11423
    alt not running
        CLI ->> BE: spawn headless backend\n(tauri dev --headless)
    end

    CLI ->> DEPS: pip install -r tui-py/requirements.txt --quiet
    DEPS -->> CLI: textual + httpx + ... installed

    CLI ->> WAIT: node bin/wait-for-backend.mjs
    WAIT ->> BE: poll GET /api/settings\n(every 200 ms, up to 30 s)
    BE -->> WAIT: 200 OK
    WAIT -->> CLI: backend ready

    CLI ->> TUI: python3 tui-py/main.py\n(stdio: inherit)

    Note over CLI,TUI: graceful shutdown
    TUI ->> CLI: exit signal
    CLI ->> BE: kill backend (if spawned)
```

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
