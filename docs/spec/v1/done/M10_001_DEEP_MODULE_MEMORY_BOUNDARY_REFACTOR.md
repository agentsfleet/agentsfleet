# M10_001: Deep Module And Memory Boundary Refactor

**Prototype:** v1.0.0
**Milestone:** M10
**Workstream:** 1
**Date:** Mar 15, 2026
**Status:** DONE
**Priority:** P0 — maintainability and correctness hardening for core Zig runtime modules
**Batch:** B1 — execute in bounded slices with verify gates per file
**Depends on:** M9_002, M9_003, M9_004

---

## 1.0 Scope And Ownership Contract

**Status:** DONE

This workstream refactors oversized Zig modules into deep, cohesive submodules with explicit allocator ownership boundaries and no behavior regressions.

**Dimensions:**
- 1.1 DONE Establish allocator ownership contract in each touched module:
  - Caller-owned returns are explicitly documented and freed by caller.
  - Module-owned allocations are freed within module deinit paths.
  - Cross-module transfer points are explicit (`dupe`, `toOwnedSlice`, `deinit` pairings).
- 1.2 DONE Immediate execution scope:
  - `src/queue/redis.zig`
  - `src/pipeline/agents.zig`
  - `src/pipeline/scoring.zig`
- 1.3 DONE Planned follow-up scope documented and handed off to M10_002:
  - `src/cmd/reconcile.zig`
  - `src/git/ops.zig`
- 1.4 DONE Dynamic-agent compatibility constraint:
  - Agent refactor must preserve dynamic harness-aware role resolution and custom skill registry behavior; no hardcoded Echo/Scout/Warden-only runtime regression.

---

## 2.0 Refactor Design Slices

**Status:** DONE

### 2.1 Redis (`2 + 5`)

Use namespace modularization plus test extraction.

**Dimensions:**
- 2.1.1 DONE Split into internal modules (`redis_types.zig`, `redis_config.zig`, `redis_protocol.zig`, `redis_transport.zig`, `redis_client.zig`) while preserving public API via `redis.zig`.
- 2.1.2 DONE Extract tests into `src/queue/redis_test.zig` with root test import hook in `redis.zig`.
- 2.1.3 DONE Preserve TLS/CA handling and consumer-group readiness semantics exactly.
- 2.1.4 DONE Verify allocator lifecycle for RESP values, transport buffers, and client teardown paths (including invalid-port leak regression fix in URL parsing).

### 2.2 Agents (`4 + 5`)

Split by role runner concerns and extract tests.

**Dimensions:**
- 2.2.1 DONE Move runner implementations and role-specific helper construction into `src/pipeline/agents_runner.zig`.
- 2.2.2 DONE Keep registry/dynamic binding APIs stable (`SkillRegistry`, `resolveRoleWithRegistry`, `runByRole`).
- 2.2.3 DONE Extract tests into `src/pipeline/agents_test.zig` with root import hook.
- 2.2.4 DONE Ensure no ownership regressions for prompt buffers, tool sets, and runtime provider lifetimes.

### 2.3 Scoring (`3 + 5`)

Extract pure math, persistence, and orchestration concerns with tests split.

**Dimensions:**
- 2.3.1 DONE Extract pure scoring math/types into `src/pipeline/scoring_mod/{types,math}.zig` with deterministic behavior unchanged.
- 2.3.2 DONE Extract persistence and config decoding into `src/pipeline/scoring_mod/persistence.zig`.
- 2.3.3 DONE Keep fail-safe orchestrator semantics: scoring failures never fail run execution.
- 2.3.4 DONE Extract tests into `src/pipeline/scoring_test.zig` with root import hook.

---

## 3.0 Verification Standard (`skills/write-unit-test/SKILL.md`)

**Status:** DONE

All refactors in M10 must satisfy robust tiered testing and incremental baseline-first validation.

**Dimensions:**
- 3.1 DONE Baseline-first gate per folder/file slice before mutation (`make test-zombied`, targeted test lanes where available).
- 3.2 DONE Required tiers per touched module:
  - T1 Happy path
  - T2 Edge cases
  - T3 Negative/error paths
  - T6 Integration coverage where existing integration hooks exist
  - T7 Regression safety for preserved external behavior
- 3.3 DONE Add/retain allocator-safety tests where practical (including parse failure leak path in Redis URL parser).
- 3.4 DONE After each slice: rerun relevant gate and capture pass/fail summary before proceeding (`zig test src/queue/redis.zig`, `make test-zombied`).

---

## 4.0 Acceptance Criteria

**Status:** IN_PROGRESS

- [x] 4.1 `redis`, `agents`, `scoring` refactors complete with public behavior parity and passing `make test-zombied`.
- [x] 4.2 No allocator ownership ambiguity remains in touched paths; ownership boundaries are explicit in code structure and call sites.
- [x] 4.3 Dynamic harness-aware agent routing remains functional (custom skill registration + resolution + dispatch preserved).
- [x] 4.4 Test robustness increased or preserved according to `skills/write-unit-test/SKILL.md` tiers for touched modules.
- [x] 4.5 Planned next slice (`reconcile`, `ops`) documented with no hidden scope assumptions.

---

## 6.0 Planned Next Slice (Non-Immediate)

**Status:** DONE

Follow-up M10 scope remains:

- `src/cmd/reconcile.zig` (active next target)
- `src/git/ops.zig` (active next target)
- `src/state/workspace_billing.zig` (deferred by request; currently skipped)
- `src/auth/jwks.zig` and `src/db/pool.zig` (already refactored/test-split in this branch)

**Dimensions:**
- 6.1 DONE Capture explicit non-immediate target list to avoid hidden scope.
- 6.2 DONE Reconcile split plan finalized in `docs/spec/v1/M10_002_RECONCILE_OPS_DEEP_MODULE_REFACTOR.md`.
- 6.3 DONE Ops split plan finalized in `docs/spec/v1/M10_002_RECONCILE_OPS_DEEP_MODULE_REFACTOR.md`.
- 6.4 DONE Execution explicitly moved to M10_002 scope.

---

## 5.0 Out of Scope (This Workstream)

- UI/website lint and frontend dependency remediation.
- New product features beyond refactor and test-hardening scope.
- Workspace billing refactor in this immediate batch (`workspace_billing.zig` deferred by request).
