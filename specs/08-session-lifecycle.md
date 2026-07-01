# Spec: End-to-End Session Lifecycle

This document traces the complete journey of a Claude Code session from initial file discovery
through live updates, covering both desktop (Tauri IPC) and browser/TUI (HTTP) paths.

---

## Phase 1: Application Startup

```mermaid
sequenceDiagram
    participant BIN as bin/cctrace.mjs
    participant RUST as Rust Backend
    participant FE as Frontend (Web/TUI/Desktop)

    BIN ->> RUST: spawn process\n(tauri dev / headless)
    RUST ->> RUST: load_settings()\n(~/.config/claude-code-trace/settings.json)
    RUST ->> RUST: start HTTP server\n(port 11423)
    RUST ->> RUST: start picker watcher\n(implicit on watch_picker call)

    BIN ->> FE: open window / start TUI

    FE ->> RUST: GET /api/settings (or invoke get_settings)
    RUST -->> FE: { projects_dir, default_dir }
    FE ->> RUST: GET /api/project-dirs (or invoke get_project_dirs)
    RUST -->> FE: { dirs: [...] }
    FE ->> RUST: POST /api/sessions (or invoke discover_sessions)
    RUST -->> FE: { sessions: [...] }

    FE ->> RUST: POST /api/picker/watch (or invoke watch_picker)
    FE ->> RUST: GET /api/events (SSE subscribe)
```

---

## Phase 2: Session Selection and Loading

Loading a session is windowed, not whole-file: the frontend (`useSession.ts`) fetches
message bodies a `PAGE_SIZE=100` page at a time via `ensureRange`, and keeps only a
sparse in-memory `Map<index, DisplayMessage>` of the pages near the viewport — far pages
are evicted as the user scrolls. `load_session` (Tauri command / `POST
/api/session/load`) accepts `start`/`limit` windowing params and always returns a
`LoadResult`: the requested `messages` slice plus `count` (total message count), `start`,
`roles` (role of every message, the lightweight full-session index), `context_tokens`,
and the existing `teams`/`ongoing`/`meta`/`session_totals` fields (which describe the
whole session regardless of the window).

```mermaid
sequenceDiagram
    participant USER as User
    participant FE as Frontend (useSession)
    participant RUST as Rust Backend
    participant FS as File System

    USER ->> FE: select session from picker
    FE ->> RUST: load_session\n{ path, start: 0, limit: 0 }
    Note over FE,RUST: metadata-only fetch to learn count/roles

    RUST ->> FS: read JSONL file\n(LineReader, buffered)
    FS -->> RUST: raw lines

    loop per line
        RUST ->> RUST: parse_entry()\ndeserialise JSON
        RUST ->> RUST: classify_entry()\nEntry → ClassifiedMsg
    end

    RUST ->> RUST: build_chunks()\nClassifiedMsg[] → Chunk[]
    RUST ->> FS: scan subagent dir\n(agent-*.jsonl)
    FS -->> RUST: subagent files
    RUST ->> RUST: discover_and_link_all()\nSubagentProcess[]
    RUST ->> RUST: reconstruct_teams()
    RUST ->> RUST: OngoingChecker.is_ongoing()
    RUST ->> RUST: chunks_to_messages()\nChunk[] → DisplayMessage[]
    RUST ->> RUST: lighten_messages()\nstrip tool_input/tool_result/tool_result_json
    RUST ->> RUST: cache lightened build\n(path, size) → CachedLight

    RUST -->> FE: LoadResult\n{ messages: [], count, roles, context_tokens,\nteams, ongoing, meta, session_totals }

    FE ->> FE: render MessageList shell\n(placeholders for count rows)
    FE ->> RUST: ensureRange(tail window)\nload_session { start, limit: PAGE_SIZE }
    RUST ->> RUST: slice_light()\nreuse cached CachedLight, no re-parse
    RUST -->> FE: LoadResult\n{ messages: [...page...], ... }
    FE ->> FE: insert page into windowMessages Map

    FE ->> RUST: POST /api/session/watch\n{ path: "...session.jsonl" }
    RUST ->> RUST: start_session_watcher()\nnotify watcher on session dir
```

