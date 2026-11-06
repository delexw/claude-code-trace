# Session-Source Plugins

Every place sessions come from — Claude Code today, other agents later — is a **source plugin**.
A plugin lives in its own folder named after it, on every surface, and registers what it adds.
The shared shell (picker, toolbar, keybind bar, info bar, settings, sidebar) holds no code for any
one source. It renders whatever the enabled plugins provide, so enabling or disabling a plugin
changes the UI with no `if source === "x"` branch anywhere outside that plugin's folder.

## Layout

| Surface       | Registry                       | One folder per plugin         |
| ------------- | ------------------------------ | ----------------------------- |
| Rust backend  | `src-tauri/src/plugins/mod.rs` | `src-tauri/src/plugins/<id>/` |
| Web / desktop | `src/plugins/registry.ts`      | `src/plugins/<id>/`           |
| Terminal UI   | `tui-py/plugins/__init__.py`   | `tui-py/plugins/<id>/`        |

`<id>` is the same lowercase identifier on all three surfaces (`claude`, `antigravity`, ...).

## Backend contract (Rust)

```rust
pub struct PluginDescriptor {
    pub id: &'static str,
    pub label: &'static str,
    /// false for a plugin the app cannot run without (Claude).
    pub can_disable: bool,
    pub enabled_by_default: bool,
}

pub trait SourcePlugin: Sync {
    fn descriptor(&self) -> PluginDescriptor;
    /// The plugin's HTTP routes; merged into the API router behind the same auth layer.
    fn http_routes(&self) -> Router<Arc<HttpState>>;
}

pub static PLUGINS: &[&dyn SourcePlugin] = &[&claude::ClaudePlugin /*, ... */];
```

- `plugins::http_routes()` merges every plugin's routes; `http_api::build_router` calls it once.
- Tauri commands: Tauri accepts exactly one `invoke_handler`, and `generate_handler!` cannot be
  composed across modules, so `lib.rs` lists each plugin's commands in one block per plugin. The
  command bodies live in `plugins/<id>/commands/`.
- Permissions: a plugin with its own commands ships `src-tauri/permissions/<id>.toml`, and its set
  is added to the default set.
- `GET /api/settings` (and `get_settings`) includes
  `plugins: [{ id, label, enabled, can_disable }]`, in registry order.
- Plugin configuration lives in `settings.json` under `plugins.<id>` (`enabled` plus the plugin's
  own keys). A plugin with `can_disable: false` is always enabled.
- Engines shared by several features (`parser/`, `indexer`, `efficiency/`, `watcher`, `wsl`) stay
  in the core for now; the Claude plugin owns the session-source surface over them: listing,
  loading, watching, git info, debug log, terminal focus, and their HTTP routes.

## Frontend contract (web / desktop)

The contract is `src/plugins/types.ts`; in short:

```ts
interface SourcePlugin {
  id: string;
  label: string;
  canDisable: boolean;
  /** Runs on every render for every registered plugin (the registry is a module constant). */
  useSource(ctx: SourceContext): SourceRuntime;
  /** Rendered in Settings → General while the plugin is enabled; registers save/reset. */
  SettingsPanel?: ComponentType<SettingsPanelProps>;
}

interface SourceContext {
  enabled: boolean;
  activeView: string | null; // this plugin's view on screen, else null
  openView(view: string): void;
  goToPicker(): void;
  setPickerIndex(index: number): void; // shared selection across every section
  searchQuery: string;
  viewActionsRef: ViewActionsRef;
  fontScale: number;
  recapPreview: boolean;
  settings: SettingsResponse | null;
  openSettings(tab?: SettingsTab): void;
}

interface SourceRuntime {
  picker: PickerSection; // { count, open(localIndex), render({ selectedIndex, onHover }), headerStats? }
  sidebar?: SourceSidebar; // { render(style), focused, focus(), blur(), keyMap }
  status?: ReactNode; // left of the keybind bar
  hasOpenSession: boolean;
  backToSession(): void;
  views: Record<string, SourceView>; // { render(), infoBar?, toolbarLeft, collapsible, keyMap, hints, hintActions? }
  overlays?: ReactNode;
  onSettingsLoaded?(settings: SettingsResponse | null): void | Promise<void>;
  onSettingsSaved?(): void;
}
```

- The shell owns: the active location (`picker` or `{ plugin, view }`), the picker frame
  (title, search box focus, one section per enabled plugin), the toolbar's right cluster
  (Expand/Collapse/Top/Bottom, Open in Browser, Settings), the keybind bar frame, Settings tabs,
  the auth banner, and global keys (`?`).
- Picker selection is one index across all sections; the shell maps it to
  `(plugin, localIndex)` from each section's `count`, so keyboard selection can never point at
  an item a section did not render.
