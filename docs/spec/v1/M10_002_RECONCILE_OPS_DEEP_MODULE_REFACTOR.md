# M10_002: Reconcile And Ops Deep Module Refactor

**Prototype:** v1.0.0
**Milestone:** M10
**Workstream:** 2
**Date:** Mar 15, 2026
**Status:** IN_PROGRESS
**Priority:** P0 — maintainability and correctness hardening for runtime control surfaces
**Batch:** B2 — execute after M10_001 completion
**Depends on:** M10_001

---

## 1.0 Scope

**Status:** IN_PROGRESS

Refactor oversized runtime orchestration files into deep modules with explicit allocator ownership and unchanged runtime behavior.

**Dimensions:**
- 1.1 DONE Target files locked: `src/cmd/reconcile.zig`, `src/git/ops.zig`.
- 1.2 IN_PROGRESS Preserve all existing public API and CLI/runtime behavior.
- 1.3 PENDING Extract cohesive submodules with clear ownership contracts at module boundaries.
- 1.4 PENDING Move and expand tests per `skills/write-unit-test/SKILL.md` tier requirements.

---

## 2.0 Reconcile Refactor

**Status:** PENDING

**Dimensions:**
- 2.1 PENDING Split reconcile orchestration, idempotency logic, and side-effect boundaries into focused modules.
- 2.2 PENDING Preserve retry/state semantics and existing command contract.
- 2.3 PENDING Add robust tests for happy path, edge cases, error paths, and regression cases.
- 2.4 PENDING Verify allocator/deinit pairing for all newly moved ownership paths.

---

## 3.0 Ops Refactor

**Status:** PENDING

**Dimensions:**
- 3.1 PENDING Split git ops execution planning, state tracking, and error normalization into focused modules.
- 3.2 PENDING Preserve existing command behavior and output contracts.
- 3.3 PENDING Add robust tests including negative paths and integration-adjacent coverage where hooks exist.
- 3.4 PENDING Verify allocator/deinit pairing and avoid leaked owned slices/buffers.

---

## 4.0 Verification Standard

**Status:** PENDING

Follow `skills/write-unit-test/SKILL.md` with mandatory coverage tiers on touched surfaces:

- T1 Happy path
- T2 Edge cases
- T3 Negative/error paths
- T6 Integration coverage (where supported)
- T7 Regression behavior parity

**Dimensions:**
- 4.1 PENDING Baseline-first run before edits.
- 4.2 PENDING Targeted tests for reconcile/ops paths.
- 4.3 PENDING Full `make test-zombied` gate green after refactor.
- 4.4 PENDING Report allocator-boundary verification results.

---

## 5.0 Acceptance Criteria

**Status:** PENDING

- [ ] 5.1 `reconcile.zig` refactored into deep modules with behavior parity.
- [ ] 5.2 `ops.zig` refactored into deep modules with behavior parity.
- [ ] 5.3 Allocator ownership boundaries explicit and verified in touched paths.
- [ ] 5.4 Tests are robust per `skills/write-unit-test/SKILL.md` and gate is green.

---

## 6.0 Out Of Scope

- `src/state/workspace_billing.zig` (deferred)
- Additional product feature changes beyond refactor/test hardening
