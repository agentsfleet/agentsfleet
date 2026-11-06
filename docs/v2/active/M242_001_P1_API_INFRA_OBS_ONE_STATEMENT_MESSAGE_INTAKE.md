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

**Goal (testable):** An accepted message on an active fleet costs one top-level Postgres statement and one round trip, and 64 concurrent sends to one fleet each admit one row, in sequence order.

**Problem:** A new message costs four statements and six round trips (`afd_admission/src/admit.rs:136-147`):
- `BEGIN`, then the repeated-key check (`stage/policy.rs:17-24`).
- The policy read, which locks the fleet row `FOR NO KEY UPDATE` (`sql/select_installed_policy.sql:5`).
- The budget count of up to 10,001 unsettled rows (`budget/mod.rs:104-128`).
- `INSERT … ON CONFLICT` (`stage/insert.rs:24-47`), then `COMMIT`.

The fleet-row lock is held across the budget count, the insert and the commit, so one fleet admits one message per three round trips. The steer route adds a fifth statement: it reads the fleet's status before admitting (`afd_api_tenant/src/handler/fleet/message_steer/mod.rs:142-147`).

**Solution summary:** The per-fleet budget goes: nothing recorded why it was 10,000, and only a runaway producer reaches it (Indy, Discovery). The deployment ceiling still bounds the total. One autocommit statement then locks the fleet row, checks ownership, answers a repeat, checks status, policy and the deployment ceiling, and inserts. The fleet-row lock lasts one statement instead of six round trips. The steer route drops its status read; the statement's answer carries the 404 and 409 cases. Responses are unchanged, except that a fleet is no longer refused at 10,000 unsettled messages.

**Design flow:**

```
POST /v1/workspaces/{workspace_id}/fleets/{fleet_id}/messages
  route ─► Steer::append ─► Admissions::admit_runnable ─► ONE statement (autocommit):
     found    fleet row by id AND workspace, FOR NO KEY UPDATE ....... none? → 404
     repeat   row under (producer, producer_key) ..................... yes  → 202 replayed
     insert   only if status = active · policy marker current · deployment estimate < ceiling
              fleet.admissions … ON CONFLICT (producer, producer_key) DO UPDATE (no-op)
  commit releases the fleet row ─► doorbell wake ─► 202 {status, event_id, replayed}
activation: Admissions::stage runs the same statement on the caller's transaction
```

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(intake): admit each message in one statement under its fleet lock
- **Intent (one sentence):** let a busy fleet take bursts of messages without a lock held across round trips, at one statement each.
- **Authoring handshake:** Indy wants intake rebuilt for scale before production: one statement per accepted message, and the fleet-row lock no longer held across round trips. The deployment gate, producer-key idempotency, claim order and activation's shared transaction all keep their behaviour. The route's extra status read goes too. ASSUMPTIONS I'M MAKING:
  1. "One statement" means one top-level statement and one round trip. No trigger runs inside it, so `bench-steer` reads about 1.0 statement per steer, down from about 4.0.
  2. Responses stay byte-identical, except that no fleet is refused for holding 10,000 unsettled messages: 202 `SteerAccepted`, 404 `UZ-AGT-009`, 409 `UZ-AGT-012` with `current_state`, 409 `UZ-AGT-016`, 503 `UZ-INTERNAL-001`. No OpenAPI, command-line interface (CLI), web app or docs-repository change.
  3. Pull Request (PR) #803 has merged (`67f5f7d19`). Its rule that an obsolete task continuation does not count goes with the per-fleet budget; the deployment estimate (`select_backlog.sql`) keeps it.
  4. Other producers keep their own status reads (webhook bindings, schedule fires); only the steer route's read goes.
  5. Intake's lock and statement bounds sit on each API login role (§6), because an autocommit statement has no transaction to carry `set_config`.
- **Implementer handshake:** recorded in the Pull Request Session Notes at PLAN, before any edit (SPEC-HANDSHAKE).

## Implementing agent — read these first

