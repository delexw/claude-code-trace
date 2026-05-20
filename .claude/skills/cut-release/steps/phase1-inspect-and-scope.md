# Phase 1 — Inspect repo state and propose scope

Goal: identify the last release, list everything since it, and converge with the user on
**which subset** of those commits ships in this release plus the resulting version
string.

## Step 1.1 — Inspect

```bash
git fetch --tags --quiet
LAST_TAG=$(git describe --tags --abbrev=0)
git log "$LAST_TAG"..HEAD --pretty=format:'%h %s' --no-merges
```

Read each commit's subject (and body when terse) so you can classify by Conventional
Commit prefix. Refer to `${CLAUDE_SKILL_DIR}/references/conventional-commits.md` for the
prefix → bump-level table.

## Step 1.2 — Ask the user about scope

Two modes, mutually exclusive:

- **Linear release** — every commit since `$LAST_TAG` ships. Use when main is
  topic-coherent (e.g. only `fix:` commits since last tag, all about the same area).
- **Curated release** — only a chosen subset ships, even though main has other
  unreleased commits piled up. Use when main mixes unrelated work (parser fixes + dep
  bumps + a feature) and the user wants this release tied to one theme.

Use `AskUserQuestion` to confirm the mode if it's not already clear from context. If the
user named a topic ("watcher fix only", "the parser cleanups"), interpret that as
curated mode and pre-select the matching commits by grepping subjects for keywords
related to the topic — then show your picks for confirmation rather than guessing
silently.

## Step 1.3 — Decide bump and propose version

Classify the **chosen subset** of commits with the table in `conventional-commits.md`.
The release's bump tier is the highest tier among those commits. Compute the next
version:

- **major**: `X.0.0`
- **minor**: `X.Y.0`
- **patch**: `X.Y.Z+1`

State the proposal explicitly to the user:

> "I see 1 `feat:` and 3 `fix:` commits in the chosen subset since `v0.5.0` — that's a
> minor bump → `v0.6.0`. Confirm or override?"

The user may force a specific version (e.g. "release as v0.5.1 even though there's a
feat — I don't want a minor bump"). Honor it.

## Step 1.4 — Record the decision

Hold three values for the rest of the workflow:

- `$LAST_TAG` — e.g. `v0.5.0`
- `$NEXT_VERSION` — e.g. `0.5.1` (no `v` prefix in version files; with `v` in tag names)
- `$SUBSET` — list of commit SHAs that will be cherry-picked, **in their original order
  on main** (cherry-pick order matters when one commit depends on another)

Proceed to Phase 2.
