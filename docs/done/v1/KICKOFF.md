# KICKOFF — Build UseZombie M1

Date: Mar 2, 2026
Status: M1 COMPLETE — all three milestones filed to docs/done/v1/ ✅

---

## The Mission

One Zig binary. Takes a spec. Ships a validated PR. Three things in the stack: zombied binary, Postgres, git.

## Read First

1. `docs/ARCHITECTURE.md` — system contract
2. `docs/spec/v1/M1_000_CONTROL_PLANE_NULLCLAW_BASELINE.md` — what to build
3. `docs/spec/v1/M1_002_API_AND_EVENTS_CONTRACTS.md` — API contracts
4. `docs/spec/v1/M1_003_OBSERVABILITY_AND_POLICY.md` — observability + policy

## NullClaw Dependency

- Repository: `https://github.com/nullclaw/nullclaw.git`
- Version: `v2026.3.1`
- Zig minimum: `0.15.2`
- Integration: `@import("nullclaw")` — native library, not subprocess
- Key APIs:
  - `nullclaw.Config.load()` — loads JSON config from disk
  - `nullclaw.agent.Agent` — agent struct with `.runSingle(message)` for single-turn execution
  - `nullclaw.agent.Agent.tokensUsed()` — token metering
  - `nullclaw.agent.Agent.clearHistory()` — session reset between agents
  - Config fields: `providers`, `default_provider`, `default_model`, `autonomy`, `security.sandbox`, `tools`, `memory`, `agents`
  - Sandbox backends: `auto`, `landlock`, `bubblewrap`, `docker`, `none`
  - Built-in tools: `file_read`, `file_write`, `file_edit`, `shell`, `grep`, `git`, `web_fetch`, `web_search`, `memory_store`, `memory_recall`, `memory_list`, `memory_forget`

## The Prompt

```text
You are a senior implementation engineer building UseZombie — an agent delivery control plane.

Read these docs in order:

1) docs/ARCHITECTURE.md
   - One Zig binary: @import("nullclaw") for agent runtime, Zap for HTTP, state machine in-process
   - 3 things: zombied binary, Postgres, git
   - Agents: Echo (planner) → Scout (builder) → Warden (validator) — native NullClaw Agent calls
   - State machine: SPEC_QUEUED → ... → DONE | BLOCKED
   - Sandbox: NullClaw Landlock/Bubblewrap (OS-level, no containers)
   - Handoff: git branch. Commit + push between stages. Works local. Works distributed.
   - Memory: Postgres (cross-run) + NullClaw memory module (per-run)

2) docs/spec/v1/M1_000_CONTROL_PLANE_NULLCLAW_BASELINE.md
   - Zig binary with @import("nullclaw"), Zap HTTP server, 6 endpoints
   - Native agent calls — typed results, Zig errors, zero serialization
   - Acceptance criteria and build order

3) docs/spec/v1/M1_002_API_AND_EVENTS_CONTRACTS.md
   - 6 operations: start_run, get_run, retry_run, pause_workspace, list_specs, sync_specs
   - Error contract, artifact contract, event envelope

4) docs/spec/v1/M1_003_OBSERVABILITY_AND_POLICY.md
   - Event taxonomy, SLOs, policy guardrails (safe/sensitive/critical)

5) playbooks/M2_002_PRIMING_INFRA.md + playbooks/M3_001_DEPLOY_DEV.md + playbooks/M3_002_DEPLOY_PROD.md
   - Stack, env vars, service keys

Task:
Build UseZombie M1. One Zig binary that processes specs into validated PRs.

Build steps:
1. SCAFFOLD — build.zig, build.zig.zon (nullclaw v2026.3.1 dep), project structure
2. NULLCLAW — @import("nullclaw"), load echo/scout/warden configs, verify Agent.runSingle()
3. HTTP API — Zap server, 6 endpoints matching M1_002
4. STATE MACHINE — transitions, idempotency, policy checks in Postgres
5. AGENT PIPELINE — Echo → Scout → Warden via NullClaw Agent.runSingle() (native calls)
6. GIT OPS — bare clone + worktree, commit + push per stage, PR via GitHub API
7. MEMORY — workspace_memories: inject into Echo, capture from Warden
8. SECRETS — AES-256-GCM in Postgres, NullClaw security module
9. PUBLIC ASSETS — regenerate public/llms.txt, public/agent-manifest.json, public/skill.md, public/openapi.json (current files are STALE placeholders and must be updated to match the live API)
10. E2E — PENDING_*.md → Echo plans → Scout builds → Warden validates → PR opens
11. VERIFY — acceptance criteria

Hard constraints:
- One Zig binary. @import("nullclaw"). Native calls, not subprocess.
- Zap HTTP server. 6 endpoints. Same contracts as M1_002.
- Git branch is the handoff. Commit + push between stages.
- State machine in Postgres. Append-only run_transitions.
- NullClaw Landlock/Bubblewrap is the sandbox. No containers.
- BYOK: LLM key via NullClaw security module.
- Sequential: Echo → Scout → Warden.
- Tiered validation: T1/T2 block. T3/T4 pass with notes.
- Spec status in DB. No file renaming.

Tech stack:
- Language: Zig (0.15.2+)
- Agent runtime: NullClaw v2026.3.1 (@import, native)
- HTTP: Zap
- Database: Postgres
- Run queue: Postgres (SELECT FOR UPDATE SKIP LOCKED)
- Git: git CLI or libgit2
- Code host: GitHub REST API
- Sandbox: Landlock/Bubblewrap (NullClaw built-in)

NOT in the stack:
- TypeScript, Bun, Node, npm
- Docker, Kubernetes, Daytona, Rivet, Temporal, Firecracker, Nomad
- Subprocess spawning, stdout parsing
- OpenClaw, PI SDK
- gRPC, containers

Acceptance test:
- Place PENDING_001_setup_database.md in a test repo
- POST /v1/runs → Echo plans → Scout implements → Warden validates → PR opens
- run_transitions has full audit trail
- Artifacts in feature branch
- usage_ledger has agent-seconds + token count
- workspace_memories has observations
- No secrets in committed files
- Idempotent: same key → same run_id
- Retry injects defects into Scout

Commit only if explicitly asked.
```

