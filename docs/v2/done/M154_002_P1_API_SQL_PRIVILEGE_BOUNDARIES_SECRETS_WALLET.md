<!--
SPEC AUTHORING RULES (load-bearing — the one comment that survives):
- Body order = the executing agent's read order. Fill via the kishore-spec-new
  skill (authoring order lives there); after filling, DELETE every "tpl:"
  guidance comment — the SPEC TEMPLATE GATE blocks tpl residue, unfilled
  {slots}, and missing required sections (audits/spec-template.sh --staged).
- No time/effort/hour/day estimates anywhere. No effort columns, complexity
  ratings, percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only
  sequencing signal. A section that contradicts these rules loses — delete it.
-->

# M154_002: The secret store and the wallet get their own privileges

**Prototype:** v2.0.0
**Milestone:** M154
**Workstream:** 002
**Date:** Aug 01, 2026
**Status:** DONE
**Priority:** P1 — a security boundary that exists in prose and not in grants
**Categories:** API, SQL
**Batch:** B1 — its own Pull Request, both halves together; the grants live in slots M154_001 authors
**Branch:** feat/m154-privilege-boundaries
**Test Baseline:** unit=3512 integration=589
**Depends on:** M154_001 (merged first) — the grants land in the slots it re-authors, so those slots must exist for this to apply. Its §1 revoke and §2 elevation ship together or not at all: the revoke alone refuses every signup, because the starter grant is written inside the tenant-create transaction
**Provenance:** LLM-drafted (Claude Opus 5, Aug 01, 2026), from a grant-level audit of the shipped schema
**Canonical architecture:** `docs/architecture/runner_fleet.md` §the control-plane/data-plane split · `docs/AUTH.md`

---

## Overview

**Goal (testable):** Selecting a ciphertext or updating a balance as `api_runtime`, without elevating first, is refused by PostgreSQL.

**Problem:** `api_runtime` is the role every Hypertext Transfer Protocol handler runs as, and it holds direct grants on `vault.secrets` and the tenant wallet. So the schema separation those tables sit behind protects nothing: any handler, and any injection or logic bug inside one, can read every stored ciphertext and move any balance. The architecture prose describes a trust boundary between the control plane and the data plane that no privilege enforces. Meanwhile `memory` already demonstrates the working shape — zero direct grants, reachable only after elevating — so the pattern is proven in this codebase and simply not applied where the stakes are highest.

**Solution summary:** Two roles are introduced, one for the secret store and one for the wallet, and the grants that currently sit on `api_runtime` move onto them. `api_runtime` is granted membership so it can elevate for the span of one transaction and no longer. Elevation is scoped to the transaction rather than the connection, and the pool refuses to hand back a connection that is still elevated — so a forgotten reset becomes a loud failure instead of a privilege leak into the next request.

## PR Intent & comprehension handshake

