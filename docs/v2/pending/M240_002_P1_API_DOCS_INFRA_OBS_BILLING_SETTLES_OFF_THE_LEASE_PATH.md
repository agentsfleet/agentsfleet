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

**Priority:** P1, every renewal takes a tenant's wallet row lock twice and every report twice more, so one slow billing statement stalls every lease of that tenant, and a mid-run stop still depends on a locked reserve

**Categories:** API, DOCS, INFRA, OBS

**Batch:** B2, after M240_001 §5 lands the in-transaction `reserve_on` seam this spec changes

**Depends on:** M240_001 (`reserve_on` inside issue and the pinned lock order); M233_002 (the delivery span the settlement lag is read beside)

**Provenance:** LLM-drafted (Claude Fable 5.1, Nov 05, 2026) from the billing settlement review on `main` at `65372f505`, the uncommitted `Queued` biller prototype in the sibling worktree `agentsfleet-soak` (`rustd/crates/afd_billing/src/biller/queued.rs`), and owner decisions quoted in Discovery

**Canonical architecture:** `docs/architecture/billing_and_provider_keys.md` §3 "One debit point", §5 "The balance gate", §11 "One billing lifecycle behind a `Biller` trait"

---

## Overview

**Goal (testable):** `test_report_and_renew_take_no_wallet_lock_and_the_journal_reconciles`: with one tenant running 50 leases, every report and renewal commits with zero `billing.wallets` row locks, every usage line is settled by a journal entry within 30 s of its report, and `balance_nanos` equals the sum of the tenant's journal entries at every sweep boundary.

**Problem:** Money moves under one row lock per tenant. A report takes it twice, once to settle lines and once to release the hold, and the second transaction re-runs the first's settle (`afd_billing/src/ledger/settle.rs`). A renewal takes it twice more, to reserve and to settle. A statement that waits past the 2 s `lock_timeout` is treated as a billing outage (`afd_billing/src/ledger/mod.rs`). A tenant with many fleets therefore serialises every lease it runs on its wallet row, and a person's chat waits behind someone else's settlement. Debits are in-place updates with no row per movement, so a balance cannot be reconciled to its lines, and a payer with no wallet row settles to a warn-log.

**Owner's constraint, verbatim:** "a billing function shouldnt impact the next lease and a few seconds of delay to claim/settle isnt an issue for an end user." The one money decision that may stop a run, whether it starts and whether it continues, keeps a bounded answer; everything else leaves the request path.

**Solution summary:** `billing.usage_lines` is already the record of what was used. This spec adds `billing.wallet_entries`, the record of what moved: one row per grant, per settlement debit and per hold capture. The report and renew transactions keep writing lines and stop touching the wallet. A `Queued` biller worker settles lines into journal entries and releases holds within seconds; the 30 s billing sweep settles whatever the worker dropped and recomputes `balance_nanos` from the journal. Reserve, once per lease at issue, takes a per-tenant advisory lock instead of the wallet row. The mid-run stop is kept: a renewal reads the rollup less open holds without a lock and refuses with `UZ-RUN-012` when it cannot cover the next floor; the overshoot is bounded by one sweep interval of one tenant's unsettled lines.

**Design flow:**

```text
ISSUE (M240_001 §5)    pg_advisory_xact_lock(hashtext(tenant)) → available = balance_nanos − Σ open holds
                       → INSERT billing.holds (ceiling = floor) or Short → refuse

RENEW (one txn)        METER_LEASE → INSERT usage_lines (ON CONFLICT DO NOTHING)
                       read available (no lock) → Short ⇒ UZ-RUN-012 ; never reserves, never settles

REPORT (one txn)       METER_LEASE → INSERT usage_lines → terminal row → fences → checkpoint → slot release
                       → COMMIT → hand WorkKey to the Queued biller (no await on money)

QUEUED WORKER          settle(work): INSERT wallet_entries(kind=debit, Σ unsettled lines) + stamp settled_at
 (in-process, retries  release(work): INSERT wallet_entries(kind=hold_capture) + released_at on the hold
  200 ms, 1 s, 5 s)    both idempotent on (work_type, work_id, seq) and on the hold row
                       then UPDATE billing.wallets SET balance_nanos = Σ entries, exhausted_at (one statement)

SWEEP (30 s)           settle unsettled lines older than GRACE, release finished holds, recompute every
                       touched tenant's balance_nanos from wallet_entries; the durable backstop
```

**Risks this spec carries:** the balance a person sees lags the lines by up to one sweep interval plus the worker's retries, so the billing page says when it was settled through; a tenant can overdraw by at most one sweep interval of its own unsettled lines, which the renewal stop bounds; the 32-bit advisory key can make two tenants wait on each other, never admit one wrongly; the in-memory queue loses its jobs when the process dies, and the sweep is the recovery.

**How the targets are measured:** `make bench-lease PROFILE=rig` reports `drain_statements_per_lease` and `drain_commits_per_lease`; the integration tests read the `pg_stat_statements` and `pg_locks` deltas. Pass lines: renew ≤ 3 statements and 1 commit; report ≤ 12 statements and 1 commit; zero `billing.wallets` row locks on either.

