# M28_001: Agent Run Observability — Grafana Full Stack

**Prototype:** v1.0.0
**Milestone:** M28
**Workstream:** 001
**Date:** Apr 05, 2026
**Status:** DONE
**Priority:** P1 — Operational necessity; without this, production incidents (slow runs, token overuse, agent loops) cannot be diagnosed without direct DB access
**Batch:** B1
**Branch:** feat/m28-001-agent-run-observability
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

**Status:** DONE

Every run should have a **root span** (`run.execute`) with all agent call spans as children. Today all agent spans have `parent_span_id = null` — they appear as unrelated in Tempo and cannot be grouped by run.

**Dimensions:**
- 1.1 DONE Root span created at top of `executeRun()` via `TraceContext.generate()`, closed in `defer` with final attrs
- 1.2 DONE `emitAgentSpan()` takes `root_span_id` param, sets `tc.parent_span_id` to it
- 1.3 DONE Root span closed in `defer` block with `run.outcome` (done/blocked/error/cancelled), `run.total_tokens`, `run.total_wall_seconds`
- 1.4 DONE Gate tool spans emitted per execution in `worker_gate_loop.zig` via `emitGateSpan()` as children of root span
- 1.5 DONE Tempo query `{run.id="<run_id>"}` returns complete trace tree (root + all agent spans + gate spans)

---

## 2.0 Per-Workspace Metric Dimensions

**Status:** DONE

**Deep module:** `src/observability/metrics_workspace.zig` — bounded hash map (4096 slots, open addressing, CAS on first write, atomic counters). Overflow to `_other` bucket.

**Dimensions:**
- 2.1 DONE `zombie_agent_tokens_by_workspace_total{workspace_id}` counter
- 2.2 DONE `zombie_runs_completed_by_workspace_total{workspace_id}` and `zombie_runs_blocked_by_workspace_total{workspace_id}` counters
- 2.3 DONE `zombie_gate_repair_loops_by_workspace_total{workspace_id}` counter
- 2.4 DONE Fixed-capacity map (4096 slots, open addressing), overflow to `_other` bucket
- 2.5 DONE Prometheus render includes per-workspace metrics with correct `{workspace_id="..."}` label syntax
- 2.6 DONE `zombie_workspace_metrics_overflow_total` counter tracks cardinality overflow

---

## 3.0 `usage_ledger` Grafana Datasource Panels

**Status:** DONE

**Dimensions:**
- 3.1 DONE Grafana datasource config documented in `docs/grafana/README.md`
- 3.2 DONE Reference dashboard JSON (`docs/grafana/agent_run_breakdown.json`) with 7 panels
- 3.3 DONE Dashboard importable via Grafana API with template variables (workspace_id, time range)
- 3.4 DONE Playbook: `playbooks/009_grafana_observability/001_playbook.md` with gates in `playbooks/gates/m28_001/`

---

## 4.0 Gate Loop Distribution Metric

**Status:** DONE

**Dimensions:**
- 4.1 DONE `zombie_gate_repair_loops_per_run` histogram (buckets: 0, 1, 2, 3, 5, 10)
- 4.2 DONE Histogram observed in `handleDoneOutcome` and `handleGateExhaustedOutcome` when `gate_loop_count > 0`
- 4.3 DONE Existing `zombie_gate_repair_loops_total` counter retained

---

## 5.0 `zombiectl workspace billing` CLI (Deferred from M27_002 dim 2.3)

**Status:** DONE

**Dimensions:**
- 5.1 DONE `GET /v1/workspaces/:id/billing/summary?period=30d` returns grouped counts
- 5.2 DONE Route in `router.zig`, handler in `workspaces_billing_summary.zig`
- 5.3 DONE `zombiectl workspace billing --workspace-id <id>` in `workspace_billing_summary.js`
- 5.4 DONE `--json` flag renders raw JSON response
- 5.5 DONE Score-gated row shows average score via LEFT JOIN `agent_run_scores`
- 5.6 DONE Unit test in `zombiectl/test/workspace_billing_summary.unit.test.js` (5 tests, all pass)
- 5.7 DONE `--period` option accepts `7d`, `30d`, `90d`
- 5.8 DONE API response includes `period_start_ms` and `period_end_ms`

---

## 6.0 Acceptance Criteria

**Status:** DONE

- [x] 6.1 Tempo: root span + child agent/gate spans (emitAgentSpan with root_span_id, emitGateSpan)
- [x] 6.2 Prometheus: `zombie_agent_tokens_by_workspace_total` per-workspace (metrics_workspace.zig)
- [x] 6.3 Dashboard JSON importable with 7 panels + playbook gate
- [x] 6.4 `zombiectl workspace billing` outputs distinct rows
- [x] 6.5 `zombie_gate_repair_loops_per_run` histogram with custom GateLoopBuckets
- [x] 6.6 Build clean; CLI tests pass (5/5)

---

## 7.0 Out of Scope

- Real-time alerting on token spend (future alert manager integration)
- Per-run Prometheus metrics with `run_id` label (unbounded cardinality — use Tempo traces instead)
- Billing invoice generation or payment portal
- Historical backfill of span data for runs before this milestone
- Per-stage latency SLOs or budget alerts
