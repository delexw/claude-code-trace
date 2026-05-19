#!/bin/bash
# Pre-commit hook: spec reminder.
# Always blocks the first commit attempt and tells the agent to reconsider
# whether specs/ needs updating. The touch-flag escape is the only way out,
# so the agent must consciously confirm "specs are aligned" each commit.
# Without the flag the hook would loop forever.

set -uo pipefail

HOOK_INPUT=$(cat)
SESSION_ID=$(printf '%s' "$HOOK_INPUT" | jq -r '.session_id // ""')
FLAG_FILE="/tmp/claude-spec-confirmed${SESSION_ID:+-$SESSION_ID}"

# Acknowledged → consume flag and pass.
if [ -f "$FLAG_FILE" ]; then
  rm -f "$FLAG_FILE"
  exit 0
fi

REASON="Before committing: deep-read specs/ and reconsider whether any spec needs updating for this change.\n\n"
REASON+="  • If a spec is now WRONG → update it, then git add specs/<file>, then retry.\n"
REASON+="  • If a changed area has NO spec → add a new specs/NN-<topic>.md or extend an existing spec, then git add specs/<file>, then retry.\n"
REASON+="  • If specs are still accurate AND coverage is intentional → in a SEPARATE Bash tool call run:\n      touch $FLAG_FILE\n    then retry the commit.\n"
REASON+="\nDeep-read means open the relevant spec files and verify against the new code — do not skim or trust the diff alone."

printf '{"decision": "block", "reason": %s}' "$(printf '%s' "$REASON" | jq -Rs .)"
