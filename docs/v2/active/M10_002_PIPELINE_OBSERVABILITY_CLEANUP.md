---
Milestone: M10
Workstream: M10_002
Name: PIPELINE_OBSERVABILITY_CLEANUP
Status: IN_PROGRESS
Priority: P2 — cosmetic; no runtime errors, no data loss
Created: Apr 11, 2026
Started: Apr 11, 2026
Branch: feat/m10-pipeline-observability-cleanup
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

## Applicable Rules

- RULE NDC — no dead code (primary driver)
- RULE ORP — cross-layer orphan sweep after deletion
- RULE XCC — cross-compile before commit
- RULE FLL — 350-line gate on touched files

## Invariants

N/A — deletion-only spec, no new compile-time guardrails.

## Eval Commands

```bash
# E1: Zero run counters in metrics output
grep -c "runs_created_total\|runs_completed_total\|runs_blocked_total\|run_retries_total" src/observability/metrics_counters.zig
echo "E1: run counter refs (should be 0)"

# E2: Zero trackRun* PostHog functions
grep -rn "trackRunStarted\|trackRunRetried\|trackRunCompleted\|trackRunBlocked" src/ --include="*.zig" | grep -v _test | head -5
echo "E2: trackRun refs (empty = pass)"

# E3: Zero queue_depth/queue_age gauge refs
grep -rn "queue_depth\|oldest_queued_age" src/observability/ --include="*.zig" | head -5
echo "E3: queue gauge refs (empty = pass)"

# E4: Build + test + lint + cross-compile
zig build 2>&1 | head -5; echo "build=$?"
zig build test 2>&1 | tail -5; echo "test=$?"
make lint 2>&1 | grep -E "✓|FAIL"
zig build -Dtarget=x86_64-linux 2>&1 | tail -3; echo "x86=$?"
zig build -Dtarget=aarch64-linux 2>&1 | tail -3; echo "arm=$?"

# E5: Memory leak check
zig build test 2>&1 | grep -i "leak" | head -5
echo "E5: leak check (empty = pass)"

# E6: Gitleaks
gitleaks detect 2>&1 | tail -3; echo "gitleaks=$?"

# E7: 350-line gate
git diff --name-only origin/main | grep -v '\.md$' | xargs wc -l 2>/dev/null | awk '$1 > 350 { print "OVER: " $2 ": " $1 }'
```

## Dead Code Sweep

| Deleted symbol | Grep command | Expected |
|---------------|--------------|----------|
| `runs_created_total` | `grep -rn "runs_created_total" src/ --include="*.zig"` | 0 matches |
| `trackRunStarted` | `grep -rn "trackRunStarted" src/ --include="*.zig"` | 0 matches |
| `trackRunCompleted` | `grep -rn "trackRunCompleted" src/ --include="*.zig"` | 0 matches |
| `trackRunRetried` | `grep -rn "trackRunRetried" src/ --include="*.zig"` | 0 matches |
| `trackRunFailed` | `grep -rn "trackRunFailed" src/ --include="*.zig"` | 0 matches |
| `queue_depth` param | `grep -rn "queue_depth" src/observability/ --include="*.zig"` | 0 matches |

## Verification Evidence

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Unit tests | `make test` | | |
| Leak detection | `zig build test \| grep leak` | | |
| Cross-compile | `zig build -Dtarget=x86_64-linux` | | |
| Lint | `make lint` | | |
| Gitleaks | `gitleaks detect` | | |
| 350L gate | `wc -l` (exempts .md) | | |
| Dead code sweep | eval E1–E3 | | |

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
