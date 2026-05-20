# Changelog

All notable changes to claude-code-trace are documented here. Versions follow
[semantic versioning](https://semver.org/).

## [0.6.0] — 2026-05-21

A release that rewrites the terminal UI from scratch in Python/Textual, fixes the JSONL
parser against two real-world Claude Code data shapes that used to silently drop content,
and finally lands a viewport-driven picker that updates the cards you're actually looking
at instead of polling the whole list. Docker headless mode also gets a long-overdue CPU
cut after the WebKit and SSE-flooding root causes were untangled.

### Added

- **Python/Textual TUI replaces the Ink/React TUI**
  ([`b21753a`](https://github.com/delexw/claude-code-trace/commit/b21753a)). The terminal UI
  now lives under `tui-py/` and runs via `python3 tui-py/main.py` (also wired into
  `cctrace --tui` and `npm run dev:tui`). A shared `HighlightListView` base owns selection
  styling and cursor-init for the picker, message list, and detail-view items list — fixing
  the inconsistencies the old Ink implementation accumulated. Rapid SSE updates and
  load-session resets are now serialised through an exclusive worker group so the message
  list no longer races with its own deferred `clear()` calls and duplicates rows. The
  picker stays visible with a `LoadingIndicator` overlay until the message list is fully
  populated, eliminating the few-second window where j/k against an empty list pane felt
  broken.

- **Viewport-driven picker refresh in the web frontend**
  ([`276f438`](https://github.com/delexw/claude-code-trace/commit/276f438)). The picker
  wraps every session card in a shared `IntersectionObserver` (`useVisibleSessions` hook)
  and re-fetches the session list whenever the visible set changes — debounced 150 ms —
  plus a 2 s heartbeat while any cards are on screen. Cards the user is actually looking at
  stay fresh without paying the cost of polling everything, and ongoing-status / token-count
  badges update much closer to real time.

- **`effort` and `terminalSequence` fields for Claude Code v2.1.133+ / v2.1.141+**
  ([`512f9e1`](https://github.com/delexw/claude-code-trace/commit/512f9e1)). Claude Code
  v2.1.133 injects an `effort: {level}` payload into hook input JSON and v2.1.141 adds
  `terminalSequence` to hook output entries. Both are now captured on the `Entry` struct
  instead of being silently dropped by serde's default unknown-field behaviour, so future
  inspectors can read them.

- **Desktop and macOS Tauri permission schemas**
  ([`0af3f9a`](https://github.com/delexw/claude-code-trace/commit/0af3f9a)). Adds
  `(allow|deny)-supports-multiple-windows`, `(allow|deny)-set-icon-with-as-template`, and
  per-window `activity-name` / `scene-identifier` permissions so capability files can opt
  into the newer Tauri 2.11 surface explicitly.

### Fixed

- **JSONL parser: deduplicate summary entries from pre-v2.1.128 sessions**
  ([`aa22a61`](https://github.com/delexw/claude-code-trace/commit/aa22a61)). Pre-v2.1.128
  Claude Code wrote duplicate `summary`-type JSONL entries when sub-agents were idle,
  causing the same summary state to be re-emitted on every tick. That surfaced as duplicate
  CompactMsg blocks in the UI, inflated token counts, and potential conversation-tree
  corruption. `read_session_incremental` now tracks a `HashSet` of
  `(agentName, teamName, summary_text)` triples and keeps only the first occurrence.

- **JSONL parser: sanitize lone UTF-16 surrogates**
  ([`833e266`](https://github.com/delexw/claude-code-trace/commit/833e266)). Sessions
  written by Claude Code before v2.1.132 occasionally split a multi-byte emoji across the
  tool-error truncation boundary, leaving a lone `\uD83D` without its matching low
  surrogate. `serde_json` rejects those per RFC 8259 and silently discarded the offending
  lines. A new `sanitize_lone_surrogates()` step replaces unpaired surrogates with `�`
  before the deserializer sees them, while valid surrogate pairs pass through untouched.
  Allocation is deferred to `Cow::Borrowed` for the common no-surrogate case.

- **Headless Docker CPU and SSE traffic**
  ([`fea185e`](https://github.com/delexw/claude-code-trace/commit/fea185e)). Two unrelated
  costs were stacking: `--headless` still booted `tauri::Builder` (spawning WebKit's web
  and network processes plus Xvfb under docker-entrypoint) and the picker watcher fanned
  the full session list to every SSE client on every inotify event. With 2000+ session
  files this produced continuous high CPU and megabytes-per-second of SSE traffic.
  `--headless` now returns early with just a tokio runtime + axum server, Xvfb is skipped,
  and the watcher emits a lightweight `{}` signal that clients turn into a single cached
  `list_sessions` re-fetch (debounce raised from 300 ms to 1 s).

- **Async handlers in `useTauriEvent` and `useSSE`**
  ([`f7d3c8d`](https://github.com/delexw/claude-code-trace/commit/f7d3c8d)). The two
  subscription hooks now accept `async` listeners, so callers can `await` inside an event
  handler without React swallowing the returned Promise. Paired with
  ([`bc173bd`](https://github.com/delexw/claude-code-trace/commit/bc173bd)) and
  ([`b76c8d5`](https://github.com/delexw/claude-code-trace/commit/b76c8d5)), every unhandled
  async inner call across the codebase is now either properly awaited, `void`-discarded,
  or `.catch()`-attached — eliminating the silent unhandled-promise-rejection class.

- **Picker auto-detect for new project folders**
  ([`270e717`](https://github.com/delexw/claude-code-trace/commit/270e717),
  [`2007752`](https://github.com/delexw/claude-code-trace/commit/2007752)). On a
  picker-refresh signal the frontend now also re-derives the project-directory set, so a
  newly-created `~/.claude/projects/<slug>/` directory shows up without restarting the app.

- **TUI picker auto-refresh**
  ([`ebb2ca5`](https://github.com/delexw/claude-code-trace/commit/ebb2ca5)). The terminal
  UI subscribed to a non-existent `picker-update` event while the backend emits
  `picker-refresh` with an empty payload. The TUI picker never auto-updated when sessions
  changed on disk — it only refreshed when the user re-entered the picker view. The TUI
  now subscribes to `picker-refresh` and re-fetches via `api.discoverSessions(dirs)`,
  mirroring the web frontend pattern in `src/hooks/usePicker.ts`.

- **Web picker re-fetch on `picker-refresh` signal**
  ([`01f8212`](https://github.com/delexw/claude-code-trace/commit/01f8212)). Memoise the
  most recent `projectDirs` in a ref so the SSE handler can re-issue `discover_sessions`
  without the caller threading state through.

[0.6.0]: https://github.com/delexw/claude-code-trace/releases/tag/v0.6.0