1. `rustd/crates/afd_admission/src/stage/insert.rs`, `stage/policy.rs`, `budget/mod.rs` and `sql/*.sql` — the four statements §2 folds into one, and the order they run in.
2. `rustd/crates/afd_fleet/src/lease/sql/lease/claim_admission_head.sql` — one statement chaining `MATERIALIZED` lock steps into a conditional upsert; §2's statement mirrors that shape.
3. `rustd/crates/afd_fleet/tests/integration_ledger_queue/ordering.rs` and `afd_fleet/tests/integration_lease_claim_race.rs` — an order test over a held transaction, and a waiter observed through `pg_blocking_pids`; §3 mirrors both.
4. `rustd/crates/afd_events/tests/queued/snapshot.rs` — `pg_stat_statements` read in a private database; §2's statement count mirrors it.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `docs/v2/pending/M242_001_P1_API_INFRA_OBS_ONE_STATEMENT_MESSAGE_INTAKE.md` | CREATE, then MOVE to `active/` and `done/` | this spec |
| `rustd/crates/afd_admission/src/sql/admit_one.sql` | CREATE | §2: the one statement |
| `rustd/crates/afd_admission/src/sql/insert_admission.sql`, `fleet_budget.sql`, `select_installed_policy.sql`, `select_repeated_on_fleet.sql` | DELETE | §1, §2: the per-fleet count goes; the rest fold into `admit_one.sql` |
| `rustd/crates/afd_admission/src/sql/mod.rs`, `stage/policy.rs` | EDIT, DELETE | §2: constants and the policy step go |
| `rustd/crates/afd_admission/src/stage/insert.rs`, `stage/mod.rs`, `stage/verdict/mod.rs`, `stage/verdict/tests.rs` | EDIT, CREATE | §2, §3: run the statement; decide the answer in order |
| `rustd/crates/afd_admission/src/admit.rs`, `budget/mod.rs`, `repeat.rs`, `lib.rs`, `Cargo.toml` | EDIT | §1, §2: the per-fleet budget goes; no transaction; `admit_runnable`; `afd_fleet_lifecycle` for `FleetStatus` |
| `rustd/crates/afd_admission/src/error/mod.rs`, `error/raise.rs`, `error/tests.rs`, `src/tests/mod.rs` | EDIT | §1, §2: `FleetAbsent` and `NotRunnable`; `OverBudget` loses its scope |
| `rustd/crates/afd_admission/tests/receipts/budgets.rs`, `receipts/mod.rs`, `receipts/failures.rs`, `receipts/staging.rs` | EDIT | §1–§3 tests |
| `rustd/crates/afd_fleet_lifecycle/src/lib.rs` | EDIT | §2: the runnable-status pin test |
| `rustd/crates/afd_fleet/tests/integration_ledger_queue/ordering.rs`, `delivery.rs` | EDIT | §3: claim order under the fleet lock; the delivery test names the fleet lock, not a budget |
| `rustd/crates/afd_authoring/tests/draft/creation/stages.rs` | EDIT | §1: a blocked activation is refused by the deployment ceiling |
| `rustd/crates/afd_api/tests/authoring/saving/initial_run.rs`, `rustd/crates/afd_approval/tests/integration_inbox_continuation.rs` | EDIT | §1: `Budgets` has one field, so their struct updates go |
| `rustd/crates/afd_events/src/steer/mod.rs`, `src/error/mod.rs` | EDIT | §4: `admit_runnable`; `Steer::replayed` goes; refusal accessors |
| `rustd/crates/afd_events/tests/integration_intake_cost.rs`, `events_suite.rs`, `integration_budgets.rs`, `integration_steer_races.rs`, `integration_steer_replay.rs` | CREATE, EDIT | §1, §2, §4 tests; callers of `Steer::replayed` move to `append` |
| `rustd/crates/afd_http/src/services/event.rs` | EDIT | §4: `replayed` leaves `FleetSteering` |
| `rustd/crates/afd_api_tenant/src/handler/fleet/message_steer/mod.rs`, `tests.rs` | EDIT | §4: no status read; refusals mapped |
| `rustd/crates/afd_api/tests/fleet_messages_steer.rs` | EDIT | §4: the no-status-read test; the outage test's comment names the ledger |
| `rustd/crates/afd_bench/src/statements/paths.rs` | EDIT | §2: the fragment naming the deleted policy read is retargeted at `admit_one.sql`, keeping its path |
| `rustd/crates/afd_db/src/login.rs`, `src/lib.rs`, `tests/integration_migrate/login.rs` | CREATE, EDIT | §6: `LOCK_TIMEOUT` and `STATEMENT_TIMEOUT`, and the test that reads them back through the API pool |
| `make/test-integration-rustd.mk` | EDIT | §6: `_api_login` sets both bounds on the test lane's API login |
| `playbooks/operations/database_region_move/001_playbook.md` | EDIT | §6: the API branch role gets both bounds after `pscale role create` |
| `bench/baselines/steer.rig.json` | EDIT | §5: the median branch run |
| `docs/architecture/push_scheduler.md` | EDIT | §5: §M242 records the design and before/after numbers |
| `docs/architecture/datastore_scaling.md`, `lease_flow.md`, `scaling.md` | EDIT | §1, §3, §5: the budget and lock sentences; the per-fleet cap line; the steer row |

No schema slot changes: the statement reads and writes existing columns.

## Applicable Rules

- **`domains/sql.md`** — SQL-STAMP, SQL-FILE, SQL-ROW, SQL-PREPARE, SQL-UPSERT, SQL-ROLE, SQL-TIMEOUT (§6), SQL-ERASE, SQL-STATACT, CNX
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
| `make lint-all` (clippy `-D warnings`, `cargo shear`) | yes | `lint-rustd` per Section; the new dependency is used |
| `docs.language` at `orly gate verify` and `orly gate pr` | yes, four architecture pages change | sentences under 25 words, paragraphs of at most three sentences |
| File & Function Length (file ≤350 lines, function ≤50, method ≤70) | yes, `admit.rs` and `message_steer/mod.rs` change | the refusal order lives in `stage/verdict/` |

## Prior-Art / Reference Implementations