As the user scrolls, the virtualized list calls `ensureRange(start, end)` again, which
fetches only the missing pages in that range and evicts pages outside a small keep-margin
band around the viewport — the session's full message list is never held in memory on
the frontend.

### Detail view message fetch

Detail-view message bodies are fetched separately and on demand, never as part of the
list load. Opening a message in Detail calls a distinct backend entry point:

```mermaid
sequenceDiagram
    participant USER as User
    participant FE as Frontend
    participant RUST as Rust Backend

    USER ->> FE: click a message row
    FE ->> FE: view = "detail"\nsession.clearWindow()
    FE ->> RUST: load_message\n(or POST /api/session/message)\n{ path, index }
    RUST ->> RUST: full_message_at()\nre-parse whole session fresh\n(no cache, heavy build dropped after)
    RUST -->> FE: DisplayMessage | null\n(full tool_input/tool_result bodies)
    FE ->> FE: render Detail view
```

See "Opening the Detail view drops the list window" below for what happens to the list's
loaded pages when this fetch begins.

---

## Phase 3: Live Update Loop

```mermaid
sequenceDiagram
    participant FS as Claude Code\n(writes JSONL)
    participant WATCH as notify Watcher\n(std::thread)
    participant DEBOUNCE as Debounce Loop
    participant REBUILD as Rebuild Task\n(tokio::task)
    participant BROADCAST as AppState broadcast
    participant FE as Frontend

    FS ->> WATCH: write event on session.jsonl\nor agent-*.jsonl
    WATCH ->> WATCH: filter: is it relevant?
    WATCH ->> DEBOUNCE: send event to debounce channel
    Note over DEBOUNCE: wait for 1 000 ms quiet
    DEBOUNCE ->> REBUILD: signal (unit)

    REBUILD ->> FS: re-read session file
    FS -->> REBUILD: updated bytes
    REBUILD ->> REBUILD: parse + classify + assemble
    REBUILD ->> REBUILD: diff: message_count changed?\nor item_count? or ongoing?
    alt changed
        REBUILD ->> BROADCAST: broadcast SessionUpdatePayload
        BROADCAST ->> FE: SSE event "session-update"\n(browser/TUI path)
        BROADCAST ->> FE: Tauri emit "session-update"\n(desktop path)
        Note over BROADCAST,FE: payload is count/roles only — no bodies
        FE ->> FE: re-fetch window (browser)\nor whole session (TUI)\nre-render
    else unchanged
        REBUILD ->> REBUILD: skip broadcast
    end
```

---

## Phase 4: Session `/clear` (Truncation)

```mermaid
sequenceDiagram
    participant FS as File System\n(/clear command)
    participant REBUILD as Rebuild Task
    participant FE as Frontend

    FS ->> FS: truncate session.jsonl to 0 bytes
    REBUILD ->> FS: poll file size
    FS -->> REBUILD: new_size < prev_size
    REBUILD ->> REBUILD: detect truncation
    REBUILD ->> REBUILD: reset IncrementalTokenScanner\nclear cached chunks
    REBUILD ->> REBUILD: re-parse from byte 0\n(empty session)
    REBUILD ->> FE: session-update\n(count=0)
    FE ->> FE: re-fetch\nshow empty state
```

---

## Phase 5: Picker Refresh

```mermaid
sequenceDiagram
    participant FS as Claude Code\n(creates new session)
    participant PWATCH as Picker Watcher\n(std::thread)
    participant PTASK as Picker Task\n(tokio::task)
    participant FE as Frontend

    FS ->> FS: write new *.jsonl file\nin project dir
    PWATCH ->> PWATCH: filter: .jsonl file?
    PWATCH ->> PWATCH: debounce 1 000 ms
    PTASK ->> FE: emit "picker-refresh"\n(empty payload)

    FE ->> FE: receive picker-refresh
    FE ->> RUST: POST /api/sessions\n(re-discover)
    RUST -->> FE: updated sessions list
    FE ->> FE: update picker UI
```

---

## Session Completion Detection Flow

