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
**Status:** IN_PROGRESS
**Priority:** P1 — a security boundary that exists in prose and not in grants
**Categories:** API, SQL
**Batch:** B1 — same batch and same Pull Request as M154_001; the grants live in slots that workstream authors
**Branch:** feat/m154-schema-rebuild
**Test Baseline:** unit=3344 integration=510
**Depends on:** M154_001 (same PR) — the grants land in the slots it re-authors, so it must be authoring them for this to apply
**Provenance:** LLM-drafted (Claude Opus 5, Aug 01, 2026), from a grant-level audit of the shipped schema
**Canonical architecture:** `docs/architecture/runner_fleet.md` §the control-plane/data-plane split · `docs/AUTH.md`

---

## Overview

**Goal (testable):** Selecting a ciphertext or updating a balance as `api_runtime`, without elevating first, is refused by PostgreSQL.

**Problem:** `api_runtime` is the role every Hypertext Transfer Protocol handler runs as, and it holds direct grants on `vault.secrets` and the tenant wallet. So the schema separation those tables sit behind protects nothing: any handler, and any injection or logic bug inside one, can read every stored ciphertext and move any balance. The architecture prose describes a trust boundary between the control plane and the data plane that no privilege enforces. Meanwhile `memory` already demonstrates the working shape — zero direct grants, reachable only after elevating — so the pattern is proven in this codebase and simply not applied where the stakes are highest.

**Solution summary:** Two roles are introduced, one for the secret store and one for the wallet, and the grants that currently sit on `api_runtime` move onto them. `api_runtime` is granted membership so it can elevate for the span of one transaction and no longer. Elevation is scoped to the transaction rather than the connection, and the pool refuses to hand back a connection that is still elevated — so a forgotten reset becomes a loud failure instead of a privilege leak into the next request.

## PR Intent & comprehension handshake

- **PR title (eventual):** shared with M154_001 — `refactor(m154): rebuild schema from empty — single identity key, money behind FKs`
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

- **Dimension 1.1** — `api_runtime` holds no direct privilege on the secret store or the wallet → Test `test_api_runtime_holds_no_direct_grant`
- **Dimension 1.2** — an unelevated read of either table is refused by PostgreSQL, not by application code → Test `test_unelevated_access_is_refused`
- **Dimension 1.3** — the migration role retains full authority, so a rebuild cannot lock itself out → Test `test_migrator_still_owns_both_tables`
- **Dimension 1.4** — every role `api_runtime` may assume is granted non-inheriting, so the privilege is unreachable without an explicit `SET ROLE` → Test `test_role_membership_is_dormant_until_set_role`
- **Dimension 1.5** — `metering_runtime` reaches exactly the fenced statement's tables and holds no direct grant on either money table → Test `test_metering_role_matches_statement_footprint`

### §2 — Elevation is scoped to the transaction

Elevation lasts for one transaction and ends with it, so a commit or a rollback both return the connection to `api_runtime` without anything having to remember. The wallet writers are already single fenced statements, so wrapping them costs nothing structurally. **Implementation default:** transaction-scoped elevation rather than connection-scoped, because the connection is pooled and its next borrower is a different request.

- **Dimension 2.1** — every secret read and write succeeds under elevation and the transaction still commits atomically → Test `test_secret_paths_work_under_elevation`
- **Dimension 2.2** — the metered renewal and the settle both still charge exactly once under elevation, with fencing unchanged → Test `test_metering_unchanged_under_elevation`
- **Dimension 2.3** — a failed statement inside an elevated transaction rolls back and leaves no elevation behind → Test `test_rollback_clears_elevation`
- **Dimension 2.4** — account erasure removes secrets and the wallet row under elevation → Test `test_erasure_elevates_for_secrets_and_wallet`

### §3 — The pool refuses to hand back an elevated connection

The failure this workstream must not introduce is a connection returned to the pool still elevated, which would hand the next request privileges it never asked for — strictly worse than the situation being fixed. The guard belongs in release, where it cannot be forgotten, rather than in each call site.

- **Dimension 3.1** — releasing a still-elevated connection is refused and reported, never silently accepted → Test `test_release_rejects_elevated_connection`
- **Dimension 3.2** — a connection that has completed an elevated transaction reports the base role and is reusable → Test `test_connection_returns_to_base_role`

## Interfaces

```
No HTTP surface changes. Every endpoint keeps its path, request shape,
response shape, and status codes.

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
| R1 | No grant on the two tables names `api_runtime` (§1) | `grep -nE "GRANT.*(vault\.secrets\|tenant_wallet).*api_runtime" schema/` | no output | P0 | |
| R2 | Every elevated path releases on the error path too (§2) | `grep -rn "elevate" src/agentsfleetd --include='*.zig' \| grep -v errdefer \| grep -v _test` | reviewed: every hit is inside a scope that releases | P0 | |
| R3 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | |
| S1 | Unit tests pass | `make test` | exit 0 | P0 | |
| S2 | Lint clean | `make lint-all` | exit 0 | P0 | |
| S3 | Integration passes | `make test-integration` | exit 0 | P0 | |
| S5 | No leaks | `make memleak` | exit 0 | P0 | |
| S6 | Cross-compile | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` | exit 0 | P0 | |
| S7 | No secrets | `gitleaks detect` | exit 0 | P0 | |

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

- **Metrics review** — events added, extra events found during `/review`, analytics/funnel playbook update or the explicit no-change reason.
- **Skill-chain outcomes** — `/write-unit-test`, `/review`, `kishore-babysit-prs` results (order per `AGENTS.md` CHORE(close); iteration counts, findings dispositioned).
- **Deferrals** — every "deferred to follow-up" needs an **Indy-acked verbatim quote** here, format `> Indy (YYYY-MM-DD HH:MM): "<quote>" — context: <which item, why>`.
