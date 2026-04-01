# M4_005: Harden Events, Observability (Langfuse), And Config Hygiene

**Prototype:** v1.0.0
**Milestone:** M4
**Workstream:** 005
**Date:** Mar 06, 2026
**Status:** DONE
**Priority:** P1 — required for free plan metering
**Batch:** B3 — needs M4_007
**Depends on:** M4_007 (Define Runtime, Observability, And Config Contracts)

---

## 1.0 Singular Function

**Status:** DONE

Implement one working hardening function for D4/D8/D19/D20 runtime concerns.

**Dimensions:**
- 1.1 DONE Add durable event persistence/replay boundary (`src/state/outbox_reconciler.zig` — startup `SELECT ... FOR UPDATE SKIP LOCKED` reconciler)
- 1.2 DONE Add canonical trace context model (`src/observability/trace.zig` — W3C traceparent, threaded through `RunContext`)
- 1.3 DONE Add OTEL-friendly export path without Prometheus regression (`src/observability/otel_export.zig` — OTLP JSON render + HTTP POST export path)
- 1.4 DONE Add key-versioned config/secret envelope and rotation verification (`src/secrets/crypto.zig` + `src/config/runtime.zig` — `KEK_VERSION`, `ENCRYPTION_MASTER_KEY_V2`)
- 1.5 DONE Integrate Langfuse as LLM/agent tracing backend (`src/observability/langfuse.zig` — ingest payload + HTTP POST path, wired into `worker_stage_executor.zig`)

---

## 2.0 Verification Units

**Status:** DONE

**Dimensions:**
- 2.1 DONE Unit/integration tests: replay model survives restart/failure without duplicate side effects (`src/cmd/reconcile.zig`, `src/state/outbox_reconciler.zig`, `src/state/machine.zig`)
- 2.2 DONE Unit test: trace fields are present across HTTP/worker paths (`src/observability/trace.zig` W3C round-trip + generate/child tests)
- 2.3 DONE Unit test: key rotation path preserves decryptability during transition (`src/secrets/crypto.zig` KEK versioned tests)
- 2.4 DONE Unit test: Langfuse + OTEL exporters fail deterministically (non-fatal error path with explicit error classification via `expectError(RequestFailed, ...)`)

---

## 3.0 Acceptance Criteria

**Status:** DONE

- [x] 3.1 Deferred hardening dimensions are implemented and test-backed
- [x] 3.2 Runtime observability and config hygiene stay deterministic under failure
- [x] 3.3 Demo evidence captured for replay + trace + rotation checks

---

## 4.0 Out of Scope

- Full distributed tracing backend operations runbook
- Dashboard/UI observability features

---

## 5.0 Standard Error Codes (Troubleshooting Contract)

**Status:** DONE

Error-code format (standardized for docs and agent remediation):
- `UZ-<DOMAIN>-<COMPONENT>-<NNN>`
- Example domains: `OBS`, `CFG`, `STATE`

M4_005 codes:
- `UZ-OBS-LANGFUSE-001`
  - Emitted from: `src/observability/langfuse.zig` (`emitTrace` catch path)
  - Meaning: Langfuse export failed (`RequestFailed` or non-success status)
  - Deterministic behavior: warning logged; run continues (no crash, no run failure)
  - Auto-remediation playbook: validate `LANGFUSE_HOST`, `LANGFUSE_PUBLIC_KEY`, `LANGFUSE_SECRET_KEY`, endpoint reachability, retry on next stage emission
- `UZ-OBS-OTEL-001`
  - Emitted from: `src/observability/otel_export.zig` (`exportMetricsSnapshotBestEffort` catch path)
  - Meaning: OTLP metrics export failed (`RequestFailed` or non-success status)
  - Deterministic behavior: warning logged; reconcile/worker flow continues
  - Auto-remediation playbook: validate `OTEL_EXPORTER_OTLP_ENDPOINT` reachability and collector status; next tick/run re-attempts export

---

## 6.0 Evidence Bundle

**Status:** DONE

Commands run:
1. `make lint`
2. `make test`
3. `zig test src/observability/langfuse.zig --test-filter "postJsonWithBasicAuth"`

Key outputs:
- `make lint`:
  - `✓ [zombied] Lint passed`
  - `✓ [website] Lint passed`
  - `✓ All lint checks passed`