## PR Intent & comprehension handshake

- **PR title (eventual):** Settle billing through a journal off the lease path and drop the wallet lock from renew and report
- **Intent (one sentence):** A person's chat and a runner's report never wait on another lease's billing, every charge is a row that reconciles to the balance, and a payer who runs out is still stopped within one renewal.
- **Authoring handshake:** Indy accepted "all four moves" and asked that billing "be moved to a queue based approach, since a billing function shouldnt impact the next lease and a few seconds of delay to claim/settle isnt an issue for an end user", and pointed at the queued biller in the soak worktree. ASSUMPTIONS I'M MAKING: 1. The `Biller` trait stays; `Queued` is one more decorator beside `Observed` and `WithOutagePolicy`, and the authoring plane's four call sites get the queue for free. 2. The prototype's `reserve_detached` is not adopted: M240_001 places the hold inside issue, so a detached reserve has no caller. 3. The mid-run stop (`UZ-RUN-012`) is kept and reads the rollup; the owner's "few seconds of delay" covers a bounded overshoot, not an unbounded one. 4. `GET /v1/tenants/me/billing` keeps `balance_nanos` and gains `settled_through`, additive. 5. No grant path exists in `agentsfleetd` today (`billing_and_provider_keys.md` §2), so the journal's `grant` kind is written by the signup starter grant and the backfill only.
- **Implementer handshake:** recorded in the Pull Request Session Notes at PLAN, before any edit (SPEC-HANDSHAKE).

## Implementing agent — read these first

