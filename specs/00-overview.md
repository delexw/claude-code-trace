# Claude Code Trace — Architecture Overview

**claude-code-trace** is a multi-platform session viewer for Claude Code JSONL transcripts.
It ships as a Tauri desktop app, a browser-served web app, and a terminal UI (TUI).
All three frontends share a single Rust backend that parses JSONL files, watches for live changes,
and exposes them over both Tauri IPC and HTTP/SSE.

---

## System Context

```mermaid
graph TB
    subgraph Claude_Code["Claude Code (external)"]
        JSONL["~/.claude/projects/\n*.jsonl session files"]
    end

    subgraph cctrace["claude-code-trace process"]
        RUST["Rust Backend\n(parser + watcher + HTTP)"]
    end

    subgraph Frontends
        TAURI["Tauri Desktop\n(WebView + IPC)"]
        WEB["Browser\n(Vite / HTTP)"]
        TUI["Terminal UI\n(Ink / React)"]
    end

    JSONL -->|"FS events"| RUST
    RUST -->|"Tauri emit"| TAURI
    RUST -->|"SSE /api/events\nREST /api/*"| WEB
    RUST -->|"SSE /api/events\nREST /api/*"| TUI
```

---

## Major Subsystems

| Layer              | Location                    | Responsibility                                   |
| ------------------ | --------------------------- | ------------------------------------------------ |
| JSONL Parser       | `src-tauri/src/parser/`     | Parse, classify, assemble conversation turns     |
| File Watcher       | `src-tauri/src/watcher.rs`  | FS events → debounce → re-parse → broadcast      |
| App State          | `src-tauri/src/state.rs`    | In-memory caches, watcher handles, SSE broadcast |
| Tauri Commands     | `src-tauri/src/commands/`   | IPC layer for desktop frontend                   |
| HTTP API           | `src-tauri/src/http_api.rs` | REST + SSE for browser and TUI                   |
| Frontend Converter | `src-tauri/src/convert.rs`  | Internal → JSON-serialisable display types       |
| Web Frontend       | `src/`                      | React components, hooks, keyboard navigation     |
| TUI                | `tui/`                      | Ink/React terminal rendering                     |
| Shared             | `shared/`                   | Types, project tree builder, format helpers      |
| CLI Launcher       | `bin/cctrace.mjs`           | Mode selector (desktop / web / tui / headless)   |

---

## Top-Level Data Flow

```mermaid
flowchart LR
    FS["JSONL files\non disk"]

    subgraph Rust["Rust Backend"]
        W["Watcher\n(notify crate)"]
        P["Parser Pipeline\nentry→classify→chunk"]
        S["Subagent / Team\nReconstruction"]
        C["Converter\ninternal→DisplayMsg"]
        ST["AppState\n(caches, broadcast)"]
    end

    subgraph Clients
        D["Desktop\n(Tauri IPC)"]
        B["Browser\n(HTTP REST+SSE)"]
        T["TUI\n(HTTP REST+SSE)"]
    end

    FS -->|"file events"| W
    W -->|"1 s debounce"| P
    P --> S
    S --> C
    C --> ST
    ST -->|"Tauri emit"| D
    ST -->|"SSE broadcast"| B
    ST -->|"SSE broadcast"| T
```

---

## All Specs

| #   | File                                               | Topic                                                               |
| --- | -------------------------------------------------- | ------------------------------------------------------------------- |
| 01  | [01-parser-pipeline.md](01-parser-pipeline.md)     | JSONL parsing: entry → classify → chunk → subagent → team → convert |
| 02  | [02-file-watcher.md](02-file-watcher.md)           | File watching, debounce, session watcher vs picker watcher          |
| 03  | [03-state-management.md](03-state-management.md)   | AppState, session cache, SSE broadcast                              |
| 04  | [04-http-api.md](04-http-api.md)                   | REST endpoints, SSE contract, Tauri IPC mirror                      |
| 05  | [05-frontend-web.md](05-frontend-web.md)           | React hooks and components (web/desktop)                            |
| 06  | [06-tui.md](06-tui.md)                             | Terminal UI (Ink/React), keyboard routing, windowing                |
| 07  | [07-data-types.md](07-data-types.md)               | Shared TypeScript types, Rust serialisation                         |
| 08  | [08-session-lifecycle.md](08-session-lifecycle.md) | End-to-end session loading, live update, truncation                 |
| 09  | [09-subagent-linking.md](09-subagent-linking.md)   | Four-phase subagent linking algorithm                               |
| 10  | [10-tool-taxonomy.md](10-tool-taxonomy.md)         | Tool categorisation and summary generation                          |
| 11  | [11-project-tree.md](11-project-tree.md)           | Project key parsing and tree construction                           |
| 12  | [12-cli-launcher.md](12-cli-launcher.md)           | CLI mode selection, service installer, health check                 |
| 13  | [13-item-rendering.md](13-item-rendering.md)       | Per-type item rendering, expansion, selection, auto-scroll          |
