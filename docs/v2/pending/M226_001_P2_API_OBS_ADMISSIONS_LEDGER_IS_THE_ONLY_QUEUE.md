<!--
SPEC AUTHORING RULES (load-bearing — the one comment that survives):
- Body order = the executing agent's read order. Fill via the orly-spec-new
  skill (authoring order lives there); after filling, DELETE every "tpl:"
  guidance comment — the SPEC TEMPLATE GATE blocks tpl residue, unfilled
  {slots}, and missing required sections (.orly/audits/spec-template.sh --staged).
- No time/effort/hour/day estimates anywhere. No effort columns, complexity
  ratings, percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only
  sequencing signal. A section that contradicts these rules loses — delete it.
-->

# M226_001: An admitted event is leased straight from the admissions ledger, with no Dragonfly copy of the queue

**Prototype:** v2.0.0
**Milestone:** M226
**Workstream:** 001
**Date:** Oct 13, 2026
**Status:** PENDING
**Priority:** P2 — no user surface changes; it removes the second copy of the work queue and the sweepers that keep the two copies in step
**Categories:** API, OBS
**Batch:** B1 — one Pull Request; the queue cannot half-move, so the cutover, the deletions and the docs ship together
**Branch:** pending — set at CHORE(open)
**Baseline revision:** pending — record the full comparison commit at CHORE(open)
**Test Baseline:** pending — measure declared unit and integration lanes before the Pull Request
**Baseline evidence:** pending — report path or run URL with revision, commands, passed/failed/skipped counts, and environment
**Depends on:** M225_001 — its doorbell (a sharded publish on every mark that wakes a parked lease poll) lands first; this spec moves the doorbell off the readiness index it deletes
**Provenance:** LLM-drafted (Claude Opus 5.5, Oct 13, 2026) from the adversarial review of the pickup path during M225_001 review round 2: three source maps of `feat/m225-lease-pickup-wake-hint` at `b25be9c72`, re-checked against `main` at `4a53b948c`
**Canonical architecture:** `docs/architecture/datastore_scaling.md` §"Readiness is partitioned"; `docs/architecture/runner_fleet.md` §Datastore topology; `docs/architecture/lease_flow.md`

---

## Overview

**Goal (testable):** `test_admitted_event_is_leased_from_the_ledger` — an event admitted through any producer is leased on the next poll by a claim that reads `fleet.admissions` alone, and a full admit → lease → report cycle leaves no `fleet:{id}:events` stream and no `fleet:ready:` key in Dragonfly.
**Problem:** One fact, "this fleet has work", lives in four stores: the ledger row, the per-fleet stream entry, a readiness mark in one of sixteen hashes, and (since M225_001) a directory entry. No transaction spans Postgres and Dragonfly, so each gap has its own repairer: replay, reconcile with progress/scan/walk, cursor and group restore, the two-arm `MARK_DELIVERED`, the history view's duplicate merge, the reclaim sweeper's re-mark and token compare-and-delete clears. The user-visible cost: a failed stream append leaves a steer unseen until the replay sweeper takes rows older than 30 s on a 30 s pass (`rustd/crates/afd_runner/src/sweep/replay.rs:36,47`), and every new pickup feature has to keep four copies agreeing.
**Solution summary:** The ledger becomes the queue. A lease poll's claim locks the oldest unsettled row of an eligible fleet whose affinity slot is free (`FOR UPDATE SKIP LOCKED`), bumps the fence and stamps the row in one transaction; "has work" is a partial index, an expired lease re-qualifies its row by the same predicate, a park and a gate answer stamp and clear the row, and the report settles it inside the transaction it already runs. Dragonfly keeps the jobs where it copies nothing: the live tail and the doorbell over sharded publish, key-value, command-line login sessions, and the outbound queue. The readiness index, the inbound streams, and every repairer listed above are deleted.

## PR Intent & comprehension handshake

