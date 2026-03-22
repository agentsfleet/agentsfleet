# M11_002: Deep Module Split For 400-Line Cap

**Prototype:** v1.0.0
**Milestone:** M11
**Workstream:** 002
**Date:** Mar 19, 2026
**Status:** DONE
**Priority:** P1 — maintainability hardening for observability, runtime execution, persistence, and billing modules
**Batch:** B2 — starts after M11_001 async observability baseline is stable
**Depends on:** M11_001 (Grafana Observability Pipeline And Langfuse Async Delivery)

---

## 1.0 Refactor Boundaries And Split Plan

**Status:** DONE

Define deterministic module boundaries and split ownership before editing behavior code.

**Dimensions:**
- 1.1 DONE `src/observability/metrics.zig` split plan finalized so no output contract changes while reducing to < 400 lines per file
- 1.2 DONE `src/pipeline/worker_stage_executor.zig` split plan finalized so stage-state behavior remains identical while reducing to < 400 lines per file
- 1.3 DONE `src/pipeline/scoring_mod/persistence.zig` split plan finalized so persistence SQL/write contract remains identical while reducing to < 400 lines per file
- 1.4 DONE `src/state/workspace_credit.zig` split plan finalized so credit lifecycle and error contracts remain identical while reducing to < 400 lines per file

---

## 2.0 Module Extraction And Wiring

**Status:** DONE

Extract cohesive submodules and rewire imports with minimal churn.

**Dimensions:**
- 2.1 DONE Extract metrics internals (snapshot/render/helpers) into focused modules; keep existing public call surface stable
- 2.2 DONE Extract worker stage execution responsibilities (stage loop, side effects, finalize paths) into focused modules; preserve run-state transitions and event semantics
- 2.3 DONE Extract scoring persistence SQL and mapping logic into focused modules; preserve query/write behavior and schema contract
- 2.4 DONE Extract workspace credit lifecycle components (grant/debit/gates/helpers) into focused modules; preserve API and state transitions

---

## 3.0 Verification And Regression Guardrails

**Status:** DONE

Prove zero behavior regression with deterministic gates.

**Dimensions:**
- 3.1 DONE Every targeted file and any new replacement file in this workstream is < 400 lines
- 3.2 DONE `make lint`, `make test-unit`, and `python3 lint-zig.py` pass after refactor
- 3.3 DONE Relevant integration/contract tests pass for worker execution path, scoring persistence path, and workspace credit path
- 3.4 DONE No newly unreachable code introduced; all extracted functions remain reachable via updated call sites

---

## 4.0 Acceptance Criteria

**Status:** DONE

- [x] 4.1 `src/observability/metrics.zig` responsibilities are split and no single resulting module exceeds 400 lines
- [x] 4.2 `src/pipeline/worker_stage_executor.zig` responsibilities are split and no single resulting module exceeds 400 lines
- [x] 4.3 `src/pipeline/scoring_mod/persistence.zig` responsibilities are split and no single resulting module exceeds 400 lines
- [x] 4.4 `src/state/workspace_credit.zig` responsibilities are split and no single resulting module exceeds 400 lines

---

## 5.0 Out Of Scope

- Behavior changes to billing policy, scoring algorithm, or runtime state machine semantics
- New observability vendors or analytics products
- API contract changes for CLI or HTTP surfaces unrelated to line-count refactor
