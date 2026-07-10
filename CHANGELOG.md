# Changelog

All notable changes to claude-code-trace are documented here. Versions follow
[semantic versioning](https://semver.org/).

## [0.11.0] — 2026-07-10

This release turns the viewer into a launchpad: you can resume, fork, or jump to the
terminal window of the session you're reading, and the picker can preview each session
by its end-of-session recap. It also fixes toolbar actions that silently did nothing,
keeps the permission-mode badge correct for Claude Code v2.1.200+, and corrects cost
estimates for sessions running Sonnet 5.

### Added

- **Resume, fork, or focus the session you're viewing**
  ([`5fe375c`](https://github.com/delexw/claude-code-trace/commit/5fe375c), @michaelstingl).
  A new session-control panel in the session view: Resume and Fork copy a ready-to-paste
  `cd '<cwd>' && claude --resume <id>` command (Fork adds `--fork-session`), and Focus —
  on a local macOS backend — brings the session's terminal window to the front. Sessions
  also get a live busy/idle badge read from Claude Code's `~/.claude/sessions` registry,
  replacing the transcript-derived "active" indicator. The TUI mirrors the liveness badge
  and a copy-resume key binding.

- **End-of-session recap as the picker preview**
  ([`0904952`](https://github.com/delexw/claude-code-trace/commit/0904952), @michaelstingl).
  When a session ends with a recap, the picker now shows that recap as the session's
  preview instead of the first message, so you can tell what a session accomplished at a
  glance. A SESSION PREVIEW toggle in Settings turns it off if you prefer the old
  first-message preview (default on).

- **Toolbar actions scoped to the view they act on**
  ([`b4483a7`](https://github.com/delexw/claude-code-trace/commit/b4483a7), @michaelstingl).
  Top and Bottom now actually scroll in every view — they previously targeted a
  non-scrolling container and did nothing. Expand All / Collapse All no longer appear in
  the picker, where nothing is collapsible, and the content actions moved to the right
  cluster, above the column they operate on.

### Fixed

- **Sonnet 5 sessions no longer overestimate cost**
  ([`7396d03`](https://github.com/delexw/claude-code-trace/commit/7396d03)). Sonnet 5
  ships with promotional pricing ($2/Mtok input, $10/Mtok output through Aug 31, 2026),
  but the cost calculator was applying the standard $3/$15 Sonnet rate — inflating every
  `claude-sonnet-5` session's cost by roughly a third. A dedicated pricing tier now
  matches any `sonnet-5` model ID before the generic Sonnet fallback.

- **Spurious permission-mode badge on Claude Code v2.1.200+ sessions**
  ([`17344f0`](https://github.com/delexw/claude-code-trace/commit/17344f0)). Claude Code
  v2.1.200 renamed the default permission mode written to session files from "default"
  to "manual", which made every normal session show a permission-mode badge in the info
  bar with a raw-string label. Both values are now recognised as the badge-free default.

[0.11.0]: https://github.com/delexw/claude-code-trace/releases/tag/v0.11.0

## [0.10.0] — 2026-07-01

This release keeps the JSONL parser current with another run of Claude Code releases
(v2.1.186 through v2.1.195 — `/rewind`, background-subagent permission attribution, and
background-agent format changes), fixes a live-chain cycle that could drop a session's
entire history, and follows up 0.9.0's performance work with virtualized message lists,
lower memory usage on large sessions, and a matching detail-view fix in the TUI.

### Added

- **Session picker titles from `/rename`**
  ([`2c63774`](https://github.com/delexw/claude-code-trace/commit/2c63774), @michaelstingl).
  Sessions renamed with Claude Code's `/rename` command now show that name as the title in
  the picker, joined from the live `~/.claude/sessions` registry by session ID. Falls back
  to the first message when a session hasn't been renamed.

### Fixed

- **`/rewind` support (Claude Code v2.1.191)**
  ([`04b813d`](https://github.com/delexw/claude-code-trace/commit/04b813d)). Resuming a
  conversation from before a `/clear` via `/rewind` produces `rewind-pointer` entries and
  `rewindable`/`checkpointUuid` markers on compaction checkpoints; both are now parsed and
  the structural markers are filtered from the display so a rewound session renders
  cleanly instead of showing raw pointer entries.

- **Background-agent JSONL format changes (Claude Code v2.1.195)**
  ([`1d0a1f4`](https://github.com/delexw/claude-code-trace/commit/1d0a1f4)). SDK and
  background-agent sessions gained new top-level fields (schema version, entrypoint,
  agent ID, and a `last-prompt` checkpoint type). These are now parsed with safe
  defaults for older sessions, and `last-prompt` checkpoints are discarded instead of
  falling through the classifier without a role.

- **Background subagent permission-prompt attribution (Claude Code v2.1.186)**
  ([`acdbf75`](https://github.com/delexw/claude-code-trace/commit/acdbf75)). Background
  subagents now surface permission prompts directly in the main session instead of
  auto-denying, and each prompt carries which subagent requested it. The viewer parses
  the new attribution fields and shows a "Requesting Agent" section in the detail view
  when present.

- **Auto-mode denials no longer vanish from the transcript (Claude Code v2.1.193)**
  ([`552bb59`](https://github.com/delexw/claude-code-trace/commit/552bb59)). Tool calls
  denied automatically in auto/plan mode were silently dropped by the noise filter
  instead of being shown. Denial entries — both top-level and the kind embedded inside
  progress entries — are now rescued and rendered as a system notice naming the tool and
  the reason it was denied.

- **Virtualized message list and lower memory use on large sessions**
  ([`800fd01`](https://github.com/delexw/claude-code-trace/commit/800fd01)). Sessions
  with heavy tool output (100MB+ transcripts) could grow the app's memory well beyond
  the file size because the message list held every message and its full tool
  input/output at once, and the desktop webview leaked render-tree memory across
  repeated session switches. The message list is now virtualized and fetched in pages
  that evict as you scroll, the detail view re-parses on demand instead of caching a
  heavy build, and the desktop app periodically reloads its webview to reclaim memory
  WebKit doesn't release on its own. Also fixes a stale row staying hover-highlighted
  after scrolling stops, caches the session-name registry read, and adds a font-size
  setting.

- **TUI detail view, live SSE updates, and a live-chain history bug**
  ([`1e92f8c`](https://github.com/delexw/claude-code-trace/commit/1e92f8c)). Following
  the virtualization work above: the TUI's detail view now fetches the full message
  on demand instead of showing empty tool bodies from the lightened list payload, and
  it re-fetches on the live-update signal instead of trusting a `messages` field the
  backend no longer sends (which was silently emptying the view). Fixes a Detail-view
  CSS flash and switches JSON/tool output to syntax-highlighted, word-wrapped text
  instead of horizontally-scrolling code blocks. Separately, a real session was found
  where an auto-compaction boundary pointed back into its own post-compaction
  descendants, closing a cycle that made the parser truncate the entire session history
  at that point — the resolver now falls back to the nearest earlier entry when this
  cycle is detected. Also fixes `cctrace --tui` orphaning its backend process group on
  exit, which could break a later `cctrace --web`.

[0.10.0]: https://github.com/delexw/claude-code-trace/releases/tag/v0.10.0

## [0.9.1] — 2026-06-27

A maintenance release that repairs two regressions from the 0.9.0 WSL and packaging
work: the WSL distro settings now actually save on the desktop build, and `install.sh`
no longer fails on setups that don't carry the legacy `tui/` directory.

### Fixed

- **WSL distro settings now save on the desktop build**
  ([`2917f29`](https://github.com/delexw/claude-code-trace/commit/2917f29), @RobotHanzo).
  The WSL discovery commands shipped in 0.9.0 were wired into the Tauri handler but never
  granted in the ACL permission set, so on the desktop build saving the Settings modal
  failed with "Command set_wsl_distros not allowed by ACL" and the distro listing was
  silently blocked. The commands are now ACL-granted, and a regression test cross-checks
  every registered command against the permission set so a missing grant fails CI instead
  of at runtime.

- **`install.sh` no longer breaks when the `tui/` directory is absent**
  ([`a6ec94e`](https://github.com/delexw/claude-code-trace/commit/a6ec94e), @DoTTak). The
  installer unconditionally `cd`'d into `tui/` to build the TUI, which aborted the install
  on checkouts that no longer carry that directory (the TUI moved to `tui-py/`). The build
  step is now guarded — it runs when `tui/` exists and prints a skip notice otherwise.

## [0.9.0] — 2026-06-26

This release widens where the viewer looks for sessions and sharpens how it shows what
Claude changed. Windows users running Claude Code inside WSL can now surface those
Linux-side sessions, Edit tool calls render as a proper colour-coded diff instead of raw
JSON, and the JSONL parser keeps pace with another run of Claude Code releases
(v2.1.178 through v2.1.183).

### Added

- **WSL session discovery**
  ([`6912a6d`](https://github.com/delexw/claude-code-trace/commit/6912a6d), @RobotHanzo).
  On Windows, Claude Code running inside a WSL distribution stores its projects under the
  Linux home, out of reach of the host viewer. Trace can now opt in to discovering those
  sessions: detected distributions appear as checkboxes in Settings, and the ones you
  enable are folded into the project list alongside your host projects. All WSL access is
  Windows-gated, so other platforms are unaffected.

- **Structured diff view for Edit tool calls**
  ([`6a6c2d9`](https://github.com/delexw/claude-code-trace/commit/6a6c2d9)). Edit tool
  inputs used to render as raw `old_string` / `new_string` JSON. They now display as a
  colour-coded diff — unchanged context lines are preserved, removed and added lines are
  marked with `-`/`+`, and only the words that actually changed are highlighted within a
  line. Both the web UI and the TUI get the new rendering.

### Fixed

- **Thinking-only and re-prompt entries from Claude Code v2.1.183**
  ([`2c69c85`](https://github.com/delexw/claude-code-trace/commit/2c69c85)). v2.1.183
  emits assistant entries that carry only thinking content, plus `isMeta`-flagged user
  entries for re-prompts. These previously tripped up the parser; they are now recognised
  and the transcript renders without gaps.

- **Partial assistant entries with missing usage and token fields**
  ([`d0581ee`](https://github.com/delexw/claude-code-trace/commit/d0581ee)). Some
  assistant entries arrive mid-stream with null `usage` and token fields. The parser now
  tolerates these instead of failing on them, so a live transcript no longer breaks while
  a partial entry is still being written.

- **Implicit team discovery for Claude Code v2.1.178**
  ([`945deb4`](https://github.com/delexw/claude-code-trace/commit/945deb4)). v2.1.178
  stopped writing the team header that discovery relied on, so sub-agent and team sessions
  silently disappeared. Discovery now uses `agentName` as the primary signal and finds
  those sessions again.

[0.9.1]: https://github.com/delexw/claude-code-trace/releases/tag/v0.9.1
[0.9.0]: https://github.com/delexw/claude-code-trace/releases/tag/v0.9.0

## [0.8.0] — 2026-06-16

This release keeps the JSONL parser in step with another run of Claude Code releases
(v2.1.157 through v2.1.174): deeper sub-agent nesting, sessions that change directory or
switch worktrees partway through, Workflow sub-agents that didn't always announce
themselves, and newer model IDs like Claude Fable 5. The project tree also stops
stranding worktree-only sessions at the top level.

### Added

- **Five-level sub-agent nesting**
  ([`beb3a61`](https://github.com/delexw/claude-code-trace/commit/beb3a61)). Claude Code
  v2.1.172 lets sub-agents spawn sub-agents up to five levels deep. The `Entry` struct now
  captures the optional `agentDepth` and `parentAgentName` fields those sidechain entries
  carry, so depth and attribution survive into the transcript instead of being dropped, and
  the main-session chain is still isolated correctly at every level.

### Fixed

- **Session picker froze on the starting directory after a worktree switch**
  ([`6aa548d`](https://github.com/delexw/claude-code-trace/commit/6aa548d)). Session
  metadata took the working directory and git branch from the first entry only, so after an
  `EnterWorktree` switch mid-session (v2.1.157+) the picker kept showing where the session
  started rather than where it ended up. Both fields now follow the last-seen value.

- **Mid-session `/cd` changes and the `Cd` tool weren't reflected**
  ([`3295fc6`](https://github.com/delexw/claude-code-trace/commit/3295fc6)). Directory
  changes from `/cd` (v2.1.169+) now update the InfoBar and session picker, and `Cd` tool
  calls render with a tool-category icon and a short directory marker in the transcript
  timeline instead of going uncategorised.

- **Workflow sub-agent sessions were silently skipped**
  ([`e2faaff`](https://github.com/delexw/claude-code-trace/commit/e2faaff)). Before
  v2.1.174, `agent()` sub-agents from the Workflow tool didn't write attribution headers on
  their first line, so team-session discovery returned empty and dropped them. Discovery now
  scans every line for attribution, making those historical sub-agent sessions visible when
  any entry carries it.

- **Model IDs with bracket suffixes or dates rendered awkwardly**
  ([`ae7c0ec`](https://github.com/delexw/claude-code-trace/commit/ae7c0ec)). Model labels
  now strip bracket context suffixes (e.g. `[1m]`/`[1M]` from pre-v2.1.173 sessions) and
  drop `YYYYMMDD` date components before shortening, so `claude-fable-5-20261001[1m]` shows
  as `fable5` while existing names like `haiku4.5` are unchanged.

- **Worktree-only sessions stranded at the top of the project tree**
  ([`d3aba67`](https://github.com/delexw/claude-code-trace/commit/d3aba67)). Sessions that
  only ever ran inside per-item worktrees (never at the repo root) rendered as flat
  top-level nodes. The tree now synthesizes a repo-root node for them so they nest under
  their repository in a `CLAUDE-WORKTREES` group; runs that already have a real anchor are
  unchanged.

## [0.7.0] — 2026-06-08

This release is mostly about keeping the JSONL parser in lockstep with a fast run of
Claude Code releases (v2.1.145 through v2.1.166) so newer transcripts stop silently
losing content or rendering blank turns. Alongside the compat work, the macOS desktop
install now produces a real `.app` bundle instead of a window that launches blank, and
turn detail reads in chronological order instead of a flattened wall of text.

### Added

- **`background_tasks` and `session_crons` hook fields**
  ([`da22c00`](https://github.com/delexw/claude-code-trace/commit/da22c00)). Stop and
  SubagentStop hook payloads in Claude Code v2.1.145+ carry two new arrays — running
  background-task descriptors and session-scoped cron jobs. Both are now captured on the
  `Entry` struct instead of being dropped, so they survive into the transcript view.

### Fixed

- **macOS install launches a blank white window**
  ([`2752850`](https://github.com/delexw/claude-code-trace/commit/2752850)). `cargo install`
  produced a bare Mach-O binary with no `.app` wrapper or `Info.plist`, so the webview came
  up blank. The macOS installer now builds a proper `.app` bundle via `tauri build`,
  installs it to `/Applications`, and removes any stale cargo binary left on `PATH`.
  Linux and other platforms keep the existing cargo-install path.

- **Turn detail rendered out of order**
  ([`2752850`](https://github.com/delexw/claude-code-trace/commit/2752850)). The detail view
  flattened every assistant text block into one blob at the top, then repeated the same text
  as collapsed Output items below — so commentary and the final result read as a jumbled
  wall. Output prose now renders inline and always-visible, interleaved with tool calls in
  chronological order, with the duplicated top blob suppressed. Web and the Python TUI now
  match.

- **Blank AI turns from fallback-model retries**
  ([`dbb7f5b`](https://github.com/delexw/claude-code-trace/commit/dbb7f5b)). Claude Code
  v2.1.166's `fallbackModel` writes a partial assistant entry with null/empty content before
  the successful fallback response. These stubs surfaced as empty AI turns and confused
  chunk aggregation. The parser now drops assistant entries with null or empty content, while
  the real fallback response (and its actual model ID, e.g. `claude-haiku-4-5`) passes through
  unaffected.

- **Hook `additionalContext` was silently dropped**
  ([`e03684b`](https://github.com/delexw/claude-code-trace/commit/e03684b)). Stop /
  SubagentStop hook entries in v2.1.163 carry `hookSpecificOutput.additionalContext` at the
  top level, which serde discarded. It is now captured as hook metadata and shown in the
  transcript's hook panel.

- **Cache-write token counts showed 0 on v2.1.152+**
  ([`e7e3a88`](https://github.com/delexw/claude-code-trace/commit/e7e3a88)). The parser only
  read the flat `cache_creation_input_tokens` field; v2.1.152+ reports cache-write tokens
  nested under `cache_creation.input_tokens`. Both formats are now read (taking the larger of
  the two), so cache-write counts are correct for recent sessions and still backward-compatible.

- **v2.1.154 dynamic-workflow entries broke parsing**
  ([`2e37441`](https://github.com/delexw/claude-code-trace/commit/2e37441)). Dynamic workflows
  introduced new entry types (`workflow-start`/`-progress`/`-complete`/`-cancelled`/`-error`)
  and `workflow*` state fields. The five lifecycle types are now discarded as noise and the
  state fields captured, instead of hitting the role-based fallback or tripping
  `deny_unknown_fields`.

- **`.meta.json` parsing fragile to schema changes**
  ([`3fc65ba`](https://github.com/delexw/claude-code-trace/commit/3fc65ba)). Sidecar metadata
  is now parsed through a typed `SidecarMeta` struct where every field is optional or
  `serde(default)`, so sessions written before (or after) a field was added keep parsing
  instead of failing.

- **Tool results not pretty-printed**
  ([`b7ea662`](https://github.com/delexw/claude-code-trace/commit/b7ea662)). JSON tool results
  now render as formatted code blocks — using the backend-parsed `tool_result_json` when
  available, otherwise attempting a JSON parse and falling back to plain text — matching how
  tool inputs already display, in both web/desktop and the Python TUI.

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

[0.8.0]: https://github.com/delexw/claude-code-trace/releases/tag/v0.8.0
[0.7.0]: https://github.com/delexw/claude-code-trace/releases/tag/v0.7.0
[0.6.0]: https://github.com/delexw/claude-code-trace/releases/tag/v0.6.0
