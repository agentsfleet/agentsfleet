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

# M164_001: Delete the free trial

**Prototype:** v2.0.0
**Milestone:** M164
**Workstream:** 001
**Date:** Aug 13, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — stage billing has never charged anyone; the gate is open for every tenant that exists
**Categories:** API, SQL, UI
**Batch:** B1 — its own Pull Request; nothing else depends on it
**Branch:** feat/m164-delete-the-free-trial
**Test Baseline:** unit=3556 integration=588
**Depends on:** none. Carved out of M154_002, which is parked in `docs/v2/done/` — this workstream shares no code with the privilege boundary that parked
**Provenance:** LLM-drafted (Claude Opus 5, Aug 13, 2026), from a source read of the pricing path on `main`
**Canonical architecture:** `docs/architecture/billing_and_provider_keys.md` · `docs/architecture/memory.md`

---

## Overview

**Goal (testable):** A metered platform stage charges the catalogue rate, and no rate resolver in the repository takes a time parameter.

**Problem:** Every tenant is billed nothing. The wallet column that bounds the promotional window is nullable with no default, exactly one writer in the repository (a test fixture), and a null in it reads as "the trial is open" forever. The gate that consults it sits ahead of the catalogue lookup, so run fees and all three token tiers resolve to zero for every account that has ever signed up. The `$5` starter grant is therefore the only thing that has ever limited usage, and the trial that was supposed to expire never began.

**Solution summary:** The promotional window is deleted rather than repaired — the column, the predicate, the branch that short-circuits pricing, and the published field. Pricing resolves from the model catalogue alone, which makes it clock-independent: the time-injected sibling resolver exists only to price post-window states and disappears with the window. The starter grant becomes the sole free allowance and `balance_exhausted_at` remains the exhaustion signal, both already shipped. The billing endpoint stops publishing `free_trial`, which is a breaking change to a documented response.

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(m164): delete the free trial and price from the catalogue
- **Intent (one sentence):** Tenants are charged the rate the catalogue publishes, instead of nothing.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `src/agentsfleetd/state/tenant_billing_rates.zig` — the whole pricing path lives here. `sliceRatesWithoutCatalogue` is where the window short-circuits ahead of the posture switch; `resolveRenewSliceRates` and the `computeStageCharge` / `computeStageChargeAt` pair are what collapse once it is gone. Read the doc comments before editing: they state why the short-circuit sits where it does, and that reason is what this workstream removes.
2. `docs/v2/done/M154_002_P1_API_SQL_PRIVILEGE_BOUNDARIES_SECRETS_WALLET.md` — the parked milestone this is carved out of. Its "Parked" section explains why none of its role work ships. Do not carry any of it across; the two overlap only in files, never in behaviour.
3. `docs/architecture/billing_and_provider_keys.md` — the canonical description of how a stage is priced and charged. It documents the window, so it changes with the code.
4. `~/Projects/dotfiles/docs/SCHEMA_CONVENTIONS.md` — a column drop and a new slot are both schema edits; the Schema Table Removal Guard fires on the first.

## Files Changed (blast radius)

Discovery grep, from the repository root:
`git grep -rn -wE 'free_trial|freeTrial|isFreeTrialActive|FREE_TRIAL|trial_ends_at_ms'`

