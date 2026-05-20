# Phase 8 — Push the release commit to `main`

Goal: ensure `origin/main` contains the version bump + CHANGELOG commit. Otherwise
`main` lags behind the published release and the next person to release will be
confused (`git describe` will return a stale tag, version files will disagree with the
last released version, etc.).

Because the release branch was created off `main` HEAD (Phase 2) and only the release
commit was added on top (Phase 5), `release/v$NEXT_VERSION` is `main` plus one commit.
That makes Phase 8 a clean fast-forward — no cherry-pick, no merge commit, no conflict
risk.

## Step 8.1 — Switch back to main and fast-forward

```bash
git checkout main
git fetch origin --quiet
git pull --ff-only origin main          # nothing new should arrive; abort if it does
git merge --ff-only "release/v$NEXT_VERSION"
```

The merge must be `--ff-only`. If git refuses ("Not possible to fast-forward"), `main`
has new commits that landed during the build. Abort the merge and surface the divergent
log — the user resolves manually (typically: rebase the release commit onto the new
main, force-push the tag if absolutely necessary, or open a PR).

## Step 8.2 — Push main

```bash
git push origin main
```

The harness's auto-mode classifier may intercept this push because `main` is a shared
default branch. If it does, surface the denial verbatim — the user runs the push
themselves with `! git push origin main` in this session. Do not retry or try to work
around the denial.

## Step 8.3 — Verify origin/main has the release commit

After the push (whether the skill ran it or the user did):

```bash
git fetch origin --quiet
git log -1 --pretty=format:'%H %s' origin/main
```

Expect the SHA to match the release commit. If it does not, the push didn't land — stop
and surface the divergence; the user resolves before Phase 9.

This verification step is mandatory. Phase 9 (local branch deletion) cannot run until
`origin/main` is confirmed to carry the release commit, because the local
`release/v$NEXT_VERSION` branch would otherwise be the only ref holding the commit on a
non-tag location.

Proceed to Phase 9.
