<p align="center">
  <img src="icon.png" alt="claude-code-trace" width="128" />
</p>

# Claude Code Trace

[![CI](https://github.com/delexw/claude-code-trace/actions/workflows/ci.yml/badge.svg)](https://github.com/delexw/claude-code-trace/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.77.2%2B-orange?logo=rust)](https://www.rust-lang.org/)
[![React](https://img.shields.io/badge/react-19-61DAFB?logo=react&logoColor=white)](https://react.dev/)
[![Tauri](https://img.shields.io/badge/tauri-v2-24C8D8?logo=tauri&logoColor=white)](https://v2.tauri.app/)
[![Platform](https://img.shields.io/badge/platform-macOS%20%7C%20Linux%20%7C%20Windows-blue)](https://github.com/delexw/claude-code-trace/releases)

Browse, tail, and search Claude Code session logs in real time. Reads `~/.claude/projects/` JSONL files and renders them as scrollable conversations with expandable tool calls, token counts, and live tailing. Runs as a **native desktop app** (macOS, Linux, Windows), **web app**, or **TUI** — built with Tauri v2 (Rust) + React.

<p align="center">
  <img src="demo.gif" alt="Demo" />
</p>

## Install

### Build from source (any platform with Rust + Node.js)

```bash
git clone git@github.com:delexw/claude-code-trace.git
cd claude-code-trace
./script/install.sh       # builds everything + installs to PATH

cctrace              # desktop app (default)
cctrace --web        # web mode (opens browser)
cctrace --tui        # terminal UI
```

### Download pre-built

Grab the latest release from [Releases](https://github.com/delexw/claude-code-trace/releases):

| Platform | File                            |
| -------- | ------------------------------- |
| macOS    | `.dmg`                          |
| Linux    | `.deb`, `.rpm`, `.AppImage`     |
| Windows  | `.msi`, `.exe` (NSIS installer) |

> [!IMPORTANT]
> **macOS:** The app is unsigned. After installing, remove the quarantine attribute:
>
> ```bash
> xattr -cr /Applications/Claude\ Code\ Trace.app
> ```

### Run from source (no install)

```bash
git clone git@github.com:delexw/claude-code-trace.git
cd claude-code-trace
npm install

npm run tauri dev        # desktop app with hot reload
npm run dev:web          # web mode (opens browser)
npm run dev:tui          # TUI (starts backend + terminal UI)
```

### Run in Docker (web mode only)

```bash
docker build -t claude-code-trace .
docker run --rm -p 1421:1421 \
  -v "$HOME/.claude:/home/app/.claude:ro" \
  claude-code-trace
# then open http://localhost:1421
```

Or with compose: `docker compose up --build`. See
[docs/docker.md](docs/docker.md) for runtime env vars, volume layout, and
troubleshooting.

## Requirements

- [Rust](https://rustup.rs/) 1.77+
- Node.js 18+
- macOS: Xcode Command Line Tools (`xcode-select --install`)
- Linux: `libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libxdo-dev libssl-dev`
- Windows: [WebView2](https://developer.microsoft.com/en-us/microsoft-edge/webview2/) (pre-installed on Windows 10/11)

## Usage

```bash
cctrace              # desktop app (default)
cctrace --web        # web mode (opens browser at http://localhost:1420)
cctrace --tui        # terminal UI (starts backend + TUI together)
```

> **Note:** The TUI is functional but has a few UX rough edges — contributions welcome.

Launch to open the session picker. It auto-discovers all sessions from `~/.claude/projects/`.

In desktop mode, click **Open in Browser** in the toolbar to switch to browser mode — this opens `http://localhost:1420` in your default browser and hides the desktop window.

If you installed the pre-built `.dmg`/`.deb`/`.msi`, you can also launch the desktop app directly and pass `--web` to the binary:

```bash
# macOS
/Applications/Claude\ Code\ Trace.app/Contents/MacOS/Claude\ Code\ Trace --web
```

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

| Key         | Action                         |
| ----------- | ------------------------------ |
| `j` / `k`   | Navigate items                 |
| `Tab`       | Toggle expand/collapse item    |
| `Enter`     | Open subagent or toggle expand |
| `h` / `l`   | Switch panels left / right     |
| `q` / `Esc` | Back to list                   |

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
npm run tauri dev        # desktop app with hot reload
npm run dev:web          # web mode (opens browser, no desktop window)
npm run dev:tui          # TUI (starts backend + terminal UI together)
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
git tag v0.4.0
git push origin v0.4.0
```

This creates a draft release with macOS, Linux, and Windows artifacts attached. Review and publish it from the [Releases](https://github.com/delexw/claude-code-trace/releases) page.

## Contributing

Bug reports, feature requests, and pull requests are welcome. See [Development](#development) for how to build and run locally. For significant changes, open an issue first to align on scope.

## License

[MIT](LICENSE)