| File | Action | Why |
|------|--------|-----|
| `schema/700_tenant_wallet.sql` | EDIT | Drops the `free_trial_ends_at` column from the wallet |
| `schema/821_memory_entries_fleet_fk.sql` | CREATE | §2 — the memory rows gain a parent so erasure reaches them |
| `schema/embed.zig` | EDIT | Registers slot 821 |
| `src/agentsfleetd/state/tenant_billing.zig` | EDIT | Deletes the window predicate and the two struct fields projecting it |
| `src/agentsfleetd/state/tenant_billing_rates.zig` | EDIT | Deletes the short-circuit branch; the resolvers lose their window and clock parameters |
| `src/agentsfleetd/state/tenant_billing_store.zig` | EDIT | Stops reading the dropped column |
| `src/agentsfleetd/state/sql.zig` | EDIT | Removes the column from the wallet read |
| `src/agentsfleetd/state/tenant_billing_test.zig` | EDIT | Drops the window unit tests; keeps the arithmetic ones |
| `src/agentsfleetd/state/tenant_billing_edge_integration_test.zig` | EDIT | Re-points the priced-stage cases off the injected clock |
| `src/agentsfleetd/http/handlers/tenant_billing.zig` | EDIT | Stops emitting the `free_trial` object |
| `src/agentsfleetd/fleet_runtime/metering.zig` | EDIT | Call site loses the window argument |
| `src/agentsfleetd/fleet_runtime/metering_edge_integration_test.zig` | EDIT | Two call sites |
| `src/agentsfleetd/fleet/renewal.zig` | EDIT | Call site loses the window and clock arguments |
| `src/agentsfleetd/fleet/service_renew.zig` | EDIT | Stops threading the window down to the resolver |
| `src/agentsfleetd/fleet/service_renew_integration_test.zig` | EDIT | Fixture and call sites |
| `src/agentsfleetd/fleet/service_token_splits_wire_integration_test.zig` | EDIT | Two resolver call sites |
| `src/agentsfleetd/fleet/budget_gate_integration_test.zig` | EDIT | Fixture reference |
| `src/agentsfleetd/db/test_fixtures.zig` | EDIT | Deletes the window fixture constants and the column write — the repository's only writer |
| `src/agentsfleetd/memory/fleet_memory_integration_test.zig` | EDIT | §2 — the foreign key's behavioural tests |
| `public/openapi.json` | EDIT | Drops `free_trial` from the schema **and from its `required` list** |
| `public/openapi/paths/billing.yaml` | EDIT | Same, in the source document the bundle is generated from |
| `cli/src/constants/billing.ts` | EDIT | Drops the Command-Line Interface (CLI) mirror of the field |
| `ui/packages/app/lib/types.ts` | EDIT | Drops the response type member |
| `ui/packages/app/tests/billing-card.test.ts` | EDIT | Response fixture |
| `ui/packages/app/tests/fleets.test.ts` | EDIT | Response fixture |
| `ui/packages/website/src/lib/rates.ts` | EDIT | Drops the pricing gate; the marketing copy constants stay (see §1 default) |
| `ui/packages/website/src/lib/rates.test.ts` | EDIT | Drops the gate's cases |
| `docs/architecture/billing_and_provider_keys.md` | EDIT | Describes the window as live |
| `docs/architecture/runner_fleet.md` | EDIT | Two references |
| `docs/architecture/README.md` | EDIT | One reference |
| `docs/architecture/memory.md` | EDIT | §2 — currently documents the absence of the foreign key as deliberate |

## Applicable Rules

- **`~/Projects/dotfiles/docs/greptile-learnings/RULES.md`** — **NDC** (the deletion leaves no dead branch behind), **ORP** (orphan sweep: the window constants and the predicate must have zero remaining references), **NLR** (touch-it-fix-it on the doc comments that describe the window as live), **UFS** (the website copy constants that survive are named, not retyped), **FLL** (`tenant_billing.zig` shrinks; no file approaches the cap).
- **`~/Projects/dotfiles/docs/SCHEMA_CONVENTIONS.md`** — a column drop plus a new slot; slot numbering and the embed registration follow the existing order.
- **`~/Projects/dotfiles/docs/REST_API_DESIGN_GUIDELINES.md`** — the billing response is a published surface and this removes a required member from it.
- **`dispatch/write_zig.md`** — every edited `*.zig` file; the drain rule applies to the store read that loses a column.
- **`dispatch/write_ts_adhere_bun.md`** — `cli/`, `ui/packages/app`, `ui/packages/website`.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes — most of the diff is `*.zig` | Cross-compile both linux targets; `make lint-zig` per commit |
| PUB / Struct-Shape | yes — `isFreeTrialActive` and `computeStageChargeAt` are removed `pub` symbols, and two struct members go | Removal, not addition; the orphan sweep is the proof. Record the shape verdict for `TenantBilling` once its two members are gone |
| File & Function Length (≤350/≤50/≤70) | no — every touched file shrinks | Nothing to split |
| UFS (repeated/semantic literals) | yes — the surviving website copy constants | Keep them named in `rates.ts`; no literal is retyped at a call site |
| UI Substitution / DESIGN TOKEN | no — no markup or styling changes | Type and constant edits only |
| LOGGING / LIFECYCLE / ERROR REGISTRY | no | No log line, lifecycle pair, or error code is added or removed |
| SCHEMA (Schema Table Removal Guard) | yes — a column drop and a new slot | The guard fires on `schema/700`; the drop is a column, not a table, and slot 821 is additive |

