<!--
SPEC AUTHORING RULES (the one comment that survives):
- The spec standard (spec.md) holds every rule. Each tpl: comment names the
  rules that govern its section; delete it once the section is filled.
- Body order is the implementing agent's read order.
- No time or effort estimates, owners or implementation dates (SPEC-ESTIMATE).
-->

# M240_001: A lease poll claims the oldest ready fleet in one statement, and a refused event writes once

**Prototype:** v2.0.0

**Milestone:** M240

**Workstream:** 001

**Date:** Nov 05, 2026

**Status:** PENDING

**Priority:** P1, a chat on an idle fleet pays about 33 statements over about 15 pool acquires before its lease exists, and one admission wakes every idle host, each of which scans every active fleet

**Categories:** API, DOCS, INFRA, OBS

**Batch:** B1, the lease path; M240_002 (billing) is B2 and builds on §5's in-transaction hold seam

**Depends on:** M233_002 (the delivery span and stage attributes this spec's counters are read beside); M225_001 (the doorbell this spec narrows, shipped)

**Provenance:** LLM-drafted (Claude Fable 5.1, Nov 05, 2026) from a five-lens source review of `afd_fleet`, `afd_dragonfly`, `afd_billing`, `afr_supervisor` and `afd_admission` on `main` at `65372f505`; owner decisions quoted in Discovery

**Canonical architecture:** `docs/architecture/lease_flow.md` §1, `docs/architecture/data_flow.md` §"One active lease per fleet", `docs/architecture/concurrency.md` §"Lock-invariant registry"

---

## Overview

**Goal (testable):** `test_one_admission_costs_one_claim_statement_and_wakes_one_worker`: with 64 idle workers held on heartbeats across two daemon replicas and one chat admission inserted, exactly one worker polls, its poll runs one claim statement and returns the lease, and the admission row carries `claimed_at` before `delivered_at`.

**Problem:** A person sends a chat message and waits. Today the daemon answers that message by scanning every active fleet on every poll (`select_ledger_candidates.sql`), opening a three-statement transaction per candidate, then a six-statement "received" transaction, then about twenty more statements of gates and issue across about fifteen pool acquires. One admission rings every held heartbeat on every replica (`afd_dragonfly/src/doorbell/mod.rs`, a `watch` every holder hears), so up to 64 workers per replica run that scan at once. A lost lease reply stalls the fleet for 30 s and counts against the runner as an expiry. The first money hold lands after the lease commits, so a payer with no credit runs unheld until the next renewal.

**Where the code stands:** M225_001 built a Dragonfly readiness index with a per-partition doorbell. M226 retired that index; `afd_fleet/src/lease/assign/mod.rs` now states "The admission is the queue" and every poll runs the Postgres candidate scan above, so M225's "zero statements on an idle poll" no longer describes the code. The doorbell that survived publishes a bare fleet id on `fleet:wake:{shard}` into a 64-entry in-process hint buffer, and a held heartbeat is the one held request per host. Today's measured cost per admitted lease, counted by the reviewers from the statements each step runs: about 33 statements over about 15 pool acquires, four write transactions and one wallet lock; an empty poll costs two statements plus three per blocked candidate.

**Three words this spec keeps apart.** A *sandbox-held fleet* is one a runner keeps a frozen sandbox for (`fleet_slots.hold_expires_at`, `HeldFleets` on the wire). A *held heartbeat* is a heartbeat request the daemon keeps open waiting for a ring. A *money hold* is a `billing.holds` row. "Affinity" below always means `fleet_slots.last_runner_id = this runner`, the previous placement.

**Solution summary:** `runner.fleet_slots` becomes the queue. A slot row exists for every active fleet and carries `ready_at`; admission arms it, the claim pops it with `FOR UPDATE SKIP LOCKED LIMIT 1`, and the same statement writes the received event row and stamps `claimed_at`. The gate pass then reads on one connection and writes nothing; a refusal writes one terminal statement. Issue places the first hold inside its transaction and records a poll nonce so a repeated poll returns the lease it already issued. The doorbell wakes one held heartbeat per ring and names the fleet. A person's chat on an idle fleet reaches a lease after three Postgres round trips of writes instead of about fifteen acquires, and one admission costs one claim statement across the whole runner estate.

**Design flow:**

```text
admit (afd_admission)  INSERT fleet.admissions ─┐ one transaction
                       UPDATE runner.fleet_slots SET ready_at ─┘ (arm, only when no live lease)
                       SPUBLISH fleet:wake:{shard} {fleet_id, event_id}          after commit

daemon hub             ring → ONE held heartbeat completes with woken_for = fleet_id

runner                 that host wakes the worker holding the fleet's sandbox, else one idle worker
                       POST /v1/runners/me/leases {poll_nonce, holds, accepted_kinds}

CLAIM (one statement,  ready slots ∩ labels,
 its own transaction)  ORDER BY sandbox_held_by_me DESC, fleet_id = hint DESC, last_runner_id = me DESC, random()
                       FOR UPDATE SKIP LOCKED LIMIT 1 → fence+1, lock head admission,
                       stamp lease_fence + claimed_at, expire prior lease, INSERT fleet.events
                       (status received, ON CONFLICT keeps the row), stamp delivered_at, clear ready_at

GATE PASS (one connection, AFTER the claim committed, so no row lock is held through decrypt)
                       payer → provider registration + envelope → estimate, balance, spend
                       → approval gate → vault names → grants       (executor-taking reads)
                       refusal: one statement sets the event terminal and settled_at, re-arms ready_at

ISSUE (one transaction) lock host → census → INSERT runner.leases (fenced, poll_nonce)
                       → choice record → INSERT billing.holds (reserve_on) → COMMIT
                       lock order: host → slot → admission → lease → wallet

reply                  200 { lease } ; a repeated poll with the same nonce answers the same lease
```

**Risks this spec carries:** a ring delivered to a host whose labels cannot take the fleet costs that fleet the 1 s poll (§3 picks an eligible beat, so this needs every held host ineligible); a runner on the previous release polling a new daemon keeps working, but a new runner polling an old daemon is refused by `deny_unknown_fields` on `LeaseRequest`, so the daemon deploys first (§5); a bigger claim statement needs its plan proven at fleet count (Dimension 2.5); the money hold keeps the wallet row lock inside issue until M240_002 replaces it.

**How the targets are measured:** `make bench-lease PROFILE=rig` already reports statements and commits per lease and idle statements per poll (`make/bench.mk`); its baseline file is compared with `make bench-compare LANE=lease`. The per-request counters are `PollCost` (`afd_fleet/src/lease/assign`) and the `pg_stat_statements` delta the integration tests read. Pass lines per verb: pull ≤ 12 statements, ≤ 3 pool acquires, 2 write transactions; empty poll 1 statement.

## PR Intent & comprehension handshake

- **PR title (eventual):** Claim the oldest ready fleet in one statement and hold money inside issue
- **Intent (one sentence):** A chat message on an idle fleet is leased after one claim statement and one issue transaction, one admission wakes one worker, and a lost lease reply or a short payer no longer costs the fleet 30 s.
- **Authoring handshake:** Indy asked for a review of leasing, admission, the doorbell, chat pickup and billing settlement "so we could improve the turn around time of the chat", then "if you were to do a large refactor and reduce the postgres sqls calls what would be your design", then "so we are performant at scale" and "and reliable at scale", then "Yes fold into a spec". ASSUMPTIONS I'M MAKING: 1. The per-worker long poll M225 rejected stays rejected; the held heartbeat remains the one held request per host. 2. Billing policy is its own spec (SPEC-BOUNDARY), so this spec adds only the in-transaction `reserve_on` seam and keeps the wallet row lock; M240_002 removes that lock. 3. No price-book or fleet-revision cache is added; `afd_billing/src/store.rs` records why a cache is an optimisation over a correct read, and one connection already removes the acquire cost. 4. Steer into a running lease belongs to M236 §2. 5. The runner API changes are additive (`poll_nonce` request field, `woken_for` response field), so they land inside `/v1`.
- **Implementer handshake:** recorded in the Pull Request Session Notes at PLAN, before any edit (SPEC-HANDSHAKE).

## Implementing agent — read these first

1. `rustd/crates/afd_fleet/src/lease/sql/lease/claim_admission_head.sql` — the statement §2 extends: slot lock, `MATERIALIZED` clock, fence bump, head lock, stamp, prior-lease expiry and tally already chain in one CTE; §2 adds the slot pop, the event insert and the readiness clear to that chain.
2. `rustd/crates/afd_fleet/src/lease/event.rs` — `record_received` and `redelivery_of`: the rows and the `Delivery::{First, Repeat, Terminal}` reading §2 folds into the claim; the module comment "Why the caller is not `issue`" is the rationale §4 replaces.
3. `rustd/crates/afd_billing/src/ledger/mod.rs` — `balance(executor, tenant)` is the executor-taking read pattern every §4 read mirrors, with its "nested acquire under load starves the pool" reason.
4. `rustd/crates/afd_fleet/src/lease/issue.rs` and `rustd/crates/afd_runner/src/capacity/mod.rs` — the issue transaction and the host lock §5 extends.
5. `rustd/crates/afd_dragonfly/src/doorbell/mod.rs` and `rustd/crates/agentsfleetd/tests/integration_runner_wake.rs` — the ring, the hold and the held-beat test §3 narrows.
6. `docs/v2/done/M225_001_P1_API_OBS_LEASE_PICKUP_WAKE_HINT.md` — the pickup design this spec supersedes in part; its Discovery records the long-poll rejection and "spec the one-queue refactor separately", which this spec is.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `docs/v2/pending/M240_001_P1_API_DOCS_INFRA_OBS_LEASE_QUEUE_ONE_STATEMENT_CLAIM.md` | CREATE then MOVE to `docs/v2/active/`, then `docs/v2/done/` | this spec |
| `schema/2190_runner_fleet_slots_ready.sql` | CREATE | §1: `ready_at` on `runner.fleet_slots`, partial index on ready slots, backfill of a slot row per active fleet and of `ready_at` where an unsettled admission waits |
| `schema/2200_fleet_admissions_claimed_at.sql` | CREATE | §2: `claimed_at` on `fleet.admissions` |
| `schema/2210_runner_leases_poll_nonce.sql` | CREATE | §5: `poll_nonce` on `runner.leases`, unique per runner |
| `rustd/crates/afd_db/src/migration.rs` | EDIT | registers the three slots |
| `rustd/crates/afd_authoring/src/creation/activate.rs` | EDIT | §1: activation inserts the fleet's slot row in the activating transaction |
| `rustd/crates/afd_admission/src/sql/insert_admission.sql`, `rustd/crates/afd_admission/src/stage/insert.rs` | EDIT | §1: the admission statement arms the slot in its own CTE |
| `rustd/crates/afd_admission/src/wake.rs`, `rustd/crates/afd_dragonfly/src/doorbell/mod.rs` | EDIT | §3: ring payload carries fleet and event ids; one held beat wakes per ring; hint peek and consume-on-claim |
| `rustd/crates/afd_fleet/src/lease/sql/lease/claim_admission_head.sql` | EDIT | §2: the queue pop, event insert, `claimed_at`, `delivered_at` and readiness clear join the chain |
| `rustd/crates/afd_fleet/src/lease/sql/lease/select_ledger_candidates.sql`, `stamp_delivery.sql`, `claim_fleet_slot.sql` | DELETE | §2: the scan, the stamp and the unused slot claim are folded or dead |
| `rustd/crates/afd_fleet/src/lease/sql/lease/release_fleet_slot.sql`, `release_unleased_slot.sql`, `rustd/crates/afd_runner/src/sql/sweep/expire_active_lease_slots.sql` | EDIT | §1: a release or expiry re-arms `ready_at` when an unsettled, unparked admission remains |
| `rustd/crates/afd_fleet/src/lease/sql/queue.rs`, `rustd/crates/afd_fleet/src/lease/sql/lease/mod.rs` | EDIT | statement registry follows the deletions |
| `rustd/crates/afd_fleet/src/lease/assign/ledger.rs`, `assign/mod.rs`, `assign/measured.rs` | EDIT | §2: one statement, no candidate loop; one `PollCost`; real Dragonfly count |
| `rustd/crates/afd_fleet/src/lease/event.rs` | EDIT | §2: `record_received` and `redelivery_of` go; the claim row carries the delivery reading |
| `rustd/crates/afd_fleet/src/lease/affinity.rs` | EDIT | §2: `Leases::claim` and `Fence::as_u64`'s silent zero go |
| `rustd/crates/afd_fleet/src/lease/pull/mod.rs`, `pull/held.rs`, `pull/claimed.rs`, `pull/refuse.rs`, `pull/poll.rs` | EDIT | §4: gate pass on one connection; refusal writes once; `Step<T>` replaced by `ControlFlow` |
| `rustd/crates/afd_fleet/src/lease/pull/step.rs` | DELETE | §4: `std::ops::ControlFlow` |
| `rustd/crates/afd_fleet/src/lease/admit/gates.rs`, `admit/mod.rs` | EDIT | §4: reads take the pass's connection |
| `rustd/crates/afd_billing/src/store.rs`, `rustd/crates/afd_billing/src/ledger/mod.rs`, `ledger/reserve.rs` | EDIT | §4, §5: `Accounts` reads and `reserve_on` take an executor |
| `rustd/crates/afd_credential/src/provider/runtime/resolve.rs`, `rustd/crates/afd_credential/src/vault/mod.rs` | EDIT | §4: resolve and declared-names reads take an executor |
| `rustd/crates/afd_gate/src/gate/store/mod.rs`, `rustd/crates/afd_gate/src/gate/grants.rs` | EDIT | §4: recorded-gate and grants reads take an executor |
| `rustd/crates/afd_fleet/src/lease/deliver/mod.rs`, `deliver/queue_wait.rs` | EDIT | §4, §6: delivery on the pass connection; queue wait from `claimed_at` |
| `rustd/crates/afd_fleet/src/lease/issue.rs`, `rustd/crates/afd_fleet/src/lease/money.rs` | EDIT | §5: hold inside the issue transaction; `hold_issued` after commit goes; `Lost` told apart from `Denied` |
| `rustd/crates/afd_wire/src/lease/request.rs`, `rustd/crates/afd_wire/src/runner/mod.rs` | EDIT | §3, §5: `poll_nonce` request field, `woken_for` response field, both optional |
| `public/openapi.json` | EDIT | regenerated from `rustd/` with `--no-banner` |
| `rustd/crates/afd_api_runner/src/handler/runner/lease/mod.rs`, `heartbeat.rs`, `wake.rs` | EDIT | §3, §5: nonce to the plane; fleet id on the woken reply |
| `rustd/crates/afr_supervisor/src/heartbeat/mod.rs`, `worker_pool/mod.rs`, `client/lease_verbs/mod.rs` | EDIT | §3, §5: wake the worker holding `woken_for`; one nonce per poll, kept across a retry of that poll |
| `rustd/crates/afd_observability/src/producers/fleet/mod.rs`, `rustd/crates/afd_observability/src/metrics/label/fleet.rs` | EDIT | §6: `lease_polled` labels, `wake_to_claim` histogram, `lease_issue_denied` |
| `rustd/crates/agentsfleetd/tests/integration_lease_queue.rs` | CREATE | §2, §5: contention, lost-reply replay and SIGKILL reclaim against the live stack |
| `rustd/crates/agentsfleetd/tests/integration_runner_wake.rs`, `rustd/crates/afd_fleet/tests/integration_lease_claim_race.rs`, `rustd/crates/afd_fleet/src/lease/pull/tests.rs`, `rustd/crates/afd_fleet/src/lease/admit/tests.rs`, `rustd/crates/afd_dragonfly/src/doorbell/tests.rs`, `rustd/crates/afr_supervisor/src/heartbeat/tests.rs` | EDIT | tests follow the mechanisms they prove |
| `docs/architecture/lease_flow.md`, `docs/architecture/data_flow.md`, `docs/architecture/runner_fleet.md`, `docs/architecture/scaling.md` | EDIT | the queue, the one-statement claim, the targeted ring and the statement budget |
| `docs/architecture/concurrency.md` | EDIT | lock-invariant registry gains the pinned order host → slot → admission → lease → wallet |
| `~/Projects/docs` branch `chore/m240-lease-queue-changelog` | EDIT (other repository) | runner API reference and changelog for the two additive fields; committed after this Pull Request merges |

## Applicable Rules

- **`domains/sql.md`** — SQL-SLOT, SQL-STAMP, SQL-LOCKCLOCK, SQL-SWEEPCLOCK, SQL-REPO, SQL-COUNTER, SQL-ONEFACT, SQL-UPSERT, IDMP, TXN, SQL-TIMEOUT, SQL-PLAN
- **`languages/rust.md`** — ERR-RS, RS-FROM, RS-MAPERR, RS-SOURCE, RS-NEWTYPE, RS-BIND, FN-RS, RS-RACE
- **`code.md`** — NDC, NLR, NLG, CODE-SEAM, OWN, FLL, CODE-MSID
- **`tests.md`** — TEST-REAL, TEST-CONTEND, TEST-ACKLOSS, TEST-KILL, TEST-INJECT, TEST-COUNTER, TEST-DELTA, TEST-LANE, TEST-RACE
- **`domains/logging.md`** — LOG-EVENT, LOG-FIELDS, LOG-HOT, OBS
- **`domains/http.md`, `domains/http-surface.md`** — HTTP-GARDE, HTTP-VALID, HTTP-OPENAPI, HTTP-ANNOTATE, HTTP-BREAK, HTTP-STABILITY
- **`docs.md`** — DOC-02, DOC-05, DOC-10, DOC-14b on every architecture page edited

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| `make lint-all` (clippy pedantic, rustfmt, `disallowed_methods`) | yes, every edited crate | no new `#[expect]`; `ControlFlow` replaces `Step` so no new helper enum |
| `test_coverage_gate_rust_source` and `test_documented_codes_match_refusals` | yes, two wire fields | fields documented at the `utoipa` annotation and the wire doc comment; `public/openapi.json` regenerated in the same commit |
| `docs.language` on changed architecture prose | yes | short sentences, DOC-05 plain words, no new em dashes |
| gitleaks pre-commit | no secrets in scope | `gitleaks detect --redact --no-banner` before every commit |
| File & Function Length (file ≤350 lines, function ≤50, method ≤70) | yes: `claim_admission_head.sql` grows; `pull/mod.rs` and `deliver/mod.rs` are near the cap | SQL files are exempt by extension; the gate pass moves to `pull/gate_pass.rs` and the issue reply rendering to `deliver/render.rs` |
| `test_route_inventory` and `route_meta_total` | no, no new route | unchanged |

## Prior-Art / Reference Implementations

- **Reference:** `rustd/crates/afd_fleet/src/lease/sql/lease/claim_admission_head.sql` — the fenced CTE chain to extend, not replace; `meter_lease.sql` for a chain that both reads under lock and settles in one statement.
- **Reference:** `rustd/crates/afd_authoring/src/lease/claim/mod.rs` — the setup chat claim that walks drafts with `NOWAIT` savepoints; this spec diverges by letting `SKIP LOCKED LIMIT 1` pick the row, so no walk and no savepoint.
- **Reference:** `rustd/crates/afd_billing/src/ledger/mod.rs` `balance(executor, …)` — the executor-taking read every §4 function mirrors.
- **Reference:** `rustd/crates/afd_fleet/src/lease/sql/lease/insert_lease_with_event.sql` — the fenced insert §5 adds `poll_nonce` to; its `ON CONFLICT` shape is the one `poll_nonce` reuses.
- **Divergence from M225_001:** the readiness directory in Dragonfly was replaced by a Postgres scan ("The admission is the queue", `afd_fleet/src/lease/assign/mod.rs`); this spec keeps Postgres as the one queue and makes the slot row its index.

## Sections (implementation slices)

### §1 — Every active fleet has a slot row, and admission arms it

Prerequisite: none. A slot row is the fleet's queue entry. Activation inserts it in the transaction that sets the fleet active (`afd_authoring/src/creation/activate.rs`), so the claim never needs the insert-or-update branch `claim_admission_head.sql` carries today. Slot 2190 adds `ready_at TIMESTAMPTZ(3)` and a partial index on `(ready_at) WHERE ready_at IS NOT NULL`, and backfills a row for every active fleet and `ready_at = created_at` of the oldest unsettled, unparked admission where one waits.

The admission statement (`insert_admission.sql`) gains a CTE that sets `ready_at = now()` on the fleet's slot when the inserted row is new and the slot's `lease_expires_at` is in the past, compared with `statement_timestamp()` (SQL-SWEEPCLOCK). A running fleet keeps `ready_at NULL`; its release re-arms it. `release_fleet_slot.sql`, `release_unleased_slot.sql` and `expire_active_lease_slots.sql` each set `ready_at` when an unsettled, unparked admission remains for the fleet, in the same statement that frees the slot (TXN). The park statement clears `ready_at`; resolving a gate re-arms it through the same release path `afd_approval` already calls. **Implementation default:** the arm lives in the admission's own statement rather than in a second call, because an admission that commits without its arm is a lost wake, and the backstop for a lost arm is the sweep, not the next poll.

- **Dimension 1.1** — activation creates the slot → Test `test_activated_fleet_has_a_slot_row` (integration): activate a fleet with start `none`; `runner.fleet_slots` has its row with `ready_at NULL`.
- **Dimension 1.2** — a new admission arms a free slot and leaves a busy one alone → Test `test_admission_arms_only_a_free_slot` (integration): admit on an idle fleet, `ready_at` is set; admit on a fleet with a live lease, `ready_at` stays `NULL`; the lease's report sets it.
- **Dimension 1.3** — a parked head disarms, resolution re-arms → Test `test_parked_head_disarms_and_resolution_rearms` (integration): an open approval gate parks the head and `ready_at` is `NULL`; the gate's approval sets it.
- **Dimension 1.4** — the backfill is exact → Test `test_ready_backfill_matches_unsettled_heads` (integration): seed three active fleets, one with an unsettled admission, run the migration; exactly that slot has `ready_at` equal to its head's `created_at`.
- **Dimension 1.5** — an expired runner's slots re-arm → Test `test_liveness_expiry_rearms_waiting_slots` (integration): a runner derived offline with two active leases; both slots are expired and re-armed by the sweep in one statement.

### §2 — One statement claims the oldest ready fleet and opens its event

Prerequisite: §1. `claim_admission_head.sql` stops taking a fleet id. Its first CTE selects from ready slots whose fleet's `required_tags` the runner's labels admit and whose hold rule (`hold_expires_at`, holder liveness) admits this runner, ordered by held fleets, the doorbell hint, `last_runner_id = runner`, then `random()`, with `FOR UPDATE OF s SKIP LOCKED LIMIT 1`. The clock is read once in a `MATERIALIZED` step after that lock (SQL-LOCKCLOCK). The rest of the chain is today's: fence bump, head admission lock with `SKIP LOCKED`, `lease_fence` stamp, prior-lease expiry and tally. Three writes join it: the `fleet.events` row today written by `record_received` (`afd_events::sql::INSERT_LEASED_FLEET_EVENT`, whose conflict arm already keeps a continuation's predecessor), `delivered_at` and the new `claimed_at` on the admission, and `ready_at = NULL` on the slot. The statement returns what `Acquired` and `Received` carry today, including the existing event row's status so `Delivery::{First, Repeat, Terminal}` is read from the same row set.