- **PR title (eventual):** refactor(lease): lease straight from the admissions ledger
- **Intent (one sentence):** a steer reaches a runner through one durable record, so no sweeper is needed to repair a copy of it and pickup has one place to get fast.
- **Handshake** — pending until the implementing agent performs PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `docs/architecture/runner_fleet.md` — §Datastore topology and the lease clear rules (drained, parked) this design must preserve.
2. `docs/architecture/datastore_scaling.md` — why readiness was partitioned and how retention and admission budgets lean on the streams; every claim there is rewritten by §6.
3. `rustd/crates/afd_fleet/src/lease/sql/lease/mod.rs` — `CLAIM_AFFINITY_SLOT`, `RECLAIM_PRIOR_ACTIVE` and `SELECT_READY_CANDIDATES`: the slot and fence the new claim reuses.
4. `rustd/crates/afd_runner/src/sql/sweep/expire_abandoned_active_leases_batch.sql` — the repository's `FOR UPDATE SKIP LOCKED` shape, used by every replica's sweepers.
5. `rustd/crates/afd_fleet/src/lease/sql/report.rs` — `CLAIM_AND_SETTLE`, whose fence guard (the lease's `fencing_token` at or above the slot's) already refuses a dead holder's late report; the settle stamp joins it.

## Files Changed (blast radius)

Discovery grep, from repo root on `main`: `git grep -l -w -E 'ReadyIndex|ReadyCursor|READY_PARTITIONS|mark_ready|restore_group|delivered_cursor|XAUTOCLAIM|fleet_lease|fleet_ready_depth'`. Re-run it at CHORE(open); every hit not below is added before EXECUTE.

| File | Action | Why |
|------|--------|-----|
| `schema/933_fleet_admissions_queue.sql` (next free slot at CHORE(open)) | CREATE | lease, park and settle stamps, the unsettled partial index, grants, and the settled backfill |
| `schema/934_fleet_admissions_drop_receipt.sql` | CREATE | drops `receipt` and its two partial indexes once nothing reads them (owner approval, Discovery Q3) |
| `rustd/crates/afd_db/src/migration.rs` | EDIT | registers both migrations |
| `rustd/crates/afd_admission/src/{admit,budget/mod,lib,sql}.rs`, `budget/ceiling/mod.rs` | EDIT | admit commits the row and rings the doorbell; both backlog budgets count unsettled rows |
| `rustd/crates/afd_admission/src/{admit_receipt,replay,cursor,reconcile/mod}.rs`, `reconcile/` | DELETE | stream append, receipt write-back and the three repairers of it |
| `rustd/crates/afd_dragonfly/src/ready/mod.rs`, `ready/` | DELETE | the readiness index and partitions; the doorbell moves to its own module |
| `rustd/crates/afd_dragonfly/src/{streams/mod,lib,capacity,kv}.rs`, `streams/consume.rs` | EDIT | inbound append, group create and autoclaim go; the outbound stream and the tail keep theirs |
| `rustd/crates/afd_fleet/src/lease/{assign/mod,envelope,event,reclaim,store,settle,finalize}.rs`, `assign/`, `pull/held.rs`, `report/steps.rs`, `sql/{lease,report}.rs` | EDIT | the claim takes a ledger row; park, expiry and settle move the row; no XACK, trim or `MARK_DELIVERED` |
| `rustd/crates/afd_fleet/src/lease/{restore,mark}.rs` | DELETE | group restore and mark clears |
| `rustd/crates/afd_runner/src/sweep/{replay,reconcile/mod}.rs`, `sweep/reconcile/` | DELETE | sweepers of a copy that no longer exists |
| `rustd/crates/afd_runner/src/sweep/{mod,rebuild/mod,reclaim/mod,census/mod}.rs`, `sweep/rebuild/`, `sweep/reclaim/`, `sql/sweep.rs` | EDIT | rebuild keeps only the outbound pass; reclaim loses the stream probe and re-mark; census publishes the backlog gauge |
| `rustd/crates/afd_approval/src/inbox/resolve/mod.rs` | EDIT | an answer clears the park stamp and rings the doorbell |
| `rustd/crates/afd_gate/src/gate/{pause,store/mod}.rs` | EDIT | no readiness clear; `status = active` already gates the claim |
| `rustd/crates/afd_fleet_lifecycle/src/{install,purge,lib}.rs` | EDIT | no stream or group at install; no readiness clear at purge |
| `rustd/crates/afd_events/src/history/queued/mod.rs`, `history/statement/queued.rs`, `src/sql.rs` | EDIT | queued steers read unleased ledger rows; the duplicate merge goes |
| `rustd/crates/afd_observability/src/metrics/declared/fleet.rs`, `src/producers/fleet/mod.rs`, `docs/metrics.census.tsv`, `playbooks/operations/observability/assets/dashboard.json` | EDIT | `agentsfleet_fleet_ready_depth` goes; the backlog gauge changes producer |
| `rustd/crates/afd_bench/src/lane/{lease/mod,steer/mod,cardinality/mod,sweep}.rs` and their directories | EDIT | §1 ledger-claim variant; lanes stop seeding or counting readiness |
| tests under `rustd/crates/{afd_dragonfly,afd_fleet,afd_runner,afd_approval,afd_events,afd_admission,agentsfleetd}/tests/` that the discovery grep names | EDIT / DELETE | readiness and stream suites go; lease, reclaim and approval suites assert ledger state |
| `docs/architecture/{runner_fleet,datastore_scaling,lease_flow,scaling,data_flow,concurrency}.md` | EDIT | one queue, described as the design |