- Enabled plugins come from `get_settings().plugins`; until that answers, only plugins that
  cannot be disabled are shown.
- Web-mode routes (command name → HTTP endpoint) for a plugin's commands live in
  `src/plugins/<id>/routes.ts` and are listed in `src/plugins/routes.ts`, which `lib/invoke`
  merges with the app's own routes. It is a separate list from the registry so `lib/invoke` does
  not import the plugins' React code; a test checks every entry belongs to a registered plugin.

## Terminal UI contract (tui-py)

`tui-py/plugins/base.py` defines the contract; `tui-py/plugins/__init__.py` holds
`REGISTRY: tuple[type[SourcePlugin], ...]` (picker order).

```python
class SourcePlugin(ABC):
    id: ClassVar[str]; label: ClassVar[str]
    can_disable: ClassVar[bool] = True
    KEYS: ClassVar[Sequence[PluginKey]] = ()        # PluginKey(key, action, description, show)
    def __init__(self, host: PluginHost) -> None
    def start(self) -> None                          # enabled: start own workers
    async def stop(self) -> None
    def refresh(self) -> None                        # `r`
    def sse_handlers(self) -> Mapping[str, Callable[[object], object]]
    @abstractmethod def picker_section(self) -> PickerSection   # PickerSection(rows, loading)
    @abstractmethod def open(self, item: object) -> None        # load, then host.present_session(...)
    async def load_full_message(self, path: str, index: int) -> DisplayMessage | None
    def compose_sidebar(self) / compose_views(self) / compose_status(self) -> Iterable[Widget]
    def sync_sidebar(self) / sync_view(self, view) / focus_view(self, view) -> None
    def check_action(self, action: str, view: str) -> bool | None
    # key actions: action_<name>(self)

PickerRow(render: Callable[[int], RenderableType], item=None, header=False, animated=False)
OpenedSession(path, messages, ongoing, meta, totals)
```

- The shell (`app.CCTraceApp`) never branches on a plugin id. It composes every plugin's sidebar,
  views and status widgets (hidden while the plugin is off), fills the shared `SourcePicker` from
  each enabled plugin's `picker_section()`, routes Enter on a row to the owning plugin's `open`,
  and binds each `PluginKey` as `plugin('<id>', '<action>')`, dispatched by `action_plugin` and
  gated by the plugin's `check_action`. SSE handlers run only while their plugin is enabled.
- `PluginHost` (in `base.py`) is everything a plugin may call on the shell: `present_session`,
  `apply_session_update`, `sync_picker`, `refresh_picker_rows`, `set_picker_busy`,
  `highlighted_picker_item`, `sync_view`, `focus_content`, plus Textual's
  `notify`/`run_worker`/`push_screen(_wait)`/`copy_to_clipboard`.
- Claude's session loading, picker rows, project-tree sidebar, index progress bar, team/debug
  views, efficiency actions and SSE handlers live in `tui-py/plugins/claude/`.

## Which plugins are on

The same rule on every surface: a plugin that cannot be disabled is always on; any other plugin is
on only when `get_settings().plugins` reports it `enabled: true`. A backend that does not report
`plugins` therefore never has an optional plugin's routes called.

## Turning plugins on and off