`assign/ledger.rs` becomes one `fetch_optional` on one connection; an empty poll is one statement that touches no row. `record_received`, `redelivery_of`, `STAMP_DELIVERY`, `SELECT_CANDIDATES`, `CLAIM_FLEET_SLOT` and `Leases::claim` go (NLR, NDC). `MAX_CANDIDATES_PER_POLL` goes with the scan. The fence semantics are unchanged and strictly tighter: the statement that bumps `fencing_token` writes the row that carries it, so `LOCK_ADMISSION` is no longer needed between claim and event, and stays for the writes after issue. **Implementation default:** `ORDER BY random()` stays as the last key, because M225 chose spreading runners over fleets and the ready set is small enough that the sort costs nothing measurable; SQL-PLAN evidence is Dimension 2.5.

- **Dimension 2.1** — one admission, one claim → Test `test_one_admission_costs_one_claim_statement_and_wakes_one_worker` (integration): the Goal; `lease_polled` counts one poll with `database_roundtrips = 1` for the claim, and the admission row has `claimed_at <= delivered_at`.
- **Dimension 2.2** — exactly one winner under contention → Test `test_hundred_pollers_claim_one_fleet_once` (integration, TEST-CONTEND): 100 polls released from a barrier against one ready fleet; one lease row, 99 empty answers, `fencing_token` advanced by one.
- **Dimension 2.3** — an empty poll is one statement → Test `test_empty_poll_runs_one_statement` (integration, TEST-COUNTER): no ready slot; `pg_stat_statements` delta for the poll is one claim statement and the request's authentication read.
- **Dimension 2.4** — a label-restricted fleet is never offered to a runner without the label → Test `test_claim_respects_required_tags` (integration): a ready fleet with `required_tags = {gpu}`; a runner without `gpu` claims nothing, one with it claims.
- **Dimension 2.5** — the ready index is used → Test `test_claim_plan_uses_ready_index` (integration, SQL-PLAN): 10 000 active fleets, 3 ready, `EXPLAIN (ANALYZE, BUFFERS)` after `ANALYZE` shows the partial index scan and no sequential scan of `fleet.fleets`.
- **Dimension 2.6** — a redelivery is read from the same row set → Test `test_reclaimed_admission_reports_repeat_delivery` (integration): a lease expires with its event `received`; the next claim answers `redelivered = true`, `Delivery::Repeat`, and no second event row.
- **Dimension 2.7** — a terminal event is settled, never re-run → Test `test_terminal_event_on_reclaim_is_settled_not_leased` (integration): the prior event is `processed`; the claim answers `Delivery::Terminal`, the admission settles and no lease is issued.

