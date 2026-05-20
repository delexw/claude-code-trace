# Phase 7 — Publish the GitHub release

Goal: confirm the release is published on the repo's main page.

**This phase is mostly automated.** `.github/workflows/release.yml` does it for us:

1. The tag push triggers a `notes` job that slices `CHANGELOG.md` for the version's
   section and stores it as a workflow output.
2. The three build jobs each pass `releaseBody: ${{ needs.notes.outputs.body }}` to
   `tauri-apps/tauri-action`, creating / updating the GitHub release as a draft with
   the CHANGELOG section as the body and the built artifacts attached.
3. A final `publish` job depends on all three builds and runs:

   ```bash
   gh release edit "$GITHUB_REF_NAME" --draft=false --latest
   ```

   That flips the draft to a published release on the main repo page and marks it as
   the latest.

So in the normal path you just need to **watch and verify**.

## Step 7.1 — Watch the run

```bash
gh run watch --exit-status \
  $(gh run list --workflow=release.yml --limit=1 --json databaseId --jq '.[0].databaseId')
```

`gh run watch` polls until the run finishes and exits non-zero on failure.

## Step 7.2 — Confirm the release is published

```bash
gh release view vX.Y.Z --json isDraft,isLatest,url
```

Expect `isDraft: false`, `isLatest: true`, plus a URL you can paste back to the user.

## Step 7.3 — Verify the body is what you expected

```bash
gh release view vX.Y.Z --json body --jq '.body' | head -40
```

Should be the same content as the new section in `CHANGELOG.md`. If it's the fallback
string `"Release vX.Y.Z. See the assets below to download the app."`, the awk slice
didn't match — usually because the CHANGELOG entry's heading doesn't follow the exact
`## [X.Y.Z] — YYYY-MM-DD` format. Fix the CHANGELOG, then edit the release body in place:

```bash
# Re-slice and update without re-tagging:
awk -v ver="X.Y.Z" '
  $0 ~ "^## \\[" ver "\\]" { inside=1; print; next }
  inside && /^## \[/ { exit }
  inside { print }
' CHANGELOG.md > /tmp/release-notes.md

gh release edit vX.Y.Z --notes-file /tmp/release-notes.md
```

## Manual fallback — if the workflow failed

If the build run failed or the `publish` job didn't run for any reason, do it manually:

```bash
gh run view <run-id> --log-failed       # investigate the failure first
```

If the artifacts built but the publish step didn't run, finish the publish yourself:

```bash
gh release edit vX.Y.Z \
  --notes-file <changelog-slice> \
  --draft=false \
  --latest
```

If the artifacts didn't build, fix the cause and push a new tag (e.g. `vX.Y.Z+1`) — do
not try to "rescue" a half-built release.

Proceed to Phase 8.
