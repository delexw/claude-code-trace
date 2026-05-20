# Phase 5 — Verify, commit, tag (local)

Goal: confirm the release branch is green, commit the release commit, tag it locally.
Everything in this phase is local — nothing leaves the machine yet.

## Step 5.1 — Run the full check suite

```bash
npm run check
```

The composition of `npm run check` (tsc + oxlint + oxfmt + clippy + cargo fmt + vitest +
cargo test) is documented in `AGENTS.md` — refer there if you need the breakdown. Any
failure must be fixed before continuing — do not proceed with a known-failing release.

If a test that was green on main fails on the release branch, suspect a missing
prerequisite commit (a fix the test depends on landed in main between `$LAST_TAG` and
the chosen subset's commits but wasn't cherry-picked). Either add the prerequisite to
the subset or fix the test to work without it.

## Step 5.2 — Stage everything

```bash
git status --short
git add CHANGELOG.md \
        package.json package-lock.json \
        src-tauri/Cargo.toml src-tauri/Cargo.lock \
        tui/package.json
# Plus any work-in-progress source files that belong in the release:
git add <other-files>
```

Use `git diff --cached --stat` to verify only the intended files are staged.

## Step 5.3 — Commit

```bash
git commit -m "chore(release): vX.Y.Z

<one-line summary mirroring the CHANGELOG opening paragraph>

See CHANGELOG.md for details."
```

Keep the body short — the CHANGELOG carries the detail. Project-level pre-commit hooks
will run as usual; handle them per the hook scripts in `.claude/hooks/`, not via any
skill-specific bypass.

## Step 5.4 — Tag

```bash
git tag -a vX.Y.Z -m "vX.Y.Z — <one-line summary>"
git log --oneline "$LAST_TAG"..vX.Y.Z
```

Verify the tag's commit range matches your intent exactly: no surprise commits, no
missing ones. The user should be able to read this list and recognize every entry.

If something's off, fix it now — before Phase 6. Local tags can be deleted with `git tag
-d vX.Y.Z` and recreated; pushed tags are much harder to undo.

(The original section 5.5 was renumbered into 5.4 — there's no 5.5.)

Proceed to Phase 6.
