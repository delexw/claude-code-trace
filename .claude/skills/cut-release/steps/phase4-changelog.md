# Phase 4 — Write the CHANGELOG entry

Goal: append (or create) `CHANGELOG.md` with a section for the new version that
faithfully describes what shipped, with commit links.

Load `${CLAUDE_SKILL_DIR}/references/changelog-template.md` now if you haven't already —
it has the section template, bucket rules, link format, and style notes.

## Step 4.1 — Write the section

Append a new section above any prior version sections. The section template includes:

- `## [X.Y.Z] — YYYY-MM-DD` header
- A one-paragraph framing
- `### Added` / `### Fixed` / `### Changed` / `### Removed` buckets as needed
- Optional `### Breaking Changes` first if the bump is major
- The `[X.Y.Z]: https://...` reference link at the bottom of the file

For each chosen commit in `$SUBSET`:

1. Map its conventional prefix to a bucket:
   - `feat:` → Added
   - `fix:` / `perf:` → Fixed
   - `refactor:` / config-only changes the user notices → Changed
   - deletions of public surface → Removed
2. Read the diff if the subject is terse — the bullet should explain user-visible impact,
   not summarize the commit message verbatim.
3. End the bullet with a commit link: ``([`<sha7>`](https://github.com/delexw/claude-code-trace/commit/<sha>))``

Skip `chore:` / `docs:` / `test:` / `ci:` unless the user asks; they clutter
user-facing CHANGELOGs.

## Step 4.2 — Format

```bash
npx oxfmt CHANGELOG.md
```

This is required — `npm run fmt:check` in Phase 5 will fail otherwise.

## Step 4.3 — Sanity check

Read the final CHANGELOG entry top-to-bottom as if you were a user encountering it on
the release page. Look for:

- Vague bullets ("fix issue", "improve performance") — rewrite with the visible
  symptom.
- Missing commit links — every bullet needs one.
- Wrong bucket — features under Fixed, bug fixes under Added, etc.
- Repetitive openers — vary sentence structure.

Proceed to Phase 5.
