#!/bin/bash
# Pre-commit hook: format + lint + tests + type-check run in parallel on staged files only.
# Covers all three languages in this repo — JS/TS (oxfmt/oxlint/vitest/tsc), Rust
# (cargo fmt/clippy/test), and Python (ruff). All jobs launch concurrently; results are
# collected after every job completes. Blocks the commit with a combined reason if any fail.

set -uo pipefail

cd "$CLAUDE_PROJECT_DIR"

# Claude Code passes BaseHookInput JSON on stdin; extract the command + session_id from it.
INPUT=$(cat)

# Only run for git commit commands.
COMMAND=$(printf '%s' "$INPUT" | jq -r '.tool_input.command // ""')
[[ "$COMMAND" != *"git commit"* ]] && exit 0

SESSION_ID=$(printf '%s' "$INPUT" | jq -r '.session_id // empty' 2>/dev/null || true)
# One flag covers BOTH confirmations (tests + specs) — merged from the former pre-commit-spec.sh.
# Kept INSIDE the workspace (not /tmp): an autonomous run commits from a git worktree whose
# write guard denies the agent any write outside that worktree, so a /tmp flag could never be
# touched. A worktree-relative flag passes the guard; it is gitignored so it never stages.
FLAG_FILE="${CLAUDE_PROJECT_DIR}/.precommit-confirmed-${SESSION_ID}"

# Get staged files — nothing to check if tree is clean.
STAGED_FILES=$(git diff --cached --name-only 2>/dev/null || true)
[ -z "$STAGED_FILES" ] && exit 0

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

# --- Partially-staged detection (fast, run inline) ---
# Warn when a staged file also has unstaged working-tree changes. This catches drift caused by
# running a formatter or lint fix AFTER staging — the staged version would be committed without
# the subsequent working-tree edits.
PARTIALLY_STAGED=$(git diff --name-only 2>/dev/null | while IFS= read -r f; do
  printf '%s\n' "$STAGED_FILES" | grep -qxF "$f" && printf '%s\n' "$f"
done || true)

# --- Launch all checks in parallel, scoped to the file types that are staged ---

# JS/TS format
FMT_FILES=$(printf '%s\n' "$STAGED_FILES" | grep -E '\.(js|ts|jsx|tsx|mjs|cjs|json|jsonc|css|scss|less|html|md|yaml|yml)$' || true)
FMT_PID=""
if [ -n "$FMT_FILES" ]; then
  (printf '%s\n' "$FMT_FILES" | xargs npx oxfmt --check --no-error-on-unmatched-pattern >"$TMP/fmt.out" 2>&1) &
  FMT_PID=$!
fi

# JS/TS lint — exclude the .claude/ skills symlink path so its targets aren't linted here.
LINT_FILES=$(printf '%s\n' "$STAGED_FILES" | grep -E '\.(js|ts|jsx|tsx|mjs|cjs)$' | grep -v '^\.claude/' || true)
LINT_PID=""
if [ -n "$LINT_FILES" ]; then
  (printf '%s\n' "$LINT_FILES" | xargs npx oxlint >"$TMP/lint.out" 2>&1) &
  LINT_PID=$!
fi

# JS/TS tests + type-check — exclude the .claude/ symlink path.
STAGED_TS=$(printf '%s\n' "$STAGED_FILES" | grep -E '\.(ts|tsx)$' | grep -v '^\.claude/' || true)
TEST_PID=""
TSC_PID=""
if [ -n "$STAGED_TS" ]; then
  (printf '%s\n' "$STAGED_TS" | sed "s|^|$CLAUDE_PROJECT_DIR/|" | xargs npx vitest related --run >"$TMP/test.out" 2>&1) &
  TEST_PID=$!
  (npx tsc --noEmit >"$TMP/tsc.out" 2>&1) &
  TSC_PID=$!
fi

# Rust — cargo operates crate-wide, so run only when a Rust source or manifest is staged.
# fmt/clippy/test share the target/ lock, so run them sequentially in one job to avoid contention.
RUST_STAGED=$(printf '%s\n' "$STAGED_FILES" | grep -E '^src-tauri/.*\.rs$|^src-tauri/Cargo\.(toml|lock)$' || true)
RUST_PID=""
if [ -n "$RUST_STAGED" ]; then
  (
    cargo fmt --manifest-path src-tauri/Cargo.toml --check &&
      cargo clippy --manifest-path src-tauri/Cargo.toml &&
      cargo test --manifest-path src-tauri/Cargo.toml
  ) >"$TMP/rust.out" 2>&1 &
  RUST_PID=$!
fi

# Python — ruff operates on tui-py/, so run only when a tui-py Python file is staged.
PY_STAGED=$(printf '%s\n' "$STAGED_FILES" | grep -E '^tui-py/.*\.py$' || true)
PY_PID=""
if [ -n "$PY_STAGED" ]; then
  (ruff format --check tui-py/ && ruff check tui-py/) >"$TMP/py.out" 2>&1 &
  PY_PID=$!
