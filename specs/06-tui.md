# Spec: Terminal UI (TUI)

**Location**: `tui-py/`

The TUI is a Python application built on the [Textual](https://textual.textualize.io/)
framework. It connects to the same Rust HTTP backend as the desktop and web frontends
over `localhost:11423` (`API_BASE` in `tui-py/app.py`). The TUI is the built-in `tui` client:
every request carries its signed credential as an `X-CCTrace-Token` header. `tui-py/auth.py`
reads it from `clients/tui.jwt` in the config dir (written by the backend, never by the TUI) and
`auth_headers()` is re-evaluated per request and per SSE (re)connect, so a `tui` credential
reissued from the Settings UI is picked up without restarting the TUI. A 401 surfaces as
`api.ApiAuthError` naming the file and Settings → Accepted clients. See
[04-http-api.md](04-http-api.md#authentication).

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
        TUI ->> APP: _on_session_update(_payload)
        APP ->> BE: GET /api/session/load (re-fetch)
    end
```

`session-update` is a lightweight refresh signal (message count + roles,
never bodies — see [04-http-api.md](04-http-api.md)). `_on_session_update`
ignores the payload's contents and re-fetches the whole session via
`load_session` instead, so `self.messages` stays authoritative even though
the TUI doesn't paginate the way the web frontend does.

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
        IP["IndexProgressBar (#index-progress)\nheight: 1"]
        F["Footer (Textual built-in)"]
    end
    subgraph Modals["Modal screens (pushed over the above)"]
        AS["AnalyticsSettingsScreen"]
        EP["EfficiencyPrivacyScreen"]
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

| Key       | Action                            | Description      |
| --------- | --------------------------------- | ---------------- |
| `j`       | `focused_cursor_down`             | ↓                |
| `k`       | `focused_cursor_up`               | ↑                |
| `enter`   | `focused_select_cursor`           | Open             |
| `q`       | `back_or_quit`                    | priority=True    |
| `escape`  | `back_or_quit`                    | priority=True    |
| `tab`     | `toggle_expand`                   | priority=True    |
| `e`       | `expand_all`                      | priority=True    |
| `c`       | `collapse_all`                    | priority=True    |
| `g` / `G` | `jump_first` / `jump_last`        | priority=True    |
| `u` / `d` | `scroll_up` / `d_action`          | priority=True    |
| `r`       | `refresh`                         | priority=True    |
| `h` / `l` | `focus_sidebar` / `focus_content` | priority=True    |
| `y`       | `copy_resume`                     | list/detail only |
| `a`       | `analyse_session`                 | picker only      |
| `,`       | `analytics_settings`              | picker only      |

`check_action` gates the last three. `y` needs a session in context, and the two Jev keys
only apply where sessions are listed; keeping them off the global footer matters because it
already overflows on narrow terminals.

While a modal screen is open (`len(self.screen_stack) > 1`) `check_action` also returns
`False` for every action in `OWN_ACTIONS` — the set derived from `BINDINGS`, so a key added
to the table above cannot be forgotten. Without it the priority bindings would steal
`Escape` from the modal and swallow characters (`q`, `j`, `c`) typed into its inputs. It is
deliberately scoped to **this app's own** actions: Textual's app-level actions live on the
same namespace, and a modal's inherited `Tab` binding runs `app.focus_next`, so disabling
everything left the modal navigable by mouse only.

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

A session that has been analysed by Jev gets a third line from
`_analysis_line(job, summary)`: the score (`% Jev 82`, coloured by the same
bands as the web badge, `· stale` when the transcript moved on), a running
analysis (`Analysing ██████░░░░ 65% · <phase message>`), or a failure with the
backend's reason. `update_analysis(jobs, summaries)` re-renders only the rows
that have analysis state — a full `populate()` would clear the list and drop
the cursor, and a running job reports progress every second or so.

---

### `IndexProgressBar` (`widgets/index_progress.py`)

One line between the content pane and the Footer, fed by the `index-progress`
SSE event. Reading every session file takes minutes on a large directory; the
picker fills in newest-first as the walk goes and this says how much is still
to come.

Measured in **bytes**, not files — one huge session among three thousand small
ones would otherwise sit at 99% with nearly all the reading still to do. The
label counts bytes too, so the number and the bar cannot contradict each other.
The row is kept whether a scan is running or not (blank when idle or done), so
starting a scan never shifts the layout above it.

```text
███░░░░░░░░░░░░░░░░░ 16%  9 sessions · 23.2 MB / 144.6 MB
```

---

### `MessageList` (`widgets/message_list.py`)

Inherits `HighlightListView`. Renders each `DisplayMessage` as a 3-column
Rich `Table` (accent rail · content · right-aligned stats).

```mermaid
flowchart LR
    SSE["SSE session-update\n(signal only)"] --> OSU["_on_session_update\n(re-fetches via load_session)"]
    OSU --> WM[watch_messages]
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
- Otherwise → eager synchronous clear of **`#items-list` only**, set
  `self.loading = True`, schedule `_rebuild` via `call_after_refresh`.
  `_rebuild` mounts the new message body + items list and clears
  `self.loading` in a `finally` block. `#msg-content` (the message-header
  Collapsible) is deliberately left untouched here — the `LoadingIndicator`
  already covers the whole pane, and emptying a `Collapsible`'s children
  makes Textual treat it as a non-container, so `Widget.render()` falls
  back to printing its raw CSS identifier
  (`Collapsible#msg-content.-collapsed`) for one frame before `_rebuild`
  remounts real content.

Headings:

- `#msg-heading` shows `── RESPONSE ──` when a message is selected, hidden
  otherwise.
- `#items-heading` shows `── STEP (N) ──` with the live item count; hidden
  when the message has no items.

### On-demand full message fetch

`self.messages` (from `load_session`) has `tool_input` / `tool_result` /
`tool_result_json` stripped on every item, to keep the list view light — the
same light/full split the web frontend uses (see
[08-session-lifecycle.md](08-session-lifecycle.md)). Opening Detail on
message `idx` doesn't index into `self.messages`; instead
`_sync_detail_view_for_message_index`:

1. Clears `self._detail_full_message` and calls `_sync_detail_view()` so the
   pane shows its loading state, not stale data from the previous message.
2. Kicks off `_fetch_detail_message(path, idx)` in an exclusive worker
   (`group="load_detail_message"`), which calls `POST /api/session/message`
   via `api.load_message` and stores the full `DisplayMessage` on
   `self._detail_full_message`.
3. The fetch guards against staleness: if the session, message index, or
   view changed while the request was in flight, the result is discarded.

`_active_detail_message()` returns `self._detail_full_message` (or the
subagent detail message when drilled into one) rather than indexing
`self.messages`. Leaving Detail (`action_back_or_quit`'s top-level branch)
clears `self._detail_full_message` so the heavy body isn't held after the
view switches back to the list.

### Item body rendering by type

| `item_type`       | Body content                                                                                  |
| ----------------- | --------------------------------------------------------------------------------------------- |
| `Thinking`        | scrollable Markdown                                                                           |
| `Output`          | pretty-printed JSON or Markdown                                                               |
| `ToolCall`        | unboxed "Input"/"Result" label + boxed, wrapped JSON (`Edit` renders a coloured diff instead) |
| `Subagent`        | agent ID, desc, prompt, last result                                                           |
| `TeammateMessage` | plain text                                                                                    |
| `HookEvent`       | hook name inline, then unboxed "cmd"/"metadata" labels + boxed JSON                           |

`ToolCall`/`HookEvent` JSON used to render as Markdown fenced code blocks,
whose `MarkdownFence` widget forces `overflow: scroll hidden` — long lines
scrolled horizontally instead of wrapping. They now render via
`textual.highlight.highlight()` (the same tokenizer Markdown uses
internally) inside a plain `Static.diff-block`, which wraps at the pane
width like the web UI's JSON viewer while keeping JSON syntax colours.

The "Input"/"Result"/"cmd"/"metadata" labels are a separate unbordered
`Static.item-label`, not a `Markdown` widget — `_ItemListView`'s CSS borders
every `Markdown` widget, so a `Markdown("**Input**")` label used to get its
own tiny bordered box above the content box. Only `Static.diff-block` (the
JSON/diff content itself) is boxed now.

---

### `ProjectTree` (`widgets/project_tree.py`)

Sidebar showing project hierarchy with expand/collapse. Uses Textual's
built-in `Tree`. Highlight is `$accent 50%` on `.tree--cursor`.

Keyboard navigation:

- `h` / `l` — focus sidebar / content pane (App-level)
- `j` / `k` — navigate tree nodes (App-level delegates to focused Tree)
- `Space` — expand/collapse a group node
- `Enter` — select a project (filter sessions)

`_rebuild()` re-adds every node, which would put the sidebar back at the top. It captures
`scroll_offset` first and restores it after the selected-project highlight (which moves the
cursor and can scroll with it), so a refresh — `r`, or a live session update, which arrives
on its own — leaves the user where they were scrolled to. Textual clamps the offset, so a
tree that shrank lands at its end.

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

## Jev efficiency analysis

The TUI drives the same analysis as the desktop and web clients over the same
HTTP routes (see [14-jev-integration.md](14-jev-integration.md)). Types and
parsers live in `tui-py/efficiency.py` — these come off the wire in camelCase,
unlike the snake_case session types in `data_types.py`.

```mermaid
sequenceDiagram
    participant U as User
    participant APP as CCTraceApp
    participant API as Backend
    participant JEV as Jev

    U ->> APP: a (on a session row)
    APP ->> API: GET /api/analytics/settings
    alt no Jev API key
        APP -->> U: warning toast, nothing prepared
    else key configured
        APP ->> API: POST /api/efficiency/prepare
        API -->> APP: payload (built and redacted locally)
        APP ->> U: EfficiencyPrivacyScreen (destination + exact payload)
        U ->> APP: tick the confirmation, Send to Jev
        APP ->> API: POST /api/efficiency/start
        API ->> JEV: analysis request
        API -->> APP: efficiency-analysis-update (SSE, repeatedly)
        APP ->> API: GET /api/efficiency/summaries (on completed)
    end
```

On start-up `_load_efficiency_state()` reads `/api/efficiency/jobs` and
`/api/efficiency/summaries`, so analyses run from the desktop app, a browser,
or an earlier TUI run show their score and any run still in flight.
`latest_jobs_by_session` keeps the newest attempt per session — the backend
keeps every attempt, and only the last one describes where a session stands.

### `EfficiencyPrivacyScreen` (`widgets/efficiency_privacy.py`)

The same gate as `EfficiencyPrivacyModal` on the other surfaces: where the data
goes, what may be shared, the exact `payload.input`, and a confirmation that is
required again for every analysis, retry, and re-analysis. Dismisses `True`
only when the box is ticked and **Send to Jev** is pressed; `Escape` and
**Cancel** dismiss `False` and nothing is sent.

The payload preview is rendered with `markup=False`, and the destination lines
as a Rich `Text`. Session content is full of brackets (`[src]`, JSON, shell
flags) and Textual would otherwise parse them as markup tags and raise
`MarkupError` while rendering.

### `AnalyticsSettingsScreen` (`widgets/analytics_settings.py`)

Jev key status, a **Test Jev connection** button, the default payload mode, and
the recommendation provider (with base URL and model for an OpenAI-compatible
endpoint). Built from settings the caller already fetched, so every control
shows its real value on first paint.

API keys cannot be entered here: the TUI reaches the backend over HTTP, which
never accepts them — the same limit web mode has. `apiKeyConfigured` is carried
back to the backend untouched so saving the form cannot clear a key the TUI has
no way to re-enter.

---

## SSE integration (`tui-py/sse.py`)

`SSEClient` is a thin httpx-based client running on a background thread.
It maintains a per-event handler dict and dispatches each `event:` /
`data:` pair to the registered async handler via `app.call_from_thread`.

Subscribed events:

- `picker-refresh` — re-runs `api.discover_sessions(dirs)`.
- `index-progress` — how much of the projects directory has been read; drives
  `IndexProgressBar`.
- `efficiency-analysis-update` — one Jev job's progress, from any client. Kept
  against its session path; a `completed` status also re-reads
  `/api/efficiency/summaries`, because the score lands in the summary, not in
  the finished job.
- `session-update` — a lightweight signal (count + roles, no bodies, see
  [04-http-api.md](04-http-api.md)). Calls `_on_session_update(_payload)` on
  the App, which ignores the payload and re-fetches the session via
  `load_session`, then updates `messages` / `teams` / `ongoing` / `meta` /
  `totals` from the fresh result and calls `_sync_all_widgets()`. If the
  re-fetch raises (backend momentarily unreachable), the previous
  `self.messages` is left untouched rather than cleared.

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

| File                                | Covers                                                                                                                                                                                                                                                    |
| ----------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `test_highlight_list.py`            | `ensure_highlight` policy, disabled-row skipping, idempotence, shared highlight color resolves to `$block-cursor-blurred-background`.                                                                                                                     |
| `test_message_list.py`              | Async populate race-safety (no duplicated rows after empty→real), full-rebuild sets index=0, incremental diff preserves the user's cursor, tail append, empty-state placeholder.                                                                          |
| `test_detail_view.py`               | Bordered container, RESPONSE/STEP headings render with live counts and hide when there is no message / no items; "Input"/"Result" label is unboxed while the JSON content is.                                                                             |
| `test_message_from_dict.py`         | `message_from_dict` (the `POST /api/session/message` response parser) parses top-level fields, full tool_input/tool_result bodies on items, and nested subagent messages.                                                                                 |
| `test_load_message.py`              | `api.load_message` posts `{path, index}` to `/api/session/message` and returns `None` for an out-of-range index.                                                                                                                                          |
| `test_session_update.py`            | `_on_session_update` re-fetches via `load_session` on the lightweight signal, is a no-op with no session open, and keeps prior messages when the re-fetch fails.                                                                                          |
| `test_format_progress.py`           | `format_bytes` units and the fixed-width, clamped block bar.                                                                                                                                                                                              |
| `test_index_progress.py`            | Scan line: blank before a scan and once done, "Counting sessions…" before totals are known, percentage measured in bytes.                                                                                                                                 |
| `test_efficiency_types.py`          | camelCase job/summary/settings parsing, the two provider payload shapes, newest-job-per-session, score bands.                                                                                                                                             |
| `test_session_picker_analysis.py`   | The score / running / failed analysis line, and that `update_analysis` refreshes rows without rebuilding the list or moving the cursor.                                                                                                                   |
| `test_efficiency_privacy.py`        | Send stays disabled until the notice is confirmed, cancel and Escape dismiss `False`, and a bracket-filled payload still renders.                                                                                                                         |
| `test_analytics_settings_screen.py` | Controls open on the loaded values, base URL only for the OpenAI-compatible provider, a stored provider key survives a save, backend refusals are reported verbatim.                                                                                      |
| `test_app_efficiency.py`            | `index-progress` drives the bar, job updates land against their session, nothing is prepared without a key or sent without confirmation, an open modal disarms the app's own keys but keeps `Tab` working, and app-bound letters type into a modal input. |
| `test_project_tree_widget.py`       | A rebuild keeps the sidebar's scroll offset, clamps it when the tree shrinks, and still marks the selected project.                                                                                                                                       |

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
- [08-session-lifecycle.md](08-session-lifecycle.md) — light/full message split, session-update signal
- [12-cli-launcher.md](12-cli-launcher.md) — `--tui` backend spawn/kill lifecycle
- [13-item-rendering.md](13-item-rendering.md) — per-type item rendering
