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

## Applicable Rules

- RULE FLS — flush all layers / drain results (PgQuery enforces this structurally)
- RULE PTR — eliminated by this migration (no more anytype for queries)
- RULE ORP — orphan sweep for removed drain/deinit patterns
- RULE XCC — cross-compile before commit
- RULE FLL — 350-line gate on touched files

## Invariants

| # | Invariant | Enforcement mechanism |
|---|-----------|----------------------|
| 1 | Every `conn.query()` must be wrapped in `PgQuery.from()` | `lint-zig.py` check-pg-drain (after migration) |
| 2 | No `anytype` parameter for pg query results | `grep -rn "q: anytype" src/` returns 0 |

## Eval Commands

```bash
# E1: Zero bare conn.query() (all wrapped in PgQuery.from)
count=$(grep -rn "conn\.query" src/ --include="*.zig" | grep -v "PgQuery.from\|conn.exec\|// check-pg-drain: ok" | wc -l | tr -d ' ')
[ "$count" -eq 0 ] && echo "PASS" || echo "FAIL: $count bare conn.query()"

# E2: Zero anytype pg query params
count=$(grep -rn "q: anytype\|q:anytype" src/ --include="*.zig" | wc -l | tr -d ' ')
[ "$count" -eq 0 ] && echo "PASS" || echo "FAIL: $count anytype query params"

# E3: Build + test + lint + cross-compile + gitleaks
zig build 2>&1 | head -5; echo "build=$?"
zig build test 2>&1 | tail -5; echo "test=$?"
make lint 2>&1 | grep -E "✓|FAIL"
zig build -Dtarget=x86_64-linux 2>&1 | tail -3; echo "x86=$?"
zig build -Dtarget=aarch64-linux 2>&1 | tail -3; echo "arm=$?"
gitleaks detect 2>&1 | tail -3; echo "gitleaks=$?"

# E4: Memory leak check
zig build test 2>&1 | grep -i "leak" | head -5
echo "E4: leak check (empty = pass)"

# E5: 350-line gate
git diff --name-only origin/main | grep -v '\.md$' | xargs wc -l 2>/dev/null | awk '$1 > 350 { print "OVER: " $2 ": " $1 }'
```

## Dead Code Sweep

| Deleted pattern | Grep command | Expected |
|----------------|--------------|----------|
| Manual `q.drain() catch {}; q.deinit();` | `grep -rn "q\.drain.*catch.*q\.deinit" src/ --include="*.zig"` | 0 matches |
| `q: anytype` for query results | `grep -rn "q: anytype" src/ --include="*.zig"` | 0 matches |
| `q.*.next()` / `q.*.drain()` | `grep -rn 'q\.\*\.' src/ --include="*.zig"` | 0 matches |

## Verification Evidence

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Unit tests | `make test` | | |
| Leak detection | `zig build test \| grep leak` | | |
| Cross-compile | `zig build -Dtarget=x86_64-linux` | | |
| Lint (incl. check-pg-drain) | `make lint` | | |
| Gitleaks | `gitleaks detect` | | |
| 350L gate | `wc -l` (exempts .md) | | |
| Dead code sweep | eval E1–E2 | | |

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
