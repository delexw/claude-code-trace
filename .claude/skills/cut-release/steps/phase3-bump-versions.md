# Phase 3 — Bump version files

Goal: bump every version-bearing file to `$NEXT_VERSION` and regenerate the two
lockfiles.

Refer to `${CLAUDE_SKILL_DIR}/references/project-shape.md` for the rules on which files
move in lockstep and which move independently.

## Step 3.1 — Bump root + Cargo (always lockstep)

```bash
# Root package.json
# Edit: "version": "<old>" → "version": "<NEXT_VERSION>"

# src-tauri/Cargo.toml
# Edit: version = "<old>" → version = "<NEXT_VERSION>"
```

Use the Edit tool with `old_string`/`new_string` rather than sed — safer for surrounding
context.

## Step 3.2 — Bump TUI version

If the chosen subset contains any commit that modifies files under `tui/`, bump
`tui/package.json` by the same tier as the main release (patch / minor / major).

If the subset has **no** TUI changes, bump TUI only at the patch level — it keeps the
lockfile consistent without claiming a feature it doesn't have. Alternatively skip the
TUI bump entirely and mention this in the CHANGELOG.

## Step 3.3 — Regenerate lockfiles

```bash
npm install --package-lock-only
( cd src-tauri && cargo check --offline )
```

These commands write the new local-workspace version into the lockfiles. Confirm the
diffs are small and only touch the version strings:

```bash
git diff package-lock.json src-tauri/Cargo.lock | head -30
```

Large diffs suggest the lockfile was stale or has unrelated deps changes — investigate
before continuing.

Proceed to Phase 4.
