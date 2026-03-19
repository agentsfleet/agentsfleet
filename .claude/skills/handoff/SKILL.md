---
name: handoff
description: Package current work state for the next agent (or future you). Documents scope, git status, PR/CI info, running processes, tests, next steps, and risks.
---

# Handoff

Purpose: package the current state so the next agent (or future you) can resume quickly.

## When to use

- Finishing work for the day
- Switching between agents (Claude → Codex → OpenCode)
- Long-running tasks that need to pause/resume
- Context switching between sessions

## Include (in order)

1. **Scope/status**: what you were doing, what's done, what's pending, and any blockers
2. **Working tree**: `git status -sb` summary and whether there are local commits not pushed
3. **Branch/PR/MR**: current branch, relevant PR/MR number/URL, CI status if known
   - Use appropriate terminology based on forge: GitHub → PR, GitLab → MR
4. **Running processes**: list tmux sessions/panes and how to attach:
   - Example: `tmux attach -t codex-shell` or `tmux capture-pane -p -J -t codex-shell:0.0 -S -200`
   - Note dev servers, tests, debuggers, background scripts
5. **Tests/checks**: which commands were run, results, and what still needs to run
6. **Next steps**: ordered bullets the next agent should do first
7. **Risks/gotchas**: any flaky tests, credentials, feature flags, or brittle areas

## Output format

Concise bullet list; include copy/paste tmux commands for any live sessions.

## Example

```markdown
## Handoff

### Scope/Status
Implementing user authentication flow. Login form complete, registration in progress.
- ✅ Login form UI + validation
- 🔄 Registration form (50% done)
- ⏳ Email verification (not started)
- Blocker: waiting on email service credentials

### Working Tree
```
M src/auth/login.tsx
A src/auth/register.tsx
?? docs/auth-flow.md
```
- 2 commits ahead of main (not pushed)

### Branch/PR (GitHub) or Branch/MR (GitLab)
- Branch: `feat/auth-flow`
- PR/MR: #42 (draft)
- CI: pending

### Running Processes
- `tmux attach -t auth-dev` (dev server on :3000)
- `tmux attach -t auth-test` (test watcher)

### Tests/Checks
- ✅ `npm run lint` - passed
- ✅ `npm run test:unit` - 45/45 passed
- ⏳ E2E tests - not run yet

### Next Steps
1. Complete registration form validation
2. Add email verification endpoint
3. Run full E2E test suite
4. Mark PR ready for review

### Risks/Gotchas
- Email service has rate limits (100/day)
- Registration endpoint not yet deployed to staging
```

## Location

Create `~/.codex/prompts/handoff.md`, `~/.opencode/prompts/handoff.md`, etc. for global access.
