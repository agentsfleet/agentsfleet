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
**Status:** SUPERSEDED — parked in `docs/v2/done/`, its branch deleted and its `DONE` marks stale; the privilege boundary is redesigned from an attacker's path, not resumed from here (see Parked below)
**Priority:** P1 — a security boundary that exists in prose and not in grants
**Categories:** API, SQL
**Batch:** B1 — its own Pull Request, both halves together; the grants live in slots M154_001 authors
**Branch:** feat/m154-privilege-boundaries — **deleted Aug 15, 2026**; archived only as `refs/pull/598/head` (`8ff760da6`)
**Test Baseline:** unit=3512 integration=589
**Depends on:** M154_001 (merged first) — the grants land in the slots it re-authors, so those slots must exist for this to apply. Its §1 revoke and §2 elevation ship together or not at all: the revoke alone refuses every signup, because the starter grant is written inside the tenant-create transaction
**Provenance:** LLM-drafted (Claude Opus 5, Aug 01, 2026), from a grant-level audit of the shipped schema
**Canonical architecture:** `docs/architecture/runner_fleet.md` §the control-plane/data-plane split · `docs/AUTH.md`

---

## Parked (Aug 13, 2026) — read this before reactivating

**The work in this spec is sound and it is not wired.** Every grant it moves
governs an identity that no statement in the running system ever assumes, so
merging it as authored would change nothing in production. It is parked here
whole, with the missing edge identified and proved, rather than shipped as a
boundary that cannot fail.

**None of the code below is on `main`, and the branch that held it is gone.**
Every Dimension marked `DONE` was completed on `feat/m154-privilege-boundaries`
(`8ff760da6`, PR #598 closed Aug 13, 2026) and merged nowhere. That branch was
deleted on Aug 15, 2026.

**Every `DONE` mark in this file is stale.** They describe 138 files of work
that exists on no branch. The spec and the tree have drifted apart: nothing
below is shipped, and nothing below is a live claim about the repository. Read
this file as a record of a defect found and a design rejected — not as a plan
with completed parts.

**Why the branch went rather than being kept alive.** Keeping a dangling branch
only so a spec's `DONE` marks stay true is the wrong trade: it preserves the
claim and not the value. Indy deleted it to restart the privilege boundary from
a fresh position — designed from how an attacker actually reaches the data
rather than from the grant-level audit this spec was drafted off. The question
the replacement answers is which path a hacker takes to the secret store and the
wallet, and what each grant denies them at that moment. This spec never asked
that; it asked which grants a tidy schema would have.

**The code is unbranched, not lost.** GitHub retains the closed PR's head:
`git fetch origin refs/pull/598/head` recovers `8ff760da6`. That is an archive
for archaeology, not a starting point — the replacement work is expected to be
designed again, not resumed from it.

> Indy (2026-08-13): "i want only the removal of free trial in this PR other
> grant and role related commits are not needed? I find it an over kill for this
> stage." … "we can design and add it better"

**The defect, from source.** `schema/110:38` creates every role `NOLOGIN`,
`api_runtime` included, so nothing can authenticate as it. The only `SET ROLE`
in production code is `SET ROLE memory_runtime`
(`http/handlers/memory/helpers.zig:92`) — nothing ever executes
`SET ROLE api_runtime`. Both halves together mean no production statement has
ever run as `api_runtime`, on `main` or on this branch. The note previously
recorded in Discovery ("in force only if that login role is the restricted one")
understated it: as the schema stands, the login role *cannot* be `api_runtime`.

This is also why §5's privilege defect ran green for the life of the branch.
`db/schema_privilege_integration_test.zig:216` sets `SET ROLE api_runtime` and
calls the purge, but the test login is a superuser (`usesuper = t` on the compose
role), so the first elevation's `SET LOCAL ROLE NONE` reverted to `session_user`
— the superuser — and widened every statement after it. The suite could not fail.

**The missing edge, proved live against the m154 container.** With a
non-superuser login granted `api_runtime` membership and
`ALTER ROLE <login> SET role = 'api_runtime'`:

| Probe | Result |
|---|---|
| On connect | `current_user=api_runtime`, `session_user=<login>` |
| `SELECT FROM vault.secrets`, unelevated | permission denied |
| Same, after `SET LOCAL ROLE vault_runtime` | permitted |
| `DELETE FROM fleet.runner_affinity` | permission denied |
| `DELETE FROM core.fleets / workspaces / tenants` | permitted |
| `SET LOCAL ROLE NONE` | → `<login>` — the footgun survives |
| `SET LOCAL role = DEFAULT` | → `api_runtime` |
| `RESET ROLE` | → `api_runtime` |

Two consequences for whoever picks this up:

1. **`ALTER ROLE <login> SET role = 'api_runtime'` is line one of the design, not
   an afterthought.** No application code, no password in a migration, and it is
   what turns every grant in this spec from decorative into enforced. The test
   harness must reproduce the same shape — the precedent is
   `scripts/check-migrate-unprivileged.sh`, which models PlanetScale's managed
   migrator role for exactly this reason.
2. **The deferred `SET LOCAL ROLE NONE` milestone is one word, not a milestone.**
   Discovery below states the fix needs "either a captured `current_user` (a round
   trip per scope) or a `poison`-style method on the vendored `pg` fork". It needs
   neither: `SET LOCAL role = DEFAULT` steps down to the role's configured
   default. That Discovery entry is superseded by this one.