## Artifacts To Create (M1 Prerequisites)

These files must be authored during M1 implementation. They do not exist yet.

### Agent configs (NullClaw config format)
- [ ] `config/echo.json` — read-only tools: `file_read`, `grep`, `find`. Model selection. Token limits. Autonomy: `full`. Sandbox: `bubblewrap`.
- [ ] `config/scout.json` — full tools: `file_read`, `file_write`, `file_edit`, `shell`. Model selection. Token limits. Autonomy: `full`. Sandbox: `bubblewrap`.
- [ ] `config/warden.json` — read + test tools: `file_read`, `grep`, `find`, `shell`. Model selection. Token limits. Autonomy: `full`. Sandbox: `bubblewrap`.

### System prompts
- [ ] `config/echo-prompt.md` — planner persona. Instructions: read repo, understand spec, produce plan.json.
- [ ] `config/scout-prompt.md` — builder persona. Instructions: implement plan, write code, produce implementation.md.
- [ ] `config/warden-prompt.md` — validator persona. Instructions: review against spec, run tests, produce validation.md with T1-T4 tiered findings.

### Build files
- [ ] `build.zig` — Zig build script with nullclaw dependency (git+https://github.com/nullclaw/nullclaw.git#v2026.3.1), Zap dependency, Postgres driver.
- [ ] `build.zig.zon` — dependency manifest with nullclaw v2026.3.1, zap, pg driver pinned versions.

### Schema
- [ ] `schema/001_initial.sql` — Postgres DDL: tenants, workspaces, specs, runs, run_transitions, artifacts, usage_ledger, workspace_memories, policy_events.

### Config
- [ ] `.env.example` — all env vars from M2/M3 deployment playbooks with placeholder values.
- [ ] `config/policy.json` — M1 policy defaults: retry budget (3), action classes (safe only), timeout (300s).

### CLI entry points
- [ ] `zombied serve` — starts Zap HTTP server + worker loop.
- [ ] `zombied doctor` — verifies Postgres connectivity, git access, agent config validity, LLM key reachability.
- [ ] `zombied run <spec_path>` — (optional) one-shot mode for local testing without HTTP server.

## Spec Completion Convention

When a milestone spec is fully implemented and verified:
1. Move the spec from `docs/spec/v1/M1_000_*.md` to `docs/done/v1/M1_000_*.md`
2. Mark the corresponding checkbox in the milestone tracker below with ✅
3. The spec file in `docs/done/v1/` is the permanent record — never edit after moving

### Milestone Tracker

#### M1 — Control Plane + Agent Pipeline
- ✅ `M1_000_CONTROL_PLANE_NULLCLAW_BASELINE.md` — Zig binary, NullClaw integration, HTTP API, state machine, agent pipeline
- ✅ `M1_002_API_AND_EVENTS_CONTRACTS.md` — API contracts, artifact schema, event envelope (all 4 gaps closed Mar 3, 2026)
- ✅ `M1_003_OBSERVABILITY_AND_POLICY.md` — Observability events, SLOs, policy guardrails (all 5 gaps closed Mar 3, 2026)

## Decisions Locked

1. **One Zig binary.** `@import("nullclaw")`. Native calls. One deploy artifact.
2. **3 things in the stack.** zombied binary, Postgres, git.
3. **No containers.** Landlock/Bubblewrap is the sandbox.
4. **State machine is the orchestrator.** No workflow engines.
5. **Git branch is the bus.** Commit + push. Works local. Works distributed.
6. **BYOK.** Revenue is agent-seconds.
7. **Tiered validation.** T1/T2 block. T3/T4 pass.
8. **Memory in Postgres.** Cross-run. Injected into prompts.
