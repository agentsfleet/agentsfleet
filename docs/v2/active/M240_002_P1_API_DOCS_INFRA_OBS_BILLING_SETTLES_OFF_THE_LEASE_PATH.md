<!--
SPEC AUTHORING RULES (the one comment that survives):
- The spec standard (spec.md) holds every rule. Each tpl: comment names the
  rules that govern its section; delete it once the section is filled.
- Body order is the implementing agent's read order.
- No time or effort estimates, owners or implementation dates (SPEC-ESTIMATE).
-->

# M240_002: Billing settles off the lease path, through one relay behind the `Biller` trait

**Prototype:** v2.0.0

**Milestone:** M240

**Workstream:** 002

**Date:** Nov 05, 2026

**Status:** IN_PROGRESS

**Priority:** P1, every report takes a tenant's wallet row lock twice after it commits, so one slow billing statement delays every runner of that tenant

**Categories:** API, DOCS, INFRA, OBS

**Batch:** B1, independent of M240_001 once the hold no longer moves inside issue

**Depends on:** none

**Provenance:** LLM-drafted (Claude Fable 5.1, Nov 05, 2026); rewritten (Claude Opus 5.5, Nov 05, 2026) after the owner chose a relay over an in-memory queue; owner decisions quoted in Discovery

**Canonical architecture:** `docs/architecture/billing_and_provider_keys.md` §11 "One billing lifecycle behind a `Biller` trait"

---

## Overview

**Goal (testable):** `report_returns_while_the_wallet_row_is_locked`: a test transaction holds one tenant's `billing.wallets` row locked, and that tenant's lease still reports and returns in less than the 2 s lock timeout. After the lock is released, one relay pass settles the report's lines, releases the lease's hold, and leaves the balance lower by exactly the report's charge.

**Problem:** A report commits its usage lines, then settles them under the tenant's wallet row lock, then releases the hold under the same lock again (`afd_fleet/src/lease/report/mod.rs`). A renewal settles its slice the same way after it commits (`renew/mod.rs`), and a setup chat report does both (`afd_authoring/src/lease/report/mod.rs`). Every runner of one tenant queues on that row. A statement that waits past the 2 s `LOCK_TIMEOUT` (`afd_billing/src/ledger/mod.rs`) counts as a billing outage. Billing is also wired into three request paths, so it cannot move to another process without touching the lease code.

**Owner's constraint, verbatim:** "a billing function shouldnt impact the next lease and a few seconds of delay to claim/settle isnt an issue for an end user."

**Solution summary:** Two changes.
- The request path writes usage lines and returns. Renew, fleet report and setup chat report stop calling `Biller::settle` and `Biller::release`. Their lines already commit inside their own transaction, so `billing.usage_lines` is the durable outbox.
- The supervised billing sweeper becomes the relay. Every 2 s it hands each unsettled line to `Arc<dyn Biller>` and releases the holds of finished work. A pass that fills its batch and makes progress runs again at once. Reported leases and settled attempts release with no wait. An unreported, expired lease keeps the 120 s grace, so a late report still wins.

The spending check stays on the request path. A renewal still reserves under the wallet lock, so a payer who runs out is stopped within one renewal.

**Design flow:**

```text
RENEW    gate: Biller::reserve (wallet lock, ceiling = charged_so_far + floor) → extend txn: meter + lines → COMMIT → return
REPORT   commit txn: meter + lines + terminal → COMMIT → return
SETUP    report txn: lines + receipt → COMMIT → return

RELAY (BILLING task, every 2 s; again at once after a full pass that made progress)
         expired, unreported leases past GRACE → log and settle the last slice
         unsettled usage_lines → Biller::settle per work        (settled_at IS NULL under the wallet lock)
         holds of reported leases and settled attempts → Biller::release
         holds of other finished leases past GRACE → Biller::release
```

**Bounds this spec states:** with the daemon up, a committed line settles within one relay interval plus one pass. With the daemon down, it settles on the first pass after restart. A renewal's reserve already counts unsettled spend: its ceiling is `charged_so_far` plus the next floor, and `charged_so_far` sums every line, settled or not. So the lag never lets a payer overdraw beyond what the holds already allow.

**Risks this spec carries:** the balance a person sees lags the charges list by up to one relay interval. Two daemon replicas run two relays over the same lines. Neither can charge a line twice, because the settle stamps `settled_at IS NULL` rows under the wallet lock, but the second does wasted reads.

## PR Intent & comprehension handshake