## Prior-Art / Reference Implementations

- **Reference:** the parked `M154_002` branch, pushed at `8ff760da6` — it performed this exact deletion once already, inside a much larger diff. It is a **reference to read, not a branch to cherry-pick**: the commit carrying the deletion (`de16e9783`) also rewrites the elevation machinery across forty files, which is why this workstream re-authors against `main` instead. Read its `schema/821` verbatim — that file is self-contained and correct.
- **Reference:** `~/Projects/dotfiles/docs/REST_API_DESIGN_GUIDELINES.md` plus the sibling handlers under `src/agentsfleetd/http/handlers/` — for how a published response member is removed.

## Sections (implementation slices)

### §1 — Delete the free trial

The promotional window disappears from the schema, the pricing path, and the published response. This is the whole revenue defect: with the window gone, a metered platform stage prices from the catalogue, which is what every rate row in the catalogue was written for.

**Implementation default:** delete rather than default the column. Backfilling a real boundary would start charging tenants on a date nobody chose and would keep a clock in the pricing path; the starter grant already bounds free usage and is the mechanism the product describes.

**Implementation default:** the website copy constants (`FREE_TRIAL_PILL` and the sentence beside it in `ui/packages/website/src/lib/rates.ts`) **stay**. They read "Free during early access", which describes the starter grant and remains true. Only the pricing gate above them goes. Their names become inaccurate once the window is gone — propose the rename to Indy in Discovery and take his answer; do not rename unilaterally, because the strings are consumed by four component tests and one end-to-end scenario.

- **Dimension 1.1** — The wallet no longer has a window column, and the wallet read no longer selects one → Test `test_wallet_row_has_no_trial_column`
- **Dimension 1.2** — No rate resolver accepts a time parameter; the time-injected sibling is gone → Test `test_rate_resolvers_take_no_clock`
- **Dimension 1.3** — A metered platform stage charges the catalogue rate rather than zero → Test `test_metered_platform_stage_charges_catalogue_rate`
- **Dimension 1.4** — A platform stage naming a model the catalogue does not price fails closed instead of pricing at zero → Test `test_uncatalogued_platform_model_is_refused`
- **Dimension 1.5** — A `self_managed` stage still charges the run rate only, token tiers recorded and not charged → Test `test_self_managed_charges_run_rate_only`
- **Dimension 1.6** — `GET /v1/tenants/me/billing` returns no `free_trial` member, and the published schema does not list it as required → Test `test_billing_response_omits_free_trial`
- **Dimension 1.7** — The starter grant and `balance_exhausted_at` remain the only free-usage boundary, unchanged by this diff → Test `test_starter_grant_still_bounds_free_usage`

### §2 — The memory rows gain a parent

`memory.memory_entries.fleet_id` is a bare identifier with no referential edge, so deleting a fleet leaves its memory behind. Every sweep that could remove those rows is scoped by a fleet the caller enumerated, so a row whose fleet is already gone is unreachable by any of them — an erased account keeps its memory permanently. Slot 821 adds the edge with `ON DELETE CASCADE`.

This slice is **separable**: it shares no code with §1 and rides along only because both are carve-outs of the same parked milestone. If Indy prefers a single-purpose Pull Request, §2 moves to its own workstream without touching §1.

