---
name: pickup
description: Rehydrate context when starting work. Reads handoff notes, checks repo state, CI/PR status, tmux sessions, and plans first 2-3 actions.
---

# Pickup

Purpose: rehydrate context quickly when you start work.

## When to use

- Starting a new session
- Taking over from another agent
- Resuming after interruption
- Morning startup routine

## Steps

1. **Read context**: AGENTS.md pointer + relevant docs + handoff notes if present
2. **Repo state**: `git status -sb`; check for local commits; confirm current branch/PR
3. **CI/PR/MR**: 
   - GitHub: `gh pr view <num> --comments --files`
   - GitLab: `glab mr view <num>`
   - Derive PR/MR from branch and note failing checks
4. **tmux/processes**: list sessions and attach if needed:
   - `tmux list-sessions`
   - If sessions exist: `tmux attach -t <session>` or `tmux capture-pane -p -J -t <session>:0.0 -S -200`
5. **Tests/checks**: note what last ran (from handoff notes/CI) and what you will run first
6. **Plan next 2–3 actions** as bullets and execute

## Output format

Concise bullet summary; include copy/paste tmux attach/capture commands when live sessions are present.

## Example

```markdown
## Pickup Summary

### Context
- Read AGENTS.md ✓
- Found handoff notes from Claude (2 hours ago)

### Repo State
- Branch: `feat/auth-flow`
- Status: 2 modified files, 1 untracked
- Ahead of main by 2 commits (not pushed)

### CI/PR (GitHub) or CI/MR (GitLab)
- PR/MR #42 (draft)
- Checks: pending (lint + test)

### tmux Sessions
```
codex-shell: 2 windows (created Wed Feb 18 10:23:05)
auth-dev: 1 window (created Wed Feb 18 11:15:22)
```
- `tmux attach -t auth-dev` (dev server running)

### Last Run (from handoff)
- ✅ Unit tests passed
- ⏳ E2E tests pending

### Next Actions
1. Attach to auth-dev tmux session
2. Complete registration form validation
3. Run E2E tests: `bunx playwright test`
4. Push commits and check CI
```

## Location

Create `~/.codex/prompts/pickup.md`, `~/.opencode/prompts/pickup.md`, etc. for global access.
