---
Milestone: M17
Workstream: M17_002
Name: WORKER_EXECUTOR_ZOMBIE_ALIGNMENT
Status: DONE
Priority: P1 — executor still uses v1 pipeline concepts (run_id, stage_id) that don't exist in zombie world
Created: Apr 12, 2026
Depends on: M17_001 (harness teardown clears the old agent_profile path)
Branch: feat/m17-002-worker-executor-alignment
---

# M17_002 — Worker & Executor Zombie Architecture Alignment

## Overview

**Goal (testable):** The executor's `CorrelationContext` uses `zombie_id` and `session_id` instead of v1 pipeline fields (`run_id`, `stage_id`, `role_id`, `skill_id`). The worker startup path (`worker.zig`) is zombie-only with no vestigial v1 pipeline setup. `grep -rn "run_id\|stage_id\|role_id\|skill_id" src/executor/ --include="*.zig"` returns 0 matches (excluding test fixtures that use the new field names).

**Problem:** The executor was built during the v1 pipeline era. Its `CorrelationContext` (types.zig:14) carries `run_id`, `stage_id`, `role_id`, `skill_id` — concepts from the pipeline runner that no longer exist. The zombie event loop (`src/zombie/event_loop.zig`) maps zombie concepts into these v1 fields at call time, creating a translation layer that obscures the real data model. The worker startup (`worker.zig`) still initializes PostHog tracking and DB pools for both pipeline and zombie modes, when only zombie mode remains.

**Solution summary:** Rename executor `CorrelationContext` fields to match zombie concepts (`zombie_id`, `session_id`, `workspace_id`, `trace_id`), update the protocol/handler/runner chain, simplify worker.zig to zombie-only startup, and update all test fixtures.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/executor/types.zig` | MODIFY | Rename CorrelationContext fields: run_id→zombie_id, stage_id→session_id, remove role_id+skill_id (or repurpose) |
| `src/executor/handler.zig` | MODIFY | Update field references in CreateExecution RPC |
| `src/executor/runner.zig` | MODIFY | Update correlation field references |
| `src/executor/session.zig` | MODIFY | Update session tracking to use zombie_id |
| `src/executor/protocol.zig` | MODIFY | Update wire protocol field names |
| `src/executor/client.zig` | MODIFY | Update client-side field mapping |
| `src/executor/transport.zig` | MODIFY | Update transport field refs |
| `src/executor/executor_metrics.zig` | MODIFY | Update metric labels from run_id to zombie_id |
| `src/executor/lease.zig` | MODIFY | If it references correlation fields |
| `src/executor/tool_bridge.zig` | MODIFY | Update tool execution context |
| `src/executor/tool_builders.zig` | MODIFY | Update tool context construction |
| `src/zombie/event_loop.zig` | MODIFY | Remove v1→zombie translation layer, pass fields directly |
| `src/cmd/worker.zig` | MODIFY | Simplify to zombie-only startup (remove dual-mode vestiges) |
| `src/cmd/worker_zombie.zig` | MODIFY | Update if it constructs CorrelationContext |
| `src/executor/*_test.zig` (10 files) | MODIFY | Update test fixtures to use new field names |

## Applicable Rules

- RULE ORP — cross-layer orphan sweep (renaming fields across 15+ files)
- RULE XCC — cross-compile before commit
- RULE FLL — 350-line gate on touched files
- RULE NDC — no dead code (remove v1 translation layer)

## Sections

### §1.0 — Rename CorrelationContext fields

| Dim | Status | Check |
|-----|--------|-------|
| 1.1 | DONE | `types.zig`: `run_id` → `zombie_id` |
| 1.2 | DONE | `types.zig`: `stage_id` → `session_id` |
| 1.3 | DONE | `types.zig`: evaluate `role_id` and `skill_id` — remove if unused by zombie, or rename |
| 1.4 | DONE | All references across handler/runner/session/transport updated |

### §2.0 — Update executor protocol + client

| Dim | Status | Check |
|-----|--------|-------|
| 2.1 | DONE | `protocol.zig` wire format uses new field names |
| 2.2 | DONE | `client.zig` sends new field names |
| 2.3 | DONE | `handler.zig` parses new field names from JSON |
| 2.4 | DONE | `executor_metrics.zig` labels use zombie_id instead of run_id |

### §3.0 — Simplify zombie event loop integration

| Dim | Status | Check |
|-----|--------|-------|
| 3.1 | DONE | `event_loop.zig` passes zombie_id/session_id directly, no v1 translation |
| 3.2 | DONE | `worker.zig` startup is zombie-only (no dual-mode vestiges) |
| 3.3 | DONE | `worker_zombie.zig` constructs CorrelationContext with native field names |

### §4.0 — Update tests

| Dim | Status | Check |
|-----|--------|-------|
| 4.1 | DONE | All 10 executor test files use `zombie_id`/`session_id` in fixtures |
| 4.2 | DONE | Zero grep hits for old field names (`run_id`, `stage_id`) in executor tests |
| 4.3 | DONE | Integration test verifies full zombie → executor → result round-trip |

## Interfaces

### CorrelationContext (before → after)

```zig
// BEFORE (v1 pipeline concepts):
pub const CorrelationContext = struct {
    trace_id: []const u8,
    run_id: []const u8,       // v1 pipeline run
    workspace_id: []const u8,
    stage_id: []const u8,     // v1 pipeline stage
    role_id: []const u8,      // v1 pipeline role
    skill_id: []const u8,     // v1 pipeline skill
};

// AFTER (zombie concepts):
pub const CorrelationContext = struct {
    trace_id: []const u8,
    zombie_id: []const u8,    // core.zombies.id
    workspace_id: []const u8,
    session_id: []const u8,   // core.zombie_sessions.id
};
```

## Eval Commands

```bash
# E1: No v1 field names in executor source (excluding comments/docs)
count=$(grep -rn "\.run_id\|\.stage_id\|\.role_id\|\.skill_id" src/executor/ --include="*.zig" | grep -v "^.*//\|^.*_test\.zig:" | wc -l | tr -d ' ')
[ "$count" -eq 0 ] && echo "PASS" || echo "FAIL: $count stale v1 refs"