1. `rustd/crates/afd_billing/src/ledger/settle.rs` and `ledger/mod.rs` — `take`, `release`, `LedgerBiller::settle` and the `bound` timeouts: the statements §2 rewrites and the lock §3 removes.
2. `rustd/crates/afd_fleet/src/lease/commit.rs` and `report/mod.rs` — the report transaction (`claim_and_settle`, `record_settled`) and the two post-commit biller calls §2 turns into one hand-off.
3. `rustd/crates/afd_fleet/src/lease/coverage.rs` and `renew/mod.rs` — `gate_renewal` reserves and `renew` settles; §2 makes renew read-only for money.
4. `/Users/kishore/Projects/agentsfleet-soak/rustd/crates/afd_billing/src/biller/queued.rs` — the prototype §3 brings into this repository; read its module comment for the durability argument this spec turns into Invariant 3.
5. `rustd/crates/afd_billing/src/ledger/sweep.rs`, `rustd/crates/afd_fleet/src/lease/billing_sweep.rs`, `rustd/crates/agentsfleetd/src/sweepers/billing.rs` — the 30 s sweep that becomes the one writer of `balance_nanos`.
6. `docs/architecture/billing_and_provider_keys.md` §3, §5 and §11 — the invariants "one debit point" and "a balance crossing zero mid-run ends the run" this spec keeps, and the `Biller` lifecycle it extends.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `docs/v2/pending/M240_002_P1_API_DOCS_INFRA_OBS_BILLING_SETTLES_OFF_THE_LEASE_PATH.md` | CREATE then MOVE to `docs/v2/active/`, then `docs/v2/done/` | this spec |
| `schema/2220_billing_wallet_entries.sql` | CREATE | §1: the journal table, its kinds as a `TEXT` column constrained in code, its indexes and grants, and the backfill of one `grant` entry per existing wallet balance |
| `schema/2230_billing_wallets_settled_through.sql` | CREATE | §1: `settled_through` on `billing.wallets`, the id of the last journal entry the rollup includes |
| `rustd/crates/afd_db/src/migration.rs` | EDIT | registers the two slots |
| `rustd/crates/afd_core/src/money.rs` | EDIT | §1: the entry kind constants and `Nanos::checked_sub` |
| `rustd/crates/afd_billing/src/ledger/sql/insert_wallet_entry.sql`, `rollup_wallet.sql`, `select_available.sql` | CREATE | §1, §2: the journal insert, the rollup statement, the lock-free available read |
| `rustd/crates/afd_billing/src/ledger/sql/debit_wallet.sql`, `lock_wallet.sql`, `settle_lines.sql`, `capture_hold.sql`, `release_hold.sql`, `select_balance.sql` | EDIT or DELETE | §2, §3: debit and lock go; settle stamps lines and writes the entry; capture and release write entries; balance reads the rollup |
| `rustd/crates/afd_billing/src/ledger/mod.rs`, `ledger/settle.rs`, `ledger/reserve.rs`, `ledger/lines.rs`, `ledger/sweep.rs` | EDIT | §2, §3, §4: entries, advisory reserve, no wallet lock, rollup in the sweep |
| `rustd/crates/afd_billing/src/biller/mod.rs`, `rustd/crates/afd_billing/src/biller/queued.rs`, `biller/queued_tests.rs` | EDIT, CREATE | §3: the `Queued` decorator and its tests, from the prototype |
| `rustd/crates/afd_billing/src/biller/conformance.rs` | EDIT | §3: the conformance suite gains the queued biller's eventual-settlement check |
| `rustd/crates/afd_billing/Cargo.toml` | EDIT | §3: `tokio` `sync`, `time`, `rt` features for the worker |
| `rustd/crates/afd_billing/src/wallet.rs`, `rustd/crates/afd_billing/src/sql/select_wallet.sql` | EDIT | §5: the wallet read returns `settled_through` |
| `rustd/crates/agentsfleetd/src/billing.rs`, `rustd/crates/agentsfleetd/src/plane/mod.rs`, `rustd/crates/agentsfleetd/src/sweepers/billing.rs` | EDIT | §3, §4: the request-path biller is queued; the sweepers keep the direct one and run the rollup |
| `rustd/crates/afd_fleet/src/lease/report/mod.rs`, `report/steps.rs`, `commit.rs`, `settle.rs`, `usage/mod.rs` | EDIT | §2: one hand-off after commit; `Reconciled` merges into `Reported`; `settle_usage` and `release_usage` go |
| `rustd/crates/afd_fleet/src/lease/renew/mod.rs`, `coverage.rs` | EDIT | §2: renew meters and appends, reads `available` without a lock, never reserves or settles |
| `rustd/crates/afd_fleet/src/lease/billing_sweep.rs`, `rustd/crates/afd_authoring/src/billing_sweep/mod.rs` | EDIT | §4: both sweeps hand released work to the rollup |
| `rustd/crates/afd_fleet/src/lease/admit/gates.rs`, `rustd/crates/afd_authoring/src/model/budget.rs`, `rustd/crates/afd_authoring/src/lease/claim/mod.rs` | EDIT | §3: the gates read `available` through the rollup; the authoring claim's reserve takes the advisory lock |
| `rustd/crates/afd_api_wire/src/tenant.rs`, `rustd/crates/afd_api_tenant/src/handler/tenant/billing.rs` | EDIT | §5: `settled_through` on the billing response |
| `public/openapi.json` | EDIT | regenerated from `rustd/` with `--no-banner` |
| `ui/packages/app/lib/api/tenant_billing.ts`, `ui/packages/app/components/layout/BalanceLink.tsx`, `ui/packages/app/app/(dashboard)/settings/billing/components/BillingReportView.tsx` | EDIT | §5: the balance shows when it was settled through |
| `cli/src/commands/billing/show.ts` | EDIT | §5: `billing show` prints `settled_through` |
| `rustd/crates/agentsfleetd/tests/integration_billing_journal.rs` | CREATE | §2 to §4: lock-free report and renew, journal reconciliation, worker loss and sweep recovery, mid-run stop |
| `rustd/crates/agentsfleetd/tests/integration_runner_e2e/money_gate.rs`, `rustd/crates/agentsfleetd/tests/integration_sweepers.rs`, `rustd/crates/afd_billing/src/ledger/tests.rs`, `rustd/crates/afd_fleet/src/lease/coverage_tests.rs` | EDIT | tests follow the mechanisms |
| `docs/architecture/billing_and_provider_keys.md`, `docs/architecture/billing_reporting.md`, `docs/architecture/concurrency.md`, `docs/architecture/scaling.md` | EDIT | the journal, the queue, the rollup, the removed lock rows and the statement budget |
| `~/Projects/docs` branch `chore/m240-billing-journal-changelog` | EDIT (other repository) | the billing response field and the balance lag a reader can see; committed after this Pull Request merges |

## Applicable Rules

- **`domains/sql.md`** — SQL-SLOT, SQL-STAMP, SQL-COUNTER, SQL-ONEFACT, SQL-UPSERT, SQL-RETRYKEY, SQL-IMMUTABLE, SQL-DEFAULT, SQL-STATUS, IDMP, TXN, SQL-TIMEOUT, SQL-SWEEPCLOCK, SQL-PLAN
- **`languages/rust.md`** — ERR-RS, RS-FROM, RS-NEWTYPE, RS-BIND, FN-RS, RS-RACE
- **`code.md`** — NDC, NLR, NLG, CODE-SEAM, OWN, FLL, CODE-MSID
- **`tests.md`** — TEST-REAL, TEST-CONTEND, TEST-ACKLOSS, TEST-KILL, TEST-INJECT, TEST-COUNTER, TEST-DELTA, TEST-LANE
- **`domains/logging.md`** — LOG-EVENT, LOG-FIELDS, LOG-HOT, OBS
- **`domains/http-surface.md`** — HTTP-OPENAPI, HTTP-ANNOTATE, HTTP-BREAK, HTTP-STABILITY
- **`languages/typescript.md`** — the rules for `ui/packages/app` and `cli` files touched in §5
- **`docs.md`, `changelog.md`** — DOC-02, DOC-05, DOC-10, DOC-14b; CL-ACT and CL-RATE for the money-visible change

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| `make lint-all` | yes | no new `#[expect]`; the worker uses `backon` for its delays, not a hand-rolled loop |
| `test_coverage_gate_rust_source`, OpenAPI check | yes, one wire field | annotation and doc comment at source; `public/openapi.json` regenerated in the commit |
| TypeScript coverage gates (`make test-unit-all`) | yes, three app files and one CLI file | component tests for the settled-through copy; CLI handler test |
| `docs.language` | yes | short sentences; DOC-05 plain words; CL-ACT states the lag in exact numbers |
| gitleaks pre-commit | no secrets in scope | `gitleaks detect --redact --no-banner` before every commit |
| File & Function Length | yes: `ledger/settle.rs` and `biller/mod.rs` are near the cap | entries move to `ledger/entries.rs`; the queue is its own file |

