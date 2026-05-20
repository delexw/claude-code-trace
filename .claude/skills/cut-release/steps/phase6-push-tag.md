# Phase 6 — Push the tag (confirmation gate)

Goal: push the local tag to `origin` so the GitHub Actions release pipeline runs and
builds the macOS / Linux / Windows artifacts.

This is the first irreversible step. Stop and confirm with the user before pushing.

## Step 6.1 — Show the user what's about to happen

Present three things:

1. **Tag name and target commit**:

   ```bash
   git log -1 vX.Y.Z
   ```

2. **Commit range in the release** (so they can verify scope):

   ```bash
   git log --oneline "$LAST_TAG"..vX.Y.Z
   ```

3. **CHANGELOG entry for the new version** — read the section back to them.

## Step 6.2 — Ask for explicit approval

A clear yes/no question:

> "Push tag `vX.Y.Z` to `origin`? This triggers the GitHub Actions release pipeline,
> which builds the artifacts and creates a draft release. Cancelling later requires
> manually deleting the draft and the remote tag."

Wait for the user's response. Do not push without a clear "yes".

If they want changes (different CHANGELOG wording, different scope, different version),
loop back to the relevant earlier phase and rebuild. Local tags and branches make this
cheap; pushed tags do not.

## Step 6.3 — Push

On approval:

```bash
git push origin vX.Y.Z
```

The release workflow starts on GitHub within seconds. Confirm by:

```bash
gh run list --workflow=release.yml --limit=1
```

You should see a queued or in-progress run for the tag.

Proceed to Phase 7. (You can do Phase 7 in parallel with the build — but the publish
step requires the build to finish.)
