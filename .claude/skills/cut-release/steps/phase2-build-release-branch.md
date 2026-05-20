# Phase 2 — Build the release branch

Goal: create a dedicated `release/v$NEXT_VERSION` branch off the current `main` HEAD so
the version-bump + CHANGELOG commit lands on its own ref. `main` is never mutated until
Phase 8.

Phase 1's clean-tree precondition means this phase has nothing to stash and no
cherry-picks to resolve. Linear scope = branch off `main` HEAD = functionally identical
to "cherry-pick every commit since `$LAST_TAG` in order" but without the merge-conflict
risk inherent to replaying 30+ commits.

## Step 2.1 — Branch off main HEAD

```bash
git checkout main
git pull --ff-only origin main
git checkout -b "release/v$NEXT_VERSION"
git log --oneline -1
```

The branch is now a single-commit-ahead-or-equal of `main` and tracking nothing on
remote. We don't push it — the tag we create in Phase 5 is the only ref CI needs.

## Step 2.2 — Sanity check the commit range

```bash
git log --oneline "$LAST_TAG"..HEAD | wc -l
```

If the count is zero, abort — there's nothing to release. If the count looks wildly off
(e.g. 500 commits when you expected 40), abort and surface the unexpected log so the
user can investigate; do not silently ship a huge release.

Proceed to Phase 3.
