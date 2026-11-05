<!--
SPEC AUTHORING RULES (the one comment that survives):
- The spec standard (spec.md) holds every rule. Each tpl: comment names the
  rules that govern its section; delete it once the section is filled.
- Body order is the implementing agent's read order.
- No time or effort estimates, owners or implementation dates (SPEC-ESTIMATE).
-->

# M240_002: Billing settles off the lease path, and no renew or report waits on a wallet lock

**Prototype:** v2.0.0

**Milestone:** M240

**Workstream:** 002

**Date:** Nov 05, 2026

**Status:** PENDING

**Priority:** P1, every renewal takes a tenant's wallet row lock and every report takes it twice, so one slow billing statement stalls every lease of that tenant

**Categories:** API, DOCS, INFRA, OBS

**Batch:** B2, after M240_001 lands the one-statement claim and the issue transaction this spec adds the first money hold to

**Depends on:** M240_001 (the issue transaction and its pinned lock order); M233_002 (the delivery span the settlement lag is read beside)

**Provenance:** LLM-drafted (Claude Fable 5.1, Nov 05, 2026) from the billing settlement review on `main` at `65372f505` and the uncommitted `Queued` biller prototype in the sibling worktree `agentsfleet-soak`; revised (Claude Opus 5.5, Nov 05, 2026) after two zero-context EGM reviews; owner decisions quoted in Discovery

**Canonical architecture:** `docs/architecture/billing_and_provider_keys.md` §3 "One debit point", §5 "The balance gate", §11 "One billing lifecycle behind a `Biller` trait"

---

## Overview

**Goal (testable):** `test_report_and_renew_complete_while_the_wallet_row_is_locked`: a test transaction holds `billing.wallets` locked for one tenant, and that tenant's lease still renews and reports within its normal deadline. Once the lock is released, its usage lines are settled by journal entries within the queue's retry budget, and `balance_nanos` equals the sum of the tenant's journal entries.

**Problem:** Money moves under one row lock per tenant. A report settles under that lock, then releases its hold under it again, and the release re-runs the settle (`afd_billing/src/ledger/settle.rs`). A renewal reserves under it, then settles under it. A billing statement that waits past the 2 s `LOCK_TIMEOUT` counts as a billing outage (`afd_billing/src/ledger/mod.rs`). A tenant with many fleets therefore serialises every lease it runs on one row, and a person's chat waits behind another lease's settlement. Debits are in-place updates with no row per movement, so a balance cannot be traced to the lines that moved it.

**Owner's constraint, verbatim:** "a billing function shouldnt impact the next lease and a few seconds of delay to claim/settle isnt an issue for an end user."

**Solution summary:** Four changes.
- The request path stops awaiting money. Renew and report already write their usage lines inside their own transaction; after commit they hand the settlement to a `Queued` biller and return.
- A supervised worker settles the queued work in seconds.
- Every settlement appends a row to a new journal, `billing.wallet_entries`, and moves `balance_nanos` by the same amount in the same transaction. The 30 s billing sweep stays the durable backstop and also reconciles each touched balance against its journal.
- The first money hold moves inside the issue transaction under a per-tenant advisory lock. That lock serialises reservations without waiting on the worker's wallet updates.

The mid-run stop stays. A renewal applies today's reserve test, reading without a lock and writing no hold, and refuses with `UZ-RUN-012` when the payer cannot cover the next slice.

**Design flow:**

```text
ISSUE txn (M240_001)   … lease insert → reserve_on: pg_advisory_xact_lock(hashtextextended('billing.wallet:'||tenant,0))
                       → balance − held_by_others ≥ floor ? upsert hold : rollback, refuse BALANCE_EXHAUSTED

RENEW                  gate (no lock, no write): balance − held_by_others ≥ uncaptured spend + floor, else UZ-RUN-012
                       → extend txn: METER_LEASE + usage lines → COMMIT → Queued.settle(settlement)   (returns at once)

REPORT                 commit txn: METER_LEASE + lines + terminal + fences + checkpoint + slot → COMMIT
                       → Queued.release(work, why)                                                   (returns at once)

WORKER (supervised)    settle:  log_lines (idempotent) → stamp unsettled lines RETURNING charged
                                → INSERT wallet_entries(debit, −Σ, last_seq) → UPDATE wallets balance += −Σ
                                → capture Σ on the hold                          one txn, bound timeouts
                       release: the same settle over every unsettled line → hold released_at
                       retries a refused job, then warns billing_queue_deferred and leaves it to the sweep

SWEEP (every 30 s)     lines older than GRACE (120 s) → settle as above; finished holds → release
                       touched tenants: balance_nanos vs Σ entries → correct and count any drift
```

**Bounds this spec states:** with the worker alive, a handed-off settlement lands within its retry budget, about 6 s. With the worker lost, the sweep settles it within `GRACE` plus one interval, 150 s. A payer can overdraw by at most the other leases' usage beyond their holds that is still waiting to settle. That waiting time is seconds with the worker alive and 150 s without it. Each lease's own unsettled usage is always counted.

**Risks this spec carries:** the queue lives in memory and loses its jobs when the process dies, and the sweep is the recovery. Fleet holds stop going through the `Biller` trait at issue, because a trait call cannot join the issue transaction. The trait keeps `reserve` for the authoring plane, which holds no transaction. The balance a person sees can lag the charges list by the bounds above.

