<!--
SPEC AUTHORING RULES (the one comment that survives):
- The spec standard (spec.md) holds every rule. Each tpl: comment names the
  rules that govern its section; delete it once the section is filled.
- Body order is the implementing agent's read order.
- No time or effort estimates, owners or implementation dates (SPEC-ESTIMATE).
-->

# M240_002: Billing settles off the lease path through one relay, and sets no money aside

**Prototype:** v2.0.0

**Milestone:** M240

**Workstream:** 002

**Date:** Nov 05, 2026

**Status:** IN_PROGRESS

**Priority:** P1, every report took a tenant's wallet row lock twice after it committed, so one slow billing statement delayed every runner of that tenant

**Categories:** API, DOCS, INFRA, OBS

**Batch:** B1, independent of M240_001

**Depends on:** none

**Provenance:** LLM-drafted (Claude Fable 5.1, Nov 05, 2026); rewritten (Claude Opus 5.5, Nov 05, 2026) after the owner chose a relay over an in-memory queue and then removed holds; owner decisions quoted in Discovery

**Canonical architecture:** `docs/architecture/billing_and_provider_keys.md` §5 "The balance gate", §11 "One billing lifecycle behind a `Biller` trait"

---

## Overview

**Goal (testable):** `report_returns_while_the_wallet_row_is_locked`: a test transaction holds one tenant's `billing.wallets` row locked, and that tenant's lease still reports and returns in less than the 2 s lock timeout. After the lock is released, one relay pass settles the report's lines and leaves the balance lower by exactly the report's charge.

**Problem:** A report committed its usage lines, then settled them under the tenant's wallet row lock, then released the work's hold under the same lock again. A renewal settled its slice the same way, and a setup chat report did both. Every runner of one tenant queued on that row, and a person's chat completion waited for both billing transactions. Holds added a second balance to keep in sync: a fleet's hold covered only 200 tokens' worth at issue, too little to stop parallel overspend, while a finished run's leftover hold kept counting against the tenant until something released it.

**Owner's constraint, verbatim:** "a billing function shouldnt impact the next lease and a few seconds of delay to claim/settle isnt an issue for an end user."

**Solution summary:** Three changes.
- The request path writes usage lines and returns. Renew, fleet report and setup chat report call no money movement; `billing.usage_lines` is the durable outbox.
- The supervised billing sweeper becomes the relay. Every 2 s, and again at once after a full pass that settled something, it hands each unsettled line to `Arc<dyn Biller>::settle`. An expired, unreported lease keeps the 120 s grace before its last slice is billed, so a late report still wins.
- No money is set aside. `Biller::reserve`, `release` and `billing.holds` are gone. `Biller::check` reads the balance less the payer's unsettled lines, with no lock. Parallel runs can overdraw by at most one slice each, and their next check refuses.

**Design flow:**

```text
ADMIT    gate: balance − unsettled lines ≥ floor           (one read, no lock)
RENEW    Biller::check(next = floor) → extend txn: meter + lines → COMMIT → return
REPORT   commit txn: meter + lines + terminal → COMMIT → return
SETUP    claim gate: balance − unsettled ≥ bound; each model call and renewal: check(bound)
         report txn: lines + receipt → COMMIT → return

RELAY (BILLING task, every 2 s; again at once after a full pass that settled something, up to 10 passes)
         expired, unreported leases past GRACE → log and settle the last slice
         unsettled usage_lines → Biller::settle per work      (settled_at IS NULL under the wallet lock)
```

**Bounds this spec states:** with the daemon up, a committed line is charged within one relay interval plus one pass. With the daemon down, it is charged on the first pass after restart. Runs that start together all pass their checks; each spends at most one slice before its next check sees the logged lines and refuses, so the balance goes below zero by at most one slice per parallel run, and that debt refuses the tenant's next work until a grant repays it.

**Risks this spec carries:** the balance a person sees trails the charges list by up to one relay pass. Two daemon replicas run two relays over the same lines; neither charges a line twice, but the second does wasted reads. A tenant can carry a bounded debt after parallel runs.

## PR Intent & comprehension handshake

