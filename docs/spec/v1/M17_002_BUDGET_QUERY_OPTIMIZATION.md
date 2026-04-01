# M17_002: Budget Query Optimization

**Prototype:** v1.0.0
**Milestone:** M17
**Workstream:** 002
**Date:** Mar 31, 2026
**Status:** DONE
**Priority:** P1 — monthly SUM query carries a JOIN through runs; unused index limits throughput at scale
**Batch:** B2 — follows M17_001 landing
**Depends on:** M17_001
**Branch:** feat/M17-002-budget-query-opt

---

## 1.0 Usage Ledger Direct Query

**Status:** DONE

The current `enforceWorkspaceMonthlyBudget` query joins `billing.usage_ledger` through
`core.runs` to reach `workspace_id` and uses a timestamp expression over `runs.created_at`.
This JOIN prevents the planner from using `idx_usage_ledger_workspace (workspace_id, created_at)`
on `usage_ledger` and forces a full scan across all runs for the workspace.

The fix is to add `workspace_id` directly to `usage_ledger` (already present as a column per
M17_001 schema) and rewrite the SUM query to filter by `workspace_id` + an epoch-ms lower bound
computed once by the caller, making the index usable.

**Dimensions:**
- 1.1 DONE Confirm `workspace_id` column exists on `billing.usage_ledger`; add via migration if absent
- 1.2 DONE Rewrite monthly SUM query: `WHERE ul.workspace_id = $1 AND ul.created_at >= $2` using precomputed epoch-ms month boundary
- 1.3 DONE Verify `EXPLAIN ANALYZE` uses `idx_usage_ledger_workspace` index — confirmed: `Index Scan using idx_usage_ledger_workspace on usage_ledger`
- 1.4 DONE Backfill `workspace_id` on existing rows — NOT NULL constraint confirms all rows populated; no backfill needed

---

## 2.0 Caller-Side Epoch Boundary

**Status:** DONE

Move the `date_trunc('month', now())` computation to the caller in Zig so PostgreSQL receives
a concrete `bigint` parameter rather than an expression. This eliminates the per-call function
evaluation and makes the query stable for query-plan caching.

**Dimensions:**
- 2.1 DONE Compute `month_start_ms: i64` in `monthStartMs()` (extracted to `start_budget.zig`) using `std.time.epoch` arithmetic
- 2.2 DONE Pass `month_start_ms` as `$2` parameter; `date_trunc` and `EXTRACT(EPOCH ...)` removed from SQL
- 2.3 DONE Unit test in `start_budget.zig`: round-trips epoch boundary and asserts `day_index == 0` and midnight alignment
- 2.4 DONE DB integration tests pass: `make test-integration-db` green against live Postgres

---

## 3.0 Index Coverage Verification

**Status:** DONE

**Dimensions:**
- 3.1 DONE EXPLAIN ANALYZE evidence: `Index Scan using idx_usage_ledger_workspace on usage_ledger ul` with `Index Cond: (workspace_id = $1 AND created_at >= $2)` — planning 0.9 ms, execution 0.06 ms
- 3.2 DONE Composite index `idx_usage_ledger_workspace (workspace_id, created_at DESC)` already covers both columns — no new index needed
- 3.3 OUT_OF_SCOPE Document index in `docs/schema-indexes.md` — index is self-documenting in `schema/001_initial.sql`; separate file would drift

---

## 4.0 Acceptance Criteria

**Status:** DONE

- [x] 4.1 `EXPLAIN ANALYZE` shows `Index Scan using idx_usage_ledger_workspace` (not Seq Scan) on `usage_ledger`
- [x] 4.2 No JOIN through `core.runs` in the monthly SUM path
- [x] 4.3 `usage_ledger.workspace_id` NOT NULL — all rows populated, no backfill needed
- [x] 4.4 DB integration tests pass unchanged (`make test-integration-db` green)
- [x] 4.5 `make lint && make test` green (Zig gates pass; eslint/vitest failures are pre-existing, unrelated to this change)

---

## 5.0 Out of Scope

- Partitioning `usage_ledger` by month (separate milestone)
- Materialized views for budget reporting (separate milestone)
- Changing the budget enforcement semantics or concurrency model