**Carved out and shipped separately.** The free-trial deletion folded into this
milestone is independent of every role change and fixes a live revenue defect —
`isFreeTrialActive(null, _)` returns `true` (`state/tenant_billing.zig:107-108`
on `main`), `free_trial_ends_at` carries no `DEFAULT` and has one writer in the
repository (a test fixture), so every tenant priced to zero. It leaves this spec
for its own milestone rather than waiting on a boundary that is not wired.

**Also unresolved, and now the higher-value gap.** Row-level tenant isolation
does not exist: application `WHERE` clauses are the only tenant boundary, and no
grant in this spec catches a missing predicate. `docs/architecture/runner_fleet.md:254`
already names it as its own workstream. Worth designing together with the roles.

**Reactivation condition:** a decision on how the deployed API assumes
`api_runtime` — which requires knowing the login role inside
`op://ZMB_CD_PROD/planetscale-prod/api-connection-string`, a value that lives in
the vault and not in this repository.

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

**Amended at §5 — reopened after CHORE(close) by the owed `/review` (Indy-directed).** Three defects the review found plus the one its fix exposed; all inside this milestone's own blast radius:

| File | Action | Why |
|------|--------|-----|
| `schema/821_memory_entries_fleet_fk.sql` | CREATE | The referential edge that makes memory erasure exact rather than racy (§5.2). Additive slot, not an edit to 820: `embed.zig` versions are skipped once recorded, so an in-place edit would silently no-op on any database that already applied it |
| `schema/embed.zig` | EDIT | Registers slot 821 |
| `src/agentsfleetd/db/pool.zig` | EDIT | `markForDiscard` — the only write to pg's `_state` in the repository, beside the release that reads it (§5.1) |
| `src/agentsfleetd/http/handlers/memory/helpers.zig` | EDIT | A failed `RESET ROLE` marks the connection instead of logging a discard that never happened (§5.1) |
| `src/agentsfleetd/errors/error_entries.zig` | EDIT | `UZ-INTERNAL-005` was orphaned by the elevation refactor while the dirty release logged under `004`; prose rewritten to the condition that now emits it |
| `src/agentsfleetd/state/account_teardown.zig` | EDIT | Three sweeps `api_runtime` may not execute deleted in favour of the `core.fleets` cascade; the memory delete moved after it; header claim that those children lack `ON DELETE CASCADE` corrected (§5.5) |
| `src/agentsfleetd/fleet_runtime/sql.zig`, `approval_gate_db.zig` | EDIT | `SET_GATE_PURGE_BYPASS_SQL` moved to the domain `sql.zig` per RULE SQLMOD, with `GATE_PURGE_SETTING` extracted so the name can be pinned (§5.6) |
| `src/agentsfleetd/http/handlers/fleets/delete.zig`, `create_grants_integration_test.zig`, `http/webhook_test_fixtures.zig`, `webhook_http_integration_test.zig` | EDIT | Call sites repointed; the two that retyped the literal now use the constant |
| `src/agentsfleetd/fleet_runtime/approval_gate_pins_test.zig` | EDIT | The slot-grep pin for the gate-purge setting (§5.6) |
| `src/agentsfleetd/db/pool_test.zig`, `memory/fleet_memory_integration_test.zig` | EDIT | Tests for §5.1–5.4 |
| `src/agentsfleetd/state/account_teardown_test.zig` | EDIT | The erasure guard extended to fleet scope, and seeded for the three tables the purge stopped naming (§5.7) |
| `src/agentsfleetd/db/index_usage_integration_test.zig`, `index_removal_integration_test.zig` | EDIT | Fallout of the new edge: both fabricated ~200 fleet ids with no parent row. Neither assertion depended on that spread — both force the index — so each now seeds one real fleet |

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