```mermaid
flowchart TD
    START["OngoingChecker.check(chunks, subagents)"]
    START --> MOD{"Last file\nmodified < 60 s ago?"}
    MOD -->|"yes"| ONGOING["→ ongoing"]
    MOD -->|"no"| SHUT{"Has\nshutdown msg?"}
    SHUT -->|"yes"| DONE["→ complete"]
    SHUT -->|"no"| BG{"Has running\nbackground tasks?"}
    BG -->|"yes"| ONGOING
    BG -->|"no"| SUB{"Any subagent\nongoing?"}
    SUB -->|"yes (recursive)"| ONGOING
    SUB -->|"no"| DONE
```

---

## Caching Strategy Throughout Lifecycle

There are three independent caches in `AppState`, each serving a different consumer.
They are not layers of the same cache — a request only ever touches one of them.

**`session_cache` (`SessionCache`, keyed by `(path, mtime, size)`)** — the picker's
per-file metadata cache. It backs `discover_all_project_sessions` (called from
`discover_sessions_cached`): scanning every project directory's `SessionInfo` (title,
timestamps, ongoing flag, token totals) on every picker refresh would mean re-reading
every session file's tail on every refresh. A cache hit re-applies ongoing staleness
(`apply_staleness`, ~60s) and a subagent-activity recheck to the cached `SessionInfo`
rather than reparsing, so an ongoing session still reports fresh liveness without a full
rescan. This is the cache the "First Load / Subsequent Loads / Ongoing Session" flow
below describes; it never holds `DisplayMessage[]` — only lightweight `SessionInfo`.

**`session_light_cache` (`CachedLight`, keyed by `(path, size)` — no mtime)** — the
active session's windowed-list-fetch cache, used only by `load_session_windowed`.
Scrolling the message list calls `ensureRange`, which issues one `load_session` per
missing page; without this cache each page fetch would re-parse and re-link the whole
session file. On a cache hit, `slice_light` windows the already-built `LightBuild`
(messages with heavy tool bodies stripped by `lighten_messages`) instead of rebuilding.
A different path, or the same path with a changed size (grown or truncated), invalidates
and rebuilds. Time-filtered loads bypass this cache entirely (rare, used only by the
by-id range endpoint). Only one session's build is held at a time — `AppState` has a
single `Mutex<Option<CachedLight>>` slot, not a map — so opening a new session evicts
the previous one automatically. `clear_session_build_cache()` also drops it explicitly
(see "Opening the Detail view drops the list window" below).

**No cache for `full_message_at`** — the Detail view's single-message fetch. This
deliberately re-parses the whole session fresh (`build_session`) on every call and drops
the heavy build as soon as one message is extracted. A tool-output-heavy session's full
build can be hundreds of MB; caching it for as long as the session stays open would hold
that in the Rust process the whole time. Re-parsing trades per-click latency (roughly the
cost of the session's first list load) for never persisting heavy tool-output bodies in
memory between Detail clicks.

```mermaid
flowchart LR
    subgraph First_Load["First Load (picker metadata)"]
        L1["Read JSONL tail\n(scan_session_metadata)"]
        L1 --> CACHE["SessionCache.insert\n(path, mtime, size → SessionInfo)"]
    end

    subgraph Subsequent_Loads["Subsequent Loads (same file, no change)"]
        L2["SessionCache.get\n(path, mtime, size)"]
        L2 -->|"hit"| CACHED["Return cached SessionInfo\n(re-applies ongoing staleness)"]
        L2 -->|"miss"| L1
    end

    subgraph Ongoing_Session["Ongoing Session (mtime/size changed)"]
        L3["SessionCache.get"] -->|"stale (file changed)"| L1
    end

    subgraph Picker["Picker Requests"]
        P1["SessionsCache.get"]
        P1 -->|"fresh (< 2 s)"| PCACHED["Return cached\nSessionInfo[]"]
        P1 -->|"stale"| PSCAN["Scan FS\n+ re-populate cache"]
    end

    subgraph List_Window["List window fetch (active session)"]
        W1["load_session_windowed"] --> W2{"session_light_cache\nsame (path, size)?"}
        W2 -->|"hit"| W3["slice_light()\nwindow existing LightBuild"]
        W2 -->|"miss"| W4["build_light_session()\nfull parse + lighten"]
        W4 --> W5["cache as CachedLight\n(path, size)"]
        W5 --> W3
    end

    subgraph Detail_Fetch["Detail message fetch (never cached)"]
        D1["load_message / full_message_at"] --> D2["build_session()\nfull parse, heavy bodies"]
        D2 --> D3["extract messages[index]\ndrop the rest immediately"]
    end
```

---

## Opening the Detail view drops the list window

Opening the Detail view calls `session.clearWindow()` before fetching the message body:
this drops every page currently held in the frontend's `windowMessages` map (the sparse
`Map<index, DisplayMessage>` `ensureRange` populates) since the list is no longer
visible while Detail is showing. The backend's `session_light_cache` is untouched by
this — it isn't cleared here, only the frontend's in-memory window is.

