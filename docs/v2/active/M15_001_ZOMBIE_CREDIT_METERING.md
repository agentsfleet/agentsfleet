# M15_001: Zombie Credit Metering — Post-Execution Deduction

**Prototype:** v0.9.0
**Milestone:** M15
**Workstream:** 001
**Date:** Apr 11, 2026
**Status:** IN_PROGRESS
**Priority:** P0 — Free-plan zombies currently consume unlimited quota
**Batch:** B1
**Branch:** feat/m15-zombie-metering
**Depends on:** M10_001 (pipeline v1 removal — billing.zig deleted)

---

## Overview

**Goal:** After each zombie event delivery, deduct agent-seconds from the workspace credit
balance so `consumed_credit_cents` and `remaining_credit_cents` stay accurate.

**Problem:** `deductCompletedRuntimeUsage()` in `src/state/workspace_credit.zig:97` exists
but is called only in tests. The pre-execution gate (`CreditPolicy.execution_required` →
`enforceExecutionAllowed()` in `src/http/workspace_guards.zig:70`) blocks exhausted
workspaces, but `consumed_credit_cents` is never updated — balance is frozen at provisioning.

**V1 reference (deleted):** `src/http/handlers/runs/start_budget.zig` — blocked `POST /v1/runs`
pre-enqueue if credits exhausted. Zombie equivalent is a post-delivery deduction (execution
already happened; don't block, but record and gate future events if now exhausted).

**Solution:** Introduce `src/zombie/metering.zig` with two types and one function (≤50L).
Call it in `src/zombie/event_loop.zig` after `deliverEvent()` succeeds and before XACK.
Write a `CREDIT_DEDUCTED` audit row via the existing `store.insertAudit()` path in
`src/state/workspace_credit_store.zig`.

---

## 1.0 Type Design

**Status:** PENDING

New file: `src/zombie/metering.zig`. All types are borrowed-slice structs (no ownership).
Callers own the strings; metering.zig does not dupe or free.

**Dimensions:**
- 1.1 PENDING
  - target: `src/zombie/metering.zig:ExecutionUsage`
  - input: event_loop delivery result — zombie_id, workspace_id, event_id, agent_seconds, token_count
  - expected: struct compiles; all fields `[]const u8` or `u64`; no optionals
  - test_type: unit (comptime)
- 1.2 PENDING
  - target: `src/zombie/metering.zig:DeductionResult`
  - input: N/A — tagged union with four variants
  - expected: `union(enum) { deducted: i64, exempt: void, exhausted: i64, db_error: void }` compiles
  - test_type: unit (comptime)
- 1.3 PENDING
  - target: `src/zombie/metering.zig:deductZombieUsage`
  - input: `ExecutionUsage{ .agent_seconds=30 }`, scale-plan workspace
  - expected: returns `.exempt` — no DB write, no audit row
  - test_type: unit (mock conn)
- 1.4 PENDING
  - target: `src/zombie/metering.zig:deductZombieUsage`
  - input: `ExecutionUsage{ .agent_seconds=30 }`, free-plan, remaining=0
  - expected: returns `.exhausted` with remaining_cents=0; `CREDIT_DEDUCTED` audit row written
  - test_type: integration (real DB)

---

## 2.0 Event Loop Integration

**Status:** PENDING

Splice `deductZombieUsage()` into `src/zombie/event_loop.zig` after the executor
`deliverEvent()` call succeeds, before XACK. Non-blocking: `db_error` variant logs
a warning and continues to XACK so the event is not redelivered.

**Dimensions:**
- 2.1 PENDING
  - target: `src/zombie/event_loop.zig:processEvent`
  - input: successful delivery, free-plan, agent_seconds=60
  - expected: `deductZombieUsage` called; audit row with `CREDIT_DEDUCTED` and `agent_seconds=60`
    exists in DB after XACK
  - test_type: integration (real DB + Redis)
- 2.2 PENDING
  - target: `src/zombie/event_loop.zig:processEvent`
  - input: `deductZombieUsage` returns `.db_error`
  - expected: event still XACK'd; log line `metering.db_error zombie_id=...` emitted; no panic
  - test_type: integration (injected conn failure)
- 2.3 PENDING
  - target: `src/zombie/metering.zig:deductZombieUsage`
  - input: same event_id presented twice (crash-recovery replay scenario)
  - expected: second call returns `.deducted` with 0 cents (idempotency via `hasAuditEvent` check)
  - test_type: integration (real DB)

---

## 3.0 Interfaces

**Status:** PENDING

### 3.1 New Types (`src/zombie/metering.zig`)

```zig
pub const ExecutionUsage = struct {
    zombie_id:    []const u8,  // borrowed
    workspace_id: []const u8,  // borrowed
    event_id:     []const u8,  // borrowed — idempotency key for audit dedup
    agent_seconds: u64,
    token_count:   u64,
};

pub const DeductionResult = union(enum) {
    deducted:  i64,   // cents consumed this call
    exempt:    void,  // scale plan — charge not applicable
    exhausted: i64,   // credits already 0; audit still written
    db_error:  void,  // non-fatal; event loop continues to XACK
};
```

### 3.2 Public Function

```zig
// src/zombie/metering.zig — must stay ≤50 lines
pub fn deductZombieUsage(
    conn:         *pg.Conn,
    alloc:        std.mem.Allocator,
    usage:        ExecutionUsage,
    plan_tier:    workspace_billing.PlanTier,
) DeductionResult
```

Calls into `workspace_credit.deductCompletedRuntimeUsage()` (line 97) and
`workspace_credit_store.insertAudit()`. No new SQL — reuse v1 billing path.

### 3.3 Error Contracts

| Error condition | Behavior | Caller sees |
|----------------|----------|-------------|
| DB unavailable | Log warning, return `.db_error` | Event still XACK'd |
| Duplicate event_id | `hasAuditEvent` returns true, skip write | `.deducted` with 0 cents |
| Scale plan workspace | Skip all DB writes | `.exempt` |
| agent_seconds = 0 | `runtimeUsageCostCents` returns 0, skip write | `.deducted` with 0 |

---

## 4.0 Implementation Constraints

| Constraint | Verify |
|-----------|--------|
| `metering.zig` ≤ 350 lines | `wc -l src/zombie/metering.zig` |
| `deductZombieUsage` ≤ 50 lines | `wc -l` the function body |
| No new SQL — reuse `workspace_credit_store` | grep for raw SQL strings in metering.zig |
| Idempotent on replay | integration test 2.3 |
| Cross-compile | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` |
| pg-drain clean | `make check-pg-drain` |

---

## 5.0 Test Specification

### Unit Tests

| Test name | Dim | Target | Input | Expected |
|-----------|-----|--------|-------|----------|
| `deduction_result_variants_compile` | 1.2 | `DeductionResult` | N/A | comptime pass |
| `exempt_on_scale_plan` | 1.3 | `deductZombieUsage` | scale plan | `.exempt` |
| `exhausted_returns_zero_remaining` | 1.4 | `deductZombieUsage` | remaining=0 | `.exhausted` |

### Integration Tests

| Test name | Dim | Infra | Input | Expected |
|-----------|-----|-------|-------|----------|
| `audit_row_written_on_deduction` | 2.1 | DB | free plan, 60s | `CREDIT_DEDUCTED` audit row |
| `db_error_does_not_block_xack` | 2.2 | DB (fail) | conn inject | event XACK'd |
| `deduction_idempotent_on_replay` | 2.3 | DB | same event_id x2 | second call 0 cents |

### Spec-Claim Tracing

| Spec claim | Test | Type |
|-----------|------|------|
| consumed_credit_cents updated post-execution | `audit_row_written_on_deduction` | integration |
| Non-blocking on DB failure | `db_error_does_not_block_xack` | integration |
| Idempotent on event replay | `deduction_idempotent_on_replay` | integration |

---

## 6.0 Execution Plan

| Step | Action | Verify |
|------|--------|--------|
| 1 | Write `src/zombie/metering.zig` — types + function skeleton | `zig build` |
| 2 | Wire `deductZombieUsage` into `event_loop.zig:processEvent` | `zig build` |
| 3 | Write unit tests for type variants | `make test` |
| 4 | Write integration tests (real DB + replay) | `make test-integration-db` |
| 5 | Cross-compile + pg-drain + lint | `make lint` |

---

## 7.0 Acceptance Criteria

- [ ] `consumed_credit_cents` increments after each zombie delivery — verify: integration test 2.1
- [ ] Scale-plan workspaces produce no audit rows — verify: unit test 1.3
- [ ] DB failure does not drop the event (XACK still fires) — verify: integration test 2.2
- [ ] Replay of same event_id deducts 0 cents — verify: integration test 2.3
- [ ] `make lint` passes — verify: `make lint`
- [ ] Cross-compiles — verify: `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux`

---

## Applicable Rules

- RULE XCC — cross-compile before commit
- RULE FLL — 350-line gate on touched files
- RULE FLS — drain all results (new DB queries must use PgQuery)
- RULE ORP — orphan sweep if renaming symbols
- RULE TXN — every DELETE in a transaction must ROLLBACK on failure

## Invariants

N/A — no compile-time guardrails for this workstream.

## Eval Commands

```bash
# E1: Build + test
zig build 2>&1 | head -5; echo "build=$?"
zig build test 2>&1 | tail -5; echo "test=$?"

# E2: Lint + cross-compile + gitleaks
make lint 2>&1 | grep -E "✓|FAIL"
zig build -Dtarget=x86_64-linux 2>&1 | tail -3; echo "x86=$?"
zig build -Dtarget=aarch64-linux 2>&1 | tail -3; echo "arm=$?"
gitleaks detect 2>&1 | tail -3; echo "gitleaks=$?"

# E3: Memory leak check
zig build test 2>&1 | grep -i "leak" | head -5
echo "E3: leak check (empty = pass)"

# E4: 350-line gate
git diff --name-only origin/main | grep -v '\.md$' | xargs wc -l 2>/dev/null | awk '$1 > 350 { print "OVER: " $2 ": " $1 }'
```

## Dead Code Sweep

N/A — no files deleted.

## Verification Evidence

**Status:** PENDING — fill during VERIFY phase.

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Unit tests | `make test` | | |
| Integration tests | `make test-integration-db` | | |
| Cross-compile | `zig build -Dtarget=x86_64-linux` | | |
| Lint | `make lint` | | |
| Line gate | `wc -l src/zombie/metering.zig` | | |
| pg-drain | `make check-pg-drain` | | |

---

## Out of Scope

- Token-based billing (charge per token, not per agent-second) — separate workstream
- Scale plan metering — Scale is unlimited; deduction is free-plan only per `FREE_PLAN_CENTS_PER_AGENT_SECOND`
- PostHog billing events for zombie deductions — covered in M15_002