### §5 — The boundary holds where the role outlives the statement

Reopened after CHORE(close), from the `/review` pass owed on the elevation refactor. The review found three defects and the fix for one of them exposed a fourth; all four are the same shape — a privilege that survives longer than the code assumes.

**The pooled connection.** `handlers/memory/helpers.zig` elevates with session-scoped `SET ROLE`, which the server does not revert at COMMIT. Its paired `RESET ROLE` swallowed failure with a hint naming a discard that never happened: a failed reset outside a transaction is answered with `ReadyForQuery('I')`, so pg leaves `_state = .idle`, the release backstop sees a clean connection, and the next borrower inherits `memory_runtime`. `db.markForDiscard` forces `.fail` — chosen over `conn.begin()` because a failing begin recovers through `Conn.read` and can land back on `.idle`, pooling the very connection it meant to discard.

**Erasure exactness.** `memory.memory_entries` carried its fleet id as a bare value, so an account purge's frozen id array was its only eraser and a capture landing after that statement outlived the erasure permanently. `schema/821` adds `REFERENCES core.fleets ON DELETE CASCADE`: a racing capture either commits before the fleet row goes and cascades away, or blocks on that row and fails closed. `ADD CONSTRAINT` validates existing rows, so the migration applying cleanly is also the proof that no orphan was already present.

**The purge could not run as its own role.** Moving the memory delete after `DELETE FROM core.fleets` removed an accidental escalation nobody knew was load-bearing: `SET LOCAL ROLE NONE` resets to `session_user`, not to the role that was current, so the first elevation's step-down had been silently widening every statement after it. With the memory elevation at statement 1 that covered the whole purge. `api_runtime` is granted SELECT/INSERT/UPDATE and deliberately **not** DELETE on `fleet.runner_affinity`, `core.fleet_approval_gates` and `core.fleet_sessions` (schema/630, /810, /510), so the three explicit sweeps were unrunnable under the role the purge actually holds. They are deleted rather than granted: each cascades from `core.fleets`, and a referential action runs with the table owner's authority. The underlying `SET LOCAL ROLE NONE` semantics are **not** changed here — named in Discovery as the remaining footgun.

- **Dimension 5.1** — a connection whose session role could not be reset is destroyed rather than pooled, so no borrower inherits it → Test `test_discarded_connection_drops_session_role` — **DONE**
- **Dimension 5.2** — an erased fleet leaves no memory row behind, by referential action rather than by a statement remembering to sweep → Test `test_fleet_delete_cascades_memory` — **DONE**
- **Dimension 5.3** — a memory write naming a fleet that does not exist is refused, not orphaned → Test `test_absent_fleet_write_refused` — **DONE**
- **Dimension 5.4** — the new edge grants `memory_runtime` no reach into `core`: it cannot read the parent it references, and the write still resolves → Test `test_memory_write_holds_no_core_grant` — **DONE**
- **Dimension 5.5** — the account purge completes under `api_runtime`, with no grant widened and no reliance on a session-role reset → Test `test_erasure_elevates_for_secrets_and_wallet` — **DONE**
- **Dimension 5.6** — the gate-purge bypass setting cannot drift between the Zig constant and the two slots whose triggers read it → Test `test_gate_purge_setting_pinned_to_slots` — **DONE**
- **Dimension 5.7** — the three tables the purge stopped naming are still erased, and the gate bypass still covers rows the cascade deletes rather than only the ones a statement names → Test `test_erasure_sweeps_fleet_scoped_tables` — **DONE**