**Implementation default:** the edge, not a sweep. A sweep would need a second scoping mechanism for rows whose parent is gone — which is the state that produced the gap. `ADD CONSTRAINT` validates every existing row, so the migration applying cleanly is itself the proof that no orphan is already present.

**This changes documented behaviour.** `docs/architecture/memory.md` states that the table carries no foreign key and survives workspace destruction by design, with the role boundary as the isolation. The role boundary is unchanged — a referential action runs with the table owner's authority, so `memory_runtime` gains no reach into `core` — but "survives workspace destruction" stops being true and the doc says so.

- **Dimension 2.1** — Deleting a fleet erases its memory, performed by a role holding no grant on the memory table → Test `test_fleet_delete_cascades_memory`
- **Dimension 2.2** — A memory write naming a fleet that does not exist is refused rather than orphaned, and the session survives the refusal → Test `test_memory_write_for_absent_fleet_is_refused`
- **Dimension 2.3** — The writing role still holds no grant on `core`, and the write still resolves the reference → Test `test_memory_write_holds_no_core_grant`

## Interfaces

```
GET /v1/tenants/me/billing

BEFORE (main):
{
  "balance_nanos": 5000000000,
  "grant_source": "bootstrap_starter_grant",
  "updated_at_ms": 1785542400000,
  "balance_exhausted_at_ms": null,
  "free_trial": { "active": true, "ends_at_ms": null }     <-- removed
}

AFTER:
{
  "balance_nanos": 5000000000,
  "grant_source": "bootstrap_starter_grant",
  "updated_at_ms": 1785542400000,
  "balance_exhausted_at_ms": null
}

Internal signatures — the window and clock parameters are removed, not defaulted:

  computeStageCharge(conn, provider, posture, model, elapsed_ms,
                     input_tokens, cached_input_tokens, output_tokens) !i64
  resolveRenewSliceRates(conn, provider, posture, model) !?SliceRates

  computeStageChargeAt(...)  -- DELETED; its only caller injected a clock to
                                price states past the window
  isFreeTrialActive(...)     -- DELETED
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Uncatalogued model, platform posture | The catalogue has no row for `(provider, model)`; previously the window branch returned zero rates ahead of the lookup | `error.ModelNotPriced`. Renew and settle fail closed; the lease-estimate gate fails open, since an estimate is not a charge. This behaviour exists on `main` but is unreachable while the window is open — the deletion is what exposes it |
| Catalogue unreachable | The rate read errors rather than returning "no row" | Unchanged — the caller fails closed rather than pricing from a stale value |
| Client still reads `free_trial` | A consumer pinned to the old response shape | The member is absent. Documented as a breaking change in the changelog and the published schema; the app never rendered it, and the CLI mirror is removed in the same diff |
| Migration re-applied | Slot 821 re-runs against a database that already has the constraint | `embed.zig` skips a recorded slot version, so re-application is unreachable from any lane; the slot's own `DROP … IF EXISTS` covers a hand-applied re-run |
| Memory write races an erasure | A capture commits while the fleet row is being deleted | Either it commits before the parent goes and cascades away, or it blocks on that row's lock and fails closed on the missing parent. Both states are the foreign key's definition; neither leaves a row behind |

## Invariants

1. **Pricing is clock-independent.** No function in `tenant_billing_rates.zig` takes a time parameter or reads the clock — enforced by Dimension 1.2's grep assertion, which fails on any reintroduced `now_ms` or `clock.nowMillis()` in that file.
2. **The window has no survivors.** No identifier matching the discovery grep remains in `src/`, `schema/`, `cli/`, `ui/`, or `public/` — enforced by the Dead Code Sweep greps as rubric rows, not by review.
3. **The published schema and the handler agree.** `make check-openapi` fails if the response and the document diverge — this is the check that caught the `required` list on the parked branch only after a follow-up commit.
4. **Memory reachability.** Every row in `memory.memory_entries` has a live parent in `core.fleets` — enforced by the foreign key itself, and validated across existing rows by `ADD CONSTRAINT` at migration time.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| not applicable — no product or operator signal changes | not applicable | The diff removes a response member and a pricing branch; it adds, renames, and removes no event | not applicable | not applicable | not applicable |

The charge itself is already carried by the existing ledger row and the wallet debit; this workstream changes the amount those record, never whether they are recorded. No analytics or funnel playbook update is required — recorded in Discovery per the rule.

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | integration | `test_wallet_row_has_no_trial_column` | The wallet read returns its documented members and the catalogue reports no `free_trial_ends_at` column on `billing.tenant_wallet` |
| 1.2 | unit | `test_rate_resolvers_take_no_clock` | `tenant_billing_rates.zig` contains zero occurrences of `now_ms` and `clock.nowMillis` — a compile-time-adjacent grep assertion, so a reintroduced clock fails the suite rather than review |
| 1.3 | integration | `test_metered_platform_stage_charges_catalogue_rate` | A metered platform stage over a catalogued model with non-zero elapsed time and non-zero tokens charges a positive amount equal to the catalogue arithmetic; the wallet decrement and the ledger row agree with it |
| 1.4 | integration | `test_uncatalogued_platform_model_is_refused` | Platform posture, a model absent from the catalogue → `error.ModelNotPriced`, no wallet movement, no ledger row |
| 1.5 | unit | `test_self_managed_charges_run_rate_only` | `self_managed` posture → run rate applied, all three token tiers zero |
| 1.6 | integration | `test_billing_response_omits_free_trial` | The billing endpoint's body has no `free_trial` key, and `public/openapi.json` neither defines the member nor lists it in `required` |
| 1.7 | integration | `test_starter_grant_still_bounds_free_usage` | A tenant at the starter grant is charged until the balance reaches zero, then `balance_exhausted_at` is stamped once — proving the grant, not the deleted window, is the boundary |
| 1.3 | e2e | `test_e2e_billing_surface_has_no_trial` | The rendered billing surface and the Command-Line Interface (CLI) billing command both complete without a `free_trial` member in the response they consume |
| 2.1 | integration | `test_fleet_delete_cascades_memory` | Seed a memory row, delete the parent fleet as a role holding no grant on the memory table, count 0 |
| 2.2 | integration | `test_memory_write_for_absent_fleet_is_refused` | A write naming an unseeded fleet is refused; the count for that identifier stays 0 and the session serves the next statement |
| 2.3 | integration | `test_memory_write_holds_no_core_grant` | The writing role cannot name `core.fleets` at all, and the write still resolves the reference |
| regression | integration | `test_renewal_sql_matches_zig_arithmetic` | The existing pin that the renewal Common Table Expression (CTE) and the Zig resolver produce identical charges still passes with the window parameters gone |
| regression | unit | `test_slice_charge_arithmetic_unchanged` | The per-tier and elapsed arithmetic is byte-identical to `main` for the same rates and deltas |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | A metered platform stage charges the catalogue rate (§1) | `zig build test -Dtest-filter="charges catalogue rate"` | `Build Summary` reports all tests passed | P0 | |
| R2 | No rate resolver reads a clock (§1) | `grep -cE 'now_ms\|clock\.nowMillis' src/agentsfleetd/state/tenant_billing_rates.zig` | `0` | P0 | |
| R3 | The window has no survivors anywhere (§1) | `git grep -rn -wE 'free_trial\|freeTrial\|isFreeTrialActive\|trial_ends_at_ms' -- src schema cli ui public \| wc -l` | `0` | P0 | |
| R4 | The published schema drops the member and its required entry (§1) | `make check-openapi && grep -c 'free_trial' public/openapi.json` | exit 0 then `0` | P0 | |
| R5 | A fleet delete erases its memory (§2) | `zig build test -Dtest-filter="cascades its memory"` | `Build Summary` reports all tests passed | P0 | |
| R6 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | |
| S1 | Unit lanes pass | `make test-unit-all` | exit 0 | P0 | |
| S2 | Lint clean | `make lint-all` | exit 0 | P0 | |
| S3 | Integration passes | `make test-integration` | exit 0 | P0 | |
| S5 | No leaks | `make memleak` | exit 0 | P0 | |
| S6 | Cross-compile | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` | exit 0 | P0 | |
| S7 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S9 | Orphan sweep | Dead Code Sweep greps below | 0 matches | P0 | |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line (`342 passed`); long evidence goes to PR Session Notes with a pointer here. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE; a P1 ❌ ships only with an Indy-acked deferral quote in Discovery.