## Prior-Art / Reference Implementations

- **Reference:** `/Users/kishore/Projects/agentsfleet-soak/rustd/crates/afd_billing/src/biller/queued.rs` — the decorator to bring in: `Arc<dyn Biller>` inner, bounded `mpsc`, inline fallback when the queue is full or absent, three retries then a `billing_queue_deferred` warn. Divergence: `reserve_detached` is dropped (no caller after M240_001 §5); `settle` and `release` return a `Receipt` that says `queued`, never a default that looks settled.
- **Reference:** `rustd/crates/afd_billing/src/biller/observed.rs` and `outage/mod.rs` — the decorator shape and the conformance suite every `Biller` passes.
- **Reference:** `schema/2040_billing_usage_lines.sql` — the append-only table to mirror for `wallet_entries`: UUIDv7 id, no `DEFAULT`, partial indexes, `api_runtime` grants.
- **Reference:** `rustd/crates/afd_billing/src/ledger/sweep.rs` — the oldest-first batched sweep the rollup joins.
- **Divergence from `billing_and_provider_keys.md` §11:** "a queue or a third party can never join our transaction" stays true: the request transaction writes lines and nothing else of money; the queue and the sweep write entries in their own transactions.

## Sections (implementation slices)

### §1 — The journal exists and the balance is a rollup of it

Prerequisite: M240_001 merged. Slot 2220 creates `billing.wallet_entries`: UUIDv7 id, `tenant_id`, `kind`, `work_type` and `work_id` (nullable for a grant), `nanos` (signed: grants positive, debits negative, hold captures zero and informational), `created_at` and `updated_at` set by the writer (SQL-STAMP). Kinds are constants in `afd_core::money` (`grant`, `debit`, `hold_capture`), never a `CHECK` literal (SQL-DEFAULT). The idempotency key for a debit is `(work_type, work_id, kind)` unique: one debit per lease, carrying the sum of the lines it settled, so a repeated settle writes nothing. Slot 2230 adds `settled_through UUID` to `billing.wallets`, the last entry id the rollup has summed. The backfill writes one `grant` entry per wallet equal to its current `balance_nanos` plus the sum of its settled lines, so the journal reconciles from the first sweep. **Implementation default:** `nanos` is signed and the rollup is `SUM(nanos)` because one formula is easier to prove than a kind-by-kind one; a hold capture carries zero so it never enters the sum and exists only to say when a hold closed.

- **Dimension 1.1** — the rollup reconciles → Test `test_rollup_equals_journal_sum_after_backfill` (integration): seed three wallets with balances and settled lines, run both migrations and one rollup; each `balance_nanos` equals `SUM(nanos)` and `settled_through` names the last entry.
- **Dimension 1.2** — a debit is written once per lease → Test `test_debit_entry_is_unique_per_work` (integration, TEST-CONTEND): 100 settles of one lease released from a barrier; one `debit` entry, all lines stamped once.
- **Dimension 1.3** — a grant moves the balance → Test `test_signup_grant_is_a_journal_entry` (integration): signup writes a `grant` entry and the rollup; `GET /v1/tenants/me/billing` shows it after the next rollup.

### §2 — Renew and report write lines and never touch the wallet

Prerequisite: §1. The report transaction (`commit.rs`) keeps `claim_and_settle`, the line insert and `record_settled`, and after commit hands the lease's `WorkKey` and its `Release` to the biller once; `settle_usage` and `release_usage` and the `Reconciled` type go (NLR). `renew` keeps `extend` (meter and line insert) and drops `gate_renewal`'s reserve and the post-extend settle; instead it reads `available = balance_nanos − Σ open holds` through `select_available.sql` without a lock, inside the same transaction as the meter, and refuses `UZ-RUN-012` when `available` cannot cover the lease's next floor. The `Short` check therefore lags the lines by at most one sweep interval plus the worker's retries, which is the bounded overshoot the owner accepted. **Implementation default:** the renewal reads `available` after the meter in the same transaction, because the meter already holds the lease row and a second acquire would cost what this spec removes.

