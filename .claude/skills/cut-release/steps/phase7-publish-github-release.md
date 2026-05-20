# Phase 7 — Watch the pipeline and verify the release is public

Goal: confirm the release is published on the repo's releases page. Mostly automated —
`.github/workflows/release.yml` does the heavy lifting:

1. The tag push triggers a `notes` job that slices `CHANGELOG.md` for the version's
   section and exposes it as a workflow output.
2. A `guard` job checks that no published (non-draft) release exists for the tag yet —
   if one does it fails the entire workflow, preventing duplicate releases (see
   `release.yml` for the exact check).
3. Three build jobs (macOS / Linux / Windows) each pass `releaseBody:
${{ needs.notes.outputs.body }}` to `tauri-apps/tauri-action`, creating / updating
   the GitHub release as a draft with the CHANGELOG section as the body and the built
   artifacts attached.
4. A final `publish` job depends on all three builds and runs:

   ```bash
   gh release edit "$GITHUB_REF_NAME" --draft=false --latest
   ```

   That flips the draft public and marks it latest.

## Step 7.1 — Watch the run (foreground, blocking)

Run `gh run watch` **in the foreground** so the session blocks until the pipeline
finishes. Do not pass `run_in_background: true`, do not append `&`, do not detach. The
session must hold every phase end-to-end in one continuous flow — there are no
parallel-work opportunities here, and downstream phases (7.2 verification, 8
back-to-main, 9 cleanup) all depend on the run conclusion.

```bash
RUN_ID=$(gh run list --workflow=release.yml --limit=1 --json databaseId --jq '.[0].databaseId')
gh run watch --exit-status "$RUN_ID"
```

`gh run watch` polls until the run finishes and exits non-zero on failure. Builds take
roughly 15–25 minutes depending on cache hits. Set the Bash `timeout` parameter to
1800000 ms (30 minutes) to cover the full window without surprise timeouts.

Do not call `gh run view` mid-watch to peek at progress — `gh run watch` already prints
live status as jobs transition.

## Step 7.2 — Confirm the release is public

After the run completes:

```bash
gh release view "v$NEXT_VERSION" --json isDraft,isPrerelease,url,publishedAt,assets \
  --jq '{isDraft,isPrerelease,url,publishedAt,assets:[.assets[]|.name]}'
```

Expect `"isDraft": false`, `"isPrerelease": false`, a published timestamp, and 7 asset
filenames stamped with `$NEXT_VERSION` (macOS aarch64 dmg + app.tar.gz, Linux rpm /
AppImage / deb, Windows exe / msi).

## Step 7.3 — Verify the body matches the CHANGELOG

```bash
gh release view "v$NEXT_VERSION" --json body --jq '.body' | head -20
```

The first line should be `## [$NEXT_VERSION] — YYYY-MM-DD`. If it's the fallback string
"Release vX.Y.Z. See the assets below to download the app." the notes-job awk slice
didn't match — the CHANGELOG heading isn't in the exact `## [X.Y.Z] — YYYY-MM-DD`
format. Patch the CHANGELOG, re-slice locally, and edit in place:

```bash
awk -v ver="$NEXT_VERSION" '
  $0 ~ "^## \\[" ver "\\]" { inside=1; print; next }
  inside && /^## \[/ { exit }
  inside { print }
' CHANGELOG.md > /tmp/release-notes.md

gh release edit "v$NEXT_VERSION" --notes-file /tmp/release-notes.md
```

## Manual fallback — if the workflow failed

If any job conclusion is not `success`:

```bash
gh run view "$RUN_ID" --log-failed
```

Read the failure verbatim. Treat it as a real CI bug (per the global "CI failures are
real bugs" rule). If the artifacts built but the `publish` step didn't run, finish
manually:

```bash
gh release edit "v$NEXT_VERSION" \
  --notes-file <changelog-slice> \
  --draft=false \
  --latest
```

If the artifacts didn't build, fix the cause and cut a new tag (`vX.Y.Z+1`) — never
"rescue" a half-built release by force-pushing the tag.

Proceed to Phase 8.