- **Reference:** `claim_admission_head.sql` — locking steps marked `MATERIALIZED`, each read by the next, ending in a conditional write. Divergence: none.
- **Reference:** `afd_fleet/tests/integration_ledger_queue/ordering.rs` — order proven over a held transaction. Divergence: the held lock is the fleet row, taken by the admitting statement itself.

## Sections (implementation slices)

### §1 — The per-fleet budget goes

`FLEET_BACKLOG_BUDGET`, `Budgets::fleet_backlog`, `check_fleet_budget` and `fleet_budget.sql` are deleted. `BudgetScope` goes with them, because one scope is left: `OverBudget` carries the deployment ceiling's limit alone, and its log line keeps its event name. The deployment ceiling (`DEPLOYMENT_BACKLOG_BUDGET`, sampled through `select_backlog.sql`) still refuses new work at 100,000 unsettled admissions. Tests that set `fleet_backlog` move to `deployment_backlog`, or go where the deployment case is already covered. `datastore_scaling.md` drops its per-fleet budget sentence.

- **Dimension 1.1** — DONE — a fleet admits past the old cap → Test `a_fleet_admits_past_ten_thousand_unsettled` (integration): one `INSERT … SELECT generate_series` seeds 10,000 unsettled rows on one fleet; the next `admit` is accepted.
- **Dimension 1.2** — DONE — the default budgets name one ceiling → Test `budgets_default_to_the_deployment_ceiling` (unit, amended in `src/tests/mod.rs`): `Budgets::default().deployment_backlog` is `DEPLOYMENT_BACKLOG_BUDGET`.
- **Dimension 1.3** — DONE — an initial run refused at the ceiling still activates blocked → Test `an_initial_run_over_the_deployment_backlog_budget_activates_blocked` (integration, renamed in `stages.rs`): `deployment_backlog: 0` activates the fleet blocked, as `fleet_backlog: 0` did.
- **Dimension 1.4** — DONE — a replay passes a spent ceiling → Test `test_replay_bypasses_the_deployment_ceiling` (integration, renamed in `integration_steer_replay.rs`): with the ceiling spent, a resent `operation_id` answers 202 with its event.

### §2 — One statement admits, refuses or answers a repeat

`afd_admission/src/sql/admit_one.sql` replaces three statements and a transaction. The mechanism (SPEC-MECH):
- **State authority:** the fleet row for status, policy and per-fleet order; `uq_admissions_producer_producer_key` for the producer key; the bound deployment estimate for the ceiling.
- **Guarded transition:** the insert runs only on a found fleet with no visible repeat, the bound runnable status (when one is bound), a current policy marker and an estimate under the ceiling.
- **Commit before release:** the statement runs in autocommit; the fleet-row lock ends at its commit, after the insert.
- **Failure assumption:** a connection lost before commit leaves nothing. A reply lost after commit is answered by the producer key on retry.

The statement's steps are common table expressions (CTEs), each read by the next, so Postgres runs them in order:
1. **found** (`MATERIALIZED`): the fleet row by id and workspace, `FOR NO KEY UPDATE OF f`, with the head revision's policy marker.
2. **repeat** (`MATERIALIZED`): the row under `(producer, producer_key)`, read only when the fleet was found, so a foreign workspace never probes a key.
3. **inserted**: the insert, drawing from **found**, so its sequence is drawn under the lock. Its unconditional no-op `DO UPDATE` returns the row a racing same-key send committed (SQL-UPSERT).
4. **answer**: one row always: fleet found, status, policy current, the stored row (the repeat's or this insert's), whether this statement inserted it, and whether the ceiling was open.

`stage/verdict/` names the outcome in order: fleet absent, repeat, not runnable, policy, deployment ceiling, accepted. `admit` and the new `admit_runnable` take one pooled connection and send no `BEGIN`. `FleetAbsent` answers `UZ-AGT-009` and `NotRunnable { status }` answers `UZ-AGT-012`, each with its registry title; no code is new. Neither is counted or logged at warn by the ledger: before this spec the route refused both before the ledger saw them.

**Implementation default:** `FOR NO KEY UPDATE`, the lock intake takes today, now held for one statement. It serialises one fleet's admissions, so the sequence is drawn under it (§3), and it excludes revision installs and status changes. It does not conflict with `FOR KEY SHARE`, so foreign-key checks that record delivery proceed (`datastore_scaling.md`). **Implementation default:** the runnable spelling is bound from `FleetStatus::Active.as_str()`, because `is_runnable` accepts exactly that status (`afd_fleet_lifecycle/src/lib.rs`). The statement returns facts and Rust names the refusal, so the order is one typed function tested without Postgres, and the statement holds no verdict literals.

