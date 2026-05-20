# CHANGELOG Template

The repo uses [Keep a Changelog](https://keepachangelog.com/) conventions.

## First-time setup

If `CHANGELOG.md` doesn't yet exist, create it with this header:

```markdown
# Changelog

All notable changes to claude-code-trace are documented here. Versions follow
[semantic versioning](https://semver.org/).
```

Then append the per-release section below.

## Per-release section template

```markdown
## [X.Y.Z] — YYYY-MM-DD

One paragraph framing what this release is fundamentally about. Speak to a user, not a
maintainer — explain visible impact, not implementation.

### Added

- **Headline name** ([`<sha7>`](https://github.com/delexw/claude-code-trace/commit/<sha>)).
  Two or three sentences. Why it exists, what the user sees, any caveat. End with a
  pointer to verification or docs if useful.

### Fixed

- **Headline name** ([`<sha7>`](https://github.com/delexw/claude-code-trace/commit/<sha>)).
  What was broken, when it surfaced, how it surfaces now. Avoid jargon-only entries like
  "fix race condition" — say which user-visible behavior was wrong and is now right.

### Changed

- Internal refactors with no behavior change usually don't belong here. Only mention if
  the user has to update their own integration (e.g. config file rename).

### Removed

- Anything a user could notice as gone (CLI flag, config key, exported function from a
  consumed package).

[X.Y.Z]: https://github.com/delexw/claude-code-trace/releases/tag/vX.Y.Z
```

## Bucket rules

- **Added** for `feat:` commits.
- **Fixed** for `fix:` / `perf:` commits.
- **Changed** for `refactor:` / config / build adjustments that the user must notice.
- **Removed** for deletions of public surface.
- **Breaking Changes** (new bucket, comes first, before Added) for major releases. Lead
  with what broke and how the user migrates.

Skip `chore:` / `docs:` / `test:` / `ci:` unless the user specifically asks. They
clutter the CHANGELOG without informing users.

## Linking

Each bullet **must** link to its commit so readers can dig in:

```
([`ebb2ca5`](https://github.com/delexw/claude-code-trace/commit/ebb2ca5))
```

7-character SHAs are enough — git accepts the abbreviation.

The reference-link at the bottom (`[X.Y.Z]: https://...releases/tag/...`) is what GitHub
uses to make the version header a clickable link in rendered markdown. Keep it.

## Style

Write substantive bullets. Read the diff if the subject is terse — "fix off-by-one" is
useless to a user, "the picker no longer skips the first session card after a refresh"
is what they care about. Lean on the "what was broken / what is now right" frame for
Fixed, "what's new and why" for Added.

## After writing

Always format:

```bash
npx oxfmt CHANGELOG.md
```

oxfmt enforces consistent table / code-fence formatting that survives the project's
`fmt:check` gate.
