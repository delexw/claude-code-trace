# Spec: Terminal UI (TUI)

**Location**: `tui/`

The TUI is a React/Ink application that provides a full-featured session browser in the terminal.
It communicates with the same Rust HTTP backend as the browser frontend over `localhost:11423`.

---

## Architecture

```mermaid
graph TB
    subgraph TUI["TUI Process (Node.js)"]
        CLI["tui/src/cli.tsx\n(entry: render &lt;App /&gt;)"]
        APP["tui/src/App.tsx\n(state machine, keyboard router)"]
        API["tui/src/api.ts\n(HTTP client)"]
        SSE["tui/src/useSSE.ts\n(SSE singleton)"]
        COMPS["Components\n(Ink/React)"]
    end

    subgraph Backend["Rust Backend (11423)"]
        HTTP["HTTP API"]
        SSESRV["SSE /api/events"]
    end

    CLI --> APP
    APP --> API
    APP --> SSE
    SSE --> SSESRV
    API --> HTTP
    APP --> COMPS
```

---

## Startup Flow

```mermaid
sequenceDiagram
    participant BIN as bin/cctrace.mjs\n--tui flag
    participant BE as Rust Backend
    participant TUI as TUI Process

    BIN ->> BE: spawn "tauri dev --headless"\n(if not already running on 11423)
    BIN ->> TUI: npm run build (tui/)
    BIN ->> BE: wait-for-backend.mjs\npoll GET /api/settings until 200
    BIN ->> TUI: node dist/tui/src/cli.js\n(stdio: inherit)

    TUI ->> BE: GET /api/settings (project dirs)
    TUI ->> BE: POST /api/sessions (discover)
    TUI ->> BE: GET /api/events (SSE subscribe)
    TUI ->> BE: POST /api/picker/watch

    loop on picker-refresh SSE event
        TUI ->> BE: POST /api/sessions (refresh)
    end
```

---

## View State Machine

```mermaid
stateDiagram-v2
    [*] --> picker : app init
    picker --> list : Enter (select session)
    list --> detail : Enter (select message)
    list --> team : T key
    list --> debug : D key
    detail --> subagent_list : Enter (on Subagent item)
    subagent_list --> subagent_detail : Enter (on message)
    subagent_detail --> subagent_list : q/Esc
    subagent_list --> detail : q/Esc
    detail --> list : q/Esc
    team --> list : q/Esc
    debug --> list : q/Esc
    list --> picker : q/Esc
```

State variables managing nested navigation:

```
view: "picker" | "list" | "detail" | "team" | "debug"
subagentItem: DisplayItem | null    (item that was drilled into)
subagentDetailMsg: DisplayMessage | null  (message inside subagent drill)
```

---

## Layout

```mermaid
graph TB
    subgraph Terminal["Terminal (full height/width)"]
        IB["InfoBar (3 rows, bordered)"]
        subgraph MIDDLE["Middle (flex-grow: 1)"]
            PT["ProjectTree\n(dynamic width, max 40%)"]
            VIEW["Active View Component\n(flex-grow: 1)"]
        end
        KB["KeybindBar (3 rows)"]
    end
    IB --> MIDDLE
    MIDDLE --> KB
```

---

## Keyboard Routing

All keyboard input is handled by a **single `useInput` listener** in `App.tsx`.
The handler is stored in a `useRef` so that Ink's `useInput` never re-subscribes on re-renders.

```mermaid
flowchart TD
    KEY["useInput callback"]
    KEY --> VIEW{current view?}
    VIEW -->|"picker"| PK["Picker keys\n(j/k/g/G/Enter/q)"]
    VIEW -->|"list"| LS["List keys\n(j/k/g/G/Tab/Enter/t/d/e/c/q/Esc)"]
    VIEW -->|"detail"| DT["Detail keys\n(j/k/Tab/Enter/u/d/e/c/q/Esc)"]
    VIEW -->|"subagent_list"| SA_L["Subagent list keys"]
    VIEW -->|"subagent_detail"| SA_D["Subagent detail keys"]
    VIEW -->|"team"| TM["Team keys (q/Esc)"]
    VIEW -->|"debug"| DB["Debug keys (j/k/q/Esc)"]
    KEY --> SIDEBAR{"sidebar\nfocus?"}
    SIDEBAR -->|"yes"| PT["ProjectTree keys\n(h/l/j/k/Space/Enter)"]
```

---

## Component Inventory

### `SessionPicker`

Groups sessions by date bucket (Today / Yesterday / This Week / This Month / Older).

```mermaid
flowchart TD
    SESSIONS["SessionInfo[]"]
    SESSIONS --> DATE_BUCKET["dategroup()\nToday / Yesterday\nThis Week / This Month / Older"]
    DATE_BUCKET --> WINDOW["stableWindow()\nwindow of ~10 sessions"]
    WINDOW --> RENDER["Ink render\n(Box/Text)"]
```

Per-session card layout:

```
▸ [model] branch · tokens · cost                   2025-05-01 10:00
  First message line...
  ─────────────────────────────────────
```

---

### `MessageList`

Lists messages from a loaded session with windowed rendering.

```mermaid
flowchart LR
    MSGS["DisplayMessage[]"]
    MSGS --> WINDOW["stableWindow()\n~3-4 msgs"]
    WINDOW --> RENDER["Per message:\n- compact → separator\n- system → hrule\n- user/claude → bordered box"]
    RENDER --> STATS["StatsBar\n(thinking/tool/output, tokens, duration)"]
```

---

### `DetailView`

