---
Milestone: M10
Workstream: M10_004
Name: SCORING_AND_BILLING_SUMMARY_CLEANUP
Status: PENDING
Priority: P0 — billing summary handler queries dropped tables (will 500)
Created: Apr 11, 2026
Depends on: M10_001 (pipeline v1 removal)
---

# M10_004 — Scoring System and Billing Summary Cleanup

## Goal

Remove the dead agent scoring system (zero callers in zombie paths) and fix
the billing summary handler that queries dropped tables.

## Problem

The agent scoring pipeline (`src/pipeline/scoring.zig`, deleted) was the only
caller of scoring metrics, PostHog events, and DB writes to `agent_run_scores`.
The zombie execution model has no scoring step — the LLM agent executes
dynamically. All scoring infrastructure is dead:

- **Runtime bug:** `workspaces_billing_summary.zig` queries `billing.usage_ledger`
  (dropped) and `scoring.agent_run_scores` (dropped). Any GET request to
  `/v1/workspaces/:id/billing/summary` will 500.
- **Dead scoring counters:** `incAgentScoreComputed`, `incAgentScoringFailed`,
  `setAgentScoreLatest`, `observeAgentScoringDurationMs` — zero callers.
- **Dead scoring PostHog events:** `trackAgentRunScored`, `trackAgentScoringFailed`,
  `trackAgentTrustEarned`, `trackAgentTrustLost`, `trackAgentImprovementStalled`,
  `trackAgentHarnessChanged` — zero callers.
- **Dead scoring Prometheus render:** `agent_score_computed_*`, `agent_scoring_*`
  gauges/counters permanently zero in `/metrics` output.
- **Stale test assertions:** `common.zig:209-210` assert `agent_run_scores` and
  `agent_run_analysis` tables exist in migration SQL that is now a version marker.

## Immediate Fix (this commit)

- `workspaces_billing_summary.zig` — stubbed to return zeros (tables dropped)
- `common.zig:209-210` — updated assertions to match version marker content

## Remaining Scope

| Item | File | Action |
|------|------|--------|
| Dead scoring counter atomic vars (6) | `metrics_counters.zig` | Remove vars + functions |
| Dead scoring counter re-exports | `metrics.zig` | Remove pub re-exports |
| Dead scoring Prometheus render lines | `metrics_render.zig` | Remove render calls |
| Dead scoring PostHog functions (6+) | `posthog_events.zig` | Remove functions |
| Dead scoring PostHog tests | `posthog_events_test.zig` | Remove test cases |
| Dead scoring metrics tests | `metrics.zig` tests | Remove assertions |
| Scoring config in workspace_entitlements | `entitlements.zig` | Evaluate: keep or remove |
| Scoring config fields in billing | `workspace_billing.zig` | Evaluate: keep or remove |
| Stale CASCADE comment | `test_fixtures.zig:52` | Remove agent_run_* refs |

## Out of Scope

- Billing summary rewrite for zombie credit data — covered by M15_001
- Schema table drops (018, 019 already version markers) — already done
- PostHog run events (trackRunStarted, etc.) — covered by M10_002

## Acceptance Criteria

- [ ] GET /v1/workspaces/:id/billing/summary returns 200 with zeros (not 500)
- [ ] `grep -rn incAgentScoreComputed src/ | grep -v test | grep -v metrics` returns 0
- [ ] `grep -rn trackAgentRunScored src/ | grep -v test` returns 0
- [ ] `make test` passes
- [ ] `make lint` passes
- [ ] Cross-compiles