### §3 — One ring wakes one worker and names the fleet

Prerequisite: §2. The ring payload becomes `{fleet_id, event_id, published_at_ms}` (`afd_admission/src/wake.rs`, `doorbell::publish`). The `Rings` `watch` is replaced by a list of held beats behind a `tokio::sync::Semaphore` of `HELD_BEATS_MAX` permits. Each held beat registers its runner's labels and sandbox-held fleets, which the heartbeat body already carries. A ring completes exactly one beat: the oldest beat whose runner sandbox-holds the fleet, else the oldest beat whose labels admit the fleet's `required_tags`, else none. A ring that completes no beat is kept as a hint only, and the fleet is leased on the next 1 s poll. `wait_for_work` answers `Waited::Rung(fleet_id)`; the heartbeat reply gains `woken_for: Option<String>` beside `woken`, and `woken` keeps its type so a runner on the previous release decodes the reply unchanged. The runner (`afr_supervisor/src/heartbeat/mod.rs`) wakes the worker whose hold list contains `woken_for`, else one idle worker; a worker uses the id for nothing else, because the claim statement already prefers sandbox-held fleets.

The hint buffer stays a 64-entry in-process list, oldest evicted first, and becomes peek-and-consume: every poll on the replica peeks the oldest hint as its `ORDER BY` key, and `assign/ledger.rs` pops a hint only when the claimed fleet equals it. A hint for a fleet no runner on this replica can claim is evicted by age, never by a losing poll.