Returning to the list view does not restore anything cached: `ensureRange` runs again
for whatever range is now visible and re-fetches those pages fresh via `load_session`.
If the backend's `session_light_cache` is still warm for that `(path, size)`, the refetch
is a cheap `slice_light` rather than a full re-parse; if the file changed size in the
meantime (e.g. the session grew while Detail was open), the next fetch rebuilds it.

---

## Token Counting Across Subagents

Token deduplication ensures agents counted in both parent tool results and their own JSONL files
are not double-counted.

```mermaid
flowchart TD
    PARENT["Parent session\ntool_result.agentId=X\ntokens=1000"]
    AGENT["Agent X JSONL\n(session totals: tokens=1200)"]

    PARENT --> INSERT1["TokenSnapshot\n(agentId=X, tokens=1000, from=tool_result)"]
    AGENT --> INSERT2["TokenSnapshot\n(agentId=X, tokens=1200, from=file)"]

    INSERT1 --> BEST["insert_best_snapshot()\n→ keep file snapshot (more complete)"]
    INSERT2 --> BEST
    BEST --> TOTAL["Session total:\nparent_tokens + 1200\n(not +1000 and +1200)"]
```

---

## Error Paths

```mermaid
flowchart LR
    subgraph Parse_Error
        PE["Malformed JSONL line"]
        PE --> SKIP["Skip line\n(log warning)"]
        SKIP --> CONT["Continue with rest"]
    end

    subgraph Subagent_Missing
        SM["Referenced agent file\nnot found on disk"]
        SM --> ORPHAN["Mark as orphan\nSynthesize DisplayItem\nwith description from prompt"]
    end

    subgraph Circular_Reference
        CR["Subagent A → B → A\n(cycle)"]
        CR --> GUARD["visited set check\nstop recursion at second A"]
    end

    subgraph File_Read_Error
        FR["FS read fails\n(permissions, deleted)"]
        FR --> STOP["Skip emit\nLog error\nContinue watching"]
    end
```

---

## Platform Paths Summary

| Step                         | Desktop (Tauri IPC)           | Browser / TUI (HTTP)        |
| ---------------------------- | ----------------------------- | --------------------------- |
| Discover sessions            | `invoke("discover_sessions")` | `POST /api/sessions`        |
| Load session (windowed list) | `invoke("load_session")`      | `POST /api/session/load`    |
| Load full message (detail)   | `invoke("load_message")`      | `POST /api/session/message` |
| Watch session                | `invoke("watch_session")`     | `POST /api/session/watch`   |
| Receive updates              | `listen("session-update")`    | `EventSource /api/events`   |
| Watch picker                 | `invoke("watch_picker")`      | `POST /api/picker/watch`    |
| Picker refresh               | `listen("picker-refresh")`    | `EventSource /api/events`   |

---

## Related Specs

- [01-parser-pipeline.md](01-parser-pipeline.md) — parse stages used in phases 2 and 3
- [02-file-watcher.md](02-file-watcher.md) — watcher detail for phases 3 and 5
- [03-state-management.md](03-state-management.md) — caching used throughout
- [04-http-api.md](04-http-api.md) — HTTP endpoints used by browser/TUI path
- [05-frontend-web.md](05-frontend-web.md) — frontend hooks for phases 2 and 3
- [06-tui.md](06-tui.md) — TUI flow for phases 2 and 3
