# Phase 5 — Verify, commit, tag (local)

Goal: confirm the release branch is green, commit the release commit through the
project's pre-commit hook chain, tag it locally. Nothing leaves the machine yet.

## Step 5.1 — Run the full check suite

```bash
npm run check
```

`npm run check` runs tsc + oxlint + oxfmt + clippy + cargo fmt + vitest + cargo test
(see `AGENTS.md`). Any failure aborts the release — do not proceed with known-failing
state.

## Step 5.2 — Stage the release files

```bash
git add CHANGELOG.md \
        package.json package-lock.json \
        src-tauri/Cargo.toml src-tauri/Cargo.lock \
        src-tauri/tauri.conf.json
git diff --cached --stat
```

The staged set must be exactly those six files. Nothing else.

## Step 5.3 — Write the commit message to a temp file

```bash
cat > /tmp/release-commit-msg.txt << 'MSG'
chore(release): vX.Y.Z

<one-line summary mirroring the CHANGELOG opening paragraph — keep under 80 chars
per line, no shell variable syntax like ${...} which Buildkite would interpret>

See CHANGELOG.md for details.
MSG
```

Use a single-quoted heredoc delimiter (`'MSG'`) so backticks and `$VAR` in the body are
not interpreted. The CHANGELOG carries the per-bullet detail; keep the commit body to a
sentence or two.

## Step 5.4 — Pre-commit hook touch-flag dance

The project enforces two `Bash(*git commit*)` PreToolUse hooks (`pre-commit-spec.sh` +
`pre-commit.sh`) that block the first commit attempt unless flag files exist for the
current `session_id`. Both hooks consume their flag on successful commit and re-arm for
the next one. The session-id-derived path is what the hook prints in its error message —
read the path from the hook output, don't guess.

For a release commit (version bump + CHANGELOG only, no code change), specs and tests
genuinely don't need updating, so the touch-flag is the correct escape. The flags must
be set in a **separate Bash call** from the commit itself — the hooks reject same-call
touches as "not a conscious confirmation".

Sequence (each bullet = one Bash call):

1. Set both flags:

   ```bash
   touch /tmp/claude-spec-confirmed-${SESSION_ID}
   touch /tmp/claude-tests-confirmed-${SESSION_ID}
   ```

2. Commit:

   ```bash
   git commit -F /tmp/release-commit-msg.txt
   ```

If the spec hook blocks on the first attempt (it always does the very first time it
runs), open the hook's printed `FLAG_FILE` path, re-touch both flags in a separate call,
then retry the commit. Two retries max — if a third attempt is blocked by a non-hook
reason (lint / test failure), abort and surface the failure.

## Step 5.5 — Tag locally

```bash
git tag -a "v$NEXT_VERSION" -m "v$NEXT_VERSION — <one-line summary>

<short paragraph mirroring the CHANGELOG opening>

See CHANGELOG.md for details."

git log --oneline "$LAST_TAG"..v$NEXT_VERSION
```

Verify the tag's commit range matches Phase 1's count exactly. If something's off, fix
it now — local tags can be deleted with `git tag -d` and recreated; pushed tags are much
harder to undo.

Proceed to Phase 6.
