# Spec: State Management

**Locations**: `src-tauri/src/state.rs`, `src-tauri/src/parser/cache.rs`, `src-tauri/src/settings.rs`

`AppState` is the central in-memory store for the Rust backend. It is shared across all Tauri
commands and HTTP handlers via `Arc<AppState>`, protected by per-field `Mutex`es.

---

## AppState Structure

```mermaid
classDiagram
    class AppState {
        +session_watcher: Mutex~Option~WatcherHandle~~
        +picker_watcher: Mutex~Option~WatcherHandle~~
        +session_cache: SessionCache
        +settings: RwLock~Settings~
        +watched_session_ongoing: Mutex~Option~bool~~
        +event_tx: broadcast::Sender~SSEMessage~
        +sessions_cache: Mutex~Option~SessionsCache~~
    }

    class SessionCache {
        -map: HashMap~CacheKey, CachedSession~
        +get(path, mtime, size) Option~&CachedSession~
        +insert(path, mtime, size, session)
    }

    class SessionsCache {
        -sessions: Vec~SessionInfo~
        -fetched_at: Instant
        -ttl: 2 s
        +is_fresh() bool
    }

    class Settings {
        +projects_dir: Option~String~
    }

    AppState --> SessionCache
    AppState --> SessionsCache
    AppState --> Settings
```

---

## Session Cache (`cache.rs`)

The session cache avoids re-parsing unchanged files. It uses a composite key:

```
CacheKey = (file_path, modification_time, file_size)
```

```mermaid
flowchart LR
    REQ["Request to load\nsession X"]
    REQ --> LOOKUP{"Cache hit?\n(path, mtime, size)"}
    LOOKUP -->|"hit"| RET["Return cached\nDisplayMessage[]"]
    LOOKUP -->|"miss"| PARSE["Run full\nparser pipeline"]
    PARSE --> STORE["Store in cache\nwith new key"]
    STORE --> RET
```

### Ongoing Session Freshness

For sessions marked `is_ongoing = true`, the cached result is considered **stale after 60 seconds**
even if the file appears unchanged (the parser may have new subagent data).

```mermaid
flowchart LR
    CACHED["Cached entry\n(is_ongoing=true)"]
    CACHED --> AGE{"Age > 60 s?"}
    AGE -->|"yes"| STALE["Treat as miss\nre-parse"]
    AGE -->|"no"| USE["Use cache"]
```

---

## Sessions List Cache (Picker Cache)

A second short-lived cache coalesces concurrent picker requests:

```mermaid
sequenceDiagram
    participant C1 as Client 1
    participant C2 as Client 2
    participant SC as sessions_cache\n(2 s TTL)
    participant FS as Filesystem

    C1 ->> SC: discover_sessions_cached()
    SC ->> FS: scan project dirs
    FS -->> SC: Vec<SessionInfo>
    SC -->> C1: sessions

    Note over C1,C2: within 2 s

    C2 ->> SC: discover_sessions_cached()
    SC -->> C2: cached sessions (no FS scan)
```

This prevents thundering-herd filesystem scans when the picker-refresh signal causes multiple
clients to call `/api/sessions` simultaneously.

---

## Watched Session Ongoing Override

The picker's `is_ongoing` for a session is derived from a lightweight heuristic (last-modified
time). The session watcher, however, has the authoritative result from a full parse.

`apply_watched_ongoing()` patches the picker's list with the watcher's verdict:

```mermaid
flowchart LR
    WA["watched_session_ongoing\n(Mutex&lt;Option&lt;bool&gt;&gt;)"]
    PICK["Vec&lt;SessionInfo&gt;\nfrom picker"]
    WA --> PATCH{"Watcher loaded\nsession X?"}
    PATCH -->|"yes"| OVERRIDE["Set sessions[X].is_ongoing\n= watcher verdict"]
    PATCH -->|"no"| PASS["No change"]
    PICK --> PATCH
    OVERRIDE --> OUT["Patched Vec&lt;SessionInfo&gt;"]
    PASS --> OUT
```

---

## SSE Broadcast Channel

All HTTP clients subscribe to a `broadcast::Sender<SSEMessage>` stored in `AppState`.

```mermaid
flowchart LR
    subgraph Backend
        TX["broadcast::Sender\n(AppState.event_tx)"]
    end
    subgraph HTTP_Clients["HTTP Clients"]
        RX1["SSE subscriber 1\nRX"]
        RX2["SSE subscriber 2\nRX"]
    end
    TX -->|"clone()"| RX1
    TX -->|"clone()"| RX2
```

`AppState::broadcast()` sends to the channel. Lagged receivers (slow clients) are simply dropped
— they re-connect via `EventSource` and receive the next event.

---

## Settings (`settings.rs`)

User settings live in `~/.config/claude-code-trace/settings.json`.

```mermaid
flowchart LR
    FILE["settings.json\non disk"]
    FILE -->|"load_settings()"| MEM["Settings in AppState\n(RwLock)"]
    MEM -->|"save_settings()"| FILE
    MEM -->|"platform_default_dir()"| DEF["~/.claude/projects\n(if not set)"]
```

### Platform Defaults

| Platform | Default `projects_dir` |
| -------- | ---------------------- |
| All      | `~/.claude/projects`   |

The configured value takes precedence; if not set, the platform default is used in all lookups.

---

## Concurrency Invariants

| Resource                  | Protection                            | Notes                                                   |
| ------------------------- | ------------------------------------- | ------------------------------------------------------- |
| `session_watcher`         | `Mutex`                               | Replaced atomically; old handle dropped → stops watcher |
| `picker_watcher`          | `Mutex`                               | Same pattern                                            |
| `session_cache`           | Internal `Mutex` (via `SessionCache`) | Fine-grained per-entry                                  |
| `settings`                | `RwLock`                              | Many readers, rare writes                               |
| `sessions_cache`          | `Mutex`                               | Short critical section; TTL check + replace             |
| `watched_session_ongoing` | `Mutex`                               | Written by watcher task, read by picker command         |

---

## Related Specs

- [02-file-watcher.md](02-file-watcher.md) — writes `watched_session_ongoing`
- [04-http-api.md](04-http-api.md) — reads `AppState` for all HTTP handlers
- [08-session-lifecycle.md](08-session-lifecycle.md) — full state transition sequence