## PR Intent & comprehension handshake

- **PR title (eventual):** Settle billing on a queue with a journal, and hold money inside issue
- **Intent (one sentence):** A person's chat and a runner's report never wait on another lease's billing, every charge is a journal row the balance reconciles to, and a payer who runs out is still stopped within one renewal.
- **Authoring handshake:** Indy accepted "all four moves" and asked that billing "be moved to a queue based approach, since a billing function shouldnt impact the next lease and a few seconds of delay to claim/settle isnt an issue for an end user", pointing at the queued biller in the soak worktree. ASSUMPTIONS I'M MAKING:
  1. The `Biller` trait stays, and `Queued` is one more decorator over `Observed`.
  2. The prototype's `reserve_detached` is not adopted, because the hold moves inside issue.
  3. The mid-run stop (`UZ-RUN-012`) is kept, read without a lock.
  4. No public field changes. The billing endpoint keeps serving `balance_nanos`, now moved by journal entries, and `updated_at` remains its freshness.
  5. Signup's starter credit becomes the journal's first `grant` entry.
- **Implementer handshake:** recorded in the Pull Request Session Notes at PLAN, before any edit (SPEC-HANDSHAKE).

## Implementing agent — read these first

1. `rustd/crates/afd_billing/src/ledger/settle.rs` and `ledger/mod.rs` — `take`, `release`, `LedgerBiller::{settle, release, reserve}` and `bound`: the statements §1 rewrites. `take`'s `settled_at IS NULL` guard is the idempotency authority this spec keeps.
2. `rustd/crates/afd_fleet/src/lease/money.rs`, `report/mod.rs`, `renew/mod.rs` and `coverage.rs` — `settle_usage`, `release_usage` and `hold_issued`; the report's two post-commit calls; the renewal's `gate_renewal` and its reserve.
3. `/Users/kishore/Projects/agentsfleet-soak/rustd/crates/afd_billing/src/biller/queued.rs` — the prototype §2 brings in. Its module comment carries the durability argument behind Invariant 3.
4. `rustd/crates/agentsfleetd/src/sweepers/mod.rs` and `sweepers/billing.rs` — `Supervisor::spawn` is the only way a daemon task starts (Invariant C2), and the 30 s billing sweep is the backstop.
5. `docs/architecture/billing_and_provider_keys.md` §3, §5 and §11 — "one debit point" and "a balance crossing zero mid-run ends the run", both kept.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `docs/v2/pending/M240_002_P1_API_DOCS_INFRA_OBS_BILLING_SETTLES_OFF_THE_LEASE_PATH.md` | CREATE then MOVE to `active/`, then `done/` | this spec |
| `schema/2220_billing_wallet_entries.sql` | CREATE | §1: the immutable journal, its refuse-update trigger, indexes, grants, and one `opening` entry per existing wallet |
| `rustd/crates/afd_db/src/migration.rs` | EDIT | registers the slot |
| `rustd/crates/afd_core/src/money.rs` | EDIT | §1: the entry kind spellings, beside `NANOS_PER_USD`, where both `afd_billing` and `afd_tenant` can read them |
| `rustd/crates/afd_billing/src/nanos.rs` | EDIT | §1: `Nanos::checked_sub` |
| `rustd/crates/afd_billing/src/ledger/sql/insert_debit_entry.sql`, `move_wallet.sql`, `reconcile_wallets.sql` | CREATE | §1, §4: the debit row, the wallet move it pairs with, the drift check |
| `rustd/crates/afd_billing/src/ledger/sql/lock_wallet.sql`, `debit_wallet.sql` | DELETE | §1, §3: the row lock and the in-place debit go |
| `rustd/crates/afd_billing/src/ledger/sql/settle_lines.sql`, `capture_hold.sql`, `select_held_by_others.sql`, `select_hold.sql`, `upsert_hold.sql`, `select_work_tenant.sql` | EDIT | §1, §3: settle returns `seq` and `charged`; reserve's reads run after the advisory lock |
| `rustd/crates/afd_billing/src/ledger/mod.rs`, `ledger/settle.rs`, `ledger/reserve.rs`, `ledger/sweep.rs`, `ledger/sql.rs` | EDIT | §1, §3, §4 |
| `rustd/crates/afd_billing/src/ledger/entries.rs` | CREATE | §1: the debit-and-move pair, kept out of `settle.rs`, which is near the length cap |
| `rustd/crates/afd_billing/src/biller/queued.rs`, `biller/queued_tests.rs` | CREATE | §2: the decorator and its worker, from the prototype |
| `rustd/crates/afd_billing/src/biller/mod.rs` | EDIT | §2: `pub use Queued, Worker`; the `settle` doc states what a deferred `Receipt` means |
| `rustd/crates/afd_billing/Cargo.toml` | EDIT | §2: `tokio` `sync` and `time`, `tokio-util` for the cancellation token |
| `rustd/crates/afd_tenant/src/sql/signup/insert_wallet.sql`, `rustd/crates/afd_tenant/src/signup/mod.rs` | EDIT | §1: the starter credit writes its `grant` entry in the wallet's own statement |
| `rustd/crates/agentsfleetd/src/billing.rs`, `agentsfleetd/src/plane/mod.rs`, `agentsfleetd/src/sweepers/mod.rs` | EDIT | §2: the request-path biller is `Queued`; its worker is spawned as `BILLING_QUEUE`; sweepers keep the direct biller |
| `rustd/crates/agentsfleetd/tests/serve.rs` | EDIT | §2: `test_boot_to_ready_on_compose`'s supervised-task inventory gains `BILLING_QUEUE` |
| `rustd/crates/afd_fleet/src/lease/money.rs`, `report/mod.rs`, `report/steps.rs` | EDIT | §2: the report hands off `release` only; `hold_issued` goes (§3) |
| `rustd/crates/afd_fleet/src/lease/coverage.rs`, `renew/mod.rs` | EDIT | §2: the renewal gate reads instead of reserving; `settle_usage` stays as the hand-off |
| `rustd/crates/afd_fleet/src/lease/issue.rs` | EDIT | §3: `reserve_on` inside the issue transaction |
| `rustd/crates/afd_authoring/src/lease/claim/mod.rs`, `afd_authoring/src/model/budget.rs` | EDIT | §3: their reserves take the advisory lock through `LedgerBiller::reserve`; no call-site change beyond tests |
| `rustd/crates/afd_observability/src/producers/billing.rs`, `afd_observability/src/metrics/declared/billing.rs`, `docs/metrics.census.tsv` | EDIT | §2, §4: queue and drift families |
| `rustd/crates/agentsfleetd/tests/integration_billing_journal.rs` | CREATE | §1 to §4: the journal, the queue, the reconciliation, the kill |
| `rustd/crates/agentsfleetd/tests/integration_runner_e2e/money_gate.rs`, `agentsfleetd/tests/integration_sweepers.rs`, `afd_billing/src/ledger/tests.rs`, `afd_fleet/src/lease/admit/tests.rs` | EDIT | tests follow the mechanisms |
| `docs/architecture/billing_and_provider_keys.md`, `docs/architecture/concurrency.md`, `docs/architecture/scaling.md` | EDIT | §5 |

