---
Milestone: M10
Workstream: M10_002
Name: PIPELINE_OBSERVABILITY_CLEANUP
Status: PENDING
Priority: P2 — cosmetic; no runtime errors, no data loss
Created: Apr 11, 2026
Depends on: M10_001 (pipeline v1 removal)
---

# M10_002 — Pipeline Observability Cleanup

## Goal

Remove pipeline-era Prometheus counters, PostHog events, and dead observability
code that survived M10_001. These emit zeros or are never called — no runtime
errors, but they pollute `/metrics` output and PostHog dashboards.

## Problem

M10_001 deleted the pipeline worker, run tables, and billing runtime. But the
observability layer still references pipeline concepts:

- **Prometheus:** `runs_created_total`, `runs_completed_total`, `runs_blocked_total`,
  `run_retries_total`, `worker_in_flight_runs`, `run_total_wall_seconds_*` —
  all permanently zero. Grafana dashboards consuming these show flatlines.
- **PostHog:** `trackRunStarted`, `trackRunCompleted`, `trackRunBlocked`,
  `trackRunRetried` — functions exist but have zero call sites.
- **serve.zig:** `trackServerStarted(ph.client, port, 0)` hardcodes
  `worker_concurrency=0` — misleading in PostHog.
- **metrics_render.zig:** `zombie_queue_depth` and `zombie_queue_oldest_age_ms`
  gauges always emit 0 (data source was `queueHealth()`, now removed).

## Scope

| Item | File | Action |
|------|------|--------|
| Dead run counters in Snapshot struct | `metrics_counters.zig` | Remove fields + atomic vars |
| Dead run counter inc/observe functions | `metrics_counters.zig` | Remove functions |
| Dead run counter re-exports | `metrics.zig` | Remove pub re-exports |
| Dead run counter render lines | `metrics_render.zig` | Remove render calls |
| Dead queue_depth / queue_age gauges | `metrics_render.zig` | Remove params + render |
| Dead PostHog track functions | `posthog_events.zig` | Remove trackRun* functions |
| Hardcoded worker_concurrency=0 | `serve.zig:223` | Remove param or pass real value |
| Dead PostHog tests for run events | `posthog_events_test.zig` | Remove test cases |
| Dead metrics tests for run counters | `metrics.zig` tests | Update assertions |
| Dead metrics histogram tests | `metrics_histograms.zig` | Remove run histograms |

## Out of Scope

- Agent scoring system — still active in zombie executor; separate evaluation
- Zombie-era metrics (M15_002 adds zombie counters — complementary, not conflicting)
- Queue depth replacement for zombie Redis streams — covered by M15_002

## Acceptance Criteria

- [ ] `curl localhost:PORT/metrics | grep -c runs_` returns 0
- [ ] `grep -rn trackRun src/observability/` returns 0 matches
- [ ] `make test` passes
- [ ] `make lint` passes
- [ ] Cross-compiles: `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux`
