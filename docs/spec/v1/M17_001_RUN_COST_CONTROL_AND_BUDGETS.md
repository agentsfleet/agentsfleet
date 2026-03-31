# M17_001: Run Cost Control and Budget Enforcement

**Prototype:** v1.0.0
**Milestone:** M17
**Workstream:** 001
**Date:** Mar 28, 2026
**Status:** IN_PROGRESS
**Branch:** feat/m17-001-run-cost-control
**Priority:** P1 — A single malformed spec can burn unlimited tokens or wall time today; no enforcement boundary exists at the run or workspace level
**Batch:** B3
**Depends on:** M16_001 (Gate Loop — runs must exist before budgeting them)

---

## 1.0 Per-Run Limits

**Status:** PENDING

Each run carries configurable limits stored on the run row at submission time. Limits are sourced from the agent profile with hardcoded defaults. After each gate loop iteration the worker checks all three limits and terminates the run with a structured failure reason if any is exceeded. Limits are immutable once the run is enqueued — they cannot be updated mid-flight.

Default limits: `max_repair_loops = 3`, `max_tokens = 100_000`, `max_wall_time = 600s` (10 minutes).

**Dimensions:**
- 1.1 PENDING Add `max_repair_loops`, `max_tokens`, and `max_wall_time_seconds` columns to the `runs` table; populate from agent profile defaults at submission time
- 1.2 PENDING Worker checks all three limits after each gate loop iteration; if any limit is exceeded, transition run to `FAILED` with reason `TOKEN_BUDGET_EXCEEDED`, `WALL_TIME_EXCEEDED`, or `REPAIR_LOOPS_EXHAUSTED` respectively
- 1.3 PENDING Prometheus counters `zombied_run_limit_exceeded_total{reason}` for each terminal limit type
- 1.4 PENDING Structured log line on limit breach includes run_id, workspace_id, limit type, actual value, and configured threshold

---

## 2.0 Workspace Budget

**Status:** PENDING

Each workspace carries a monthly token budget derived from its billing plan (free plan credit from M6_002). The budget check is a synchronous gate at run submission, not a deferred check during execution. Runs that would exceed the remaining monthly budget are rejected before they are enqueued, with a clear error message returned to the caller.

**Dimensions:**
- 2.1 PENDING `workspace_monthly_token_budget` column on `workspaces` table; free plan default sourced from M6_002 plan definition
- 2.2 PENDING `workspace_monthly_token_usage` materialized from the `runs` table (sum of `tokens_used` for the current calendar month, terminal runs only)
- 2.3 PENDING Budget check at submission: if `monthly_token_usage + run.max_tokens > monthly_token_budget`, reject with HTTP 402 and body `{"error": "workspace_budget_exceeded", "remaining_tokens": N}`
- 2.4 PENDING Budget check is transactional (`SELECT ... FOR UPDATE` on workspace row) to prevent concurrent submissions from double-spending the same budget

---

## 3.0 Run Cancellation

**Status:** PENDING

Operators can cancel an active or queued run via `zombiectl runs cancel <id>`. The cancel signal is delivered through Redis so the worker receives it without polling the database on every tick. The worker checks for cancellation between gate loop iterations. For runs actively executing an agent, the worker issues an Executor `DestroyExecution` RPC before marking the run terminal.

**Dimensions:**
- 3.1 PENDING `zombiectl runs cancel <id>` publishes a cancel signal key to Redis (`run:cancel:<run_id>`) with a TTL of 1 hour
- 3.2 PENDING Worker checks `EXISTS run:cancel:<run_id>` in Redis after each gate loop iteration; on match, issues `DestroyExecution` RPC to the executor and transitions run to `CANCELLED`
- 3.3 PENDING `CANCELLED` is a distinct terminal state (not `FAILED`) — billing finalizes as non-billable, scoring records outcome `cancelled`
- 3.4 PENDING `zombiectl runs cancel` returns an error if the run is already in a terminal state (`FAILED`, `CANCELLED`, `COMPLETE`)

---

## 4.0 Acceptance Criteria

**Status:** PENDING

- [ ] 4.1 A run that exceeds `max_tokens` is terminated mid-flight and lands in `FAILED` with reason `TOKEN_BUDGET_EXCEEDED`; no further gate loop iterations execute
- [ ] 4.2 A run submitted when the workspace has insufficient monthly token budget is rejected at submission with HTTP 402 before any Redis enqueue occurs
- [ ] 4.3 `zombiectl runs cancel <id>` cancels an active run within one gate loop iteration; run lands in `CANCELLED`, not `FAILED`
- [ ] 4.4 Prometheus counter `zombied_run_limit_exceeded_total` increments for each breached limit type

---

## 5.0 Out of Scope

- Queue fairness and priority scheduling across workspaces
- Per-agent model fallback on budget exhaustion (e.g. downgrade to cheaper model)
- Per-agent token budgets (only run-level and workspace-level budgets in this workstream)
- Real-time budget consumption streaming to the client
- Cross-workspace budget pooling or enterprise seat management