- **PR title (eventual):** Settle billing through one relay, off the lease path, with no holds
- **Intent (one sentence):** No report or renewal waits on moving money, every charge goes through one supervised relay to the `Biller` trait, and the balance less unsettled lines is the one number every gate reads.
- **Authoring handshake:** Indy asked for billing to be "disconnected from the agentsfleet system" behind "an abstracted interface", with Postgres staying the store, then called holds "stupid logic" and chose to remove them here. ASSUMPTIONS I'M MAKING:
  1. The `Biller` trait is the interface: `check`, `settle`, `balance`.
  2. `billing.usage_lines` is the outbox. No in-memory queue, no `afd_queue` crate, no `reserve_detached`.
  3. The spending check stays synchronous and becomes a lock-free read.
  4. No public field changes. `balance_nanos` moves seconds later than today.
- **Implementer handshake:** recorded in the Pull Request Session Notes at PLAN.

## Implementing agent — read these first

1. `rustd/crates/afd_billing/src/biller/mod.rs` and `ledger/mod.rs`: the trait and the built-in biller's one-read `check`.
2. `rustd/crates/afd_fleet/src/lease/coverage.rs`, `admit/gates.rs`, `renew/mod.rs`, `report/mod.rs`: the fleet gates and the removed post-commit calls.
3. `rustd/crates/afd_authoring/src/model/budget.rs` and `lease/report/mod.rs`: the setup chat gate, the per-call check, and the removed `pay`.
4. `rustd/crates/afd_fleet/src/lease/billing_sweep.rs` and `agentsfleetd/src/sweepers/billing.rs`: the relay.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| this spec | EDIT | moved to `done/` at CHORE(close) |
| `rustd/crates/afd_billing/src/biller/{mod,observed,recording,conformance}.rs`, `biller/outage/{mod,tests}.rs` | EDIT | §3: `check` replaces `reserve`; `release`, `Hold`, `Reservation` and `Release` go |
| `rustd/crates/afd_billing/src/ledger/{mod,settle,sweep}.rs`, `ledger/sql/{select_balance,select_owed,select_unsettled,settle_lines}.sql`, `ledger/sql/select_unsettled_work.sql` | EDIT, CREATE | §2, §3: one-read check, settle without capture, no line grace, a test-util read of one work's lines |
| `rustd/crates/afd_billing/src/ledger/reserve.rs`, `ledger/sql/{select_hold,select_held_by_others,upsert_hold,capture_hold,release_hold,select_work_tenant}.sql` | DELETE | §3 |
| `rustd/crates/afd_billing/src/sql/mod.rs` | EDIT | doc |
| `rustd/crates/afd_observability/src/{metrics/declared/billing,metrics/label/billing,metrics/label/tests,producers/billing}.rs`, `tests/billing_capture.rs`, `docs/metrics.census.tsv` | EDIT | §3: `agentsfleet_billing_funds_check_total` replaces the reserve family; the held gauge goes |
| `rustd/crates/afd_fleet/src/lease/{mod,coverage,billing_sweep}.rs`, `lease/{renew,report,deliver}/mod.rs`, `lease/admit/gates.rs`, `lease/{settle,usage/mod}.rs` | EDIT | §1, §2, §3 |
| `rustd/crates/afd_fleet/src/lease/money.rs`, `lease/sql/select_finished_holds.sql` | DELETE | §1, §3 |
| `rustd/crates/afd_authoring/src/{lib,model/budget,review/wallet}.rs`, `lease/{renew,claim/mod,claim/issue/mod}.rs`, `lease/report/{mod,settle,retained}.rs`, `model/dispatch/funding.rs` | EDIT | §1, §3 |
| `rustd/crates/afd_authoring/src/billing_sweep/**` | DELETE | §3 |
| `rustd/crates/agentsfleetd/src/sweepers/billing.rs` | EDIT | §2 |
| `schema/2210_billing_holds_drop.sql`, `schema/2220_billing_usage_lines_unsettled_tenant_index.sql`, `rustd/crates/afd_db/src/migration.rs` | CREATE, EDIT | §3: drop `billing.holds`; index each tenant's unsettled lines |
| `rustd/crates/afd_fleet/tests/integration_billing_relay.rs` | CREATE | §1, §2 |
| `rustd/crates/afd_authoring/tests/draft/admission/unreadable.rs` | RENAME from `holds.rs` | §3 |
| `rustd/crates/afd_fleet/tests/integration_lease_gates/money.rs` | DELETE | §1, §3: both its cases tested calls that no longer exist |
| `rustd/crates/{afd_billing,afd_fleet,afd_authoring,afd_api,afd_db,afd_observability,agentsfleetd}/tests/**` | EDIT | tests follow the mechanisms |
| `docs/architecture/{billing_and_provider_keys,concurrency,data_flow}.md` | EDIT | §4 |

## Applicable Rules

