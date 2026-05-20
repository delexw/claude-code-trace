# Phase 6 — Push the tag

Goal: push the local tag to `origin` so the GitHub Actions release pipeline runs and
builds the macOS / Linux / Windows artifacts. No interactive confirmation — the
preflight in Phase 1 already verified the version isn't a duplicate; this phase verifies
that nothing changed between Phase 1 and now.

## Step 6.1 — Final duplicate-version preflight

A second check matters because Phase 1's verdict can become stale: another release run
could have raced this one between Phase 1 and Phase 6 (e.g. on a CI runner or a parallel
session).

```bash
git fetch --tags --quiet origin
if git ls-remote --tags origin "refs/tags/v$NEXT_VERSION" | grep -q "v$NEXT_VERSION$"; then
  echo "Error: v$NEXT_VERSION appeared on origin between Phase 1 and Phase 6. Aborting to avoid clobbering the existing tag."
  exit 1
fi
```

If this trips, abort — surface the conflicting remote SHA and let the user resolve.
Never `--force` to recover.

## Step 6.2 — Push the tag

```bash
git push origin "v$NEXT_VERSION"
```

The release workflow starts on GitHub within seconds. Confirm by:

```bash
gh run list --workflow=release.yml --limit=1
```

You should see a queued or in-progress run for the tag. If the run is not found within
~10 seconds, the workflow file may not be on the default branch — check
`.github/workflows/release.yml` exists on `main` and the `on: push: tags: [v*]` trigger
is intact.

Proceed to Phase 7.
