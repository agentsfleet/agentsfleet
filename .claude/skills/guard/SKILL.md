---
name: guard
version: 1.0.0
description: |
  Full safety mode: destructive command warnings + directory-scoped edits.
  Combines /careful (warns before rm -rf, DROP TABLE, force-push, etc.) with
  /freeze (blocks edits outside a specified directory). Use for maximum safety
  when touching prod or debugging live systems.
  Use when asked to "guard mode", "full safety", "lock it down", or "maximum safety".
allowed-tools:
  - Bash
  - Read
  - AskUserQuestion
hooks:
  PreToolUse:
    - matcher: "Bash"
      hooks:
        - type: command
          command: "bash ${CLAUDE_SKILL_DIR}/../careful/bin/check-careful.sh"
          statusMessage: "Checking for destructive commands..."
    - matcher: "Edit"
      hooks:
        - type: command
          command: "bash ${CLAUDE_SKILL_DIR}/../freeze/bin/check-freeze.sh"
          statusMessage: "Checking freeze boundary..."
    - matcher: "Write"
      hooks:
        - type: command
          command: "bash ${CLAUDE_SKILL_DIR}/../freeze/bin/check-freeze.sh"
          statusMessage: "Checking freeze boundary..."
---

# /guard — Full Safety Mode

Activates both `/careful` and `/freeze` simultaneously.

**Instruction enforcement (all agents):**
1. Before any bash command — check against the `/careful` destructive pattern list. If matched, stop and warn before running.
2. Before any Edit or Write — check the file path is within the frozen directory. If outside, refuse.

## Setup

Ask the user which directory to restrict edits to:

```
/guard src/http
```

Set the freeze boundary:

```bash
STATE_DIR="${CLAUDE_PLUGIN_DATA:-$HOME/.gstack}"
mkdir -p "$STATE_DIR"
echo "<frozen-directory>/" > "$STATE_DIR/freeze-dir.txt"
echo "Guard active: destructive command warnings ON, edits restricted to <frozen-directory>/"
```

Tell the user: "Full safety mode active. Destructive commands require confirmation. Edits are restricted to `<dir>/`. Run `/unfreeze` to remove the directory restriction."

## Enforcement

Apply both rule sets from `/careful` and `/freeze` on every action. Neither can be bypassed without explicit user confirmation.

## Reset

Run `/unfreeze` to clear the directory restriction. Careful mode persists until the session ends.