- **Dimension 2.1** — DONE — a new message costs one statement → Test `an_accepted_message_costs_one_statement` (integration, private database): after one warm-up send, ten `Steer::append` calls add exactly 10 top-level `pg_stat_statements` calls for the runtime role and no `BEGIN` or `COMMIT`. The private database keeps a parallel test's calls out of the count.
- **Dimension 2.2** — DONE — concurrent sends to one fleet each land once → Test `sixty_four_concurrent_sends_to_one_fleet_each_land_once` (integration): 64 unkeyed `Steer::append` calls on one active fleet, released from a barrier, answer 64 accepted; the ledger holds 64 rows with 64 distinct sequences.
- **Dimension 2.3** — DONE — a repeat costs one statement and writes nothing → Test `a_repeat_costs_one_statement_and_writes_nothing` (integration, private database): resending key K answers K's event with `replayed: true`, adds one top-level call, and leaves the fleet's row count unchanged.
- **Dimension 2.4** — DONE — racing repeats admit one row → Test `test_a_hundred_racing_repeats_admit_one_row` (integration, TEST-CONTEND): 100 sends of one key from a barrier leave one row; all 100 answers name the same event.
- **Dimension 2.5** — DONE — a revision install cannot slip under an admission → Test `an_admission_waiting_on_a_revision_install_never_lands_under_the_old_policy` (integration): a transaction holds the fleet row `FOR UPDATE` and makes a foreign-marker revision head; an admission blocks behind it; after the commit it is refused with `UZ-AUTHOR-009` and no row lands.
- **Dimension 2.6** — DONE — a foreign workspace records nothing → Test `an_invalid_fleet_scope_records_nothing_and_sends_no_notification` (integration, amended): the refusal is `FleetAbsent`; no admission row and no `SPUBLISH`.
- **Dimension 2.7** — DONE — the refusal order → Test `verdict_names_the_first_refusal_in_order` (unit): absent with a stored row answers absent; a stored row on a stopped fleet answers the repeat; stopped with a foreign marker answers not runnable; a foreign marker with the ceiling closed answers policy; a closed ceiling on a current fleet answers the deployment ceiling.
- **Dimension 2.8** — DONE — only `active` is runnable → Test `only_active_is_runnable` (unit): `FleetStatus::ALL` filtered by `is_runnable` is `[Active]`, so a second runnable status fails here before the bound spelling goes stale.
- **Dimension 2.9** — DONE — the deployment gate still refuses new work and answers a retry → Test `a_spent_deployment_budget_refuses_new_work_and_still_answers_a_retry` (integration, unchanged): with the estimate at its ceiling, a new key is refused over capacity, and the earlier key is answered with its event.

### §3 — The shared transaction and claim order hold under the fleet lock

`stage` binds no runnable status, so activation still admits while the fleet is `installing`: `afd_authoring/src/creation/activate.rs` stages before it flips the status. The insert and the fleet-row lock follow the caller's transaction, and the lock lasts until the caller commits. The insert draws its sequence only after the lock is held, and the lock lasts through commit. So within a fleet, commit order is sequence order, which the claim reads (`claim_admission_head.sql`, `ORDER BY a.sequence`). Slot 2140's comment naming the fleet-row lock stays true.

- **Dimension 3.1** — DONE — a waiter takes the later sequence → Test `a_send_queued_behind_the_fleet_lock_takes_the_later_sequence` (integration): a transaction stages an admission and stays open; a concurrent admission blocks (seen through `pg_blocking_pids`); the transaction commits. The waiter's sequence is higher, and the next claim takes the staged row. Drawing the sequence before the lock gives the waiter the lower one and fails this.
- **Dimension 3.2** — DONE — a rolled-back activation leaves nothing → Test `rollback_removes_staged_admission_without_a_notification` (integration, unchanged): no row and no notification.
- **Dimension 3.3** — DONE — a committed activation confirms once → Test `a_committed_stage_confirms_without_a_second_queue_record` (integration, unchanged).
- **Dimension 3.4** — DONE — activation admits on an installing fleet → Test `staging_admits_on_an_installing_fleet` (integration): on an `installing` fleet, `stage` admits, while `admit_runnable` refuses with `NotRunnable { status: Installing }`.

### §4 — The steer route admits with no status read

`Steer::append` calls `admit_runnable`. The route drops `ingress_status` and the stopped-fleet lookup: one append answers 202, `FleetAbsent` maps to the route's 404, and `NotRunnable` to its 409 with `current_state`. The statement answers a repeat before it checks status, so a retry on a stopped fleet is still answered. `Steer::replayed` and `FleetSteering::replayed` are deleted, with every caller (NLG). `repeat_despite` stays for one case only: a same-key send racing a spent deployment ceiling. The `steer` module notes change to say so.