- **Dimension 3.1** — one ring, one beat → Test `test_a_ring_completes_exactly_one_held_beat` (unit): three held waiters, one `ring`; one returns `Rung(fleet)`, two still wait, a second ring releases one more.
- **Dimension 3.2** — the holder wakes → Test `test_woken_for_wakes_the_worker_holding_that_fleet` (unit, `afr_supervisor`): two idle workers, one holding fleet F; `woken_for = F` ends that worker's pause first.
- **Dimension 3.3** — a lost ring costs only the hold → Test `test_a_held_heartbeat_is_woken_by_admitted_work` (integration, existing, extended): the beat is woken with `woken_for` equal to the admitted fleet, and with the doorbell closed the worker still leases on the 1 s retry.
- **Dimension 3.4** — the hint survives a losing poll → Test `test_hint_is_kept_until_its_fleet_is_claimed` (unit): hint F, a claim that answers fleet G leaves F buffered; a claim of F consumes it.
- **Dimension 3.5** — the cap refuses, never blocks → Test `test_held_beats_past_the_cap_answer_capped` (unit): 65 waiters; the 65th returns `Capped` at once and the permit count never exceeds 64.

### §4 — The gate pass reads on one connection and a refusal writes once

Prerequisite: §2. The claim statement is its own transaction and has committed before the pass starts, so no slot or admission row lock is held while a secret is decrypted or a policy is assembled; the fence the claim stamped is what the pass carries. `pull/mod.rs` acquires one connection after the claim and passes it to every read: payer (`Accounts`), provider registration and secret envelope (`Providers::resolve_reference_on`), estimate, balance and fleet spend, the recorded approval gate (`afd_gate` store, Dragonfly first then the connection), declared vault names and approved integrations. Each becomes an executor-taking function in its own crate (SQL-REPO, CODE-SEAM); decrypt and policy assembly stay in process and off the connection. Nothing in the pass writes. A refusal runs one statement, `Leases::block` (`event.rs`) rewritten over one CTE chain: the event row's status becomes the refusal's terminal spelling with its detail, the admission's `settled_at` is set under its fence (`settle_terminal.sql`, extended to take the status), the slot's lease is released, and `ready_at` is set again if another unsettled, unparked admission waits. The Events tab shows `gate_blocked` on the same poll as today, because the one statement commits before the poll answers. The `event_received` tail frame publishes after the pass admits, so a refused event publishes only its ending. `Step<T>` and `proceed` are replaced by `std::ops::ControlFlow<Ending, T>`.

