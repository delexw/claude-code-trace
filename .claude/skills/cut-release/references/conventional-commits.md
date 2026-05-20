# Conventional Commit → Semver Bump

The bump level for a release is the highest tier among the commits in the release
subset. One `feat:` upgrades the whole release to minor.

| Prefix or marker in commit subject / body        | Bump tier                      | Notes                                                                                |
| ------------------------------------------------ | ------------------------------ | ------------------------------------------------------------------------------------ |
| `feat:` or `feat(scope):`                        | **minor**                      | new user-facing feature                                                              |
| `fix:` `fix(scope):`                             | **patch**                      | bug fix                                                                              |
| `perf:` `perf(scope):`                           | **patch**                      | observable performance improvement                                                   |
| `refactor:` `refactor(scope):`                   | **patch**                      | internal restructure with no behavior change                                         |
| `chore:` `docs:` `test:` `ci:` `build:` `style:` | **patch** (or "no release")    | usually skip in CHANGELOG; if the whole release is only these, still call it a patch |
| `!` after type — e.g. `feat!:`, `fix!:`          | **major** (capped — see below) | breaking change in API or schema                                                     |
| `BREAKING CHANGE:` footer (anywhere)             | **major** (capped — see below) | breaking change in API or schema                                                     |
| Anything else / no convention                    | **patch**                      | default conservatively — most non-conventional commits are docs/chore-shaped         |

## Algorithm (fully automated)

1. List commits: `git log $LAST_TAG..HEAD --pretty=format:'%H %s%n%b' --no-merges`
2. Classify each by the table above; pick the highest tier.
3. Apply the pre-1.0 cap (next section).
4. Compute the next version:
   - **major**: `X+1.0.0` (e.g. `1.2.3` → `2.0.0`)
   - **minor**: `X.Y+1.0` (e.g. `0.5.1` → `0.6.0`)
   - **patch**: `X.Y.Z+1` (e.g. `0.5.0` → `0.5.1`)
5. State the chosen bump inline (not as a question) and proceed.

## Pre-1.0 cap (currently active — version is `0.X.Y`)

While the major version is still `0`, the skill **caps the bump at minor** even when a
`BREAKING CHANGE:` footer or `!:` marker is present. Rationale: `0.X.Y` semver does not
imply API stability, so breaking changes are expected; promoting to `1.0.0` is a
deliberate product decision, not an automated one.

The cap is automatic — the skill never promotes to `1.0.0` without the user explicitly
typing "release as 1.0.0" or similar in the same turn. If the user did, honor it.

Remove this cap (in code review of this file, not at runtime) once the project
intentionally crosses `1.0.0`.