- **PR title (eventual):** Settle billing through one relay, off the lease path
- **Intent (one sentence):** No report or renewal waits on settling money, and every money movement goes through one supervised relay to the `Biller` trait, so billing can move to its own process without touching the lease code.
- **Authoring handshake:** Indy asked for billing to be "disconnected from the agentsfleet system" behind "an abstracted interface", with Postgres staying the store. ASSUMPTIONS I'M MAKING:
  1. The `Biller` trait is the interface. The relay is its only caller for settle and release.
  2. `billing.usage_lines` is the outbox. No in-memory queue, no `afd_queue` crate and no `reserve_detached`.
  3. The renewal's reserve stays synchronous: it is the spending check.
  4. No public field changes. `balance_nanos` moves seconds later than today.
- **Implementer handshake:** recorded in the Pull Request Session Notes at PLAN.

## Implementing agent — read these first

1. `rustd/crates/afd_fleet/src/lease/money.rs`, `renew/mod.rs`, `report/mod.rs`: `settle_usage` and `release_usage`, and the three calls §1 removes.
2. `rustd/crates/afd_authoring/src/lease/report/mod.rs`: `pay`, the setup chat's post-commit settle and release.
3. `rustd/crates/afd_billing/src/ledger/sweep.rs` and `settle.rs`: `resettle`, `unsettled`, and the `settled_at IS NULL` guard that makes a repeat settle charge nothing.
4. `rustd/crates/afd_fleet/src/lease/billing_sweep.rs`, `afd_authoring/src/billing_sweep/mod.rs` and `agentsfleetd/src/sweepers/billing.rs`: the passes §2 turns into the relay.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `docs/v2/active/M240_002_P1_API_DOCS_INFRA_OBS_BILLING_SETTLES_OFF_THE_LEASE_PATH.md` | EDIT | this spec, moved to `done/` at CHORE(close) |
| `rustd/crates/afd_fleet/src/lease/money.rs`, `renew/mod.rs`, `report/mod.rs` | EDIT | §1: no settle or release after commit |
| `rustd/crates/afd_authoring/src/lease/report/mod.rs` and its `settle` module | EDIT | §1: no settle or release after commit |
| `rustd/crates/afd_billing/src/ledger/sweep.rs`, `ledger/mod.rs`, `ledger/sql/select_unsettled.sql` | EDIT | §2: no grace on unsettled lines; a pass reports whether it filled its batch |
| `rustd/crates/afd_fleet/src/lease/billing_sweep.rs`, `lease/sql/select_finished_holds.sql` | EDIT | §2: the grace moves here and applies only to unreported leases |
| `rustd/crates/afd_authoring/src/billing_sweep/mod.rs`, `finished_holds.sql` | EDIT | §2: settled attempts release with no wait |
| `rustd/crates/agentsfleetd/src/sweepers/billing.rs` | EDIT | §2: 2 s interval, drain after a full pass |
| `rustd/crates/afd_fleet/tests/integration_billing_relay.rs` | CREATE | §1, §2: the relay's Dimensions |
| `rustd/crates/afd_fleet/tests/**`, `afd_authoring/tests/**`, `afd_billing/tests/**`, `agentsfleetd/tests/**` | EDIT | tests that read the wallet after a report run the relay for that work first, or wait one relay pass |
| `docs/architecture/billing_and_provider_keys.md`, `docs/architecture/concurrency.md` | EDIT | §3: the relay is the only settler |

## Applicable Rules

- **`domains/sql.md`** — SQL-SLOT, IDMP, TXN, SQL-TIMEOUT, SQL-SWEEPCLOCK, SQL-PLAN
- **`languages/rust.md`** — ERR-RS, FN-RS, RS-RACE
- **`code.md`** — NDC, NLR, NLG, CODE-SEAM, FLL
- **`tests.md`** — TEST-REAL, TEST-CONTEND, TEST-INJECT, TEST-DELTA, TEST-LANE
- **`domains/logging.md`** — LOG-EVENT, LOG-FIELDS
- **`docs.md`** — DOC-02, DOC-05 on the architecture pages

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| `make lint-all` | yes | no new `#[expect]` |
| `docs.language` | yes | short sentences, plain words |
| gitleaks pre-commit | no secrets in scope | runs on every commit |
| File & Function Length (file ≤350 lines, function ≤50, method ≤70) | yes: `billing_sweep.rs` | the drain loop lives in the sweeper task, not in the pass |

## Prior-Art / Reference Implementations

- **Reference:** the billing sweeper already settles unsettled lines and releases finished holds (`afd_fleet/src/lease/billing_sweep.rs`, `afd_billing/src/ledger/sweep.rs`). This spec makes it the only settler rather than adding a second mechanism.
- **Rejected reference:** the in-memory `Queued` biller and `afd_queue` crate from #808, reverted in `eadbe7f5b`. It had one sink, one producer and one consumer, ran unsupervised workers, and lost its jobs on a crash, so the sweeper had to repair it anyway.

## Sections (implementation slices)