- `make test`:
  - `Build Summary: 9/9 steps succeeded; 134/134 tests passed`
  - `✓ [zombied] Unit + integration passed`
  - `✓ [zombiectl] Unit tests passed`
  - `✓ [website] Unit tests passed`
  - `✓ [app] Unit tests passed`
  - `✓ Full test suite passed`
- Focused failure determinism check:
  - `1/1 langfuse.test.postJsonWithBasicAuth returns RequestFailed when endpoint unreachable...OK`
  - `All 1 tests passed.`

What proves each acceptance criterion:
- AC 3.1: Implementation landed across outbox reconciler, trace model, OTEL exporter, Langfuse exporter, and KEK rotation config paths; covered by backend + integration test lane in `make test`.
- AC 3.2: Failure paths are deterministic and non-fatal:
  - OTEL/Langfuse exporter failures are converted into warn logs with stable error codes (`UZ-OBS-OTEL-001`, `UZ-OBS-LANGFUSE-001`), not propagated as runtime-fatal errors.
  - Reconciler restart/rollback robustness is covered in existing integration tests under `src/cmd/reconcile.zig`.
- AC 3.3: Demo evidence captured above via exact command outputs and focused failure-path test output.

Verification boundary note:
- `make build` was not run in final verification by explicit user instruction on Mar 12, 2026.

---

## 7.0 Coverage Audit Addendum (M4_005)

**Status:** DONE
**Date:** Mar 12, 2026: 11:10 PM

Scope audited:
- `src/observability/*`
- `src/state/*`
- `src/cmd/reconcile.zig`

Audit method:
1. Enumerate existing tests in target modules (`rg "^test \""`).
2. Inspect branch-heavy code paths in observability/reconciler/state functions.
3. Add unit tests for uncovered high-risk branches.
4. Re-run deterministic verification gates.

Uncovered high-risk branches found (before this addendum):
- `daemonHealthy` missing explicit branch check for daemon-not-running path (`running == false`) in `src/cmd/reconcile.zig`.
- Argument parser helpers lacked non-numeric rejection tests (`parseU64Arg`, `parseU16Arg`) in `src/cmd/reconcile.zig`.
- HTTP exporter status gates lacked explicit non-2xx checks (`isSuccessStatus`) in both `src/observability/langfuse.zig` and `src/observability/otel_export.zig`.
- OTLP conversion path lacked explicit assertion that Prometheus histogram helper series (`_bucket/_sum/_count`) are excluded from OTLP payload mapping.
- Trace context helper lacked explicit test for root vs child parent-span slice behavior (`parentSpanIdSlice`) in `src/observability/trace.zig`.
- Side-effect reconciliation label/reason fallbacks in `src/state/machine.zig` lacked direct branch tests for non-reconcile states and all outbox labels.

Tests added to close gaps:
- `src/cmd/reconcile.zig`
  - `parseU64Arg rejects non-numeric values`
  - `parseU16Arg rejects non-numeric values`
  - `daemonHealthy returns false when daemon not running`
- `src/observability/langfuse.zig`
  - `isSuccessStatus accepts 2xx and rejects non-2xx`
- `src/observability/otel_export.zig`
  - `isSuccessStatus accepts 2xx and rejects non-2xx`
  - `renderOtlpJson excludes histogram helper series from prometheus text`
- `src/observability/trace.zig`
  - `parentSpanIdSlice returns empty for root and span id for child`
- `src/state/machine.zig`
  - `dead-letter reconciliation reason falls back for non-reconcile target states`
  - `outbox status labels are stable`

Verification evidence:
1. `make lint`
   - `✓ [zombied] Lint passed`
   - `✓ [website] Lint passed`
   - `✓ All lint checks passed`
2. `make test`
   - `Build Summary: 9/9 steps succeeded; 140/140 tests passed`
   - `✓ [zombied] Unit + integration passed`
   - `✓ [zombiectl] Unit tests passed`
   - `✓ [website] Unit tests passed`
   - `✓ [app] Unit tests passed`
   - `✓ Full test suite passed`
3. `zig build test --summary all` (with local Zig cache dirs in repo)
   - `Build Summary: 9/9 steps succeeded; 140/140 tests passed`
   - `test success`

Coverage audit outcome:
- All identified high-risk uncovered branches in the audited scope now have explicit unit tests.
- No runtime behavior changes were required; this addendum is test/evidence hardening only.