## Applicable Rules

- **`domains/sql.md`** — SQL-SLOT, SQL-STAMP, SQL-IMMUTABLE, STS, SQL-DEFAULT, SQL-ONEFACT, SQL-UPSERT, SQL-RETRYKEY, IDMP, TXN, SQL-TIMEOUT, SQL-SWEEPCLOCK, SQL-PLAN, SQL-REPO
- **`languages/rust.md`** — ERR-RS, RS-FROM, RS-NEWTYPE, RS-BIND, FN-RS, RS-RACE
- **`code.md`** — NDC, NLR, NLG, CODE-SEAM, OWN, FLL, CODE-MSID
- **`tests.md`** — TEST-REAL, TEST-CONTEND, TEST-ACKLOSS, TEST-KILL, TEST-INJECT, TEST-COUNTER, TEST-DELTA, TEST-LANE
- **`domains/logging.md`** — LOG-EVENT, LOG-FIELDS, LOG-HOT, OBS
- **`docs.md`** — DOC-02, DOC-05, DOC-10, DOC-14b on the architecture pages

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| `make lint-all` | yes | no new `#[expect]`; retry delays come from `backon`, as the prototype's sibling `afd_dragonfly` does |
| supervised-task inventory in `test_boot_to_ready_on_compose` | yes | `BILLING_QUEUE` added with the spawn |
| metrics census check | yes, three families | rows added to `docs/metrics.census.tsv` in the same commit |
| `docs.language` | yes | short sentences, DOC-05 plain words |
| gitleaks pre-commit | no secrets in scope | `gitleaks detect --redact --no-banner` before each commit |
| File & Function Length (file ≤350 lines, function ≤50, method ≤70) | yes: `ledger/settle.rs`, `biller/mod.rs` near the cap | the pair goes to `ledger/entries.rs`; the queue is its own file |

## Prior-Art / Reference Implementations

- **Reference:** `/Users/kishore/Projects/agentsfleet-soak/rustd/crates/afd_billing/src/biller/queued.rs` is the decorator to bring in. It wraps an inner `Arc<dyn Biller>` over a bounded channel of 4 096 jobs shaped `Settle(Settlement)` and `Release(WorkKey, Release)`. A full or closed channel runs the job inline, and a job refused after three retries leaves a `billing_queue_deferred` warn. This spec diverges in three ways:
  - `reserve_detached` is dropped.
  - The worker becomes a `Worker` value the daemon spawns under `Supervisor::spawn`, rather than a `runtime.spawn` inside the crate.
  - The worker stops on its cancellation token.
- **Reference:** `rustd/crates/afd_billing/src/biller/observed.rs` and `outage/mod.rs` show the decorator shape. `biller/conformance.rs` is the behaviour suite the direct billers pass.
- **Reference:** `schema/2040_billing_usage_lines.sql` is the append-only table `wallet_entries` mirrors: UUIDv7 id, no `DEFAULT`, `api_runtime` grants.
- **Reference:** `rustd/crates/afd_tenant/src/cli_credential/mod.rs` already takes `pg_advisory_xact_lock(hashtextextended(…, 0))`, the 64-bit form §3 uses.
- **Divergence from `billing_and_provider_keys.md` §11:** "a queue or a third party can never join our transaction" stays true. Request transactions write lines, the issue transaction writes a hold through the ledger's own in-transaction function, and money moves happen in the worker's and the sweep's own transactions.