## Applicable Rules

- **`.orly/docs/greptile-learnings/RULES.md`** — NDC (every repairer is deleted, not left idle), NLR (touched readiness callers are cleaned, not wrapped), ORP (cross-layer orphan sweep over keys, metrics, docs and the dashboard), NSQ and STS (named, schema-qualified statements; no literal state strings in the migration), SGR (grants in the new migration), UFS (column and key names as constants).
- **`.orly/dispatch/write_rust.md`** and **`docs/RUST_ERROR_STANDARD.md`** — removed stream-failure error kinds leave no dead `ErrorKind` variants; new fallible paths use `error_shell!`.
- **`.orly/dispatch/write_sql.md`** — `VERSION` is `0.59.0`, so every schema change is a forward migration and the `receipt` drop needs owner approval for that specific change.
- **`.orly/docs/SCHEMA_CONVENTIONS.md`** — column, index and grant shape of the two migrations.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| SCHEMA GUARD | yes — two new `schema/*.sql` files and the migration array | forward migrations only; the drop waits for Q3's approval quote in Discovery |
| Architecture consult | yes — deletes the `fleet:ready:` and `fleet:{id}:events` key families and changes a Postgres schema | §6's doc edits land in the same commit as the code that changes each fact |
| UFS | yes — new column names and the doorbell channel | named constants beside the statements that use them |
| LOGGING | yes — the backlog gauge moves producer; removed sweeper events | event names from the logging standard; removed events leave no orphan reference |
| ERROR REGISTRY | yes — stream-append and group-restore failures disappear | delete their variants with their callers; no reserved codes left dangling |
| File & Function Length (≤350/≤50/≤70) | yes — `assign.rs`, `sql/lease/mod.rs`, `admit.rs` | split the ledger claim into `assign/ledger.rs` if `assign.rs` nears the cap |

## Prior-Art / Reference Implementations

- **Reference:** `rustd/crates/afd_runner/src/sql/sweep/expire_abandoned_active_leases_batch.sql` — the in-repo `FOR UPDATE SKIP LOCKED` claim every replica runs safely; the lease claim mirrors its lock-then-stamp shape.
- **Reference:** `rustd/crates/afd_fleet/src/lease/sql/report.rs` — fence-guarded settle; the new stamps ride its guard rather than adding a second one.

## Sections (implementation slices)

### §1 — Measure the ledger claim before cutting

A bench variant of `make bench-lease` runs the §3 claim against a seeded ledger and reports leases per second, Postgres round trips per lease and the idle probe's cost, beside the current contended row (1,928 leases/s, 2.4 round trips, `docs/architecture/scaling.md:272`) and idle row (1.00 Dragonfly command, 0 Postgres statements, `scaling.md:271`). Completion: both figures recorded in `scaling.md` and Indy's answers to Q1 and Q2 quoted in Discovery. Missing either blocks §3–§5. **Implementation default:** the variant reuses the drain lane's seeding because it already proves every event processed once.

- **Dimension 1.1** — the ledger-claim lane runs and reports round trips per lease → Test `test_ledger_claim_lane_reports_round_trips`
- **Dimension 1.2** — figures recorded and Q1/Q2 answered → Test `manual_ledger_claim_figures_recorded`

### §2 — The ledger carries queue state

A forward migration adds three nullable stamps to `fleet.admissions`: the fence of the lease holding the row, when it parked on a gate, and when its report settled it. A partial index on `(fleet_id, created_at, seq)` covers unsettled rows. The same migration backfills `settled_at` for every row whose `fleet.events` row is terminal, so a populated ledger upgrades with no stream drain. **Implementation default:** backfill over a quiesce window, because the fence guard already refuses any report the old code issued against a re-claimed row (Q4 can overrule).