- **Dimension 4.1** — one acquire for the pass → Test `test_gate_pass_acquires_one_connection` (integration, TEST-COUNTER): a pool sized at two with one connection pinned; an admitted lease completes, proving the pass used one connection.
- **Dimension 4.2** — a refusal writes once and leaves nothing half-open → Test `test_refused_event_is_terminal_in_one_statement` (integration): a payer with no wallet row refused by the balance gate; the event is `gate_blocked`, the admission settled, no lease row, no `event_received` frame on the tail, one `event_complete` frame.
- **Dimension 4.3** — a datastore fault mid-pass leaves the claim reclaimable → Test `test_pass_fault_leaves_admission_claimable_after_ttl` (integration, TEST-INJECT): inject a refused statement at the grants read; the poll answers an error, the event stays `received`, and a poll after `LEASE_TTL` claims it as a redelivery.
- **Dimension 4.4** — the pass orders reads so a refusal costs no decrypt → Test `test_money_refusal_skips_provider_decrypt` (unit): a `Short` balance returns before `resolve_reference_on` is called, asserted through the recording providers double.

### §5 — Issue holds the money and answers a repeated poll with the same lease

Prerequisite: §4. `issue.rs` adds two writes to its transaction. First, `afd_billing::ledger::reserve_on(executor, ask)` places the first `billing.holds` row under the wallet row lock, after the host, slot, admission and lease rows, so the pinned order is host → slot → admission → lease → wallet; a `Short` answer rolls the transaction back, releases the slot and refuses the event with `BALANCE_EXHAUSTED`. `money.rs`'s post-commit `hold_issued` goes. This spec owns hold-at-issue; the hold still goes through `afd_billing` and is released by today's report release and billing sweep, so M240_001 merges and runs alone. M240_002 replaces the wallet row lock inside `reserve_on` with the tenant advisory lock and changes nothing in `issue.rs`.

Second, the poll nonce. Each worker mints a UUIDv7 per poll attempt and sends it as `LeaseRequest.poll_nonce` (optional, 36 bytes under garde). The worker keeps the same nonce while it retries that one poll after a transport failure, a client timeout (`CALL_TIMEOUT`) or a 5xx, and mints a fresh one for every poll it starts after an answer. The lease row records it, unique per `(runner_id, poll_nonce)`. A poll whose nonce matches an active lease of this runner answers that lease again, rendered from the lease row and the fleet's installed revision, before any claim; a nonce whose lease has expired or reported matches nothing and the poll claims normally. Workers never share a nonce, so a replay can only return the lease that worker's own poll issued. `LeaseRequest` carries `deny_unknown_fields`, so a daemon on the previous release refuses a poll carrying the field: the daemon deploys before the runners, which is the order every runner wire change in this repository already follows. `issue`'s `Ok(None)` splits into `Lost` (fence or ticket) and `Denied(capacity)`, each with its own event and counter.