### §1 — The request path writes lines and moves no money

Renew, fleet report and setup chat report commit their usage lines and return. `settle_usage`, `release_usage` and `pay` are deleted with their deferred-settle log events. Issue's first hold (`hold_issued`) and the per-call reserves stay: they are spending checks.

- **Dimension 1.1** — DONE: a report returns while its tenant's wallet row is locked → Test `report_returns_while_the_wallet_row_is_locked` (integration, `afd_fleet`).
- **Dimension 1.2** — DONE: a renewal logs its slice and settles nothing → Test `a_renewal_logs_its_slice_and_leaves_it_unsettled` (integration, `afd_fleet`).
- **Dimension 1.3** — DONE: a setup chat report moves no money → Test `a_report_logs_its_lines_and_leaves_them_to_the_relay` (integration, `afd_authoring`).

### §2 — The billing sweeper is the relay

Lines settle with no age filter. A reported lease's hold and a settled attempt's hold release on the next pass. An expired, unreported lease keeps `GRACE`, now owned by `afd_fleet`, for both its last slice and its hold. The sweeper task runs every 2 s and runs again at once after a pass that filled its batch and settled something.

- **Dimension 2.1** — DONE: one pass settles a reported lease and releases its hold → Test `one_pass_settles_a_reported_lease_and_releases_its_hold` (integration, `afd_fleet`).
- **Dimension 2.2** — DONE: an expired lease keeps its hold through the grace → Test `an_expired_lease_keeps_its_hold_until_the_grace_passes` (integration, `afd_fleet`).
- **Dimension 2.3** — DONE: a settled attempt's hold releases on the next pass → Test `a_settled_attempt_is_released_on_the_next_pass` (integration, `afd_authoring`).
- **Dimension 2.4** — DONE: two relays never charge a line twice → Test `two_concurrent_passes_charge_each_line_once` (integration, `afd_fleet`, TEST-CONTEND).
- **Dimension 2.5** — DONE: a full pass that made progress runs again at once → Test `a_full_pass_that_settled_something_drains_again` (unit, `agentsfleetd`).

### §3 — The pages say what the code does

`billing_and_provider_keys.md` §11 names the relay as the only settler and states the lag. `concurrency.md` drops the wallet from the report's lock order.

- **Dimension 3.1** — DONE: the pages agree with the code → `make lint-all` docs checks, and `git grep -n -e settle_usage -e release_usage -- docs rustd` prints nothing.

## Interfaces

- `afd_billing::ledger::resettle(database, biller) -> Result<Resettled>`; `Resettled` gains `full: bool`, true when the pass read a whole batch.
- `afd_billing::ledger::unsettled(executor, limit)`: the grace argument is gone.
- `afd_fleet::lease::billing_sweep::GRACE` is defined there; `afd_billing::ledger::GRACE` is gone.
- `Biller` signatures are unchanged.

## Failure Modes

| Mode | Class | Trigger | Behaviour | Test |
|------|-------|---------|-----------|------|
| Wallet row locked | contention | another transaction holds it | reports return; the relay's settle waits up to 2 s, counts a failure and retries next pass | `report_returns_while_the_wallet_row_is_locked` |
| Biller refuses | failure | Postgres fault in the relay | counted, retried next pass; lines and holds stay open | `a_finished_hold_the_biller_cannot_release_waits_for_the_next_pass`, `a_settled_attempt_is_released_on_the_next_pass` |
| Daemon down | failure | process killed after a report commits | lines stay unsettled; the first pass after restart settles them | `one_pass_settles_a_reported_lease_and_releases_its_hold` (a pass with no prior request-path settle is this case) |
| Two relays | contention | two daemon replicas | each line charged once | `two_concurrent_passes_charge_each_line_once` |
| Biller down for a whole batch | failure | every settle refused | no drain loop: the pass made no progress, so it waits for the next tick | `a_full_pass_that_settled_nothing_waits_for_the_tick` |

## Invariants

1. No request path calls `Biller::settle` or `Biller::release`. Enforced by Dimensions 1.1 to 1.3 and the Dimension 3.1 grep.
2. A line is charged once. `SETTLE_LINES` stamps only `settled_at IS NULL` rows, under the wallet lock (Dimension 2.4).
3. Nothing is lost when the relay stops. Every movement is derived from committed lines and lease or attempt state.

## Metrics & Observability

| Name | Audience | Emitted when | Labels | Privacy | Test |
|------|----------|--------------|--------|---------|------|
| `sweeper_resettled` (existing) | operator | the relay settles a work | work type | no identifiers | existing |
| owed gauges (existing) | operator | every pass | work type | none | existing |
| `billing_sweep_completed` log (existing) | operator | a pass moved money | counts | none | existing |