**What the deleted sweeps cost in coverage, and how it is paid back.** Removing three DELETEs moved those tables from "erased by a statement a test can read" to "erased by a referential action nothing names". The existing erasure guard could not see the loss: it sweeps `information_schema` for a `tenant_id` column, and all three are keyed by fleet. The guard now sweeps fleet scope as well — twelve tables rather than three, so a fleet-scoped table added later inherits the assertion — and the fixture seeds all three. Seeding a gate row is the load-bearing half: the cascade fires that table's BEFORE DELETE append-only trigger, so this is the only fixture under which `SET LOCAL fleet.allow_gate_purge` is required for the purge to complete at all.

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
| 5.1 | integration | `test_discarded_connection_drops_session_role` | On a size-1 pool: `SET ROLE memory_runtime`, mark the connection, release, re-acquire — `current_role` is not `memory_runtime`. Pool size 1 is load-bearing; a larger pool could hand back a different connection and pass vacuously |
| 5.2 | integration | `test_fleet_delete_cascades_memory` | Store a memory row, delete its fleet as the **base** role (which holds no grant on `memory.memory_entries`), count returns 0 — proving the cascade runs with the owner's authority |
| 5.3 | integration | `test_absent_fleet_write_refused` | `storeEntry` for a fleet id never seeded raises `error.PG` (23503) and the count stays 0; the session survives the refusal and remains usable |
| 5.4 | integration | `test_memory_write_holds_no_core_grant` | As `memory_runtime`: a direct `SELECT … FROM core.fleets` is refused (no USAGE on the schema), yet `storeEntry` against the FK still succeeds |
| 5.5 | integration | `test_erasure_elevates_for_secrets_and_wallet` | The existing erasure test, now load-bearing: it runs the purge under a session-level `SET ROLE api_runtime`, so it fails if any purge statement needs a grant that role lacks |
| 5.6 | unit | `test_gate_purge_setting_pinned_to_slots` | `GATE_PURGE_SETTING` appears verbatim in embedded slots 810 and 830, and the composed bypass statement sets exactly that name to the value those triggers compare against |
| 5.7 | integration | `test_erasure_sweeps_fleet_scoped_tables` | The erasure fixture seeds `fleet.runner_affinity`, `core.fleet_approval_gates` and `core.fleet_sessions`, asserts each holds a row, then purges and sweeps every `fleet_id`-keyed table from the catalogue for survivors. Red-green proved by deleting the purge's bypass statement: the seeded gate row makes the cascade raise the append-only exception |

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

- **Discovery — `SET LOCAL ROLE NONE` resets to `session_user`, not to the prior role (2026-08-13).** Not fixed here, and the largest remaining footgun in this area. Every elevated scope steps down with `SET LOCAL ROLE NONE`; PostgreSQL defines that as reverting to the **session** user, so on a connection whose session role was set by something other than login, the step-down *widens* privilege rather than restoring it. With the memory elevation as purge statement 1, that had been silently granting statements 2–12 the session role's authority for the life of this branch, which is why §5's privilege defect had no failing test. Production is unaffected only while the API connection's login role equals its intended runtime role. A correct step-down restores the role that was current before elevation; doing that needs either a captured `current_user` (a round trip per scope) or a `poison`-style method on the vendored `pg` fork. Deliberately out of scope at the end of a long session — it deserves its own milestone.

  > Indy (2026-08-13): "prod connect as api_runtime (ignore)" — asked, and dropped, whether the deployed `api-connection-string` logs in as `api_runtime`. Recorded because it decides whether §5's purge defect was live or latent, and because **no production code executes `SET ROLE api_runtime`**: the grants this milestone tightened are in force only if that login role is the restricted one. Not verifiable from the repository; the value lives in 1Password.

- **Discovery — a test named `integration:` is not necessarily in the integration lane (2026-08-13).** `make test-integration` builds from `src/agentsfleetd/integration_tests.zig`, which imports files by name; `account_teardown_test.zig` is imported by the unit root `tests.zig` instead, so **every purge test — including the erasure guard this milestone leans on — runs only in the live-database unit lane**, never in `make test-integration`. The `integration:` prefix on those test names comes from the build's *name* filter and says nothing about which target runs them. This cost a full red-green cycle: the first attempt to prove §5.7 red ran `make test-integration-db`, which reported `EXIT=0` because the mutated test was not in that binary at all. Worth knowing before reading any purge-related lane result as coverage. `make test-unit-all` sets `LIVE_DB=1`, so the lane is real — it is simply not the one its name suggests.

  Two things made the wrong lane look convincing, both already-known hazards in this repository: `make/test-unit.mk` documents that a Zig test binary "exits 0 whether or not its tests ran AND whether or not they passed", and `zig build`'s `failed command:` line appears in passing runs too. The reliable proof is the `Build Summary` counts (`59/59 tests passed`), not an exit status and not a `✓` line.

