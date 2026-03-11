# tail-claude-gui

A desktop GUI for reading Claude Code session JSONL files. Built with [Tauri v2](https://v2.tauri.app/) (Rust backend + React frontend).

Reads session logs from `~/.claude/` and renders them as a scrollable conversation with expandable tool calls, token counts, and live tailing. GUI port of [tail-claude](https://github.com/kylesnowschwartz/tail-claude).

## Requirements

- [Rust](https://rustup.rs/) 1.77+
- Node.js 18+
- macOS: Xcode Command Line Tools (`xcode-select --install`)

## Install

Build from source:

```bash
git clone git@github.com:kylesnowschwartz/tail-claude-gui.git
cd tail-claude-gui
npm install
npm run tauri build
```

The built app will be in `src-tauri/target/release/bundle/`.

## Usage

Launch the app to open the session picker. It auto-discovers all sessions from `~/.claude/projects/`.

Select a session to view the conversation. Click messages to expand tool calls, or open the detail view for full inspection.

### Keybindings

`?` toggles keybind hints in any view.

**List view**

| Key | Action |
|-----|--------|
| `j` / `k` | Move cursor down / up |
| `G` / `g` | Jump to last / first message |
| `Tab` | Toggle expand/collapse current message |
| `e` / `c` | Expand / collapse all Claude messages |
| `Enter` | Open detail view |
| `d` | Open debug log viewer |
| `t` | Open team task board (when teams exist) |
| `s` / `q` / `Esc` | Open session picker |

**Detail view**

| Key | Action |
|-----|--------|
| `q` / `Esc` | Back to list |

**Session picker**

| Key | Action |
|-----|--------|
| `j` / `k` | Navigate sessions |
| `Enter` | Open selected session |
| `q` / `Esc` | Back to list |

**Debug log viewer**

| Key | Action |
|-----|--------|
| `q` / `Esc` | Back to list |

## Development

```bash
npm install
npm run tauri dev    # launch dev mode with hot reload
```

### Project structure

```
src/                  # React frontend (TypeScript)
  components/         # SessionPicker, MessageList, MessageDetail, TeamBoard, DebugViewer
  hooks/              # useSession, usePicker
  lib/                # format utilities, theme
  types/              # TypeScript type definitions
src-tauri/            # Rust backend
  src/
    parser/           # JSONL parsing (entry, classify, chunk, session, subagent, team, etc.)
    commands/         # Tauri commands (session, picker, git, debug)
    convert.rs        # Chunk → frontend message conversion
    watcher.rs        # File watching with debounce
    state.rs          # Shared app state
```

### Commands

```bash
npm run tauri dev     # dev mode with hot reload
npm run tauri build   # production build
cargo check --manifest-path src-tauri/Cargo.toml   # check Rust compilation
npx tsc --noEmit      # check TypeScript types
```

## Attribution

Parsing heuristics ported from [tail-claude](https://github.com/kylesnowschwartz/tail-claude), which itself ported from [claude-devtools](https://github.com/matt1398/claude-devtools).

## License

[MIT](LICENSE)