- **Dimension 2.1** — the Goal → Test `test_report_and_renew_take_no_wallet_lock_and_the_journal_reconciles` (integration, TEST-COUNTER): 50 leases of one tenant report and renew; `pg_locks` shows no `billing.wallets` tuple lock from any of them; every line is settled within 30 s; the rollup equals the journal at the sweep boundary.
- **Dimension 2.2** — a report is one commit → Test `test_report_commits_once_and_hands_off_money` (integration): a report's transaction count is one and its biller hand-off is recorded by the recording biller with the lease's `WorkKey`.
- **Dimension 2.3** — the mid-run stop is kept → Test `test_renewal_refuses_when_available_cannot_cover_the_floor` (integration): a tenant whose rollup minus holds is below the floor; the next renewal answers `UZ-RUN-012` and the runner ends the run with `BudgetBreach`.
- **Dimension 2.4** — the overshoot is bounded → Test `test_overshoot_is_at_most_one_sweep_of_unsettled_lines` (integration, TEST-INJECT): a stopped queue worker; a tenant at the floor runs one lease for two renewals; the debt after the sweep equals that lease's lines and no further lease is admitted.
- **Dimension 2.5** — a repeated report charges nothing → Test `test_repeated_report_settles_nothing_twice` (integration, TEST-ACKLOSS): the report's 200 is dropped and re-sent; one debit entry, `AlreadySettled` on the second.

### §3 — Money moves on a queue, and reserve takes an advisory lock

Prerequisite: §2. `Queued` joins `afd_billing::biller` from the prototype: one worker task per process on a bounded channel, jobs `Settle(WorkKey)` and `Release(WorkKey, Release)`, retries at 200 ms, 1 s and 5 s through `backon`, then a `billing_queue_deferred` warn that names the work. A full or closed channel runs the job inline. `agentsfleetd/src/billing.rs` wraps the request-path biller as `Queued(Observed(WithOutagePolicy(LedgerBiller)))`; the sweepers keep the direct biller. `LedgerBiller::settle` becomes: stamp the work's unsettled lines, insert the `debit` entry with their sum, then run the rollup for that tenant, all in one transaction and with no wallet row lock; `release` inserts the `hold_capture` entry and sets `released_at`. `reserve_on` (M240_001 §5) replaces `lock_wallet.sql` with `pg_advisory_xact_lock(hashtext(tenant_id))` before its `available` read and hold insert; the authoring claim's reserve takes the same lock. `lock_wallet.sql` and `debit_wallet.sql` go. The `Biller` conformance suite gains a check that a queued settle is visible within the worker's retry budget.

- **Dimension 3.1** — the queue settles within seconds → Test `test_queued_settle_lands_within_the_retry_budget` (integration): a report; the debit entry exists within 6 s without a sweep.
- **Dimension 3.2** — a dead process loses no money → Test `test_sigkill_with_queued_jobs_is_repaired_by_the_sweep` (integration, TEST-KILL): kill the daemon after a report commits and before the worker runs; after restart and one sweep the lines are settled, the hold released and the rollup equal to the journal.
- **Dimension 3.3** — a full queue runs inline → Test `test_full_queue_settles_inline` (unit): a channel of depth one with its worker blocked; the second settle runs on the caller and returns its receipt.
- **Dimension 3.4** — two reservations cannot both pass → Test `test_concurrent_reserves_of_one_tenant_admit_within_balance` (integration, TEST-CONTEND): 100 reserves released together against a balance that covers 40 floors; exactly 40 holds, 60 `Short`.
- **Dimension 3.5** — an advisory collision waits, never admits wrongly → Test `test_hashtext_collision_serialises_two_tenants` (unit): two tenant ids chosen to collide on `hashtext`; both reserves complete in order, each against its own balance.
- **Dimension 3.6** — the authoring plane gets the queue → Test `test_authoring_report_settles_through_the_queue` (integration): an authoring lease reports; its lines settle to a `debit` entry with `work_type = authoring_attempt` and its hold releases.
- **Dimension 3.7** — the wallet lock is gone → Test `test_no_statement_locks_the_wallet_row` (unit, `afd_billing` sql registry): no statement file under `afd_billing/src/ledger/sql` contains `FOR UPDATE` on `billing.wallets`.

### §4 — The sweep is the one durable writer of the balance

Prerequisite: §3. `ledger/sweep.rs` keeps settling unsettled lines older than `GRACE` and releasing finished holds, both now through the entry writes of §3, and after each batch recomputes `balance_nanos` and `settled_through` for every tenant it touched from the journal, in one statement per tenant (`rollup_wallet.sql`), comparing deadlines with `statement_timestamp()` (SQL-SWEEPCLOCK). `exhausted_at` is set by the rollup when the sum crosses zero and cleared by a grant, so the one writer of the wallet's two facts is the rollup. A payer with no wallet row gets its lines settled and its debit entry written, and the sweep logs `settled_without_wallet` once per tenant per cycle instead of per line.

- **Dimension 4.1** — the sweep repairs a dropped job → Test `test_sweep_settles_lines_the_queue_gave_up_on` (integration, TEST-INJECT): the ledger refuses three times; the queue warns `billing_queue_deferred`; the next sweep settles the lines and the rollup matches.
- **Dimension 4.2** — exhaustion is a rollup fact → Test `test_exhausted_at_is_set_by_the_rollup_and_cleared_by_a_grant` (integration): a tenant crosses zero; `exhausted_at` is set after the rollup, `is_exhausted` is true on the billing response, and a grant entry clears it on the next rollup.
- **Dimension 4.3** — a wallet-less payer is a row, not a log → Test `test_wallet_less_payer_settles_to_a_debit_entry` (integration): lines for a tenant with no wallet row settle; one debit entry exists; the warn fires once.

