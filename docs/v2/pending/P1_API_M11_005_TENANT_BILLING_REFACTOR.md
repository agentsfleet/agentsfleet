# M11_005: Move Plan + Credits from Workspace to Tenant — Billing Foundation Cleanup

**Prototype:** v2
**Milestone:** M11
**Workstream:** 005
**Date:** Apr 16, 2026 (salvaged Apr 21 post-M11_003 Clerk pivot)
**Status:** PENDING — needs re-baseline before execution (see Post-Pivot Notes)
**Priority:** P1 — Foundational; blocks Team Accounts (M16+) and clean tier upgrades
**Batch:** B5 — follows M11_003 (Clerk signup) on main; no longer parallel
**Branch:** feat/m11-tenant-billing-refactor (TBD)
**Depends on:** M11_003 identity layer — `core.tenants`, `core.users`, `core.memberships` — landed on main Apr 21 via PR #237.

---

## Post-Pivot Notes (Apr 21, 2026)

This spec was drafted against M11_003's original invite-signup flow and cherry-picked onto main after M11_003 pivoted to Clerk webhook signup (PR #237 merged Apr 21). Before execution:

- **§9 (Coordination with M11_003 redemption) is obsolete.** `POST /v1/workspaces/{ws}/credits/redeem` was killed by the Clerk pivot — no redemption endpoint exists on main. The cutover sequencing in §9 can be deleted; the tenant-scoped credit model itself still stands independently.
- **CLI `zombiectl credits redeem` references throughout §7–§9 are dead.** The command was never shipped.
- **References to "access code" / "invite" anywhere below** are pre-pivot vocabulary; re-frame as plain "credit grant" when re-baselining.
- **Identity layer on main matches the spec's Layer B assumption** — `core.tenants(tenant_id, name, kind)` and `core.users(oidc_subject, tenant_id, ...)` are live. `tenant.kind` defaults to `personal` for Clerk-bootstrapped tenants.

The foundational idea (plan + credit at tenant level, workspace is a usage scope) is unaffected by the pivot and remains load-bearing for the Team Accounts milestone (M16).

---

## Overview

**Goal (testable):** A signed-up user has a Personal account (`tenant.kind = 'personal'`). Their plan tier (Free / Pro / Scale / Enterprise) lives at the **tenant** level — not on each workspace. Their credit balance lives at the **tenant** level too. Workspaces (projects) consume from the tenant's shared credit pool. `GET /v1/tenants/{tenant_id}/credits` is the source of truth; `GET /v1/workspaces/{ws}/credits` becomes a derived view that returns the parent tenant's balance with workspace-scoped usage attribution.

**Problem:** Today, plan and credits live on each `workspace` row (`billing.workspace_billing_state.plan_tier`, `billing.workspace_credit_state.*`). This means:

1. **Personal accounts split their economy across projects** — if Jane has three workspaces, each gets its own free 20-credit pool (not what we promise). Or, if the bootstrap only creates credits on the default workspace, her second workspace can't run.
2. **Team accounts (M16) are impossible to model cleanly** — five teammates in one tenant should share a Pro plan and one credit pool. Per-workspace plan rows mean one of them is "the billing workspace" and others are second-class.
3. **Plan changes require touching N workspace rows** — Pro upgrade has to fan out across every workspace the user owns. State drift is inevitable.
4. **The mental model is upside-down vs. industry standard** (Anthropic, OpenAI, Vercel, Stripe). Every dashboard product has plan and credits at the **account** level. Projects/workspaces are usage scopes.

**Solution summary:** Add two tenant-scoped tables; mark workspace-scoped tables as deprecated; flip API endpoints to tenant-scoped (workspace-scoped paths return a derived view); update the credit-deduction path in the worker to debit tenant credits, attributed to the workspace that ran the consumption. Pre-v2.0 means the deprecated workspace tables can be dropped outright once callers move (no migration needed).

**Non-goals (this milestone):**
- Team accounts (multi-user invites, shared dashboards) — M16.
- Stripe subscription wiring — billing adapter stays as-is; only the data model moves.
- Per-workspace credit caps within a tenant pool (e.g. "workspace acme-prod gets 50% of monthly credits") — V3.
- Plan-tier UI changes — Settings page (M12) consumes the new endpoints; no visual redesign.

---

## 1.0 New Schema (Tenant-Scoped Billing)

**Status:** PENDING

Two new tables, each ≤100 lines, single concern.

### 1.1 `billing.tenant_plan` (replaces workspace plan rows)

