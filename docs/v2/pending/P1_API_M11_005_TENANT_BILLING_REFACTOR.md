# M11_005: Tenant Billing — Single Row per Tenant (Plan + Credits)

**Prototype:** v2.0.0
**Milestone:** M11
**Workstream:** 005
**Date:** Apr 21, 2026
**Status:** PENDING
**Priority:** P1 — Blocks the end-to-end "zombie runs and we verify credits deducted" demo
**Batch:** B2 — parallel with M19_001, M13_001, M21_001
**Branch:** feat/m11-tenant-credits (TBD)
**Depends on:** M11_003 (Clerk bootstrap — DONE, landed on main Apr 21 via PR #237)

---

## §0 — Pre-EXECUTE Verification (run BEFORE any file edits)

**Status:** PENDING

Before touching any file, run these greps and confirm the surface matches this spec. If results diverge, update the spec first, then EXECUTE.

```bash
# E0.1: Enumerate every live reference to the workspace-scoped billing + credit symbols this spec deletes.
grep -rn "workspace_billing_state\|billing\.workspace_billing\|workspace_credit\|workspace_free_credit" src/ schema/
```

**Expected callers (enumerated — not "any that exist"):**

| File | Expected reference | Action |
|------|--------------------|--------|
| `schema/016_workspace_billing_state.sql` | table definition | DELETE file |
| `schema/017_workspace_free_credit.sql` | table definition | DELETE file |
| `schema/embed.zig` | `workspace_billing_state_sql`, `workspace_free_credit_sql` `@embedFile` | REMOVE both |
| `src/cmd/common.zig` | version-16 and version-17 entries in `canonicalMigrations()` | REMOVE entries + update length + index tests |
| `src/state/workspace_credit.zig`, `workspace_credit_store.zig`, `workspace_credit_test.zig` | facade, store, unit tests | DELETE all three |
| `src/state/workspace_billing*.zig` | any workspace-plan facade/store | DELETE all |
| `src/http/handlers/workspaces/lifecycle.zig` | `provisionWorkspaceCredit(..., "api")` call | REMOVE the call |
| `src/zombie/metering.zig` | `workspace_credit.*` debit path | REWRITE to call `tenant_billing.debit` |
| `src/http/handlers/workspaces/credit*.zig`, `billing*.zig` | per-workspace read handlers | DELETE |
| `src/http/router.zig` | `/v1/workspaces/{ws}/credits`, `/v1/workspaces/{ws}/billing`, `/v1/workspaces/{ws}/credits/redeem` | REMOVE registrations |
| `openapi/paths/*workspaces*credits*`, `*billing*` | path definitions | DELETE |
| `src/main.zig` | `_ = @import("state/workspace_credit*.zig");` test-discovery lines | REMOVE |

If the grep surfaces a reference **outside this table**, STOP and amend the spec before editing.

**Tenant-id resolution contract (worker debit):** the worker performs exactly one lookup per run:

```sql
SELECT tenant_id FROM core.workspaces WHERE workspace_id = $1
```

The resulting tenant_id is carried in memory for the run's lifetime. No per-debit repeat lookup.

**M15_001 metering cross-check (mandatory):**

```bash
# E0.2: Verify M15_001 metering does NOT read workspace_billing_state.plan_tier.
grep -rn "plan_tier\|workspace_billing_state" src/ | grep -vi test | grep -v billing/
```

If any M15_001 metering call site reads `workspace_billing_state.plan_tier`, **add a patch to this milestone's scope**: route plan_tier reads to `billing.tenant_billing.plan_tier` via the tenant_id resolved from workspace_id. Record the found files under Files Changed before EXECUTE.

---

## Overview

**Goal (testable):** A new Clerk signup results in one tenant with `plan_tier='free'` and a 1000¢ balance; any zombie run in any workspace owned by that tenant debits that single tenant balance; creating a second workspace does not grant additional credits and does not create a separate plan row.

**Problem:** Today both credit state (`billing.workspace_free_credit`, `schema/017`) AND plan state (`billing.workspace_billing_state`, `schema/016`) live per-workspace. Workspaces live under tenants (`core.workspaces.tenant_id → core.tenants`), so billing anchored on the workspace row is upside-down: every new workspace gets a fresh 1000¢ grant (trivially exploitable) and every plan upgrade has to fan out across N workspace rows. The earlier M11_005 draft piled on audit tables and dual-scope endpoints; none of that is required for the MVP loop.

**Solution summary:** Collapse the billing story to one table (`billing.tenant_billing`) holding the tenant's `plan_tier`, `plan_sku`, balance in cents, and `grant_source`. Seeded on signup at `plan_tier='free'`, `plan_sku='free_default'`, `balance_cents=1000`. Debited by the worker on every completed run. Remove the per-workspace credit grant at workspace-create time. Delete per-workspace credit state AND per-workspace billing state entirely. No audit tables (skill run logs + activity_events already carry debit intent), no Stripe fields (`billing_status`, `subscription_id`, grace timestamps) — those come back when Stripe wires in.

---

## Files Changed (blast radius)

```
SCHEMA GUARD: VERSION=0.25.0 (<2.0.0) → full teardown branch.
  Creating:  schema/NNN_tenant_billing.sql                   (new table billing.tenant_billing)
  Deleting:  schema/016_workspace_billing_state.sql          (workspace-scoped plan + audit, obsolete)
  Deleting:  schema/017_workspace_free_credit.sql            (workspace-scoped credit pool, obsolete)
  Removing:  schema.workspace_billing_state_sql from schema/embed.zig
  Removing:  schema.workspace_free_credit_sql  from schema/embed.zig
  Removing:  version 16 and version 17 entries from canonicalMigrations() in src/cmd/common.zig
  Deleting:  src/state/workspace_credit.zig                  (facade replaced by tenant_billing)
  Deleting:  src/state/workspace_credit_store.zig            (SQL replaced by tenant_billing_store)
  Deleting:  src/state/workspace_credit_test.zig             (superseded)
  Deleting:  src/state/workspace_billing*.zig                (any facade/store for the workspace plan row)
```

NNN = next free slot at execute time (verify with `ls schema/`).

| File | Action | Why |
|------|--------|-----|
| `schema/NNN_tenant_billing.sql` | CREATE | Single table holding `(tenant_id, plan_tier, plan_sku, balance_cents, grant_source, created_at, updated_at)`. ≤100 lines, single concern. **Unit note on `balance_cents`:** MVP uses cents (integer, Stripe-native for chargebacks). Migration to `balance_micros` (millionths of a cent) is deferred to the per-token-metering milestone — pre-v2.0 teardown makes that migration free (drop + recreate), so we explicitly do not preemptively widen the unit now. |
| `schema/embed.zig` | MODIFY | Add `tenant_billing_sql` `@embedFile`; remove `workspace_billing_state_sql` and `workspace_free_credit_sql`. |
| `src/cmd/common.zig` | MODIFY | Add new migration entry; remove version-16 and version-17 workspace-billing entries; update array length / index-based tests. |
| `src/state/tenant_billing.zig` | CREATE | Facade: `provision(tenant_id, plan='free', cents=1000)`, `debit(tenant_id, cents)`, `getBilling(tenant_id)`. |
| `src/state/tenant_billing_store.zig` | CREATE | Raw SQL: `INSERT ... ON CONFLICT DO NOTHING`, atomic `UPDATE ... WHERE balance_cents >= $cents RETURNING`, `SELECT`. `conn.query().drain()` discipline. |
| `src/state/tenant_billing_test.zig` | CREATE | Unit coverage for provision / debit / exhaustion / concurrent-debit. |
| `src/state/signup_bootstrap.zig` | MODIFY | After tenant/user/workspace insert, call `tenant_billing.provision(tenant_id, 'free', 1000, "bootstrap_free_grant")`. |
| `src/zombie/metering.zig` | MODIFY | Replace `workspace_credit.*` calls with `tenant_billing.debit(tenant_id, cents)`. Tenant id resolved via `zombie.workspace_id → core.workspaces.tenant_id` (single `SELECT tenant_id FROM core.workspaces WHERE workspace_id=$1`). |
| `src/http/handlers/workspaces/lifecycle.zig` | MODIFY | Delete the `provisionWorkspaceCredit(..., "api")` call from the workspace-create handler. New workspaces inherit the tenant balance and plan. |
| `src/http/handlers/tenant_billing.zig` | CREATE | Single read handler: `GET /v1/tenants/me/billing` → `{plan_tier, plan_sku, balance_cents, updated_at}`. |
| `src/http/router.zig` (or equivalent) | MODIFY | Register `/v1/tenants/me/billing`; remove `GET /v1/workspaces/{ws}/credits`, `GET /v1/workspaces/{ws}/billing`, `POST /v1/workspaces/{ws}/credits/redeem`. Pre-v2 → 404s. |
| `src/state/workspace_credit.zig`, `workspace_credit_store.zig`, `workspace_credit_test.zig` | DELETE | Replaced by `tenant_billing.*`. |
| `src/state/workspace_billing*.zig` (any that exist) | DELETE | Superseded; no separate plan module. |
| `schema/016_workspace_billing_state.sql` | DELETE | Per Schema Guard (pre-v2.0): remove file + embed + migration entry. |
| `schema/017_workspace_free_credit.sql` | DELETE | Per Schema Guard (pre-v2.0): remove file + embed + migration entry. |
| `openapi/paths/*` | MODIFY | Add `/v1/tenants/me/billing`; remove workspace credit + billing endpoints. |

---

## Applicable Rules

- **RULE FLL** — every touched `.zig`/`.sql` ≤350 lines; each method ≤~50 lines.
- **RULE FLS** — `conn.query()` + `.drain()` in the same function before `deinit()`; `conn.exec()` for write paths.
- **RULE XCC** — cross-compile `x86_64-linux` and `aarch64-linux` before commit.
- **RULE ORP** — orphan sweep: zero remaining references to `workspace_credit*`, `provisionWorkspaceCredit`, `workspace_free_credit` in non-historical files.
- **RULE TXN** — debit is atomic (`UPDATE ... WHERE balance_cents >= $cents RETURNING balance_cents`); exhausted balance returns a typed error, never a partial debit.
- **Schema Table Removal Guard** — pre-v2.0 teardown branch: delete SQL file + `@embedFile` + migration array entry; no `ALTER`, no `DROP TABLE`, no `SELECT 1;` marker.

---

## Sections (implementation slices)

### §1 — Schema + migration wiring

**Status:** PENDING

Create the new table, delete the workspace-free-credit file + embed + migration entry in one slice so tier-3 fresh DB is always coherent.

**Dimensions:**

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 1.1 | PENDING | `schema/NNN_tenant_billing.sql` | fresh DB, run migrations | `billing.tenant_billing` exists with all five columns, PK on `tenant_id`, FK → `core.tenants` | integration (tier-3) |
| 1.2 | PENDING | `schema/embed.zig` + `src/cmd/common.zig` | fresh DB, run migrations | `billing.workspace_free_credit` does NOT exist; migration array passes length check | integration (tier-3) |
| 1.3 | PENDING | Schema Guard output | pre-edit | Guard block printed exactly per CLAUDE.md format before any file mutation | lint (manual verify in diff) |

### §2 — State module (tenant_billing)

**Status:** PENDING

Facade + store mirroring the existing state-module layout.

**Dimensions:**

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 2.1 | PENDING | `tenant_billing.provision` | new tenant, `cents=1000`, `source="bootstrap_free_grant"` | row inserted; second call for same tenant is a no-op (idempotent on replay) | unit |
| 2.2 | PENDING | `tenant_billing.debit` | tenant with balance 1000, debit 5 | returns `{balance_cents: 995}`; row updated atomically | unit |
| 2.3 | PENDING | `tenant_billing.debit` (exhaustion) | tenant with balance 3, debit 5 | returns `error.CreditExhausted`; balance unchanged at 3 | unit |
| 2.4 | PENDING | `tenant_billing.debit` (concurrent) | two parallel debits of 600 against balance 1000 | exactly one succeeds, exactly one returns `CreditExhausted`; final balance = 400 | integration |
| 2.4a | PENDING | integration harness itself | inspect whether `tests/integration_*.zig` runs each test in a shared transaction (serialized) or opens real parallel connections | if the harness serializes inside one transaction, **rewrite 2.4 against a raw `pg.Pool` with two `std.Thread.spawn` calls** that each check out their own connection — document the chosen path in the test file header before writing 2.4 | design / harness probe |

### §3 — Signup bootstrap + worker debit wiring

**Status:** PENDING

Wire the provision call into signup; replace the workspace-credit debit path in the worker metering module.

**Dimensions:**

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 3.1 | PENDING | `signup_bootstrap.zig` | Clerk webhook for new user | one `billing.tenant_billing` row with `balance_cents=1000`, `grant_source='bootstrap_free_grant'` | integration |
| 3.2 | PENDING | `zombie/metering.zig` | completed run in workspace W owned by tenant T | `billing.tenant_billing.balance_cents` for T decremented by the run cost | integration |
| 3.3 | PENDING | `workspaces/lifecycle.zig` | operator creates a second workspace | no new credit row inserted; tenant balance unchanged | integration |
| 3.4 | PENDING | metering path | run in workspace W2 created after signup | debits the same tenant row, not a per-workspace row | integration |

### §4 — Read endpoint

**Status:** PENDING

One handler, one route.

**Dimensions:**

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 4.1 | PENDING | `GET /v1/tenants/me/billing` | authed operator, bootstrap balance | `200 {"balance_cents": 1000, "updated_at": <epoch_ms>}` | integration |
| 4.2 | PENDING | `GET /v1/tenants/me/billing` | after one debit of 5 | `200 {"balance_cents": 995, ...}` | integration |
| 4.3 | PENDING | removed workspace credit endpoints | `GET /v1/workspaces/{ws}/credits` | `404` (pre-v2.0 teardown — no 410 ceremony) | integration |

---

## Interfaces

### Public functions (Zig)

```zig
// src/state/tenant_billing.zig
pub fn provision(
    conn: *pg.Conn,
    tenant_id: Uuid,
    balance_cents: i64,
    grant_source: []const u8,
) !void;

pub fn debit(
    conn: *pg.Conn,
    tenant_id: Uuid,
    cents: i64,
) !DebitResult; // returns new balance, or error.CreditExhausted

pub fn getBalance(
    conn: *pg.Conn,
    tenant_id: Uuid,
) !?Balance; // returns null if no row (should be impossible post-bootstrap)

pub const DebitResult = struct { balance_cents: i64, updated_at_ms: i64 };
pub const Balance    = struct { balance_cents: i64, updated_at_ms: i64 };
```

### Input contract — `GET /v1/tenants/me/billing`

| Field | Type | Constraints | Example |
|-------|------|-------------|---------|
| Authorization | header | `Bearer <clerk_jwt>` | `Bearer eyJ...` |

### Output contract

| Field | Type | When | Example |
|-------|------|------|---------|
| `balance_cents` | i64 | always | `1000` |
| `updated_at` | i64 (epoch ms) | always | `1713700000000` |

### Error contracts

| Error condition | Behavior | Caller sees |
|----------------|----------|-------------|
| Missing / invalid JWT | reject at auth middleware | `401 UZ-AUTH-001` |
| Tenant has no row (bug) | log + synthesize `{0, now}` or return 500 — pick during EXECUTE; prefer 500 so the invariant bug is visible | `500 UZ-BILLING-010` |
| Debit below zero | DB `UPDATE ... WHERE balance_cents >= $cents` returns 0 rows | `error.CreditExhausted` at call site; worker maps to `UZ-CREDIT-EXHAUSTED` run outcome |

---

## Failure Modes

| Failure | Trigger | System behavior | User observes |
|---------|---------|-----------------|---------------|
| Signup bootstrap runs twice (Clerk retry) | duplicate webhook | `provision` idempotent — `INSERT ... ON CONFLICT (tenant_id) DO NOTHING` | balance stays 1000, no double-grant |
| Worker debits while concurrent debit runs | two runs complete simultaneously | atomic conditional `UPDATE` — exactly one commits the contested cents | one succeeds, one returns `CreditExhausted` |
| Operator creates second workspace | workspace-create handler | no credit-provision call is made | tenant balance unchanged |
| Clerk deletes tenant | `ON DELETE CASCADE` on FK | `tenant_billing` row removed | row gone; API returns 401 (JWT invalid post-delete) |
| Debit on tenant with no row | bootstrap failure + later run | debit returns `error.CreditExhausted` (0-row update) | run fails with `UZ-CREDIT-EXHAUSTED` |

---

## Implementation Constraints

| Constraint | How to verify |
|-----------|---------------|
| SQL file ≤100 lines, single concern (`billing.tenant_billing` only) | `wc -l schema/NNN_tenant_billing.sql` |
| No `CHECK` constraint enumerating `grant_source` values (open set) | `grep CHECK schema/NNN_tenant_billing.sql` returns only the `balance_cents >= 0` check |
| Debit is atomic — no read-then-write race | SQL is single `UPDATE ... WHERE balance_cents >= $cents RETURNING`; integration dim 2.4 proves it |
| No audit table, no plan-tier table introduced | grep schema for `tenant_plan`, `tenant_credit_audit` — 0 matches |
| Workspace-create handler no longer provisions credit | `grep provisionWorkspaceCredit src/http/handlers/workspaces/lifecycle.zig` → 0 matches |
| Orphan sweep: `workspace_credit`, `workspace_free_credit`, `provisionWorkspaceCredit` gone from non-historical files | see Eval E5 |
| Cross-compile green | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` |
| Drain discipline on new store | `make check-pg-drain` |
| Per-file FLL gate | `wc -l` < 350 on every touched `.zig` |

---

## Test Specification

### Unit tests

| Test name | Dim | Target | Input | Expected |
|-----------|-----|--------|-------|----------|
| `provision inserts one row` | 2.1 | `tenant_billing.provision` | new tenant, 1000¢ | row present, balance=1000 |
| `provision is idempotent` | 2.1 | `tenant_billing.provision` | call twice for same tenant | second call no-ops; balance still 1000 |
| `debit decrements balance` | 2.2 | `tenant_billing.debit` | balance 1000, debit 5 | returns `{995, now}` |
| `debit rejects when exhausted` | 2.3 | `tenant_billing.debit` | balance 3, debit 5 | `error.CreditExhausted`; balance still 3 |

### Integration tests

| Test name | Dim | Infra | Input | Expected |
|-----------|-----|-------|-------|----------|
| `concurrent debits serialize correctly` | 2.4 | DB | two parallel 600¢ debits, balance 1000 | one succeeds, one `CreditExhausted`, final 400 |
| `signup bootstrap grants 1000c` | 3.1 | DB | Clerk webhook | one `billing.tenant_billing` row, source `bootstrap_free_grant` |
| `worker debits tenant balance` | 3.2 | DB + worker | run completes, cost 5¢ | tenant balance 1000 → 995 |
| `second workspace does not grant more credits` | 3.3 | DB | create two workspaces | tenant balance stays 1000 |
| `cross-workspace runs share balance` | 3.4 | DB + worker | run in ws1, then run in ws2 | both decrement same row |
| `GET /v1/tenants/me/billing happy path` | 4.1, 4.2 | DB + HTTP | authed GET | `200 {balance_cents, updated_at}` matches state |
| `removed workspace credit endpoint 404s` | 4.3 | HTTP | `GET /v1/workspaces/{ws}/credits` | 404 |

### Negative tests

| Test name | Dim | Input | Expected error |
|-----------|-----|-------|----------------|
| `debit on missing tenant` | 2.3 | unknown tenant | `CreditExhausted` (0-row update) |
| `unauthed credits GET` | 4.1 | no Authorization header | `401 UZ-AUTH-001` |

### Regression tests

| Test name | What it guards | File |
|-----------|----------------|------|
| none — greenfield billing layer; workspace credit tests are deleted with the module |  |  |

### Leak detection

| Test name | Dim | What it proves |
|-----------|-----|----------------|
| `tenant_billing store unit suite` | 2.x | `std.testing.allocator` detects zero leaks across provision / debit / getBalance |

### Spec-claim tracing

| Spec claim | Test | Type |
|-----------|------|------|
| "New signup → 1000¢ balance" | `signup bootstrap grants 1000c` | integration |
| "Any workspace debits the tenant balance" | `cross-workspace runs share balance` | integration |
| "Second workspace does not grant more credits" | `second workspace does not grant more credits` | integration |

---

## Execution Plan

| Step | Action | Verify |
|------|--------|--------|
| 1 | Print Schema Guard block (see Files Changed). Create `schema/NNN_tenant_billing.sql`; delete `schema/017_workspace_free_credit.sql`; update `schema/embed.zig` and the migration array in `src/cmd/common.zig`. | `make down && make up` clean; `psql -c '\d billing.tenant_billing'` |
| 2 | Write `src/state/tenant_billing{,_store,_test}.zig`. | `zig build test -Dtest-filter=tenant_billing` green |
| 3 | Update `src/state/signup_bootstrap.zig` to call `tenant_billing.provision(tenant_id, 1000, "bootstrap_free_grant")` after tenant/user/workspace insert. | integration dim 3.1 |
| 4 | Update `src/zombie/metering.zig` to debit `billing.tenant_billing` using `tenant_id` resolved from `workspace_id`. | integration dims 3.2, 3.4 |
| 5 | Remove `provisionWorkspaceCredit(..., "api")` call from `src/http/handlers/workspaces/lifecycle.zig`. | integration dim 3.3 |
| 6 | Delete `src/state/workspace_credit.zig`, `src/state/workspace_credit_store.zig`, `src/state/workspace_credit_test.zig`. Remove `_ = @import("...");` lines from `src/main.zig`. | `grep -rn workspace_credit src/` = 0 matches |
| 7 | Add `src/http/handlers/tenant_billing.zig` + route. Remove any workspace credit handler files + route registrations. | integration dims 4.1–4.3 |
| 8 | OpenAPI regen; update any CLI / docs references under scope. | `make check-openapi-errors` |
| 9 | Tier-3 fresh DB + full branch gate. | `make down && make up && make test-integration`, `make lint`, `make check-pg-drain`, cross-compile both targets, `gitleaks detect` |

---

## Acceptance Criteria

- [ ] `billing.tenant_billing` exists on fresh DB; `billing.workspace_free_credit` does not — verify: `make down && make up && psql -c '\dt billing.*'`
- [ ] Clerk signup produces exactly one `billing.tenant_billing` row at 1000¢ — verify: integration dim 3.1
- [ ] Completed run debits the tenant row — verify: integration dim 3.2
- [ ] Second workspace does not grant additional credits — verify: integration dim 3.3
- [ ] Runs across two workspaces share the balance — verify: integration dim 3.4
- [ ] `GET /v1/tenants/me/billing` returns current balance — verify: integration dim 4.1
- [ ] Concurrent debit correctness — verify: integration dim 2.4
- [ ] Orphan sweep clean for `workspace_credit`, `provisionWorkspaceCredit`, `workspace_free_credit` — verify: Eval E5
- [ ] `make lint`, `make check-pg-drain`, cross-compile both targets, `gitleaks detect` all green
- [ ] 350-line file gate clean on all touched `.zig`

---

## Eval Commands

```bash
# E1: Zig build
zig build 2>&1 | tail -5; echo "zig_build=$?"

# E2: Tier-3 fresh DB + full integration (mandatory for schema teardown)
make down && make up && make test-integration 2>&1 | tail -10

# E3: Cross-compile
zig build -Dtarget=x86_64-linux 2>&1 | tail -3
zig build -Dtarget=aarch64-linux 2>&1 | tail -3

# E4: Drain + gitleaks + lint
make check-pg-drain
make lint 2>&1 | grep -E "PASS|FAIL"
gitleaks detect 2>&1 | tail -3

# E5: Orphan sweep — deleted symbols must vanish from non-historical files
grep -rn "workspace_credit\|workspace_free_credit\|provisionWorkspaceCredit" src/ tests/ schema/ \
  | grep -v -E "done/|historical|v1/done" \
  || echo "ORP clean"

# E6: 350-line gate on touched .zig / .sql
git diff --name-only origin/main \
  | grep -E '\.(zig|sql)$' \
  | grep -v -E '_test\.|/tests?/' \
  | xargs -I{} sh -c 'wc -l "{}"' \
  | awk '$1 > 350 { print "OVER: " $2 ": " $1 }'

# E7: OpenAPI error compliance
make check-openapi-errors
```

---

## Dead Code Sweep

| File to delete | Verify deleted |
|----------------|----------------|
| `schema/017_workspace_free_credit.sql` | `test ! -f schema/017_workspace_free_credit.sql` |
| `src/state/workspace_credit.zig` | `test ! -f src/state/workspace_credit.zig` |
| `src/state/workspace_credit_store.zig` | `test ! -f src/state/workspace_credit_store.zig` |
| `src/state/workspace_credit_test.zig` | `test ! -f src/state/workspace_credit_test.zig` |

Orphaned references:

| Deleted symbol | Grep | Expected |
|----------------|------|----------|
| `workspace_credit` | `grep -rn workspace_credit src/` | 0 |
| `provisionWorkspaceCredit` | `grep -rn provisionWorkspaceCredit src/` | 0 |
| `workspace_free_credit` | `grep -rn workspace_free_credit src/ schema/` | 0 |

Also remove the `_ = @import("state/workspace_credit*.zig");` lines from `src/main.zig` test discovery.

---

## Out of Scope

- Separate `billing.tenant_plan` table — plan_tier and plan_sku live on `billing.tenant_billing`; no split until Stripe lands.
- Stripe subscription fields (`billing_status`, `subscription_id`, `payment_failed_at`, `grace_expires_at`, `pending_status`, `pending_reason`) — deferred to the Stripe-wiring milestone; removed from the new table by design.
- Audit tables (`tenant_billing_audit`, etc.) — no regulatory need pre-alpha; skill run logs + activity_events already carry debit intent.
- `PUT /v1/tenants/me/plan` — no tier-change endpoint in this spec; plan stays `free` until Stripe wires in.
- Per-workspace credit attribution / roll-up views — one tenant balance is the only concept.
- Stripe top-up / purchase flows — post-MVP; this spec only handles the free grant.
- Team accounts / multi-user grant sharing — M16+.
- Invite-based or redeem-code credit grants — killed by the Clerk pivot.
- `PUT /v1/tenants/me/plan` / any tier-change endpoint — no tier in this spec.
- Per-workspace plan overrides — deliberately excluded; tenant is the billing boundary.
