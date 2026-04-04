# M1_000: Control Plane + NullClaw Baseline

Date: Mar 2, 2026
Status: PENDING — first spec to implement
Supersedes: `docs/done/v1/stale_M1_001_CONTROL_PLANE_PI_SDK.dont_read.md`

## What This Spec Covers

One Zig binary that imports NullClaw as a library, exposes 6 HTTP API endpoints, runs the agent pipeline via native calls, and uses git branch as distributed state.

## What Carries Forward From M1_001 (unchanged)

These contracts are locked:

1. **State machine:** `SPEC_QUEUED → RUN_PLANNED → PATCH_IN_PROGRESS → PATCH_READY → VERIFICATION_IN_PROGRESS → PR_PREPARED → PR_OPENED → NOTIFIED → DONE`. Failure: `VERIFICATION_FAILED → PATCH_IN_PROGRESS` (retries) or `BLOCKED → NOTIFIED_BLOCKED` (exhausted).
2. **6 API operations:** `start_run`, `get_run`, `retry_run`, `pause_workspace`, `list_specs`, `sync_specs`.
3. **Idempotency:** `idempotency_key`, `retry_token`, monotonic `version`.
4. **Transition ledger:** Append-only `run_transitions`.
5. **Reason codes:** Full catalog (PLAN_COMPLETE, PATCH_STARTED, VALIDATION_PASSED, etc.).
6. **Actor roles:** `echo`, `scout`, `warden`, `orchestrator`.
7. **Artifact contract:** plan.json, implementation.md, validation.md, attempt_N_defects.md, run_summary.md.
8. **Spec lifecycle:** `PENDING_*.md` in repo. Status in DB. No renaming.

## Implementation Model

### 1) Zig binary with `@import("nullclaw")`

```zig
const nullclaw = @import("nullclaw");

// Native agent calls — typed results, Zig errors, zero serialization
const echo_cfg = try nullclaw.config.load("echo.json");
const plan = try nullclaw.agent.run(echo_cfg, .{
    .message = spec_content ++ memory_context,
    .sandbox = .{ .mode = .bubblewrap, .allow_paths = &.{workspace_path} },
});
// plan.artifacts, plan.token_count, plan.exit_status — all typed
```

Implementation requirements:
- [ ] `build.zig.zon` with nullclaw dependency
- [ ] Agent configs: `echo.json`, `scout.json`, `warden.json` in NullClaw vtable format
- [ ] System prompts: `echo-prompt.md`, `scout-prompt.md`, `warden-prompt.md`
- [ ] Artifact extraction from `nullclaw.agent.run()` result — typed, not stdout parsing
- [ ] Error handling via Zig error unions — not exit codes

### 2) HTTP API via Zap

6 endpoints, same contracts as M1_001:

```zig
// Zap HTTP server — 6 endpoints
fn initRouter(router: *zap.Router) void {
    router.post("/v1/runs", startRun);
    router.get("/v1/runs/:run_id", getRun);
    router.post("/v1/runs/:run_id\\:retry", retryRun);
    router.post("/v1/workspaces/:workspace_id\\:pause", pauseWorkspace);
    router.get("/v1/specs", listSpecs);
    router.post("/v1/workspaces/:workspace_id\\:sync", syncSpecs);
}
```

### 3) Security: "Isolate the agent"

NullClaw's built-in sandbox via native API:

```zig
const result = try nullclaw.agent.run(scout_cfg, .{
    .message = plan_content,
    .sandbox = .{
        .mode = .bubblewrap,
        .allow_paths = &.{"/workspace"},
        .allow_net = &.{"control-plane.internal", "api.anthropic.com"},
    },
    .secrets = .{
        .session_token = session_token,
        .control_plane_url = callback_url,
    },
});
```

Zero secrets in agent context. LLM key passed via NullClaw's security module, not env vars.

### 4) NullClaw tool permissions (vtable config)

| Agent | Config | Enabled | Disabled |
|-------|--------|---------|----------|
| Echo | `echo.json` | `file_read`, `grep`, `find`, `ls` | `file_write`, `file_edit`, `shell` |
| Scout | `scout.json` | `file_read`, `file_write`, `file_edit`, `shell` | — |
| Warden | `warden.json` | `file_read`, `grep`, `find`, `shell` | `file_write`, `file_edit` |

Enforced by NullClaw vtable — not application-level checks.

### 5) Git branch as handoff

Each stage commits and pushes to the feature branch:

```
zombie binary
    ├── git fetch + worktree add
    ├── nullclaw.agent.run(echo) → commit plan.json → push
    ├── nullclaw.agent.run(scout) → commit code → push
    ├── nullclaw.agent.run(warden) → commit validation.md → push
    └── create PR (if PASS)
```

Local: shared worktree. Distributed: push/pull between stages.

### 6) Memory

- **PlanetScale** (cross-run): `workspace_memories` table
- **NullClaw SQLite** (per-run): `nullclaw.memory` module — ephemeral

### 7) Tiered validation (T1-T4)

| Tier | Meaning | Action |
|------|---------|--------|
| T1 | Critical (security, data loss) | FAIL |
| T2 | Significant (spec mismatch, missing tests) | FAIL |
| T3 | Minor (style, naming) | PASS with notes |
| T4 | Suggestion (optimization) | PASS |

## Schema (PlanetScale Postgres)

```
tenants, workspaces, specs, runs, run_transitions,
artifacts, usage_ledger, workspace_memories, policy_events
```

## Execution Order

1. **SCAFFOLD** — `build.zig`, `build.zig.zon` (nullclaw dep), project structure
2. **NULLCLAW INTEGRATION** — `@import("nullclaw")`, load configs, verify agent.run() works
3. **HTTP API** — Zap server, 6 endpoints, error contract
4. **STATE MACHINE** — transitions, policy checks, idempotency
5. **SCHEMA** — PlanetScale tables, Postgres driver
6. **AGENT PIPELINE** — Echo → Scout → Warden via native calls
7. **GIT OPS** — bare clone + worktree, commit, push, PR via GitHub API
8. **MEMORY** — workspace_memories inject + capture
9. **SECRETS** — AES-256-GCM, NullClaw security module
10. **E2E** — spec → pipeline → PR
11. **VERIFY** — acceptance criteria

## Acceptance Criteria

1. [ ] One Zig binary compiles with `@import("nullclaw")`
2. [ ] 6 HTTP endpoints match M1_002 contracts exactly
3. [ ] NullClaw agent.run() executes for all 3 roles (typed results)
4. [ ] Tool permissions enforced by vtable config
5. [ ] Artifacts committed to feature branch via git
6. [ ] Tiered validation (T1/T2 block, T3/T4 pass)
7. [ ] workspace_memories populated after run
8. [ ] State transitions recorded in append-only table
9. [ ] Idempotency works (same key → same run_id)
10. [ ] E2E: `PENDING_*.md` → Echo → Scout → Warden → PR

## What NOT to Build

- No TypeScript, Bun, Node, npm
- No subprocess spawning (native calls only)
- No Docker, containers, Daytona, Rivet, Temporal
- No OpenClaw, PI SDK
- No chat/voice, no website routes
- No billing (metering recorded, deferred)
- No Clerk auth (API key only)
