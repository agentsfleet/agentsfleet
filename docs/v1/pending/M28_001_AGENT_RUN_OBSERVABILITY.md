# M28_001: Agent Run Observability — Grafana Full Stack

**Prototype:** v1.0.0
**Milestone:** M28
**Workstream:** 001
**Date:** Apr 05, 2026
**Status:** PENDING
**Priority:** P1 — Operational necessity; without this, production incidents (slow runs, token overuse, agent loops) cannot be diagnosed without direct DB access
**Batch:** B1
**Branch:**
**Depends on:** M27_002 (score-gated billing — DONE, quick wins already landed)

---

## Context

Today it is impossible to answer the following questions from Grafana/Tempo alone:

- **Why did this agent take so long on this spec?** — per-stage wall time exists in `usage_ledger` but is not queryable in Grafana without a datasource panel.
- **How many tokens did this agent consume and why?** — token totals exist in `usage_ledger` per stage, but no breakdown by stage type or run in Prometheus. Per-actor token metrics were added in M27_002 (quick wins) but workspace-level drill-down is still missing.
- **How many gate repair loops did this run do?** — counted globally in Prometheus but not correlated to a specific run or workspace.
- **Why was this run score-gated?** — the `billing.score_gate` log now has `agent_id` and `trace_id` (M27_002), but there is no Grafana dashboard to surface this.
- **Which workspace is burning the most tokens?** — no per-workspace metrics exist.

The quick wins in M27_002 added:
- `run.id`, `workspace.id`, `agent.id`, `stage.id` on OTel spans → Tempo filtering now works
- Per-actor token Prometheus counters (`zombie_agent_{echo,scout,warden,orchestrator}_tokens_total`)
- `agent_id` + `trace_id` in the `billing.score_gate` log line

This milestone completes what the quick wins could not — root run spans, per-workspace metric dimensions, `usage_ledger` Grafana panels, gate loop distribution, and the `zombiectl workspace billing` CLI surface (deferred dim 2.3 from M27_002).

---

## 1.0 Root Run Span + Span Hierarchy

**Status:** PENDING

Every run should have a **root span** (`run.execute`) with all agent call spans as children. Today all agent spans have `parent_span_id = null` — they appear as unrelated in Tempo and cannot be grouped by run.

**Dimensions:**
- 1.1 PENDING A root span is created at the start of `executeRunInternal()` in `worker_stage_executor.zig` with name `run.execute` and attrs: `run.id`, `workspace.id`, `agent.id`, `attempt`
- 1.2 PENDING All `emitAgentSpan()` calls set `tc.parent_span_id` to the root span's `span_id`
- 1.3 PENDING Root span is closed (enqueued) at the end of `executeRunInternal()` with final attrs: `run.outcome` (done/blocked/error), `run.total_tokens`, `run.total_wall_seconds`, `run.score` (if scored)
- 1.4 PENDING Gate repair spans emitted per repair attempt in `worker_gate_loop.zig` as children of the root span, with attrs: `gate.name`, `gate.attempt`, `gate.exit_code`, `gate.duration_ms`
- 1.5 PENDING Tempo query `{run.id="<run_id>"}` returns complete trace tree (root + all agent spans + gate spans) in a single waterfall

---

## 2.0 Per-Workspace Metric Dimensions

**Status:** PENDING

Prometheus metrics today have zero labels — every counter is a global aggregate. You cannot ask "which workspace is using the most tokens?" or "which workspace has the highest block rate?".

**Design constraint:** `run_id` must NOT be a label (unbounded cardinality). `workspace_id` is bounded (thousands, not millions) and is the right drill-down dimension.

**Dimensions:**
- 2.1 PENDING `zombie_agent_tokens_by_workspace_total{workspace_id}` counter — token consumption per workspace, incremented alongside existing global counter
- 2.2 PENDING `zombie_runs_completed_by_workspace_total{workspace_id}` and `zombie_runs_blocked_by_workspace_total{workspace_id}` counters
- 2.3 PENDING `zombie_gate_repair_loops_by_workspace_total{workspace_id}` counter — gate loop rate per workspace
- 2.4 PENDING Per-workspace counters use a fixed-capacity label map (max 10,000 distinct workspace IDs) with overflow to an `other` bucket — prevents unbounded memory growth
- 2.5 PENDING Prometheus render includes per-workspace metrics with correct `{workspace_id="..."}` label syntax

---

## 3.0 `usage_ledger` Grafana Datasource Panels

**Status:** PENDING

