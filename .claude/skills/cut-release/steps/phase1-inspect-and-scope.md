# Phase 1 — Inspect and decide

Goal: identify the last release, list everything since it, compute the next version
deterministically, and lock in the three variables the rest of the workflow uses. No
user interaction.

## Step 1.1 — Fetch + read state

```bash
git fetch --tags --prune --quiet origin
LAST_TAG=$(git describe --tags --abbrev=0)              # last tag in HEAD's ancestry
LAST_VERSION="${LAST_TAG#v}"                            # strip the leading v
git log "$LAST_TAG"..HEAD --pretty=format:'%h %s' --no-merges
```

The skill always operates on **linear** scope: everything between `$LAST_TAG` and `HEAD`
on `main` ships. Curated subsets are explicitly out of scope — they invite ambiguity and
the skill is non-interactive. If the user wants a curated release they have to land the
unwanted commits on a different branch first.

## Step 1.2 — Classify and compute the bump

Read each commit subject (and body when terse) and apply the table in
`${CLAUDE_SKILL_DIR}/references/conventional-commits.md`. The release's bump tier is the
**highest** tier observed.

Pre-1.0 rule (the project is currently `0.X.Y`): a `BREAKING CHANGE:` / `!:` marker maps
to a **minor** bump, not a major. The skill never silently promotes to `1.0.0`. If the
user explicitly asks for the 1.0 bump in the same turn, honor it; otherwise cap at minor.

Compute `NEXT_VERSION` from `$LAST_VERSION`:

- **minor**: `X.Y+1.0`
- **patch**: `X.Y.Z+1`

State the computed bump tier and version inline in your status message — not as a
question — e.g. "Highest tier in the 41-commit subset is `feat:` → minor bump →
v0.6.0". Then proceed.

## Step 1.3 — Preflight: duplicate version

```bash
if git ls-remote --tags origin "refs/tags/v$NEXT_VERSION" | grep -q "v$NEXT_VERSION$"; then
  echo "Error: v$NEXT_VERSION already exists on origin. Refusing to release the same version twice."
  exit 1
fi
```

Also check locally:

```bash
git tag -l "v$NEXT_VERSION" | grep -q "^v$NEXT_VERSION$" && {
  echo "Error: v$NEXT_VERSION exists locally. Delete with 'git tag -d v$NEXT_VERSION' first if it's stale."
  exit 1
}
```

If either check trips, **abort the skill** and surface the error verbatim. Do not
silently bump again — the user needs to know their state is inconsistent.

## Step 1.4 — Preflight: clean working tree

```bash
git status --short
```

If anything is uncommitted, abort with the file list. The skill operates on a clean tree
so the release commit only contains version + CHANGELOG diffs (and so the harness's
pre-commit hooks have a stable input).

## Step 1.5 — Lock in the three variables

For the rest of the workflow:

- `$LAST_TAG` — e.g. `v0.5.0`
- `$NEXT_VERSION` — e.g. `0.6.0` (no `v`)
- `$SUBSET` — the commit range `$LAST_TAG..HEAD` (always linear)

Proceed to Phase 2.
