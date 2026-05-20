# Phase 2 — Build the release branch

Goal: build the release on a dedicated branch off `$LAST_TAG` so the tag points at a
clean commit and `main` is never mutated until you're sure.

This phase is the same shape for linear and curated releases — both use cherry-pick onto
a branch off the previous tag. Cherry-picking even for a "linear" release is safer than
fast-forwarding `main`, because it makes the release explicitly a curated subset rather
than an implicit "everything that happened to be on main".

## Step 2.1 — Stash uncommitted work

Working-tree changes might be V1-style feature work you want bundled into the release.
Save them first:

```bash
git stash push -u -m "release/${NEXT_VERSION} wip"
```

`-u` includes untracked files. If working tree was already clean, the stash is a no-op
and step 2.5 will note "nothing to pop".

## Step 2.2 — Branch off the last tag

```bash
git checkout "$LAST_TAG" -b "release/${NEXT_VERSION}"
```

## Step 2.3 — Cherry-pick the chosen subset

In `$SUBSET` order:

```bash
git cherry-pick <sha-1> <sha-2> ... <sha-N>
```

Or one at a time if you expect conflicts:

```bash
git cherry-pick <sha>
# resolve conflicts, git add, git cherry-pick --continue
```

Order matters. Two commits that both touch the same file must be picked in the same
order they landed on main.

## Step 2.4 — Resolve cherry-pick conflicts

Common patterns:

- **Source conflict**: two commits touched the same lines. Read the markers, keep the
  semantically correct combination, `git add`, `git cherry-pick --continue`.
- **File doesn't exist**: the conflicting file was introduced by a commit you're not
  picking. `git rm <path>` if the file shouldn't exist in this release, or pull in the
  prerequisite commit.

## Step 2.5 — Pop the stash

```bash
git stash pop
```

Expect conflicts on `package-lock.json` and `src-tauri/Cargo.lock` because the stash was
created from a different base than the new branch. Resolve them by **regenerating** the
lockfiles, not by hand-merging:

```bash
git checkout HEAD -- package-lock.json src-tauri/Cargo.lock
npm install --package-lock-only
( cd src-tauri && cargo check --offline )
```

For other stash conflicts (e.g. a source file the stash modified but is shaped
differently on this base), read and merge manually. If the conflict is unrecoverable,
re-stash with `git stash push -u`, drop the offending file, and re-pop.

## Step 2.6 — Verify the branch state

```bash
git status --short
git log --oneline "$LAST_TAG"..HEAD
```

The log should show exactly the commits you intended to cherry-pick. The status should
show the work-in-progress changes (V1 files + any other in-flight work) as staged or
untracked, with no remaining conflict markers.

Proceed to Phase 3.