- **Discovery — both SQL rules are write-time only (2026-08-13).** RULE STS fired correctly when `schema/821` was authored; its wording simply did not reach `current_setting()`, and it has been widened in dotfiles `6a9b421`. RULE SQLMOD's check *does* match `SET_GATE_PURGE_BYPASS_SQL`, but `write_zig.sh` invokes it `--staged`, so a misplaced SQL constant sat in `approval_gate_db.zig` unflagged until asked about. Neither rule sweeps the tree, so pre-existing violations stay invisible by design. A full-tree STS audit is the missing half; `sql-mod.sh --all` currently reports 47 findings on this repository, heavily false-positive (names merely ending `_QUERY`; the exemption matches only the literal filename `sql.zig`, so `gallery_sql.zig` and `sql_budget_drain.zig` are wrongly flagged), so such a check needs a real carve-out mechanism before it is wired.

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

- **Reviewer finding on the open Pull Request (P1, valid, fixed here).** Greptile flagged `account_teardown.zig:103`: *"If workspace creation overlaps account erasure, this `FOR UPDATE` locks only the tenant's existing workspaces, so a later workspace and fleet are omitted from the captured `fleet_ids`."* Verified from source rather than from the label, and it is a **regression this branch introduced**. The first commit locked the tenant row (`lockTenant`); a later commit deleted that helper and substituted a workspace-row lock, reasoning that *"a tenant-row lock would not close this: fleets do not reference it."* True of fleets, and the wrong conclusion — the two locks guard different levels of the same parent chain and neither substitutes for the other:

  | Lock | Blocks | Via | Cost of omitting it |
  |---|---|---|---|
  | tenant row | a concurrent **workspace** INSERT | `core.workspaces` → `core.tenants` | the new workspace keeps its `vault.secrets` |
  | workspace rows | a concurrent **fleet** INSERT | `core.fleets` → `core.workspaces` | the new fleet keeps its `memory_entries` |

  The elevated DELETEs bind arrays frozen at resolve time while the `core` DELETEs re-evaluate their subqueries per statement, so anything inserted in that window loses its `core` row and keeps the elevated one, which no foreign key cascades behind. Trading one lock for the other closed the fleet hole and reopened the workspace hole — orphaned ciphertext outliving the account it belonged to. `resolve` now takes both, parent-to-child, so two concurrent purges of one tenant queue rather than deadlock.

  **Shipped without a regression test, deliberately.** A contention test was written and then deleted, because probing it showed it passed with `lockTenant` removed — it proved nothing. The reason is structural: a contender that holds its workspace INSERT open holds `FOR KEY SHARE` on the tenant row for the whole test, and the purge's final statement (`DELETE FROM core.tenants`) conflicts with that lock regardless, so "the purge is refused" is satisfied by a statement far below the one under test. Nor can a held-open contender distinguish the two cases at all: with or without the lock the purge times out and rolls back, leaving no observable difference.

  Reproducing the real defect needs the concurrent insert to **commit** mid-purge — after the id sets resolve, before the `core` deletes run — which requires `std.Thread.spawn`, a secret seeded on the late workspace, and an assertion that `vault.secrets` for it is empty afterwards. That was offered and declined; a source-text tripwire asserting the two `FOR UPDATE` statements still exist was offered as the cheap alternative and rejected on the grounds that it is coverage in appearance only.

  > Indy (2026-08-12): "i think to me 2 is just a cover up or pointless fix, so i prefer 3" · "2 is almost 3"

  **Residual gap, stated plainly:** both purge locks are unpinned. An edit that removes either one reopens a permanent erasure gap — orphaned `vault.secrets` or `memory_entries` outliving the account — and no lane will fail. That is exactly how this defect entered: an edit that removed the tenant lock while reasoning carefully about why the workspace lock sufficed.