### §5 — The person sees a balance with a time on it

Prerequisite: §4. `GET /v1/tenants/me/billing` and `agentsfleet billing show` gain `settled_through` (RFC 3339 instant of the last rollup), beside `balance_nanos`. The header `BalanceLink` shows the balance with "settled through HH:MM" when `settled_through` is older than 30 s, and nothing extra when it is fresh. The billing page's settled-charges copy says that charges appear at once and the balance follows within a minute. `is_exhausted` keeps its meaning and is read from the rollup's `exhausted_at`.

- **Dimension 5.1** — the field is served → Test `test_billing_response_carries_settled_through` (integration): the response has `settled_through` equal to the wallet's last rollup instant.
- **Dimension 5.2** — the lag is shown → Test `BalanceLink shows settled-through when stale` (unit, vitest): `settled_through` 90 s old renders the copy; 10 s old renders none.
- **Dimension 5.3** — the CLI prints it → Test `billing show prints settled_through` (unit, bun test, handler tier): the handler returns the field and the renderer prints it in both JSON and table modes.

### §6 — The pages say what the code does

Prerequisite: §1 to §5. `billing_and_provider_keys.md` §3 describes the journal as the debit record and the rollup as the balance; §5 describes the lock-free gate and the renewal stop with its bound; §11 adds `Queued` to the decorator list. `concurrency.md` drops the wallet from the runtime renewal and report rows, keeps it only under issue's advisory lock, and adds the queue worker to the thread map. `scaling.md` §1 restates the report and renew statement budget.

- **Dimension 6.1** — the pages pass the gate → Test `orly gate verify` (manual, evidence in the Pull Request Session Notes): `docs.language` reports zero findings on the changed pages.

## Interfaces

- `GET /v1/tenants/me/billing` response gains `settled_through: string | null` (RFC 3339), `x-stability: stable`; `null` only for a wallet the rollup has never run on.
- `afd_billing::biller::Biller`: unchanged signatures; `Receipt` gains `queued: bool` so a caller can tell a deferred answer from a settled one. `Queued::start(inner: Arc<dyn Biller>) -> Queued`.
- `afd_billing::ledger::{reserve_on, available_on, settle_on, release_on, rollup_on}`: executor-taking, tenant or `WorkKey` first after the executor.
- `billing.wallet_entries(id, tenant_id, kind, work_type, work_id, nanos, created_at, updated_at)`; `billing.wallets.settled_through`.
- CLI `agentsfleet billing show` prints `settled_through` in table and JSON output.

## Failure Modes

| Mode | Kind | Cause | Handling (response and what the caller observes) | Test |
|------|------|-------|--------------------------------------------------|------|
| Queue worker dies with jobs | failure | SIGKILL after a report commits | lines stay unsettled; the sweep settles and rolls up within 30 s | `test_sigkill_with_queued_jobs_is_repaired_by_the_sweep` |
| Ledger refuses repeatedly | failure | Postgres fault during settle | three retries then `billing_queue_deferred`; the sweep repairs | `test_sweep_settles_lines_the_queue_gave_up_on` |
| Queue full | capacity | more than `QUEUE_DEPTH` jobs waiting | the caller settles inline and returns a settled receipt | `test_full_queue_settles_inline` |
| Overshoot | capacity | lines unsettled when a renewal reads `available` | bounded to one sweep interval of the tenant's lines; next renewal refuses | `test_overshoot_is_at_most_one_sweep_of_unsettled_lines` |
| Mid-run exhaustion | capacity | rollup minus holds below the floor | `UZ-RUN-012`; the runner ends the run | `test_renewal_refuses_when_available_cannot_cover_the_floor` |
| Duplicate settle | replay | a report retried after a lost 200 | debit unique per work; `AlreadySettled`; nothing charged twice | `test_repeated_report_settles_nothing_twice` |
| Reserve race | race | two reservations of one tenant | advisory lock serialises; admits within balance | `test_concurrent_reserves_of_one_tenant_admit_within_balance` |
| Advisory collision | race | two tenants hash alike | they wait on each other; each admits against its own balance | `test_hashtext_collision_serialises_two_tenants` |
| Wallet-less payer | failure | tenant erased or never provisioned | lines settled, debit entry written, one warn per cycle | `test_wallet_less_payer_settles_to_a_debit_entry` |
| Stale balance shown | failure | rollup lags the lines | `settled_through` on the response; the header shows the time past 30 s | `BalanceLink shows settled-through when stale` |
| Rollup drift | failure | an entry written outside the sweep's batch | the rollup always recomputes from `SUM(nanos)`, never increments | `test_rollup_equals_journal_sum_after_backfill` |

## Invariants

