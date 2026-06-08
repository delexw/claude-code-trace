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

- **Headline name** ([`<sha7>`](https://github.com/delexw/claude-code-trace/commit/<sha>), @contributor).
  Two or three sentences. Why it exists, what the user sees, any caveat. End with a
  pointer to verification or docs if useful. (Drop `@contributor` when the author is the
  maintainer — see Attribution.)

### Fixed

- **Headline name** ([`<sha7>`](https://github.com/delexw/claude-code-trace/commit/<sha>), @contributor).
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

Always skip `chore:` / `docs:` / `test:` / `ci:` — they clutter the CHANGELOG without
informing users. The skill is non-interactive; never include these even if the count
looks short.

## Linking

Each bullet **must** link to its commit so readers can dig in:

```
([`ebb2ca5`](https://github.com/delexw/claude-code-trace/commit/ebb2ca5))
```

7-character SHAs are enough — git accepts the abbreviation.

The reference-link at the bottom (`[X.Y.Z]: https://...releases/tag/...`) is what GitHub
uses to make the version header a clickable link in rendered markdown. Keep it.

## Attribution

Credit external contributors. When a bullet's commit/PR was authored by **anyone other
than the maintainer (`delexw`)**, append the author's GitHub handle to the commit link so
the contribution is acknowledged:

```
([`ebb2ca5`](https://github.com/delexw/claude-code-trace/commit/ebb2ca5), @contributor)
```

- Omit the handle entirely for the maintainer's own commits (`delexw`) — don't write
  `@delexw`. The default authorship is the maintainer, so an unattributed bullet means
  "by the maintainer".
- One handle per bullet (the PR author). If a bullet summarizes several commits by the
  same external author, attribute once.
- Phase 4 explains how to resolve the handle from each commit/PR.

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
