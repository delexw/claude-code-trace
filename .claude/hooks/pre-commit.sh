#!/bin/bash
# Pre-commit hook: check format, lint, and tests; block commit if issues found.
# Claude Code will receive the block reason and must fix before retrying.

set -uo pipefail

HOOK_INPUT=$(cat)
SESSION_ID=$(printf '%s' "$HOOK_INPUT" | jq -r '.session_id // ""')
FLAG_FILE="/tmp/claude-tests-confirmed${SESSION_ID:+-$SESSION_ID}"

cd "$CLAUDE_PROJECT_DIR"

STAGED=$(git diff --cached --name-only)

if [ -z "$STAGED" ]; then
  exit 0
fi

ERRORS=""

# --- Format check ---
FMT_OUTPUT=$(npm run fmt:check 2>&1) || {
  ERRORS="Format issues found. Run: npm run fmt, do not ask user for options, fix and stage the changes, then retry the commit.\n\n$FMT_OUTPUT"
}

# --- Lint check ---
LINT_OUTPUT=$(npm run lint 2>&1)
LINT_EXIT=$?
if [ $LINT_EXIT -ne 0 ] || echo "$LINT_OUTPUT" | grep -qE "[1-9][0-9]* warnings? "; then
  if [ -n "$ERRORS" ]; then
    ERRORS="$ERRORS\n\nLint issues:\n$LINT_OUTPUT"
  else
    ERRORS="Lint issues found. Fix each issue properly at the root cause — do NOT add eslint-disable comments or suppress rules. ⚠️  STAGE THE FIXED FILES in a SEPARATE Bash tool call: git add <files>\nThen retry the commit in another Bash tool call.$LINT_OUTPUT"
  fi
fi

if [ -n "$ERRORS" ]; then
  printf '{"decision": "block", "reason": %s}' "$(printf '%s' "$ERRORS" | jq -Rs .)"
  exit 0
fi

# --- Test check ---
TEST_OUTPUT=$(npm run test 2>&1)
TEST_EXIT=$?
if [ $TEST_EXIT -ne 0 ]; then
  REASON="Tests are failing. Fix the tests properly — do NOT skip or disable them.\n⚠️  STAGE THE FIXED FILES in a SEPARATE Bash tool call: git add <files>\nThen retry the commit in another Bash tool call.$TEST_OUTPUT"
  printf '{"decision": "block", "reason": %s}' "$(printf '%s' "$REASON" | jq -Rs .)"
  exit 0
fi

# --- Self-reflection gate ---
# Flag must be set explicitly by Claude (separate Bash call) to confirm tests were written.
# The hook never sets the flag itself — only Claude can, after consciously answering "yes".
if [ -f "$FLAG_FILE" ]; then
  rm -f "$FLAG_FILE"
  exit 0
fi

REFLECTION="All checks pass. Did you write or update tests for the behaviour you just changed?\n\n  If not → write the tests then in a SEPARATE Bash tool call: git add <files>, then retry.\n  If yes → run this in a SEPARATE Bash tool call, then retry the commit in another:\n\n    touch $FLAG_FILE"
printf '{"decision": "block", "reason": %s}' "$(printf '%s' "$REFLECTION" | jq -Rs .)"