- **PR title (eventual):** `feat(m154): the secret store and the wallet get their own privileges` — *(amended at EXECUTE: M154_001 merged alone as PR #587, so the original "shared title" plan is stale and this workstream ships as its own PR)*
- **Intent (one sentence):** A bug in an unrelated handler should be unable to read a secret or move money, as a matter of privilege rather than of code review.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `src/agentsfleetd/memory/fleet_memory.zig` and `http/handlers/memory/helpers.zig` — the elevation pattern that already works here, and the boundary comments that explain what runs under which role.
2. `src/agentsfleetd/db/pg_query.zig` and the pool acquire/release path — where the reset backstop has to live to be structural rather than advisory.
3. `src/agentsfleetd/fleet/renewal.zig` and `renewal_settle.zig` — the wallet writers, both single fenced statements, and the reason elevation must not break their atomicity.
4. `docs/AUTH.md` — the credential posture the secret store sits behind; this workstream narrows who can read ciphertext, it does not change how ciphertext is sealed.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `schema/110_roles_and_privileges.sql` | EDIT | Declares the two roles and grants `api_runtime` **non-inheriting** membership |
| `schema/120_metering_role.sql` | CREATE | The composite `metering_runtime` role, whose grain is the fenced statement rather than a table (§1) |
| `schema/300_vault_secrets.sql`, `schema/700_tenant_wallet.sql` | EDIT | Table grants land on the owning role, not on `api_runtime` |
| `schema/610_runner_leases.sql`, `630_runner_affinity.sql`, `650_runner_lifetime_counters.sql` | EDIT | The fenced statement's `fleet` footprint grants to `metering_runtime` (RULE SGR) |
| `schema/710_usage_ledger.sql` | EDIT | `api_runtime` keeps SELECT — a charge history does not move, and four readers need it |
| `src/agentsfleetd/db/schema_privilege_test.zig` | CREATE | Unit proof of the boundary against the embedded slot text, where a superuser connection cannot hide it |
| `src/agentsfleetd/state/vault.zig`, `secrets/*.zig` | EDIT | Secret reads and writes elevate for the statement's transaction |
| `src/agentsfleetd/fleet/renewal.zig`, `renewal_settle.zig` | EDIT | The two wallet writers elevate around their fenced statement |
| `src/agentsfleetd/state/tenant_billing*.zig`, `http/handlers/tenant_billing.zig` | EDIT | Balance reads elevate; the charges read does not (the ledger is not moving) |
| `src/agentsfleetd/db/pool*.zig` | EDIT | Release refuses a still-elevated connection |
| `src/agentsfleetd/state/account_teardown.zig` | EDIT | The purge deletes secrets and the wallet row, so it elevates too |
| `docs/architecture/runner_fleet.md` | EDIT | Corrects the trust-boundary claim to describe what grants now enforce |

**Amended at EXECUTE — blast radius the authoring pass under-counted** (each a discovery, none opportunistic):

| File | Action | Why |
|------|--------|-----|
| `schema/embed.zig` | EDIT | One-line registration of slot 120 (the migration array IS this file) |
| `src/agentsfleetd/errors/error_registry.zig`, `errors/error_entries.zig` | EDIT | The two registered codes: `UZ-INTERNAL-004` (elevation refused), `UZ-INTERNAL-005` (elevated release refused) |
| `src/agentsfleetd/db/pool_elevation.zig` | CREATE | The elevation module: `Elevated(role)` typestate handles + `withRole` closure scopes (within the `pool*.zig` glob above; named for greppability) |
| `src/agentsfleetd/db/schema_privilege_test.zig`, `db/schema_privilege_integration_test.zig` | CREATE | The unit and integration proof tiers |
| `src/agentsfleetd/state/fleet_telemetry_store.zig` | EDIT | A ledger WRITER the spec's inventory missed — the per-event stage row elevates to `billing_runtime` |
| `src/agentsfleetd/state/secret_reference_txn.zig` | EDIT | Its step-1 `FOR UPDATE` on `vault.secrets` needs `vault_runtime` inside the protocol's own transaction |
| `src/agentsfleetd/state/workspace_onboarding.zig` + `workspace_onboarding/sql.zig` | EDIT | The onboarding signal probe spanned `core` + `vault` in one statement; split so the vault EXISTS elevates alone |
| `src/agentsfleetd/state/tenant_model_entries.zig` + `tenant_model_entries/sql.zig` | EDIT | Same split for the primary-workspace secret probe |
| `src/agentsfleetd/fleet_library/store.zig` | EDIT | The fleet-library install check probes `vault.secrets` presence — elevates |
| `src/agentsfleetd/secrets/metadata_backfill.zig`, `cmd/backfill.zig` | EDIT | The backfill's vault UPDATE elevates; the stale grant comment corrected |
| `src/agentsfleetd/integration_tests.zig` | EDIT | Roster line for the new integration suite |
| ~50 files under `cmd/`, `fleet/`, `fleet_runtime/`, `http/`, `cron/`, `auth/`, `credentials/`, `memory/`, `events/`, `db/`, `state/` | EDIT | Mechanical `*pg.Pool` → `*db.Pool` type respell so every borrower passes through the wrapper's release backstop; test files gain `db.adopt` at construction seams |
| `build.zig.zon` | EDIT | pg.zig re-pinned to fork tag `v0.0.0-af.4`: upstream `b5a1f25` merge + the `peekForError` use-after-reset fix the coverage lane exposed (Indy-directed; Discovery) |
| `scripts/check-migrate-unprivileged.sh` | EDIT | Its pre-existing-role list mirrors the ADMIN OPTION a managed migrator holds from having created each role. The three new elevation roles belong in it, or slot 110's membership GRANT is refused with 42501 (CI discovery; Indy-approved) |
| `scripts/check_migrate_role_parity.sh` | CREATE | Static gate: the lane's `APP_ROLES` must equal the roles `schema/` creates. Turns a twelve-minute coverage-lane 42501 into a one-second failure naming the omitted role |
| `make/check-safety-gates.mk`, `make/quality.mk` | EDIT | The parity gate joins the file's other static tree checks and `lint-all`'s prerequisites |

**Amended — free-trial removal folded in (Indy-directed, see Discovery).** Removing the mechanism retires the merge-blocker below rather than guarding it, so the two land together:

| File | Action | Why |
|------|--------|-----|
| `schema/700_tenant_wallet.sql` | EDIT | The `free_trial_ends_at` column is dropped — nothing in production ever wrote it |
| `src/agentsfleetd/state/tenant_billing.zig` | EDIT | `isFreeTrialActive`, `FREE_TRIAL_STAGE_NANOS`, and the two `Billing` projection fields are deleted |
| `src/agentsfleetd/state/tenant_billing_rates.zig` | EDIT | The trial short-circuit ahead of the posture switch is deleted; `computeStageChargeAt` goes with it (the injected clock existed only to price around the trial) |
| `src/agentsfleetd/state/tenant_billing_store.zig`, `state/sql.zig` | EDIT | `loadTrialBoundary` + `SELECT_TENANT_TRIAL_BOUNDARY` deleted; the balance SELECT drops the column |
| `src/agentsfleetd/fleet/renewal_meter.zig` | EDIT | The boundary load disappears, and with it the `catch null` that priced a failed lookup as a live trial |
| `src/agentsfleetd/fleet_runtime/metering.zig`, `fleet/service_renew.zig`, `fleet/service_report.zig` | EDIT | Call sites drop the trial argument; `buildMeterInputs` sheds `tenant_id` and `now_ms` (RULE NDC) |
| `src/agentsfleetd/http/handlers/tenant_billing.zig` | EDIT | `free_trial` leaves the billing response — a breaking API change |
| `src/agentsfleetd/db/test_fixtures.zig` | EDIT | `endFreeTrialFor` + `TRIAL_ENDED_AT_MS` deleted; the only writer of the column was this fixture |
| `ui/packages/app/lib/types.ts`, `ui/packages/app/tests/*.ts` | EDIT | `TenantBilling.free_trial` removed from the typed surface and its fixtures |
| `ui/packages/website/src/lib/rates.ts` | EDIT | The `FREE_TRIAL_STAGE_NANOS` mirror is deleted. **Marketing copy stays** — `FREE_TRIAL_PILL` / `FREE_TRIAL_BANNER` describe a $5 starter grant, which is still true |
| 9 billing/metering integration suites | EDIT | Trial fixtures dropped; assertions now hold unconditionally because no clock gates pricing |

## Applicable Rules

- **`~/Projects/dotfiles/docs/greptile-learnings/RULES.md`** — **SGR** (a grant belongs with the table it describes, and every role that queries a table appears there), **CTX** (cross-tenant and cross-trust data needs a process boundary, which here is the role), **VLT** (secrets stay in the vault; this narrows who may read them), **NSQ** (schema-qualified statements), **OWN** (one owner per resource — elevation is acquired and released by the same scope), **TXN** (a transaction that fails rolls back, which is what makes transaction-scoped elevation safe)
- `~/Projects/dotfiles/docs/SCHEMA_CONVENTIONS.md` — grants accompany their `CREATE TABLE`
- `dispatch/write_zig.md` — every call-site change is Zig; `errdefer` placement matters because elevation is a resource
- `docs/AUTH.md` — read before touching the secret path

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| SCHEMA GUARD | yes — role and grant changes in `schema/` | rides M154_001's teardown posture; no `ALTER`, no `DROP` |
| ZIG GATE | yes — stores, handlers, pool | `errdefer` releases elevation on every error path; cross-compile both linux targets |
| LIFECYCLE | yes — elevation is acquire/release | the reset is paired structurally, proven by the pool backstop test |
| PUB / Struct-Shape | yes — the elevation helper is new public surface | shape verdict at PLAN |
| UFS | yes — role names are repeated literals | named constants shared by the pool and the call sites |
| LOGGING / ERROR REGISTRY | yes — a refused elevation needs a registered code | register it; no bare literal message |

## Prior-Art / Reference Implementations

- **Reference:** `src/agentsfleetd/memory/fleet_memory.zig` — the in-repo proof that a role boundary works here: `api_runtime` holds nothing on `memory.*` and handlers elevate per request. This workstream generalises it and tightens the scope from connection to transaction.
- **Divergence:** the memory path elevates for the request; this one elevates for the transaction, because a pooled connection that keeps elevation between statements is exactly the leak the pool backstop exists to catch.

## Sections (implementation slices)

### §1 — Two roles own the two tables

The grants that sit on `api_runtime` today move to roles that do nothing else, and `api_runtime` is granted membership so it can assume them. Nothing about how secrets are sealed changes — this narrows who may read the sealed bytes. **Implementation default:** one role per table rather than one shared elevated role, because a wallet writer has no reason to read ciphertext and the whole point is that reach is enumerable.

**Amended — the grain is the table EXCEPT where one statement spans two schemas.** The settle and renewal paths are each a single fenced statement touching three `fleet` tables plus the wallet and the ledger, and `SET ROLE` replaces the privilege set rather than adding to it, so no per-table role can carry them. `metering_runtime` (`schema/120`) is composed to that statement's footprint: a member of `billing_runtime`, plus direct grants on exactly those three `fleet` tables. Reach stays enumerable — the grant list *is* the statement's table list — and the fenced statement is not modified. The alternatives were widening `billing_runtime` across the control plane, splitting a statement whose atomicity is what makes a replayed renewal charge nothing, or leaving the wallet unfenced.

**Membership must be non-inheriting.** A bare `GRANT <role> TO api_runtime` follows `api_runtime`'s own INHERIT attribute, which `CREATE ROLE` defaults to TRUE — the privileges then apply ambiently and nothing ever elevates. `WITH INHERIT FALSE, SET TRUE` is what makes membership dormant, and Dimension 1.1's catalogue query cannot see the difference, which is why Dimension 1.4 exists.

- **Dimension 1.1** — `api_runtime` holds no direct privilege on the secret store or the wallet → Test `test_api_runtime_holds_no_direct_grant` — **DONE**
- **Dimension 1.2** — an unelevated read of either table is refused by PostgreSQL, not by application code → Test `test_unelevated_access_is_refused` — **DONE**
- **Dimension 1.3** — the migration role retains full authority, so a rebuild cannot lock itself out → Test `test_migrator_still_owns_both_tables` — **DONE**
- **Dimension 1.4** — every role `api_runtime` may assume is granted non-inheriting, so the privilege is unreachable without an explicit `SET ROLE` → Test `test_role_membership_is_dormant_until_set_role` — **DONE**
- **Dimension 1.5** — `metering_runtime` reaches exactly the fenced statement's tables and holds no direct grant on either money table → Test `test_metering_role_matches_statement_footprint` — **DONE**

### §2 — Elevation is scoped to the transaction

Elevation lasts for one transaction and ends with it, so a commit or a rollback both return the connection to `api_runtime` without anything having to remember. The wallet writers are already single fenced statements, so wrapping them costs nothing structurally. **Implementation default:** transaction-scoped elevation rather than connection-scoped, because the connection is pooled and its next borrower is a different request.

- **Dimension 2.1** — every secret read and write succeeds under elevation and the transaction still commits atomically → Test `test_secret_paths_work_under_elevation` — **DONE**
- **Dimension 2.2** — the metered renewal and the settle both still charge exactly once under elevation, with fencing unchanged → Test `test_metering_unchanged_under_elevation` — **DONE**
- **Dimension 2.3** — a failed statement inside an elevated transaction rolls back and leaves no elevation behind → Test `test_rollback_clears_elevation` — **DONE**
- **Dimension 2.4** — account erasure removes secrets and the wallet row under elevation → Test `test_erasure_elevates_for_secrets_and_wallet` — **DONE**

### §3 — The pool refuses to hand back an elevated connection

The failure this workstream must not introduce is a connection returned to the pool still elevated, which would hand the next request privileges it never asked for — strictly worse than the situation being fixed. The guard belongs in release, where it cannot be forgotten, rather than in each call site.

- **Dimension 3.1** — releasing a still-elevated connection is refused and reported, never silently accepted → Test `test_release_rejects_elevated_connection` — **DONE**
- **Dimension 3.2** — a connection that has completed an elevated transaction reports the base role and is reusable → Test `test_connection_returns_to_base_role` — **DONE**

### §4 — The free trial is deleted, not guarded

Folded in at Indy's direction. The elevation work grew the failure surface of one line — `loadTrialBoundary(...) catch null` in the renewal meter — from a single SELECT to `mark + BEGIN + SET LOCAL ROLE + SELECT + COMMIT`, and added a dependency on a `billing_runtime` membership that a pre-split cluster does not have. Because `isFreeTrialActive(null)` returns TRUE, any failure along that longer path priced the slice as a live trial: every rate collapsed to zero, the fenced statement wrote a zero charge, and `last_metered_at` advanced — so the slice could never be re-billed. Silent, permanent, and unlogged.

The mechanism is redundant, which is why deleting beats guarding. `tenant_billing.STARTER_CREDIT_NANOS` already grants every new tenant $5 at signup as `bootstrap_starter_grant`, so the platform has an explicit, auditable "new user tries it free" path. The trial was a second, implicit one that worked by zeroing every rate. **Measured before removal:** `free_trial_ends_at` has no `DEFAULT` and exactly one writer in the repository — `db/test_fixtures.zig`, a test fixture. No production path ever set it, so every tenant held NULL, read as an open-ended trial, and was charged nothing for any stage. This removal switches stage billing on for the first time; the $5 grant is the free allowance and the existing `balance_exhausted_at` path handles running out. Admin-granted credits are the named follow-up.

- **Dimension 4.1** — no pricing path consults a clock or a trial boundary; the same inputs price identically at any instant → Test `test_pricing_is_clock_independent` — **DONE**
- **Dimension 4.2** — a metered run that consumed time is charged for it, so no slice settles at zero while advancing the cursor → Test `test_every_slice_is_charged` — **DONE**
- **Dimension 4.3** — a fresh tenant holds exactly the starter grant, and it is positive → Test `test_fresh_tenant_funded_by_starter_grant_alone` — **DONE**
- **Dimension 4.4** — the billing response no longer carries `free_trial`, and the typed dashboard surface matches → Test `test_billing_response_has_no_free_trial` — **DONE**

## Interfaces

```
BREAKING   GET /v1/tenants/me/billing drops the `free_trial` object
           (`{ active, ends_at_ms }`). Every other field is unchanged, and
           no path, status code, or request shape moves.

Otherwise no HTTP surface changes. Every endpoint keeps its path, request
shape, response shape, and status codes.

INTERNAL   an elevation helper wrapping a transaction under a named role,
           releasing on both the commit and the error path.
           Callers: the secret store, the two wallet writers, the balance
           read, and account erasure.

INTERNAL   pool release gains a base-role assertion. A connection whose
           effective role is not the base role is refused, reported under a
           registered error code, and not returned to the pool.
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Leaked elevation | An elevated transaction ends without the role reverting | Release refuses the connection under a registered code; it is not reused |
| Missing grant found at runtime | A path needs a table the elevated role was not granted | PostgreSQL refuses; the handler answers a 500 with the registered code, never a partial write |
| Elevation inside a failed transaction | A statement errors mid-transaction | Rollback ends the transaction and the elevation with it; no manual cleanup |
| Migrator locked out | Roles are created without the migration role retaining authority | Bootstrap fails at the slot, loudly, before any later slot runs |
| Nested elevation | A path elevates while already elevated | Refused rather than nested, so the release contract stays single-owner (RULE OWN) |
| Purge cannot reach a table | Erasure runs unelevated against secrets or the wallet | The transaction fails and rolls back; the account is not partially erased |

## Invariants

1. **`api_runtime` holds zero direct privilege on the secret store and the wallet** — asserted from the catalogue, so a future grant re-widening the role fails the test.
2. **Elevation ends with its transaction** — enforced by scope, not by a reset call a path could skip.
3. **A connection is never pooled while elevated** — enforced at release, the one place every borrower passes through.
4. **The migration role retains full authority on both tables** — otherwise a rebuild cannot re-author them.
5. **Sealing is unchanged** — the envelope, its key versions, and its bound data are untouched; only reach narrows.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| refused-release counter | ops | A connection is refused at release for still being elevated | count only | no tenant, workspace, or role-holder identity | `test_release_rejects_elevated_connection` |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | integration | `test_api_runtime_holds_no_direct_grant` | A catalogue query for privileges held by `api_runtime` on either table returns zero rows |
| 1.2 | integration | `test_unelevated_access_is_refused` | A select and an update on each table as `api_runtime` both raise insufficient_privilege |
| 1.3 | integration | `test_migrator_still_owns_both_tables` | The migration role can re-create and grant on both tables from empty |
| 1.4 | unit | `test_role_membership_is_dormant_until_set_role` | Every `GRANT <role> TO api_runtime` in the embedded slots carries `WITH INHERIT FALSE, SET TRUE`; the count of such grants is asserted, so a scan that matches nothing fails |
| 1.5 | unit | `test_metering_role_matches_statement_footprint` | Object grants to `metering_runtime` cover exactly the three `fleet` tables the fenced statement names, and none on the wallet or ledger (those arrive by membership, which is asserted inheriting) |
| 2.1 | integration | `test_secret_paths_work_under_elevation` | Store, read and delete a secret end to end; the ciphertext round-trips unchanged |
| 2.2 | integration | `test_metering_unchanged_under_elevation` | A run with three renewals plus settle debits the same total as before, once |
| 2.3 | integration | `test_rollback_clears_elevation` | A deliberately failing statement leaves the connection reporting the base role |
| 2.4 | integration | `test_erasure_elevates_for_secrets_and_wallet` | Erasing a tenant with stored secrets and a balance leaves zero rows in both |
| 3.1 | unit | `test_release_rejects_elevated_connection` | Releasing a connection whose role was left elevated is refused and counted |
| 3.2 | integration | `test_connection_returns_to_base_role` | After an elevated transaction commits, the same connection serves an unelevated read |
| regression | integration | `test_no_endpoint_behaviour_changed` | Billing, secret and model endpoints return identical shapes and codes to before |
| regression | integration | `test_memory_elevation_still_works` | The pre-existing memory role path is unaffected by the new helper |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | No grant on the two tables names `api_runtime` (§1) | `grep -nE "GRANT.*(vault\.secrets\|tenant_wallet).*api_runtime" schema/` | no output | P0 | ✅ no output (grep exit 1) |
| R2 | Every elevated path releases on the error path too (§2) | `grep -rn "elevate" src/agentsfleetd --include='*.zig' \| grep -v errdefer \| grep -v _test` | reviewed: every hit is inside a scope that releases | P0 | ✅ 12 non-comment hits: 8 module-internal (`pool_elevation.zig`), 4 error-catalog prose; every call site rides `withRole`'s releasing scope; only raw `SET ROLE` outside the module is the documented legacy memory-handler path |
| R3 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | ✅ 103 paths, 0 outside the table (`state/` added to the respell row; `build.zig.zon` row added with the af.4 re-pin — both at VERIFY) |
| S1 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | ✅ Zig lanes + coverage green: agentsfleetd 2111 pass/284 skip/0 fail · runner 414/7/0 · lib 157+6 · zig coverage 87.40% ≥ 83. The TypeScript acceptance lane fails 7 auth-guard tests on logged-in machines only (pre-existing `composeEnv` HOME leak × M160 durable credential — Indy-acked to another agent's M160_001 stream; Discovery + PR note) |
| S2 | Lint clean | `make lint-all` | exit 0 | P0 | ✅ "✓ All lint checks passed" |
| S3 | Integration passes | `make test-integration` | exit 0 | P0 | ✅ exit 0 — "✓ Full integration suite passed", 0 failing tests |
| S5 | No leaks | `make memleak` | exit 0 | P0 | ✅ "✓ memleak gate passed (agentsfleetd + runner + lib lanes + boot→drain lifecycle)" |
| S6 | Cross-compile | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` | exit 0 | P0 | ✅ `X86_64 OK` · `AARCH64 OK` (final tree) |
| S7 | No secrets | `gitleaks detect` | exit 0 | P0 | ✅ "no leaks found" — 4248 commits scanned |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE; a P1 ❌ ships only with an Indy-acked deferral quote in Discovery.

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

N/A — no files deleted.

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| direct `api_runtime` grants on the two tables | `grep -rnE "(vault\.secrets\|tenant_wallet)[^;]*api_runtime" schema/` | 0 matches |

## Out of Scope

- **Row-Level Security** — its own milestone. This workstream narrows *which role* reaches a table; Row-Level Security narrows *which rows* a tenant sees. They compose but are independent.
- **Splitting `api_runtime` further** — the control-plane/data-plane boundary between `core` and `fleet` stays unenforced. Naming it honestly in the architecture doc is in scope; enforcing it is not.
- **Changing how secrets are sealed** — key versions, bound data and the envelope are untouched.

---

## Product Clarity (authoring record)

1. **Successful user moment** — none visible, and that is the intent: every endpoint behaves identically. The moment belongs to the operator who, reading an incident report about a handler bug, can say the blast radius stopped at the role.
2. **Preserved user behaviour** — all of it. Same paths, shapes, codes, latencies.
3. **Optimal-way check** — the most direct route to the goal is exactly this: move the grants and elevate where needed. The unconstrained-optimal shape is Row-Level Security on top, which is deferred and named.
4. **Rebuild-vs-iterate** — iterate. The pattern already exists for `memory`; this applies it. Determinism is untouched — metering keeps its fencing and its tests.
5. **What we build** — two roles, a transaction-scoped elevation helper, a pool release guard, and the tests that prove all three.
6. **What we do NOT build** — a shared elevated role, connection-scoped elevation, Row-Level Security, or any change to sealing.
7. **Fit with existing features** — compounds with the foreign-key work in M154_001, which gives Row-Level Security its prerequisite. Must not destabilize metering: the wallet writers are fenced money paths and their tests are the guard.
8. **Surface order** — N/A — no user surface; the change is entirely internal.
9. **Dashboard restraint** — N/A — no user surface. The refused-release counter is operator-only and carries no identity.
10. **Confused-user next step** — N/A for users. For an engineer who hits a refusal, the registered error code names the role that was missing, so the fix is to elevate rather than to widen a grant.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** three Sections split by what can fail independently — the grants, the elevation, and the pool guard. The third exists because the obvious implementation of the first two introduces a worse bug than it fixes.
- **Alternatives considered:** (a) *One shared elevated role* — simpler, but reach stops being enumerable and a wallet writer could read ciphertext. Rejected. (b) *Connection-scoped elevation, matching the existing memory path* — less churn, but a pooled connection carries the elevation to the next borrower unless every path resets, which is exactly the convention-not-structure failure this repository's rules push against. Rejected, and the memory path should later adopt the transaction scope this establishes.
- **Patch-vs-refactor verdict:** this is a **patch** — it moves grants and wraps call sites; no shape changes. It is separated from M154_001 because it fails differently (a privilege leak, not a bad query plan) and deserves its own failure modes, not because it ships separately. Both land in one Pull Request.

## Discovery (consult log)

- **Consults** — Architecture / Legacy-Design / gate-flag triage: the question asked + Indy's decision.

  > Indy (2026-08-01): "Just fold it into these" — in answer to whether the `vault` / `billing` privilege split should join this milestone or wait for the Row-Level Security work. Folded in; authored as a second workstream rather than inside M154_001 because that spec had reached its length bound and this change carries a distinct failure class.

  > Indy (2026-08-01): "Okay we move the RLS to later" — Row-Level Security is deferred to its own milestone.

  > Indy (2026-08-01): "go" — acks building the wallet half in full via the composite `metering_runtime` role rather than deferring it, after a second-model advisory review (Fable) rejected all three options the prior agent had framed. Nothing is deferred out of this workstream.

- **Consult — the fenced statement, second-model review (2026-08-01).** The prior agent's premise that the settle statement's fencing might be redundant (that `ON CONFLICT … DO UPDATE` already made it idempotent) was **refuted from the code**: the ledger arm is a deliberate accumulator (`credit_deducted_nanos = existing + EXCLUDED`), so replay *adds*. Idempotency comes from cursor-diffing against `runner_affinity` — deltas computed as `GREATEST(0, $n - last_metered_at)` while the same statement advances that cursor. That is only sound if the charge and the cursor advance commit together, so the statement cannot be split. It is not modified by this workstream.

- **Reversal — slot 890 stays.** The prior agent recommended deleting `schema/890_fleet_activity_counter_triggers.sql` and inlining the counter arms, on the premise that the triggers would need an elevated function. They do not: both trigger functions are already `SECURITY DEFINER` with a pinned `search_path` (890:31, 890:65), so they are unaffected by elevation. Inlining would have been a regression — `schema/880` grants `api_runtime` SELECT only, and inline arms would require handing write grants on the counter table back to every writing role. No change to either file.

- **Defect found during implementation (P0, fixed).** The authored membership grants were bare — `GRANT vault_runtime TO api_runtime;` — which takes its inheritance from `api_runtime`'s INHERIT attribute, defaulted TRUE by `CREATE ROLE`. Every handler would have held vault and billing privileges ambiently, with the boundary existing only in the comment above the grant, and Dimension 1.1's catalogue query would still have passed. Fixed with `WITH INHERIT FALSE, SET TRUE` on all three, plus Dimension 1.4 as the regression guard. Red-green proved: re-introducing the bare grant fails `test_role_membership_is_dormant_until_set_role` naming the slot and line.

- **Defect found during implementation (P1, fixed).** `schema/710` granted the ledger to `billing_runtime` only, which would have answered the charges list, the events-list cost join, the per-fleet outcome reads and the fleet delete path with `insufficient_privilege`. `api_runtime` keeps SELECT; every write still runs elevated.

- **Consult — in-PR reshape to the typestate design (2026-08-11).**

  > Indy (2026-08-11): "reshape now inside this PR and push it up with tests integrations tests and so on." — context: after a canon review (`oss/bun`'s closure-scoped `sql.begin` transactions; `oss/ghostty`'s owner-holds-the-state guards), the guard-object API was replaced in place by `Elevated(comptime role)` typestate handles + `withRole` closure scopes: an unelevated call to a privileged statement is now a compile error, the closure cannot leak an open scope, and the pool-release audit remains as the belt-and-braces backstop. Per-role pool facades with separate LOGIN credentials were surfaced as the next rung and deferred (the Row-Level Security-tier conversation).

  > Indy (2026-08-11): "Make the code robust performant and make zig coverage above 90% as well?" — context: quality bar for the diff. Performance: envelope crypto runs BEFORE elevating so no transaction spans key derivation; elevation adds BEGIN/SET LOCAL/COMMIT round-trips only on cold or per-slice paths. Coverage: unit + integration tiers below.

- **Defects found during implementation (both latent on `main`, both fixed here).** (1) `state/fleet_telemetry_store.zig` writes the ledger's per-event stage row — a writer the spec's inventory missed; it now elevates to `billing_runtime`. (2) The account purge's `DELETE FROM memory.memory_entries` statement had NO privilege under `api_runtime` (schema/820 grants only `memory_runtime`; api holds no USAGE on the schema) — masked everywhere by superuser test connections, it would have failed the first production purge. The purge's statement list is now role-tagged and elevates per statement (`.memory`, `.vault`) inside its one transaction.

- **Composite-statement discovery.** Two probes spanned `core` + `vault` in a single statement (`workspace_onboarding` signals; `tenant_model_entries` primary-workspace secret check) — impossible under any single role once `SET ROLE` replaces the privilege set. Both split into an unelevated `core` statement plus an elevated vault EXISTS; the alternatives (granting `vault_runtime` core SELECTs, or column grants on the non-secret projection) were rejected because both widen a reach the spec pins to zero (R1, Dimension 1.1).

- **Defects found at VERIFY (live integration run, all fixed here).** (1) The purge's two role-tagged DELETEs repeated the composite-statement trap this log already names: their `WHERE … IN (SELECT … FROM core.…)` subqueries ran under `memory_runtime`/`vault_runtime`, which hold no `core` grants — PostgreSQL refused the whole statement and every `user.deleted` webhook answered 500. Same split as the probes: the ids resolve unelevated inside the purge transaction and the elevated DELETEs bind them as text arrays. The failed purges also left pooled connections dirty, which is what crashed a later credentials test in full-suite runs. (2) `db/pool_test.zig` still pinned the pre-boundary world — the privilege matrix granted `api_runtime` full CRUD on both tables, the executed-statements test ran the money/secret paths unelevated expecting success, and the sealed-columns test named `api_runtime` as the decrypting role; all flipped to the new grants (`billing_runtime`/`vault_runtime`). (3) The migration-role test queried `information_schema.role_usage_grants`, which never carries schema Access Control Lists in PostgreSQL — rewritten against `has_schema_privilege`, asserting all four grants exactly. (4) The two vault-touching privilege tests hit `MissingMasterKey`; they now seed the process Key Encryption Key (KEK) via the existing `setTestKek` convention.

- **Consult — pg.zig driver crash under the coverage lane (2026-08-11).**

  > Indy (2026-08-11): "Why is it crashing now? than before what changed now? Also can you take a pull from the upstream for pg.zig and then apply the patch on to the newly moved upstream commit in main and apply your patch and tag the next version liek you have there, and re-pin build.zig.zon in this branch. But first find the cause of the crash."

  Cause, from the driver source: `Conn.peekForError` borrows the error payload from the reader's buffer; when the `ErrorResponse` is the last buffered message the reader resets its positions to 0, and the socket read inside the subsequent `readyForQuery()` overwrites the peeked bytes before `setErr` dupes them — `Error.parse` then panics (`else => unreachable`) on the `Z` frame's transaction-state byte. Why now: the driver is unchanged; this workstream's elevated write-back made the trigger-raised error path run inside an explicit transaction, and macOS kcov's slowdown reliably hits the window where the `E` and `Z` frames arrive in separate reads. Landed per the directive: upstream `b5a1f25` merged into `agentsfleet/pg.zig` `patch/agentsfleet-0.16`, the reorder patch on top (`setErr` before `readyForQuery`), tagged `v0.0.0-af.4` (`d50a33d`), `build.zig.zon` re-pinned by tag and commit.

- **Consult — CLI acceptance failures under `make test-unit-all` (2026-08-11).** Seven `cli/test/acceptance` auth-guard tests fail on any machine holding a real login: `composeEnv` passes the operator's `HOME` through, and the durable credential the login flow now mints (`~/.config/agentsfleet/credentials.json`) satisfies the auth guard the tests expect to refuse. Zero `cli/` files in this diff; Continuous Integration (CI) is green only because it holds no credential file.

  > Indy (2026-08-11): "I have another agent to fix this cli acceptance failure (M160_001 spec) so keep moving on this by just adding a note in the PR about that spec in progress by another agent."

- **Directive — the free trial is removed, folded into this Pull Request (2026-08-11).** Raised as its own milestone on scope grounds (a billing-semantics change riding into a privilege-boundary review); Indy overruled and directed the fold.

  > Indy (2026-08-11): "I feel the FreeTrial can be removed and we could later build with credits added by the platform admin for the user. So its better that way. The website can still claim and free trial or so."

  > Indy (2026-08-11): "ensure Free Trial is removal is folded in this PR"

  **Measured before executing, and surfaced to Indy.** `free_trial_ends_at` carries no `DEFAULT` and has exactly one writer in the repository — `db/test_fixtures.zig`. No production path ever set it, so every tenant held NULL, `isFreeTrialActive(null)` returned TRUE, and the gate fired *ahead* of the catalogue branch: run fees and every token tier collapsed to zero. Stage billing has therefore never charged anyone. The removal switches it on for the first time. The `$5` starter grant becomes the sole free allowance, and the existing `balance_exhausted_at` path handles exhaustion. **The website's marketing copy is deliberately kept** — `FREE_TRIAL_PILL` / `FREE_TRIAL_BANNER` say "Free during early access", which stays true of a starter grant.

  **What this retires.** F1 from the adversarial review — `loadTrialBoundary(...) catch null` pricing a failed lookup as a live trial, writing a zero charge and advancing `last_metered_at` so the slice could never be re-billed. Deleting the mechanism removes the bug class, so no guard was written for it. Also noted: `event_lifecycle_integration_test.zig` documented the balance-exhausted HTTP path as unreachable *because* every charge priced to zero — that path is now reachable end-to-end and worth covering.

- **Directive — fix the five review findings in-branch (2026-08-12).** A Chief Technology Officer (CTO)-framed review of the branch produced five findings; Indy directed all five fixed here rather than deferred.

  > Indy (2026-08-12): "Okay Go and fix all the 5 findings in the worktree and branch you are in. commit and then start on the changes."

  | Finding | Resolution |
  |---|---|
  | Closure ceremony: ~15 lines × 21 call sites for a context struct plus an anonymous callback struct | `withRole` replaced by a `begin`/`commit`/`deinit` scope guard; `withRole` deleted. Call sites net −102 lines |
  | The typestate's guarantee was overstated in its own documentation | `Elevated(role)` kept and still required by vault/billing signatures, now from `scope.handle()`, with a test pinning that a billing scope cannot produce a vault handle. The module comment now says plainly that PostgreSQL is the enforcement and the type moves that refusal to the call site |
  | File-shape churn: the tracker fold only fit under the 350-line cap because a fix had been dropped | Re-split to `pool_elevation_tracker.zig` (role names, one-way import). Files now 270 and 198 lines |
  | `POOL_SIZE_DEFAULT` / `ACQUIRE_TIMEOUT_MS_DEFAULT` duplicated in `pool.zig` and `pool_url.zig` | One home in `pool_url.zig`; `pool.zig` imports |
  | The table-full pressure path had no test | Tested: exhaustion refuses and moves `refusedMarkCount`, a freed slot is reusable, and a nesting refusal is asserted NOT to move that counter |
  | Invariants over machinery | A metered renewal now asserts the outcome's charge, the wallet decrement, and the ledger row all agree and are positive. Every prior renewal case passed an empty meter, so all rates were zero and none could distinguish a working billing path from one charging nothing |

  Three operational rules were added to the repository `AGENTS.md`, each from a failure observed this session: never read a lane result through a pipe (it reports `tail`'s exit status and made a red integration lane read as green); commit before restructuring (an untracked file deleted by a restructure is unrecoverable and leaves no trace in `git status`); one integration lane per machine (concurrent lanes fail timeout-bounded tests in untouched files).

- **Cross-agent work loss (2026-08-12).** Three adversarial-review fixes were absent from the working tree when this session picked it up, and `git status` showed nothing wrong. Two were merge-blockers and were re-applied here: the `FOR UPDATE` on the purge's workspace read, and the elevation table bound plus its refusal counter. The third (F1, the trial-boundary `catch null`) was superseded by the free-trial removal. A restructure had also deleted a test helper, leaving the unit root uncompilable, and destroyed an untracked file outright.

- **Defect found in CI after the Pull Request opened (fixed here).** `test-coverage-zig` went red on `make check-migrate-unprivileged`: `42501 permission denied to grant role "vault_runtime"` — *"Only roles with the ADMIN option on role vault_runtime may grant this role."* Coverage itself passed (98.70% ≥ 83%, integration 844 passed / 0 failed); the job died in the later gate step. Cause: `scripts/check-migrate-unprivileged.sh` mirrors PostgreSQL 16's implicit CREATEROLE ADMIN grant onto its scratch migrator only for the roles named in `APP_ROLES`, and that list did not carry the three roles slots 110 and 120 add. The scratch migrator therefore met roles it had not created and could not grant them.

  **The production path was verified from source before treating this as a fixture gap**, not inferred from the script's own prose. `deploy/fly/agentsfleetd-{dev,prod}/fly.toml` run `agentsfleetd migrate` as the Fly release command; it connects with `DATABASE_URL_MIGRATOR` from `planetscale-{env}/migrator-connection-string`; and `playbooks/founding/03_priming_infra/001_playbook.md` states *"Do not apply schema files manually. The checked-in migration runner applies them in order during deployment."* So on the managed databases the migrator creates these roles itself and holds ADMIN OPTION by construction — that slot 110 already creates five roles on a live deploy is the proof it holds CREATEROLE. The divergence is local only: PostgreSQL roles are cluster-level, every other lane migrates the compose database as superuser, and the scratch database therefore faces roles created by another identity — a state deploy never reaches. The mirror loop exists precisely to erase that difference, so omitting a role models an impossible cluster.

  > Indy (2026-08-12): "yes do that" — acks extending `APP_ROLES` after the alternatives were surfaced. No schema-side fix preserves the guarantee: skipping the grant when ADMIN OPTION is absent leaves `api_runtime` without dormant membership and breaks the boundary this workstream exists to create; the role-creating `DO` block cannot confer ADMIN OPTION on itself; and failing loudly is what 42501 already does.

  Red-green: CI attempt 1 (job `94059107452`) refused the grant; `make check-migrate-unprivileged` passes locally with the extended list.

  **Guard added so the list cannot drift again.** `scripts/check_migrate_role_parity.sh` asserts set equality between `APP_ROLES` and the roles `schema/` creates, scanning both spellings the slots use (slot 110's quoted-name ARRAY, slot 120's literal `CREATE ROLE`). Static — no database, no container — so drift fails in a second naming the omitted role rather than as a 42501 twelve minutes into the coverage lane. Its own red-green surfaced two defects in itself before it shipped: a `sed` range that never terminates on its start line (so the single-line form of `APP_ROLES` harvested `docker compose exec postgres psql` as role names — rewritten in awk, with both formatting forms pinned), and a scan that read `--` comments and invented a role named `defaults` from slot 110's own prose *"which CREATE ROLE defaults to TRUE"*.

  **Not wired into CI, by decision.** The `safety-gates` job (`.github/workflows/lint.yml`) runs named targets rather than `make lint-all`, and no workflow invokes `lint-all` at all — so a gate wired only there fires on local VERIFY and in no Pull Request. Adding a four-line step to `safety-gates` was offered and declined.

  > Indy (2026-08-12): "skil this check-migrate-role-parity in CI"

  Residual gap, stated plainly: a future milestone that adds a role, omits it from `APP_ROLES`, and does not run `make lint-all` before opening a Pull Request will still surface the drift as a 42501 in the coverage lane rather than as this gate's one-line failure.

- **Metrics review** — no new events. The two operator-facing counters (`refusedReleaseCount`, `refusedMarkCount`) are process-local counts with no identity, per §3's metric shape; no analytics or funnel surface changes.

- **Skill-chain outcomes** — the six-reviewer `/review` pass and its adversarial round ran before this session and are recorded above with their findings ledger. `kishore-babysit-prs` runs after the first push.

- **Deferrals** — none. Every finding raised in this workstream landed in-branch.