Renders items from a selected message with expandable bodies and scroll.

```mermaid
flowchart TD
    ITEMS["DisplayItem[]"]
    ITEMS --> WINDOW["stableWindow()"]
    WINDOW --> HEADER["Item header\n(cursor | icon | name | summary | tokens | duration)"]
    HEADER --> BODY{expanded?}
    BODY -->|"yes"| DETAIL["Item body\n(type-specific rendering)"]
    BODY -->|"no"| SKIP["collapsed"]
```

Item body rendering by type:

| `item_type`       | Body content                        |
| ----------------- | ----------------------------------- |
| `Thinking`        | scrollable text block               |
| `Output`          | pretty-printed JSON or markdown     |
| `ToolCall`        | input JSON + hrule + result/error   |
| `Subagent`        | agent ID, desc, prompt, last result |
| `TeammateMessage` | plain text                          |
| `HookEvent`       | hook name, cmd, metadata key-values |

---

### `ProjectTree` (TUI)

Sidebar showing project hierarchy with expand/collapse.

```mermaid
flowchart LR
    SESSIONS["all sessions"]
    SESSIONS --> BUILD["buildProjectTree()\n(shared/projectTree.ts)"]
    BUILD --> NODES["ProjectNode[]\n(hierarchical)"]
    NODES --> RENDER["Ink render\n(chevron + indent + name + count)"]
```

Keyboard navigation:

- `h` / `l` — move keyboard focus between sidebar and main panel
- `j` / `k` — navigate tree nodes
- `Space` — expand/collapse a group node
- `Enter` — select a project (filter sessions)

---

### `InfoBar` (TUI)

```
┌─────────────────────────────────────────────────────────┐
│ my-app · abc12345 · * main · default │ 45.2% · 8.3k · $0.03 ● │
└─────────────────────────────────────────────────────────┘
```

Context colour:

- `< 50%` → accent blue
- `50–80%` → orange
- `> 80%` → red

---

## SSE Integration (`useSSE.ts`)

Singleton `EventSource` with reference counting, shared across all components.

```mermaid
sequenceDiagram
    participant C1 as Component A
    participant C2 as Component B
    participant SS as useSSE singleton
    participant SRV as /api/events

    C1 ->> SS: acquireSource() [refcount=1]
    SS ->> SRV: new EventSource("/api/events")
    C2 ->> SS: acquireSource() [refcount=2]
    Note over SS: no new connection

    C1 ->> SS: releaseSource() [refcount=1]
    Note over SS: still open

    C2 ->> SS: releaseSource() [refcount=0]
    SS ->> SRV: source.close()
```

`useSSE<T>(event, handler)` subscribes to a named event and calls `handler(data: T)` on receipt.

### Picker Auto-Refresh

The TUI subscribes to the `"picker-refresh"` event (the backend's actual event name) and
re-fetches the session list via `api.discoverSessions(dirs)` on each signal. The dirs are
remembered in a `projectDirsRef` populated when the initial discovery succeeds — this mirrors
the web frontend pattern in `src/hooks/usePicker.ts`.

> Historical note: an earlier version subscribed to `"picker-update"` and tried to destructure
> `payload.sessions`. Because the backend has always emitted `"picker-refresh"` with an empty
> payload (`watcher.rs:340`), picker auto-refresh in the TUI was a no-op. Fixed in this branch.

---

## Windowing (`lib/window.ts`)

Prevents "shaking" lists when content changes.

```mermaid
flowchart LR
    SEL["selectedIndex"]
    WIN["windowStart\n(cached per viewId)"]
    TOTAL["totalItems"]
    SEL --> CALC["stableWindow()"]
    WIN --> CALC
    TOTAL --> CALC
    CALC --> OUT["{ start, end }\nonly moves when\nwithin 1-item margin"]
    OUT --> WIN
```

The window is only scrolled when selection reaches within 1 item of the current edge —
not on every selection change. This eliminates the "vertical shaking" caused by re-centering.

---

## Theme (`lib/theme.ts`)

256-color palette (hex values mapping to ANSI terminal colors):

| Role            | Color     |
| --------------- | --------- |
| Primary text    | `#d0d0d0` |
| Secondary text  | `#8a8a8a` |
| Accent (Claude) | `#5fafff` |
| Model: Opus     | `#ff5f87` |
| Model: Sonnet   | `#5fafff` |
| Model: Haiku    | `#87d787` |
| Ongoing         | `#5faf00` |
| Token high      | `#ff8700` |
| Error           | `#ff0000` |
| Thinking        | `#767676` |
| Tool            | `#5fafff` |
| Agent           | `#5fafaf` |
| Hook            | `#ffdf00` |

---

## Build & Distribution

```mermaid
flowchart LR
    SRC["tui/src/**/*.tsx"]
    SRC -->|"tsc"| DIST["tui/dist/"]
    DIST -->|"node dist/tui/src/cli.js"| RUN["Terminal UI"]
    SHARED["shared/**/*.ts"] -->|"tsc → dist/shared/"| DIST
```

The TUI imports shared modules via relative paths (`../../shared/...`) that survive compilation
into `dist/` because the shared output is placed at `dist/shared/`.

---

## Related Specs

- [04-http-api.md](04-http-api.md) — API consumed by TUI
- [05-frontend-web.md](05-frontend-web.md) — web frontend sharing same types
- [07-data-types.md](07-data-types.md) — shared type system
- [13-item-rendering.md](13-item-rendering.md) — per-type item rendering (icons, bodies, selection accent)
