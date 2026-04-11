---
Milestone: M17
Workstream: M17_002
Name: WORKER_EXECUTOR_ZOMBIE_ALIGNMENT
Status: PENDING
Priority: P1 — executor still uses v1 pipeline concepts (run_id, stage_id) that don't exist in zombie world
Created: Apr 12, 2026
Depends on: M17_001 (harness teardown clears the old agent_profile path)
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
| 1.1 | PENDING | `types.zig`: `run_id` → `zombie_id` |
| 1.2 | PENDING | `types.zig`: `stage_id` → `session_id` |
| 1.3 | PENDING | `types.zig`: evaluate `role_id` and `skill_id` — remove if unused by zombie, or rename |
| 1.4 | PENDING | All references across handler/runner/session/transport updated |

### §2.0 — Update executor protocol + client

| Dim | Status | Check |
|-----|--------|-------|
| 2.1 | PENDING | `protocol.zig` wire format uses new field names |
| 2.2 | PENDING | `client.zig` sends new field names |
| 2.3 | PENDING | `handler.zig` parses new field names from JSON |
| 2.4 | PENDING | `executor_metrics.zig` labels use zombie_id instead of run_id |

### §3.0 — Simplify zombie event loop integration

| Dim | Status | Check |
|-----|--------|-------|
| 3.1 | PENDING | `event_loop.zig` passes zombie_id/session_id directly, no v1 translation |
| 3.2 | PENDING | `worker.zig` startup is zombie-only (no dual-mode vestiges) |
| 3.3 | PENDING | `worker_zombie.zig` constructs CorrelationContext with native field names |

### §4.0 — Update tests

| Dim | Status | Check |
|-----|--------|-------|
| 4.1 | PENDING | All 10 executor test files use `zombie_id`/`session_id` in fixtures |
| 4.2 | PENDING | Zero grep hits for old field names (`run_id`, `stage_id`) in executor tests |
| 4.3 | PENDING | Integration test verifies full zombie → executor → result round-trip |

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
count=$(grep -rn "\.run_id\|\.stage_id\|\.role_id\|\.skill_id" src/executor/ --include="*.zig" | grep -v "//\|///\|test" | wc -l | tr -d ' ')
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
grep -rn "run_id\|stage_id" src/zombie/ --include="*.zig" | grep -v "//\|test"
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

- [ ] `CorrelationContext` uses `zombie_id` and `session_id` — verify: `grep "zombie_id" src/executor/types.zig`
- [ ] Zero `run_id` references in executor source (non-comment) — verify: grep command
- [ ] Zombie event loop passes native field names, no translation — verify: read `event_loop.zig`
- [ ] Worker startup is zombie-only — verify: read `worker.zig`
- [ ] All executor tests pass with new field names — verify: `zig build test`
- [ ] Cross-compiles — verify: `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux`

## Out of Scope

- Executor backend changes (cgroup, landlock, network policy — unchanged)
- Zombie event loop logic changes (only correlation field mapping changes)
- New executor features (Firecracker backend — future milestone)
- Schema changes (core.zombies table is already correct)
- Protocol version negotiation (single version, clean swap)