**A note on Zig lane results.** A Zig test binary exits 0 whether or not its tests ran and whether or not they passed, and `zig build` prints `failed command:` on fully green runs. Grade R1 and R5 from the `Build Summary` counts, never from an exit status or a `✓` line.

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

| File to delete | Verify |
|----------------|--------|
| none — this workstream deletes symbols and a column, not files | `true` |

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| `isFreeTrialActive` | `git grep -rn -w 'isFreeTrialActive' \| head` | 0 matches |
| `computeStageChargeAt` | `git grep -rn -w 'computeStageChargeAt' \| head` | 0 matches |
| `free_trial_ends_at` | `git grep -rn -w 'free_trial_ends_at' \| head` | 0 matches |
| `trial_ends_at_ms` | `git grep -rn -w 'trial_ends_at_ms' \| head` | 0 matches |
| `free_trial_active` | `git grep -rn -w 'free_trial_active' \| head` | 0 matches |
| `free_trial` (response member) | `git grep -rn -w 'free_trial' -- src public cli ui \| head` | 0 matches |
| `TRIAL_ENDS_AT_MS` / `TRIAL_ENDED_AT_MS` | `git grep -rn -wE 'TRIAL_ENDS_AT_MS\|TRIAL_ENDED_AT_MS' \| head` | 0 matches |

