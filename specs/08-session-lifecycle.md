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

```mermaid
sequenceDiagram
    participant USER as User
    participant FE as Frontend
    participant RUST as Rust Backend
    participant FS as File System

    USER ->> FE: select session from picker
    FE ->> RUST: POST /api/session/load\n{ path: "...session.jsonl" }

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
    RUST ->> RUST: cache result\n(path, mtime, size) → CachedSession

    RUST -->> FE: { messages, teams, ongoing, meta, session_totals }

    FE ->> FE: render MessageList
    FE ->> RUST: POST /api/session/watch\n{ path: "...session.jsonl" }
    RUST ->> RUST: start_session_watcher()\nnotify watcher on session dir
```

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
        FE ->> FE: merge new messages\nre-render
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
    REBUILD ->> FE: session-update\n(messages=[])
    FE ->> FE: clear message list\nshow empty state
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

```mermaid
flowchart LR
    subgraph First_Load["First Load"]
        L1["Read JSONL\n(full parse)"]
        L1 --> CACHE["SessionCache.insert\n(path, mtime, size → CachedSession)"]
    end

    subgraph Subsequent_Loads["Subsequent Loads (same file, no change)"]
        L2["SessionCache.get\n(path, mtime, size)"]
        L2 -->|"hit"| CACHED["Return cached\nDisplayMessage[]"]
        L2 -->|"miss"| L1
    end

    subgraph Ongoing_Session["Ongoing Session (> 60 s since last parse)"]
        L3["SessionCache.get"] -->|"stale (ongoing)"| L1
    end

    subgraph Picker["Picker Requests"]
        P1["SessionsCache.get"]
        P1 -->|"fresh (< 2 s)"| PCACHED["Return cached\nSessionInfo[]"]
        P1 -->|"stale"| PSCAN["Scan FS\n+ re-populate cache"]
    end
```

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

| Step              | Desktop (Tauri IPC)           | Browser / TUI (HTTP)      |
| ----------------- | ----------------------------- | ------------------------- |
| Discover sessions | `invoke("discover_sessions")` | `POST /api/sessions`      |
| Load session      | `invoke("load_session")`      | `POST /api/session/load`  |
| Watch session     | `invoke("watch_session")`     | `POST /api/session/watch` |
| Receive updates   | `listen("session-update")`    | `EventSource /api/events` |
| Watch picker      | `invoke("watch_picker")`      | `POST /api/picker/watch`  |
| Picker refresh    | `listen("picker-refresh")`    | `EventSource /api/events` |

---

## Related Specs

- [01-parser-pipeline.md](01-parser-pipeline.md) — parse stages used in phases 2 and 3
- [02-file-watcher.md](02-file-watcher.md) — watcher detail for phases 3 and 5
- [03-state-management.md](03-state-management.md) — caching used throughout
- [04-http-api.md](04-http-api.md) — HTTP endpoints used by browser/TUI path
- [05-frontend-web.md](05-frontend-web.md) — frontend hooks for phases 2 and 3
- [06-tui.md](06-tui.md) — TUI flow for phases 2 and 3