## Sections (implementation slices)

### §1 — Every settlement writes a journal row and moves the balance with it

Prerequisite: M240_001 merged.

Slot 2220 creates `billing.wallet_entries` as an immutable table (SQL-IMMUTABLE): no `updated_at`, no `UPDATE` grant, and a trigger that refuses `UPDATE`. Its columns are a UUIDv7 id, `tenant_id`, `kind`, `work_type`, `work_id`, `last_seq`, a signed `nanos` and `created_at`. The last three of the work columns are null for `opening` and `grant` entries. The kinds `opening`, `grant` and `debit` are constants in `afd_core::money`, bound by the writer, never a `CHECK` literal (STS). Grants and openings are positive and debits are negative. A unique index on `(work_type, work_id, last_seq)` covers debits.

The slot backfills one `opening` entry per wallet equal to its `balance_nanos`. The journal starts at the migration and does not reconstruct older history, so `SUM(nanos) = balance_nanos` holds from the first row. An index on `(tenant_id, created_at) INCLUDE (nanos)` serves the reconciliation.

`LedgerBiller::settle` keeps its shape and loses the wallet lock. It runs in one transaction under `bound`'s timeouts, so `55P03` still reads as an outage. The steps:
1. `log_lines` inserts the settlement's lines with `ON CONFLICT DO NOTHING`, as today.
2. `settle_lines` stamps `settled_at` on the work's unsettled lines and returns their `seq` and `charged_nanos`.
3. If it returned rows, `insert_debit_entry` writes one `debit` of minus their sum with `last_seq` set to their highest `seq`.
4. `move_wallet` adds that amount to `balance_nanos`. It sets `exhausted_at = COALESCE(exhausted_at, now())` when the result is at or below zero, and clears it when the result is positive.
5. `capture_hold` adds the sum to the hold's `captured_nanos`.

`release` runs the same pair over every unsettled line, then sets `released_at`. Two settles of one work serialise on the line row locks, and the second stamps nothing, so it writes nothing. A run settled in several batches, by the sweep and later the report, writes one debit per batch.

A payer with no wallet row has its lines stamped and its debit written. `move_wallet` affects no row and the existing `billing_settled_without_wallet` warn fires.

Signup's `insert_wallet.sql` (`afd_tenant`) writes the wallet and its starter `grant` entry in one statement. **Implementation default:** the balance moves with each entry instead of being re-summed, because a re-sum grows with a tenant's history. §4's reconciliation catches drift instead.

- **Dimension 1.1** — the journal starts reconciled → Test `test_opening_entries_equal_wallet_balances` (integration): seed three wallets and run the migration. Each wallet's `SUM(nanos)` equals its `balance_nanos`, and a second run of the backfill statement inserts nothing.
- **Dimension 1.2** — a partial settle is never lost → Test `test_partial_settles_write_one_debit_per_batch` (integration): log lines 0 to 4, settle, then log lines 5 to 9 and settle again. There are two debits with `last_seq` 4 and 9, their sum equals all ten lines, and the balance moved by both. The false pass this rejects is a second batch dropped silently.
- **Dimension 1.3** — a repeated settle writes nothing → Test `test_hundred_settles_of_one_work_debit_once` (integration, TEST-CONTEND): 100 settles of one work released from a barrier produce one debit and stamp each line once.
- **Dimension 1.4** — signup is a grant → Test `test_signup_writes_its_starter_grant_entry` (integration): a new tenant has one `grant` entry equal to `STARTER_CREDIT_NANOS`, and its balance equals it.
- **Dimension 1.5** — exhaustion follows the move → Test `test_move_sets_and_clears_exhausted_at` (integration): a debit to zero sets `exhausted_at`, a second debit keeps the first instant, and a grant entry that makes the balance positive clears it.
- **Dimension 1.6** — the journal refuses edits → Test `test_wallet_entries_refuse_update` (integration): an `UPDATE` as the table owner raises the trigger's error.

### §2 — Renew and report hand money to a supervised queue

Prerequisite: §1.

`Queued` joins `afd_billing::biller`. `Queued::new(inner, depth) -> (Queued, Worker)`, and `Worker::run(self, token)` is the future the daemon spawns as `BILLING_QUEUE` through `Supervisor::spawn`. On cancellation, the worker settles the jobs already in the channel until the supervisor's shutdown deadline. It leaves the rest to the sweep.

`settle` and `release` enqueue and return at once. A queued `settle` answers `Receipt::default()`, meaning this call settled nothing yet; the trait's doc states it. `reserve` and `balance` pass straight through. Retries follow the prototype, then `billing_queue_deferred`. Tests drive a `Worker` they never run, or run one over a refusing inner biller (TEST-INJECT).

`agentsfleetd/src/billing.rs` builds the request-path biller as `Queued(Observed(WithOutagePolicy(LedgerBiller)))`. The sweepers keep the unqueued biller. The conformance suite keeps running on the direct billers. `Queued` gets its own tests, because a deferred receipt cannot meet the suite's immediate `charged` checks.

