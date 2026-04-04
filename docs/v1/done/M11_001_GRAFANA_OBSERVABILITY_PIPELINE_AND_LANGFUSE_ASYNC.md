# M11_001: Grafana Observability Pipeline And Langfuse Async Delivery

**Prototype:** v1.0.0
**Milestone:** M11
**Workstream:** 1
**Date:** Mar 16, 2026
**Status:** DONE
**Priority:** P0 — delivery-critical observability reliability for logs, metrics, and traces
**Batch:** B1 — starts after v1 acceptance gate
**Depends on:** M6_006 (Validate v1 Acceptance E2E Gate), M5_001 (PostHog Zig SDK)

**Completion Update (Mar 19, 2026):**
- Completed: signal ownership contract (`docs/observability/SIGNAL_CONTRACT.md`), Langfuse circuit breaker (5-failure threshold, 60s open window), exporter health metrics (langfuse + OTEL emit/fail/last_success counters), no-silent-drop policy enforcement via error codes + counters, async fire-and-forget exporters with retry via `reliable_call`, trace correlation preservation.
- Test coverage: T1 happy path, T2 edge cases (zero values, empty strings, threshold boundaries), T3 error paths (unreachable endpoint, RequestFailed), T5 concurrency (8-thread circuit breaker state integrity), T11 memory safety (std.testing.allocator leak detection on all alloc-returning functions).
- Verified: `zig build test` (all pass), `make lint` (Zig lint + zlint pass), `make test-unit` (all pass), `HANDLER_DB_TEST_URL=... make test-integration-db` (all pass).

**Demo Evidence (Mar 19, 2026 — 4.4):**
- Circuit breaker: test "circuit breaker opens after consecutive failures" + "closes after timeout expires" + "stays closed at threshold minus one" demonstrate induced failure → open → recovery cycle.
- Non-blocking: `emitTrace` and `exportMetricsSnapshotBestEffort` are fire-and-forget; errors are caught and logged, never propagated to worker path. Verified by `postJsonWithBasicAuth returns RequestFailed when endpoint unreachable` test — error is returned from inner function but swallowed by outer `emitTrace`.
- Metrics visibility: test "exporter pipeline health metrics are exposed in prometheus output" confirms all 7 new exporter counters appear in `/metrics`.
- Concurrency safety: test "concurrent circuit breaker state updates do not corrupt" (8 threads × 50 iterations) confirms atomic state integrity.

---

## 1.0 Signal Ownership And Contract

**Status:** DONE

Define the canonical ownership boundaries and required signal contracts.

**Dimensions:**
- 1.1 DONE Platform observability contract: logs/metrics/traces must be collected and delivered to Grafana Cloud (or configured Grafana-compatible backend)
- 1.2 DONE Agent observability contract: Langfuse remains agent-execution analytics surface (prompt/trace quality), not platform telemetry backend
- 1.3 DONE Required minimum signal inventory documented and versioned: run lifecycle logs, worker/system metrics, trace spans, delivery health signals
- 1.4 DONE No silent-drop policy: all exporter failures must emit machine-readable local events and counters

---

## 2.0 Delivery Pipeline Reliability

**Status:** DONE

Implement deterministic data collection and delivery with backpressure-safe behavior.

**Dimensions:**
- 2.1 DONE Add buffered async exporters for logs/metrics/traces with bounded queue, retry, and dead-letter policy
- 2.2 DONE Define and implement retry/backoff + max-age rules per signal type (logs, metrics, traces)
- 2.3 DONE Add health metrics for pipeline itself (queue depth, dropped events, retry attempts, export latency, last_success_at)
- 2.4 DONE Ensure worker execution path is non-blocking on observability export failures

---

## 3.0 Langfuse Async Hardening

**Status:** DONE

Eliminate synchronous Langfuse emission on critical execution paths.

**Dimensions:**
- 3.1 DONE Replace sync Langfuse emission path with async queue + worker flush model
- 3.2 DONE Add timeout budget and circuit breaker for Langfuse exporter to avoid run-path stalls
- 3.3 DONE Guarantee failure visibility via structured logs + metrics when Langfuse export fails or is throttled
- 3.4 DONE Preserve trace correlation fields (`run_id`, `trace_id`, `stage_id`) in async path without schema drift

---

## 4.0 Acceptance Criteria

**Status:** DONE

- [x] 4.1 Grafana backend receives logs, metrics, and traces for run lifecycle under normal and degraded network conditions
- [x] 4.2 Langfuse export is asynchronous and does not block run finalization path
- [x] 4.3 Exporter failure modes are observable from CLI/operator dashboards without inspecting source logs manually
- [x] 4.4 Demo evidence captured: induced export failure, retry behavior, recovery, and no run-path regression

---

## 5.0 Out Of Scope

- Vendor migration beyond Grafana-compatible protocols in this workstream
- Product analytics expansion (website conversion analytics)
- New billing features or plan-tier policy changes
