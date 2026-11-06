<!--
SPEC AUTHORING RULES (the one comment that survives):
- The spec standard (spec.md) holds every rule. Each tpl: comment names the
  rules that govern its section; delete it once the section is filled.
- Body order is the implementing agent's read order.
- No time or effort estimates, owners or implementation dates (SPEC-ESTIMATE).
-->

# M242_001: An accepted message costs one Postgres statement, and no lock holds a fleet's intake across round trips

**Prototype:** v2.0.0

**Milestone:** M242

**Workstream:** 001

**Date:** Nov 06, 2026

**Status:** IN_PROGRESS

**Priority:** P1, every message a person, webhook or schedule sends pays this path, and at 10 messages a second per fleet the fleet-row lock is the ceiling

**Categories:** API, INFRA, OBS

**Batch:** B1, built before M242_002 (folding, which extends `admit_one.sql`) in the same milestone; starts after M241_001 merges

**Depends on:** M241_001, which creates `docs/architecture/push_scheduler.md` and the statement counts this spec is judged by

**Provenance:** LLM-drafted (Claude Opus 5.5, Nov 06, 2026) from the EGM brief and source research on main 14539a73e; amended at CHORE(open) against `origin/main` `67f5f7d19` (PR #803)

**Canonical architecture:** `docs/architecture/push_scheduler.md` §M242 (created by M241_001); `docs/architecture/datastore_scaling.md` §"The admissions ledger is the queue" describes the ledger this changes

---

## Overview

**Goal (testable):** An accepted message on an active fleet costs one top-level Postgres statement and one round trip, and 64 concurrent sends against a fleet budget of 10 admit exactly 10.

**Problem:** A new message costs four statements and six round trips (`afd_admission/src/admit.rs:136-147`):
- `BEGIN`, then the repeated-key check (`stage/policy.rs:17-24`).
- The policy read, which locks the fleet row `FOR NO KEY UPDATE` (`sql/select_installed_policy.sql:5`).
- The budget count of up to 10,001 unsettled rows (`budget/mod.rs:104-128`).
- `INSERT … ON CONFLICT` (`stage/insert.rs:24-47`), then `COMMIT`.

The fleet-row lock is held across the budget count, the insert and the commit, so one fleet admits one message per three round trips. The steer route adds a fifth statement: it reads the fleet's status before admitting (`afd_api_tenant/src/handler/fleet/message_steer/mod.rs:142-147`).

**Solution summary:** A trigger-kept row per fleet, `fleet.admission_backlogs`, holds the unsettled count. One autocommit statement checks ownership, answers a repeat, checks status, policy and both budgets, and inserts. It locks only the backlog row, from before the sequence is drawn until its own commit. The steer route drops its status read; the statement's answer carries the 404 and 409 cases. Responses are unchanged.

**Design flow:**

```
POST /v1/workspaces/{workspace_id}/fleets/{fleet_id}/messages
  route ─► Steer::append ─► Admissions::admit_runnable ─► ONE statement (autocommit):
     fleet    fleet row by id AND workspace, FOR SHARE ............ none? → 404
     repeat   row under (producer, producer_key), visible? ........ yes  → 202 replayed
     open     status = active · policy marker current · deployment estimate < ceiling
     backlog  upsert fleet.admission_backlogs WHERE unsettled_count < budget (row locked)
     insert   fleet.admissions … ON CONFLICT (producer, producer_key) DO UPDATE (no-op)
     trigger  AFTER INSERT: unsettled_count + 1   (nested, inside the same statement)
  commit releases the backlog row ─► doorbell wake ─► 202 {status, event_id, replayed}
report commit:  deferred trigger on settled_at NULL → set: unsettled_count − 1, at COMMIT
activation:     Admissions::stage runs the same statement on the caller's transaction
```

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(intake): admit each message in one statement under a per-fleet backlog row
- **Intent (one sentence):** let a busy fleet take bursts of messages without a lock held across round trips, at one statement each.
- **Authoring handshake:** Indy wants intake rebuilt for scale before production: one statement per accepted message, and the fleet-row lock no longer serialising a fleet's intake. The fleet budget, the deployment gate, producer-key idempotency, claim order and activation's shared transaction all keep their behaviour. The route's extra status read goes too. ASSUMPTIONS I'M MAKING:
  1. "One statement" means one top-level statement and one round trip. The backlog trigger runs inside it, and `pg_stat_statements.track=all` (`docker-compose.yml`) reports the trigger's update as a second, nested call. So `bench-steer` reads about 2.0 statements per steer, down from about 4.0.
  2. Responses stay byte-identical: 202 `SteerAccepted`, 404 `UZ-AGT-009`, 409 `UZ-AGT-012` with `current_state`, 409 `UZ-AGT-016`, 503 `UZ-INTERNAL-001`. No OpenAPI, command-line interface (CLI), web app or docs-repository change.
  3. Pull Request (PR) #803 has merged (`67f5f7d19`). CHORE(open) found it changed the budget: an obsolete task continuation no longer counts. §1 keeps the count exact by settling those rows (Dimensions 1.8, 1.9).
  4. Other producers keep their own status reads (webhook bindings, schedule fires); only the steer route's read goes.
  5. Intake's lock and statement bounds sit on each API login role (§6), because an autocommit statement has no transaction to carry `set_config`.
- **Implementer handshake:** recorded in the Pull Request Session Notes at PLAN, before any edit (SPEC-HANDSHAKE).

## Implementing agent — read these first

1. `rustd/crates/afd_admission/src/stage/insert.rs`, `stage/policy.rs`, `budget/mod.rs` and `sql/*.sql` — the four statements §2 folds into one, and the order they run in.
2. `rustd/crates/afd_fleet/src/lease/sql/lease/claim_admission_head.sql` — one statement chaining `MATERIALIZED` lock steps into a conditional upsert; §2's statement mirrors that shape.
3. `schema/1770_fleet_activity_counters.sql` — a trigger-kept aggregate with a `SECURITY DEFINER` function and a pinned `search_path`; §1 mirrors it.
4. `rustd/crates/afd_fleet/tests/integration_ledger_queue/ordering.rs` and `afd_fleet/tests/integration_lease_claim_race.rs` — an order test over a held transaction, and a waiter observed through `pg_blocking_pids`; §1 and §3 mirror both.
5. `rustd/crates/afd_events/tests/queued/snapshot.rs` — `pg_stat_statements` read in a private database; §2's statement count mirrors it.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `docs/v2/pending/M242_001_P1_API_INFRA_OBS_ONE_STATEMENT_MESSAGE_INTAKE.md` | CREATE, then MOVE to `active/` and `done/` | this spec |
| `schema/2700_fleet_admission_backlogs.sql` | CREATE | §1: placeholder slot, renumbered from `origin/main` at CHORE(open) because PR #803 takes 2190-2690 |
| `rustd/crates/afd_db/src/migration.rs` | EDIT | §1: registers the slot |
| `rustd/crates/afd_db/tests/integration_migrate/roles.rs`, `integration_migrate/backfill.rs`, `integration_migrate/mod.rs` | EDIT, CREATE | §1: the grant matrix row; the backfill test |
| `rustd/crates/afd_admission/src/sql/admit_one.sql` | CREATE | §2: the one statement |
| `rustd/crates/afd_admission/src/sql/insert_admission.sql`, `fleet_budget.sql`, `select_installed_policy.sql`, `select_repeated_on_fleet.sql` | DELETE | §2: folded into `admit_one.sql` |
| `rustd/crates/afd_admission/src/sql/mod.rs`, `stage/policy.rs` | EDIT, DELETE | §2: constants and the policy step go |
| `rustd/crates/afd_admission/src/stage/insert.rs`, `stage/mod.rs`, `stage/verdict/mod.rs`, `stage/verdict/tests.rs` | EDIT, CREATE | §2, §3: run the statement; decide the answer in order |
| `rustd/crates/afd_admission/src/admit.rs`, `budget/mod.rs`, `repeat.rs`, `lib.rs`, `Cargo.toml` | EDIT | §2: no transaction, `admit_runnable`, `check_fleet_budget` gone, docs; `afd_fleet_lifecycle` for `FleetStatus` |
| `rustd/crates/afd_admission/src/error/mod.rs`, `error/raise.rs`, `error/tests.rs` | EDIT | §2: `FleetAbsent` and `NotRunnable` |
| `rustd/crates/afd_admission/tests/receipts/backlog.rs`, `receipts/mod.rs`, `receipts/failures.rs`, `receipts/staging.rs` | CREATE, EDIT | §1–§4 tests |
| `rustd/crates/afd_fleet_lifecycle/src/lib.rs` | EDIT | §2: the runnable-status pin test |
| `rustd/crates/afd_fleet/tests/integration_ledger_queue/ordering.rs`, `settlement.rs` | EDIT | §3: claim order; §1: a report releases its count once |
| `rustd/crates/afd_fleet/src/task/sql/retire_continuations.sql`, `task/continuation.rs` | EDIT | §1: an issued obsolete continuation is settled too, so the count matches #803's budget rule |
| `rustd/crates/afd_fleet/tests/integration_tasks/**` | EDIT, CREATE | §1: Dimensions 1.8 and 1.9 |
| `rustd/crates/afd_db/src/login.rs`, `src/lib.rs`, `tests/integration_migrate/login.rs` | CREATE, EDIT | §6: `LOCK_TIMEOUT` and `STATEMENT_TIMEOUT`, and the test that reads them back through the API pool |
| `make/test-integration-rustd.mk` | EDIT | §6: `_api_login` sets both bounds on the test lane's API login |
| `playbooks/operations/database_region_move/001_playbook.md` | EDIT | §6: the API branch role gets both bounds after `pscale role create` |
| `rustd/crates/afd_events/src/steer/mod.rs`, `src/error/mod.rs` | EDIT | §4: `admit_runnable`; `Steer::replayed` goes; refusal accessors |
| `rustd/crates/afd_events/tests/integration_intake_cost.rs`, `events_suite.rs`, `integration_budgets.rs`, `integration_steer_races.rs`, `integration_steer_replay.rs` | CREATE, EDIT | §2, §4 tests; callers of `Steer::replayed` move to `append` |
| `rustd/crates/afd_http/src/services/event.rs` | EDIT | §4: `replayed` leaves `FleetSteering` |
| `rustd/crates/afd_api_tenant/src/handler/fleet/message_steer/mod.rs`, `tests.rs` | EDIT | §4: no status read; refusals mapped |
| `rustd/crates/afd_api/tests/fleet_messages_steer.rs` | EDIT | §4: the no-status-read test; the outage test's comment names the ledger |
| `rustd/crates/afd_bench/src/statements/paths.rs` | EDIT | §2: the fragment naming the deleted policy read is retargeted at `admit_one.sql`, keeping its path |
| `bench/baselines/steer.rig.json` | EDIT | §5: the median branch run |
| `docs/architecture/push_scheduler.md` | EDIT | §5: §M242 records the design and before/after numbers |
| `docs/architecture/datastore_scaling.md`, `postgres_schemas.md`, `scaling.md` | EDIT | §1, §3, §5: lock and budget sentences; the table and deletion-matrix rows; the steer row |

## Applicable Rules

- **`domains/sql.md`** — SCH, MIG, SQL-SLOT, SGR, STS, SQL-COUNTER, SQL-READER, SQL-ONEFACT, SQL-ID, SQL-TSTYPE, SQL-STAMP, SQL-FILE, SQL-ROW, SQL-PREPARE, SQL-UPSERT, SQL-ROLE, SQL-TIMEOUT (§6), SQL-ERASE, SQL-STATACT, CNX
- **`languages/rust.md`** — ERR-RS, RS-FROM, RS-MAPERR, RS-SOURCE, FN-RS, RS-RACE
- **`code.md`** — NDC, NLR, NLG, HLP, ORP, GRD, ECL, EMS, TGU, FLL, CODE-MSID
- **`tests.md`** — TCF, TEST-NEG, TEST-ERROR, TEST-CONTEND, TEST-COUNTER, TEST-RESIDUAL, TEST-REAL, TEST-SCOPE, TEST-LANE
- **`domains/http-responses.md`** — HTTP-STATUS, HTTP-STATEEXT, HTTP-REGISTRY
- **`domains/logging.md`** — LOG-EVENT, LOG-CODE, OBS
- **`domains/architecture.md`** — ARCH-LAND, ARCH-HOMES
- **`docs.md`** — DOC-02, DOC-03

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| `test_every_sql_file_prepares` (SQL-PREPARE) | yes, one statement file added and four deleted | `admit_one.sql` prepares against the rebuilt schema |
| `test_schema_objects_carry_no_vocabulary_literals` (STS) | yes, the slot adds trigger functions | trigger predicates test `settled_at` for NULL only |
| Grant matrix (`integration_migrate/roles.rs`) | yes, a new table | one row for `fleet.admission_backlogs` |
| `make lint-all` (clippy `-D warnings`, `cargo shear`) | yes | `lint-rustd` per Section; the new dependency is used |
| `docs.language` at `orly gate verify` and `orly gate pr` | yes, four architecture pages change | sentences under 25 words, paragraphs of at most three sentences |
| File & Function Length (file ≤350 lines, function ≤50, method ≤70) | yes, `admit.rs` and `message_steer/mod.rs` change | the refusal order lives in `stage/verdict/` |

## Prior-Art / Reference Implementations

- **Reference:** `claim_admission_head.sql` — locking steps marked `MATERIALIZED`, each read by the next, and an upsert whose `DO UPDATE … WHERE` is the guard. Divergence: none.
- **Reference:** `schema/1770_fleet_activity_counters.sql` — a per-fleet aggregate kept by a `SECURITY DEFINER` trigger the runtime cannot bypass. Divergence: the settle side is a deferred constraint trigger (§1).
- **Reference:** `afd_fleet/tests/integration_ledger_queue/ordering.rs` — order proven over a held transaction. Divergence: the held lock is the backlog row, not a transaction start.

## Sections (implementation slices)

### §1 — The backlog row counts each fleet's unsettled admissions

Slot `schema/2700_fleet_admission_backlogs.sql` creates `fleet.admission_backlogs`: keyed by `fleet_id` (one row per fleet, SQL-ID's keyed-by-parent exception, `ON DELETE CASCADE`), with `unsettled_count`, `created_at` and `updated_at`. Two `SECURITY DEFINER` functions with a pinned `search_path` keep the count, mirroring slot 1770:
- An `AFTER INSERT` row trigger on `fleet.admissions` upserts the fleet's row and adds one.
- A deferred constraint trigger, `AFTER UPDATE OF settled_at` when the old value is NULL and the new one is not, subtracts one at commit.

The slot creates both triggers before it backfills. Creating a trigger locks `fleet.admissions` against inserts and updates until the slot commits, so no concurrent write falls between the backfill and the triggers. `unsettled_count` defaults to zero. `api_runtime` gets `SELECT`, `INSERT` on `fleet_id`, `created_at` and `updated_at`, and `UPDATE` on `updated_at` only, so only the triggers move the count. The slot comment names its reader (`admit_one.sql`) and its consistency rule (SQL-ONEFACT's trigger-kept aggregate). It also says why this is not a `fleet.fleets` column: a report's commit would then wait on any revision install holding the fleet row `FOR UPDATE`.

**Implementation default:** the settle trigger is deferred to commit, because a plain `AFTER` trigger would hold the backlog row from the report's settle statement through its remaining writes (`afd_fleet/src/lease/commit.rs`), stalling the fleet's intake behind every report.

- **Dimension 1.1** — the count follows inserts and first settles → Test `backlog_count_follows_inserts_and_settles` (integration): three inserts on one fleet, then one settle, read 3 then 2, equal at each step to the fleet's `settled_at IS NULL` row count.
- **Dimension 1.2** — a settled row stamped again releases nothing → Test `restamping_a_settled_row_releases_nothing` (integration): `SET settled_at = COALESCE(settled_at, clock_timestamp())` on a settled row leaves 2, not 1.
- **Dimension 1.3** — a rolled-back insert leaves the count → Test `a_rolled_back_insert_leaves_the_count` (integration): an insert inside a transaction that rolls back leaves the count and the row total unchanged.
- **Dimension 1.4** — the runtime role cannot set the count → Test `runtime_role_cannot_set_the_backlog_count` (integration, under `api_runtime`, SQL-ROLE): `UPDATE … SET unsettled_count = 0`, and an `INSERT` naming `unsettled_count` for a fleet with no row, each fail with SQLSTATE `42501`; the count is unchanged.
- **Dimension 1.5** — an open report does not hold intake → Test `an_open_settle_does_not_hold_intake` (integration): a transaction settles a row and stays open; an admission on that fleet returns within 2 s, and a separate autocommit connection sees no session blocked on it (SQL-STATACT); after commit the count drops by one. A plain `AFTER` trigger fails this test.
- **Dimension 1.6** — the backfill counts what it finds → Test `backfill_counts_the_unsettled_rows_it_finds` (integration): a database built through the slot before 2700, holding 3 unsettled and 1 settled admission on one fleet, reads 3 once the slot applies.
- **Dimension 1.8** — an issued obsolete continuation leaves the count → Test `an_issued_obsolete_continuation_is_settled_and_released` (integration): a leased `task_continuation` row, then a generation advance through `continuation::retire`, reads the row settled and the count one lower. The count equals #803's budget predicate over the same fleet. Its late report is refused by the fence guard and bills nothing further (Indy, Discovery).
- **Dimension 1.9** — a report that advances its own task still settles its own row once → Test `a_report_that_advances_its_task_settles_its_row_once` (integration): the report's own continuation row is settled by the report, not by `retire`, and the count drops by exactly one.
- **Dimension 1.7** — every settle writer releases once → Test `failed_admission_settlement_rolls_back_the_report_and_retry_settles_once` (integration, amended in `settlement.rs`): the rolled-back report leaves the count; the retried report lowers it by exactly one.

### §2 — One statement admits, refuses or answers a repeat

`afd_admission/src/sql/admit_one.sql` replaces four statements. The mechanism (SPEC-MECH):
- **State authority:** the backlog row's `unsettled_count` for the fleet budget, `uq_admissions_producer_producer_key` for the producer key, and the fleet row for status and policy.
- **Guarded transition:** the backlog upsert's `DO UPDATE … WHERE unsettled_count < $budget`. Postgres evaluates it on the latest committed version once the row lock is granted, so each waiter sees the count its predecessors left.
- **Commit before release:** the statement runs in autocommit; the row lock ends at commit, after the insert and its trigger.
- **Failure assumption:** a connection lost before commit leaves nothing. A reply lost after commit is answered by the producer key on retry.

The statement's steps are common table expressions (CTEs), each read by the next, so Postgres runs them in order:
1. **fleet** (`MATERIALIZED`): the row by id and workspace, `FOR SHARE OF f`, with the head revision's policy marker. The lock excludes revision installs and status changes, which take `FOR UPDATE`, until commit; concurrent admitters share it.
2. **repeat**: the row under `(producer, producer_key)`, read only when the fleet was found, so a foreign workspace never probes a key.
3. **backlog**: runs only for a new key on a found fleet whose status matches the bound runnable spelling (when one is bound), whose policy marker is current, and whose deployment estimate is under its ceiling. An empty answer means only "full": the conflicting row is locked and re-read before the `WHERE` runs, so SQL-UPSERT's race does not arise.
4. **insert**: runs only when the backlog step answered. Its unconditional no-op `DO UPDATE` returns the row a racing same-key send committed (SQL-UPSERT).
5. **answer**: one row always: fleet found, status, policy current, the stored row (the repeat's or this insert's), whether this statement inserted it, and whether each budget was open.

`stage/verdict/` names the outcome in order: fleet absent, repeat, not runnable, policy, fleet budget, deployment budget, accepted. `admit` and the new `admit_runnable` take one pooled connection and send no `BEGIN`. `FleetAbsent` answers `UZ-AGT-009` and `NotRunnable { status }` answers `UZ-AGT-012`, each with its registry title; no code is new. Neither is counted or logged at warn by the ledger: before this spec the route refused both before the ledger saw them.

**Implementation default:** the runnable spelling is bound from `FleetStatus::Active.as_str()`, because `is_runnable` accepts exactly that status (`afd_fleet_lifecycle/src/lib.rs`). `FOR SHARE` because `FOR KEY SHARE` does not conflict with the status and head-revision updates the check must exclude. The statement returns facts and Rust names the refusal, so the order is one typed function tested without Postgres, and the statement holds no verdict literals.

- **Dimension 2.1** — a new message costs one statement → Test `an_accepted_message_costs_one_statement` (integration, private database): after one warm-up send, ten `Steer::append` calls add exactly 10 top-level `pg_stat_statements` calls for the runtime role and no `BEGIN` or `COMMIT`. They also add 10 nested calls of the backlog trigger's update, counted across roles because the function runs as its `SECURITY DEFINER` owner. The private database keeps a parallel test's calls out of the count.
- **Dimension 2.2** — the budget holds under concurrency → Test `sixty_four_sends_against_ten_admit_ten` (integration): 64 unkeyed `Steer::append` calls on one active fleet, released from a barrier with `fleet_backlog` 10, answer 10 accepted and 54 over-capacity refusals. The ledger holds 10 rows and the backlog row 10. A count read from the statement's snapshot admits more than 10 and fails this.
- **Dimension 2.3** — a repeat is answered while the budget is full → Test `a_repeat_is_answered_while_the_budget_is_full` (integration, private database): with budget 1 held by key K, resending K answers K's event with `replayed: true`. It adds one top-level call and no nested call, so neither a fallback read nor an insert ran.
- **Dimension 2.4** — racing repeats admit one row → Test `test_a_hundred_racing_repeats_admit_one_row` (integration, TEST-CONTEND): 100 sends of one key from a barrier leave one row and a count raised by one; all 100 answers name the same event.
- **Dimension 2.5** — a revision install cannot slip under an admission → Test `an_admission_waiting_on_a_revision_install_never_lands_under_the_old_policy` (integration): a transaction holds the fleet row `FOR UPDATE` and makes a foreign-marker revision head; an admission blocks behind it; after the commit it is refused with `UZ-AUTHOR-009` and no row lands.
- **Dimension 2.6** — a foreign workspace records nothing → Test `an_invalid_fleet_scope_records_nothing_and_sends_no_notification` (integration, amended): the refusal is `FleetAbsent`; no admission row, no backlog row and no `SPUBLISH`.
- **Dimension 2.7** — the refusal order → Test `verdict_names_the_first_refusal_in_order` (unit): absent with a stored row answers absent; a stored row on a stopped fleet answers the repeat; stopped with a foreign marker answers not runnable; a foreign marker on a full fleet answers policy; full with the deployment closed answers fleet budget.
- **Dimension 2.8** — only `active` is runnable → Test `only_active_is_runnable` (unit): `FleetStatus::ALL` filtered by `is_runnable` is `[Active]`, so a second runnable status fails here before the bound spelling goes stale.
- **Dimension 2.9** — the deployment gate still refuses new work and answers a retry → Test `a_spent_deployment_budget_refuses_new_work_and_still_answers_a_retry` (integration, unchanged): with the estimate at its ceiling, a new key is refused over capacity, and the earlier key is answered with its event.

### §3 — The shared transaction and claim order hold under the backlog row

`stage` binds no runnable status, so activation still admits while the fleet is `installing`: `afd_authoring/src/creation/activate.rs` stages before it flips the status. The insert and its count follow the caller's transaction, and the backlog row stays locked until the caller commits. The insert draws its sequence only after the backlog row is locked, and the lock lasts through commit. So within a fleet, commit order is sequence order, which the claim reads (`claim_admission_head.sql`, `ORDER BY a.sequence`). Slot 2140's comment naming the fleet-row lock is shipped history (SCH); slot 2700's comment names the new holder, and `datastore_scaling.md` changes with it.

- **Dimension 3.1** — a waiter takes the later sequence → Test `a_send_queued_behind_the_backlog_row_takes_the_later_sequence` (integration): a transaction locks the fleet's backlog row; a concurrent admission blocks (seen through `pg_blocking_pids`); the transaction stages an admission and commits. The waiter's sequence is higher, and the next claim takes the staged row. Drawing the sequence before the lock gives the waiter the lower one and fails this.
- **Dimension 3.2** — a rolled-back activation leaves nothing → Test `rollback_removes_staged_admission_without_a_notification` (integration, amended): no row, no notification, and the backlog count unchanged.
- **Dimension 3.3** — a committed activation counts once → Test `a_committed_stage_confirms_without_a_second_queue_record` (integration, amended): the count is unchanged before the caller commits and one higher after.
- **Dimension 3.4** — activation admits on an installing fleet → Test `staging_admits_on_an_installing_fleet` (integration): on an `installing` fleet, `stage` admits, while `admit_runnable` refuses with `NotRunnable { status: Installing }`.

### §4 — The steer route admits with no status read

`Steer::append` calls `admit_runnable`. The route drops `ingress_status` and the stopped-fleet lookup: one append answers 202, `FleetAbsent` maps to the route's 404, and `NotRunnable` to its 409 with `current_state`. The statement answers a repeat before it checks status, so a retry on a stopped fleet is still answered. `Steer::replayed` and `FleetSteering::replayed` are deleted, with every caller (NLG). `repeat_despite` stays for one case only: a same-key send racing a full budget. The `steer` module notes change to say so.

- **Dimension 4.1** — the route behaves as before, end to end → Test `test_replay_bypasses_ingress_refusal` (e2e, router with live Postgres and Dragonfly): a saved fleet answers 202; after stop, the retry answers 202 with the same event; a changed message 409 `UZ-AGT-016`; a fresh id 409 `UZ-AGT-012`; a keyless send 409.
- **Dimension 4.2** — a foreign fleet is a 404 → Test `test_foreign_fleet_replay_is_not_found` (e2e): another workspace's id and message under the owner's fleet answers 404 `UZ-AGT-009`.
- **Dimension 4.3** — no status read precedes the admission → Test `a_steer_reads_no_fleet_status_before_admitting` (integration, `afd_api` harness): with the fleets store unreachable and the steering ledger live over an active fleet, a steer answers 202. The parent commit answers 503, because it read the status first.
- **Dimension 4.4** — a stopped fleet's 409 names its state → Test `a_stopped_fleet_refusal_names_its_status` (unit): `NotRunnable { status: Paused }` renders 409, `UZ-AGT-012`, `current_state: "paused"`.
- **Dimension 4.5** — a stopped-fleet refusal is not an admission failure → Test `a_stopped_fleet_refusal_counts_no_admission` (integration): `admit_runnable` on a stopped fleet adds nothing to `agentsfleet_admissions_total` and logs no `admission_failed`.
- **Dimension 4.6** — an outage still answers 503 → Test `a_well_formed_steer_reaches_the_store_and_reports_the_outage` (unit, router over unreachable stores, comment amended): every well-formed steer answers 503 `UZ-INTERNAL-001`, now from the ledger statement instead of the status read.

### §5 — Statements per message and one fleet's intake, measured

On Indy's laptop, `make bench-steer PROFILE=rig` runs three times on the base revision and three times on the branch, interleaved. The median branch run is copied to `bench/baselines/steer.rig.json`. The one-fleet shape, `BENCH_FLEETS=1 BENCH_CONCURRENCY=16 BENCH_WINDOW_SECONDS=3`, runs the same way, with `failures` 0 so the budget never refuses. `make bench-lease PROFILE=rig BENCH_SHAPE=burst` runs once on the branch. `push_scheduler.md` §M242 records every value beside M241's baselines, and `datastore_scaling.md` and `scaling.md` change with them. **Implementation default:** `bench-steer` is the gate because its lane appends through `Steer::append`. Its counter includes nested calls, so the gate reads at most 2.05.

- **Dimension 5.1** — statements per steer halve → Test `make bench-steer PROFILE=rig` (integration): `postgres_statements_per_steer` is at most 2.05 in the committed baseline, from about 4.0 on the base.
- **Dimension 5.2** — the numbers are median real runs → Test `intake_numbers_recorded` (manual): §M242 lists three base and three branch values for rate, p95 and statements per steer in both shapes, plus the burst's `burst_append_p95_ms` and `burst_refused`; the committed file holds the median run.
- **Dimension 5.3** — the pages read clean → Test `orly gate verify` (integration): 0 `docs.language` findings on the four changed architecture pages.

### §6 — Intake runs under the login role's bounds

`afd_db::login` holds `LOCK_TIMEOUT` (`2000ms`) and `STATEMENT_TIMEOUT` (`5000ms`), the values the billing ledger and the admin bounds use. Postgres applies a role's settings only to the role that logs in, so each API login gets them: `ALTER ROLE <login> SET lock_timeout = …, statement_timeout = …`. The test lane's `_api_login` sets them, and the region-move playbook sets them after `pscale role create`. A transaction that sets its own bounds with `set_config` still wins, so billing and task reads keep theirs.

**Implementation default:** the login role rather than pool startup options, which Indy chose (Discovery). Pool startup options were also considered; unverified: whether the production PgBouncers pass them through.

- **Dimension 6.1** — the API pool carries both bounds → Test `the_api_login_runs_under_the_intake_bounds` (integration): `SHOW lock_timeout` and `SHOW statement_timeout` over the API pool equal `LOCK_TIMEOUT` and `STATEMENT_TIMEOUT`.
- **Dimension 6.2** — a held backlog row fails one send, not every caller → Test `a_send_behind_a_held_backlog_row_times_out` (integration): a transaction holds a fleet's backlog row past `LOCK_TIMEOUT`; a send answers 503 `UZ-INTERNAL-001` with SQLSTATE `55P03` logged, and records nothing.

## Interfaces

- **`afd_admission`:** `Admissions::admit(Admission) -> Result<Admitted>` keeps its signature and may now refuse `FleetAbsent`. New `Admissions::admit_runnable(Admission) -> Result<Admitted>` also refuses `NotRunnable` unless the key is a repeat. `prepare` and `stage` keep their signatures; `stage` binds no status. `Error::is_fleet_absent() -> bool` and `Error::not_runnable() -> Option<FleetStatus>` are new; a status spelling this build cannot parse refuses as an unreadable row (500), as `ingress_status` does today. `check_fleet_budget` is removed.
- **`afd_events`:** `Steer::append` keeps its signature. `Error::is_fleet_absent` and `Error::not_runnable` forward the ledger's. `Steer::replayed` is removed, and `FleetSteering::replayed` with it.
- **HTTP:** `POST /v1/workspaces/{workspace_id}/fleets/{fleet_id}/messages` is unchanged in paths, bodies, codes and `current_state` values; `public/openapi.json` does not change.
- **Login roles:** each API login carries `lock_timeout` and `statement_timeout` from `afd_db::login`; provisioning sets them (§6).
- **Schema:** `fleet.admission_backlogs (fleet_id, unsettled_count, created_at, updated_at)`, read by `admit_one.sql`. The runtime may create a zero row and stamp `updated_at`; only the two triggers change `unsettled_count`.
- **Bench:** `steer.rig.json` gains `postgres_statements_per_steer` (the current lane writes it; the committed file predates it).

## Failure Modes

| Mode | Kind | Cause | Handling (response and what the caller observes) | Test |
|------|------|-------|--------------------------------------------------|------|
| Fleet budget full under concurrency | capacity | many sends to one fleet at its budget | the budget's worth admitted; the rest 503 `UZ-INTERNAL-001`, nothing recorded | `sixty_four_sends_against_ten_admit_ten` |
| Repeat while full | replay | a retry after the budget filled | 202 `replayed: true`, first event | `a_repeat_is_answered_while_the_budget_is_full` |
| Same-key race | race | two sends with one key at once | one row, one count; both answered with its event | `test_a_hundred_racing_repeats_admit_one_row` |
| Deployment ceiling spent | capacity | estimate at 100,000 | new work 503; a repeat still answered | `a_spent_deployment_budget_refuses_new_work_and_still_answers_a_retry` |
| Fleet stopped or paused | failure | send after stop | 409 `UZ-AGT-012` with `current_state`; a repeat answered 202 | `test_replay_bypasses_ingress_refusal` |
| Foreign workspace | security | an id under another tenant's fleet | 404 `UZ-AGT-009` before any key is read | `test_foreign_fleet_replay_is_not_found` |
| Revision install races a send | race | install holds the fleet row | the send waits, then `UZ-AUTHOR-009`; nothing recorded | `an_admission_waiting_on_a_revision_install_never_lands_under_the_old_policy` |
| Sequence drawn out of order | race | a send queued behind the backlog row | the waiter takes the later sequence; claim order holds | `a_send_queued_behind_the_backlog_row_takes_the_later_sequence` |
| Activation rolls back | failure | the caller's transaction fails | no row, no count, no wake | `rollback_removes_staged_admission_without_a_notification` |
| Report holds the backlog row | capacity | a settle inside an open report | intake proceeds; the count drops at commit | `an_open_settle_does_not_hold_intake` |
| Settle stamped twice | failure | a `COALESCE` re-stamp | one release | `restamping_a_settled_row_releases_nothing` |
| Count written by hand | security | runtime SQL sets the count | `42501`, count unchanged | `runtime_role_cannot_set_the_backlog_count` |
| Obsolete continuation still leased | race | a task's generation advances mid-run | the row settles at once; the late report is refused and bills nothing further | `an_issued_obsolete_continuation_is_settled_and_released` |
| Backlog row held past the bound | capacity | a stalled holder of the row | that send 503 `UZ-INTERNAL-001`, `55P03` logged, nothing recorded | `a_send_behind_a_held_backlog_row_times_out` |
| API statement over 5 s | capacity | a slow query on the API login | `57014`; the request fails instead of holding a connection | `the_api_login_runs_under_the_intake_bounds` |
| Datastore unavailable | dependency | pool cannot connect | 503 `UZ-INTERNAL-001`, nothing recorded | `a_well_formed_steer_reaches_the_store_and_reports_the_outage` |

## Invariants

1. A fleet's `unsettled_count` equals its admissions with `settled_at IS NULL`. Authority: the two triggers, fired by every writer; the runtime role holds no `UPDATE` on the count. Counterexamples: Dimensions 1.1, 1.2, 1.4 and 1.7.
2. No admission lands without holding its fleet's backlog row from before its sequence through its commit. Authority: the insert step reads the backlog step's answer. Counterexample: Dimension 3.1.
3. A fleet never holds more unsettled admissions than its budget. Authority: the backlog upsert's `WHERE`, evaluated on the latest version under the lock. Counterexample: Dimension 2.2.
4. Ownership is decided before any producer key is read. Authority: the repeat step runs only on a found fleet. Counterexample: Dimension 4.2.
5. The bound runnable spelling is the only status `is_runnable` accepts. Authority: `only_active_is_runnable` (Dimension 2.8).

## Metrics & Observability

| Metric / event | Audience | Fires when | Properties allowed | Privacy guard | Test |
|----------------|----------|------------|--------------------|---------------|------|
| `agentsfleet_admissions_total` | operator | unchanged label set (`accepted`, `replayed`, `refused`, `over_budget`); a steer refused as stopped or foreign is not counted, as before | outcome | no identifiers or message text | `a_stopped_fleet_refusal_counts_no_admission` |
| `postgres_statements_per_steer` (bench key) | operator | each `bench-steer` run | number, revision | none | `make bench-steer PROFILE=rig` |

No event name is added or renamed.

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | An accepted message costs one top-level statement (§2) | `make test-integration-rustd` | output holds `an_accepted_message_costs_one_statement ... ok` | P0 | |
| R2 | 64 sends against a budget of 10 admit 10 (§2) | `make test-integration-rustd` | output holds `sixty_four_sends_against_ten_admit_ten ... ok` | P0 | |
| R3 | A repeat is answered while the budget is full (§2) | `make test-integration-rustd` | output holds `a_repeat_is_answered_while_the_budget_is_full ... ok` | P0 | |
| R4 | Claim order equals sequence order (§3) | `make test-integration-rustd` | output holds `a_send_queued_behind_the_backlog_row_takes_the_later_sequence ... ok` | P0 | |
| R5 | An open report does not hold a fleet's intake (§1) | `make test-integration-rustd` | output holds `an_open_settle_does_not_hold_intake ... ok` | P0 | |
| R6 | A report releases its count exactly once (§1) | `make test-integration-rustd` | output holds `failed_admission_settlement_rolls_back_the_report_and_retry_settles_once ... ok` | P0 | |
| R7 | The route reads no status before admitting (§4) | `git grep -n ingress_status -- rustd/crates/afd_api_tenant/src/handler/fleet/message_steer` | no output | P0 | |
| R8 | The four folded statements are gone (§2) | `git grep -n -w -e SELECT_REPEATED_ON_FLEET -e SELECT_INSTALLED_POLICY -e FLEET_BUDGET -e INSERT_ADMISSION -e check_fleet_budget -- rustd` | no output | P0 | |
| R9 | Statements per steer halve on the rig (§5) | `jq -e '.measurements.postgres_statements_per_steer <= 2.05' bench/baselines/steer.rig.json` | `true`, exit 0 | P1 | |
| R11 | An issued obsolete continuation leaves the count (§1) | `make test-integration-rustd` | output holds `an_issued_obsolete_continuation_is_settled_and_released ... ok` | P0 | |
| R12 | The API login runs under the intake bounds (§6) | `make test-integration-rustd` | output holds `the_api_login_runs_under_the_intake_bounds ... ok` | P0 | |
| R10 | The changed pages read clean (§5) | `orly gate verify` | exit 0 | P1 | |
| S1 | Lint | `make lint-all` | exit 0 | P0 | |
| S2 | Unit | `make test-unit-all` | exit 0 | P0 | |
| S3 | Integration | `make test-integration-rustd` | exit 0 | P0 | |
| S4 | Version | `make check-version` | exit 0 | P0 | |
| S5 | No secrets | `gitleaks detect --redact --no-banner` | exit 0 | P0 | |

## Dead Code Sweep

| Deleted file or symbol | Verify | Expected |
|------------------------|--------|----------|
| the four folded statements and `check_fleet_budget` | `git grep -n -w -e SELECT_REPEATED_ON_FLEET -e SELECT_INSTALLED_POLICY -e FLEET_BUDGET -e INSERT_ADMISSION -e check_fleet_budget -- rustd` | no output |
| `stage/policy.rs` | `git ls-files rustd/crates/afd_admission/src/stage/policy.rs` | no output |
| `Steer::replayed`, `FleetSteering::replayed` | `git grep -c "fn replayed" -- rustd/crates/afd_events/src/steer/mod.rs rustd/crates/afd_http/src/services/event.rs` | `rustd/crates/afd_events/src/steer/mod.rs:1` (only `Steered::replayed` stays) |
| the route's status read | `git grep -n ingress_status -- rustd/crates/afd_api_tenant/src/handler/fleet/message_steer` | no output |

`ingress_status` stays for the stream route (`afd_api_tenant/src/handler/stream/mod.rs`).

Schema this milestone leaves with no runtime reader or writer is dropped in a new slot in this Pull Request, under Indy's blanket approval (Discovery). Each drop gets a row above, with a `git grep -n -w` over `rustd`, `ui` and `cli` proving nothing still reads or writes it.

## Out of Scope

- Folding messages into a running run: M242_002.
- Group commit at intake: cut in the brief until measured.
- Reading the deployment ceiling's sample from the backlog table instead of the ledger: an addition, not needed to finish this one.
- The status reads other producers make (webhook bindings, schedule fires): each already reads its fleet for its own reasons.
- Fewer round trips on the lease, renewal and report paths: M245.

---

## Product Clarity (authoring record)

1. **Successful user moment** — a person sends ten messages a second to one busy fleet, and each send answers 202 after one database round trip.
2. **Preserved user behaviour** — retries with `operation_id`, 404 for a foreign fleet, 409 for a stopped one or a changed message, and 503 when the fleet is full all answer as before.
3. **Optimal-way check** — one statement per message is the floor short of batching sends, which the brief cut until measured.
4. **Rebuild-vs-iterate** — iterate: the ledger stays the queue; only how a row gets in changes.
5. **What we build** — the backlog row and its triggers, the one statement, the route without its status read, and the counter proofs.
6. **What we do NOT build** — group commit (unmeasured); a daemon-side fleet owner (no measured need); other producers' status reads (outside the steer path).
7. **Fit with existing features** — M242_002's folding settles several rows per report, each released by the deferred trigger. It must not destabilise activation's first run, which stages on its own transaction.
8. **Surface order** — API only; the CLI and the web app see the same responses.
9. **Dashboard restraint** — no new screen; `agentsfleet_admission_backlog` stays the operator's gauge.
10. **Confused-user next step** — a 503 `UZ-INTERNAL-001` says try later, and resending with the same `operation_id` is safe, as the route's description says.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** the table first, then the statement, then the activation and order proofs, then the route, then the measurement. The slot deploys before the daemon. A daemon still on the old code keeps counting rows itself, while the triggers already keep the new table current.
- **Alternatives considered:**
  - Keep the fleet-row lock inside one statement: rejected, because the count reads the snapshot taken before the lock wait and overshoots under concurrency.
  - The count as a `fleet.fleets` column: rejected, because a report's commit would wait on revision installs.
  - Serializable intake: rejected, because it retries exactly the contended burst.
  - A per-fleet advisory lock: rejected for the same snapshot overshoot.
  - A plain `AFTER` settle trigger: rejected, because it holds intake behind each report's remaining writes.
- **Patch-vs-refactor verdict:** a **refactor** because four statements and a transaction become one statement and the lock moves to a new row, with every response unchanged.

## Discovery (consult log)

- **Consults** — Source review on `main` `14539a73e`, Nov 06, 2026:
  - Intake's statements and lock are as the Problem states; the wake follows the commit (`admit.rs:102`, `wake.rs:6-8`).
  - Activation stages on its own transaction while the fleet is `installing` (`afd_authoring/src/creation/activate.rs`, `creation/start/admit.rs:35,53`).
  - The claim takes the lowest unsettled `sequence` (`claim_admission_head.sql`), and slot 2140 records the fleet lock as the reason that order holds.
  - Two runtime statements set `settled_at`: `settle_terminal.sql` and `meter_lease.sql`. PR #803 adds `retire_continuations.sql` and `settle_source.sql`; the triggers cover every writer.
  - CHORE(open) at `67f5f7d19`, Nov 06, 2026: PR #803 made `fleet_budget.sql` and `select_backlog.sql` skip a `task_continuation` row whose task generation moved or whose task ended. `retire_continuations.sql` already settles such rows when never issued. Every statement that advances a generation runs `continuation::retire` in the same transaction (`report/settle.rs:265`, `waits/claim.rs:169`, `waits/finish.rs:231`, `adoption.rs:295`, `lease.rs:165`, `waits/cleanup.rs:73`, `control.rs:62`). The report's fence guard refuses a settled row (`meter_lease.sql:55-59`), and M245 keeps that guard verbatim.
  - Production API logins are PlanetScale branch roles made with `pscale role create … --inherited-roles api_runtime` (`database_region_move/001_playbook.md:44-51`). Unverified: that a branch role may hold `ALTER ROLE … SET`; §6's playbook step settles it on first deploy.
  - Producers: `afd_cron/src/fire/mod.rs:127`, `afd_ingress/src/deliver.rs:111`, `afd_ingress/src/slack/admit.rs:63`, `afd_approval/src/inbox/resolve/mod.rs:190`, and activation; only the steer route reads status separately.
  - `docker-compose.yml` runs `postgres:19rc1-alpine` with `pg_stat_statements.track=all`; `rustd/Cargo.lock` pins sqlx 0.9.0. Dimensions 1.5, 2.2, 2.5 and 3.1 rerun when either, or the production Postgres major, changes (unverified: that major).
  - Risk: `FOR SHARE` from many concurrent senders creates multixact lock records on the fleet row. §5's one-fleet run measures it.
  - Risk: a deadlock needs a same-key send whose snapshot predates the first send's commit, still waiting on the backlog row when the first send's run reports. Postgres's detector then aborts one side, and the producer key makes the retry safe.
- **Owner decisions** —
  > Indy (Nov 06, 2026: 08:50 AM): "And we are not in production yet, so i want to rethink the approach on admissions, lease, and so on as i am focussed on scale. If something fails in one host bare metal, it should automatically cordon and respawn in another runner" — context: rebuilding intake before production.
  > Indy (Nov 06, 2026: 10:43 AM): "1. Yes placement rules are via the tags and labels 2. Respawn across ownership - this is a tenant setting ... Based on the scope i would like to build this ... 4. Yes, but donot overengineer now, we can improve this." — context: one table and one statement, no group commit or fleet owner.
  > Indy (Nov 06, 2026: 11:57 AM): "Yes i am ok with postgres store and accepting messages, fences runs" and "The message has to wait. I assume you show message like compacting" and "assume atleast 50 fleets" — context: Postgres stays the acceptance store.
  > Indy (Nov 06, 2026: 02:13 PM): "And i prefer we create all the spects and close M241 and push a PR?" — context: this spec lands with M241.
  > Indy (Nov 06, 2026: 04:06 PM): chose "On the login role (Recommended)" for the intake timeout — context: `lock_timeout` and `statement_timeout` are set once on the API's Postgres login role at provisioning, from named constants a test reads back through the pool; zero extra round trips. The provisioning change is his approved deploy edit.
  > Indy (Nov 06, 2026: 04:06 PM): chose "Settle them (Recommended)" for #803's obsolete continuations, noting "we are not in production, i donot care about older tasks, so we could even cleanup all the fleeets and test." — context: an obsolete leased continuation is settled when its task generation advances, so the trigger-kept count stays exact; the backfill needs no care for pre-existing tasks, and dev data may be reset.
  > Indy (Nov 06, 2026: 09:50 PM): chose "Amend, skip EGM" — context: the spec is amended at CHORE(open) for #803 and builds without the Elephant-Goldfish Model (EGM) review.
  > Indy (Nov 06, 2026: 09:52 PM): chose "Settle, accept the loss", then "but i thought this is changing in the subsequent? on renew, if so why poke around this" — context: an issued obsolete continuation settles when its generation advances; its late report is refused, so up to one renewal interval of its run time goes unbilled. Release with a marker column was declined as not worth the column before production.
  > Indy (Nov 06, 2026: 04:18 PM): "yes" — context: blanket approval, asked as "Should each milestone drop the tables and columns it leaves with no reader or writer?": a milestone drops, in a new slot in its own Pull Request, any table, column or index it leaves with no runtime reader or writer (`.orly/standards/domains/sql.md` destructive-change approval).
- **Model reviews** — skipped on Indy's call (Owner decisions, 09:50 PM).
- **Open blockers** — none; Indy answered both on Nov 06 (above).
- **Deferrals and transfers** — none. Each one needs an owner quote below, as `> Owner (<name>, <MMM DD, YYYY HH:MM>): "<quote>" — context: <item, why>`.
