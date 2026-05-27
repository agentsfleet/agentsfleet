# M80_003: Strip the direct in-zombied execution path (delivered by M80_002)

**Prototype:** v2.0.0
**Milestone:** M80
**Workstream:** 003
**Date:** May 27, 2026
**Status:** DONE
**Priority:** P1 — removing the direct worker path is what lets the runner be the sole execution plane.
**Categories:** API
**Batch:** B1
**Branch:** feat/m80-001-runner-contract-keystone
**Depends on:** M80_002 (the cutover that performed this removal)
**Provenance:** agent-generated (Opus 4.7, May 27, 2026 — from the `runner_fleet.md` S-stage roadmap)

> **Provenance is load-bearing.** This is a roadmap-reconciliation record, not new work. Trust it as a pointer to M80_002, where the code and tests actually live.

**Canonical architecture:** `docs/architecture/runner_fleet.md` (S2 row) — the roadmap stage this workstream number was reserved for.

---

## Implementing agent — read these first

> No implementation remains. This record exists so the milestone numbering is honest: S2/M80_003 was **absorbed into M80_002**, not skipped.

1. `docs/v2/active/M80_002_P1_API_RUNNER_CUTOVER.md` §3 — the cutover slice that deleted the direct worker path; this record's Dimensions map onto it.
2. `docs/architecture/runner_fleet.md` (S-stage table) — defines S1–S6 / M80_002–007; explains why M80_002 absorbed S1–S4.
3. `docs/architecture/data_flow.md` — the single-process model this removal retired (reconcile pending in M80_002 §7).

---

## PR Intent & comprehension handshake

- **PR title (eventual):** delivered under M80_002's PR (no separate PR)
- **Intent (one sentence):** record that the "thin the worker, strip the direct PG/Redis execution path" stage shipped inside M80_002, so the M80_003 number is accounted for rather than dangling.
- **Handshake:** the roadmap (`runner_fleet.md`) reserved M80_003 for stripping the direct path. M80_002 explicitly absorbed S1–S4 into one PR, so this stage's work is already merged on this branch. ASSUMPTIONS: no further code is owed under this number; the Dimensions below are verified by M80_002's tests, not new ones.

---

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — RULE NDC / NLR / ORP governed the deletion (no dead code left, orphan sweep after the strip); applied within M80_002.
- **`docs/ZIG_RULES.md`** — applied to the `*.zig` deletions in M80_002.

No new rule surface — this record adds no code.

---

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG / PUB / LENGTH / UFS / LOGGING / SCHEMA | no | docs/markdown only — this record adds no source. The deletions tripped these gates inside M80_002 and were satisfied there. |

N/A — docs/markdown only.

---

## Overview

**Goal (testable):** the direct per-zombie worker execution path and the `zombied-executor` sidecar no longer exist in zombied; execution flows only through the runner lease/report protocol — asserted by M80_002 Dimension 3.1/3.2 (`test_direct_worker_path_removed`, build shows sidecar targets absent).

**Problem:** the roadmap split execution-plane work across S1–S6. S2 (M80_003) was "thin the worker / strip the direct path." Shipping it as a standalone workstream after M80_002 is impossible — M80_002 already deleted the path it would have stripped.

**Solution summary:** record the absorption. The strip happened in M80_002 §3 (delete `event_loop*`, `cmd/worker*`, the executor sidecar + Unix-socket transport). This file marks the number DONE and points at that work; no code changes here.

---

## Prior-Art / Reference Implementations

- **API** → M80_002 §3 is the implementation; this record mirrors its Dimensions. No new pattern.

---

## Files Changed (blast radius)

> No files change under this record. The table lists what M80_002 deleted to satisfy this stage, for traceability.

| File | Action | Why |
|------|--------|-----|
| `src/zombied/zombie/event_loop*.zig` | DELETE (in M80_002) | the 12-step direct `processEvent` execution path |
| `src/zombied/cmd/worker*.zig` | DELETE (in M80_002) | the `zombied worker` subcommand that spawned per-zombie threads |
| `src/executor/main.zig`, `transport.zig` + sidecar build targets | DELETE (in M80_002) | the Unix-socket `zombied-executor` sidecar |