```sql
CREATE TABLE IF NOT EXISTS billing.tenant_plan (
    plan_id            UUID PRIMARY KEY,
    tenant_id          UUID NOT NULL UNIQUE REFERENCES core.tenants(tenant_id) ON DELETE CASCADE,
    plan_tier          TEXT NOT NULL,             -- values in src/types/plan_tier.zig
    plan_sku           TEXT NOT NULL,
    billing_status     TEXT NOT NULL,             -- values in src/types/billing_status.zig
    adapter            TEXT NOT NULL,
    subscription_id    TEXT,
    payment_failed_at  BIGINT,
    grace_expires_at   BIGINT,
    pending_status     TEXT,
    pending_reason     TEXT,
    created_at         BIGINT NOT NULL,
    updated_at         BIGINT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_tenant_plan_tier
    ON billing.tenant_plan (plan_tier, billing_status, updated_at DESC);
```

Companion audit table:

```sql
CREATE TABLE IF NOT EXISTS billing.tenant_plan_audit (
    audit_id            UUID PRIMARY KEY,
    tenant_id           UUID NOT NULL REFERENCES core.tenants(tenant_id) ON DELETE CASCADE,
    event_type          TEXT NOT NULL,
    from_plan_tier      TEXT,
    to_plan_tier        TEXT,
    actor               TEXT NOT NULL,
    metadata_json       TEXT NOT NULL,
    created_at          BIGINT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_tenant_plan_audit_tenant
    ON billing.tenant_plan_audit (tenant_id, created_at DESC);
```

### 1.2 `billing.tenant_credits` (replaces workspace credit pool)

```sql
CREATE TABLE IF NOT EXISTS billing.tenant_credits (
    credit_id              UUID PRIMARY KEY,
    tenant_id              UUID NOT NULL UNIQUE REFERENCES core.tenants(tenant_id) ON DELETE CASCADE,
    currency               TEXT NOT NULL,
    initial_credit_cents   BIGINT NOT NULL CHECK (initial_credit_cents >= 0),
    consumed_credit_cents  BIGINT NOT NULL CHECK (consumed_credit_cents >= 0),
    remaining_credit_cents BIGINT NOT NULL CHECK (remaining_credit_cents >= 0),
    exhausted_at           BIGINT,
    created_at             BIGINT NOT NULL,
    updated_at             BIGINT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_tenant_credits_remaining
    ON billing.tenant_credits (remaining_credit_cents, updated_at DESC);

CREATE TABLE IF NOT EXISTS billing.tenant_credit_audit (
    audit_id               UUID PRIMARY KEY,
    tenant_id              UUID NOT NULL REFERENCES core.tenants(tenant_id) ON DELETE CASCADE,
    workspace_id           UUID REFERENCES core.workspaces(workspace_id) ON DELETE SET NULL,
    event_type             TEXT NOT NULL,
    delta_credit_cents     BIGINT NOT NULL,
    remaining_credit_cents BIGINT NOT NULL CHECK (remaining_credit_cents >= 0),
    reason                 TEXT NOT NULL,
    actor                  TEXT NOT NULL,
    metadata_json          TEXT NOT NULL,
    created_at             BIGINT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_tenant_credit_audit_tenant
    ON billing.tenant_credit_audit (tenant_id, created_at DESC);
```

`workspace_id` on the audit row is **nullable** because some events (signup grant, plan upgrade) aren't workspace-attributable. Runtime debits always carry a workspace_id (which workspace consumed credits).

### 1.3 Deprecate (delete, pre-v2.0) the workspace-scoped versions

Per the Schema Table Removal Guard (VERSION < 2.0.0):
- Delete `schema/016_workspace_billing_state.sql` and `schema/017_workspace_free_credit.sql`.
- Remove the `@embedFile` constants from `schema/embed.zig`.
- Remove the corresponding entries from `canonicalMigrations()` in `src/cmd/common.zig`.

**Dimensions:**

- 1.1 PENDING — fresh DB applies new tables, drops old; tier-3 `make down && make up && make test-integration` clean
- 1.2 PENDING — `tenant_plan` row created via new state helper for a freshly-bootstrapped personal tenant
- 1.3 PENDING — `tenant_credits` provisioned at 0 cents on bootstrap; redemption grants credits atomically
- 1.4 PENDING — old `workspace_billing_state` / `workspace_credit_state` / `_audit` tables not present in fresh DB

---

## 2.0 New State Modules

**Status:** PENDING

Mirror the existing layout (facade + store + tests).