- **Dimension 5.1** — the hold is in the lease's transaction → Test `test_issue_places_hold_before_commit` (integration): a lease is issued; `billing.holds` has its row in the same snapshot that sees the lease, and a transaction that fails at the choice record leaves no hold.
- **Dimension 5.2** — a short payer is refused before the lease exists → Test `test_short_payer_gets_no_lease_and_no_hold` (integration): balance below the floor with a wallet row; refusal `BALANCE_EXHAUSTED`, no lease, no hold, slot released and re-armed only if another admission waits.
- **Dimension 5.3** — a lost reply is replayed, not re-leased → Test `test_lost_lease_reply_is_answered_by_the_next_poll_with_the_same_nonce` (integration, TEST-ACKLOSS): issue commits, the response is dropped by the test harness, the runner re-polls with the same nonce; the same `lease_id` returns, one lease row, `expired_count` unchanged.
- **Dimension 5.4** — a different nonce after a lost reply does not double-lease → Test `test_fresh_nonce_after_lost_reply_finds_the_slot_busy` (integration): the re-poll with a new nonce answers no work, and the lease is reclaimed only after `LEASE_TTL`.
- **Dimension 5.5** — capacity denial is named → Test `test_capacity_denial_is_not_logged_as_claim_lost` (unit): a host at its worker count; the outcome is `Denied` with `lease_issue_denied{reason=capacity}` and no `lease_claim_lost` event.
- **Dimension 5.6** — the lock order holds under a racing renewal → Test `test_issue_and_renewal_never_deadlock` (integration, RS-RACE): 50 issues and 50 renewals of other leases of one payer released together; zero `40P01` errors, every statement under the declared `lock_timeout`.
- **Dimension 5.7** — a killed runner's lease is reclaimed once → Test `test_sigkill_mid_lease_reclaims_once_and_refuses_the_late_report` (integration, TEST-KILL): the runner process is killed after issue; after `LEASE_TTL` another worker claims the same admission as a redelivery, and a report with the dead fence is refused `RUN_STALE_FENCING_TOKEN`.

### §6 — The poll tells the truth about its cost and its wait

Prerequisite: §2, §3. `lease_polled` gains labels `hinted` and `woken`; `database_roundtrips` counts every statement the request ran, and `dragonfly_commands` counts the gate read and the publishes instead of the hard-coded zero in `assign/measured.rs`. A `wake_to_claim_seconds` histogram measures ring `published_at_ms` to the claim's `claimed_at`. `queue_wait_ms` on the issue log line is read from `claimed_at` minus `event_created_at`. These ride beside M233_002's delivery span attributes and add no fleet identity to any metric.

- **Dimension 6.1** — counts are real → Test `test_lease_polled_counts_every_statement` (unit, `measured`): a recorded poll with one claim and seven pass reads reports `database_roundtrips = 8` and `dragonfly_commands = 2`.
- **Dimension 6.2** — the wait is durable → Test `test_claimed_at_is_stamped_once_before_delivered_at` (integration): the claim row has `claimed_at`, a reclaim leaves the first `claimed_at` and sets a new `delivered_at` only on first delivery.

### §7 — The pages say what the code does

Prerequisite: §1 to §6. `lease_flow.md` §1 and `data_flow.md` §"One active lease per fleet" describe the slot as the queue and the one-statement claim; `runner_fleet.md` §"the held heartbeat" describes the targeted ring and `woken_for`; `scaling.md` §1 restates the statement budget; `concurrency.md` §"Runner capacity and authoring lock order" replaces its two runtime rows ("runtime renewal takes runner → lease/affinity → wallet", "reports take lease → wallet") with the pinned order host → slot → admission → lease → wallet for issue, and names Dimension 5.6 as its proof; the registry gains a row for the doorbell's held-beat list and semaphore, replacing the hint-buffer row's description.

- **Dimension 7.1** — the pages pass the gate → Test `orly gate verify` (manual, evidence in the Pull Request Session Notes): `docs.language` reports zero findings on the changed pages.

## Interfaces

- `POST /v1/runners/me/leases` request gains optional `poll_nonce` (UUIDv7 string, 36 bytes, garde length). Absent means "no replay": the poll behaves as today. Same `(runner, poll_nonce)` with an active lease answers that lease's body again with the same `lease_id` and `fencing_token`.
- `POST /v1/runners/me/heartbeats` response gains optional `woken_for` (fleet id string) beside `woken`. `woken: true` with `woken_for` absent means a ring with no fleet, which only a daemon older than this spec sends.
- `afd_billing::ledger::reserve_on(executor, &Reservation) -> Result<Hold>`, `afd_billing::Accounts::{payer_on, estimate_on, spend_on}`, `afd_credential::Providers::resolve_reference_on`, `afd_credential::Vault::declared_on`, `afd_gate::Store::recorded_on`, `afd_gate::Grants::approved_integrations_on`: each takes `impl sqlx::PgExecutor<'_>` first and the existing arguments after (M-PARAMETER-CONSISTENCY).
- `afd_dragonfly::Doorbell::wait_for_work(max, cap) -> Waited` where `Waited::Rung(Uuid7)` carries the fleet; `Doorbell::hint() -> Option<Uuid7>` peeks and `Doorbell::consume(&Uuid7)` pops.
- `runner.fleet_slots.ready_at`, `fleet.admissions.claimed_at`, `runner.leases.poll_nonce`: new columns, nullable, no `DEFAULT`.

## Failure Modes

| Mode | Kind | Cause | Handling (response and what the caller observes) | Test |
|------|------|-------|--------------------------------------------------|------|
| Lost lease reply | failure | network drops the 200 after issue commits | the next poll with the same nonce answers the same lease; no second lease, no expiry tally | `test_lost_lease_reply_is_answered_by_the_next_poll_with_the_same_nonce` |
| Lost reply, fresh nonce | failure | runner restarts between poll and reply | no work until `LEASE_TTL`; then redelivery under a new fence | `test_fresh_nonce_after_lost_reply_finds_the_slot_busy` |
| Pass fault | failure | Postgres refuses a gate read | poll answers the error; event stays `received`; reclaimable after `LEASE_TTL` | `test_pass_fault_leaves_admission_claimable_after_ttl` |
| Short payer | capacity | balance below the floor at issue | transaction rolls back; refusal `BALANCE_EXHAUSTED`; no hold | `test_short_payer_gets_no_lease_and_no_hold` |
| Lost ring | failure | `SPUBLISH` fails or exceeds 100 ms | admission intact; the worker polls on the 1 s retry | `test_a_held_heartbeat_is_woken_by_admitted_work` |
| Lost arm | failure | a release path misses `ready_at` | the sweep re-arms on its cycle; Dimension 1.5 | `test_liveness_expiry_rearms_waiting_slots` |
| Claim race | race | 100 pollers, one fleet | one winner by `SKIP LOCKED`; others see no row | `test_hundred_pollers_claim_one_fleet_once` |
| Replay of a terminal event | replay | reclaim after the event ended | `Delivery::Terminal`: settle, no lease | `test_terminal_event_on_reclaim_is_settled_not_leased` |
| Deadlock | race | issue and renewal of one payer | pinned order; `lock_timeout` from the named constant; zero `40P01` | `test_issue_and_renewal_never_deadlock` |
| Runner dies mid-lease | failure | SIGKILL after issue | reclaim once after `LEASE_TTL`; late report refused | `test_sigkill_mid_lease_reclaims_once_and_refuses_the_late_report` |
| Held-beat cap | capacity | 65th held beat | `Capped` at once; in-flight permits preserved | `test_held_beats_past_the_cap_answer_capped` |
| Malformed nonce | failure | a non-UUID `poll_nonce` | garde refuses the request with the standard 400 problem | `test_lease_request_rejects_malformed_poll_nonce` (unit, `afd_wire`) |