The report keeps its commit transaction unchanged. After commit it calls `release_usage` only, because `release` settles every unsettled line first. Its `settle_usage` call goes. `Reconciled.charged` is still read from the settlement's lines, so the report's answer does not change.

The renewal keeps `extend` (meter and lines) and still calls `settle_usage` after commit, which now enqueues. `gate_renewal` keeps its order, before `extend`, and keeps the fleet budget check (`UZ-RUN-015`). Its `hold_covers` stops reserving and reads instead, on one connection and without a lock. It reads the price-book floor and the lease's `charged_so_far`, as today. It also reads the wallet balance and `held_by_others`, which excludes this lease's hold, and this lease's captured amount.

It refuses `UZ-RUN-012` when `balance − held_by_others` is below `charged_so_far − captured + floor`. That is the uncaptured remainder of the ceiling today's reserve would place. A refusal extends nothing and writes nothing, and the runner maps it to `RenewalTerminate` (`afr_supervisor/src/renew/mod.rs`). A tenant with no wallet row is admitted, and an unreadable balance fails open, both as today.

- **Dimension 2.1** — the Goal → Test `test_report_and_renew_complete_while_the_wallet_row_is_locked` (integration): a test transaction holds `SELECT … FOR UPDATE` on the tenant's wallet, and a renewal and a report of that tenant's lease both answer 200 within `CALL_TIMEOUT`. The `pg_stat_statements` delta for those two requests contains no statement that touches `billing.wallets`. After the blocker commits, the work's debit exists within 7 s. The false pass this rejects is a renewal that succeeds only because it waited.
- **Dimension 2.2** — the report hands off once → Test `test_report_enqueues_one_release_and_no_settle` (unit, `afd_fleet` with the recording biller): a report records one `release` for its `WorkKey` and no `settle`.
- **Dimension 2.3** — the mid-run stop is kept → Test `test_renewal_refuses_when_the_payer_cannot_cover_the_next_slice` (integration): a balance below the uncaptured spend plus one floor makes the renewal answer `UZ-RUN-012`. The lease deadline is unchanged and the runner's outcome is `RenewalTerminate`.
- **Dimension 2.4** — the stop does not count the lease's own hold twice → Test `test_renewal_admits_a_balance_that_covers_exactly_one_slice` (integration): with a balance equal to the uncaptured spend plus one floor, and this lease's own hold open, the renewal renews.
- **Dimension 2.5** — a killed worker loses no money → Test `test_sigkill_with_queued_jobs_is_repaired_by_the_sweep` (integration, TEST-KILL): the daemon is killed after a report commits and before its worker runs. After restart and one sweep past `GRACE`, the lines are settled, the hold is released, and the balance equals the journal.
- **Dimension 2.6** — a refused job is left to the sweep → Test `test_refused_job_warns_and_the_sweep_settles_it` (integration, TEST-INJECT): with an inner biller refusing every call, the queue warns `billing_queue_deferred` once, and the sweep's direct biller settles the work.
- **Dimension 2.7** — a full queue runs inline → Test `test_full_queue_settles_inline` (unit): with depth one and an unrun worker, the second `settle` runs on the caller and its debit exists when the call returns.
- **Dimension 2.8** — the worker is supervised → Test `test_boot_to_ready_on_compose` (integration, existing): the inventory names `BILLING_QUEUE`, and a cancelled supervisor ends it.

### §3 — The first hold is placed inside issue under an advisory lock

Prerequisite: §2. `afd_billing::ledger::reserve_on(executor, ask)` is the in-transaction reserve, beside `ledger::balance`. The steps:
1. It takes `pg_advisory_xact_lock(hashtextextended('billing.wallet:' || tenant_id::text, 0))`. The prefix keeps it apart from the repository's other advisory keys.
2. It reads the balance and `held_by_others`.
3. It upserts the hold, or answers `Short`.

`issue.rs` calls it as the last statement before commit, after the lease row and the choice record, so the issue order is host → slot → lease → tenant advisory lock.

Before calling it, issue runs `bound`, so billing's `LOCK_TIMEOUT` and `STATEMENT_TIMEOUT` apply to that final statement. The `Reservation` is `WorkKey::FleetLease(lease_id)`, the payer's tenant and workspace, and `ceiling = floor`. `issue::Billed` gains `floor`, which the money gate already computed. `reserve_on` answers `Held` or `Short` only; there is no outage policy inside a transaction.

A `Short` rolls the transaction back, releases the slot through `let_go`, and refuses the event with `BALANCE_EXHAUSTED` through M240_001's one-statement ending. A datastore error aborts issue, and the poll answers the error with the slot released, as M240_001 §4 states for every pass fault. `money.rs`'s post-commit `hold_issued` goes. `LedgerBiller::reserve`, which the authoring claim and each authoring model call use, runs `reserve_on` in its own bounded transaction, so `lock_wallet.sql` has no caller and goes.

The advisory lock serialises reservations of one tenant and never waits on the worker's `move_wallet`. A settle lowers the balance and the settled work's open hold by the same amount, so `balance − held_by_others` is unchanged by any other work's settle. The exception is a work whose capture passes its ceiling, which the stated overdraw bound covers.

