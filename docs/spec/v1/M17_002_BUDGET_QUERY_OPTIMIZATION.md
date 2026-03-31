# M17_002: Budget Query Optimization

**Prototype:** v1.0.0
**Milestone:** M17
**Workstream:** 002
**Date:** Mar 31, 2026
**Status:** PENDING
**Priority:** P1 — monthly SUM query carries a JOIN through runs; unused index limits throughput at scale
**Batch:** B2 — follows M17_001 landing
**Depends on:** M17_001

---

## 1.0 Usage Ledger Direct Query

**Status:** PENDING

The current `enforceWorkspaceMonthlyBudget` query joins `billing.usage_ledger` through
`core.runs` to reach `workspace_id` and uses a timestamp expression over `runs.created_at`.
This JOIN prevents the planner from using `idx_usage_ledger_workspace (workspace_id, created_at)`
on `usage_ledger` and forces a full scan across all runs for the workspace.

The fix is to add `workspace_id` directly to `usage_ledger` (already present as a column per
M17_001 schema) and rewrite the SUM query to filter by `workspace_id` + an epoch-ms lower bound
computed once by the caller, making the index usable.

**Dimensions:**
- 1.1 PENDING Confirm `workspace_id` column exists on `billing.usage_ledger`; add via migration if absent
- 1.2 PENDING Rewrite monthly SUM query: `WHERE ul.workspace_id = $1 AND ul.created_at >= $2` using precomputed epoch-ms month boundary
- 1.3 PENDING Verify `EXPLAIN ANALYZE` uses `idx_usage_ledger_workspace` index (index scan, not seq scan)
- 1.4 PENDING Backfill `workspace_id` on existing `usage_ledger` rows via `UPDATE ... FROM runs` if column was added

---

## 2.0 Caller-Side Epoch Boundary

**Status:** PENDING

Move the `date_trunc('month', now())` computation to the caller in Zig so PostgreSQL receives
a concrete `bigint` parameter rather than an expression. This eliminates the per-call function
evaluation and makes the query stable for query-plan caching.

**Dimensions:**
- 2.1 PENDING Compute `month_start_ms: i64` in `enforceWorkspaceMonthlyBudget` using `std.time.milliTimestamp()` and epoch arithmetic (truncate to first of month UTC)
- 2.2 PENDING Pass `month_start_ms` as `$2` parameter; remove `to_timestamp(r.created_at / 1000.0)` and `date_trunc` from SQL
- 2.3 PENDING Add unit test: epoch boundary calculation matches `date_trunc('month', now() AT TIME ZONE 'UTC')` for representative dates
- 2.4 PENDING Confirm no regression in existing `enforceWorkspaceMonthlyBudget` integration tests

---

## 3.0 Index Coverage Verification

**Status:** PENDING

Add a migration-time check and a make target that surfaces missing index coverage before
production load exposes it.

**Dimensions:**
- 3.1 PENDING Add `EXPLAIN (ANALYZE, FORMAT JSON)` snapshot test that asserts `Index Scan` on `idx_usage_ledger_workspace` for the rewritten query
- 3.2 PENDING Add composite index `idx_usage_ledger_workspace_created (workspace_id, created_at)` if current index does not cover both columns
- 3.3 PENDING Document index in `docs/schema-indexes.md` with query pattern and expected plan

---

## 4.0 Acceptance Criteria

**Status:** PENDING

- [ ] 4.1 `EXPLAIN ANALYZE` on `enforceWorkspaceMonthlyBudget` query shows Index Scan (not Seq Scan) on `usage_ledger`
- [ ] 4.2 No JOIN through `core.runs` in the monthly SUM path
- [ ] 4.3 `usage_ledger.workspace_id` is populated for all rows (backfill confirmed or not needed)
- [ ] 4.4 Existing budget integration tests pass unchanged
- [ ] 4.5 `make lint && make test` green

---

## 5.0 Out of Scope

- Partitioning `usage_ledger` by month (separate milestone)
- Materialized views for budget reporting (separate milestone)
- Changing the budget enforcement semantics or concurrency model