- **Dimension 4.1** — the route behaves as before, end to end → Test `test_replay_bypasses_ingress_refusal` (e2e, router with live Postgres and Dragonfly): a saved fleet answers 202; after stop, the retry answers 202 with the same event; a changed message 409 `UZ-AGT-016`; a fresh id 409 `UZ-AGT-012`; a keyless send 409.
- **Dimension 4.2** — a foreign fleet is a 404 → Test `test_foreign_fleet_replay_is_not_found` (e2e): another workspace's id and message under the owner's fleet answers 404 `UZ-AGT-009`.
- **Dimension 4.3** — no status read precedes the admission → Test `a_steer_reads_no_fleet_status_before_admitting` (integration, `afd_api` harness): with the fleets store unreachable and the steering ledger live over an active fleet, a steer answers 202. The parent commit answers 503, because it read the status first.
- **Dimension 4.4** — a stopped fleet's 409 names its state → Test `a_stopped_fleet_refusal_names_its_status` (unit): `NotRunnable { status: Paused }` renders 409, `UZ-AGT-012`, `current_state: "paused"`.
- **Dimension 4.5** — a stopped-fleet refusal is not an admission failure → Test `a_stopped_fleet_refusal_counts_no_admission` (integration): `admit_runnable` on a stopped fleet adds nothing to `agentsfleet_admissions_total` and logs no `admission_failed`.
- **Dimension 4.6** — an outage still answers 503 → Test `a_well_formed_steer_reaches_the_store_and_reports_the_outage` (unit, router over unreachable stores, comment amended): every well-formed steer answers 503 `UZ-INTERNAL-001`, now from the ledger statement instead of the status read.

### §5 — Statements per message and one fleet's intake, measured

On Indy's laptop, `make bench-steer PROFILE=rig` runs three times on the base revision and three times on the branch, interleaved. The median branch run is copied to `bench/baselines/steer.rig.json`. The one-fleet shape, `BENCH_FLEETS=1 BENCH_CONCURRENCY=16 BENCH_WINDOW_SECONDS=3`, runs the same way, with `failures` 0 so the ceiling never refuses. `make bench-lease PROFILE=rig BENCH_SHAPE=burst` runs once on the branch. `push_scheduler.md` §M242 records every value beside M241's baselines, and `datastore_scaling.md` and `scaling.md` change with them. **Implementation default:** `bench-steer` is the gate because its lane appends through `Steer::append`. No trigger runs inside the statement, so the gate reads at most 1.05.

- **Dimension 5.1** — statements per steer halve → Test `make bench-steer PROFILE=rig` (integration): `postgres_statements_per_steer` is at most 1.05 in the committed baseline, from about 4.0 on the base.
- **Dimension 5.2** — the numbers are median real runs → Test `intake_numbers_recorded` (manual): §M242 lists three base and three branch values for rate, p95 and statements per steer in both shapes, plus the burst's `burst_append_p95_ms` and `burst_refused`; the committed file holds the median run.
- **Dimension 5.3** — the pages read clean → Test `orly gate verify` (integration): 0 `docs.language` findings on the four changed architecture pages.

### §6 — Intake runs under the login role's bounds

`afd_db::login` holds `LOCK_TIMEOUT` (`2000ms`) and `STATEMENT_TIMEOUT` (`5000ms`), the values the billing ledger and the admin bounds use. Postgres applies a role's settings only to the role that logs in, so each API login gets them: `ALTER ROLE <login> SET lock_timeout = …, statement_timeout = …`. The test lane's `_api_login` sets them, and the region-move playbook sets them after `pscale role create`. A transaction that sets its own bounds with `set_config` still wins, so billing and task reads keep theirs.

**Implementation default:** the login role rather than pool startup options, which Indy chose (Discovery). Pool startup options were also considered; unverified: whether the production PgBouncers pass them through.

- **Dimension 6.1** — DONE — the API pool carries both bounds → Test `the_api_login_runs_under_the_intake_bounds` (integration): `SHOW lock_timeout` and `SHOW statement_timeout` over the API pool equal `LOCK_TIMEOUT` and `STATEMENT_TIMEOUT`.
- **Dimension 6.2** — DONE — a held fleet row fails one send, not every caller → Test `a_send_behind_a_held_fleet_row_times_out` (integration): a transaction holds a fleet row `FOR UPDATE` past `LOCK_TIMEOUT`; a send answers 503 `UZ-INTERNAL-001` with SQLSTATE `55P03` logged, and records nothing.

## Interfaces

- **`afd_admission`:** `Admissions::admit(Admission) -> Result<Admitted>` keeps its signature and may now refuse `FleetAbsent`. New `Admissions::admit_runnable(Admission) -> Result<Admitted>` also refuses `NotRunnable` unless the key is a repeat. `prepare` and `stage` keep their signatures; `stage` binds no status. `Budgets` loses `fleet_backlog`, `BudgetScope` is removed, and `OverBudget` carries the deployment ceiling's limit. `Error::is_fleet_absent() -> bool` and `Error::not_runnable() -> Option<FleetStatus>` are new; a status spelling this build cannot parse refuses as an unreadable row (500), as `ingress_status` does today. `check_fleet_budget` is removed.
- **`afd_events`:** `Steer::append` keeps its signature. `Error::is_fleet_absent` and `Error::not_runnable` forward the ledger's. `Steer::replayed` is removed, and `FleetSteering::replayed` with it.
- **HTTP:** `POST /v1/workspaces/{workspace_id}/fleets/{fleet_id}/messages` is unchanged in paths, bodies, codes and `current_state` values; `public/openapi.json` does not change.
- **Login roles:** each API login carries `lock_timeout` and `statement_timeout` from `afd_db::login`; provisioning sets them (§6).
- **Schema:** none.
- **Bench:** `steer.rig.json` gains `postgres_statements_per_steer` (the current lane writes it; the committed file predates it).

## Failure Modes

