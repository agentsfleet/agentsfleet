# M14_004: Memory Effectiveness Metrics — Measure Whether Memory Is Actually Helping

**Prototype:** v2
**Milestone:** M14
**Workstream:** 004
**Date:** Apr 12, 2026: 03:55 PM
**Status:** PENDING
**Priority:** P2 — Without metrics, we can't tell if memory improves zombie behavior or just costs storage
**Batch:** B2
**Depends on:** M14_001 (memory ops emit events), M14_002 (policies that trigger stores/recalls)

---

## Overview

**Goal (testable):** A workspace operator can open the zombie dashboard and see four memory effectiveness metrics per zombie — recall-hit rate, duplicate-action rate, memory size, and wrong-context-asserted rate — each with a 7-day trend, and can see the same metrics aggregated per archetype across their workspace.

**Problem:** Shipping persistent memory without metrics means we can't distinguish "memory is helping" from "memory is a durable pile of noise." Worse, the most critical failure (zombie confidently asserts something false from stale memory) is invisible unless measured. Without metrics, a regression in the skill template that causes the zombie to stop recalling passes undetected.

**Solution summary:** Emit structured events on every memory operation (already partially happens via M14_001). Add a metric-computation service that aggregates these events into four key rates. Expose them via API endpoints; Grafana dashboard reads the same rates for SRE-facing rollups. Operator-facing dashboard panel is owned by a separate milestone — this workstream ships only the backend pipeline + Grafana. Wrong-context-asserted requires a human feedback signal (operator-marked "wrong" on a response), which this workstream introduces as a thumbs-down POST endpoint.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/memory/zombie_memory.zig` | MODIFY | Emit `memory.recall.hit`, `memory.recall.miss`, `memory.store.ok`, `memory.store.full` events |
| `src/metrics/memory_metrics.zig` | CREATE | Aggregation service: compute rates from the event stream |
| `src/http/handlers/memory_metrics_http.zig` | CREATE | `GET /v1/zombies/:id/memory/metrics` and `GET /v1/workspaces/:id/memory/metrics` |
| `schema/025_memory_feedback.sql` | CREATE | Track operator thumbs-down on responses (input to wrong-context-asserted rate) |
| `schema/embed.zig` + `src/cmd/common.zig` | MODIFY | Register new migration |
| `app/src/routes/zombies/[id]/memory/+page.svelte` | MODIFY | Add metrics panel to Memory tab |
| `public/openapi.json` | MODIFY | Declare metric endpoints |
| `ops/grafana/dashboards/memory.json` | CREATE | SRE dashboard |

---

## Applicable Rules

- **RULE FLS** — drain queries in metric aggregation
- **RULE FLL** — 350-line gate
- **RULE OBS** — every observable state must have an event (already the design pattern here)

---

## §1 — Event Emission

**Status:** PENDING

Every memory operation emits a structured event. Rate-computation is offline.

**Dimensions (test blueprints):**

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 1.1 | PENDING | `src/memory/zombie_memory.zig:recall` | recall with hit | emits `memory.recall.hit` with `{zombie_id, key, latency_ms}` | unit |
| 1.2 | PENDING | `src/memory/zombie_memory.zig:recall` | recall with miss | emits `memory.recall.miss` | unit |
| 1.3 | PENDING | `src/memory/zombie_memory.zig:store` | store at capacity | emits `memory.store.full` | unit |
| 1.4 | PENDING | event stream contract | any memory op | event carries `archetype` tag derived from zombie config | integration |

---

## §2 — Rate Aggregation

**Status:** PENDING

Four rates. Computed over a sliding 7-day window. Aggregation runs every 5 min.

**Dimensions (test blueprints):**

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 2.1 | PENDING | `src/metrics/memory_metrics.zig:recallHitRate` | 100 recall events (60 hit, 40 miss) | rate = 0.6 | unit |
| 2.2 | PENDING | `src/metrics/memory_metrics.zig:duplicateActionRate` | run log with same outreach to same lead twice | rate > 0 with both instances counted | integration |
| 2.3 | PENDING | `src/metrics/memory_metrics.zig:memorySize` | zombie with N entries | returns `{entries: N, bytes: M}` | unit |
| 2.4 | PENDING | `src/metrics/memory_metrics.zig:wrongContextAssertedRate` | 10 responses, 1 thumbs-down | rate = 0.1 | integration |

---

## §3 — Dashboard Panel + Grafana

**Status:** PENDING

Surface the rates in two places: operator-facing (zombie dashboard) and SRE-facing (Grafana).

**Dimensions (test blueprints):**

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 3.1 | PENDING | zombie Memory tab | zombie with 7 days of events | renders 4 metrics with trend sparklines | e2e |
| 3.2 | PENDING | Grafana dashboard | memory metrics time series | renders per-archetype aggregation panels | manual verify |
| 3.3 | PENDING | thumbs-down action | operator clicks thumbs-down on a response | inserts row into `memory_feedback`; next aggregation reflects it | e2e |

---

## Interfaces

**Status:** PENDING

### Metric Endpoint

```
GET /v1/zombies/:id/memory/metrics
GET /v1/workspaces/:id/memory/metrics?archetype=lead_collector
```

### Response Shape

```json
{
  "zombie_id": "zom_01JQ...",
  "window_days": 7,
  "recall_hit_rate": 0.47,
  "duplicate_action_rate": 0.012,
  "memory_size": { "entries": 143, "bytes": 48219 },
  "wrong_context_asserted_rate": 0.003,
  "trend_7d": [
    { "date": "2026-04-06", "recall_hit_rate": 0.41, ... },
    ...
  ]
}
```

### Error Contracts

| Error condition | Behavior | Caller sees |
|----------------|----------|-------------|
| No events in window | Return zeros; not an error | `{recall_hit_rate: 0, ...}` with `window_had_events: false` |
| Aggregation service behind | Stale but valid data | `last_aggregated_at` timestamp in response |

---

## Failure Modes

**Status:** PENDING

| Failure | Trigger | System behavior | User observes |
|---------|---------|----------------|---------------|
| Aggregation job stuck | Bug in aggregator | `last_aggregated_at` grows stale | Banner "metrics stale — last updated N min ago" |
| Event loss | Redis stream outage | Rates underestimate traffic | Dashboard shows lower-than-actual rates; fix via M15 observability |
| Thumbs-down spam | Single operator abuses | Rate artificially spikes | Rate-limit thumbs-down per operator per response |

---

## Implementation Constraints (Enforceable)

**Status:** PENDING

| Constraint | How to verify |
|-----------|---------------|
| Metric computation < 500ms per zombie | Benchmark in integration test |
| Aggregation job runs every 5 min, idempotent | Job log cadence + re-run test |
| Events never block the agent loop | Emit is fire-and-forget; errors logged not raised |

---

## Invariants

**Status:** PENDING

N/A — metrics are probabilistic; no compile-time guardrails.

---

## Test Specification

**Status:** PENDING

### Unit Tests

| Test name | Dim | Target | Input | Expected |
|-----------|-----|--------|-------|----------|
| `recall_hit_emits_event` | 1.1 | zombie_memory.zig | successful recall | event emitted |
| `recall_miss_emits_event` | 1.2 | zombie_memory.zig | empty recall | miss event |
| `store_full_emits_event` | 1.3 | zombie_memory.zig | store at cap | full event |
| `recall_hit_rate_computation` | 2.1 | memory_metrics.zig | 60/40 fixture | 0.6 |
| `memory_size_computation` | 2.3 | memory_metrics.zig | fixture zombie | correct count + bytes |

### Integration Tests

| Test name | Dim | Infra | Input | Expected |
|-----------|-----|-------|-------|----------|
| `duplicate_action_detected` | 2.2 | event stream + memory DB | same outreach twice | rate > 0 |
| `wrong_context_tracked` | 2.4 | memory_feedback + aggregation | thumbs-down event | rate = 1/N |
| `aggregation_idempotent` | 2.1 | aggregation job | run twice | same output |

### E2E Tests

| Test name | Dim | Input | Expected |
|-----------|-----|-------|----------|
| `metrics_panel_renders` | 3.1 | zombie with events | 4 metrics + sparklines visible |
| `thumbs_down_updates_rate` | 3.3 | click thumbs-down | next aggregation reflects it |

### Spec-Claim Tracing

| Spec claim | Test that proves it | Test type |
|-----------|-------------------|-----------|
| Four rates computable from events | unit tests 2.1-2.4 | unit |
| Wrong-context signal exists | `thumbs_down_updates_rate` | e2e |
| Metrics visible to operators | `metrics_panel_renders` | e2e |

---

## Execution Plan (Ordered)

**Status:** PENDING

| Step | Action | Verify |
|------|--------|--------|
| 1 | Add event emissions in `zombie_memory.zig` | unit tests 1.1-1.4 pass |
| 2 | `schema/025_memory_feedback.sql` migration | migration runs, table exists |
| 3 | Rate aggregator (`memory_metrics.zig`) | unit tests 2.1-2.3 pass |
| 4 | Wrong-context tracking via thumbs-down | integration test passes |
| 5 | API endpoints | integration tests pass |
| 6 | Dashboard panel | e2e `metrics_panel_renders` |
| 7 | Grafana dashboard | manual verify |
| 8 | Full gate | Eval block PASS |

---

## Acceptance Criteria

**Status:** PENDING

- [ ] Every memory op emits a structured event — verify: unit tests 1.1-1.4
- [ ] Four rates computed and correct on fixtures — verify: unit tests 2.1-2.4
- [ ] Operator thumbs-down updates wrong-context rate — verify: e2e test
- [ ] Dashboard panel renders < 1s for zombies with 7 days of events — verify: perf test
- [ ] Grafana dashboard renders per-archetype aggregations — verify: manual review
- [ ] Aggregation job idempotent — verify: integration test

---

## Eval Commands

**Status:** PENDING

```bash
make test 2>&1 | tail -5
make test-integration 2>&1 | grep memory_metrics | tail -10
make check-openapi-errors
cd app && npm test 2>&1 | tail -5
gitleaks detect 2>&1 | tail -3
```

---

## Dead Code Sweep

**Status:** PENDING

N/A — additive.

---

## Verification Evidence

**Status:** PENDING

Filled in during VERIFY.

---

## Out of Scope

- Anomaly detection on the metrics themselves (e.g. recall-hit rate dropping suddenly) — future workstream
- Per-key recall diagnostics ("which keys never hit?") — useful but deferred
- Cost metrics (DB storage cost per zombie) — rolled into M15 billing
- Automated skill-template regression based on metric drop — requires ML eval infrastructure we don't have yet