- **Refactor of the elevation machinery, Indy-directed ("i think fix 1, 2, 3").** Three findings from a design pass over this Pull Request's own changed files:

  1. **The typestate was decorative.** `Elevated(role)` was documented as making the privilege legible in a signature, but exactly ONE function in the codebase took one (`tenant_billing_store.rowExists`), and even that reached `v.conn` to do its work — the handle carried no capability. It now carries `exec`/`query`, and all 21 production call sites plus 3 tests run statements through the scope (`scope.exec(...)`) instead of past it (`scope.conn.exec(...)`). Both `Scope` and `Elevated` name the field `_conn`, so a deliberate bypass greps rather than reading as ordinary field access. Zig has no field privacy; this is ergonomics plus a marker, and PostgreSQL remains the enforcement.
  2. **The pool-release backstop was redundant and could misfire.** Elevation is `SET LOCAL ROLE`, which the server reverts at COMMIT or ROLLBACK, so a connection with no open transaction cannot still be elevated — `conn._state` is the whole test, and a wider one, since it catches any leaked transaction rather than only an elevated one. Enforcement was always pg's (its release destroys non-idle connections); the old path asked a side table and then called `conn.begin()` to *manufacture* the non-idle state it was reporting, which on a stale mark poisoned a connection the server had already reverted. `auditRelease`, `refusedReleaseCount` and `g_refused_releases` are deleted — nothing read them (RULE NDC).
  3. **One switch over `Role`, not two.** `setLocalStatement` composed the statement from a second 4-arm switch that had to stay in sync with `dbName`; it is now `S_SET_LOCAL_ROLE_PREFIX ++ comptime role.dbName()`.

  **Correction recorded, because the first plan was wrong.** The refactor was proposed as "delete `pool_elevation_tracker.zig`". That was mistaken: `mark` does two jobs, and only the release-audit half is redundant. The other half refuses a *second* claim on the same connection (RULE OWN), which nothing else enforces — without it a nested `begin` silently overwrites the role and the inner `commit`'s `SET LOCAL ROLE NONE` strips the outer scope's elevation while it still believes it holds one. The tracker stays, at 154 lines rather than 198.

  **Deliberately not done:** migrating `handlers/memory/helpers.zig` off session-scoped `SET ROLE`. Its four call sites carry multiple early returns and one is the runner memory *write* path, so moving them under a transaction changes error semantics — a mid-way failure would roll back writes that persist today. Arguably better, but a behaviour change on a live handler, and the module header already scopes it as its own change.

  Verification: `make lint-zig` green (ZLint, pg-drain, length, depth unit=3577 integration=599), `make test-unit-agentsfleetd` 2153 passed / 0 failed, `make test-integration` green on a quiet host.

- **Metrics review** — no new events. The one operator-facing counter, `refusedMarkCount`, is a process-local count with no identity, per §3's metric shape; `refusedReleaseCount` went with the release audit that fed it, so the earlier "two counters" reading of this line is stale. No analytics or funnel surface changes.

- **Skill-chain outcomes** — the six-reviewer `/review` pass and its adversarial round ran before §5 and are recorded above with their findings ledger. §5 re-ran the chain against its own diff: `/write-unit-test` produced a thirteen-row ledger and found the §5.7 gap below; `/write-integration-test` audited the three new live-Postgres tests and returned no further requirement.

- **Coverage the deleted sweeps cost, found by `/write-unit-test` (§5.7).** Removing three DELETEs in favour of the `core.fleets` cascade moved `fleet.runner_affinity`, `core.fleet_approval_gates` and `core.fleet_sessions` out of reach of the erasure guard: it sweeps `information_schema` for a `tenant_id` column and all three are keyed by fleet, so the loss was invisible to it. All three were confirmed to carry `ON DELETE CASCADE` from source, so nothing was broken — but nothing pinned it either. The guard now sweeps fleet scope as well and the fixture seeds all three.

- **Two ledger rows resolved as `won't-test`, with reasons rather than deferrals.** (1) `schema/821`'s `DROP`-then-`ADD` idempotency: `embed.zig` skips a slot version once recorded, so re-application is unreachable from any lane. (2) The `catch` branch in `resetRole`: the only failure that produces the bug is a `RESET ROLE` that fails while the server still answers `ReadyForQuery('I')`, which no client can induce — every injectable failure (`pg_terminate_backend`, socket close) sets `_state = .fail` on its own, so such a test would pass on the pre-fix code and be false by the red-green rule. The `pool_test.zig` stand-in covers the discard chain instead.

- **No concurrent-race test for §5.2, by construction rather than by omission.** The race the review named collapses into two states the foreign key defines: a capture either commits before the fleet row goes, and cascades away, or blocks on that row and fails closed. Both arms are pinned deterministically (`test_fleet_delete_cascades_memory`, `test_absent_fleet_write_refused`). A thread-race test would exercise PostgreSQL's referential machinery, not this repository's.

- **Deferrals** — none. Every finding raised in this workstream landed in-branch. The `SET LOCAL ROLE NONE` semantics recorded in Discovery are not a deferral of declared scope: they are a pre-existing behaviour on `main` that §5 discovered and deliberately did not restate as its own, and they need their own milestone.
