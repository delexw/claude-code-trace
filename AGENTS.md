# CLAUDE.md

## Commands

```bash
# Dev
npm run dev              # Vite dev server
npm run tauri dev        # Full Tauri app

# Lint
npx oxlint              # JS/TS lint
cargo clippy --manifest-path src-tauri/Cargo.toml  # Rust lint

# Format
npx oxfmt               # JS/TS format
cargo fmt --manifest-path src-tauri/Cargo.toml     # Rust format

# Test
npx vitest run           # Frontend tests
cargo test --manifest-path src-tauri/Cargo.toml    # Rust tests

# Type check
npx tsc --noEmit

# All at once
npm run check            # tsc + oxlint + oxfmt --check + clippy + cargo fmt --check + vitest + cargo test
```

## Rule

After every code change (src, tests, config that affects build), always add enough tests for the changes, then run lint, format, and test before committing:

```bash
npx oxfmt && npx oxlint && npx tsc --noEmit && npx vitest run && cargo fmt --manifest-path src-tauri/Cargo.toml && cargo clippy --manifest-path src-tauri/Cargo.toml && cargo test --manifest-path src-tauri/Cargo.toml
```

<!-- gitnexus:start -->

# GitNexus — Code Intelligence

This project is indexed by GitNexus as **claude-code-trace** (2596 symbols, 5375 relationships, 223 execution flows). Use the GitNexus MCP tools to understand code, assess impact, and navigate safely.

> If any GitNexus tool warns the index is stale, run `npx gitnexus analyze` in terminal first.

## Always Do

- **MUST run impact analysis before editing any symbol.** Before modifying a function, class, or method, run `gitnexus_impact({target: "symbolName", direction: "upstream"})` and report the blast radius (direct callers, affected processes, risk level) to the user.
- **MUST run `gitnexus_detect_changes()` before committing** to verify your changes only affect expected symbols and execution flows.
- **MUST warn the user** if impact analysis returns HIGH or CRITICAL risk before proceeding with edits.
- When exploring unfamiliar code, use `gitnexus_query({query: "concept"})` to find execution flows instead of grepping. It returns process-grouped results ranked by relevance.
- When you need full context on a specific symbol — callers, callees, which execution flows it participates in — use `gitnexus_context({name: "symbolName"})`.

## Never Do

- NEVER edit a function, class, or method without first running `gitnexus_impact` on it.
- NEVER ignore HIGH or CRITICAL risk warnings from impact analysis.
- NEVER rename symbols with find-and-replace — use `gitnexus_rename` which understands the call graph.
- NEVER commit changes without running `gitnexus_detect_changes()` to check affected scope.

## Resources

| Resource                                           | Use for                                  |
| -------------------------------------------------- | ---------------------------------------- |
| `gitnexus://repo/claude-code-trace/context`        | Codebase overview, check index freshness |
| `gitnexus://repo/claude-code-trace/clusters`       | All functional areas                     |
| `gitnexus://repo/claude-code-trace/processes`      | All execution flows                      |
| `gitnexus://repo/claude-code-trace/process/{name}` | Step-by-step execution trace             |

## CLI

| Task                                         | Read this skill file                                        |
| -------------------------------------------- | ----------------------------------------------------------- |
| Understand architecture / "How does X work?" | `.claude/skills/gitnexus/gitnexus-exploring/SKILL.md`       |
| Blast radius / "What breaks if I change X?"  | `.claude/skills/gitnexus/gitnexus-impact-analysis/SKILL.md` |
| Trace bugs / "Why is X failing?"             | `.claude/skills/gitnexus/gitnexus-debugging/SKILL.md`       |
| Rename / extract / split / refactor          | `.claude/skills/gitnexus/gitnexus-refactoring/SKILL.md`     |
| Tools, resources, schema reference           | `.claude/skills/gitnexus/gitnexus-guide/SKILL.md`           |
| Index, status, clean, wiki CLI commands      | `.claude/skills/gitnexus/gitnexus-cli/SKILL.md`             |

<!-- gitnexus:end -->
