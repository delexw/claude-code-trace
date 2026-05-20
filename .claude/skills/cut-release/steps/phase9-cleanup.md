# Phase 9 — Clean up

Goal: remove the local release branch and emit a final summary. Fully automated.

## Step 9.1 — Verify the release branch is fully captured

```bash
git log --oneline "release/v$NEXT_VERSION" "^main" "^v$NEXT_VERSION"
```

Empty output = every commit on the release branch is reachable from `main` or the
`v$NEXT_VERSION` tag, so the branch is safe to delete.

If this prints commits, **abort** — something on the branch isn't preserved. Do not
delete. Surface the orphan commits so the user can investigate.

## Step 9.2 — Delete the local branch

```bash
git branch -D "release/v$NEXT_VERSION"
```

Use `-D` (force) because the branch isn't merged in the traditional sense — its commit
is reachable via the tag and via main, but git's `-d` check doesn't always recognize
that.

## Step 9.3 — Final report

Emit a single summary block (no follow-up questions). Include:

- Tag: `v$NEXT_VERSION`
- Release URL: from `gh release view v$NEXT_VERSION --json url --jq '.url'`
- Commit count in the release (Phase 1's number)
- Top-level theme (one short sentence)
- `origin/main` HEAD short SHA + subject (proves the release commit landed)
- Pipeline conclusion (5/5 green, or list the failing job)

That's the end of the workflow.
