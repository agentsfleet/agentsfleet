# M11_001: Grafana Observability Pipeline And Langfuse Async Delivery

**Prototype:** v1.0.0
**Milestone:** M11
**Workstream:** 1
**Date:** Mar 16, 2026
**Status:** PENDING
**Priority:** P0 — delivery-critical observability reliability for logs, metrics, and traces
**Batch:** B1 — starts after v1 acceptance gate
**Depends on:** M6_006 (Validate v1 Acceptance E2E Gate), M5_001 (PostHog Zig SDK)

---

## 1.0 Signal Ownership And Contract

**Status:** PENDING

Define the canonical ownership boundaries and required signal contracts.

**Dimensions:**
- 1.1 PENDING Platform observability contract: logs/metrics/traces must be collected and delivered to Grafana Cloud (or configured Grafana-compatible backend)
- 1.2 PENDING Agent observability contract: Langfuse remains agent-execution analytics surface (prompt/trace quality), not platform telemetry backend
- 1.3 PENDING Required minimum signal inventory documented and versioned: run lifecycle logs, worker/system metrics, trace spans, delivery health signals
- 1.4 PENDING No silent-drop policy: all exporter failures must emit machine-readable local events and counters

---

## 2.0 Delivery Pipeline Reliability

**Status:** PENDING

Implement deterministic data collection and delivery with backpressure-safe behavior.

**Dimensions:**
- 2.1 PENDING Add buffered async exporters for logs/metrics/traces with bounded queue, retry, and dead-letter policy
- 2.2 PENDING Define and implement retry/backoff + max-age rules per signal type (logs, metrics, traces)
- 2.3 PENDING Add health metrics for pipeline itself (queue depth, dropped events, retry attempts, export latency, last_success_at)
- 2.4 PENDING Ensure worker execution path is non-blocking on observability export failures

---

## 3.0 Langfuse Async Hardening

**Status:** PENDING

Eliminate synchronous Langfuse emission on critical execution paths.

**Dimensions:**
- 3.1 PENDING Replace sync Langfuse emission path with async queue + worker flush model
- 3.2 PENDING Add timeout budget and circuit breaker for Langfuse exporter to avoid run-path stalls
- 3.3 PENDING Guarantee failure visibility via structured logs + metrics when Langfuse export fails or is throttled
- 3.4 PENDING Preserve trace correlation fields (`run_id`, `trace_id`, `stage_id`) in async path without schema drift

---

## 4.0 Acceptance Criteria

**Status:** PENDING

- [ ] 4.1 Grafana backend receives logs, metrics, and traces for run lifecycle under normal and degraded network conditions
- [ ] 4.2 Langfuse export is asynchronous and does not block run finalization path
- [ ] 4.3 Exporter failure modes are observable from CLI/operator dashboards without inspecting source logs manually
- [ ] 4.4 Demo evidence captured: induced export failure, retry behavior, recovery, and no run-path regression

---

## 5.0 Out Of Scope

- Vendor migration beyond Grafana-compatible protocols in this workstream
- Product analytics expansion (website conversion analytics)
- New billing features or plan-tier policy changes