- **`domains/sql.md`** — SQL-SLOT, SQL-MIGRATOR, IDMP, TXN, SQL-TIMEOUT, SQL-SWEEPCLOCK, SQL-PLAN
- **`languages/rust.md`** — ERR-RS, FN-RS, RS-RACE, RS-TESTCONST
- **`code.md`** — NDC, NLR, NLG, CODE-SEAM, FLL, CODE-MSID
- **`tests.md`** — TEST-REAL, TEST-CONTEND, TEST-INJECT, TEST-DELTA, TEST-SCOPE, TEST-LANE
- **`domains/logging.md`** — LOG-EVENT, LOG-FIELDS, LOG-HOT, LOG-PAIR, LOG-FAIL, LOG-CODE
- **`docs.md`** — DOC-02, DOC-05 on the architecture pages

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| `make lint-all` | yes | no new `#[expect]` |
| metrics census check | yes, two families | the census rows change in the same commit |
| `docs.language` | yes | short sentences, plain words |
| gitleaks pre-commit | no secrets in scope | runs on every commit |
| File & Function Length | yes: `lease_renewal/wallet.rs` | its helpers are extracted |

## Prior-Art / Reference Implementations

- **Reference:** the billing sweeper already settled unsettled lines. This spec makes it the only settler rather than adding a second mechanism.
- **Rejected reference:** the in-memory `Queued` biller and `afd_queue` crate from #808, reverted in `eadbe7f5b`: one sink, one producer, one consumer, unsupervised workers, and jobs lost on a crash.

## Sections (implementation slices)

### §1 — The request path writes lines and moves no money

Renew, fleet report and setup chat report commit their usage lines and return. `settle_usage`, `release_usage`, `hold_issued`, `hold_claimed` and `pay` are deleted with their events.

- **Dimension 1.1** — DONE: input a locked wallet row and a reporting lease; output the report returns in under 2 s with its lines unsettled; rejects a report that waits on the lock → Test `report_returns_while_the_wallet_row_is_locked` (integration, `afd_fleet`).
- **Dimension 1.2** — DONE: input a renewal carrying one priced request; output its lines logged and unsettled, the balance unchanged; rejects a renewal that settles → Test `a_renewal_logs_its_slice_and_leaves_it_unsettled` (integration, `afd_fleet`).
- **Dimension 1.3** — DONE: input a setup chat report with known usage; output two logged, unsettled lines and an unchanged balance; rejects a report that charges → Test `a_report_logs_its_lines_and_leaves_them_to_the_relay` (integration, `afd_authoring`).

### §2 — The billing sweeper is the relay

Lines settle with no age filter. An expired, unreported lease keeps `GRACE`, owned by `afd_fleet`, before its last slice is billed. The task runs every 2 s and up to 10 passes back to back while a pass fills its batch and settles something. A clean pass logs at `debug`; a pass the biller refused part of warns once with the first refusal's `error_code`, and each refused work logs at `debug` with its `work_id`.

- **Dimension 2.1** — DONE: input a reported lease; output one pass settles its lines and a second settles nothing; rejects a pass that waits → Test `one_pass_settles_a_reported_lease` (integration, `afd_fleet`).
- **Dimension 2.2** — DONE: input a reported attempt and a biller that cannot answer; output the first pass charges nothing, the next charges once; rejects a double charge → Test `a_reported_attempt_is_settled_on_the_next_pass` (integration, `afd_authoring`).
- **Dimension 2.3** — DONE: input 100 passes released together over one line; output the passes overlapped and the line is charged once → Test `concurrent_passes_charge_each_line_once` (integration, `afd_fleet`, TEST-CONTEND).
- **Dimension 2.4** — DONE: input passes reporting full, full, partial; output three passes in one tick, and a never-emptying backlog stops at the cap → Tests `a_tick_runs_passes_until_one_leaves_nothing_waiting`, `a_tick_stops_at_the_drain_cap` (unit, `agentsfleetd`).
- **Dimension 2.5** — DONE: input a pass with refusals; output one `warn` under the refusal's code → Test `a_refused_pass_warns_with_its_code` (unit, `agentsfleetd`).

### §3 — No money is set aside

`Biller::check` replaces `reserve`: the balance less the payer's unsettled lines must cover what the work is about to spend, read without a lock. Every gate reads that same number. `billing.holds` is dropped in slot 2210; slot 2220 indexes each tenant's unsettled lines.