1. One debit per lease — `UNIQUE (work_type, work_id, kind)` on `billing.wallet_entries`; Dimension 1.2.
2. The request path writes no money but lines — the report and renew transactions hold a connection type with no `wallet_entries` or `wallets` statement reachable; Dimension 2.1 and Dimension 3.7.
3. Nothing the queue drops is lost — every job's effect is derivable from rows that committed before the job was queued (unsettled lines, open holds), and the sweep derives it; Dimension 3.2 and 4.1.
4. `balance_nanos` equals `SUM(wallet_entries.nanos)` at every `settled_through` — the rollup statement is the only writer of `balance_nanos`, and it recomputes rather than increments; Dimension 1.1.
5. A reservation never admits beyond `available` as of its lock — `pg_advisory_xact_lock` before the read and the insert in one transaction; Dimension 3.4.
6. A run whose payer cannot cover the next floor ends within one renewal — the `available` read inside `extend`'s transaction; Dimension 2.3.

## Metrics & Observability

| Metric / event | Audience | Fires when | Properties allowed | Privacy guard | Test |
|----------------|----------|------------|--------------------|---------------|------|
| `billing_queue_depth` gauge, `billing_queue_jobs{kind,outcome}` counter | operator | a job is queued, settled, retried or deferred | kind, outcome, depth | no identifiers | `test_queued_settle_lands_within_the_retry_budget` |
| `billing_queue_deferred` log event | operator | three retries fail | `work_type`, `work_id`, `error_code` | ids only | `test_sweep_settles_lines_the_queue_gave_up_on` |
| `billing_rollup_lag_seconds` histogram | operator | a rollup runs | lag from the oldest unsettled line | no identifiers | `test_rollup_equals_journal_sum_after_backfill` |
| `settled_without_wallet` log event | operator | once per tenant per sweep cycle | `tenant_id`, `charged_nanos` | ids only | `test_wallet_less_payer_settles_to_a_debit_entry` |
| `renew_budget_breach` (existing) | operator | a renewal refuses `UZ-RUN-012` | lease and fleet ids, `available_nanos`, `floor_nanos` | ids only | `test_renewal_refuses_when_available_cannot_cover_the_floor` |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | No wallet lock on report or renew, journal reconciles (§2) | `make test-integration-rustd` | `test_report_and_renew_take_no_wallet_lock_and_the_journal_reconciles ... ok` | P0 | |
| R2 | A killed process loses no money (§3) | `make test-integration-rustd` | `test_sigkill_with_queued_jobs_is_repaired_by_the_sweep ... ok` | P0 | |
| R3 | The mid-run stop is kept (§2) | `make test-integration-rustd` | `test_renewal_refuses_when_available_cannot_cover_the_floor ... ok` | P0 | |
| R4 | One debit per lease under contention (§1) | `make test-integration-rustd` | `test_debit_entry_is_unique_per_work ... ok` | P0 | |
| R5 | Reservations admit within balance (§3) | `make test-integration-rustd` | `test_concurrent_reserves_of_one_tenant_admit_within_balance ... ok` | P0 | |
| R6 | No statement locks the wallet row (§3) | `git grep -n "FOR UPDATE" -- rustd/crates/afd_billing/src/ledger/sql` | no output | P0 | |
| R7 | The balance shows its time (§5) | `make test-unit-all` | `BalanceLink shows settled-through when stale ... ✓` | P1 | |
| R8 | Report and renew meet the statement budget (Overview) | `make bench-lease PROFILE=rig` then `make bench-compare LANE=lease PROFILE=rig` | `drain_statements_per_lease` ≤ 12 and `drain_commits_per_lease` ≤ 1 in the compare output | P1 | |
| R9 | The OpenAPI document matches the server (Interfaces) | `scripts/openapi-check.sh` | exit 0 | P0 | |
| S1 | Declared commands green | `make lint-all` · `make test-unit-all` · `make test-integration-rustd` · `make check-version` | exit 0 each | P0 | |
| S2 | No secrets | `gitleaks detect --redact --no-banner` | exit 0 | P0 | |

## Dead Code Sweep

| Deleted file or symbol | Verify | Expected |
|------------------------|--------|----------|
| `rustd/crates/afd_billing/src/ledger/sql/lock_wallet.sql`, `lock_wallet_if_any` | `git grep -n -w -e lock_wallet -e lock_wallet_if_any -- rustd` | no output |
| `rustd/crates/afd_billing/src/ledger/sql/debit_wallet.sql`, `DEBIT_WALLET` | `git grep -n -w DEBIT_WALLET -- rustd` | no output |
| `settle_usage`, `release_usage`, `Reconciled`, `gate_renewal` | `git grep -n -w -e settle_usage -e release_usage -e Reconciled -e gate_renewal -- rustd/crates/afd_fleet` | no output |
| `Release::Expired { runtime }` unused payload, `Reservation.payer.workspace` unused field | `git grep -n -w -e "Expired {" -- rustd/crates/afd_billing/src/biller` | no output |
| `Posture::parse(..).unwrap_or(Posture::Platform)` fallbacks (four sites) | `git grep -n "unwrap_or(Posture::Platform)" -- rustd` | no output |

