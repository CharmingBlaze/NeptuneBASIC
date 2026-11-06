#!/usr/bin/env bash
# PostToolUse hook: after Claude edits or writes a .rs file, run `cargo check`
# and, if it fails, send the compiler errors back to Claude (exit code 2).
# Output is trimmed to the error lines to keep token use low.

input=$(cat)
case "$input" in
  *'.rs"'*) ;;
  *) exit 0 ;;
esac

cd "${CLAUDE_PROJECT_DIR:-.}" || exit 0

if out=$(cargo check --workspace --all-targets --message-format short --color never -q 2>&1); then
  exit 0
fi

{
  echo "cargo check failed after this edit:"
  printf '%s\n' "$out" | grep -E '(^|: )error' | head -30
} >&2
exit 2