- **Dimension 3.1** — DONE: input a logged, unsettled line; output a check one nano past what it leaves is `Short` with the exact amount left → Test `test_unsettled_lines_count_against_a_check` (integration, `afd_billing`).
- **Dimension 3.2** — DONE: input ten runs started at once on three slices of balance; output all ten start, the balance ends seven slices below zero, and every next check is refused; rejects a debt past one slice per run → Test `parallel_runs_overdraw_by_at_most_a_slice_each_then_stop` (integration, `afd_billing`).
- **Dimension 3.3** — DONE: input another attempt's unsettled spend equal to the balance; output the claim is refused as the wallet refusal and admitted after a top-up → Test `uncharged_spend_that_leaves_less_than_the_bound_refuses_admission` (integration, `afd_authoring`).
- **Dimension 3.4** — DONE: input every biller; output covered, short and debt answers match → Test `conformance::every_check` (integration over `LedgerBiller`, unit over `RecordingBiller`).

### §4 — The pages say what the code does

- **Dimension 4.1** — DONE: `git grep -n -e billing.holds -e settle_usage -e release_usage -e hold_issued -- rustd/crates docs/architecture` prints only the migration list's historical slot.

## Interfaces

- `afd_billing::biller::Biller`: `check(&Ask) -> Result<Funds>`, `settle(&Settlement) -> Result<Receipt>`, `balance(&Account) -> Result<Balance>`. `Ask { work, payer, next }`; `Funds::{Covered, Short { available, asked }, Unconfirmed}`; `Balance { balance, unsettled, exhausted_at }`.
- `afd_billing::ledger::resettle` answers `Resettled { settled, failed, full, refused }`; `unsettled(executor, limit)` takes no grace; `unsettled_work` (test-util) reads one work's lines.
- `afd_fleet::lease::billing_sweep::GRACE` is defined there.

## Failure Modes

| Mode | Class | Trigger | Behaviour | Test |
|------|-------|---------|-----------|------|
| Wallet row locked | contention | another transaction holds it | reports return; the relay's settle waits up to 2 s, counts a failure and retries | `report_returns_while_the_wallet_row_is_locked` |
| Biller refuses | failure | Postgres fault in the relay | counted, warned with its code, retried next pass | `a_line_the_biller_cannot_settle_waits_for_the_next_pass` |
| Daemon down | failure | process killed after a report commits | the first pass after restart settles the lines | `test_the_first_sweep_settles_orphaned_lines_and_ends_expired_receipts` |
| Two relays | contention | two daemon replicas | each line charged once | `concurrent_passes_charge_each_line_once` |
| Parallel overspend | capacity | runs started together on a small balance | bounded debt, next checks refuse | `parallel_runs_overdraw_by_at_most_a_slice_each_then_stop` |
| Unreadable ledger | failure | an unsettled sum past `BIGINT` | the claim and the review refuse with the operation code | `a_ledger_that_cannot_sum_its_unsettled_lines_refuses_the_claim_and_the_review` |

## Invariants

1. No request path settles. Enforced by Dimensions 1.1 to 1.3 and the Dimension 4.1 grep.
2. A line is charged once: `SETTLE_LINES` stamps only `settled_at IS NULL` rows under the wallet lock (Dimension 2.3).
3. Nothing is lost when the relay stops: every charge is derived from committed lines.
4. Every gate reads one number, the balance less the payer's unsettled lines (Dimensions 3.1 to 3.3).

## Metrics & Observability

| Name | Audience | Emitted when | Labels | Privacy | Test |
|------|----------|--------------|--------|---------|------|
| `agentsfleet_billing_funds_check_total` | operator | every funds check | work type, outcome | no identifiers | `the_labels_spell_the_stored_values` and the label cardinality test |
| `sweeper_resettled`, unsettled gauges (existing) | operator | every relay pass | work type | none | `billing_capture.rs` |
| `billing_sweep_refused` log | operator | a pass the biller refused part of | `error_code`, count | none | `a_refused_pass_warns_with_its_code` |
| `billing_relay_refused` log | operator | one refused work | `work_id`, `error_code` at `debug` | ids only | exercised by the fault tests |