The four deferred-settle events (`lease_settle_deferred`, `lease_release_deferred`, `setup_chat_settle_deferred`, `setup_chat_release_deferred`) are deleted with their call sites.

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Reports do not wait on a locked wallet (§1) | `make test-integration-rustd` | `report_returns_while_the_wallet_row_is_locked ... ok` | P0 | |
| R2 | One pass settles and releases (§2) | `make test-integration-rustd` | `one_pass_settles_a_reported_lease_and_releases_its_hold ... ok` | P0 | |
| R3 | No double charge across relays (§2) | `make test-integration-rustd` | `two_concurrent_passes_charge_each_line_once ... ok` | P0 | |
| R4 | The mid-run stop is kept | `make test-integration-rustd` | the existing renewal refusal tests pass | P0 | |
| S1 | Declared commands green | `make lint-all` · `make test-unit-all` · `make test-integration-rustd` · `make check-version` | exit 0 each | P0 | |
| S2 | No secrets | `gitleaks detect --redact --no-banner` | exit 0 | P0 | |

## Dead Code Sweep

| Deleted file or symbol | Verify | Expected |
|------------------------|--------|----------|
| `settle_usage`, `release_usage`, `pay` | `git grep -n -w -e settle_usage -e release_usage -- rustd` | no output |
| the four deferred-settle events | `git grep -n -e _settle_deferred -e _release_deferred -- rustd docs` | no output |
| `afd_billing::ledger::GRACE` | `git grep -n "ledger::GRACE" -- rustd` | no output |

## Out of Scope

- An append-only journal for wallet debits. It serves audit and reconciliation; the relay does not need it.
- A read trait over the console's billing reads. It is needed when billing runs in another process, not before.
- A billing API server or a metering vendor. Either is a new `Biller` implementation behind the relay.
- M240_001's claim changes.
- Making the renewal's reserve lock-free. It is the spending check and stays synchronous.
- Partitioning relay work across replicas with `SKIP LOCKED`.

---

## Product Clarity (authoring record)

1. **Who feels it** — a person whose fleets run in parallel, and whose chat waits on another lease's settlement today.
2. **What they see after** — the same replies and charges. The balance moves within seconds of a reply instead of with it.
3. **Optimal-way check** — the outbox already exists and the sweeper already settles from it. The shortest path removes the request path's calls and shortens the sweeper's wait.
4. **Fit with existing features** — the authoring plane keeps its per-call reserve. The outage policy is unchanged.
5. **Confused-user next step** — an operator who sees a balance behind the charges reads the owed gauges and `billing_sweep_completed`.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** §1 request path, §2 relay, §3 pages. §1 and §2 land together: §1 alone would leave reports waiting 120 s for a settle.
- **Rejected alternatives:**
  - An in-memory queue in the daemon. Rejected: it loses jobs on a crash and duplicates the sweeper.
  - A broker or a separate billing database. Rejected: a line must commit with its work, which needs an outbox in Postgres anyway.
  - DuckDB for billing. Rejected: an embedded analytics engine with one writing process, and the bottleneck is a hot row, not a scan.
  - The hold inside the issue transaction (old §3). Rejected: a billing process outside this one can never join that transaction.
- **Patch-vs-refactor verdict:** a **refactor** of who moves money. The money statements themselves are unchanged.

## Discovery (consult log)

- **Owner decisions, verbatim (Indy, Nov 05, 2026):**
  > "and my theory is drop the message to a queue on billing so its disconnected from the agentsfleet system"

  > "the reciever sink gets it and does what is needed a new billing api server or the agentsfleet will need to get the data from that db (for now) i presume its best to keep it an abstracted ingterface"

  > "so i want a large scale refactor here" · "so it can scale"

  > "I agree on the next steps." · "yes write it and i prefer to start the implementation and send the PR." · "no doc spec"

- **Datastore decision:** Postgres stays the system of record for money. PostHog moved its analytics warehouse off DuckDB on Nov 02, 2026, five months after rebuilding on it, and that workload was analytics, not billing (https://posthog.com/blog/moving-off-duckdb).
- **Greptile findings on #807 (the old draft):**
  - "Test forbids required balance reads": that test is gone with the old §2.
  - "Renewal can count spend twice": the renewal keeps the reserve, which reads the hold and the balance under one wallet lock.
  - "Correction can restore spent credit": there is no balance correction in this design.
  - "Backlog exceeds promised recovery times": the relay drains a full batch at once, and the bounds are stated per pass.
- **Consults:** `main` at `66059667b`. The request-path money calls are `afd_fleet/src/lease/renew/mod.rs` (settle after `extend`), `report/mod.rs` (settle and release after `commit_report`), and `afd_authoring/src/lease/report/mod.rs` (`pay`). The finished-holds statements waited `GRACE` for every status.
