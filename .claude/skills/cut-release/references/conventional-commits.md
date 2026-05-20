# Conventional Commit → Semver Bump

The bump level for a release is the highest tier among the commits the user wants to
ship. One `feat:` upgrades the whole release to minor; one `feat!:` or breaking-change
marker upgrades it to major.

| Prefix or marker in commit subject / body        | Bump tier                   | Notes                                                                                |
| ------------------------------------------------ | --------------------------- | ------------------------------------------------------------------------------------ |
| `feat:` or `feat(scope):`                        | **minor**                   | new user-facing feature                                                              |
| `fix:` `fix(scope):`                             | **patch**                   | bug fix                                                                              |
| `perf:` `perf(scope):`                           | **patch**                   | observable performance improvement                                                   |
| `refactor:` `refactor(scope):`                   | **patch**                   | internal restructure with no behavior change                                         |
| `chore:` `docs:` `test:` `ci:` `build:` `style:` | **patch** (or "no release") | usually skip in CHANGELOG; if the whole release is only these, still call it a patch |
| `!` after type — e.g. `feat!:`, `fix!:`          | **major**                   | breaking change in API or schema                                                     |
| `BREAKING CHANGE:` footer (anywhere)             | **major**                   | always a major bump                                                                  |
| Anything else / no convention                    | judge case-by-case          | read the diff, ask the user if unclear                                               |

## Algorithm

1. List commit subjects: `git log $LAST_TAG..HEAD --pretty=format:'%H %s%n%b' --no-merges`
2. For each commit in the **chosen subset** (linear or curated), classify by the table
   above.
3. The release's bump tier = the highest tier observed.
4. Compute the next version:
   - **major**: `X.0.0` (e.g. `0.5.1` → `1.0.0`, `1.2.3` → `2.0.0`)
   - **minor**: `X.Y.0` (e.g. `0.5.1` → `0.6.0`)
   - **patch**: `X.Y.Z+1` (e.g. `0.5.0` → `0.5.1`)
5. State the proposed version and ask the user to confirm or override.

## Pre-1.0 caveat

While the version is `0.X.Y` (we are at the time of writing), the project follows
"`0.minor.patch`" loosely — breaking changes can still bump only the minor instead of
forcing a 1.0. Defer to the user. Don't auto-promote to `1.0.0` even when a breaking
marker appears unless the user explicitly asks.

## `tui/package.json` track

The TUI's version moves independently. If the chosen subset contains commits that
modified files under `tui/`, bump the TUI version by the same tier; otherwise bump only
its patch number (so its lockfile stays in sync but the user-visible TUI npm version
ticks gently).

If the subset contains **no** TUI changes at all, you may skip bumping `tui/package.json`
entirely — but then mention this in the CHANGELOG explicitly so consumers know the TUI
package didn't release.
