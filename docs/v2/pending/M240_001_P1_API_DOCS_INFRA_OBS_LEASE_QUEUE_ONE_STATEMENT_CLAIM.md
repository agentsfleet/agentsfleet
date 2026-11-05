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

**Priority:** P1, every lease poll scans every active fleet and one admission wakes every idle host, so chat pickup cost grows with the size of the estate rather than with the work waiting

**Categories:** API, DOCS, INFRA, OBS

**Batch:** B1, the lease path; M240_002 (billing) is B2 and adds the first money hold to the issue transaction this spec leaves unchanged

**Depends on:** M233_002 (the delivery span this spec's counters are read beside); M226_001 (made the admissions ledger the only queue, shipped)

**Provenance:** LLM-drafted (Claude Fable 5.1, Nov 05, 2026) from a five-lens source review on `main` at `65372f505`; revised (Claude Opus 5.5, Nov 05, 2026) after two zero-context EGM reviews; owner decisions quoted in Discovery

**Canonical architecture:** `docs/architecture/lease_flow.md` §1, `docs/architecture/data_flow.md` §"One active lease per fleet", `docs/architecture/concurrency.md` §"Lock-invariant registry"

---

## Overview

**Goal (testable):** `test_one_admission_claims_in_one_statement_and_wakes_one_beat_per_replica`: 1 000 active fleets, one with a new chat admission, and idle hosts held on heartbeats across two daemon replicas. The admission completes at most one held beat on each replica, and the winning poll claims the fleet in one statement that also opens its event row. The losing poll runs one statement that writes nothing.

**Where the code stands:** M225_001 built a Dragonfly readiness index. M226_001 retired it, and `afd_fleet/src/lease/assign/mod.rs` now states "The admission is the queue". Every poll runs `select_ledger_candidates.sql`, which drives from `fleet.fleets`. Its `EXISTS (SELECT 1 FROM fleet.admissions WHERE settled_at IS NULL)` names no fleet, so every active fleet passes it and the scan cannot prune.

The poll then tries up to 64 candidates, at three statements each, and writes the event row in a separate six-statement "received" transaction (`afd_fleet/src/lease/event.rs`). The doorbell publishes a bare fleet id on `fleet:wake:{shard}`. Each replica's `watch` generation completes every held heartbeat on that replica (`afd_dragonfly/src/doorbell/mod.rs`), so each woken host runs that scan.

The committed bench baseline (`bench/baselines/lease.rig.json`, revision `c82badcb5`) predates M226, so the implementer re-measures it at PLAN.

**Three words this spec keeps apart.**
- A *sandbox-held fleet* is one whose frozen sandbox a runner keeps (`fleet_slots.hold_expires_at`, `HeldFleets` on the wire). One keeper task per runner owns every hold, and any worker can take one (`afr_supervisor/src/holds/mod.rs`).
- A *held heartbeat* is a heartbeat the daemon keeps open waiting for a ring, one per host.
- *Affinity* means `fleet_slots.last_runner_id = this runner`.

**Solution summary:** Five changes.
- `runner.fleet_slots` gains `ready_at`. A trigger on `fleet.admissions` maintains it as the `created_at` of the fleet's oldest unsettled admission, or `NULL` when that head is parked or there is none.
- The claim is one statement. It locks the oldest ready, free, eligible slot together with its head admission, using `SKIP LOCKED LIMIT 1`. In the same statement it advances the fence, stamps `delivered_at` and opens the event row.
- One ring completes one held beat per replica, and the hint buffer goes.
- A refusal ends in one statement, and the independent gate reads run concurrently.
- A runner repeating a poll whose reply it never saw frees that lease at once instead of stalling the fleet for `LEASE_TTL`.

**Design flow:**

```text
any write to fleet.admissions (insert, park, gate resolve or expiry, settle)
   └─ AFTER ROW trigger: upsert runner.fleet_slots(fleet) SET ready_at =
        head.created_at if the oldest unsettled admission is unparked, else NULL

admit (afd_admission)  INSERT admission → trigger arms ready_at → COMMIT → SPUBLISH fleet:wake:{shard}
each replica's hub     ring → Notify: ONE held beat on this replica returns woken

runner                 that host ends one idle worker's pause
                       POST /v1/runners/me/leases   Idempotency-Key: <UUIDv7 per poll attempt>

CLAIM (one statement, autocommit)
  replay?   a lease of this runner carrying this key and still active → expire it, free its slot,
            no expiry tally, answer "retry now"; a key whose lease ended → answer no work
  pick      fleet_slots s JOIN fleet.fleets f (active, required_tags ⊆ labels)
            JOIN fleet.admissions a (a.id = the head)
            WHERE s.ready_at IS NOT NULL AND s.lease_expires_at < now AND hold rule admits me
            ORDER BY sandbox_held_by_me DESC, last_runner_id = me DESC, s.ready_at
            FOR UPDATE OF s, a SKIP LOCKED LIMIT 1
  write     only FROM pick: fence+1, lease_expires_at = now+TTL, admission lease_fence and
            delivered_at = COALESCE(delivered_at, now), prior lease expired + tallied,
            INSERT fleet.events (status received, ON CONFLICT keeps the row)
  return    Acquired + delivery (First | Repeat | Terminal), read from the same row set

event_received frame on First (as today) → gate pass, reads overlapped → refusal: one statement
→ ISSUE transaction (unchanged here; M240_002 adds the money hold) → 200 { lease }
```

**Lock order this spec pins:** every statement that updates an existing `fleet.admissions` row locks that fleet's `runner.fleet_slots` row first, so the trigger's slot update always finds the slot already held by its own transaction. The claim takes the admission with `SKIP LOCKED` after the slot. `park` (`LOCK_ADMISSION`), the refusal, the report (`meter_lease.sql`) and the renewal already lock slot-first.

`afd_approval`'s `resolve_gate.sql` and `expire_gates.sql` do not. They update every unsettled admission of the fleet, including a running head, and would take the slot through the trigger afterwards. That deadlocks against `park` and against a report settling that head. §1 makes each a transaction whose first statement locks the slot, so the unpark also reads a fresh snapshot.

The admission insert is the one writer that reaches the slot after an admission row. Its new row is invisible to every other transaction until commit, so no one can be waiting on it. Dimensions 1.6 and 2.8 prove the order.

**Risks this spec carries:**
- A lost ring costs the fleet one 1 s poll.
- The trigger adds one slot update to every admission write.
- The claim statement grows, so its plan is proven at fleet count (Dimension 2.5).
- A `Idempotency-Key` header is ignored by a daemon on the previous release, so no deploy order is pinned.

**How the targets are measured:** `make bench-lease PROFILE=rig` reports `drain_statements_per_lease`, `drain_commits_per_lease` and `idle_statements_per_poll` (`make/bench.mk`). The implementer records a fresh baseline on the base revision at PLAN. The pass lines:
- `drain_statements_per_lease` at least 7 below that baseline.
- `drain_commits_per_lease` at least 2 below it.
- `idle_statements_per_poll` no higher than the baseline at zero blocked candidates, and constant as blocked candidates grow.

## PR Intent & comprehension handshake

- **PR title (eventual):** Claim the oldest ready fleet in one statement and wake one beat per ring
- **Intent (one sentence):** A chat message on an idle fleet is leased after one claim statement however many fleets the estate runs, one admission wakes one host per replica, and a lost lease reply no longer costs the fleet 30 s.
- **Authoring handshake:** Indy asked for a review of leasing, admission, the doorbell, chat pickup and billing "so we could improve the turn around time of the chat", then for a design that would "reduce the postgres sqls calls", "so we are performant at scale" and "reliable at scale". ASSUMPTIONS I'M MAKING:
  1. The per-worker long poll M225 rejected stays rejected.
  2. Billing changes, including the money hold at issue, belong to M240_002.
  3. No cache is added.
  4. Steer into a running lease belongs to M236 §2.
  5. The only runner API change is an optional request header.
- **Implementer handshake:** recorded in the Pull Request Session Notes at PLAN, before any edit (SPEC-HANDSHAKE).

## Implementing agent — read these first

1. `rustd/crates/afd_fleet/src/lease/sql/lease/claim_admission_head.sql` is the chain §2 rebuilds. Today its slot upsert runs before the head lock, which is why `assign/ledger.rs` rolls the transaction back when no head is claimed. §2 reverses that order so an empty claim writes nothing.
2. `rustd/crates/afd_fleet/src/lease/event.rs` holds `record_received`, `redelivery_of` and `block`: the received rows §2 folds in and the refusal §4 rewrites. Its "Why the caller is not `issue`" comment is the rationale §2 retires.
3. `schema/2040_billing_usage_lines.sql` and its `fleet.activity_counters` trigger are the repository's existing row trigger maintaining another table's column, the shape §1's trigger mirrors.
4. `rustd/crates/afd_dragonfly/src/doorbell/mod.rs` and `rustd/crates/agentsfleetd/tests/integration_runner_wake.rs` are the ring and hold §3 narrows, and the held-beat test it extends.
5. `rustd/crates/afd_api_runner/src/handler/runner/report.rs` shows how a runner handler reads `Idempotency-Key` today, the pattern §5 mirrors.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `docs/v2/pending/M240_001_P1_API_DOCS_INFRA_OBS_LEASE_QUEUE_ONE_STATEMENT_CLAIM.md` | CREATE then MOVE to `active/`, then `done/` | this spec |
| `schema/2190_runner_fleet_slots_ready.sql` | CREATE | §1: `ready_at`, the partial index, the trigger and its function on `fleet.admissions`, and the backfill of slot rows and `ready_at` |
| `schema/2200_runner_leases_poll_key.sql` | CREATE | §5: `poll_key` on `runner.leases`, unique per runner |
| `rustd/crates/afd_db/src/migration.rs` | EDIT | registers both slots |
| `rustd/crates/afd_fleet/src/lease/sql/lease/claim_admission_head.sql` | EDIT | §2, §5: the one-statement claim with its replay arm |
| `rustd/crates/afd_fleet/src/lease/sql/lease/select_ledger_candidates.sql`, `stamp_delivery.sql`, `claim_fleet_slot.sql` | DELETE | §2: the scan, the separate stamp and the slot claim with no production caller |
| `rustd/crates/afd_fleet/src/lease/sql/lease/settle_terminal.sql`, `afd_events/src/sql/` statement used by `block` | EDIT | §4: the one-statement ending |
| `rustd/crates/afd_fleet/src/lease/sql/queue.rs`, `sql/lease/mod.rs` | EDIT | statement registry follows the deletions |
| `rustd/crates/afd_fleet/src/lease/assign/ledger.rs`, `assign/mod.rs`, `assign/measured.rs` | EDIT | §2, §6: one statement, no loop, honest counts |
| `rustd/crates/afd_fleet/src/lease/event.rs` | EDIT | §2, §4: `record_received` and `redelivery_of` go; `block` is one statement |
| `rustd/crates/afd_fleet/src/lease/affinity.rs` | EDIT | §2: `Leases::claim` goes; `Fence::as_u64` refuses a negative fence. §4: `let_go` takes the ending's backoff |
| `rustd/crates/afd_core/src/timing.rs` | EDIT | §4: `CLAIM_RETRY_BACKOFF` and its compile-time relation to `LEASE_TTL` |
| `rustd/crates/afd_fleet/src/lease/sql/lease/release_unleased_slot.sql` | EDIT | §4: the release takes a backoff interval |
| `rustd/crates/afd_approval/src/sql/resolve_gate.sql`, `rustd/crates/afd_approval/src/sql/expire_gates.sql` | EDIT | §1: unpark only parked rows; expiry takes the locked fleet ids |
| `rustd/crates/afd_approval/src/sql/lock_gate_fleet_slot.sql`, `rustd/crates/afd_approval/src/sql/lock_expiring_fleet_slots.sql`, `rustd/crates/afd_approval/src/sql/mod.rs` | CREATE, EDIT | §1: the slot locks that open each transaction |
| `rustd/crates/afd_approval/src/inbox/resolve/mod.rs`, `rustd/crates/afd_approval/src/inbox/sweep.rs` | EDIT | §1: resolve and expire run their two statements in one transaction |
| `rustd/crates/afd_approval/tests/` gate resolution suite | EDIT | §1: Dimension 1.6's resolve and expiry ordering |
| `rustd/crates/afd_fleet/src/lease/pull/mod.rs`, `pull/held.rs`, `pull/claimed.rs`, `pull/refuse.rs`, `pull/poll.rs` | EDIT | §4: overlapped reads, `ControlFlow` |
| `rustd/crates/afd_fleet/src/lease/pull/step.rs` | DELETE | §4: `std::ops::ControlFlow` |
| `rustd/crates/afd_fleet/src/lease/admit/gates.rs`, `admit/mod.rs` | EDIT | §4: estimate, balance and spend overlapped |
| `rustd/crates/afd_fleet/src/lease/deliver/mod.rs`, `deliver/queue_wait.rs` | EDIT | §4, §6: declared names and grants overlapped; queue wait from `delivered_at` |
| `rustd/crates/afd_fleet/src/lease/issue.rs`, `rustd/crates/afd_fleet/src/lease/claim.rs` | EDIT | §5: `Lost` and `Denied` apart; the poll key reaches the claim |
| `rustd/crates/afd_dragonfly/src/doorbell/mod.rs`, `rustd/crates/afd_admission/src/wake.rs` | EDIT | §3: one beat per ring; the hint buffer goes |
| `rustd/crates/afd_api_runner/src/handler/runner/lease/mod.rs`, `afd_api_runner/src/openapi.rs` | EDIT | §5: the optional `Idempotency-Key` header, annotated |
| `public/openapi.json` | EDIT | regenerated from `rustd/` with `--no-banner` |
| `rustd/crates/afr_supervisor/src/client/lease_verbs/mod.rs`, `afr_supervisor/src/worker_pool/mod.rs` | EDIT | §5: one key per poll attempt, kept until a decoded answer |
| `rustd/crates/afd_observability/src/producers/fleet/mod.rs`, `afd_observability/src/metrics/declared/fleet.rs`, `afd_observability/src/metrics/label/fleet.rs`, `afd_bench/src/instrument/mod.rs`, `docs/metrics.census.tsv` | EDIT | §6: `candidates_scanned` goes; `lease_issue_denied` and `lease_poll_replayed` arrive |
| `rustd/crates/agentsfleetd/tests/integration_lease_queue.rs` | CREATE | §1, §2, §5: trigger, contention, plan, replay, kill |
| `rustd/crates/agentsfleetd/tests/integration_runner_wake.rs`, `afd_fleet/tests/integration_lease_claim_race.rs`, `afd_fleet/tests/integration_held_sandbox/{mod,report,resume}.rs`, `afd_fleet/src/lease/pull/tests.rs`, `afd_fleet/src/lease/admit/tests.rs`, `afd_dragonfly/src/doorbell/tests.rs` | EDIT | tests follow the mechanisms; callers of `Leases::claim` move to the plane |
| `docs/architecture/lease_flow.md`, `data_flow.md`, `runner_fleet.md`, `scaling.md`, `concurrency.md` | EDIT | §7 |
| `~/Projects/docs` branch `chore/m240-lease-queue-changelog` | EDIT (other repository) | the runner API header; committed after this Pull Request merges |

## Applicable Rules

- **`domains/sql.md`** — SQL-SLOT, SQL-STAMP, SQL-LOCKCLOCK, SQL-SWEEPCLOCK, SQL-REPO, SQL-ONEFACT, SQL-UPSERT, SQL-RETRYKEY, STS, IDMP, TXN, SQL-PLAN
- **`languages/rust.md`** — ERR-RS, RS-FROM, RS-NEWTYPE, RS-BIND, FN-RS, RS-RACE
- **`code.md`** — NDC, NLR, NLG, CODE-SEAM, OWN, FLL, CODE-MSID
- **`tests.md`** — TEST-REAL, TEST-CONTEND, TEST-ACKLOSS, TEST-KILL, TEST-INJECT, TEST-COUNTER, TEST-DELTA, TEST-LANE
- **`domains/logging.md`** — LOG-EVENT, LOG-FIELDS, OBS
- **`domains/http.md`, `domains/http-surface.md`** — HTTP-OPENAPI, HTTP-ANNOTATE, HTTP-BREAK
- **`docs.md`** — DOC-02, DOC-05, DOC-10, DOC-14b on the architecture pages

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| `make lint-all` | yes | no new `#[expect]` |
| `test_coverage_gate_rust_source`, `test_documented_codes_match_refusals`, OpenAPI check | yes, one header | annotated at the handler; `public/openapi.json` regenerated in the same commit |
| metrics census check | yes | census rows change with the families |
| `docs.language` | yes | short sentences, DOC-05 plain words |
| gitleaks pre-commit | no secrets in scope | `gitleaks detect --redact --no-banner` before each commit |
| File & Function Length (file ≤350 lines, function ≤50, method ≤70) | yes: `pull/mod.rs`, `deliver/mod.rs` near the cap | split where the gate fires; SQL files are exempt |

## Prior-Art / Reference Implementations

- **Reference:** `claim_admission_head.sql`'s fenced chain, and `meter_lease.sql`'s chain that reads under lock and settles in one statement.
- **Reference:** the `fleet.activity_counters` trigger on `billing.usage_lines` (`schema/2040`), for one table's row keeping another table's column current.
- **Reference:** `afd_api_runner` handlers reading `Idempotency-Key` (`handler/runner/report.rs`, `message.rs`, `schedule.rs`).
- **Divergence from M225_001:** its readiness directory lived in Dragonfly and was retired by M226. This spec keeps Postgres as the one queue and makes the slot row its index.

## Sections (implementation slices)

### §1 — A trigger keeps each fleet's readiness on its slot

Prerequisite: none. Slot 2190 adds `ready_at TIMESTAMPTZ(3)` to `runner.fleet_slots` and a partial index on `(lease_expires_at, ready_at) WHERE ready_at IS NOT NULL`.

It adds an `AFTER INSERT OR UPDATE OF settled_at, parked_at` row trigger on `fleet.admissions`. The trigger upserts the fleet's slot row:
- **On insert,** `fencing_token = 0` and `lease_expires_at = now() − 1 ms`, so the first claim carries fence 1 as today. `last_runner_id` and `hold_expires_at` start `NULL`.
- **On update,** it sets `ready_at` to the `created_at` of the fleet's oldest unsettled admission when that admission's `parked_at` is `NULL`, and to `NULL` otherwise.

The value is recomputed, never incremented, so every writer of `fleet.admissions` keeps it exact. That covers the `afd_admission` insert, `park_waiting.sql`, `afd_approval`'s `resolve_gate.sql` and `expire_gates.sql`, `meter_lease.sql`'s settle, `settle_terminal.sql`, and future writers. The trigger names no status string, only `NULL` markers (STS).

Gate resolution and gate expiry each become a two-statement transaction, so the slot lock comes from an earlier statement than the unpark:
- **Resolve** (`inbox/resolve/mod.rs`): `lock_gate_fleet_slot.sql` locks the slot of the fleet that owns the gate's `action_id`, `FOR UPDATE`. Then `resolve_gate.sql` runs unchanged except for its `unparked` filter.
- **Expire** (`inbox/sweep.rs`): `lock_expiring_fleet_slots.sql` locks the slots of every fleet with an unresolved gate past `expires_at`, ordered by `fleet_id`, and returns those fleet ids. `expire_gates.sql` then takes that array and expires only gates of those fleets. A gate that expires between the two statements waits for the next sweep, so the sweep never unparks an admission whose slot it does not hold.

The order matters for correctness as well as deadlocks. Under the pool's READ COMMITTED isolation, the unpark statement's snapshot is taken after the slot wait ends. So a `park` that committed while approval waited is visible, and its row is unparked. A `park` that starts after approval took the slot waits for approval to commit, then finds the gate resolved and parks nothing. One statement that locks the slot in a CTE would keep its starting snapshot and miss that row.

The `unparked` update gains `AND a.parked_at IS NOT NULL`, so it touches only parked admissions and never a running head. Today it rewrites every unsettled admission of the fleet, which would fire the trigger on rows that did not change. The isolation level is part of this proof: if the pool's default isolation ever changes, Dimension 1.6 is rerun.

A lease taking or extending the slot leaves `ready_at` alone. Readiness and occupancy are separate facts: the claim reads `ready_at IS NOT NULL AND lease_expires_at < now`. A lease that lapses silently therefore becomes claimable when its deadline passes, with no writer. The backfill inserts missing slot rows for fleets with unsettled admissions and sets every slot's `ready_at` by the same rule.

**Implementation default:** a trigger over arming in each writer. Seven writers in three crates would each need the same rule, and a missed one strands a fleet. Rejected alternative: driving the claim from `idx_admissions_unsettled` with no new column. Every running fleet's head stays unsettled until its report, so that drive still visits every running fleet on every poll.

- **Dimension 1.1** — a new admission arms its fleet → Test `test_admission_insert_sets_ready_at_to_the_head` (integration): admitting on a fleet with no slot creates the slot with fence 0 and `ready_at` equal to the admission's `created_at`. A second admission leaves `ready_at` unchanged.
- **Dimension 1.2** — park disarms, resolution re-arms → Test `test_parked_head_disarms_and_gate_resolution_rearms` (integration): `park_waiting` sets `ready_at` `NULL`, and `afd_approval`'s approve sets it back to the head's `created_at`.
- **Dimension 1.6** — gate resolution waits on the slot and still unparks a concurrent park → Test `test_gate_resolution_locks_the_slot_before_any_admission` (integration). A test transaction holds the fleet's slot `FOR UPDATE`, then stamps `parked_at` on the head without committing, as `park` does. A gate resolution started beside it waits on the slot and holds no admission row while it waits. The held transaction commits, and the resolution finishes with no `40P01`. Afterwards the head's `parked_at` is `NULL` and `ready_at` equals the head's `created_at`. The same sequence is checked for gate expiry. Two false passes are rejected: a test that only checks for no deadlock, and a test whose timing never interleaves the two locks.
- **Dimension 1.3** — settling the head moves readiness to the next → Test `test_settling_the_head_moves_ready_at_to_the_next_admission` (integration): with two admissions, a report settles the first and `ready_at` becomes the second's `created_at`. Settling the second sets `NULL`.
- **Dimension 1.4** — a silent lapse needs no writer → Test `test_lapsed_lease_is_claimable_without_any_write` (integration): a lease whose runner stops renewing is claimed by another runner once `LEASE_TTL` passes, with no sweep run.
- **Dimension 1.5** — the backfill is exact → Test `test_ready_backfill_matches_unsettled_heads` (integration): three fleets are seeded, one with an unsettled unparked head and one with a parked head. After the migration, only the first has `ready_at`.

### §2 — One statement claims the oldest ready fleet and opens its event

Prerequisite: §1. `claim_admission_head.sql` takes the runner, its labels, its sandbox-held fleets and the poll key, and no fleet id. The pick joins `runner.fleet_slots s` to `fleet.fleets f` (status active, `required_tags` contained in the runner's labels) and to `fleet.admissions a`, where `a.id` is the fleet's oldest unsettled admission and `a.parked_at IS NULL`. It requires `s.ready_at IS NOT NULL`, `s.lease_expires_at < statement_timestamp()` and today's hold rule.

It orders by sandbox-held-by-me, then `last_runner_id = me`, then `s.ready_at`, and takes `FOR UPDATE OF s, a SKIP LOCKED LIMIT 1`. A slot or head locked elsewhere is skipped and the next candidate is tried. The clock is read once in a `MATERIALIZED` step after the lock (SQL-LOCKCLOCK).

Every write joins the pick, so an empty pick writes nothing:
- The slot's fence advances and its `lease_expires_at` becomes now plus `LEASE_TTL`.
- The admission's `lease_fence` is stamped, with `delivered_at = COALESCE(delivered_at, now)`. `redelivered` is read from the prior value through `RETURNING WITH (OLD AS prior)`.
- The prior lease is expired and tallied, as today.
- The `fleet.events` row is inserted through `afd_events::sql::INSERT_LEASED_FLEET_EVENT`, whose conflict arm keeps the existing row and returns its status.

The statement returns what `Acquired` carries and the delivery reading: `First` when the event row is new, `Repeat` when it exists and is `received`, `Terminal` when it exists and has ended. `assign/ledger.rs` becomes one `fetch_optional` with autocommit.

`Terminal` ends in §4's one-statement ending, with the event row left as it is. `record_received`, `redelivery_of`, `STAMP_DELIVERY`, `SELECT_CANDIDATES`, `MAX_CANDIDATES_PER_POLL`, `CLAIM_FLEET_SLOT` and `Leases::claim` go (NLR). The `event_received` frame publishes on `First`, after the claim, as today.

- **Dimension 2.1** — the Goal → Test `test_one_admission_claims_in_one_statement_and_wakes_one_beat_per_replica` (integration): one admission across 1 000 fleets and two replicas. Each replica completes at most one held beat, and the `pg_stat_statements` delta shows one claim statement for the winning poll and one write-free claim statement for any losing poll. The false pass this rejects is a claim that commits a fence bump with no lease.
- **Dimension 2.2** — exactly one winner → Test `test_hundred_pollers_claim_one_fleet_once` (integration, TEST-CONTEND): 100 polls released from a barrier against one ready fleet. There is one claimed row and 99 empty answers, the fence advanced by one, and the slot is untouched by the losers.
- **Dimension 2.3** — an empty claim writes nothing → Test `test_empty_claim_writes_nothing` (integration): a ready slot whose head is locked by an open transaction. The claim returns no row and leaves the slot's fence, `lease_expires_at` and `ready_at` unchanged.
- **Dimension 2.4** — labels and status are respected → Test `test_claim_respects_required_tags_and_fleet_status` (integration): a ready fleet with `required_tags = {gpu}` is not claimed by a runner without `gpu`, and a paused fleet's ready slot is claimed by no runner.
- **Dimension 2.5** — the plan holds at scale → Test `test_claim_plan_uses_ready_index` (integration, SQL-PLAN): 10 000 active fleets, 2 000 with live leases and 3 ready and free, after `ANALYZE`. `EXPLAIN (ANALYZE, BUFFERS)` shows the partial index and no sequential scan of `fleet.fleets` or `runner.fleet_slots`.
- **Dimension 2.6** — oldest first → Test `test_claim_takes_the_oldest_ready_fleet` (integration): with three ready fleets armed at ascending instants and no hold or affinity, the claim takes the oldest.
- **Dimension 2.7** — redelivery and terminal readings → Test `test_reclaim_reports_repeat_and_terminal_correctly` (integration): a lapsed `received` event reads `Repeat` with `redelivered = true`, and a lapsed `processed` event reads `Terminal`. That admission settles with no lease, and `delivered_at` keeps its first instant in both.
- **Dimension 2.8** — no deadlock with the trigger → Test `test_claim_report_park_and_admit_never_deadlock` (integration, RS-RACE): 50 claims, 50 reports, 50 gate resolutions and 50 admissions over 10 fleets released together produce zero `40P01`.

### §3 — One ring wakes one held beat per replica

Prerequisite: §2. The `Rings` `watch` generation becomes a `tokio::sync::Notify`. A ring calls `notify_one`, which completes the longest-waiting held beat on this replica. With none waiting, it stores one permit, so the next held beat returns at once.

`close()` sets the closed flag and calls `notify_waiters`, so every held beat ends at drain as today. The `Held` guard and the `AtomicUsize` cap stay, so a beat dropped mid-wait returns its place and the 65th beat answers `Capped`.

The hint buffer and its consumer go, because the claim orders by age. Each replica still hears every ring, so an admission completes at most one beat per replica. The payload and the heartbeat reply are unchanged.

- **Dimension 3.1** — one ring, one beat → Test `test_a_ring_completes_exactly_one_held_beat` (unit): with three held waiters, one ring returns one waiter `Rung` while two still wait.
- **Dimension 3.2** — drain ends every hold → Test `test_close_ends_every_held_beat` (unit): with three waiters, `close()` returns all three and a later wait returns at once.
- **Dimension 3.3** — the cap and the drop still hold → Test `test_held_beats_past_the_cap_answer_capped` (unit): 65 waiters; the 65th is `Capped`, and dropping one waiter lets a new one hold.
- **Dimension 3.4** — a lost ring costs only the hold → Test `test_a_held_heartbeat_is_woken_by_admitted_work` (integration, existing): the held beat is woken, and with publishing disabled the worker still leases on the 1 s retry.

### §4 — A refusal ends in one statement, and independent reads overlap

Prerequisite: §2. The claim has committed before the pass starts, so no row lock is held while a secret is decrypted or a policy assembled.

The pass keeps its reads and their order. Two groups of reads that do not depend on each other run under `future::join`: estimate, balance and fleet spend (`admit/gates.rs`), and declared vault names with approved integrations (`deliver/mod.rs`). Each read keeps its own pooled connection, because none nests.

`Leases::block` becomes one statement guarded by the fence the claim stamped:
- It locks the slot `WHERE fencing_token = $fence`.
- It sets the event's terminal status and detail `WHERE status = received`. A returned row reads `Ended::Now`, no row reads `Ended::Already`.
- It settles the admission `WHERE lease_fence = $fence`.
- It sets the slot's `lease_expires_at` in the past, and the trigger recomputes `ready_at`.

`settle_terminal.sql` takes the same shape for the `Terminal` ending, leaving the event status unchanged. `Step<T>` and `proceed` become `std::ops::ControlFlow<Ending, T>`.

A pass that ends without settling or parking the admission frees the slot with a backoff. These endings are a datastore fault, a `Declined::Retry` from a transient gate read, an unwritten grant request, and an unreadable config. `let_go` takes the ending's backoff and sets `lease_expires_at = clock_timestamp() + CLAIM_RETRY_BACKOFF` instead of a past instant. The claim's `lease_expires_at < now` predicate then skips that fleet until the backoff passes, while `ready_at` keeps its place in the queue.

Without the backoff, the oldest-first order would pick a failing fleet first on every poll and starve the fleets behind it. Today's `random()` order hides that. `CLAIM_RETRY_BACKOFF` is a new `afd_core::timing` constant of 5 s, asserted below `LEASE_TTL` at compile time. Endings that free the fleet for another runner, `Denied` and `Lost` in §5, keep the past instant.

- **Dimension 4.1** — a refusal is one statement → Test `test_refused_event_ends_in_one_statement` (integration): a balance-gate refusal. The event is `gate_blocked`, the admission is settled, the slot is free, `ready_at` moved to the next admission, and the request's `pg_stat_statements` delta holds one ending statement.
- **Dimension 4.2** — the money reads overlap → Test `test_money_gate_reads_start_together` (unit): with recording accounts doubles that block until released, all three reads have started before any completes.
- **Dimension 4.3** — a fault frees the fleet after the backoff → Test `test_pass_fault_releases_the_slot_after_the_backoff` (integration, TEST-INJECT): a refused statement is injected at the grants read. The poll answers the error, a poll inside `CLAIM_RETRY_BACKOFF` does not claim that fleet, and a poll after it claims the same admission as `Repeat`.
- **Dimension 4.5** — a failing fleet does not starve a healthy one → Test `test_a_failing_fleet_does_not_starve_a_healthy_one` (integration, TEST-INJECT): two ready fleets, the older one's provider read failing on every pass. Two sequential polls lease the younger fleet on the second, and the older fleet is retried only after the backoff. The false pass this rejects is a test with two runners polling concurrently, which would lease the younger fleet without any backoff.
- **Dimension 4.4** — a stale refusal writes nothing → Test `test_refusal_under_a_stale_fence_reads_already` (integration): after the fence advanced, `block` returns `Already` and changes no row.

### §5 — A lost lease reply frees the fleet at once

Prerequisite: §2. Each worker mints a UUIDv7 per poll attempt and sends it as `Idempotency-Key`. It keeps the key across the backoff of a transport failure, a client timeout or a 5xx. It mints a fresh one after any decoded answer: a lease, no work, or a 4xx.

Slot 2200 adds `poll_key UUID` to `runner.leases`, unique on `(runner_id, poll_key)`, written by issue. The claim statement's replay arm runs first:
- If this runner holds an active lease carrying the key, the arm expires that lease without tallying an expiry and frees its slot. It answers no work with `retry_after_ms = 0`, so the worker's next poll, under a fresh key, claims the admission as `Repeat`.
- If the key names a lease that has ended, it answers no work and writes nothing.

A worker never shares a key, so a replay can only free the lease that worker's own poll issued and never ran. A daemon on the previous release ignores the header.

`issue`'s `Ok(None)` splits in two:
- `Lost`: the fence was superseded, the slot deadline passed, or the choice record missed.
- `Denied`: the host row is missing, the host cannot claim, or the ticket refuses.

Both call `let_go`, which frees a slot still under this fence so another runner claims at once. Each logs its own event and counts `lease_issue_denied{reason}` or the existing `lease_claim_lost`.

- **Dimension 5.1** — a lost reply is freed, not stalled → Test `test_lost_lease_reply_is_freed_by_the_repeated_key` (integration, TEST-ACKLOSS): issue commits and the harness drops the response. The repeated poll answers `retry_after_ms = 0`, the next poll leases the same admission as `Repeat` within one round trip, and `expired_count` is unchanged.
- **Dimension 5.2** — an ended key does nothing → Test `test_key_of_an_ended_lease_answers_no_work` (integration): a key whose lease reported answers no work and writes no row.
- **Dimension 5.3** — a denial frees the fleet → Test `test_capacity_denial_releases_the_slot` (integration): a host at its worker count claims, issue answers `Denied`, `lease_issue_denied{reason=capacity}` counts, and another runner claims the fleet on its next poll.
- **Dimension 5.4** — a killed runner's lease runs once → Test `test_sigkill_mid_lease_reclaims_once_and_refuses_the_late_report` (integration, TEST-KILL): the runner is killed after issue. After `LEASE_TTL` another runner claims the admission as `Repeat`, and a report under the dead fence is refused `RUN_STALE_FENCING_TOKEN`.
- **Dimension 5.5** — a malformed key is refused → Test `test_lease_poll_rejects_a_malformed_idempotency_key` (unit, `afd_api_runner`): a non-UUID header answers the standard 400 problem.

### §6 — The poll reports its real cost

Prerequisite: §2. `PollCost` counts every statement the claim and pass ran, and `dragonfly_commands` counts the approval gate's `GET` and the `event_received` publish instead of the hard-coded zero in `assign/measured.rs`. `candidates_scanned` goes with the scan: its producer, its declared family, the bench instrument and its census row.

`queue_wait_ms` on the issue log line becomes `delivered_at` minus `event_created_at`, clamped at zero as today. `lease_poll_replayed` counts §5's replays, so the lost-reply rate becomes a measured number.

- **Dimension 6.1** — counts are real → Test `test_lease_polled_counts_every_statement` (unit, `measured`): a recorded poll with one claim and seven pass reads reports `database_roundtrips = 8` and `dragonfly_commands = 2`.

### §7 — The pages say what the code does

Prerequisite: §1 to §6.
- `lease_flow.md` §1 and `data_flow.md` §"One active lease per fleet" describe the trigger-kept readiness and the one-statement claim.
- `runner_fleet.md` §"The held heartbeat" describes one beat per ring per replica.
- `scaling.md` §"Idle request volume" and §"Candidate and claim bounds" replace the 64-candidate bound with the index-driven claim.
- `concurrency.md` keeps its renewal and report rows, drops the hint-buffer row, and adds the slot-first rule for every update of an existing admission, with the admission insert as its one documented exception, naming Dimensions 1.6 and 2.8 as the proof.

- **Dimension 7.1** — the pages pass the gate → Test `orly gate verify` (manual, evidence in the Pull Request Session Notes): `docs.language` reports zero findings on the changed pages.

## Interfaces

- `POST /v1/runners/me/leases` accepts an optional `Idempotency-Key` header (UUIDv7). The body and response shapes are unchanged. A replay answers the existing no-work body with `retry_after_ms = 0`.
- `afd_dragonfly::Doorbell`: `ring(fleet)` and `wait_for_ring(deadline: Duration, cap: usize) -> Waited` keep their signatures. `take()` goes.
- `runner.fleet_slots.ready_at` and `runner.leases.poll_key` are new nullable columns with no `DEFAULT`, plus the trigger on `fleet.admissions`.

## Failure Modes

| Mode | Kind | Cause | Handling (response and what the caller observes) | Test |
|------|------|-------|--------------------------------------------------|------|
| Lost lease reply | failure | the network drops the 200 after issue | the repeated key frees the lease; the next poll leases it | `test_lost_lease_reply_is_freed_by_the_repeated_key` |
| Stale key | replay | a key whose lease ended | no work, no write | `test_key_of_an_ended_lease_answers_no_work` |
| Head locked during claim | race | another transaction holds the head | skipped; the slot is untouched | `test_empty_claim_writes_nothing` |
| Claim race | race | 100 pollers, one fleet | one winner | `test_hundred_pollers_claim_one_fleet_once` |
| Deadlock | race | claim, report, park, gate resolution and admit of one fleet | every update of an existing admission locks the slot first | `test_claim_report_park_and_admit_never_deadlock` |
| Approval against park | race | `resolve_gate` takes an admission between `park`'s slot and admission locks | resolution locks the slot in its first statement and waits; its unpark statement then reads a fresh snapshot and clears the new park; no cycle | `test_gate_resolution_locks_the_slot_before_any_admission` |
| Pass fault | failure | Postgres refuses a gate read | the error is answered; the slot frees after `CLAIM_RETRY_BACKOFF`; then a poll redelivers | `test_pass_fault_releases_the_slot_after_the_backoff` |
| Failing fleet at the head | failure | a fleet's transient gate read fails on every pass | that fleet is skipped for `CLAIM_RETRY_BACKOFF` after each failure; younger ready fleets are leased meanwhile | `test_a_failing_fleet_does_not_starve_a_healthy_one` |
| Capacity denial | capacity | a host at its worker count | the slot is freed for another runner | `test_capacity_denial_releases_the_slot` |
| Runner dies mid-lease | failure | SIGKILL after issue | reclaimed once after `LEASE_TTL`; the late report is refused | `test_sigkill_mid_lease_reclaims_once_and_refuses_the_late_report` |
| Lost ring | failure | `SPUBLISH` fails | the worker leases on the 1 s retry | `test_a_held_heartbeat_is_woken_by_admitted_work` |
| Held-beat cap | capacity | the 65th held beat | `Capped` at once | `test_held_beats_past_the_cap_answer_capped` |
| Malformed key | failure | a non-UUID header | 400 problem | `test_lease_poll_rejects_a_malformed_idempotency_key` |

## Invariants

1. At most one active lease per fleet. The slot row lock and the fenced `insert_lease_with_event.sql` predicate enforce it (Dimension 2.2).
2. A claim writes only when it took a head. Every write CTE joins the pick, so an empty pick changes no row (Dimension 2.3).
3. `ready_at` equals the `created_at` of the fleet's oldest unsettled unparked admission, or `NULL`. One trigger writes it, recomputing on every admission write (Dimensions 1.1 to 1.3, 1.5).
4. Every statement that updates an existing admission row locks that fleet's slot first, and the claim takes admissions with `SKIP LOCKED` after the slot. The admission insert reaches the slot after a row no other transaction can see (Dimensions 1.6, 2.8).
5. A poll key frees at most the one lease its worker's poll issued. `UNIQUE (runner_id, poll_key)` holds, and the arm matches this runner's active lease only (Dimension 5.1).
6. One ring completes at most one held beat per replica, through `Notify::notify_one` (Dimension 3.1).
7. A fleet whose pass ended with a retry is not claimed again before `CLAIM_RETRY_BACKOFF`. `let_go` writes the deadline, and the claim's `lease_expires_at < now` predicate reads it (Dimension 4.5).

## Metrics & Observability

| Metric / event | Audience | Fires when | Properties allowed | Privacy guard | Test |
|----------------|----------|------------|--------------------|---------------|------|
| `lease_polled` counts `database_roundtrips`, `dragonfly_commands` | operator | every lease poll | counts | no identifiers | `test_lease_polled_counts_every_statement` |
| `lease_issue_denied{reason}` counter and log event | operator | issue answers `Denied` | reason, runner and fleet ids in the log | ids only | `test_capacity_denial_releases_the_slot` |
| `lease_poll_replayed` counter and log event | operator | the replay arm frees a lease | runner and lease ids in the log | ids only | `test_lost_lease_reply_is_freed_by_the_repeated_key` |
| `lease_issued` log line, `queue_wait_ms` from `delivered_at` | operator | issue commits | ids, durations | ids only | `test_reclaim_reports_repeat_and_terminal_correctly` |
| `wake_answers{reason}` (existing) | operator | a held beat ends | reason | none | `test_held_beats_past_the_cap_answer_capped` |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | One admission claims in one statement and wakes one beat per replica (§2, §3) | `make test-integration-rustd` | `test_one_admission_claims_in_one_statement_and_wakes_one_beat_per_replica ... ok` | P0 | |
| R2 | 100 pollers, one lease (§2) | `make test-integration-rustd` | `test_hundred_pollers_claim_one_fleet_once ... ok` | P0 | |
| R3 | An empty claim writes nothing (§2) | `make test-integration-rustd` | `test_empty_claim_writes_nothing ... ok` | P0 | |
| R4 | Readiness follows every admission write (§1) | `make test-integration-rustd` | `test_parked_head_disarms_and_gate_resolution_rearms ... ok` | P0 | |
| R5 | The claim plan holds at 10 000 fleets (§2) | `make test-integration-rustd` | `test_claim_plan_uses_ready_index ... ok` | P1 | |
| R6 | No deadlock across claim, report, park and admit (§2) | `make test-integration-rustd` | `test_claim_report_park_and_admit_never_deadlock ... ok` | P0 | |
| R7 | A lost reply is freed at once (§5) | `make test-integration-rustd` | `test_lost_lease_reply_is_freed_by_the_repeated_key ... ok` | P1 | |
| R12 | Gate resolution locks the slot before any admission (§1) | `make test-integration-rustd` | `test_gate_resolution_locks_the_slot_before_any_admission ... ok` | P0 | |
| R11 | A failing fleet does not starve a healthy one (§4) | `make test-integration-rustd` | `test_a_failing_fleet_does_not_starve_a_healthy_one ... ok` | P0 | |
| R8 | A refusal ends in one statement (§4) | `make test-integration-rustd` | `test_refused_event_ends_in_one_statement ... ok` | P1 | |
| R9 | The scan and the separate stamp are gone (§2) | `git grep -n -w -e SELECT_CANDIDATES -e STAMP_DELIVERY -e CLAIM_FLEET_SLOT -e MAX_CANDIDATES_PER_POLL -- rustd` | no output | P1 | |
| R10 | Statements and commits per lease fall (Overview) | `make bench-lease PROFILE=rig` then `make bench-compare LANE=lease PROFILE=rig` | `drain_statements_per_lease` at least 7 and `drain_commits_per_lease` at least 2 below the baseline recorded at PLAN | P1 | |
| S1 | Declared commands green | `make lint-all` · `make test-unit-all` · `make test-integration-rustd` · `make check-version` | exit 0 each | P0 | |
| S2 | No secrets | `gitleaks detect --redact --no-banner` | exit 0 | P0 | |

## Dead Code Sweep

| Deleted file or symbol | Verify | Expected |
|------------------------|--------|----------|
| `select_ledger_candidates.sql`, `SELECT_CANDIDATES`, `MAX_CANDIDATES_PER_POLL` | `git grep -n -w -e SELECT_CANDIDATES -e MAX_CANDIDATES_PER_POLL -- rustd` | no output |
| `stamp_delivery.sql`, `STAMP_DELIVERY`, `record_received`, `redelivery_of` | `git grep -n -w -e STAMP_DELIVERY -e record_received -e redelivery_of -- rustd` | no output |
| `claim_fleet_slot.sql`, `CLAIM_FLEET_SLOT` | `git grep -n -w CLAIM_FLEET_SLOT -- rustd` | no output |
| `pull/step.rs` | `git grep -n -w -e "Step<" -e proceed -- rustd/crates/afd_fleet/src/lease` | no output |
| the doorbell hint buffer | `git grep -n -w -e HINT_CAPACITY -e "fn take" -- rustd/crates/afd_dragonfly/src/doorbell` | no output |
| `candidates_scanned` family | `git grep -n -w candidates_scanned -- rustd docs/metrics.census.tsv` | no output |

## Out of Scope

- Billing, including the first money hold inside issue and the wallet lock: M240_002.
- A chat admission joining a running lease: M236 §2.
- A per-worker long poll, a gRPC (Google Remote Procedure Call) runner transport, or caches: not planned.
- Label-aware ring targeting: not planned. A ring wakes one beat per replica, and the oldest-first claim serves whichever host polls.
- Executor-taking gate reads on one pinned connection: rejected in review. Today's reads never nest an acquire, and pinning one connection lengthens hold time.
- Deferring the provider decrypt until after the money gates: not planned. It saves work only on refusals.
- Retiring `afd_dragonfly/src/hub/repair` and `hub/gap`: a follow-up with its own lane test.

---

## Product Clarity (authoring record)

1. **Successful user moment** — A person sends a chat to an idle fleet, and the lease is claimed by one statement, on an estate of ten thousand fleets as on one of ten.
2. **Preserved user behaviour** — Runners on the previous release keep polling, because the header is optional. Events, refusals and the live tail read as today.
3. **Optimal-way check** — The shortest path is one statement that picks, fences and opens the event. The gate pass stays in process for decrypt and policy, so its reads overlap rather than merge.
4. **Rebuild-vs-iterate** — Iterate: the fenced chain, the fence semantics and the issue transaction stay. One column, one trigger and one header are added.
5. **What we build** — the readiness trigger, the one-statement claim, the one-beat ring, the one-statement refusal, the poll-key replay, honest counters.
6. **What we do NOT build** — a Dragonfly readiness directory, a long poll, caches, label-aware wake, pinned-connection gate reads.
7. **Fit with existing features** — Compounds with M233_002's delivery span and M236's task admission, since one slot per fleet stays. It must not destabilise the authoring claim, whose statements and order are untouched.
8. **Surface order** — API first, with one optional header. No UI change.
9. **Dashboard restraint** — No new panel. `lease_poll_replayed` and `lease_issue_denied` feed the existing operator dashboard after a week of samples.
10. **Confused-user next step** — An operator who sees a fleet not leasing reads its slot: a `NULL` `ready_at` with an unsettled head means the head is parked on an approval card. A future `lease_expires_at` means a lease holds it.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** §1 readiness, §2 claim, §3 ring, §4 refusal and overlap, §5 replay, §6 counters, §7 pages. Each Section leaves the daemon shippable. §3 to §6 depend only on §2.
- **Alternatives considered:**
  - A claim driven from `idx_admissions_unsettled` with no column. Rejected: it visits every running fleet on every poll.
  - Arming `ready_at` in each writer. Rejected: seven writers in three crates.
  - A Dragonfly ready set. Retired by M226.
  - `LISTEN/NOTIFY` wakes. Rejected: a second pub/sub with no gap signal.
  - A body-field poll nonce with full lease replay. Rejected: `deny_unknown_fields` forces a deploy order, and replaying the body duplicates the construction path.
  - Label-aware targeted wake. Rejected: the ring carries no tags, and the claim already serves whoever polls.
- **Patch-vs-refactor verdict:** a **refactor** because the scan, the per-candidate loop and the received transaction are replaced, and the removed code is listed above.

## Discovery (consult log)

- **Consults** — Source review on `main` `65372f505`, Nov 05, 2026, each claim cited to a file and line.
  - `select_ledger_candidates.sql`'s `EXISTS` is uncorrelated.
  - `claim_admission_head.sql` upserts the slot before the head lock.
  - The doorbell's `watch` wakes every holder on a replica.
  - Sandbox holds are runner-wide under one keeper (`afr_supervisor/src/holds/mod.rs`).
  - Runner handlers already read `Idempotency-Key`.
  - `afd_approval` clears `parked_at` in its own statements.
  - The bench baseline revision `c82badcb5` predates M226.

  Microsoft's Rust guidelines checklist, Nov 05, 2026, was consulted for M-DI-HIERARCHY, M-THROUGHPUT, M-HOTPATH and M-RUST-SHAPED.
- **Owner decisions** —
  > Indy (Nov 05, 2026): "Can you review the core leasing, admission, dragonfly, (essentially how the doorbell work, and how we pickup the chat in a reliable way, how does billing settlement happens) so we could improve the turn around time of the chat." — context: the request.
  > Indy (Nov 05, 2026): "Also should we move the runner's to agentsfleetd api's to grpc? if any" — context: answered no for now; under Out of Scope.
  > Indy (Nov 05, 2026): "Yes fold into a spec. Prior to that if you were to do a large refactor and reduce the postgres sqls calls what would be your design" — context: the design.
  > Indy (Nov 05, 2026): "so we are performant at scale" and "and reliable at scale" — context: the targets and the contention, kill and replay Dimensions.
  > Indy (Nov 05, 2026): "Yes all four moves, the other tree had a similar one, i requested if this could be moved to a queue based approach, since a billing function shouldnt impact the next lease and a few seconds of delay to claim/settle isnt an issue for an end user." — context: all four moves accepted; billing is M240_002.
  > Indy (Nov 05, 2026): "ensure you review the spec for implementation completeness and that when its handed over to fable would be able to do its job." — context: the review bar.
- **Model reviews** —
  - EGM brainstorm (Claude Fable 5.1): one of six lenses completed before a usage limit; its findings were applied to the first draft.
  - EGM zoom-out review of the first draft (Claude Fable 5.1): 11 findings, 10 applied, 1 declined. Applied:
    - `woken_for` and label-aware wake dropped.
    - The ring shrunk to `notify_one`, and the hint buffer removed.
    - Executor threading dropped for overlapped reads.
    - The poll key moved to the existing header.
    - The money hold moved to M240_002.
    - `concurrency.md` rows kept, `candidates_scanned` removed, altitude reduced, and the Goal reworded per replica.
    - The uncorrelated `EXISTS` recorded.

    Declined: dropping `ready_at`, because a backlog-driven claim still visits every running fleet.
  - EGM comprehension review of the first draft (Claude Fable 5.1): 7 blockers, 21 majors, 10 nits. Blockers resolved:
    - `ready_at` is an order key, and the title matches.
    - An empty claim writes nothing because every write joins the pick.
    - Readiness moved into a trigger that covers `afd_approval`.
    - Label matching dropped.
    - The Goal is per replica.
    - The replay arm lives inside the claim statement.
    - The read-only connection invariant dropped with the executor threading.

    The majors are applied: initial slot values, the writer list replaced by the trigger, fleet status in the pick, the `redelivered` and `delivered_at` rules, the terminal ending, the refusal's fence and `Now`/`Already` reading, the fault path, the key's unique index and its ended-lease answer, replay without re-rendering, the `Ok(None)` split with slot release, and the lock-order scope. The rest were made moot by the scope cuts. Nits are applied.
- **Open blockers** — none.
- **Deferrals and transfers** — none.