- `settings.json` stores `plugins.<id>.enabled` (plus the plugin's own keys under `plugins.<id>`).
  A plugin with no stored value uses its descriptor's `enabled_by_default`.
- `set_plugin_enabled { id, enabled }` (HTTP `POST /api/plugins/enabled`) saves the flag and
  returns the full settings response. Unknown ids, and turning off a plugin with
  `can_disable: false`, are errors.
- Settings → General lists every plugin that can be disabled with an on/off switch. Turning one
  on or off swaps in the returned settings, so its picker section, views and settings panel appear
  or disappear without a reload; a plugin that just came on gets `onSettingsLoaded` to start up.
  The terminal UI reads the flags at start-up.

## Source filter (web)

When two or more plugins are on, the picker toolbar shows an All / one-per-plugin selector
(`components/SourceFilter.tsx`). Picking a plugin shows only its picker section and header totals;
the choice is kept in `localStorage` (`cct.sourceFilter`, per device like other UI-only prefs). A
remembered plugin that is no longer on falls back to All. Changing the filter resets the picker
selection to the first item.

## Plugin: `antigravity`

Read-only viewer for Google Antigravity (Gemini IDE) "brain" conversations,
`<root>/<uuid>/.system_generated/logs/transcript.jsonl`. Off by default; can be disabled.

Brain roots, in order: the user's override (`plugins.antigravity.root`), `$GEMINI_HOME/<flavor>/brain`,
then `~/.gemini/<flavor>/brain` for each flavor in `["antigravity-ide", "antigravity"]`. Folders
from every existing root are merged and de-duplicated by `uuid`.

| Command (Tauri)                                     | HTTP                            | Returns                                                                        |
| --------------------------------------------------- | ------------------------------- | ------------------------------------------------------------------------------ |
| `list_antigravity_folders`                          | `GET /api/antigravity/folders`  | `AntigravityBrainFolder[]`, newest first; `[]` while disabled                  |
| `get_antigravity_settings`                          | `GET /api/antigravity/settings` | `{ root: string \| null, roots: BrainRoot[] }`                                 |
| `set_antigravity_root { path: string \| null }`     | `POST /api/antigravity/root`    | same as `get_antigravity_settings`; a path that is not a directory is an error |
| `load_antigravity_session { path, start?, limit? }` | `POST /api/antigravity/session` | `AntigravityWindow` (`limit` omitted = whole conversation, `0` = index only)   |

```ts
interface BrainRoot {
  flavor: string;
  path: string;
  exists: boolean;
} // flavor "custom" for the override
interface AntigravityBrainFolder {
  uuid: string;
  path: string;
  mod_time_ms: number;
  title: string | null;
  flavor: string;
  has_transcript: boolean;
}
interface AntigravityToolCall {
  name: string;
  summary: string;
  input: string;
}
interface AntigravityMessage {
  step_index: number;
  role:
    | "user"
    | "assistant"
    | "tool"
    | "file"
    | "search"
    | "error"
    | "checkpoint"
    | "context"
    | "system";
  content: string;
  created_at: string | null;
  tool_calls: AntigravityToolCall[];
  thinking_text: string | null;
  error_text: string | null;
  duration_ms: number;
}
interface AntigravityWindow {
  messages: AntigravityMessage[];
  count: number;
  start: number;
  roles: string[];
  total_rows: number;
}
```

- Only folders with `has_transcript` are listed in the picker; the rest are artifact folders.
- Each step keeps its own label and colour (`MCP Tool`, `View File`, `Grep Search`, ...) in the
  message list and detail view: web passes a `RoleStyle` through `MessageList.rowRoleStyle` and
  `MessageDetail.roleLabel`; the TUI sets `DisplayMessage.role_label`. Neither surface shows
  Claude-only hints (token counts, the debug-log hint) for these steps.
- Steps that record nothing (no content, thought, tool call or error) are left out of `count`.
- Each transcript gets a byte-offset index cached in the app's config dir, so opening a large
  conversation does not re-parse it whole. Nothing is ever written under a brain root.

## Plugin: `codex`

Read-only Codex rollout viewer, off by default. It scans `sessions/` and
`archived_sessions/` below `$CODEX_HOME` or `~/.codex`. On Windows it also discovers installed
WSL distributions and their Codex homes. A custom `plugins.codex.root` replaces auto-detection
and points to a Codex home, rather than directly to its sessions folder.

The picker excludes rollouts whose first session metadata explicitly identifies a subagent
source. Later inherited metadata does not change that classification. User-created forks and
independent chats remain separate even when they share a title. Saved chat names take priority;
client page-context blocks are skipped when selecting either a saved name or a prompt-derived
fallback. A real prompt following a known page-context block is preserved.

| Command                                                 | HTTP                                 | Returns                                                     |
| ------------------------------------------------------- | ------------------------------------ | ----------------------------------------------------------- |
| `list_codex_sessions`                                   | `GET /api/codex/sessions`            | `CodexSession[]`, newest first; empty while disabled        |
| `get_codex_settings`                                    | `GET /api/codex/settings`            | `{root: string \| null, roots: [{path, source, exists}]}`   |
| `set_codex_root {path}`                                 | `POST /api/codex/root`               | Updated Codex settings; null clears the override            |
| `load_codex_session {path, start?, limit?}`             | `POST /api/codex/session`            | `CodexWindow`; limit 0 fetches metadata only                |
| `prepare_codex_efficiency_payload {path, payloadMode?}` | `POST /api/codex/efficiency/prepare` | Locally normalized and redacted `PreparedEfficiencyPayload` |

`CodexSession` contains `id`, `path`, `title`, `cwd`, `model`, `modified_at` (ISO timestamp),
`archived`, `source`, `total_tokens`, and nullable `cost_usd`. `CodexWindow` contains shared
`DisplayMessage` bodies plus `count`,
`start`, `roles`, `ongoing`, `revision`, and `input_tokens`, `output_tokens`, `cache_read_tokens`,
`total_tokens`, and nullable `cost_usd`. The shared assistant role remains `claude` for renderer
compatibility; each
Codex frontend labels it **Codex**. No Claude-only actions are attached to these sessions.

The original `cwd` and recorded `dirs` drive the project tree, independently of Claude's
encoded transcript paths. Windows paths compare without case; POSIX paths retain case and WSL
distributions retain separate namespaces. Forks group with their known root. Source-owned
precomputed tree items share the tree renderer; the shell stacks visible source sidebars in
one resizable column and follows the picker source filter or open conversation. Project
filters combine with search, and date headings preserve keyboard/card order.

Additional session/window metadata includes recorded model, Git branch, approval and sandbox
policy, start/activity timestamps, active turn duration (excluding idle gaps), turn count,
context usage and context capacity. Missing timing/context remain null. The context gauge
uses the latest request's total context usage, including cached input, rather than cumulative session usage.
The session's original directory remains stable after later working-directory changes.

Resume/Fork actions copy quoted Codex CLI commands; they do not execute the CLI. Commands
preserve the transcript's Codex home, convert WSL-mounted Windows paths to native paths and
name the target shell. Shared efficiency analysis accepts normalized Codex display messages,
uses the existing redaction/preview/explicit-consent flow and shares jobs, cached reports and
finding navigation. Merely browsing a chat does not send transcript data to Jev.

Worker expansion requires a spawn result's `agent_id`, matching first child parent metadata,
and an unambiguous assigned prompt boundary in the same Codex home. It excludes inherited
history, follows at most four levels and counts each descendant's own usage once. Dependency
revisions refresh open nested panels and picker totals when children change. Unresolved or
unpriced workers preserve any known subtotal with `cost_partial: true`, rendered with `+`;
fully unknown costs remain null. Ordinary tool/results remain visible when a link cannot be verified.

The parser reconciles response items and duplicate event messages, matches tool results by call
ID, and omits encrypted reasoning. The web/desktop source polls the active conversation using
its revision and loads bounded message windows. The terminal polls a metadata-only index, reloading
messages on a changed revision; `r` also refreshes manually. Test transcripts
and expected messages are fabricated; personal Windows and WSL chats may only be used for
read-only local validation.

Costs are estimated API-equivalent model-token USD, using bundled [official OpenAI rates](https://developers.openai.com/api/docs/pricing)
checked on 2026-11-07. They appear on picker cards, in filtered picker totals, and in the open
conversation bar, including live updates. Subscription charges and tool fees can differ.
Cached input and cache writes are separated from uncached input; reasoning is already part of
output usage. Repeated modern response IDs and legacy usage twins must not be billed twice,
and legacy-only usage before modern records must remain included. Per-request model and
recorded context size determine pricing; aggregate-only historical usage assumes standard
rates when request sizes or speed tiers are unavailable. Incomplete usage or unknown pricing
returns `null`, distinct from a known zero. An aggregate with unknown chats displays its known
subtotal with `+`, rather than presenting that subtotal as a complete cost.

## Codex parity audit

| Claude session feature                                                             | Codex support                                                                                    |
| ---------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------ |
| Project/folder tree, nesting, collapse and project filters                         | Web, desktop and terminal; original working folder and known fork ancestry                       |
| Search and date groups                                                             | All viewers; displayed order matches keyboard selection                                          |
| Messages, reasoning, tools/results, errors and compaction                          | Shared renderers; encrypted reasoning is omitted                                                 |
| Token totals and estimated costs                                                   | All viewers; linked worker usage counted once; incomplete subtotals marked `+`                   |
| Model, context gauge, branch, permission and timing metadata                       | Recorded when available; web/desktop also query current Git state                                |
| Live updates                                                                       | Revision-aware polling, including worker dependencies                                            |
| Resume and Fork                                                                    | Copy commands in web/desktop; Resume in terminal, matching its existing Claude action            |
| Nested subagent browsing                                                           | Verified spawn links in shared nested-message panels                                             |
| Efficiency analysis                                                                | All viewers; shared redaction, exact preview, explicit per-send consent, jobs and cached results |
| Claude hook/debug logs, team/task board, terminal registry focus and recap records | Remain source-specific; Codex rollouts do not supply those Claude records                        |

## Adding a plugin

1. Rust: `src-tauri/src/plugins/<id>/` implementing `SourcePlugin`; add it to `PLUGINS`; list its
   commands in `lib.rs`; add `permissions/<id>.toml`.
2. Web: `src/plugins/<id>/` exporting a `SourcePlugin`; add it to `src/plugins/registry.ts`, and
   its `routes.ts` to `src/plugins/routes.ts`. Styles go in `src/plugins/<id>/<id>.css`, imported
   by the plugin.
3. TUI: `tui-py/plugins/<id>/`; add it to the registry in `tui-py/plugins/__init__.py`.
4. No other file changes. A shared file that needs a new `if id === ...` means the contract is
   missing an extension point — extend the contract instead.

## Open concerns

- `settings.json` still keeps Claude's `projects_dir` and `wsl_distros` at the top level. Moving
  them under `plugins.claude` needs a migration and is deferred.
- The shared engines (`parser/`, `indexer`, `efficiency/`) are Claude-specific in practice; moving
  them under `plugins/claude/` is a later structural step.