```
src/state/tenant_plan.zig          ← facade (provision, transition, get)
src/state/tenant_plan_store.zig    ← SQL
src/state/tenant_plan_test.zig
src/state/tenant_credits.zig       ← facade (provision, grant, debit, balance)
src/state/tenant_credits_store.zig
src/state/tenant_credits_test.zig
```

`tenant_credits.zig` carries forward the bvisor error-mapping pattern; `CreditExhausted` keeps its existing error-registry code (`ERR_CREDIT_EXHAUSTED`).

The previous `src/state/workspace_billing*.zig` and `src/state/workspace_credit*.zig` modules are **deleted** along with their stores and tests. Any caller that imports them updates to the tenant equivalents.

**Dimensions:**
- 2.1 PENDING — `tenant_plan.provision(tenant_id, plan_tier='free')` writes a row, idempotent on replay
- 2.2 PENDING — `tenant_credits.grant(tenant_id, +N, source_ref)` is atomic with audit insert
- 2.3 PENDING — `tenant_credits.debit(tenant_id, workspace_id, N, run_id)` debits the tenant pool, attributes audit row to workspace
- 2.4 PENDING — `tenant_credits.debit` returns `CreditExhausted` and does not partial-debit when balance < amount
- 2.5 PENDING — concurrent debit safety: two parallel debits totalling more than balance → exactly one returns `CreditExhausted`, the other commits

---

## 3.0 API Endpoint Migration

**Status:** PENDING

### 3.1 New tenant-scoped endpoints (canonical)

```
GET  /v1/tenants/{tenant_id}/plan                  — returns plan_tier, billing_status, sub_id
PUT  /v1/tenants/{tenant_id}/plan                  — admin/billing-adapter upgrade/downgrade
GET  /v1/tenants/{tenant_id}/credits               — total balance + audit summary
POST /v1/tenants/{tenant_id}/credits/redeem        — redeem access code (target of M11_003 step 9 — see §9)
```

### 3.2 Workspace-scoped endpoints (derived views, kept for back-compat)

```
GET  /v1/workspaces/{ws}/credits   — returns parent tenant's balance + workspace's contribution to consumption
GET  /v1/workspaces/{ws}/billing   — returns parent tenant's plan
```

These are **read-only** derived views. Writes (`PUT /v1/workspaces/{ws}/billing/plan`, etc.) return `409 UZ-BILLING-006 "Plan is tenant-scoped — use /v1/tenants/{tenant_id}/plan"` so callers can't drift.

### 3.3 Removed endpoints

```
PUT  /v1/workspaces/{ws}/billing/plan         → returns 410 with redirect hint to tenant endpoint
POST /v1/workspaces/{ws}/billing/event        → moved to tenant scope
POST /v1/workspaces/{ws}/credits/redeem       → moved to tenant scope (see §9 coordination with M11_003)
```

Pre-v2.0 carve-out per RULE EP4: removed endpoints may return bare 404; we choose 410 with redirect hint here because the data is still present (just at a different scope), and a redirect saves callers a round of debugging.

**Dimensions:**
- 3.1 PENDING — `GET /v1/tenants/{tenant_id}/plan` returns provisioned plan row
- 3.2 PENDING — `PUT /v1/tenants/{tenant_id}/plan` upgrades free → pro, audit row written
- 3.3 PENDING — `GET /v1/workspaces/{ws}/credits` returns parent tenant balance (derived)
- 3.4 PENDING — `PUT /v1/workspaces/{ws}/billing/plan` returns 409 with redirect hint
- 3.5 PENDING — removed endpoints return 410 with redirect message

---

## 4.0 Worker / Runtime Debit Path

**Status:** PENDING

The credit-debit hot path lives in `src/zombie/metering.zig` and `src/state/workspace_credit.zig#deductCompletedRuntimeUsage`. After this milestone:

- `src/state/workspace_credit.zig` is **deleted**.
- The worker calls `tenant_credits.debit(tenant_id, workspace_id, cents, run_id)` instead.
- `tenant_id` is resolved from `workspace_id` via the existing FK (already loaded for most run paths; otherwise add a single SELECT before the debit).
- Audit row carries both `tenant_id` (the pool) and `workspace_id` (the consumer attribution).

**Dimensions:**
- 4.1 PENDING — completed run debits parent tenant's pool, audit row attributes to workspace
- 4.2 PENDING — exhausted tenant pool returns `CreditExhausted` for runs from any of the tenant's workspaces
- 4.3 PENDING — workspace-deletion does not zero the audit trail (workspace_id audit FK is `ON DELETE SET NULL`)
- 4.4 PENDING — debit idempotency by `run_id` survives the move (same key, same dedupe semantics)

---

