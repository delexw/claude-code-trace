# Phase 9 — Clean up

Goal: remove the local release branch and any stash entry, leaving the working tree on
`main` in a clean state.

Ask the user before deleting anything that contains work not already on `origin/main` or
on the tag.

## Step 9.1 — Verify the release branch is fully captured

```bash
git log --oneline release/<version> ^main ^vX.Y.Z
```

If this prints nothing, every commit on `release/<version>` is already reachable from
either `main` or the `vX.Y.Z` tag, and the branch is safe to delete.

If it prints commits, **stop**. Something on the branch isn't preserved. Investigate
before deleting.

## Step 9.2 — Delete the local branch

```bash
git branch -D release/<version>
```

Use `-D` (force) rather than `-d` because the branch isn't merged into main in the
traditional sense — its commit was cherry-picked, not merged. The `-d` would refuse.

## Step 9.3 — Drop the stash

If you used `git stash push` in Phase 2 and have already reapplied everything you
needed:

```bash
git stash drop
```

This removes the saved stash entry. Skip if you still want a backup of the in-flight
work in case something needs to be reproduced.

## Step 9.4 — Final report to the user

Summarise:

- Tag pushed: `vX.Y.Z`
- Release URL: from `gh release view vX.Y.Z --json url --jq '.url'`
- Commits in the release: count and one-liner topic
- Main updated: yes/no (Phase 8 outcome)
- Any follow-ups: e.g. parser fixes that were skipped from this release and will need a
  future tag.

That's the end of the workflow.
