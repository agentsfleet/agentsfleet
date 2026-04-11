---
Milestone: M10
Workstream: M10_005
Name: PGQUERY_MIGRATION
Status: PENDING
Priority: P2 — progressive cleanup, not blocking
Created: Apr 11, 2026
Depends on: M10_004 (PgQuery wrapper created)
---

# M10_005 — Migrate all conn.query() sites to PgQuery wrapper

## Goal

Migrate every remaining `conn.query()` call site to use `PgQuery.from()` so that:
- RULE PTR is structurally eliminated (no more `anytype` for query results)
- `check-pg-drain` lint can be simplified to: "every `conn.query()` must be inside `PgQuery.from()`"
- Manual `q.drain() catch {}; q.deinit();` pairs are replaced by `defer q.deinit()`

## Background

M10_004 created `src/db/pg_query.zig` and migrated 2 files as proof of concept:
- `entitlements.zig` — 2 functions, removed 6 manual drain/deinit pairs
- `activity_stream.zig` — eliminated the only `anytype` query parameter site

~85 `conn.query()` call sites remain across ~35 files.

## Scope

### Batch 1 — State layer (highest drain complexity)

| File | conn.query() calls | Notes |
|------|-------------------|-------|
| `state/workspace_credit_store.zig` | TBD | Multiple drain paths |
| `state/workspace_billing.zig` | TBD | |
| `state/workspace_billing/row.zig` | TBD | |
| `state/workspace_credit.zig` | TBD | |
| `state/outbox_reconciler.zig` | TBD | |

### Batch 2 — HTTP handlers

| File | Notes |
|------|-------|
| `http/workspace_guards.zig` | |
| `http/handlers/approval_http.zig` | |
| `http/handlers/zombie_api.zig` | |
| `http/handlers/webhooks.zig` | |
| `http/handlers/workspace_credentials_http.zig` | |
| `http/handlers/health.zig` | |
| `http/handlers/agents/get.zig` | |
| `http/handlers/workspaces_billing.zig` | |
| `http/handlers/workspaces_ops.zig` | |
| `http/handlers/zombie_activity_api.zig` | |
| `http/handlers/common.zig` | |
| `http/handlers/harness_control_plane/*.zig` | 4 files |
| `http/handlers/github_callback.zig` | |
| `http/handlers/admin_platform_keys_http.zig` | |

### Batch 3 — Worker / reconciler / other

| File | Notes |
|------|-------|
| `cmd/reconcile/daemon.zig` | |
| `cmd/reconcile.zig` | |
| `cmd/worker_zombie.zig` | |
| `memory/workspace.zig` | |
| `zombie/event_loop.zig` | |
| `secrets/crypto_store.zig` | |
| `observability/prompt_events.zig` | |
| `audit/profile_linkage.zig` | |
| `db/pool.zig` | May need special handling |

### Batch 4 — Test files

| File | Notes |
|------|-------|
| `state/workspace_billing_test.zig` | |
| `http/byok_http_integration_test.zig` | |
| `http/handlers/harness_control_plane/tests*.zig` | |
| `db/pool_test.zig` | |

### Batch 5 — Lint simplification

After all sites migrated:
- Rewrite `lint-zig.py` check-pg-drain: "every `conn.query()` must be wrapped in `PgQuery.from()`"
- Remove the `.drain()` fallback check
- Update ZIG_RULES.md: remove manual drain/deinit patterns, keep only PgQuery

## Out of Scope

- `conn.exec()` calls — these already auto-drain internally
- `pool.query()` calls — these return `QueryRow`, not `Result`
- Changing the pg library itself

## Acceptance Criteria

- [ ] `grep -rn "conn\.query" src/ --include="*.zig" | grep -v "PgQuery.from"` returns only `conn.exec()` or suppressed lines
- [ ] `check-pg-drain` passes
- [ ] No `anytype` parameters for pg query results remain
- [ ] `make test` passes
- [ ] `make lint` passes
- [ ] Cross-compiles
