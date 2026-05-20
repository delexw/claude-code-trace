---
name: cut-release
description: |
  Cuts a new versioned release of claude-code-trace by detecting commits since
  the last tag, classifying them with conventional-commit rules to decide the
  semver bump (major / minor / patch), bumping every version-bearing file in
  sync, writing a CHANGELOG entry, building the release on a dedicated branch,
  tagging + pushing to trigger the GitHub Actions release pipeline, and
  publishing the resulting draft release on the repo page. Supports BOTH
  whole-main releases ("everything since v0.5.0") and curated releases ("only
  the watcher fix, even though main has other unreleased commits") via
  cherry-pick onto a fresh branch off the previous tag. Use this skill
  whenever the user mentions cutting, tagging, bumping, or shipping a
  release — including phrases like "cut a release", "release v1.2.3", "tag and
  release", "ship the watcher fix", "bump version to next", "release for X
  only", "publish a release", "do a patch release", or "make a new release".
  Trigger even when the user does not name the version explicitly — the skill
  computes it. Also use proactively when the user finishes a unit of work and
  asks to publish it as a release rather than just merging.
---

# Cut a Release — `claude-code-trace`

Turns "we should release this" into a tagged, pushed, pipeline-triggered, **publicly
published** release with a curated CHANGELOG, in a way that survives the project's strict
pre-commit hook chain and stays honest about what's actually shipping.

The skill is project-local because the steps depend on this repo's specific shape (three
version files, two lockfiles, GH Actions release on `v*` tag, the spec-drift +
test-reflection pre-commit hook pair).

---

## Before you start

Read these references on the first invocation in a session — they describe constants the
phase files refer to.

- `${CLAUDE_SKILL_DIR}/references/project-shape.md` — version files, lockfiles, pre-commit
  hooks, release pipeline wiring.
- `${CLAUDE_SKILL_DIR}/references/conventional-commits.md` — how commit prefixes map to
  semver bump level.

`${CLAUDE_SKILL_DIR}/references/changelog-template.md` is loaded when you reach Phase 4.

---

## Execution

### Phase 1 — Inspect repo state and propose scope

Read `${CLAUDE_SKILL_DIR}/steps/phase1-inspect-and-scope.md` and follow all instructions.

---

### Phase 2 — Build the release branch

Read `${CLAUDE_SKILL_DIR}/steps/phase2-build-release-branch.md` and follow all instructions.

---

### Phase 3 — Bump version files

Read `${CLAUDE_SKILL_DIR}/steps/phase3-bump-versions.md` and follow all instructions.

---

### Phase 4 — Write the CHANGELOG entry

Read `${CLAUDE_SKILL_DIR}/steps/phase4-changelog.md` and follow all instructions.

---

### Phase 5 — Verify, commit, tag

Read `${CLAUDE_SKILL_DIR}/steps/phase5-verify-commit-tag.md` and follow all instructions.

---

### Phase 6 — Push the tag (confirmation gate)

Read `${CLAUDE_SKILL_DIR}/steps/phase6-push-tag.md` and follow all instructions.

---

### Phase 7 — Publish the GitHub release

Read `${CLAUDE_SKILL_DIR}/steps/phase7-publish-github-release.md` and follow all
instructions.

---

### Phase 8 — Bring the release commit back to `main`

Read `${CLAUDE_SKILL_DIR}/steps/phase8-back-to-main.md` and follow all instructions.

---

### Phase 9 — Clean up

Read `${CLAUDE_SKILL_DIR}/steps/phase9-cleanup.md` and follow all instructions.

---

## Safety guardrails

These hold across every phase. The skill must never do any of them without an explicit
"yes" from the user in the current turn:

- `git push origin v*` — release-triggering, never reversible cleanly.
- `gh release edit ... --draft=false` — flips the release public on the repo page.
- `git push -f` or `--force` to any shared branch.
- `git branch -D` on an unmerged branch.
- `--no-verify` to bypass pre-commit hooks — fix the underlying issue instead.
- Cutting a **major** bump unless the user explicitly asked for it OR a `BREAKING
CHANGE:` footer / `!:` marker is present AND the user has confirmed they intend the
  major bump.
