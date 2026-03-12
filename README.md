<p align="center">
  <img src="icon.png" alt="tail-claude-gui" width="128" />
</p>

# tail-claude-gui

A desktop GUI for reading Claude Code session JSONL files. Built with [Tauri v2](https://v2.tauri.app/) (Rust backend + React frontend).

Reads session logs from `~/.claude/` and renders them as a scrollable conversation with expandable tool calls, token counts, and live tailing. Inspired by [tail-claude](https://github.com/kylesnowschwartz/tail-claude).

<p align="center">
  <img src="demo.gif" alt="Demo" />
</p>

## Requirements

- [Rust](https://rustup.rs/) 1.77+
- Node.js 18+
- macOS: Xcode Command Line Tools (`xcode-select --install`)

## Install

### Download

Grab the latest `.dmg` from [Releases](https://github.com/delexw/tail-claude-gui/releases). Open it and drag the app to `/Applications`.

> The app is unsigned. On first launch, right-click > Open > Open to bypass Gatekeeper.

### Build from source

```bash
git clone git@github.com:delexw/tail-claude-gui.git
cd tail-claude-gui
npm install
npm run tauri build
```

The built app will be in `src-tauri/target/release/bundle/`.

## Usage

Launch the app to open the session picker. It auto-discovers all sessions from `~/.claude/projects/`.

Select a session to view the conversation. Click messages to expand tool calls, or open the detail view for full inspection.

MCP (Model Context Protocol) tool calls are automatically detected and displayed with human-friendly names. For example, `mcp__chrome-devtools__take_screenshot` renders as **MCP chrome-devtools** with the summary "take screenshot". Supported MCP servers include chrome-devtools, figma, atlassian, buildkite, cloudflare, and any other server following the `mcp__<server>__<tool>` naming convention.

### Keybindings

`?` toggles keybind hints in any view.

**List view**

| Key               | Action                                  |
| ----------------- | --------------------------------------- |
| `j` / `k`         | Move cursor down / up                   |
| `G` / `g`         | Jump to last / first message            |
| `Tab`             | Toggle expand/collapse current message  |
| `e` / `c`         | Expand / collapse all Claude messages   |
| `Enter`           | Open detail view                        |
| `d`               | Open debug log viewer                   |
| `t`               | Open team task board (when teams exist) |
| `s` / `q` / `Esc` | Open session picker                     |

**Detail view**

| Key         | Action       |
| ----------- | ------------ |
| `q` / `Esc` | Back to list |

**Session picker**

| Key         | Action                |
| ----------- | --------------------- |
| `j` / `k`   | Navigate sessions     |
| `Enter`     | Open selected session |
| `q` / `Esc` | Back to list          |

**Debug log viewer**

| Key         | Action       |
| ----------- | ------------ |
| `q` / `Esc` | Back to list |

## Development

```bash
npm install
npm run tauri dev        # dev mode with hot reload
npm run tauri build      # production build
```

### Check & Test

```bash
npm run check            # run all checks at once
npx vitest run           # frontend tests
cargo test --manifest-path src-tauri/Cargo.toml    # Rust tests
npx tsc --noEmit         # TypeScript type check
npx oxlint               # JS/TS lint
npx oxfmt                # JS/TS format
cargo clippy --manifest-path src-tauri/Cargo.toml  # Rust lint
cargo fmt --manifest-path src-tauri/Cargo.toml     # Rust format
```

## Release

Push a version tag to trigger a GitHub Actions build:

```bash
git tag v0.1.0
git push origin v0.1.0
```

This creates a draft release with the `.dmg` attached. Review and publish it from the [Releases](https://github.com/delexw/tail-claude-gui/releases) page.

## Attribution

Parsing heuristics inspired by [tail-claude](https://github.com/kylesnowschwartz/tail-claude) and [claude-devtools](https://github.com/matt1398/claude-devtools).

## License

[MIT](LICENSE)
