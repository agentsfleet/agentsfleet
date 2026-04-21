# M11_006: Bootstrap-API-Key Removal + Credit-Balance Gate

**Prototype:** v2.0.0
**Milestone:** M11
**Workstream:** 006
**Date:** Apr 21, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — Unblocks "tenant billing is the only debit path" and closes the tenant-less auth escape hatch.
**Batch:** B3 — alpha-hardening; ships after M11_005 + M11_003 are live.
**Branch:** feat/m11-006-bootstrap-removal-balance-gate
**Depends on:** M11_005 (tenant-scoped billing — DONE, PR #240), M11_003 (Clerk signup bootstrap — DONE)

---

## §0 — Pre-EXECUTE Verification

**Status:** PENDING

Before touching any file, run these greps and confirm the surface matches this spec. If results diverge, amend the spec before EXECUTE.

```bash
# E0.1: Enumerate every live reference to the legacy bootstrap principal.
grep -rn "ZOMBIED_ADMIN_API_KEY\|api_key.bootstrap_env_var_used\|bootstrap_env_var" src/ docs/ vendor/ 2>/dev/null
```

**Expected surface (predicted — confirm at execute):**

| File | Expected reference | Action |
|------|--------------------|--------|
| `src/auth/middleware/*` | env-var API-key mint site that logs `api_key.bootstrap_env_var_used` | DELETE the branch + the warn log |
| `src/http/handlers/workspaces/lifecycle.zig` | `hx.principal.tenant_id orelse id_format.generateTenantId(...)` + `TODO(legacy-bootstrap)` provision call | REMOVE orelse fallback; reject null tenant as 403; DELETE the provision stopgap |
| `src/http/handlers/auth/github_callback.zig` | tenant-id fabrication + `TODO(legacy-bootstrap)` provision call | same: REMOVE fabrication; trust the OAuth `state` → existing tenant lookup; DELETE the provision stopgap |
| `compose/*.yml`, `fly.toml`, `.env.example`, `README.md`, `AGENTS.md` | any reference to `ZOMBIED_ADMIN_API_KEY` | REMOVE |
| `src/http/test_harness.zig` + integration tests | tests that authenticate via the bootstrap key | MIGRATE to a tenant-issued `zmb_t_...` key minted in setup |

```bash
# E0.2: Enumerate every metering/exhaustion site that needs the balance gate.
grep -rn "metering.exhausted\|metering.missing_tenant_billing\|balance_cents" src/ 2>/dev/null
```

**Expected balance-gate surface:**

| File | Expected reference | Action |
|------|--------------------|--------|
| `src/zombie/metering.zig` | `recordZombieDelivery` XACKs on `.exhausted` without halting the zombie | ADD gate path: write a `core.activity_events` row + set a tenant-level `balance_exhausted_at` flag so the worker can short-circuit future deliveries |
| `src/state/tenant_billing.zig` + store | no `balance_exhausted_at` column | ADD column + facade field (new migration; next free schema slot) |
| `src/http/handlers/tenant_billing.zig` | GET response lacks `is_exhausted` / `exhausted_at` | ADD those fields |
| `src/zombie/event_loop.zig` (or claim path) | no balance check before `deliverEvent()` | ADD pre-claim balance gate with configurable policy (see §2) |

If either grep surfaces references **outside these tables**, STOP and amend.

---

## Overview

**Goal (testable):** no env-var API key in the repo or prod; every authenticated request carries a `tenant_id` (enforced at middleware); a tenant whose `billing.tenant_billing.balance_cents` is 0 sees its zombie runs halt at a configurable policy boundary, with the exhausted state visible on `GET /v1/tenants/me/billing` and in `core.activity_events`.

**Problem:**

1. **Legacy bootstrap key.** `ZOMBIED_ADMIN_API_KEY` (env-var) mints an `api_key` principal with `role=admin, tenant_id=null` to let smoke-tests + admin CLI hit the API before Clerk signup existed. Post-M11_005 the `tenant_id=null` principal reaches `create_workspace` and `github_callback`, both of which fabricate a tenant_id on the fly — M11_005 papered this over with two `provisionFreeDefault` stopgaps (see `// TODO(legacy-bootstrap)` comments in both files). The escape hatch is now the only way an authenticated request can have no tenant. Remove the hatch, remove the stopgaps.

2. **Silent credit exhaustion.** M11_005's worker path `info`-logs `metering.exhausted` on a zero-balance debit and XACKs the event. The zombie keeps running free. There is no operator-visible signal, no dashboard banner, no API flag, no run-level halt. Pre-alpha this was acceptable; alpha needs either (a) a documented "free runs continue" policy *or* (b) a concrete gate. This spec ships (b) behind a policy knob so the default behavior is explicit and operators can see what's happening.

**Solution summary:**

- **Auth (decided at CHORE(open), supersedes earlier "decided during PLAN" wording):** delete the env-var principal entirely — no fallback, no bootstrap. Every authenticated request is either a Clerk session JWT or a `zmb_t_` tenant API key (M28_002). Admin gating uses Clerk `publicMetadata.role=admin`, set manually in the Clerk Dashboard for the one global admin user. Default role for all signups is `operator`; `admin` is a one-line manual promotion in Clerk (see §5 playbook). Programmatic admin access uses a regular `zmb_t_` key minted by the admin user via `POST /v1/api-keys`, stored at `op://ZMB_CD_<env>/usezombie-admin` field `api_key`. No "platform tenant" — the admin is a regular tenant whose Clerk user has `role=admin`. Middleware treats any authenticated-without-tenant_id request as `403 UZ-AUTH-001`. Two handler fabrication paths + their `provisionFreeDefault` stopgaps removed.
- **Balance gate:** new column `billing.tenant_billing.balance_exhausted_at BIGINT NULL`, set on the first `CreditExhausted` debit, cleared on any successful debit (which can only happen via top-up since we don't refund). New `is_exhausted` + `exhausted_at` fields on `GET /v1/tenants/me/billing`. A policy env var `BALANCE_EXHAUSTED_POLICY={continue|stop|warn}` (default `warn`) drives the worker pre-claim gate:
  - `continue` — current M11_005 behavior: log + run free.
  - `warn` — log at `warn`, emit a `balance_exhausted` activity event per delivery (one-shot per tenant/day), run free.
  - `stop` — pre-claim check rejects the event: worker does not run the zombie, activity event records `balance_gate_blocked`, run never starts.
- **Activity events:** new event types `balance_exhausted_first_debit` (on transition) and `balance_gate_blocked` (per blocked delivery under `stop`).

---

## Files Changed (blast radius — predicted)

```
SCHEMA GUARD: VERSION=<TBD at execute> (<2.0.0) → full teardown branch.
  Creating: schema/NNN_tenant_billing_exhaustion.sql   (ALTER column; pre-v2.0 alt: new file that DROP+CREATEs with the new shape; confirm at PLAN)
```

Actual action depends on VERSION at execute time — pre-v2.0 we drop + recreate `billing.tenant_billing` with the new column; at v2.0.0+ we use `ALTER TABLE ADD COLUMN`.

| File | Action | Why |
|------|--------|-----|
| `src/auth/middleware/*` (specific file TBD at PLAN) | MODIFY | Delete the env-var bootstrap branch + its warn log. Auth middleware rejects requests with no bearer or an unmatched bearer — no implicit admin principal. |
| `src/auth/rbac.zig` or equivalent | MODIFY | If admin role is still needed post-removal, wire it to Clerk JWT `metadata.role=admin` claim; document in spec header. |
| `src/http/handlers/workspaces/lifecycle.zig` | MODIFY | Remove `orelse id_format.generateTenantId(...)`; reject null tenant as 403. Remove `tenant_billing.provisionFreeDefault` stopgap. |
| `src/http/handlers/auth/github_callback.zig` | MODIFY | Same: remove tenant fabrication; trust OAuth `state` carries an existing tenant_id. Remove stopgap. |
| `src/zombie/metering.zig` | MODIFY | On first `CreditExhausted` debit, set `balance_exhausted_at`. Emit activity event per policy. |
| `src/zombie/event_loop.zig` (or claim path) | MODIFY | Under policy `stop`, skip `deliverEvent` when tenant balance is exhausted; record `balance_gate_blocked`. |
| `src/state/tenant_billing.zig` + `tenant_billing_store.zig` | MODIFY | Add `balance_exhausted_at` to `Billing` and `BillingRow`; new `markExhausted(tenant_id)` facade; `getBilling` returns the new field. |
| `src/http/handlers/tenant_billing.zig` | MODIFY | Response JSON gains `is_exhausted: bool` + `exhausted_at: ?i64`. |
| `compose/*.yml`, `fly.toml`, `.env.example`, `README.md`, `AGENTS.md` | MODIFY | Scrub `ZOMBIED_ADMIN_API_KEY` references. Add `BALANCE_EXHAUSTED_POLICY` to env templates with default `warn`. |
| `src/http/test_harness.zig` | MODIFY | Replace bootstrap-key helper with a `mintTenantApiKey()` that inserts into the real `api_keys` table during setup. |
| Integration tests using the bootstrap key | MIGRATE | Grep for `ZOMBIED_ADMIN_API_KEY` in test env; swap for the minted key. |
| `public/openapi/paths/billing.yaml` | MODIFY | Add `is_exhausted`, `exhausted_at` to the tenant billing response schema. |
| `docs/changelog.mdx` | MODIFY | New `<Update>` block at CHORE(close). Tags `["What's new","API","Billing","Breaking","Internal"]`. |

---

## Sections

### §1 — Bootstrap principal removal

**Dimensions:**

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 1.1 | PENDING | `src/auth/middleware/*` | request with bearer matching `ZOMBIED_ADMIN_API_KEY` (after removal) | `401 UZ-AUTH-002` (bearer does not resolve to any principal) | integration |
| 1.2 | PENDING | `workspaces/lifecycle.zig` | Clerk JWT with no `metadata.tenant_id` | `403 UZ-AUTH-001`; no `core.tenants` row created | integration |
| 1.3 | PENDING | `auth/github_callback.zig` | OAuth callback with a `state` that does not resolve to an existing tenant | `403 UZ-AUTH-001`; no tenant/workspace written | integration |
| 1.4 | PENDING | admin surface | authenticated operator without admin claim hits `/v1/admin/platform-keys` | `403 UZ-AUTH-009 ERR_INSUFFICIENT_ROLE` | integration (existing rbac suite) |
| 1.5 | PENDING | env templates | `grep ZOMBIED_ADMIN_API_KEY .` anywhere in repo (excluding historical `docs/v*/done`) | 0 matches | lint |
| 1.6 | PENDING | `src/auth/claims.zig` + `src/auth/rbac.zig` | Clerk JWT with `publicMetadata.role=admin` | `principal.role == .admin`; admin-only endpoints accept the request | integration |
| 1.7 | PENDING | signup webhook (`src/http/handlers/webhooks/clerk.zig`) | fresh `user.created` event for a brand-new user | After webhook returns 200: Clerk user's `publicMetadata` contains both `tenant_id=<new uuid>` AND `role="operator"`. Verified by a second request carrying that user's JWT that reads the claims back. If the current webhook does not write `role=operator`, this dim covers adding that write. | integration |

### §2 — Balance-gate column + activity events

**Dimensions:**

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 2.1 | DONE | `tenant_billing` schema | fresh DB migrate | column `balance_exhausted_at BIGINT NULL` present; default NULL | integration (tier-3) |
| 2.2 | DONE | `tenant_billing.markExhausted` | tenant at 0¢, `markExhausted` called | row's `balance_exhausted_at` set to now_ms; second call idempotent | unit |
| 2.3 | DONE | metering transition | tenant 5¢ balance, debit 10¢ | `CreditExhausted` returned AND `balance_exhausted_at` set AND one `balance_exhausted_first_debit` activity event written | integration |
| 2.4 | DONE | metering replay | already-exhausted tenant, another debit 10¢ | still `CreditExhausted`; `balance_exhausted_at` unchanged; NO duplicate activity event | integration |

### §3 — Exhaustion policy knob

**Dimensions:**

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 3.1 | DONE | `continue` policy | `BALANCE_EXHAUSTED_POLICY=continue`, exhausted tenant, zombie event | event delivered; zero cents deducted; no activity event beyond the one-shot in §2.3 | integration |
| 3.2 | DONE | `warn` policy (default) | unset or `warn`, exhausted tenant, zombie event | event delivered; `balance_exhausted` activity event written (rate-limited to 1/tenant/day) | integration |
| 3.3 | DONE | `stop` policy | `BALANCE_EXHAUSTED_POLICY=stop`, exhausted tenant, zombie event | `deliverEvent` NOT called; `balance_gate_blocked` activity event written; event still XACKed so Redis doesn't retry | integration |
| 3.4 | DONE | policy config parse | env var parsing | known values accepted; unknown value defaults to `warn` with a startup warn log | unit |

### §4 — Response surface

**Dimensions:**

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 4.1 | DONE | `GET /v1/tenants/me/billing` on non-exhausted tenant | Bearer JWT | `200 {plan_tier, plan_sku, balance_cents, updated_at, is_exhausted: false, exhausted_at: null}` | integration |
| 4.2 | DONE | same on exhausted tenant | Bearer JWT | `200 {... is_exhausted: true, exhausted_at: <epoch_ms>}` | integration |
| 4.3 | DONE | openapi schema | Redocly lint + router parity | public/openapi.json round-trips with new fields | lint |

### §5 — Admin Bootstrap Playbook (authored, not executed)

**Status:** PENDING

Deliver `playbooks/012_usezombie_admin_bootstrap/001_playbook.md`. The playbook is authored in this milestone but **not executed as part of the merge** — it is run manually per environment (dev, prod) when the operator is ready for live testing. Step 1 (signup via website) and Step 2 (manual Clerk Dashboard role flip) are **human-only**. Steps 3–4 (key mint + vault write) are agent-executable via `curl` + `op` CLI.

**Dimensions:**

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 5.1 | PENDING | `playbooks/012_usezombie_admin_bootstrap/001_playbook.md` | file exists | Markdown file with Human/Agent split table, step-by-step instructions, acceptance criteria per step, vault paths resolved via `op` CLI, idempotency notes on each step. | lint |
| 5.2 | PENDING | playbook correctness | agent reads playbook, extracts the `curl` for key minting | `POST /v1/api-keys` payload shape matches M28_002 contract (`{"key_name":"admin-cli"}`); response path `$.key` written to `op://ZMB_CD_<env>/usezombie-admin` field `api_key` | manual |

---

## Interfaces

```zig
// src/state/tenant_billing.zig (additions)
pub fn markExhausted(conn: *pg.Conn, tenant_id: []const u8) !bool; // returns true if transition happened
pub const Billing = struct {
    plan_tier: []const u8,
    plan_sku: []const u8,
    balance_cents: i64,
    grant_source: []const u8,
    updated_at_ms: i64,
    exhausted_at_ms: ?i64, // NEW
};

// src/zombie/metering.zig (DeductionResult variants unchanged after M11_005)

// new env var
BALANCE_EXHAUSTED_POLICY = "continue" | "warn" | "stop"  // default "warn"
```

### Output contract — `GET /v1/tenants/me/billing`

| Field | Type | When | Example |
|-------|------|------|---------|
| `plan_tier` | string | always | `"free"` |
| `plan_sku` | string | always | `"free_default"` |
| `balance_cents` | i64 | always | `995` |
| `updated_at` | i64 (epoch ms) | always | `1713700000000` |
| `is_exhausted` | bool | always | `false` |
| `exhausted_at` | i64 (epoch ms) or null | null when not exhausted | `1713700400000` |

---

## Failure Modes

| Failure | Trigger | System behavior | User observes |
|---------|---------|-----------------|---------------|
| Auth middleware still carries bootstrap branch after deploy | rollback raced forward | `auth.bootstrap_env_var_used` log absent in prod; `§1` integration test fails | |
| Tenant exhausts exactly at delivery boundary | last debit takes balance to 0 | `markExhausted` runs in the same tx as the successful debit that hit 0; `balance_exhausted_first_debit` written once | `is_exhausted: true` on next GET |
| Policy env var missing | process start with unset `BALANCE_EXHAUSTED_POLICY` | defaults to `warn`; startup emits one `info` log naming the default | |
| Admin without Clerk admin claim hits `/v1/admin/platform-keys` after key removal | Clerk config mis-set | `403 UZ-AUTH-009`; no bootstrap fallback | admin must fix their Clerk metadata |

---

## Implementation Constraints

| Constraint | How to verify |
|-----------|---------------|
| SQL file ≤100 lines, single-concern | `wc -l schema/NNN_*.sql` |
| `markExhausted` atomic (one UPDATE, no read-then-write race) | SQL is `UPDATE … WHERE tenant_id=$1 AND balance_exhausted_at IS NULL RETURNING balance_exhausted_at` |
| Activity event rate-limit (1/tenant/day for `warn`) is stateless-correct | test dim 3.2 replays deliveries under the same day, asserts single event |
| Grep orphan sweep: `ZOMBIED_ADMIN_API_KEY`, `api_key.bootstrap_env_var_used`, `generateTenantId` (outside `signup_bootstrap`), `TODO(legacy-bootstrap)` gone from non-historical files | `grep -rn ... src/ | grep -v done/` → 0 |
| Cross-compile green | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` |
| Drain discipline | `make check-pg-drain` |
| Per-file FLL gate | `wc -l` < 350 on every touched `.zig` |

---

## Out of Scope

- Stripe top-up / purchase flows — next milestone.
- Un-exhaustion path (how `balance_exhausted_at` gets cleared). Top-up milestone handles this; pre-Stripe, the only way out is admin manual reset via psql.
- ~~Refactoring Clerk role claims — if admin auth via Clerk claim isn't already wired, PLAN stage can extract it into a sibling workstream `M11_006_b`.~~ Decided at CHORE(open): one workstream, Clerk `publicMetadata.role=admin` is the chosen path. Extractor already reads `metadata.role` (`src/auth/claims.zig:62`); this workstream maps `"admin"` to the admin RBAC enum variant.
- Automated Clerk user creation — the admin bootstrap playbook (§5) is manual. A future infra milestone may automate Clerk user provisioning via the Backend API; out of scope for M11_006.
- Server-side admin allowlist / tenant-id cross-check for the `role=admin` claim — trust boundary is Clerk Dashboard access itself (protected by 1Password + 2FA). A second check against a server-side list would be theater; an attacker with Clerk write access can forge any claim.
- Per-workspace billing — killed by M11_005; do not resurrect.
- UI banners / dashboard affordances for exhausted state — separate M19 workstream pulls `is_exhausted` into the UI.

---

## Discovery

(to be filled during EXECUTE, per AGENTS.md Legacy-Design Consult Guard)

---

## Eval Commands

```bash
# E1: Zig build
zig build 2>&1 | tail -5; echo "zig_build=$?"

# E2: Tier-3 fresh DB + full integration
make down && make up && make test-integration 2>&1 | tail -10

# E3: Cross-compile
zig build -Dtarget=x86_64-linux 2>&1 | tail -3
zig build -Dtarget=aarch64-linux 2>&1 | tail -3

# E4: Drain + gitleaks + lint
make check-pg-drain
make lint 2>&1 | grep -E "PASS|FAIL"
gitleaks detect 2>&1 | tail -3

# E5: Orphan sweep — bootstrap key surface must vanish
grep -rn "ZOMBIED_ADMIN_API_KEY\|api_key.bootstrap_env_var_used\|TODO(legacy-bootstrap)" src/ schema/ docs/ \
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

## Acceptance Criteria

- [ ] `grep ZOMBIED_ADMIN_API_KEY src/ schema/ compose/ ui/ .env.example README.md AGENTS.md fly.toml` → 0 matches
- [ ] Clerk JWT with no `tenant_id` claim hits `create_workspace` → `403`; no rows written
- [ ] `GET /v1/tenants/me/billing` includes `is_exhausted` + `exhausted_at`
- [ ] `BALANCE_EXHAUSTED_POLICY=stop` + exhausted tenant → zombie delivery is short-circuited; activity event recorded
- [ ] `BALANCE_EXHAUSTED_POLICY=warn` (default) → rate-limited activity event, run continues
- [ ] Tier-3 gate green; cross-compile both targets green; `make lint`, `make check-pg-drain`, `gitleaks` green
- [ ] Both `TODO(legacy-bootstrap)` comments and the `provisionFreeDefault` stopgap calls removed from `workspaces/lifecycle.zig` and `auth/github_callback.zig`
- [ ] `docs/changelog.mdx` has a new `<Update>` block covering the auth break + the new billing fields