`agentsfleet_billing_reserve_total` and `agentsfleet_billing_held_nanos` are removed.

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Reports do not wait on a locked wallet | `make test-integration-rustd` | `report_returns_while_the_wallet_row_is_locked ... ok` | P0 | |
| R2 | Parallel overspend is bounded and then stopped | `make test-integration-rustd` | `parallel_runs_overdraw_by_at_most_a_slice_each_then_stop ... ok` | P0 | |
| R3 | No double charge across relays | `make test-integration-rustd` | `concurrent_passes_charge_each_line_once ... ok` | P0 | |
| R4 | The mid-run stop is kept | `make test-integration-rustd` | `integration_renew_coverage` tests pass | P0 | |
| S1 | Declared commands green | `make lint-all` · `make test-unit-all` · `make test-integration-rustd` · `make check-version` | exit 0 each | P0 | |
| S2 | No secrets | `gitleaks detect --redact --no-banner` | exit 0 | P0 | |

## Dead Code Sweep

| Deleted file or symbol | Verify | Expected |
|------------------------|--------|----------|
| holds, reserve and release | `git grep -n -w -e Reservation -e reserve_detached -e hold_issued -e hold_claimed -e release_finished -- rustd/crates` | no output |
| the deferred-settle events | `git grep -n -e _settle_deferred -e _release_deferred -- rustd/crates docs/architecture` | no output |
| `billing.holds` | `git grep -n billing.holds -- rustd/crates docs/architecture` | only `afd_db/src/migration.rs` |

## Out of Scope

- An append-only journal for wallet debits.
- A read trait over the console's billing reads.
- A billing API server or a metering vendor; either is a new `Biller` behind the relay.
- M240_001's claim changes.
- Partitioning relay work across replicas with `SKIP LOCKED`.
- Renaming `authoring.attempts.reserved_nanos`; it is the attempt's bound and a column rename is its own migration.

---

## Product Clarity (authoring record)

1. **Who feels it** — a person whose fleets run in parallel, whose chat waited on another lease's billing.
2. **What they see after** — the same replies and charges; the balance moves within seconds of a reply instead of with it. A tenant who starts many runs at once on a small balance can end slightly in debt.
3. **Optimal-way check** — the outbox and the settling sweep already existed; the shortest path removes the request path's calls and the holds that second-guessed the balance.
4. **Fit with existing features** — the outage policy is unchanged; budgets still read usage lines.
5. **Confused-user next step** — an operator who sees a balance behind the charges reads the unsettled gauges and `billing_sweep_completed`.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** §1 request path, §2 relay, §3 no holds, §4 pages. §1 and §2 land together; §3 removes the window §1 and §2 opened, where a finished run's hold still counted against the tenant.
- **Rejected alternatives:**
  - An in-memory queue in the daemon: loses jobs on a crash and duplicates the sweeper.
  - A broker or a separate billing database: a line must commit with its work, which needs an outbox in Postgres anyway.
  - DuckDB: an embedded analytics engine with one writing process; the bottleneck is a hot row, not a scan.
  - Keeping holds and accepting the window: chosen first, then reversed by the owner.
- **Patch-vs-refactor verdict:** a **refactor** of who moves money and what the gates read.

## Discovery (consult log)

- **Owner decisions, verbatim (Indy, Nov 05, 2026):**
  > "and my theory is drop the message to a queue on billing so its disconnected from the agentsfleet system"

  > "the reciever sink gets it and does what is needed a new billing api server or the agentsfleet will need to get the data from that db (for now) i presume its best to keep it an abstracted ingterface"

  > "so i want a large scale refactor here" · "so it can scale" · "yes write it and i prefer to start the implementation and send the PR." · "no doc spec"

  > "how do you decide what to decide on `billing hold value`" · "this is stupid logic" · AskUserQuestion answer: "Remove holds in this PR"

  > "if i spin 10 concurrent, after a second i may get billed? and my credit may go negative, that is still fine" · "we just wanna make sure we can test first" · AskUserQuestion answer: "Remove holds here, test first"

- **Destructive change approval:** dropping `billing.holds` (slot 2210) rests on "Remove holds in this PR" above (SQL standard: a `DROP TABLE` needs the owner's approval).
- **Datastore decision:** Postgres stays the system of record. PostHog moved its analytics warehouse off DuckDB on Nov 02, 2026, and that workload was analytics, not billing (https://posthog.com/blog/moving-off-duckdb).
- **Greptile findings on #807 (the old draft):** the balance-read test, the double-counted renewal and the balance correction belonged to designs this spec no longer has; backlog recovery is the relay's drain.
- **Standards review (zero-context, Nov 05):** 20 findings on the relay; all applied, and the refusal-window finding was first accepted, then made moot by §3.
- **Consults:** `main` at `66059667b`. Before this spec, a report awaited `settle_usage` and `release_usage` before announcing completion (`afd_fleet/src/lease/report/mod.rs`).