- **Dimension 2.1** — fresh bootstrap creates the stamps, index and grants → Test `test_admissions_queue_migration_bootstraps`
- **Dimension 2.2** — populated upgrade settles exactly the terminal rows → Test `test_admissions_backfill_settles_terminal_rows`

### §3 — The claim takes the work

Admission commits the ledger row, then rings the doorbell; no append, no mark. A poll's candidate scan reads eligible fleets from the unsettled index (fleet `active`, slot free or expired, oldest unsettled row not parked). The claim then, in one transaction, wins the slot, bumps `fencing_token`, locks that fleet's oldest unsettled row with `FOR UPDATE SKIP LOCKED` and stamps the fence on it. `Acquired` is built from the row; `redelivered` is `delivered_at IS NOT NULL`. Gates and issue keep today's order after the claim and stay fence-guarded. Both backlog budgets count unsettled rows, the per-fleet one bounded by `LIMIT 10001`.

- **Dimension 3.1** — an admitted event is leased on the next poll from the ledger → Test `test_admitted_event_is_leased_from_the_ledger`
- **Dimension 3.2** — two replicas claiming the same fleet: one lease, one empty answer → Test `test_concurrent_claims_lease_one_row`
- **Dimension 3.3** — a fleet's rows lease in `(created_at, seq)` order, and a parked oldest row holds the rows behind it → Test `test_fleet_rows_lease_in_order_behind_a_park`
- **Dimension 3.4** — a re-claimed row is marked redelivered → Test `test_reclaimed_row_is_leased_as_redelivered`
- **Dimension 3.5** — the fleet budget refuses at 10,000 unsettled rows → Test `test_fleet_budget_counts_unsettled_rows`

### §4 — Report, park, answer and expiry move the row

`CLAIM_AND_SETTLE` stamps `settled_at` in the transaction it already runs, under its fence guard. A park stamps the row and frees the slot; a gate answer clears the stamp and rings the doorbell. An expired lease needs no sweeper: the expired slot and the unsettled row re-qualify by the §3 predicate. A poll that hears no doorbell for `NO_WORK_RETRY_AFTER` runs the §1-measured probe, so a lost doorbell costs one retry interval and never the event.

- **Dimension 4.1** — settle stamps the row in the report transaction; a rollback leaves it unsettled → Test `test_report_settles_its_ledger_row`
- **Dimension 4.2** — an expired lease is re-leased and the dead holder's late report is refused → Test `test_expired_lease_requalifies_and_late_report_is_refused`
- **Dimension 4.3** — park then answer: no lease while parked, leased after the answer → Test `test_parked_row_leases_after_the_answer`
- **Dimension 4.4** — a lost doorbell still leases within one retry interval → Test `test_lost_doorbell_leases_on_the_probe`

### §5 — Delete the Dragonfly queue and its repairers

The readiness index, its partitions and the M225_001 directory if it still exists; the inbound stream and its consumer group; group restore and the delivered cursor; replay, reconcile and its progress/scan/walk; the reclaim sweeper's stream probe and re-mark; the pause and purge clears; `MARK_DELIVERED`'s second arm and the history view's duplicate merge. Rebuild keeps only the outbound pass. The second migration drops `receipt` and its two indexes after Q3.

- **Dimension 5.1** — a full admit → lease → report cycle leaves no inbound stream or readiness key → Test `test_cycle_leaves_no_dragonfly_queue_keys`
- **Dimension 5.2** — the receipt drop applies on a populated ledger → Test `test_receipt_drop_migration_applies_populated`
- **Dimension 5.3** — a queued steer shows once in history, from the ledger → Test `test_queued_steer_reads_from_the_ledger`
- **Dimension 5.4** — the census pass publishes the admission backlog from the unsettled index → Test `test_census_publishes_admission_backlog`

### §6 — Architecture docs describe one queue

`runner_fleet.md`, `datastore_scaling.md`, `lease_flow.md`, `scaling.md`, `data_flow.md` and `concurrency.md` describe the ledger as the queue and Dragonfly's remaining roles. The scaling rows are re-measured, not edited by hand.

- **Dimension 6.1** — no architecture doc or Rust source names the deleted keys or types → Test `test_no_reference_to_the_dragonfly_queue`

## Interfaces