---

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** record-only. The numbering is reconciled by a DONE pointer rather than a retroactive standalone PR.
- **Alternatives considered:** (a) leave M80_003 dangling as a forward-reference — rejected, it leaves the roadmap inconsistent with `done/`; (b) renumber the remaining workstreams to close the gap — rejected, it breaks existing forward-references (`data_flow.md` "reconcile in M80_003").
- **Patch-vs-refactor verdict:** neither — this is a **documentation reconciliation**. The refactor itself was M80_002.

---

## Sections (implementation slices)

### §1 — Direct path removal (delivered by M80_002 §3)

What it delivered: zombied holds no execution path; the runner is the only processor. Why it must exist: Invariant — a single execution plane, no flag to fall back to.

- **Dimension 1.1** — the direct per-zombie worker path + `zombie:control` consumer are removed → Test `test_direct_worker_path_removed` (M80_002). **DONE.**
- **Dimension 1.2** — `src/executor/main.zig` + `transport.zig` + sidecar build targets deleted; no Unix-socket transport remains → verified by build (M80_002). **DONE.**

---

## Interfaces

```
No interface owned by this record. The execution interface is the runner
lease/report protocol frozen by M80_001 and implemented by M80_002:
  POST /v1/runners/me/leases   — assignment
  POST /v1/runners/me/reports  — terminal write
```

---

## Failure Modes

> The removal's failure handling is M80_002's (a runner death is reclaimed; a stale report is fenced). No failure path is owned here.

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Direct-path symbol resurfaces | a stray import of a deleted worker symbol | orphan sweep (RULE ORP) + the `legacy event-substrate symbols` lint gate fail the build → caught in CI, not at runtime |

---

## Invariants

1. Single execution plane — there is no direct path to fall back to (the flag is deleted, not flipped) — enforced by M80_002 Dimension 6.1 (`test_runner_is_default_processor`) + the orphan-sweep lint gate.

---

## Test Specification (tiered)

> No new tests. The Dimensions are verified by M80_002's suite, listed for traceability.

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | integration | `test_direct_worker_path_removed` (M80_002) | the worker entrypoint is gone / inert; no per-zombie execution thread spawns |
| 1.2 | build | sidecar targets absent | `zig build` exposes no `zombied-executor` artifact |

Regression: N/A — the removal IS the change; M80_002's row-equivalence tests guard that the runner path reproduces the deleted path's writes.

---

## Acceptance Criteria

- [x] Direct worker path removed — verify: `grep -rn "processEvent\|zombied-worker" src/zombied | head` (0 active-code matches; lint gate enforces)
- [x] Executor sidecar gone — verify: `zig build 2>&1 | grep -c executor` (no sidecar target)
- [x] Covered by M80_002's merged tests — verify: `make test-integration` on this branch

---

## Discovery (consult log)

- **Consult (May 27, 2026):** Indy asked for M80_003–006 specs created in this PR before CHORE(close). Scope decision: option 1 — remaining-only, honest status. M80_003's work was absorbed by M80_002, so it lands as a DONE record in `done/` pointing at the cutover (not a standalone PR).

---

## Skill-Driven Review Chain (mandatory)

| When | Skill | What it does | Required output |
|------|-------|--------------|-----------------|
| n/a — no code | `/write-unit-test` / `/review` / `/review-pr` | nothing to review here; the code + tests were reviewed under M80_002 | covered by M80_002's chain |

---

## Verification Evidence

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Direct path gone | `grep -rn "processEvent" src/zombied` | enforced by M80_002 + lint gate | ✓ (M80_002) |

---

## Out of Scope

- The runner that replaced the direct path — M80_002.
- Fleet inventory / heartbeat reassignment — M80_006.