# E2: New field names present
grep -rn "zombie_id\|session_id" src/executor/types.zig --include="*.zig"

# E3: Build + test
zig build 2>&1 | head -5; echo "build=$?"
zig build test 2>&1 | tail -5; echo "test=$?"

# E4: Cross-compile
zig build -Dtarget=x86_64-linux 2>&1 | tail -3; echo "x86=$?"
zig build -Dtarget=aarch64-linux 2>&1 | tail -3; echo "arm=$?"

# E5: Lint
make lint-zig 2>&1 | grep -E "✓|FAIL"

# E6: Orphan sweep — no stale run_id/stage_id in zombie event loop
grep -rn "run_id\|stage_id" src/zombie/ --include="*.zig" | grep -v "^.*//\|^.*_test\.zig:"
echo "E6: (empty = pass)"
```

## Dead Code Sweep

**Orphaned references after rename:**

| Old symbol | Grep command | Expected |
|-----------|-------------|----------|
| `run_id` (in executor) | `grep -rn "run_id" src/executor/ --include="*.zig"` | 0 (non-comment) |
| `stage_id` | `grep -rn "stage_id" src/executor/ --include="*.zig"` | 0 |
| `role_id` | `grep -rn "role_id" src/executor/ --include="*.zig"` | 0 |
| `skill_id` | `grep -rn "skill_id" src/executor/ --include="*.zig"` | 0 |

## Acceptance Criteria

- [x] `CorrelationContext` uses `zombie_id` and `session_id` ✅
- [x] Zero `run_id`/`stage_id`/`role_id`/`skill_id` references in `src/executor/` ✅
- [x] Zombie event loop passes native field names, no translation ✅
- [x] Worker startup is zombie-only (was already — no vestiges to remove) ✅
- [x] `StagePayload` stripped of `stage_id`/`role_id`/`skill_id` (scope expansion) ✅
- [x] All executor tests pass (`zig build test` exit 0) ✅
- [x] Cross-compiles (x86_64-linux, aarch64-linux) ✅
- [x] `make lint` passes (zig + website + app + zombiectl + actionlint) ✅
- [x] `make check-pg-drain` passes ✅

## Scope Decisions (executed)

- **Dropped `role_id` and `skill_id` entirely** from both `CorrelationContext` and `StagePayload`. They were hardcoded literals (`"agent"`, `session.config.name`) in the sole real caller — dead v1 baggage.
- **Dropped `stage_id` from `StagePayload`** as well; `CorrelationContext.session_id` is the single authoritative identity.
- **Deleted `startStageBasic`** — RULE ORP sweep found zero callers. Purely dead scaffolding left from the pipeline era.

## Out-of-Scope Findings (needs separate spec)

`src/state/topology.zig` (`ProfileDoc` struct) and `src/state/entitlements.zig` still carry v1 `role_id` / `skill_id` fields for pipeline profile-doc parsing. They're **live** — used by `types/defaults.zig` and `workspace_billing/db.zig`. If the v1 pipeline profile subsystem is also slated for teardown, it needs its own workstream (suggest **M17_003 — TOPOLOGY_ENTITLEMENTS_V1_TEARDOWN**). Not touched here to avoid scope creep.
- **Log-line rewrite** in `handler.zig`: `executor.create_execution` and `executor.runner.start` now emit `zombie_id`/`session_id`.
- **Worker dual-mode vestige** — inspected `src/cmd/worker.zig`: already zombie-only (spawns `worker_zombie.zombieWorkerLoop` per zombie). No changes needed — spec §3.2 was pre-corrected by an earlier pass.
- **Orphan sweep** verified across `schema/*.sql`, `src/`, `zombiectl/`, `ui/packages/website/`, `docs/v2/active/`. The `run_id` refs in `zombiectl/test/` and `ui/packages/website/src/` are the independent HTTP `/runs` API concept, unrelated to executor CorrelationContext — explicitly out of scope.

## Out of Scope

- Executor backend changes (cgroup, landlock, network policy — unchanged)
- Zombie event loop logic changes (only correlation field mapping changes)
- New executor features (Firecracker backend — future milestone)
- Schema changes (core.zombies table is already correct)
- Protocol version negotiation (single version, clean swap)
