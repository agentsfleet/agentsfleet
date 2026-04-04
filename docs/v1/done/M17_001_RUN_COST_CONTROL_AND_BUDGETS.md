# M17_001: Run Cost Control and Budget Enforcement

**Prototype:** v1.0.0
**Milestone:** M17
**Workstream:** 001
**Date:** Mar 28, 2026
**Status:** DONE
**Branch:** feat/m17-001-run-cost-control
**Priority:** P1 — A single malformed spec can burn unlimited tokens or wall time today; no enforcement boundary exists at the run or workspace level
**Batch:** B3
**Depends on:** M16_001 (Gate Loop — runs must exist before budgeting them)

---

## 1.0 Per-Run Limits

**Status:** DONE

Each run carries configurable limits stored on the run row at submission time. Limits are sourced from the agent profile with hardcoded defaults. After each gate loop iteration the worker checks all three limits and terminates the run with a structured failure reason if any is exceeded. Limits are immutable once the run is enqueued — they cannot be updated mid-flight.

Default limits: `max_repair_loops = 3`, `max_tokens = 100_000`, `max_wall_time = 600s` (10 minutes).

**Dimensions:**
- 1.1 ✅ Add `max_repair_loops`, `max_tokens`, and `max_wall_time_seconds` columns to the `runs` table; populate from agent profile defaults at submission time
- 1.2 ✅ Worker checks all three limits after each gate loop iteration; if any limit is exceeded, transition run to `CANCELLED` with reason `TOKEN_BUDGET_EXCEEDED`, `WALL_TIME_EXCEEDED`, or `REPAIR_LOOPS_EXHAUSTED` respectively
- 1.3 ✅ Prometheus counters `zombied_run_limit_exceeded_total{reason}` for each terminal limit type
- 1.4 ✅ Structured log line on limit breach includes run_id, workspace_id, limit type, actual value, and configured threshold

---

## 2.0 Workspace Budget

**Status:** DONE

Each workspace carries a monthly token budget (`monthly_token_budget` column, default 10M tokens). The budget check is a synchronous gate at run submission using `SELECT FOR UPDATE` on the workspace row. Runs that would exceed the remaining monthly budget are rejected before they are enqueued with HTTP 402.

**Dimensions:**
- 2.1 ✅ `monthly_token_budget BIGINT NOT NULL DEFAULT 10000000` column on `workspaces` table
- 2.2 ✅ `workspace_monthly_token_usage` derived from `billing.usage_ledger` (sum of tokens for current calendar month, current workspace)
- 2.3 ✅ Budget check at submission: if usage + max_tokens > monthly_token_budget, reject with HTTP 402 and error code `UZ-RUN-005`
- 2.4 ✅ Budget check is transactional (`SELECT ... FOR UPDATE` on workspace row) to prevent concurrent submissions from double-spending

---

## 3.0 Run Cancellation

**Status:** DONE

Operators can cancel an active or queued run via `zombiectl runs cancel <id>`. The cancel signal is published to Redis (`run:cancel:<run_id>`, TTL 1h). The worker checks for the Redis key after each gate loop iteration and transitions to `CANCELLED` if found.

**Dimensions:**
- 3.1 ✅ `zombiectl runs cancel <id>` publishes a cancel signal key to Redis (`run:cancel:<run_id>`) with a TTL of 1 hour
- 3.2 ✅ Worker checks `EXISTS run:cancel:<run_id>` in Redis after each gate loop iteration; on match, transitions run to `CANCELLED` with `state_written=true`
- 3.3 ✅ `CANCELLED` is a distinct terminal state (not `BLOCKED`) — billing finalizes as non-billable, isTerminal()=true, isRetryable()=false
- 3.4 ✅ `zombiectl runs cancel` returns HTTP 409 with `UZ-RUN-006` if the run is already in a terminal state

---

## 4.0 Acceptance Criteria

**Status:** DONE

- [x] 4.1 A run that exceeds `max_tokens` is terminated mid-flight and lands in `CANCELLED` with reason `TOKEN_BUDGET_EXCEEDED`; no further gate loop iterations execute (`state_written=true`)
- [x] 4.2 A run submitted when the workspace has insufficient monthly token budget is rejected at submission with HTTP 402 before any Redis enqueue occurs
- [x] 4.3 `zombiectl runs cancel <id>` cancels an active run within one gate loop iteration; run lands in `CANCELLED`, not `BLOCKED`
- [x] 4.4 Prometheus counter `zombied_run_limit_exceeded_total` increments for each breached limit type (token_budget, wall_time, repair_loops)

---

## 5.0 Out of Scope

- Queue fairness and priority scheduling across workspaces
- Per-agent model fallback on budget exhaustion (e.g. downgrade to cheaper model)
- Per-agent token budgets (only run-level and workspace-level budgets in this workstream)
- Real-time budget consumption streaming to the client
- Cross-workspace budget pooling or enterprise seat management