- **Dimension 3.1** — the hold is in the lease's transaction → Test `test_issue_places_hold_before_commit` (integration): an issued lease's hold is visible in the same snapshot as the lease. An issue failed at the choice record leaves neither.
- **Dimension 3.2** — a short payer gets no lease → Test `test_short_payer_gets_no_lease_and_no_hold` (integration): with a wallet below the floor, the event is refused `BALANCE_EXHAUSTED`, there is no lease and no hold, and the slot is free.
- **Dimension 3.3** — reservations admit within the balance → Test `test_concurrent_reserves_of_one_tenant_admit_within_balance` (integration, TEST-CONTEND): 100 issues released together against a balance that covers 40 floors produce exactly 40 holds and 60 refusals.
- **Dimension 3.4** — issue never waits on settlement → Test `test_issue_completes_while_the_worker_moves_the_wallet` (integration): with a test transaction holding the wallet row as the worker's `move_wallet` would, an issue for that tenant commits within `CALL_TIMEOUT`.
- **Dimension 3.5** — the lock order holds → Test `test_issue_and_renewal_never_deadlock` (integration, RS-RACE): 50 issues and 50 renewals of one payer released together produce zero `40P01` errors.

### §4 — The sweep is the backstop and checks every balance it touched

Prerequisite: §3. `ledger/sweep.rs` keeps settling lines older than `GRACE` and releasing finished holds, through §1's pair, comparing deadlines with `statement_timestamp()` (SQL-SWEEPCLOCK).

After each pass, `reconcile_wallets.sql` compares `balance_nanos` with `SUM(nanos)` for every tenant the pass wrote an entry for. The journal is the authority. A drifted wallet is set to its sum in the same statement, and each correction counts `billing_wallet_drift` and logs an error with the tenant and the difference.

- **Dimension 4.1** — drift is corrected and counted → Test `test_reconcile_corrects_a_drifted_wallet` (integration, TEST-INJECT): a test write moves `balance_nanos` without an entry, and the next pass restores the sum and counts one drift.
- **Dimension 4.2** — the check stays indexed at scale → Test `test_reconcile_plan_uses_the_tenant_index` (integration, SQL-PLAN): 200 000 entries over 50 tenants, after `ANALYZE`. `EXPLAIN (ANALYZE, BUFFERS)` shows the `(tenant_id, created_at)` index and no sequential scan.

### §5 — The pages say what the code does

Prerequisite: §1 to §4.
- `billing_and_provider_keys.md` §3 names the journal as the debit record. §5 describes the lock-free renewal stop with its bound. §11 adds `Queued` and states that fleet holds use `reserve_on`.
- `concurrency.md` drops the wallet from "runtime renewal takes runner → lease/affinity → wallet" and "reports take lease → wallet". It records the issue order ending in the tenant advisory lock, and adds `BILLING_QUEUE` to the thread map.
- `scaling.md` §1 restates the report and renew cost.

- **Dimension 5.1** — the pages pass the gate → Test `orly gate verify` (manual, evidence in the Pull Request Session Notes): `docs.language` reports zero findings on the changed pages.

## Interfaces

- `afd_billing::biller::{Queued, Worker}`: `Queued::new(inner: Arc<dyn Biller>, depth: usize) -> (Queued, Worker)`; `Worker::run(self, token: CancellationToken)`. `Biller` signatures are unchanged. A queued `settle` answers `Receipt::default()`.
- `afd_billing::ledger::reserve_on(executor, &Reservation) -> Result<Hold>`, taking `impl sqlx::PgExecutor<'_>` first.
- `billing.wallet_entries(id, tenant_id, kind, work_type, work_id, last_seq, nanos, created_at)`: immutable, with kinds from `afd_core::money`.
- No public HTTP, CLI or UI change. `UZ-RUN-012`, `UZ-RUN-015` and `BALANCE_EXHAUSTED` keep their meaning.

## Failure Modes

| Mode | Kind | Cause | Handling (response and what the caller observes) | Test |
|------|------|-------|--------------------------------------------------|------|
| Worker dies with jobs | failure | SIGKILL after a report commits | lines stay unsettled; the sweep settles within 150 s | `test_sigkill_with_queued_jobs_is_repaired_by_the_sweep` |
| Ledger refuses repeatedly | failure | Postgres fault in the worker | retries, one `billing_queue_deferred`, the sweep settles | `test_refused_job_warns_and_the_sweep_settles_it` |
| Queue full | capacity | more jobs than the depth | the caller settles inline | `test_full_queue_settles_inline` |
| Partial settles | race | the sweep settles part of a running lease | one debit per batch, keyed by `last_seq` | `test_partial_settles_write_one_debit_per_batch` |
| Duplicate settle | replay | a report retried after a lost 200 | line stamps decide; the second writes nothing | `test_hundred_settles_of_one_work_debit_once` |
| Mid-run exhaustion | capacity | the payer cannot cover the next slice | `UZ-RUN-012`, `RenewalTerminate` | `test_renewal_refuses_when_the_payer_cannot_cover_the_next_slice` |
| Own hold counted twice | failure | the gate subtracting this lease's hold | excluded through `held_by_others` | `test_renewal_admits_a_balance_that_covers_exactly_one_slice` |
| Reserve race | race | concurrent issues of one tenant | the advisory lock admits within the balance | `test_concurrent_reserves_of_one_tenant_admit_within_balance` |
| Wallet locked by settlement | capacity | the worker or sweep moving the wallet | renew, report and issue do not wait | `test_report_and_renew_complete_while_the_wallet_row_is_locked`, `test_issue_completes_while_the_worker_moves_the_wallet` |
| Balance drift | failure | a wallet write without its entry | corrected to the journal sum and counted | `test_reconcile_corrects_a_drifted_wallet` |
| Deadlock | race | issue and renewal of one payer | the pinned order; zero `40P01` | `test_issue_and_renewal_never_deadlock` |