## Out of Scope

- **Every role and grant change from M154_002.** Parked in `docs/v2/done/`; it ships nowhere until the deployed Application Programming Interface assumes the runtime role. Nothing in this workstream depends on it.
- **Row-level tenant isolation.** Application predicates remain the only tenant boundary. Named as its own workstream in `docs/architecture/runner_fleet.md`, and the higher-value neighbour of the parked privilege work.
- **Renaming the surviving website copy constants.** Decided against — Indy confirmed the copy is accurate, so the constants stay exactly as they are, names included. See Discovery.
- **Backfilling a real promotional window.** Rejected in §1 — it would start charging on an unchosen date and keep a clock in the pricing path.

---

## Product Clarity (authoring record)

1. **Successful user moment** — A tenant runs a metered stage, opens the billing surface, and sees the balance has gone down by the amount the published rate says it should. Today it does not move at all.
2. **Preserved user behaviour** — Signup still issues the starter grant; the balance read, the exhaustion stamp, and the replenish path are untouched. The app never rendered the trial — it existed only as a response member and a type — so no screen changes. The website's "Free during early access" message stays true and stays put.
3. **Optimal-way check** — Deleting is the most direct route to the moment: the window is the only thing between a catalogued rate and a charge. The gap to the unconstrained-optimal shape is that a promotional window is a real product capability we are removing rather than fixing; that is acceptable because the starter grant already expresses the same intent, with a bound that is a balance rather than a clock.
4. **Rebuild-vs-iterate** — Iterate. The pricing path is correct once the short-circuit is gone; nothing about the catalogue, the arithmetic, or the ledger needs rebuilding. A refactor here would trade away a determinism the deletion actually improves.
5. **What we build** — A column drop, the removal of one predicate and one branch, the collapse of a time-injected resolver into its plain sibling, and the removal of one published response member with its mirrors.
6. **What we do NOT build** — A replacement promotional mechanism (the grant is it); a migration that backfills a boundary; any change to how the grant is issued or exhausted; anything from the parked privilege milestone.
7. **Fit with existing features** — Compounds with the starter grant and the exhaustion signal, which become the whole free-usage story. It must not destabilise the renewal path, where the same rates are applied in SQL and must keep agreeing with the Zig resolver.
8. **Surface order** — Application Programming Interface first. The published response is the surface that changes; the Command-Line Interface mirror and the app type follow it in the same diff so no consumer is left describing a member that no longer exists.
9. **Dashboard restraint** — Nothing is added to hide. The trial member is removed rather than replaced by a new indicator; the balance and the exhaustion timestamp are the only signals, and both already have real values behind them.
10. **Confused-user next step** — A tenant who expected free usage sees a balance and an exhaustion timestamp on the billing surface, and the website states what the free allowance is. A consumer still sending the old field gets a response without it and a changelog entry naming the removal.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** Two Sections in one Pull Request, because both are carve-outs of the same parked milestone and both are small. §1 is the revenue defect and the reason this workstream exists; §2 is an erasure gap that shares no code with it.
- **Alternatives considered:** (a) **Cherry-pick the parked branch's deletion commit** — rejected: that commit also rewrites the elevation machinery across forty files, so there is no boundary that isolates the deletion. Re-authoring against `main` is both smaller and reviewable. (b) **Repair the window instead of deleting it** — rejected in §1: it keeps a clock in the pricing path and picks a charging date nobody chose. (c) **Split §2 into its own workstream** — live option, named in §2; it costs a second Pull Request and buys a single-purpose diff. Indy's call.
- **Patch-vs-refactor verdict:** this is a **patch**. The problem is one branch sitting ahead of a lookup, and the fix is to remove it. The collapse of the time-injected resolver is a consequence of the deletion, not a refactor undertaken alongside it.