## 5.0 Bootstrap Wiring (coordination with M11_003)

**Status:** PENDING

The signup bootstrap helper (`src/state/signup_bootstrap.zig`, written in M11_003 step 5) currently calls:

```zig
workspace_credit.provisionWorkspaceCredit(workspace_id, 0)
```

After this milestone, it calls **two** state functions:

```zig
tenant_plan.provision(tenant_id, plan_tier="free")
tenant_credits.provision(tenant_id, initial_credit_cents=0)
```

(The workspace gets created the same way; it just no longer carries plan or credit state.)

The change to `signup_bootstrap.zig` happens in this milestone, not M11_003. M11_003 ships with the workspace-scoped call; this milestone replaces it as part of the cutover.

**Dimensions:**
- 5.1 PENDING — bootstrap creates exactly one `tenant_plan` (free) and one `tenant_credits` (0 cents) row per new tenant
- 5.2 PENDING — bootstrap idempotent: replay returns existing rows, no double-provision

---

## 6.0 Interfaces

### 6.1 Removed code

- `src/state/workspace_billing*.zig` (model, transition, store, facade)
- `src/state/workspace_credit*.zig` (facade, store, tests)
- Any handler that wrote to the deleted endpoints (§3.3)

### 6.2 New code

- `src/state/tenant_plan{,_store,_test}.zig`
- `src/state/tenant_credits{,_store,_test}.zig`
- `src/types/plan_tier.zig` and `src/types/billing_status.zig` (closed-set Zig enums; SQL stays TEXT)
- `src/http/handlers/tenant_plan.zig`, `src/http/handlers/tenant_credits.zig`
- Router entries `mint_tenant_plan`, `get_tenant_plan`, `redeem_tenant_credits`, etc.

### 6.3 Error contracts

| Condition | Code | HTTP |
|---|---|---|
| Plan write attempted at workspace scope | `UZ-BILLING-006` | 409 |
| Credit redemption at workspace scope after cutover | `UZ-BILLING-007` | 410 (with redirect hint) |
| (existing `UZ-CODE-00x` codes from M11_003 carry over unchanged) | | |

---

## 7.0 Implementation Constraints

| Constraint | How to verify |
|---|---|
| Schema files ≤100 lines each, single concern | RULE FLL gate |
| No CHECK constraints on enum-like TEXT columns | grep schema for `CHECK.*IN \(` |
| Plan/credits derived-view endpoints are read-only | Dim 3.4 |
| Tenant credit debit is atomic + idempotent by run_id | Dim 4.4 |
| Old workspace_billing/credit modules fully deleted (RULE ORP — no orphan refs) | grep `workspace_credit\|workspace_billing` in src/ → only in deleted files |
| Existing test fixtures (uc1, etc.) updated to use tenant scope | Dim 1.1 fresh DB green |
| Bootstrap helper (M11_003 §5) updated to call tenant provisions | Dim 5.1 |

---

## 8.0 Execution Plan

| Step | Action | Verify |
|---|---|---|
| 1 | New schema files for `tenant_plan`, `tenant_plan_audit`, `tenant_credits`, `tenant_credit_audit`. Register in `embed.zig` + `canonicalMigrations()`. Tier-3 fresh DB green. | Dim 1.1 |
| 2 | `src/types/plan_tier.zig`, `src/types/billing_status.zig` Zig enums | unit |
| 3 | `src/state/tenant_plan{,_store,_test}.zig` | Dim 2.1 |
| 4 | `src/state/tenant_credits{,_store,_test}.zig` | Dims 2.2–2.5 |
| 5 | New tenant-scoped HTTP handlers + router wires | Dims 3.1, 3.2 |
| 6 | Workspace-scoped derived view handlers (read-only); 409/410 on writes | Dims 3.3, 3.4, 3.5 |
| 7 | Worker debit path: switch from workspace_credit to tenant_credits | Dims 4.1–4.4 |
| 8 | Update `signup_bootstrap.zig` (M11_003 product) to call tenant provisions | Dim 5.1 |
| 9 | **Coordination cutover** (see §9): swap M11_003's redemption handler from workspace path to tenant path; update CLI `zombiectl credits redeem` accordingly | M11_003 dims 5.1, 6.3 still pass |
| 10 | Delete old workspace_billing/credit files; orphan sweep | RULE ORP green |
| 11 | OpenAPI regen for new tenant endpoints + 410 redirect on removed workspace endpoints | `make check-openapi-errors` |
| 12 | Full branch gate (lint, drain, cross-compile, gitleaks, 350-line, fresh-DB tier-3) | All dims |

---

