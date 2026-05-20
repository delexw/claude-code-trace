# Phase 8 — Bring the release commit back to `main`

Goal: ensure `main` contains the version bump, CHANGELOG entry, and any in-flight
feature work that shipped in the release. Otherwise `main` lags behind the published
release and the next person to release will be confused.

For **linear** releases off `main`'s HEAD, this phase is a no-op — the release commit
already sits on main's trajectory and you just push. For **curated** releases (most
common), the release commit lives only on `release/<version>` and main has unrelated
commits.

## Step 8.1 — Switch back to main

```bash
git checkout main
git pull --ff-only
```

If `git pull` fails because main has new commits, fetch and rebase or merge before
continuing — don't operate on stale state.

## Step 8.2 — Cherry-pick the release commit

```bash
git cherry-pick <release-commit-sha>
```

The release commit's SHA is on `release/<version>` — find it with:

```bash
git log -1 release/<version> --format=%H
```

## Step 8.3 — Resolve lockfile conflicts

Expect `package-lock.json` and `src-tauri/Cargo.lock` to conflict because main's
dependency drift differs from the release branch's. Regenerate from main's state:

```bash
git checkout HEAD -- package-lock.json src-tauri/Cargo.lock
npm install --package-lock-only
( cd src-tauri && cargo check --offline )
```

Then stage:

```bash
git add -A
```

## Step 8.4 — Continue the cherry-pick

```bash
git cherry-pick --continue --no-edit
```

If `--no-edit` would produce a confusing commit message on main (e.g. the message says
"chore(release): vX.Y.Z" but main isn't a release branch), drop `--no-edit` and amend
the message to something like "chore: bring vX.Y.Z release commit into main".

Project-level pre-commit hooks will run as usual; handle them per the hook scripts in
`.claude/hooks/`, not via any skill-specific bypass.

## Step 8.5 — Push main

```bash
git push origin main
```

This is reversible (you can revert the commit), unlike pushing tags. Still, ask the user
before pushing if main has branch protection that requires PR review — in which case
skip step 8.5 here and use the optional PR path in Phase 9 of the workflow listed in
SKILL.md (or open a PR from `release/<version>` directly).

Proceed to Phase 9.
