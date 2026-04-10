# AGENTS Policy Appendix

## Hard Safety Rules

- Never use destructive commands without explicit user approval: `reset --hard`, `clean -fd`, `checkout --`, `restore --source`, broad `rm`.
- Never revert changes you did not create unless explicitly instructed.
- If unexpected changes appear in files you are actively editing, stop and ask.
- No branch mutation outside lifecycle transitions.
- No cross-worktree edits.
- No secrets in commits/docs.
- Never resolve or print credential values in conversation, code, docs, playbooks, or evidence files.
- When writing verification steps that reference credentials, always use `op read 'op://...'` at runtime.
- Prefer CLI and text artifacts.

## Cognitive Discipline

### Confusion Management (Critical)

When encountering inconsistencies, conflicting requirements, or unclear specifications:

1. STOP.
2. Name the specific confusion.
3. Present the tradeoff or ask one precise question.
4. Wait for resolution.

### Simplicity Enforcement

- Can this be done in fewer lines?
- Are these abstractions earning their complexity?
- Would a senior dev say "why didn't you just..."?

Prefer the obvious solution. Avoid unnecessary complexity.

### No Insecure Fallbacks

- One auth path.
- No deferred security.
- No throwaway code.
- No backward-compatibility shims for unreleased software.

### No Process Launches — Native SDK Only

- Use native SDKs for core functionality.
- Exception: personal developer tools (`op`, `gh`, `glab`, `oracle`).

### Error Surfacing — Design for Autonomous Recovery

- No silent hangs.
- Errors must name the dependency and suggest the fix.
- Build and CI errors must be reproducible locally.
- Prefer closed local feedback loops.

## Memory Boundaries

- Process decisions belong in repo docs.
- Runbooks belong in `runbooks/docs/*.md`.
- Do not rely on chat context when files can hold canonical state.

## Git Forge Policy (`gh` vs `glab`)

Detect forge from remote host:

| Remote host contains | Forge tool |
|---|---|
| `gitlab.com` | `glab` |
| `github.com` | `gh` |

Quick checks:

```bash
git remote -v
gh auth status
glab auth status
```

## PR And CI Workflow

GitHub:

```bash
gh pr view --json number,title,url
gh pr diff
gh run list
gh run view <run-id>
```

GitLab:

```bash
glab mr view
glab mr diff
glab ci status
glab pipeline view
```

If CI is red, inspect logs, fix, push, and re-check.

## Standard Make Target Taxonomy

Required targets:

- `make dev`
- `make up`
- `make down`
- `make lint`
- `make test`
- `make build`
- `make _clean`
- `make push`
- `make qa`
- `make qa-smoke`

Rules:

- `make quality` is banned.
- `make test` is unit only.
- E2E should be `make qa` / `make qa-smoke`.

## Screenshot Workflow

- Pick newest PNG from `~/Desktop` or `~/Downloads`.
- Validate dimensions: `sips -g pixelWidth -g pixelHeight <file>`.
- Optimize before commit: `imageoptim <file>`.

## Multi-Agent Execution Model

- One worktree per active stream.
- One tmux pane per role.
- No file edits outside current worktree.
- Merge only after `VERIFY`.

Session commands:

```bash
tmux new -s agents
tmux list-sessions
tmux attach -t agents
```

## QA Testing Decision

Default browser E2E is Playwright CLI:

```bash
bun add -d @playwright/test
bunx playwright install --with-deps
bunx playwright test --reporter=line
bunx playwright test tests/e2e/login.spec.ts --project=chromium
```

## Knowledge Base (QMD)

Collection: `clawable` in `~/notes/clawable/`.

```bash
qmd search "actor model implementation" -c clawable
qmd vsearch "sandbox isolation patterns" -c clawable
qmd query "how to deploy sandbox agents" -c clawable
qmd query "sandbox architecture" --json -n 10
```

## Greptile Learnings

Primary rules file: `docs/greptile-learnings/RULES.md`.

Read it at EXECUTE start and before review/fix cycles.

RULES.md contains generic principles, not per-incident entries. Most findings are instances
of existing rules (unused code, missing parser, double-free, etc.).

Post-PR greptile fix workflow:

1. Fetch review ID/comments.
2. Fix findings.
3. Run verification (`make lint`, `make test`, and DB integration when needed).
4. For each P0/P1 finding: check if an existing rule in RULES.md covers it.
   If yes — append the incident reference. If no — add a new generic principle.
5. Reply to greptile threads with fix commit SHA.
6. Commit and push.
7. Report findings/fixes/rules/reply IDs.

## Web-to-Markdown Workflow

| Approach | Use When | Command |
|---|---|---|
| Cloudflare header | Site supports it | `curl -H "Accept: text/markdown" URL` |
| html2text | Fallback | `curl -s URL \| html2text` |
| webfetch | quick extraction | `webfetch URL --format markdown` |

## Code Structure Policies

- Keep files under 400 lines.
- Extract repeated strings to constants.
- Shared constants go in shared files.
- Avoid unnecessary constants.
- Use typed enums with serialization methods (toSlice/fromSlice) instead of DB CHECK constraints for status/type values.
- Never create plaintext credential/secret tables — reuse vault.secrets with crypto_store.
- When extracting tests to a new file, verify test discovery by adding import to main.zig.
- Single source of truth for templates/configs — if two copies exist, one will drift.

## Skill Routing

Use dedicated skills when the request matches:

- ideas/brainstorming -> office-hours
- bugs/errors -> investigate
- ship/deploy/PR -> ship
- QA/testing -> qa
- code review -> review
- post-ship docs -> document-release
- retro -> retro
- design system -> design-consultation
- visual polish -> design-review
- architecture review -> plan-eng-review
- checkpoint/resume -> checkpoint
- health check -> health