## Invariants

1. At most one active lease per fleet — `runner.fleet_slots` row lock plus the fenced `insert_lease_with_event.sql` predicate `fencing_token = $n AND lease_expires_at > now`; Dimension 2.2.
2. The statement that advances `fencing_token` is the statement that writes the event row carrying it — one CTE chain in `claim_admission_head.sql`; a split is a compile-time failure of the `sqlx` row type that expects both columns.
3. A ready slot is a fleet with an unsettled, unparked head and no live lease — every writer of `lease_expires_at`, `parked_at` or `settled_at` sets or clears `ready_at` in the same statement; Dimension 1.2, 1.3, 1.5 and the sweep.
4. The gate pass writes nothing — the pass holds a `&mut PgConnection` behind a type that exposes only `fetch_*`, so an `execute` does not compile.
5. Lock order host → slot → admission → lease → wallet — every statement in `afd_fleet/src/lease/sql` and `afd_billing/src/ledger/sql` locks in that order; Dimension 5.6 is the contention proof; `concurrency.md` is the registry.
6. A `poll_nonce` names at most one lease per runner — `UNIQUE (runner_id, poll_nonce)`; Dimension 5.3.
7. One ring completes at most one held beat — the semaphore permit is returned by the completed waiter alone; Dimension 3.1.

## Metrics & Observability

| Metric / event | Audience | Fires when | Properties allowed | Privacy guard | Test |
|----------------|----------|------------|--------------------|---------------|------|
| `lease_polled{hinted,woken}` counters and `database_roundtrips`, `dragonfly_commands`, `candidates_scanned` | operator | every lease poll | booleans, counts | no identifiers | `test_lease_polled_counts_every_statement` |
| `wake_to_claim_seconds` histogram | operator | a claim whose ring carried `published_at_ms` | duration | no identifiers | `test_one_admission_costs_one_claim_statement_and_wakes_one_worker` |
| `lease_issue_denied{reason}` counter | operator | issue answers `Denied` | reason label | no identifiers | `test_capacity_denial_is_not_logged_as_claim_lost` |
| `lease_issued` log line gains `claimed_at`, `queue_wait_ms` from `claimed_at` | operator | issue commits | ids, durations | no secrets; ids only | `test_claimed_at_is_stamped_once_before_delivered_at` |
| `wake_answers{reason=rung,deadline,capped}` | operator | a held beat ends | reason | none | `test_held_beats_past_the_cap_answer_capped` |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | One admission, one claim statement, one woken worker (§2, §3) | `make test-integration-rustd` | `test_one_admission_costs_one_claim_statement_and_wakes_one_worker ... ok` | P0 | |
| R2 | 100 pollers, one lease (§2) | `make test-integration-rustd` | `test_hundred_pollers_claim_one_fleet_once ... ok` | P0 | |
| R3 | A lost reply replays the same lease (§5) | `make test-integration-rustd` | `test_lost_lease_reply_is_answered_by_the_next_poll_with_the_same_nonce ... ok` | P0 | |
| R4 | The hold is inside the issue transaction (§5) | `make test-integration-rustd` | `test_issue_places_hold_before_commit ... ok` | P0 | |
| R5 | A refusal writes once (§4) | `make test-integration-rustd` | `test_refused_event_is_terminal_in_one_statement ... ok` | P0 | |
| R6 | The claim uses the ready index at 10 000 fleets (§2) | `make test-integration-rustd` | `test_claim_plan_uses_ready_index ... ok` | P1 | |
| R7 | No deadlock between issue and renewal (§5) | `make test-integration-rustd` | `test_issue_and_renewal_never_deadlock ... ok` | P0 | |
| R8 | The scan and the stamp are gone (§2) | `git grep -n -w -e SELECT_CANDIDATES -e STAMP_DELIVERY -e CLAIM_FLEET_SLOT -e MAX_CANDIDATES_PER_POLL -- rustd` | no output | P1 | |
| R9 | The OpenAPI document matches the server (Interfaces) | `scripts/openapi-check.sh` | exit 0 | P0 | |
| R10 | Statements per lease and idle statements per poll meet the pass lines (Overview) | `make bench-lease PROFILE=rig` then `make bench-compare LANE=lease PROFILE=rig` | `drain_statements_per_lease` ≤ 12, `drain_commits_per_lease` ≤ 2 and `idle_statements_per_poll` = 1 in the compare output | P1 | |
| S1 | Declared commands green | `make lint-all` · `make test-unit-all` · `make test-integration-rustd` · `make check-version` | exit 0 each | P0 | |
| S2 | No secrets | `gitleaks detect --redact --no-banner` | exit 0 | P0 | |

## Dead Code Sweep

| Deleted file or symbol | Verify | Expected |
|------------------------|--------|----------|
| `rustd/crates/afd_fleet/src/lease/sql/lease/select_ledger_candidates.sql`, `SELECT_CANDIDATES` | `git grep -n -w SELECT_CANDIDATES` | no output |
| `rustd/crates/afd_fleet/src/lease/sql/lease/stamp_delivery.sql`, `STAMP_DELIVERY` | `git grep -n -w STAMP_DELIVERY` | no output |
| `rustd/crates/afd_fleet/src/lease/sql/lease/claim_fleet_slot.sql`, `CLAIM_FLEET_SLOT`, `Leases::claim` | `git grep -n -w CLAIM_FLEET_SLOT` | no output |
| `rustd/crates/afd_fleet/src/lease/pull/step.rs`, `Step`, `proceed` | `git grep -n -w -e "Step<" -e proceed -- rustd/crates/afd_fleet/src/lease` | no output |
| `record_received`, `redelivery_of`, `hold_issued`, `MAX_CANDIDATES_PER_POLL`, `Fence::as_u64` | `git grep -n -w -e record_received -e redelivery_of -e hold_issued -e MAX_CANDIDATES_PER_POLL -- rustd` | no output |
| `Rings` `watch` generation and `Held` guard in `doorbell/mod.rs` | `git grep -n -w -e "watch::Sender<u64>" -e "struct Held" -- rustd/crates/afd_dragonfly/src/doorbell` | no output |