The `usage_ledger` table has per-stage token counts and wall seconds per run — the most granular data available — but it is only queryable via direct SQL today.

**Dimensions:**
- 3.1 PENDING Grafana datasource config (JSON or provisioning file) for the zombie Postgres DB is documented in `docs/grafana/` — operators can import it and query `usage_ledger` directly
- 3.2 PENDING Reference dashboard JSON (`docs/grafana/agent_run_breakdown.json`) with panels:
  - Token breakdown by stage type (`source = 'runtime_stage'`, grouped by `actor`)
  - Wall seconds by stage type
  - Score-gated run rate (`lifecycle_event = 'run_not_billable_score_gated'`) per workspace over time
  - Top-N runs by token consumption (last 24h)
- 3.3 PENDING Dashboard is importable into Grafana with no manual edits — all variables are template variables (workspace_id filter, time range)

---

## 4.0 Gate Loop Distribution Metric

**Status:** PENDING

Today `zombie_gate_repair_loops_total` is a single monotonic counter. You can compute a rate but not see the distribution (are most runs doing 1 loop or 5?).

**Dimensions:**
- 4.1 PENDING `zombie_gate_repair_loops_per_run` histogram added (buckets: 0, 1, 2, 3, 5, 10) — one observation per run that entered gate repair, value = total loops
- 4.2 PENDING Histogram is observed in `handleDoneOutcome` and `handleGateExhaustedOutcome` when `gate_loop_count > 0`
- 4.3 PENDING Existing `zombie_gate_repair_loops_total` counter is retained (backward compat for existing dashboards)

---

## 5.0 `zombiectl workspace billing` CLI (Deferred from M27_002 dim 2.3)

**Status:** PENDING

**What it does:** Shows billing breakdown for a workspace — how many runs completed (billed), how many were non-billable by reason (cancelled, retried, score-gated). This is the operator-facing surface for understanding why they were or were not charged.

**What it does NOT do:** It is not a payment portal, not a subscription management tool (that is `workspace upgrade-scale`), and not a real-time spend dashboard.

**Example output:**
```
$ zombiectl workspace billing --workspace-id ws_abc123
workspace: ws_abc123  period: last 30 days

  completed (billed):          42    agent_seconds: 1,240
  non-billable / cancelled:     8
  non-billable / retried:       5
  non-billable / score-gated:   3    avg score: 27
  ─────────────────────────────────
  total runs:                  58
```

**Dimensions:**
- 5.1 PENDING New API endpoint `GET /v1/workspaces/:id/billing/summary` returns JSON with run counts grouped by `lifecycle_event` from `usage_ledger`, plus total `agent_seconds` and `billable_quantity` for completed runs
- 5.2 PENDING Route added to `src/http/router.zig`, handler in `src/http/handlers/workspaces_billing.zig`
- 5.3 PENDING New CLI command `zombiectl workspace billing --workspace-id <id>` in `zombiectl/src/commands/workspace_billing.js` — calls new endpoint, renders table as shown above
- 5.4 PENDING `--json` flag renders raw JSON response
- 5.5 PENDING Score-gated row shows average score of gated runs (join `usage_ledger` ↔ `agent_run_scores` on `run_id`)
- 5.6 PENDING Unit test in `zombiectl/test/workspace.unit.test.js` mocking the new endpoint

---

## 6.0 Acceptance Criteria

**Status:** PENDING

- [ ] 6.1 Grafana Tempo: query `{run.id="<id>"}` shows complete trace tree with root span + all child agent/gate spans
- [ ] 6.2 Grafana Prometheus: `zombie_agent_tokens_by_workspace_total` graph shows per-workspace token breakdown
- [ ] 6.3 Grafana dashboard (imported from `docs/grafana/agent_run_breakdown.json`) renders without errors against real data
- [ ] 6.4 `zombiectl workspace billing --workspace-id <id>` outputs distinct rows for completed, non-billable (cancelled/retried/score-gated)
- [ ] 6.5 `zombie_gate_repair_loops_per_run` histogram is queryable in Prometheus — `histogram_quantile(0.95, ...)` returns a value
- [ ] 6.6 All changes build clean; existing tests pass

---

## 7.0 Out of Scope

- Real-time alerting on token spend (future alert manager integration)
- Per-run Prometheus metrics with `run_id` label (unbounded cardinality — use Tempo traces instead)
- Billing invoice generation or payment portal
- Historical backfill of span data for runs before this milestone
- Per-stage latency SLOs or budget alerts