## Invariants

1. One debit per settled batch. The `settled_at IS NULL` stamp in `settle_lines.sql` returns each line once, and `UNIQUE (work_type, work_id, last_seq)` refuses a second debit for the same batch (Dimensions 1.2, 1.3).
2. Renew and report never touch `billing.wallets`. Their transactions run only `afd_fleet` statements, and money reaches the wallet through `Queued` (Dimension 2.1, through `pg_stat_statements`).
3. Nothing the queue drops is lost. Every job's effect is derivable from rows committed before it was queued, unsettled lines and open holds, and the sweep derives it (Dimensions 2.5, 2.6).
4. Every write of `balance_nanos` writes its matching entry in the same transaction: `move_wallet` after `insert_debit_entry`, signup's one statement, and the backfill. The sweep's reconciliation corrects and counts any drift (Dimensions 1.1, 4.1).
5. A reservation never admits beyond `balance − held_by_others`, as read under its tenant's advisory lock (Dimension 3.3).
6. A run whose payer cannot cover the next slice ends at that renewal. The gate runs before `extend` and refuses without writing (Dimension 2.3).

## Metrics & Observability

| Metric / event | Audience | Fires when | Properties allowed | Privacy guard | Test |
|----------------|----------|------------|--------------------|---------------|------|
| `billing_queue_jobs{kind,outcome}` counter | operator | a job is queued, settled, retried, run inline or deferred | kind, outcome | no identifiers | `test_full_queue_settles_inline` |
| `billing_queue_depth` gauge | operator | every enqueue and dequeue | depth | none | `test_full_queue_settles_inline` |
| `billing_queue_deferred` log event | operator | the retries are spent | `work_type`, `work_id`, `error_code` | ids only | `test_refused_job_warns_and_the_sweep_settles_it` |
| `billing_wallet_drift` counter and error log | operator | the reconciliation corrects a wallet | `tenant_id`, `drift_nanos` | ids only | `test_reconcile_corrects_a_drifted_wallet` |
| `renew_no_credits` (existing) | operator | a renewal refuses `UZ-RUN-012` | fleet and lease ids | ids only | `test_renewal_refuses_when_the_payer_cannot_cover_the_next_slice` |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Renew and report do not wait on a locked wallet (§2) | `make test-integration-rustd` | `test_report_and_renew_complete_while_the_wallet_row_is_locked ... ok` | P0 | |
| R2 | A partial settle is never lost (§1) | `make test-integration-rustd` | `test_partial_settles_write_one_debit_per_batch ... ok` | P0 | |
| R3 | A killed worker loses no money (§2) | `make test-integration-rustd` | `test_sigkill_with_queued_jobs_is_repaired_by_the_sweep ... ok` | P0 | |
| R4 | The mid-run stop is kept (§2) | `make test-integration-rustd` | `test_renewal_refuses_when_the_payer_cannot_cover_the_next_slice ... ok` | P0 | |
| R5 | The hold is placed inside issue (§3) | `make test-integration-rustd` | `test_issue_places_hold_before_commit ... ok` | P0 | |
| R6 | Reservations admit within the balance (§3) | `make test-integration-rustd` | `test_concurrent_reserves_of_one_tenant_admit_within_balance ... ok` | P0 | |
| R7 | Drift is corrected and counted (§4) | `make test-integration-rustd` | `test_reconcile_corrects_a_drifted_wallet ... ok` | P1 | |
| R8 | Each lease commits fewer transactions (Overview) | `make bench-lease PROFILE=rig` then `make bench-compare LANE=lease PROFILE=rig` | `drain_commits_per_lease` at least 2 below the baseline recorded in the Session Notes at PLAN | P1 | |
| S1 | Declared commands green | `make lint-all` · `make test-unit-all` · `make test-integration-rustd` · `make check-version` | exit 0 each | P0 | |
| S2 | No secrets | `gitleaks detect --redact --no-banner` | exit 0 | P0 | |

## Dead Code Sweep

| Deleted file or symbol | Verify | Expected |
|------------------------|--------|----------|
| `lock_wallet.sql`, `LOCK_WALLET`, `lock_wallet_if_any` | `git grep -n -w -e LOCK_WALLET -e lock_wallet_if_any -- rustd` | no output |
| `debit_wallet.sql`, `DEBIT_WALLET` | `git grep -n -w DEBIT_WALLET -- rustd` | no output |
| `hold_issued` | `git grep -n -w hold_issued -- rustd` | no output |

## Out of Scope