fi

# --- Wait for all jobs ---
FMT_EXIT=0
[ -n "$FMT_PID" ] && { wait "$FMT_PID" || FMT_EXIT=$?; }
LINT_EXIT=0
[ -n "$LINT_PID" ] && { wait "$LINT_PID" || LINT_EXIT=$?; }
TEST_EXIT=0
[ -n "$TEST_PID" ] && { wait "$TEST_PID" || TEST_EXIT=$?; }
TSC_EXIT=0
[ -n "$TSC_PID" ] && { wait "$TSC_PID" || TSC_EXIT=$?; }
RUST_EXIT=0
[ -n "$RUST_PID" ] && { wait "$RUST_PID" || RUST_EXIT=$?; }
PY_EXIT=0
[ -n "$PY_PID" ] && { wait "$PY_PID" || PY_EXIT=$?; }

# --- Collect errors ---
ERRORS=""
append_error() {
  [ -n "$ERRORS" ] && ERRORS="$ERRORS

"
  ERRORS="${ERRORS}$1"
}

if [ $FMT_EXIT -ne 0 ]; then
  append_error "Format issues found. Run: npm run fmt
⚠️  Run: git diff --name-only to see which files the fix changed, then stage ONLY those files in a SEPARATE Bash tool call: git add <only the files changed by the fix above — NOT other unrelated unstaged files>
Then retry the commit in another Bash tool call.

$(cat "$TMP/fmt.out")"
fi

if [ -n "$LINT_PID" ]; then
  LINT_OUTPUT=$(cat "$TMP/lint.out")
  if [ $LINT_EXIT -ne 0 ] || printf '%s' "$LINT_OUTPUT" | grep -qE "[1-9][0-9]* warnings? "; then
    append_error "Lint issues found. Fix each issue at the root cause — do NOT add eslint-disable comments.
⚠️  Run: git diff --name-only to see which files the fix changed, then stage ONLY those files in a SEPARATE Bash tool call: git add <only the files changed by the fix above — NOT other unrelated unstaged files>
Then retry the commit in another Bash tool call.

$LINT_OUTPUT"
  fi
fi

if [ $TEST_EXIT -ne 0 ]; then
  append_error "Tests failed. Fix before committing.

$(cat "$TMP/test.out")"
fi

if [ $TSC_EXIT -ne 0 ]; then
  append_error "TypeScript errors found. Fix before committing.

$(cat "$TMP/tsc.out")"
fi

if [ $RUST_EXIT -ne 0 ]; then
  append_error "Rust checks failed (cargo fmt --check / clippy / test). Fix at the root cause — do NOT add #[allow(...)] to silence clippy.
⚠️  Run: git diff --name-only to see which files the fix changed, then stage ONLY those files in a SEPARATE Bash tool call.
Then retry the commit in another Bash tool call.

$(cat "$TMP/rust.out")"
fi

if [ $PY_EXIT -ne 0 ]; then
  append_error "Python checks failed (ruff format --check / ruff check). Fix at the root cause — do NOT add noqa comments unless genuinely necessary.
⚠️  Run: git diff --name-only to see which files the fix changed, then stage ONLY those files in a SEPARATE Bash tool call.
Then retry the commit in another Bash tool call.

$(cat "$TMP/py.out")"
fi

if [ -n "$PARTIALLY_STAGED" ]; then
  append_error "⚠️  These staged files also have unstaged working-tree changes — the committed version will be MISSING those changes. Stage them too or discard them:

$PARTIALLY_STAGED"
fi

if [ -n "$ERRORS" ]; then
  printf '{"decision": "block", "reason": %s}' "$(printf '%s' "$ERRORS" | jq -Rs .)"
  exit 0
fi

# --- Combined reminder: confirm tests AND specs before the commit (one flag for both) ---
if [ -n "$SESSION_ID" ] && [ -f "$FLAG_FILE" ]; then
  rm -f "$FLAG_FILE"
  exit 0
fi

REFLECTION=$(printf '%s' "All checks pass. Two confirmations before this commit:

  1. TESTS — did you write or update tests for the behaviour you just changed?
  2. SPECS — deep-read specs/ and update any spec this change makes inaccurate (open the files,
     verify against the new code; add a new specs/NN-<topic>.md if a changed area has none).

  If EITHER still needs work → do it, git add the files, then git commit again — the hook re-asks.
  If BOTH are satisfied → run the touch command below in a SEPARATE Bash tool call, then retry the
  commit in another:

    touch $FLAG_FILE

  NEVER do '<command> && touch file' in a single tool call — the hook consumes the flag on first pass.
  NEVER touch the flag file unless both confirmations above are genuinely done.
  If you modified any files since the last git commit, run git commit again first — the hook re-asks.")

printf '{"decision": "block", "reason": %s}' "$(printf '%s' "$REFLECTION" | jq -Rs .)"
exit 0