## Discovery (consult log)

- **Consults** — Architecture / Legacy-Design / gate-flag triage: the question asked + Indy's decision.

  > Indy (2026-08-13): "i want only the removal of free trial in this PR other grant and role related commits are not needed? I find it an over kill for this stage." — context: M154_002's privilege boundary was found unwired (`api_runtime` is `NOLOGIN` and no production code assumes it), so its grants govern an identity nothing runs as. The free-trial deletion was carved out into this workstream and the rest parked.

  > Indy (2026-08-13): "we can design and add it better" — context: the privilege boundary returns as its own design, starting from the login-role edge rather than from the grants.

  > Indy (2026-08-13): "web site is true" — context: asked whether the surviving website copy constants (`FREE_TRIAL_PILL` and the sentence beside it) should be renamed once the window is gone, since the text stays accurate under a name that no longer is. The copy is true of the starter grant, so both the strings and their names stay untouched. No rename lands in this workstream and none is queued.

  > Indy (2026-08-13): "memory foreign key scan be added in this PR of M164_001" — context: asked whether §2 should split into its own workstream for a single-purpose Pull Request. It stays here.

  > Indy (2026-08-13): "docs PR#171 close and reopen" — context: `agentsfleet/docs` #171 covered "privilege boundaries and the free-trial removal", half of which now ships nowhere. Closed; the replacement is authored under this workstream at DOCUMENT, when the code it describes is final.

  > Indy (2026-08-13): "Since M164 PR carries it ignore main" — context: local `main` carries spec commits not yet on `origin/main`, so this branch's Pull Request diff includes them. Accepted rather than pushing `main` first.

- **Metrics review** — No analytics or funnel playbook update required: the diff removes a response member and a pricing branch, and adds, renames, and removes no event. The charge amount changes; whether a charge is recorded does not.

- **Skill-chain outcomes** — `/write-unit-test`, `/review`, `kishore-babysit-prs` results recorded here as the work proceeds.

- **Deferrals** — every "deferred to follow-up" needs an **Indy-acked verbatim quote** here, format `> Indy (YYYY-MM-DD HH:MM): "<quote>" — context: <which item, why>`. An agent-unilateral deferral is **incomplete scope, not deferral**, and blocks CHORE(close) until the item lands or the quote is captured.
