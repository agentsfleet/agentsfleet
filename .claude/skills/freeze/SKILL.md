---
name: freeze
version: 1.0.0
description: |
  Restrict file edits to a specific directory for the session. Before editing
  any file, check that the path is within the frozen boundary. If not, refuse
  and explain. Works via instruction enforcement on all agents (Claude Code,
  Codex, OpenCode, Amp). Claude Code also gets a PreToolUse hook.
  Use when asked to "freeze", "restrict edits", "only edit this folder",
  or "lock down edits". Also activated automatically by /investigate.
allowed-tools:
  - Bash
  - Read
  - AskUserQuestion
hooks:
  PreToolUse:
    - matcher: "Edit"
      hooks:
        - type: command
          command: "bash ${CLAUDE_SKILL_DIR}/bin/check-freeze.sh"
          statusMessage: "Checking freeze boundary..."
    - matcher: "Write"
      hooks:
        - type: command
          command: "bash ${CLAUDE_SKILL_DIR}/bin/check-freeze.sh"
          statusMessage: "Checking freeze boundary..."
---

# /freeze — Restrict Edits to a Directory

**Instruction enforcement (all agents):** Before editing or writing any file,
check that its path is within the frozen directory. If it is outside, refuse
and explain. Do not rely on a hook — enforce this manually.

## Setup

Ask the user which directory to restrict edits to, or accept a path as argument:

```
/freeze src/pipeline
```

Store the frozen path for the session:

```bash
STATE_DIR="${CLAUDE_PLUGIN_DATA:-$HOME/.gstack}"
mkdir -p "$STATE_DIR"
echo "<frozen-directory>/" > "$STATE_DIR/freeze-dir.txt"
echo "Edits restricted to: <frozen-directory>/"
```

Tell the user: "Edits restricted to `<dir>/`. I will refuse edits outside this boundary. Run `/unfreeze` to remove the restriction."

## Enforcement

Before every Edit or Write, check:
1. Is the target file path inside the frozen directory?
2. If yes — proceed.
3. If no — refuse:

```
🚫 [freeze] Blocked: <file> is outside the freeze boundary (<frozen-dir>).
Only edits within the frozen directory are allowed.
Run /unfreeze to remove this restriction.
```

## Reset

Run `/unfreeze` to clear the boundary and allow edits anywhere.