## Out of Scope

- A grant path in `agentsfleetd` beyond the signup starter grant: `billing_and_provider_keys.md` §13 holds it.
- Per-fleet or per-workspace budgets beyond the existing ceiling: unchanged.
- A third-party biller behind the trait: the trait stays; no new implementation.
- Removing `afd_billing/src/biller/conformance.rs` from `src/` (306 lines of feature-gated test code): a follow-up under the tests standard.
- Changing `LEASE_TTL`, `RENEWAL_TICK` or the sweep interval: unchanged; the overshoot bound is stated against the existing 30 s.

---

## Product Clarity (authoring record)

1. **Successful user moment** — A person's chat on a fleet whose tenant runs forty other fleets gets its lease and its report as fast as on a quiet tenant, and the billing page shows the charge at once and the balance within a minute, with the time it was settled through.
2. **Preserved user behaviour** — Charges history reads the same lines; `is_exhausted` keeps its meaning; a run that cannot pay is still stopped at its next renewal.
3. **Optimal-way check** — The shortest path is lines-only on the request path and a journal for everything else; the gap is the rollup, which exists because the gate must read one number without summing a tenant's history.
4. **Rebuild-vs-iterate** — Iterate: `usage_lines`, `holds`, the sweep and the `Biller` decorators all stay; one table and one decorator are added.
5. **What we build** — the journal, the rollup, a lock-free renewal stop, the `Queued` decorator, the advisory reserve, `settled_through` on the response and in the header and CLI.
6. **What we do NOT build** — a durable job table for billing (the lines are the durable queue), a separate billing service, a cache of prices, a per-tenant worker.
7. **Fit with existing features** — compounds with M240_001's one-transaction issue; it must not destabilise the authoring plane's budget and report, which get the queue through the same trait and are tested in Dimension 3.6.
8. **Surface order** — API first, then the dashboard header and the CLI in the same Pull Request, because the lag is user-visible the day the queue ships.
9. **Dashboard restraint** — no new billing panel; the one new signal is the settled-through time, shown only when it is older than 30 s.
10. **Confused-user next step** — a person who sees a balance that does not match their charges reads "settled through HH:MM" in the header and the billing page's one-sentence explanation; an operator runs `agentsfleet billing show` and reads the same field.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** §1 journal, §2 request path, §3 queue and reserve, §4 sweep rollup, §5 user surface, §6 pages. §2 is shippable alone because the sweep already settles unsettled lines; §3 shortens the lag; §4 makes the balance exact.
- **Alternatives considered:** keep the wallet lock and only drop the duplicate settle (rejected: still one lock per tenant on every report); a durable `billing.jobs` table drained by the worker (rejected: the unsettled lines already are that table, and a second queue needs its own sweep); a Dragonfly stream as the queue (rejected: money facts live in Postgres, and the sweep must read them there anyway); a per-request synchronous journal append with no lock (considered: correct and lock-free, but it keeps money on the request path, which the owner asked to end).
- **Patch-vs-refactor verdict:** a **refactor** because the debit statement, the wallet lock and two post-commit transactions are replaced by a journal, a queue and a rollup, and the removed code is listed above.

## Discovery (consult log)

- **Consults** — Billing settlement review on `main` `65372f505`, Nov 05, 2026, every claim with a `file:line`: the report runs settle then release, each under the wallet lock, and release re-runs settle (`ledger/settle.rs:46-66`); the renewal reserves and settles under the same lock (`coverage.rs:78-131`, `renew/mod.rs:258-287`); a lock wait past 2 s counts as an outage (`ledger/mod.rs:49-52`); debits are in-place updates (`debit_wallet.sql`); `billing_and_provider_keys.md` §5 states "a balance crossing zero mid-run ends the run" as an invariant this spec keeps. The soak worktree's `queued.rs` prototype read in full; its `reserve_detached` is not adopted because M240_001 places the hold inside issue.
- **Owner decisions** —
  > Indy (Nov 05, 2026): "Yes all four moves, the other tree had a similar one, i requested if this could be moved to a queue based approach, since a billing function shouldnt impact the next lease and a few seconds of delay to claim/settle isnt an issue for an end user. Can you check the other tree on thsi as well." — context: the queue, the accepted lag, and the prototype this spec adopts.
  > Indy (Nov 05, 2026): "ensure you review the spec for implementation completeness and that when its handed over to fable would be able to do its job." — context: the EGM review bar.
- **Model reviews** — EGM brainstorm, Claude Fable 5.1, Nov 05, 2026, comprehension lens (the one of six that completed): three findings routed here and applied: the journal's kinds, signs and rollup formula (§1); the hold-release trigger after report (§3, the queue, bounded by the sweep); the user-visible balance lag and who writes `exhausted_at` (§4, §5). Its question on whether the mid-run stop survives is answered in §2 and Invariant 6. The EGM review of this draft is recorded below it once run.
- **Open blockers** — none.
- **Deferrals and transfers** — none.