| Mode | Kind | Cause | Handling (response and what the caller observes) | Test |
|------|------|-------|--------------------------------------------------|------|
| Runaway producer on one fleet | capacity | a webhook loop or script floods one fleet | admitted until the deployment ceiling refuses; then 503 `UZ-INTERNAL-001` for new work everywhere | `a_fleet_admits_past_ten_thousand_unsettled` |
| Same-key race | race | two sends with one key at once | one row; both answered with its event | `test_a_hundred_racing_repeats_admit_one_row` |
| Deployment ceiling spent | capacity | estimate at 100,000 | new work 503; a repeat still answered | `a_spent_deployment_budget_refuses_new_work_and_still_answers_a_retry` |
| Fleet stopped or paused | failure | send after stop | 409 `UZ-AGT-012` with `current_state`; a repeat answered 202 | `test_replay_bypasses_ingress_refusal` |
| Foreign workspace | security | an id under another tenant's fleet | 404 `UZ-AGT-009` before any key is read | `test_foreign_fleet_replay_is_not_found` |
| Revision install races a send | race | install holds the fleet row | the send waits, then `UZ-AUTHOR-009`; nothing recorded | `an_admission_waiting_on_a_revision_install_never_lands_under_the_old_policy` |
| Sequence drawn out of order | race | a send queued behind the fleet lock | the waiter takes the later sequence; claim order holds | `a_send_queued_behind_the_fleet_lock_takes_the_later_sequence` |
| Activation rolls back | failure | the caller's transaction fails | no row, no wake | `rollback_removes_staged_admission_without_a_notification` |
| Fleet row held past the bound | capacity | a stalled holder of the row | that send 503 `UZ-INTERNAL-001`, `55P03` logged, nothing recorded | `a_send_behind_a_held_fleet_row_times_out` |
| API statement over 5 s | capacity | a slow query on the API login | `57014`; the request fails instead of holding a connection | `the_api_login_runs_under_the_intake_bounds` |
| Datastore unavailable | dependency | pool cannot connect | 503 `UZ-INTERNAL-001`, nothing recorded | `a_well_formed_steer_reaches_the_store_and_reports_the_outage` |

## Invariants

1. No admission lands without holding its fleet row from before its sequence through its commit. Authority: the insert step draws from the locked **found** step. Counterexample: Dimension 3.1.
2. A producer key admits one row. Authority: `uq_admissions_producer_producer_key` and the no-op `DO UPDATE`. Counterexample: Dimension 2.4.
3. Ownership is decided before any producer key is read. Authority: the repeat step runs only on a found fleet. Counterexample: Dimension 4.2.
4. The bound runnable spelling is the only status `is_runnable` accepts. Authority: `only_active_is_runnable` (Dimension 2.8).

## Metrics & Observability

| Metric / event | Audience | Fires when | Properties allowed | Privacy guard | Test |
|----------------|----------|------------|--------------------|---------------|------|
| `agentsfleet_admissions_total` | operator | unchanged label set (`accepted`, `replayed`, `refused`, `over_budget`); `over_budget` now means the deployment ceiling only; a steer refused as stopped or foreign is not counted, as before | outcome | no identifiers or message text | `a_stopped_fleet_refusal_counts_no_admission` |
| `postgres_statements_per_steer` (bench key) | operator | each `bench-steer` run | number, revision | none | `make bench-steer PROFILE=rig` |

No event name is added or renamed.

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | An accepted message costs one top-level statement (§2) | `make test-integration-rustd` | output holds `an_accepted_message_costs_one_statement ... ok` | P0 | |
| R2 | 64 concurrent sends to one fleet each land once (§2) | `make test-integration-rustd` | output holds `sixty_four_concurrent_sends_to_one_fleet_each_land_once ... ok` | P0 | |
| R3 | Claim order equals sequence order (§3) | `make test-integration-rustd` | output holds `a_send_queued_behind_the_fleet_lock_takes_the_later_sequence ... ok` | P0 | |
| R4 | No per-fleet cap refuses a fleet (§1) | `make test-integration-rustd` | output holds `a_fleet_admits_past_ten_thousand_unsettled ... ok` | P0 | |
| R5 | The route reads no status before admitting (§4) | `git grep -n ingress_status -- rustd/crates/afd_api_tenant/src/handler/fleet/message_steer` | no output | P0 | |
| R6 | The folded statements and the per-fleet budget are gone (§1, §2) | `git grep -n -w -e SELECT_REPEATED_ON_FLEET -e SELECT_INSTALLED_POLICY -e FLEET_BUDGET -e INSERT_ADMISSION -e check_fleet_budget -e FLEET_BACKLOG_BUDGET -e fleet_backlog -e BudgetScope -- rustd` | no output | P0 | |
| R7 | Statements per steer drop to one on the rig (§5) | `jq -e '.measurements.postgres_statements_per_steer <= 1.05' bench/baselines/steer.rig.json` | `true`, exit 0 | P1 | |
| R8 | The changed pages read clean (§5) | `orly gate verify` | exit 0 | P1 | |
| R9 | The API login runs under the intake bounds (§6) | `make test-integration-rustd` | output holds `the_api_login_runs_under_the_intake_bounds ... ok` | P0 | |
| S1 | Lint | `make lint-all` | exit 0 | P0 | |
| S2 | Unit | `make test-unit-all` | exit 0 | P0 | |
| S3 | Integration | `make test-integration-rustd` | exit 0 | P0 | |
| S4 | Version | `make check-version` | exit 0 | P0 | |
| S5 | No secrets | `gitleaks detect --redact --no-banner` | exit 0 | P0 | |

