# Spec: Terminal UI (TUI)

**Location**: `tui-py/`

The TUI is a Python application built on the [Textual](https://textual.textualize.io/)
framework. It connects to the same Rust HTTP backend as the desktop and web frontends
over `localhost:11423` (`API_BASE` in `tui-py/app.py`).

The TUI replaces the earlier React/Ink implementation that lived in `tui/`. The Python
port keeps the same domain types and the same HTTP/SSE contract — only the rendering
layer changed.

---

## Architecture

```mermaid
graph TB
    subgraph TUI["TUI Process (Python)"]
        MAIN["tui-py/main.py\n(entry: CCTraceApp().run())"]
        APP["tui-py/app.py\n(CCTraceApp: reactive state, BINDINGS, watchers)"]
        API["tui-py/api.py\n(httpx async client)"]
        SSE["tui-py/sse.py\n(SSEClient — background thread)"]
        WIDGETS["widgets/\n(Textual widgets)"]
    end

    subgraph Backend["Rust Backend (11423)"]
        HTTP["HTTP API"]
        SSESRV["SSE /api/events"]
    end

    MAIN --> APP
    APP --> API
    APP --> SSE
    SSE --> SSESRV
    API --> HTTP
    APP --> WIDGETS
```

---

## Startup Flow

```mermaid
sequenceDiagram
    participant BIN as bin/cctrace.mjs\n--tui flag
    participant BE as Rust Backend
    participant TUI as Python TUI

    BIN ->> BE: spawn "tauri dev --headless"\n(if not already running on 11423)
    BIN ->> TUI: pip install -r tui-py/requirements.txt --quiet
    BIN ->> BE: wait-for-backend.mjs (poll GET /api/settings)
    BIN ->> TUI: python3 tui-py/main.py

    TUI ->> BE: GET /api/settings (project dirs)
    TUI ->> BE: POST /api/sessions (discover)
    TUI ->> BE: GET /api/events (SSE subscribe — background thread)
    TUI ->> BE: POST /api/picker/watch

    loop on picker-refresh SSE event
        TUI ->> BE: POST /api/sessions (refresh)
    end

    loop on session-update SSE event
        TUI ->> APP: _on_session_update(payload)
    end
```

---

## View State Machine

```mermaid
stateDiagram-v2
    [*] --> picker : app init
    picker --> list : Enter (after _load_session completes)
    list --> detail : Enter (on a message)
    list --> team : t key
    list --> debug : (debug action)
    detail --> list (subagent_list) : Enter (on a Subagent item)
    list (subagent_list) --> detail (subagent_detail) : Enter (on inner msg)
    detail (subagent_detail) --> list (subagent_list) : q/Esc
    list (subagent_list) --> detail : q/Esc
    detail --> list : q/Esc
    team --> list : q/Esc
    debug --> list : q/Esc
    list --> picker : q/Esc
```

State lives on `CCTraceApp` as Textual reactives:

```
view: "picker" | "list" | "detail" | "team" | "debug"
messages, teams, ongoing, meta, totals, expanded_messages, expanded_items
subagent_item, subagent_detail_msg
```

Each reactive has a `watch_<name>` callback that calls the matching `_sync_<view>`
method to push state into the corresponding widget.

---

## Layout

```mermaid
graph TB
    subgraph App["Screen"]
        H["Header"]
        subgraph MAIN["Horizontal #main-area"]
            PT["ProjectTree (#project-tree)\nwidth: 30 (resizable)"]
            SR["SidebarResizer (1 col)"]
            subgraph CS["ContentSwitcher (#content-switcher)"]
                SP["SessionPicker (#picker)"]
                ML["MessageList (#list)"]
                DV["DetailView (#detail)"]
                TB["TeamBoard (#team)"]
                DB["DebugViewer (#debug)"]
            end
        end
        F["Footer (Textual built-in)"]
    end
```

---

## Keyboard Routing

Textual collects bindings from the focused widget chain + App and renders them
in the Footer. Bindings come from two layers:

```mermaid
flowchart TD
    KEY["Key press"]
    KEY --> APP_B{Match in\nApp.BINDINGS?}
    APP_B -->|"q / Esc / Tab / e / c / g / G / u / d / r / h / l"| APP_ACT["App action method"]
    APP_B -->|"j / k / Enter"| DELEGATE["action_focused_*\n→ _delegate_to_focused\n→ focused widget's action_cursor_up\n  / cursor_down / select_cursor"]
    APP_B -->|"no"| WIDGET_B[Match in focused widget's BINDINGS]
    WIDGET_B -->|"t / s on MessageList"| WIDGET_ACT["MessageList action"]
```

**App.BINDINGS** (in `tui-py/app.py`):

| Key       | Action                            | Description   |
| --------- | --------------------------------- | ------------- |
| `j`       | `focused_cursor_down`             | ↓             |
| `k`       | `focused_cursor_up`               | ↑             |
| `enter`   | `focused_select_cursor`           | Open          |
| `q`       | `back_or_quit`                    | priority=True |
| `escape`  | `back_or_quit`                    | priority=True |
| `tab`     | `toggle_expand`                   | priority=True |
| `e`       | `expand_all`                      | priority=True |
| `c`       | `collapse_all`                    | priority=True |
| `g` / `G` | `jump_first` / `jump_last`        | priority=True |
| `u` / `d` | `scroll_up` / `d_action`          | priority=True |
| `r`       | `refresh`                         | priority=True |
| `h` / `l` | `focus_sidebar` / `focus_content` | priority=True |

Putting `j/k/Enter` at the App level (not on each list widget) ensures they
always render in the Footer. The action methods (`action_focused_cursor_up`,
etc.) call `_delegate_to_focused(name)` which invokes
`action_<name>` on `self.focused`.

---

## Shared list base — `HighlightListView`

```mermaid
classDiagram
    class HighlightListView {
        +DEFAULT_CSS
        +ensure_highlight()
        -_first_selectable_index()
    }
    class SessionPicker
    class MessageList
    class _ItemListView
    HighlightListView <|-- SessionPicker
    HighlightListView <|-- MessageList
    HighlightListView <|-- _ItemListView
```

All three list pages extend `HighlightListView` (`widgets/highlight_list.py`).
The base class owns:

- **Highlight CSS** — `ListItem.-highlight` and the focused variant both paint
  with `$block-cursor-blurred-background` (`#0178D44C`). `background-tint`
  on focus is forced to transparent so the focused tint doesn't lighten the
  selection.
- **`ensure_highlight()`** — sets `index` to the first non-disabled child
  if no row is currently highlighted. Idempotent. Skips the disabled
  header/section rows that SessionPicker puts at the top of its list.

This eliminates the bug class where each list page had its own copy of the
highlight CSS / index initialization and fell out of sync when only one was
patched.

---

## Component Inventory

### `SessionPicker` (`widgets/session_picker.py`)

Inherits `HighlightListView`. Groups sessions by date bucket
(Today / Yesterday / This Week / This Month / Older), with disabled header
rows for the bucket title and an aggregate "Sessions (N)" header at the top.

```mermaid
flowchart TD
    SESSIONS["SessionInfo[]"]
    SESSIONS --> DATE_BUCKET["_group_by_date()\nToday / Yesterday\nThis Week / This Month / Older"]
    DATE_BUCKET --> RENDER["Per session:\n_render_session() → Rich Group"]
    RENDER --> ENSURE["ensure_highlight()\nlands on first selectable session\n(skips disabled headers)"]
```

Uses `self.loading = True` (Textual's built-in `LoadingIndicator` overlay)
while discovery / re-discovery is in flight. Loading overlay is also raised
while a session is being loaded — see `_load_session` below.

---

### `MessageList` (`widgets/message_list.py`)

Inherits `HighlightListView`. Renders each `DisplayMessage` as a 3-column
Rich `Table` (accent rail · content · right-aligned stats).

```mermaid
flowchart LR
    SSE["SSE session-update"] --> WM[watch_messages]
    WM --> SML[_sync_message_list]
    SML --> WORKER["run_worker(populate(...),\nexclusive=True,\ngroup='populate_msglist')"]
    WORKER --> POP["MessageList.populate (async)"]
```

`populate()` is **async** and the caller schedules it in an exclusive worker
group so back-to-back populates serialise. Three branches:

1. **`new_total == 0`** → mount a disabled "No messages loaded" placeholder.
2. **`old_total == 0` or `node_count != old_total`** → full rebuild
   (`await self.clear()` then `await self.append(...)` per row),
   followed by `ensure_highlight()`.
3. **Otherwise** → incremental diff: refresh only the rows whose content or
   expansion state changed; append new tail rows; remove dropped tail rows.

> Historical bug: an earlier sync `populate` raced with its own deferred
> `clear()/append()` calls (both return `AwaitComplete`) so rapid back-to-back
> populates (`self.messages = []` then `= real` during `_load_session`)
> produced 27 / 53 children for 26 messages and the deferred `clear()` later
> snapped the index back to `None`. The async + exclusive-worker design fixes
> the race; the incremental diff path preserves the user's cursor through SSE
> updates.

---

### `DetailView` (`widgets/detail_view.py`)

Pane that opens when the user presses Enter on a message in `MessageList`.
Wrapped in a single bordered container (`border: round $border`) so the
header and items list read as one panel.

```mermaid
graph TB
    subgraph DV["DetailView (bordered)"]
        H1["#msg-heading\n── RESPONSE ──"]
        MC["#msg-content (Collapsible)\nrole title + Markdown body\nborder-bottom divider"]
        H2["#items-heading\n── STEP (N) ──"]
        IL["_ItemListView #items-list\n(extends HighlightListView)"]
    end
    H1 --> MC --> H2 --> IL
```

`populate()` classifies the call:

- If `prev_items == new_items` (only an expansion bit flipped) →
  `_sync_expanded_only()` walks each `#item-N` Collapsible and updates
  `collapsed` in place. **The ListView is never cleared**, so `lv.index`
  keeps the user's cursor where it was.
- Otherwise → eager synchronous clear of `#items-list` and `#msg-content`,
  set `self.loading = True`, schedule `_rebuild` via `call_after_refresh`.
  `_rebuild` mounts the new message body + items list and clears
  `self.loading` in a `finally` block.

Headings:

- `#msg-heading` shows `── RESPONSE ──` when a message is selected, hidden
  otherwise.
- `#items-heading` shows `── STEP (N) ──` with the live item count; hidden
  when the message has no items.

Item body rendering by type (unchanged from previous spec):

| `item_type`       | Body content                        |
| ----------------- | ----------------------------------- |
| `Thinking`        | scrollable Markdown                 |
| `Output`          | pretty-printed JSON or Markdown     |
| `ToolCall`        | input JSON + result/error           |
| `Subagent`        | agent ID, desc, prompt, last result |
| `TeammateMessage` | plain text                          |
| `HookEvent`       | hook name, cmd, metadata key-values |

---

### `ProjectTree` (`widgets/project_tree.py`)

Sidebar showing project hierarchy with expand/collapse. Uses Textual's
built-in `Tree`. Highlight is `$accent 50%` on `.tree--cursor`.

Keyboard navigation:

- `h` / `l` — focus sidebar / content pane (App-level)
- `j` / `k` — navigate tree nodes (App-level delegates to focused Tree)
- `Space` — expand/collapse a group node
- `Enter` — select a project (filter sessions)

---

### `InfoBar` (`widgets/info_bar.py`)

```
┌────────────────────────────────────────────────────────────────┐
│ my-app · abc12345 · * main · default │ 45.2% · 8.3k · $0.03 ● │
└────────────────────────────────────────────────────────────────┘
```

Context percentage colour:

- `< 50%` → accent blue
- `50–80%` → orange
- `> 80%` → red

---

## Session loading flow

```mermaid
sequenceDiagram
    participant USER as User
    participant SP as SessionPicker
    participant APP as CCTraceApp
    participant ML as MessageList
    participant BE as Rust Backend

    USER ->> SP: Enter on a session row
    APP ->> APP: on_list_view_selected
    APP ->> APP: run_worker(_load_session(path),\nexclusive=True, group='load_session')
    APP ->> SP: picker.loading = True (LoadingIndicator overlay)
    APP ->> BE: GET /api/session/load
    BE -->> APP: LoadResult
    APP ->> APP: messages / teams / meta / totals = result.*\n(watch_messages skips since view != "list")
    APP ->> ML: await ml.populate(messages, ...)
    Note over ML: full rebuild + ensure_highlight\nML is fully built before view flips
    APP ->> BE: POST /api/session/watch
    APP ->> APP: self.view = "list"
    APP ->> SP: picker.loading = False
```

The view flips to `"list"` **only after** `MessageList` is fully populated.
This avoids the "j/k does nothing for a couple of seconds" window where the
user lands on a half-built list pane.

---

## SSE integration (`tui-py/sse.py`)

`SSEClient` is a thin httpx-based client running on a background thread.
It maintains a per-event handler dict and dispatches each `event:` /
`data:` pair to the registered async handler via `app.call_from_thread`.

Subscribed events:

- `picker-refresh` — re-runs `api.discover_sessions(dirs)`.
- `session-update` — calls `_on_session_update(payload)` on the App, which
  updates `messages` / `teams` / `ongoing` / `meta` / `totals` and calls
  `_sync_all_widgets()`.

---

## Theme (`tui-py/theme.py`)

Maps domain roles to Rich/Textual colour tokens. Same colour palette as the
previous Ink implementation (Primary text `#d0d0d0`, accent Claude `#5fafff`,
Opus `#ff5f87`, Sonnet `#5fafff`, Haiku `#87d787`, Ongoing `#5faf00`,
Token-high `#ff8700`, Error `#ff0000`, Thinking `#767676`, Tool `#5fafff`,
Agent `#5fafaf`, Hook `#ffdf00`).

The CSS lives in `tui-py/cctrace.tcss` and is loaded via `App.CSS_PATH`. The
file is intentionally small — most per-widget styling sits in `DEFAULT_CSS`
on the widget class. The shared highlight rules live on `HighlightListView`,
not duplicated in the global CSS.

---

## Tests (`tui-py/tests/`)

`pytest` + `pytest-asyncio` (configured in `pytest.ini`).

| File                     | Covers                                                                                                                                                                           |
| ------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `test_highlight_list.py` | `ensure_highlight` policy, disabled-row skipping, idempotence, shared highlight color resolves to `$block-cursor-blurred-background`.                                            |
| `test_message_list.py`   | Async populate race-safety (no duplicated rows after empty→real), full-rebuild sets index=0, incremental diff preserves the user's cursor, tail append, empty-state placeholder. |
| `test_detail_view.py`    | Bordered container, RESPONSE/STEP headings render with live counts and hide when there is no message / no items.                                                                 |

Run with:

```bash
cd tui-py && python -m pytest tests/
```

---

## Build & Distribution

```mermaid
flowchart LR
    SRC["tui-py/**/*.py"] -->|"python3"| RUN["Terminal UI"]
    BIN["bin/cctrace.mjs --tui"] -->|"pip install -r requirements.txt"| DEPS["textual + httpx + ..."]
    DEPS --> RUN
```

No build step — the source is run directly with the system Python (3.11+).
`pip install -r tui-py/requirements.txt` installs runtime dependencies on
first launch.

---

## Related Specs

- [04-http-api.md](04-http-api.md) — API consumed by the TUI
- [05-frontend-web.md](05-frontend-web.md) — web frontend sharing same types
- [07-data-types.md](07-data-types.md) — shared type system
- [13-item-rendering.md](13-item-rendering.md) — per-type item rendering