## 9.0 Coordination With M11_003 (Cutover Order)

M11_003 is shipping `POST /v1/workspaces/{ws}/credits/redeem` as part of step 9. This milestone moves it to `POST /v1/tenants/{tenant_id}/credits/redeem`. **Both PRs cannot land in arbitrary order without conflicts.** The cutover is:

1. **M11_003 lands first** with the workspace-scoped redemption handler. CLI `zombiectl credits redeem` calls workspace-scoped path. Tests pass.
2. **This milestone lands second**, with three coordinated changes in a single commit:
   a. New tenant-scoped redemption handler (alongside the existing workspace-scoped one).
   b. CLI updated to call the tenant-scoped path.
   c. Workspace-scoped redemption handler returns 410 with redirect hint.
3. The M11_003 redemption tests are updated in the same commit to use the new endpoint.

Alternative if this milestone lands first: M11_003's step 9 ships directly against the tenant-scoped endpoint. Either order works; the agents picking up the two milestones must coordinate via the branch.

---

## 10.0 Acceptance Criteria

- [ ] `billing.tenant_plan` and `billing.tenant_credits` (+ audit tables) exist on fresh DB — Dim 1.1
- [ ] `billing.workspace_billing_state` and `billing.workspace_credit_state` no longer exist on fresh DB — Dim 1.4
- [ ] `tenant_plan.provision`, `tenant_credits.provision/grant/debit` work + tests pass — Dims 2.1–2.5
- [ ] New tenant-scoped endpoints return correct shape — Dims 3.1–3.2
- [ ] Workspace-scoped endpoints become read-only derived views; writes return 409/410 — Dims 3.3–3.5
- [ ] Worker debit path attributes consumption to workspace, drains tenant pool — Dims 4.1–4.4
- [ ] Signup bootstrap (M11_003 product) provisions tenant plan + tenant credits — Dim 5.1
- [ ] M11_003 redemption flow continues to work end-to-end after cutover — coordination with M11_003 dims 5.1, 6.3
- [ ] No orphan refs to deleted workspace_billing/workspace_credit symbols — RULE ORP sweep
- [ ] Full branch gate green (lint, drain, cross-compile, gitleaks, 350-line, fresh-DB tier-3)

---

## Applicable Rules

RULE FLL (350-line file gate), RULE FLS (conn.query drain), RULE XCC (cross-compile Zig), RULE TXN (atomic debit + audit), RULE EP4 (error contracts), RULE ORP (orphan sweep — old workspace_billing/credit symbols must vanish from non-historical files).

Schema Table Removal Guard (VERSION=0.9.0 < 2.0.0): teardown branch active for `workspace_billing_state` + `workspace_credit_state` removals — full delete (file + embed entry + migration array entry), no DROP/ALTER. Print guard output at EXECUTE before each removal.

---

## Eval Commands

```bash
# E1: Zig build
zig build 2>&1 | head -5; echo "zig_build=$?"

# E2: Tier-3 fresh DB integration (mandatory for schema deletes)
make down && make up && make test-integration 2>&1 | tail -10

# E3: Cross-compile
zig build -Dtarget=x86_64-linux 2>&1 | tail -3
zig build -Dtarget=aarch64-linux 2>&1 | tail -3

# E4: Drain + gitleaks
make check-pg-drain
gitleaks detect 2>&1 | tail -3

# E5: Orphan sweep — old symbols must be gone
grep -rn "workspace_credit_state\|workspace_billing_state\|provisionWorkspaceCredit" src/ tests/ \
  | grep -v -E "deleted|historical|done/" || echo "ORP clean"

# E6: 350-line gate
git diff --name-only origin/main \
  | grep -v -E '\.md$|^vendor/|_test\.|\.test\.|\.spec\.|/tests?/' \
  | xargs -I{} sh -c 'wc -l "{}"' \
  | awk '$1 > 350 { print "OVER: " $2 ": " $1 }'

# E7: OpenAPI error compliance
make check-openapi-errors
```

---

## Out of Scope

- Team accounts (multi-user invites, shared dashboards, member roles beyond owner) — M16.
- Per-workspace credit quotas within a tenant pool — V3.
- Stripe / billing-adapter changes — adapter is unchanged; only the storage scope moves.
- Plan-tier UI redesign — Settings page keeps current visuals; just consumes new endpoints.
- Backfill from existing per-workspace billing rows — pre-v2.0 means no production data exists; teardown handles it.
- Per-workspace plan overrides (e.g. one workspace on Pro, another on Free within the same tenant) — explicitly excluded; tenant scope is intentional.
