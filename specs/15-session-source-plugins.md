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
