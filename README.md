<p align="center">
  <img src="icon.png" alt="claude-code-trace" width="128" />
</p>

# Claude Code Trace

A desktop + web viewer for Claude Code session JSONL files. Built with [Tauri v2](https://v2.tauri.app/) (Rust backend + React frontend).

Reads session logs from `~/.claude/` and renders them as a scrollable conversation with expandable tool calls, token counts, and live tailing. Works as a **native desktop app** (macOS, Linux, Windows) or as a **web app** in any browser.

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

## License

[MIT](LICENSE)
