# Project Shape — `claude-code-trace`

The skill-specific facts the phase files rely on. For everything that's already in the
codebase, this file points at the source of truth rather than repeating it.

## Read these first if you don't know the codebase

- `AGENTS.md` (symlinked to `CLAUDE.md`) — toolchain commands (`npm run check`, `oxfmt`,
  `oxlint`, etc.) and the "format + lint + test before committing" rule.
- `.claude/hooks/pre-commit-spec.sh` and `.claude/hooks/pre-commit.sh` — header comments
  describe exactly what each hook checks and how the per-session flag bypass works.
- `.github/workflows/release.yml` — the release pipeline: tag pattern, build matrix,
  notes extraction, publish step.
- `.claude/settings.json` — the matcher that wires the hooks into `git commit` calls.

## Version-bearing files (skill-specific rule)

Three files must agree on the next-version string. Nothing in the codebase enforces this
sync — the skill does.

| File                   | Owns                             | Bumps with                                 |
| ---------------------- | -------------------------------- | ------------------------------------------ |
| `package.json` (root)  | Node/TS workspace + binary entry | the Rust crate (lockstep)                  |
| `src-tauri/Cargo.toml` | Rust crate version               | root `package.json` (lockstep)             |
| `tui/package.json`     | Terminal UI subpackage           | independently; bumps when TUI code changes |

Root + Cargo move together because the desktop binary the user installs is built from
both. `tui/package.json` tracks separately because it's shipped as its own npm package.

## Lockfile regen after a version bump

The lockfiles embed the local workspace's version, so they have to be regenerated after
editing version files — `npm run check` won't fix this on its own. Run:

```bash
npm install --package-lock-only           # → package-lock.json
( cd src-tauri && cargo check --offline ) # → src-tauri/Cargo.lock
```

`--package-lock-only` skips the full reinstall (nothing in `node_modules` needs to
change) and `--offline` skips the registry round-trip — only the local crate's version
moved.

## Release pipeline (delegated to CI)

`.github/workflows/release.yml` reads `CHANGELOG.md`, extracts the section matching the
pushed tag's version, attaches it to the auto-built draft release, and a final
`publish` job flips the draft to public. See Phase 7 for the verification flow.

This is the source of truth — if the pipeline changes, edit the workflow and update
Phase 7's narrative, not this file.

## GitHub repo identity

Read once with `git remote get-url origin`; the URL template for commits and releases
follows from there:

- Commit: `<repo-url>/commit/<sha>`
- Release: `<repo-url>/releases/tag/v<X.Y.Z>`

The CHANGELOG template (`changelog-template.md`) hardcodes `delexw/claude-code-trace`
in the link format. If the repo moves, update that file once.