## Dead Code Sweep

| Deleted file or symbol | Verify | Expected |
|------------------------|--------|----------|
| the four folded statements, `check_fleet_budget`, `FLEET_BACKLOG_BUDGET`, `Budgets::fleet_backlog`, `BudgetScope` | `git grep -n -w -e SELECT_REPEATED_ON_FLEET -e SELECT_INSTALLED_POLICY -e FLEET_BUDGET -e INSERT_ADMISSION -e check_fleet_budget -e FLEET_BACKLOG_BUDGET -e fleet_backlog -e BudgetScope -- rustd` | no output |
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
2. **Preserved user behaviour** — retries with `operation_id`, 404 for a foreign fleet, 409 for a stopped one or a changed message, and 503 when the deployment is full all answer as before; no fleet is refused for its own backlog.
3. **Optimal-way check** — one statement per message is the floor short of batching sends, which the brief cut until measured.
4. **Rebuild-vs-iterate** — iterate: the ledger stays the queue; only how a row gets in changes.
5. **What we build** — the one statement, the per-fleet budget removed, the route without its status read, the intake bounds, and the counter proofs.
6. **What we do NOT build** — group commit (unmeasured); a daemon-side fleet owner (no measured need); other producers' status reads (outside the steer path).
7. **Fit with existing features** — M242_002 extends `admit_one.sql` with disposition facts. It must not destabilise activation's first run, which stages on its own transaction.
8. **Surface order** — API only; the CLI and the web app see the same responses.
9. **Dashboard restraint** — no new screen; `agentsfleet_admission_backlog` stays the operator's gauge.
10. **Confused-user next step** — a 503 `UZ-INTERNAL-001` says try later, and resending with the same `operation_id` is safe, as the route's description says.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** the per-fleet budget first, then the statement, then the activation and order proofs, then the route, the measurement and the bounds. No slot changes, so the daemon deploys alone.
- **Alternatives considered:**
  - A trigger-kept per-fleet counter row (`fleet.admission_backlogs`): built at CHORE(open), then dropped with the per-fleet budget it enforced (Indy, Discovery).
  - A per-fleet advisory lock: rejected, because the fleet row already serialises a fleet, and an advisory lock taken before it could deadlock against activation.
  - A row count under the lock inside one statement: rejected, because the count reads the snapshot taken before the lock wait.
  - Serializable intake: rejected, because it retries exactly the contended burst.
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
  - `docker-compose.yml` runs `postgres:19rc1-alpine` with `pg_stat_statements.track=all`; `rustd/Cargo.lock` pins sqlx 0.9.0. Dimensions 2.2, 2.5 and 3.1 rerun when either, or the production Postgres major, changes (unverified: that major).
  - Risk: one fleet admits one message per statement's lock hold. §5's one-fleet run (16 concurrent senders) measures it.
