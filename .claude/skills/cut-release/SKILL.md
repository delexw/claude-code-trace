---
name: cut-release
description: |
  Cuts a new versioned release of claude-code-trace end-to-end without asking
  any questions. Detects commits since the last ancestral tag, classifies them
  with conventional-commit rules to pick the semver bump, bumps every
  version-bearing file in sync, writes the CHANGELOG entry, commits and tags
  on a release branch, pushes the tag to trigger the GitHub Actions release
  pipeline, fast-forwards `main` onto the release commit and pushes it, then
  cleans up the local branch. Use this skill whenever the user mentions
  cutting, tagging, bumping, or shipping a release — including "cut a
  release", "release v1.2.3", "tag and release", "bump version", "publish a
  release", "do a patch release", or "make a new release". Trigger even when
  the user does not name the version — the skill computes it. Also use
  proactively when the user finishes a unit of work and asks to publish it.
---

# Cut a Release — `claude-code-trace`

Turns "we should release this" into a tagged, pushed, pipeline-triggered, **publicly
published** release with a curated CHANGELOG, in a way that survives the project's strict
pre-commit hook chain and stays honest about what's actually shipping.

The skill is project-local because the steps depend on this repo's specific shape (three
version files, two lockfiles, GH Actions release on `v*` tag, the spec-drift +
test-reflection pre-commit hook pair).

## Operating mode

The skill is **fully automated end-to-end** and **fully synchronous**. It never calls
`AskUserQuestion`, never waits for "yes", never branches on user preference, and
**never runs any command in the background**. Every Bash invocation runs in the
foreground so the session holds continuously from Phase 1 through Phase 9 — no
`run_in_background: true`, no trailing `&`, no `nohup`, no `disown`. This includes
long-running steps like `gh run watch` in Phase 7; set the Bash `timeout` parameter to
match the expected duration (e.g. 1800000 ms for the release pipeline) rather than
detaching.

Defaults are deterministic:

- **Scope** — always linear: every commit since `git describe --tags --abbrev=0` ships.
- **Bump tier** — highest conventional-commit tier in the subset (see
  `${CLAUDE_SKILL_DIR}/references/conventional-commits.md`). `BREAKING CHANGE:` or `!:`
  produces a **minor** bump while the version is `0.X.Y` (pre-1.0 caveat) and a **major**
  bump otherwise; the skill never silently promotes to 1.0.0.
- **CHANGELOG** — every Added/Fixed bullet is written from commit subjects + diff reads;
  `chore:` / `docs:` / `test:` / `ci:` are always skipped.
- **Push order** — tag first, then main (so CI sees the tag immediately; main catches up
  after).

The skill aborts (loudly) only on hard preconditions that would corrupt the release:

- **Duplicate version** — the proposed `vX.Y.Z` tag already exists on `origin`. Abort
  with `Error: vX.Y.Z already exists on origin (commit <sha>). Refusing to release the
same version twice.` The user must delete the remote tag intentionally if they want a
  re-release, or bump again.
- **Dirty working tree** — uncommitted changes the skill didn't create. Abort with a
  list of dirty files.
- **`npm run check` fails** — abort and surface the failure verbatim.

## Before you start

Read these references on the first invocation in a session — they describe constants the
phase files refer to.

- `${CLAUDE_SKILL_DIR}/references/project-shape.md` — version files, lockfiles, pre-commit
  hooks, release pipeline wiring.
- `${CLAUDE_SKILL_DIR}/references/conventional-commits.md` — how commit prefixes map to
  semver bump level.

`${CLAUDE_SKILL_DIR}/references/changelog-template.md` is loaded when you reach Phase 4.

## Execution

### Phase 1 — Inspect and decide

Read `${CLAUDE_SKILL_DIR}/steps/phase1-inspect-and-scope.md` and follow all instructions.

### Phase 2 — Build the release branch

Read `${CLAUDE_SKILL_DIR}/steps/phase2-build-release-branch.md` and follow all instructions.

### Phase 3 — Bump version files

Read `${CLAUDE_SKILL_DIR}/steps/phase3-bump-versions.md` and follow all instructions.

### Phase 4 — Write the CHANGELOG entry

Read `${CLAUDE_SKILL_DIR}/steps/phase4-changelog.md` and follow all instructions.

### Phase 5 — Verify, commit, tag

Read `${CLAUDE_SKILL_DIR}/steps/phase5-verify-commit-tag.md` and follow all instructions.

### Phase 6 — Push the tag (with duplicate-version preflight)

Read `${CLAUDE_SKILL_DIR}/steps/phase6-push-tag.md` and follow all instructions.

### Phase 7 — Watch the pipeline and verify the release is public

Read `${CLAUDE_SKILL_DIR}/steps/phase7-publish-github-release.md` and follow all
instructions.

### Phase 8 — Push the release commit to `main`

Read `${CLAUDE_SKILL_DIR}/steps/phase8-back-to-main.md` and follow all instructions.

### Phase 9 — Clean up

Read `${CLAUDE_SKILL_DIR}/steps/phase9-cleanup.md` and follow all instructions.

## What the skill will NOT do automatically

- Bypass pre-commit hooks with `--no-verify` — always fix the underlying issue.
- Force-push (`git push -f`) to `main`, the release tag, or any shared branch.
- Delete a remote tag or remote branch.
- Cut a **major** bump (`X.0.0`) while the version is still `0.X.Y` — pre-1.0 the skill
  caps at minor even when a breaking change is detected.
- Re-tag an existing version — the duplicate-version preflight in Phase 6 aborts first.

The harness's auto-mode classifier may still intercept pushes to shared branches (e.g.
`git push origin main`) — that is out of skill scope. When it does, surface the error
and continue with the remaining phases; the user resolves the push themselves.