## Out of Scope

- Removing the wallet row lock, the usage-line journal and queued settlement: M240_002.
- A chat admission joining a running lease at a safe boundary: M236 §2.
- A per-worker long poll or a gRPC (Google Remote Procedure Call) runner transport: not planned; M225 rejected the former, and the measured cost is Postgres statements, not framing.
- A price-book or fleet-revision cache: not planned; `afd_billing/src/store.rs` records the reason.
- Retiring `afd_dragonfly/src/hub/repair` and `hub/gap` once redis-rs 1.8.1's `resubscribe_node` is proven on the four-node lane: a follow-up spec with its own lane test.
- The `afd_fleet` crate split (21 `afd_*` dependencies, `afd_api_wire` imported by `lease/event.rs`): not planned here; §2 removes the `event.rs` import with `record_received`.

---

## Product Clarity (authoring record)

1. **Successful user moment** — A person sends a chat to an idle fleet and the first streamed chunk appears after one claim and one issue round trip plus model time, on a busy estate as on a quiet one.
2. **Preserved user behaviour** — Runners on the previous release keep polling and heartbeating: both new fields are optional, and a poll without a nonce behaves as today.
3. **Optimal-way check** — The shortest path is the one statement that claims and opens the event; the gap to it is the gate pass, which must stay in process for decrypt and policy, so it is one connection, not one statement.
4. **Rebuild-vs-iterate** — Iterate: the fenced CTE chain already exists and is extended; no new table, no new service.
5. **What we build** — a `ready_at` column and its arming writers, one claim statement, executor-taking gate reads, an in-transaction hold, a poll nonce, a targeted ring, honest poll counters.
6. **What we do NOT build** — a Dragonfly readiness directory (M225 retired it), a long poll, caches, a second queue or scheduler (M236 §1 forbids one too).
7. **Fit with existing features** — compounds with M233_002's stage timing (the counters land beside the span) and M236's task admission (one slot per fleet stays); it must not destabilise the authoring claim, which keeps its own lock order and is untouched.
8. **Surface order** — API first: the runner API fields ship with the daemon; no UI change.
9. **Dashboard restraint** — no new panel until `wake_to_claim_seconds` has a week of production samples; the M233_002 round-trip row is where the number first shows.
10. **Confused-user next step** — an operator who sees a fleet not leasing runs `agentsfleet fleets get <id>` and reads `ready_at` beside the slot's `lease_expires_at` in the existing admin runner view; a `NULL` `ready_at` with an unsettled head names the writer that missed the arm.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** §1 schema and arming, §2 the statement, §3 the ring, §4 the pass, §5 issue, §6 counters, §7 pages. Each Section leaves the daemon shippable; §4 and §5 are the ones that change crate seams and sit after the statement they depend on.
- **Alternatives considered:** a Dragonfly ready set per partition (M225's design, retired because it duplicated the queue and split hot slots); a `LISTEN/NOTIFY` wake from Postgres (rejected: Dragonfly already carries the ring to every replica, and `NOTIFY` adds a second pub/sub with no gap signal); a SQL function composing the gate reads (rejected: moves cross-crate reads into the schema, against SQL-REPO and the crate ownership of statements); a server-held long poll per worker (rejected by Indy in M225, and the targeted ring buys the herd fix at host granularity).
- **Patch-vs-refactor verdict:** a **refactor** because the candidate scan, the received transaction and the post-commit hold are replaced rather than tuned, and the removed code is listed in Dead Code Sweep.

## Discovery (consult log)

- **Consults** — Source review on `main` `65372f505`, Nov 05, 2026: lease pull, doorbell, runner loop, billing settlement and chat admission, each by a reviewer with no session context, with every claim carrying a `file:line`. Deciding facts: `claim_admission_head.sql` already chains fence, head lock and tally; `record_received` runs apart only so a refusal has a row (`event.rs` module comment); the doorbell's `watch` wakes every holder by design (`doorbell/mod.rs:40-45`); `assign/mod.rs` states "The admission is the queue"; `afd_billing/src/store.rs` states why there is no rate cache; `concurrency.md` records runtime renewal as runner → lease/affinity → wallet and reports as lease → wallet. Microsoft's Rust guidelines (checklist, Nov 05, 2026) consulted for M-DI-HIERARCHY, M-THROUGHPUT, M-HOTPATH, M-PARAMETER-CONSISTENCY and M-RUST-SHAPED.
- **Owner decisions** —
  > Indy (Nov 05, 2026): "Can you review the core leasing, admission, dragonfly, (essentially how the doorbell work, and how we pickup the chat in a reliable way, how does billing settlement happens) so we could improve the turn around time of the chat." — context: the request.
  > Indy (Nov 05, 2026): "Also should we move the runner's to agentsfleetd api's to grpc? if any" — context: answered no for now; recorded under Out of Scope.
  > Indy (Nov 05, 2026): "Yes fold into a spec. Prior to that if you were to do a large refactor and reduce the postgres sqls calls what would be your design" — context: the design this spec carries.
  > Indy (Nov 05, 2026): "so we are performant at scale" and "and reliable at scale" — context: the targets in Overview and the contention, kill and replay Dimensions.
  > Indy (Nov 05, 2026): "Yes all four moves, the other tree had a similar one, i requested if this could be moved to a queue based approach, since a billing function shouldnt impact the next lease and a few seconds of delay to claim/settle isnt an issue for an end user." — context: all four moves accepted; the billing move is M240_002, and this spec keeps the wallet lock inside issue until then.
  > Indy (Nov 05, 2026): "ensure you review the spec for implementation completeness and that when its handed over to fable would be able to do its job." — context: the EGM review bar below.
- **Model reviews** — EGM brainstorm, Claude Fable 5.1, Nov 05, 2026: one of six lenses (comprehension and ergonomics) completed before a usage limit ended the other five; 16 findings on the hypothesis, 13 applied in this draft (nonce protocol, `ready_at` lifecycle, holder selection and `woken` compatibility, affinity spelled as its SQL key, three "hold" meanings named, gate pass after the claim commit, refusal effect on the slot, hold-at-issue ownership and merge order, baseline paragraph, measurement method, risks, lock-order registry rows, mid-run exhaustion routed to M240_002), 3 routed to M240_002 (journal kinds and rollup formula, hold release trigger, balance read lag). The EGM review of this draft is recorded below it once run.
- **Open blockers** — none.
- **Deferrals and transfers** — none.