- **Owner decisions** —
  > Indy (Nov 06, 2026: 08:50 AM): "And we are not in production yet, so i want to rethink the approach on admissions, lease, and so on as i am focussed on scale. If something fails in one host bare metal, it should automatically cordon and respawn in another runner" — context: rebuilding intake before production.
  > Indy (Nov 06, 2026: 10:43 AM): "1. Yes placement rules are via the tags and labels 2. Respawn across ownership - this is a tenant setting ... Based on the scope i would like to build this ... 4. Yes, but donot overengineer now, we can improve this." — context: one table and one statement, no group commit or fleet owner.
  > Indy (Nov 06, 2026: 11:57 AM): "Yes i am ok with postgres store and accepting messages, fences runs" and "The message has to wait. I assume you show message like compacting" and "assume atleast 50 fleets" — context: Postgres stays the acceptance store.
  > Indy (Nov 06, 2026: 02:13 PM): "And i prefer we create all the spects and close M241 and push a PR?" — context: this spec lands with M241.
  > Indy (Nov 06, 2026: 04:06 PM): chose "On the login role (Recommended)" for the intake timeout — context: `lock_timeout` and `statement_timeout` are set once on the API's Postgres login role at provisioning, from named constants a test reads back through the pool; zero extra round trips. The provisioning change is his approved deploy edit.
  > Indy (Nov 06, 2026: 04:06 PM): chose "Settle them (Recommended)" for #803's obsolete continuations, noting "we are not in production, i donot care about older tasks, so we could even cleanup all the fleeets and test." — context: an obsolete leased continuation is settled when its task generation advances, so the trigger-kept count stays exact; the backfill needs no care for pre-existing tasks, and dev data may be reset.
  > Indy (Nov 06, 2026: 09:50 PM): chose "Amend, skip EGM" — context: the spec is amended at CHORE(open) for #803 and builds without the Elephant-Goldfish Model (EGM) review.
  > Indy (Nov 06, 2026: 09:52 PM): chose "Settle, accept the loss" (superseded at 10:29 PM: with no per-fleet count, obsolete continuations stay as #803 left them), then "but i thought this is changing in the subsequent? on renew, if so why poke around this" — context: an issued obsolete continuation settles when its generation advances; its late report is refused, so up to one renewal interval of its run time goes unbilled. Release with a marker column was declined as not worth the column before production.
  > Indy (Nov 06, 2026: 10:14 PM): "If  you yes drop that, what other crap assumptions have you made like this" — context: the counter row's backfill was dropped; superseded by the next two quotes, which drop the counter row itself.
  > Indy (Nov 06, 2026: 10:22 PM): "We are not in production yet, and i can wipe out the db and start ov er in dev, you can even rewrite the existing sql files thought the rule prohibits, but i give you signoff on that, and you are free to nuke the sql entirely as well from schema/*" — context: SCH's frozen-slot rule is waived for this programme; dev data may be wiped at deploy.
  > Indy (Nov 06, 2026: 10:29 PM): "what is this 10K?", then chose "Drop it (Recommended)" — context: `FLEET_BACKLOG_BUDGET` arrived in `806deed0c` (M227) with no recorded reason; only a runaway producer reaches it. The per-fleet budget, the counter row, its triggers and the obsolete-continuation change go; the deployment ceiling stays.
  > Indy (Nov 06, 2026: 11:19 PM): "6 OK" — context: signed off §6's `ALTER ROLE <api_login> SET lock_timeout = '2000ms'` and `SET statement_timeout = '5000ms'`, run by `_api_login` on the test lane and by the region-move playbook after `pscale role create`; the bounds apply to every request-path statement on the API login, not background (`Default`) or `Migrator`.
  > Indy (Nov 06, 2026: 11:41 PM): "I need both PRs to be sent out and upon sending them babysit them, CI jobs green, greptile threads resolved. if you did touch th existing sql files deelte or edit then Indy will eyeball the PR and merge or else you are free to merge." — context: shipping and merge rule. This PR deletes and edits existing statements, so Indy merges it; a PR touching no SQL may be merged once green with threads resolved.
  > Indy (Nov 06, 2026: 11:41 PM): chose "agentsfleet + docs (Recommended)" — context: the two PRs are this agentsfleet PR (M241, M242_001, M242_002) and the docs-repository PR for M242_002's public changes, whose API pages are committed after this PR merges.
  > Indy (Nov 06, 2026: 11:41 PM): chose "Grant override (Recommended)" — context: an `Orly-Override` trailer for `orly gate pr`'s `telemetry_boot` criterion only, which fails on macOS for want of `/proc`; Linux CI runs it. Every other criterion must pass.
  > Indy (Nov 06, 2026: 11:41 PM): "Yes i am allowing you to write and commit. In you final PR merge status report, in the claude let me be the last message there with all the sqls for me to review, so indy can review in the morning on any changed if you did." — context: M242_002's chat-history SQL may be written and committed without prior review; the final status report ends with every product SQL change in the PR for his morning review.
  > Indy (Nov 06, 2026: 11:42 PM): chose "Reply + resolve (Recommended)" — context: a Greptile comment that conflicts with a recorded decision gets a reply citing the Discovery quote and is resolved; real bugs are fixed, tested and resolved; every disagreement is listed in the final report.
  > Indy (Nov 06, 2026: 11:42 PM): chose "Fix whatever is red" — context: any failing CI check is fixed in this PR, related to its diff or not, so the PR lands green.
  > Indy (Nov 06, 2026: 11:42 PM): chose "Minor, 0.74.0 (Recommended)" — context: if the release check demands a bump, 0.73.0 becomes 0.74.0 with a changelog entry naming the removed per-fleet cap and the new disposition.
  > Indy (Nov 06, 2026: 11:42 PM): chose "Wait for both" — context: the PR never splits; if M242_002 is blocked on a decision only Indy can make, no PR goes out until he answers.
  > Indy (Nov 06, 2026: 11:05 PM): "OK to keep ours" — context: signed off `admit_one.sql` (four steps: lock the fleet row, read an earlier send of the key, insert behind the gates, answer one row) over a variant that folded the key lookup into the lock step; reviewed line by line.
  > Indy (Nov 06, 2026: 04:18 PM): "yes" — context: blanket approval, asked as "Should each milestone drop the tables and columns it leaves with no reader or writer?": a milestone drops, in a new slot in its own Pull Request, any table, column or index it leaves with no runtime reader or writer (`.orly/standards/domains/sql.md` destructive-change approval).
- **Model reviews** — skipped on Indy's call (Owner decisions, 09:50 PM).
- **Open blockers** — none; Indy answered both on Nov 06 (above).
- **Deferrals and transfers** — none. Each one needs an owner quote below, as `> Owner (<name>, <MMM DD, YYYY HH:MM>): "<quote>" — context: <item, why>`.