- A grant path beyond signup's starter credit: `billing_and_provider_keys.md` §13 holds it.
- A user-facing "balance as of" display. The owner accepted seconds of lag, and the billing endpoint already serves the wallet's `updated_at`.
- Changing `GRACE`, the sweep interval or `LEASE_TTL`. The bounds above are stated against today's 120 s and 30 s.
- Removing `Release::Expired { runtime }`, `Account.workspace` or the posture fallbacks: unrelated to the lock, not planned here.
- A durable job table for billing. The unsettled lines already are one, and a second queue needs its own sweep.

---

## Product Clarity (authoring record)

1. **Successful user moment** — A person's chat on a fleet whose tenant runs forty other fleets gets its lease and its report as fast as on a quiet tenant.
2. **Preserved user behaviour** — Charges history reads the same lines. `is_exhausted` keeps its meaning. A run that cannot pay is still stopped at its next renewal, and a payer who cannot cover the floor still gets no lease.
3. **Optimal-way check** — The shortest path writes only lines on the request path and moves money elsewhere. The gap is the renewal gate, which must still read a number to stop an exhausted run; it reads without a lock.
4. **Rebuild-vs-iterate** — Iterate: `usage_lines`, `holds`, the sweep and the decorators stay. One table, one decorator and one in-transaction reserve are added.
5. **What we build** — the journal, the paired move, the supervised queue, the lock-free renewal gate, the in-issue hold, and the sweep's reconciliation.
6. **What we do NOT build** — a billing service, a durable billing job table, a balance-freshness surface, a price cache.
7. **Fit with existing features** — Compounds with M240_001's single issue transaction. It must not destabilise the authoring plane's per-call reserve, which keeps `Biller::reserve` and is proven by Dimension 3.3's lock, and its report, which gets the queue through the trait.
8. **Surface order** — API only. No UI or CLI change.
9. **Dashboard restraint** — No new panel. The queue families feed the existing operator dashboard only after a week of samples.
10. **Confused-user next step** — An operator who sees a balance that disagrees with charges reads `billing_wallet_drift` and `billing_queue_deferred` in the logs. Each names the tenant or the work.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** §1 journal and pair, §2 queue and request path, §3 hold in issue, §4 backstop, §5 pages. Each Section leaves the daemon shippable. §2's Goal test needs §1's lock-free settle, which is why §1 comes first.
- **Alternatives considered:**
  - The queue alone, with no journal. A zero-context reviewer preferred it, and it would solve the lock. It was rejected because the owner accepted all four moves, and because a balance with no row per movement cannot be reconciled.
  - A journal re-summed on every settle. Rejected: the cost grows with history. The paired move plus a reconciliation costs one statement each.
  - The wallet row lock kept inside `reserve_on`. Rejected: issue would wait behind the worker's wallet move, which the owner's constraint forbids.
  - A Dragonfly stream as the queue. Rejected: money facts live in Postgres, and the sweep reads them there.
- **Patch-vs-refactor verdict:** a **refactor** because the debit statement, the wallet lock and two post-commit transactions are replaced by a journal, a queue and an in-transaction hold.

## Discovery (consult log)

- **Consults** — The billing settlement review on `main` `65372f505`, Nov 05, 2026, cited each claim to a file and line. A report settles, then releases, and the release re-runs the settle (`ledger/settle.rs`). A renewal reserves and settles under the wallet lock (`coverage.rs`, `renew/mod.rs`). `LOCK_TIMEOUT` is 2 s and counts as an outage (`ledger/mod.rs`). `GRACE` is 120 s (`ledger/sweep.rs`). `select_balance.sql` already reads the balance and open holds without a lock. `held_by_others` excludes the caller's own hold. `UZ-RUN-012` maps to `RenewalTerminate` in the runner. The soak worktree's `queued.rs` was read in full.
- **Owner decisions** —
  > Indy (Nov 05, 2026): "Yes all four moves, the other tree had a similar one, i requested if this could be moved to a queue based approach, since a billing function shouldnt impact the next lease and a few seconds of delay to claim/settle isnt an issue for an end user. Can you check the other tree on thsi as well." — context: the queue, the accepted lag, the journal and the prototype.
  > Indy (Nov 05, 2026): "ensure you review the spec for implementation completeness and that when its handed over to fable would be able to do its job." — context: the review bar.
- **Model reviews** —
  - EGM brainstorm (Claude Fable 5.1): one of six lenses completed before a usage limit; its three billing findings are applied.
  - EGM zoom-out review of the first draft (Claude Fable 5.1): 13 findings, 11 applied, 2 declined. Applied: the paired move over a re-sum; `held_by_others` reused; the 64-bit advisory key; the prototype's job shape; the supervised worker; bounds stated against `GRACE`; the balance-freshness surface dropped; the unrelated dead-code rows dropped; `Receipt.queued` dropped; `Nanos` placed in `afd_billing`; the grep gate replaced by a lock test. Declined: cutting the journal, because the owner accepted it; keeping the wallet row lock in reserve, because issue would wait on the worker.
  - EGM comprehension review of the first draft (Claude Fable 5.1): 5 blockers, 15 majors, 8 nits. All blockers applied: one debit per batch keyed by `last_seq`; the gate order with a refusal that writes nothing; `settled_through` dropped; the opening backfill without reconstructed history; signup's grant in `afd_tenant`. Majors applied, with the advisory key fully stated and the Section order fixed; nits applied or made moot.
- **Open blockers** — none.
- **Deferrals and transfers** — none.
