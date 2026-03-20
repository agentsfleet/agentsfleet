#!/usr/bin/env bash
# check-freeze.sh — PreToolUse hook for /freeze skill (Claude Code only)
set -euo pipefail

INPUT=$(cat)
STATE_DIR="${CLAUDE_PLUGIN_DATA:-$HOME/.gstack}"
FREEZE_FILE="$STATE_DIR/freeze-dir.txt"

[ ! -f "$FREEZE_FILE" ] && echo '{}' && exit 0

FREEZE_DIR=$(tr -d '[:space:]' < "$FREEZE_FILE")
[ -z "$FREEZE_DIR" ] && echo '{}' && exit 0

FILE_PATH=$(printf '%s' "$INPUT" | grep -o '"file_path"[[:space:]]*:[[:space:]]*"[^"]*"' | head -1 | sed 's/.*:[[:space:]]*"//;s/"$//' || true)
if [ -z "$FILE_PATH" ]; then
  FILE_PATH=$(printf '%s' "$INPUT" | python3 -c 'import sys,json; print(json.loads(sys.stdin.read()).get("tool_input",{}).get("file_path",""))' 2>/dev/null || true)
fi
[ -z "$FILE_PATH" ] && echo '{}' && exit 0

case "$FILE_PATH" in /*) ;; *) FILE_PATH="$(pwd)/$FILE_PATH" ;; esac
FILE_PATH=$(printf '%s' "$FILE_PATH" | sed 's|/\+|/|g;s|/$||')

case "$FILE_PATH" in
  "${FREEZE_DIR}"*)
    echo '{}'
    ;;
  *)
    printf '{"permissionDecision":"deny","message":"[freeze] Blocked: %s is outside the freeze boundary (%s). Run /unfreeze to remove the restriction."}\n' "$FILE_PATH" "$FREEZE_DIR"
    ;;
esac