```
HTTP: unchanged. POST /v1/runners/me/leases, the reports route and their wire shapes keep every field.
fleet.admissions       + lease_fence BIGINT NULL   fence of the lease holding the row
                       + parked_at   BIGINT NULL   set by a park, cleared by the gate answer
                       + settled_at  BIGINT NULL   set by CLAIM_AND_SETTLE, never cleared
                       - receipt (§5, after Q3)
queue predicate        settled_at IS NULL; a fleet is eligible when its oldest such row has parked_at IS NULL
doorbell               M225_001's sharded publish, rung after the admission commit and the gate answer
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Concurrent claim | two replicas poll the same fleet | slot claim and `SKIP LOCKED` let one win; the other answers no-work and the loser's poll moves on |
| Dead holder | runner dies mid-lease | slot expiry re-qualifies the row; the new claim bumps the fence; the late report is refused by the existing guard |
| Lost doorbell | publish dropped, hub reconnecting | the parked poll's probe finds the row within one retry interval |
| Report rollback | report transaction aborts | `settled_at` stays null; the row is re-leased after expiry; billing stays single because the settle and the charge share the transaction |
| Park without answer | a person never answers | row stays parked and holds its fleet, as today; the inbox expiry sweep answers it |
| Upgrade with runs in flight | leases issued by the old build when §2's migration runs | their rows stay unsettled and their reports settle them under the fence guard; a row whose holder died re-qualifies on expiry and leases once more, as reclaim does today |
| Postgres unavailable | database outage | admission already fails closed on the ledger insert; polls answer an error instead of a stale Dragonfly offer |

## Invariants

1. A ledger row leases only under the fence its claim minted — enforced by the claim transaction stamping `lease_fence` from the `fencing_token` it bumped, and by `CLAIM_AND_SETTLE`'s guard.
2. One unsettled row per fleet is leased at a time — enforced by the affinity slot claim preceding the row lock in the same transaction.
3. A settled row never leases again — enforced by the claim statement's `settled_at IS NULL` predicate, and by `CLAIM_AND_SETTLE` being the only statement that writes `settled_at` (the Dead Code Sweep grep proves no other writer).
4. No Dragonfly write sits between the admission commit and the caller's answer except the best-effort doorbell — enforced by `admit` returning after the commit and logging, never propagating, a doorbell failure.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| `agentsfleet_fleet_ready_depth` | ops | removed with the index it sampled | — | — | `test_no_reference_to_the_dragonfly_queue` |
| `agentsfleet_admission_backlog`, `agentsfleet_admission_backlog_oldest_age_seconds` | ops | each census pass, from the unsettled index (was the replay sweeper) | row count, oldest age | counts only | `test_census_publishes_admission_backlog` |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | integration | `test_ledger_claim_lane_reports_round_trips` | a 200-fleet seeded drain through the ledger claim reports a nonzero leases/s and a round-trip figure, every event processed once |
| 1.2 | manual | `manual_ledger_claim_figures_recorded` | Indy reads the §1 figures in `scaling.md`; his Q1/Q2 answers are quoted in Discovery |
| 2.1 | integration | `test_admissions_queue_migration_bootstraps` | empty database → three nullable stamps, the partial index and the runtime role's grants exist |
| 2.2 | integration | `test_admissions_backfill_settles_terminal_rows` | rows with terminal, running and no `fleet.events` rows → only the terminal ones get `settled_at` |
| 3.1 | integration | `test_admitted_event_is_leased_from_the_ledger` | steer admitted → next poll leases it; Dragonfly holds no `fleet:{id}:events` key; regression: the held-sandbox, sticky-routing and approval suites pass unchanged |
| 3.2 | integration | `test_concurrent_claims_lease_one_row` | two claims on one fleet at once → one lease, one empty answer, one fence bump |
| 3.3 | integration | `test_fleet_rows_lease_in_order_behind_a_park` | rows A, B, C with A parked → no lease; answer A → A, then B, then C |
| 3.4 | integration | `test_reclaimed_row_is_leased_as_redelivered` | lease A, expire the slot, poll → A again with `redelivered = true` |
| 3.5 | integration | `test_fleet_budget_counts_unsettled_rows` | 10,000 unsettled rows → the next admit is refused `over_budget`; settle one → admitted |
| 4.1 | integration | `test_report_settles_its_ledger_row` | report commits → `settled_at` set; injected abort → null and wallet unchanged |
| 4.2 | integration | `test_expired_lease_requalifies_and_late_report_is_refused` | holder silent past expiry → second runner leases; first runner's report answers fenced |
| 4.3 | integration | `test_parked_row_leases_after_the_answer` | gate parks A → polls lease nothing; answer → next poll leases A |
| 4.4 | integration | `test_lost_doorbell_leases_on_the_probe` | doorbell publish dropped → lease arrives within one `NO_WORK_RETRY_AFTER` |
| 5.1 | integration | `test_cycle_leaves_no_dragonfly_queue_keys` | admit → lease → report, then `SCAN` for `fleet:ready:` and `:events` → 0 keys |
| 5.2 | integration | `test_receipt_drop_migration_applies_populated` | populated ledger → drop applies; leases still flow |
| 5.3 | integration | `test_queued_steer_reads_from_the_ledger` | unleased steer → one QUEUED row in the thread; after lease → one received row, no duplicate |
| 5.4 | integration | `test_census_publishes_admission_backlog` | 3 unsettled rows → gauge reads 3 and an oldest age ≥ 0 |
| 6.1 | unit | `test_no_reference_to_the_dragonfly_queue` | workspace sources and architecture docs scanned for the deleted type, key and metric names → 0 hits |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Work leases from the ledger alone (§3) | `make test-integration-rustd` | `test_admitted_event_is_leased_from_the_ledger` passed | P0 | |
| R2 | Expiry, park and lost doorbell lose no event (§4) | `make test-integration-rustd` | `test_expired_lease_requalifies_and_late_report_is_refused` and `test_lost_doorbell_leases_on_the_probe` passed | P0 | |
| R3 | No Dragonfly queue remains (§5) | `git grep -n -w -E 'ReadyIndex\|ReadyCursor\|READY_PARTITIONS\|restore_group\|delivered_cursor\|XAUTOCLAIM' -- rustd docs/architecture` | 0 matches | P0 | |
| R4 | Ledger claim measured and Q1–Q4 answered (§1) | manual — Discovery quotes and `scaling.md` rows | four Indy quotes; two re-measured rows | P0 | |
| R5 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | |
| S1 | Conform gates green | `make harness-verify` | exit 0 | P0 | |
| S2 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S3 | Lint green | `make lint-all` | exit 0 | P0 | |
| S4 | Integration green | `make test-integration-rustd` | exit 0 | P0 | |
| S5 | Version in sync | `make check-version` | exit 0 | P0 | |
| S6 | No secrets | `gitleaks detect` | exit 0 | P0 | |

**Command source rule:** copy every declared `conform` and `verify.*` invocation from `.orly/orly.json` into a Verify cell, verbatim, with an Expected value. See `.orly/dispatch/lifecycle.md` for command timing; baseline metadata is pending at opening and measured before the Pull Request.

**Grading protocol (VERIFY):** run each spec-specific Verify command verbatim; Graded = ✅/❌ + one decisive output line. Repository-command rows point to the final `orly gate pr` results in Pull Request Session Notes. **Ship gate:** every required check must pass before the Pull Request is ready; missing evidence or any ❌ returns to EXECUTE. A P0 may be MOVED only under the transfer rule in `.orly/docs/TEMPLATE.md`.

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

| File to delete | Verify |
|----------------|--------|
| `rustd/crates/afd_dragonfly/src/ready/mod.rs` | `test ! -f rustd/crates/afd_dragonfly/src/ready/mod.rs` |
| `rustd/crates/afd_admission/src/replay/mod.rs` | `test ! -f rustd/crates/afd_admission/src/replay/mod.rs` |
| `rustd/crates/afd_admission/src/reconcile/mod.rs` | `test ! -f rustd/crates/afd_admission/src/reconcile/mod.rs` |
| `rustd/crates/afd_admission/src/cursor.rs` | `test ! -f rustd/crates/afd_admission/src/cursor.rs` |
| `rustd/crates/afd_fleet/src/lease/restore.rs` | `test ! -f rustd/crates/afd_fleet/src/lease/restore.rs` |
| `rustd/crates/afd_runner/src/sweep/replay.rs` | `test ! -f rustd/crates/afd_runner/src/sweep/replay.rs` |

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| readiness and stream-queue types | `git grep -n -w -E 'ReadyIndex\|ReadyCursor\|READY_PARTITIONS\|mark_ready\|restore_group\|delivered_cursor' -- rustd docs` | 0 matches |
| `agentsfleet_fleet_ready_depth` | `git grep -n -w fleet_ready_depth` | 0 matches |
| `MARK_DELIVERED`, `RECORD_RECEIPT` | `git grep -n -w -E 'MARK_DELIVERED\|RECORD_RECEIPT' -- rustd` | 0 matches |
| other `settled_at` writers | `git grep -n 'settled_at =' -- rustd/crates \| grep -v sql/report.rs` | 0 matches |

## Out of Scope

- The outbound queue (`connector:outbound`): it stays a Dragonfly stream; its head-of-line row in `scaling.md` is a separate spec.
- Ledger retention: no sweeper deletes `fleet.admissions` rows today (`grep -rn fleet.admissions rustd/crates/afd_runner/src` finds none); the partial index keeps the claim off settled rows, and retention stays as it is.
- The report path's five-file split (report, steps, commit, settle, finalize) and the fence rule's eight spellings: review findings, not this queue move.
- Name collisions (`Admission`, `Delivery`, `Verdict`, `claim`, `session`): a rename spec of their own.

---

## Product Clarity (authoring record)

1. **Successful user moment** — a fleet owner sends a steer during a Dragonfly node restart and it still runs, once, without waiting for a 30 s sweeper.
2. **Preserved user behaviour** — every producer, the lease and report routes, per-fleet order, parked gates holding their fleet, sticky routing and held sandboxes.
3. **Optimal-way check** — one durable record is the direct shape; the gap is idle polls costing a Postgres probe when the doorbell is silent, sized by §1.
4. **Rebuild-vs-iterate** — rebuild of the queue only; the lease, gate and report code is iterated in place.
5. **What we build** — two migrations, the ledger claim, the park and settle stamps, the census backlog gauge, the bench variant.
6. **What we do NOT build** — a new queue service, an outbound move, a report-path refactor, renames.
7. **Fit with existing features** — compounds with M225_001's doorbell; must not destabilize the held-sandbox routing on `runner.fleet_slots`.
8. **Surface order** — N/A — no user surface; the HTTP routes are unchanged.
9. **Dashboard restraint** — the readiness panel goes with its metric; no replacement panel until the census backlog has a week of samples.
10. **Confused-user next step** — N/A — no user surface; an operator reads `agentsfleet_admission_backlog`.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** six Sections in dependency order: measure, schema, claim, row transitions, deletions, docs; one Pull Request, because a half-moved queue has two sources of truth again.
- **Alternatives considered:** (a) keep both stores and add a transactional outbox from the ledger to the stream: removes replay's 30 s gap but keeps the mark, the directory and every Dragonfly repairer; (b) collapse the sixteen partitions into one sorted set: fixes pickup order but keeps two queues; (c) M225_001's doorbell alone: removes the newest layer only.
- **Patch-vs-refactor verdict:** this is a **refactor** because the concept count comes from the two-store design itself; Postgres load barely moves, since a drained lease already costs 50.0 Postgres statements (`docs/architecture/scaling.md:273`) and the claim adds about one while `MARK_DELIVERED` and the receipt write go.

## Discovery (consult log)

- **Consults** — Indy (Oct 13, 2026, AskUserQuestion during M225_001 review round 2) picked "Spec it now (Recommended)" for this refactor as a pending spec, no code. Open human decisions, each blocking the boundary named:
  - **Q1 (blocks §3)** — reverse the recorded design "The streams are the record" (`rustd/crates/afd_dragonfly/src/ready/mod.rs:9-14` on `main`) and "PostgreSQL is the forge and Dragonfly a cache of it" (`rustd/crates/afd_runner/src/sweep/rebuild/mod.rs:5`)? Not decided by this spec.
  - **Q2 (blocks §4.4)** — idle fallback: a Postgres probe per silent poll, or a one-bit Dragonfly flag beside the doorbell; decided from §1's figures.
  - **Q3 (blocks the §5 drop migration)** — approve dropping `fleet.admissions.receipt` and its two indexes on a live datastore (`VERSION` 0.59.0, `.orly/dispatch/write_sql.md` §Repository schema policy).
  - **Q4 (blocks §2.2)** — cutover by backfill (default) or by a quiesce window that drains live streams first.
- **Metrics review** — `agentsfleet_fleet_ready_depth` removed with its dashboard panel; the backlog gauge changes producer only; analytics/funnel playbook update not required — no product event changes.
- **Skill-chain outcomes** — pending.
- **Deferrals** — none.
