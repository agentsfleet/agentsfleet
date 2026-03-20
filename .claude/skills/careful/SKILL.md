---
name: careful
version: 1.0.0
description: |
  Safety guardrails for destructive commands. Before running any bash command,
  check it against the protected pattern list. If it matches, stop and warn the
  user before proceeding. Works via instruction enforcement on all agents
  (Claude Code, Codex, OpenCode, Amp). Claude Code also gets a PreToolUse hook
  as a belt-and-suspenders layer.
  Use when asked to "be careful", "safety mode", "prod mode", or "careful mode".
allowed-tools:
  - Bash
  - Read
hooks:
  PreToolUse:
    - matcher: "Bash"
      hooks:
        - type: command
          command: "bash ${CLAUDE_SKILL_DIR}/bin/check-careful.sh"
          statusMessage: "Checking for destructive commands..."
---

# /careful — Destructive Command Guardrails

Safety mode is **active**.

**Before running any bash command**, scan it against the patterns below.
If it matches, **do not run it**. Output the warning format and wait for
explicit user confirmation. This applies to all agents — do not rely on a hook.

## Protected patterns

| Pattern | Example | Risk |
|---------|---------|------|
| `rm -rf` / `rm -r` / `rm --recursive` | `rm -rf /var/data` | Recursive delete |
| `DROP TABLE` / `DROP DATABASE` | `DROP TABLE users;` | Data loss |
| `TRUNCATE` | `TRUNCATE orders;` | Data loss |
| `git push --force` / `-f` | `git push -f origin main` | History rewrite |
| `git reset --hard` | `git reset --hard HEAD~3` | Uncommitted work loss |
| `git checkout .` / `git restore .` | `git checkout .` | Uncommitted work loss |
| `kubectl delete` | `kubectl delete pod` | Production impact |
| `docker rm -f` / `docker system prune` | `docker system prune -a` | Container/image loss |

## Warning format

```
⚠️  [careful] Destructive command detected: <pattern matched>
Command: <command>
Risk: <risk from table>

Proceed? Confirm with "yes, run it" or describe a safer alternative.
```

Wait for explicit confirmation before running.

## Safe exceptions — allow without warning

`rm -rf` targeting known build artifacts only:
`node_modules`, `.next`, `dist`, `__pycache__`, `.cache`, `build`, `.turbo`,
`coverage`, `zig-out`, `.tmp`, `zig-cache`

## Combined mode

Use `/guard` to add directory-scoped edit restrictions on top of careful mode.
