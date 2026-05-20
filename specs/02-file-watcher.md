# Spec: File Watcher System

**Location**: `src-tauri/src/watcher.rs`

The file watcher is the live-update engine. It watches JSONL files and project directories on disk
using OS-level file system events, debounces rapid changes, and triggers re-parses that are
broadcast to all connected clients.

There are two distinct watchers:

| Watcher             | Watches                                    | Event emitted    |
| ------------------- | ------------------------------------------ | ---------------- |
| **Session watcher** | One session JSONL + its subagent directory | `session-update` |
| **Picker watcher**  | All project parent directories             | `picker-refresh` |

---

## Architecture

```mermaid
graph TB
    subgraph OS["Operating System"]
        FS["File System Events\n(inotify / kqueue / FSEvents)"]
    end

    subgraph WatcherThread["std::thread (sync context)"]
        NW["notify::RecommendedWatcher"]
        DB["run_debounce_loop()\n1 000 ms quiet time\n100 ms poll"]
    end

    subgraph AsyncTask["tokio::task (async context)"]
        RB["Rebuild loop\n- parse JSONL\n- discover subagents\n- compare to prev"]
        BC["broadcast / Tauri emit"]
    end

    subgraph Clients
        TAURI_C["Tauri window\n(session-update emit)"]
        SSE["HTTP SSE\n(broadcast channel)"]
    end

    FS -->|"raw events"| NW
    NW -->|"mpsc channel"| DB
    DB -->|"signal (unit)"| RB
    RB -->|"changed?"| BC
    BC --> TAURI_C
    BC --> SSE
```

---

## Session Watcher Detail

### Startup Sequence

```mermaid
sequenceDiagram
    participant CMD as Tauri Command\nwatch_session()
    participant ST as AppState
    participant WT as std::thread\n(notify watcher)
    participant TK as tokio::task\n(rebuild loop)

    CMD ->> ST: stop_session_watcher() [if any]
    CMD ->> WT: spawn thread
    WT ->> WT: RecommendedWatcher::new()
    WT ->> WT: watch(session_dir, Recursive)
    CMD ->> TK: spawn task
    TK ->> TK: initial load (parse + emit)

    loop file events
        WT ->> WT: filter events\n(exact path or .jsonl in subdirs)
        WT ->> WT: debounce 1 000 ms
        WT -->> TK: signal via oneshot
        TK ->> TK: re-parse session
        TK ->> TK: diff vs previous\n(msg count, item count, ongoing)
        alt changed
            TK -->> CMD: session-update broadcast
        end
    end
```

### Filter Logic

Only events matching these criteria trigger a rebuild:

```mermaid
flowchart LR
    EV["FS Event\n(path)"]
    EV --> F1{"path ==\nsession file?"}
    F1 -->|"yes"| TRIGGER["Trigger rebuild"]
    F1 -->|"no"| F2{"path is in\nsession subdirectory\nAND .jsonl ext?"}
    F2 -->|"yes"| TRIGGER
    F2 -->|"no"| F3{"path is new team\nsession file in\nproject dir?"}
    F3 -->|"yes"| TRIGGER
    F3 -->|"no"| DROP["Ignore"]
```

### Deduplication Guard

Before emitting a `session-update`, the rebuild loop checks whether anything materially changed:

```
if message_count == prev_count
   AND display_item_count == prev_item_count
   AND ongoing == prev_ongoing
   → skip emit
```

This prevents noisy updates from unrelated file touches in watched directories.

---

## Picker Watcher Detail

The picker watcher is simpler — it emits a **lightweight signal** (no data) so the frontend can
call `/api/sessions` to fetch an updated session list.

```mermaid
sequenceDiagram
    participant CMD as watch_picker()
    participant WT as std::thread
    participant TK as tokio::task
    participant CL as Client

    CMD ->> WT: derive unique parent dirs\nfrom project paths
    WT ->> WT: watch each parent\n(Recursive)

    loop file events
        WT ->> WT: filter: any non-empty event\n(broad — macOS FSEvents may coalesce\nnew-dir creates as parent Modify)
        WT ->> WT: debounce 1 000 ms
        WT -->> TK: signal
        TK -->> CL: emit "picker-refresh"\n(empty payload)
    end

    CL ->> CL: call get_project_dirs to pick up\nnewly created project folders
    CL ->> CL: call GET /api/sessions\nto refresh list
```

---

## Debounce Loop (`run_debounce_loop`)

```mermaid
stateDiagram-v2
    [*] --> IDLE : start
    IDLE --> PENDING : relevant event arrives\n(filter passes)
    PENDING --> PENDING : more events arrive\n(reset 1 000 ms timer)
    PENDING --> SIGNAL : timer expires\n(1 000 ms quiet)
    SIGNAL --> IDLE : signal sent to async task
    IDLE --> STOP : stop channel closed
    PENDING --> STOP : stop channel closed
    STOP --> [*]

    note right of PENDING
        Poll interval: 100 ms
        Timer: 1 000 ms quiet window
    end note
```

---

## WatcherHandle Lifecycle

```mermaid
classDiagram
    class WatcherHandle {
        +stop_tx oneshot_Sender
        +thread_stop_tx sync_Sender
        +drop() void
    }
    note for WatcherHandle "drop() sends stop signals\nto both channels"

    class AppState {
        -session_watcher: Mutex~Option~WatcherHandle~~
        -picker_watcher: Mutex~Option~WatcherHandle~~
        +stop_session_watcher()
        +set_session_watcher(h: WatcherHandle)
    }

    AppState --> WatcherHandle : owns (0..1)
```

Stopping a watcher is a two-signal operation:

1. `stop_tx` (oneshot) → signals the async rebuild task to exit
2. `thread_stop_tx` (sync mpsc) → signals the `std::thread` to exit the debounce loop

Both are sent when `WatcherHandle` is dropped, which happens whenever a new watcher replaces it.

---

## Truncation Detection

When the user runs `/clear` in Claude Code, the session file is truncated.
The rebuild loop detects this by comparing the new file size to the cached size:

```mermaid
flowchart LR
    PREV_SIZE["Cached byte\noffset"]
    NEW_SIZE["Current file\nsize"]
    PREV_SIZE --> CMP{"new_size &lt; prev?"}
    CMP -->|"yes"| RESET["Reset token scanner\nClear cached state\nRe-parse from byte 0"]
    CMP -->|"no"| INCR["Incremental scan\nfrom prev offset"]
```

---

## Concurrency Model

```mermaid
graph LR
    subgraph Sync["sync world (std::thread)"]
        NW["notify Watcher\n(OS callbacks)"]
        CH1["std::mpsc channel"]
        DB["Debounce loop"]
        CH2["tokio::sync::mpsc channel"]
    end
    subgraph Async["async world (tokio)"]
        RB["Rebuild task"]
        BC["broadcast::Sender&lt;SSEMessage&gt;"]
        TAURI["app_handle.emit()"]
    end
    NW --> CH1 --> DB --> CH2 --> RB --> BC
    RB --> TAURI
```

The watcher runs in a `std::thread` because the `notify` crate requires a sync callback.
All heavy work (parsing, broadcasting) happens in the async tokio task.

---

## Related Specs

- [01-parser-pipeline.md](01-parser-pipeline.md) — the parse logic triggered by the watcher
- [03-state-management.md](03-state-management.md) — AppState that holds watcher handles
- [04-http-api.md](04-http-api.md) — SSE endpoint that receives broadcasts
- [08-session-lifecycle.md](08-session-lifecycle.md) — end-to-end live update sequence
