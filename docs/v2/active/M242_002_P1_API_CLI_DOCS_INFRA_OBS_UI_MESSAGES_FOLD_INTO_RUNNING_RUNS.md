<!--
SPEC AUTHORING RULES (the one comment that survives):
- The spec standard (spec.md) holds every rule. Each tpl: comment names the
  rules that govern its section; delete it once the section is filled.
- Body order is the implementing agent's read order.
- No time or effort estimates, owners or implementation dates (SPEC-ESTIMATE).
-->

# M242_002: A chat message sent mid-run folds into that run, and every send answers its disposition

**Prototype:** v2.0.0

**Milestone:** M242

**Workstream:** 002

**Date:** Nov 06, 2026

**Status:** IN_PROGRESS

**Priority:** P1, a burst of chat messages today becomes a queue of separate runs, and every person waits for runs they did not start

**Categories:** API, CLI, DOCS, INFRA, OBS, UI

**Batch:** B1, built after M242_001 in the same milestone; M243 starts after M242 merges

**Depends on:** M242_001, whose one intake statement this spec extends with the disposition reads; M241_001, whose burst bench and `push_scheduler.md` are the proof and the design

**Provenance:** LLM-drafted (Claude Opus 5.5, Nov 06, 2026) from the EGM brief and source research on main 14539a73e

**Canonical architecture:** `docs/architecture/push_scheduler.md` §M242 (created by M241_001)

---

## Overview

**Goal (testable):** A steer sent while its fleet's chat run is live answers `folded`, reaches that run's next model call under the run's fence, settles exactly once with the run, and the burst bench's pickup p95 stays under 1 s.

**Problem:** Every chat message is a run of its own; `docs/architecture/runner_fleet.md:541` says "The current run finishes first; the steer runs next."
- M241's short burst left 480 of 500 messages unleased at window close, with pickup p95 at 8.56 s (`bench/results/lease-burst.rig.json`).
- A person who adds a sentence mid-reply waits a full run, then pays for another.
- The 202 says only `accepted`, so neither the command-line interface (CLI) nor the chat can say where the message went.

**Solution summary:** The runner holding the fleet reads pending steers at each model-call boundary under its fence, as Codex drains its turn input queue, and the daemon settles each fold with the run's report. A send answers `started`, `folded`, `queued` or `waiting_compaction`. During compaction a message waits while both surfaces say "compacting", then folds at the first model call after.

**Design flow:**

```
POST /messages ─► M242_001 intake statement + slot and head reads ─► 202 {event_id, disposition, folded_into_event_id}
runner root loop, at each model-call boundary
  preflight compacts ─► POST /folds {compacting: true} ─► slot.compacting_started_at set ─► sends answer waiting_compaction
  POST /folds {fencing_token, after_event_id} ─► fold_pending.sql under the fence
       link the steer prefix after the head, stamp lease_fence ─► messages appended as user input
       event_folded per new fold on the tail (best effort)
  answered turn and input waiting ─► one more model call (Codex tasks/regular.rs:117-120)
report {folded_through_event_id} ─► meter_lease.sql says folds wait ─► settle_folded.sql
       settle links at or below the cursor under this fence, write their event rows; release every other link
```

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(fleet): fold chat messages into the running run, and answer each send's disposition
- **Intent (one sentence):** a message sent mid-run joins the run already answering, so a burst is one conversation instead of a queue of runs.
- **Authoring handshake:** Indy wants a chat message that arrives mid-run to fold into that run, read by the runner holding the lease at each model call under its fence, with compaction making it wait and both surfaces saying "compacting". Each send answers its disposition; each folded message keeps its own event row and settles once with its run. ASSUMPTIONS I'M MAKING:
  1. Folding covers steer-route messages only (producer `steer`, no reply destination). Slack mentions are `chat` too but each owes a reply to its own thread (`afd_ingress/src/slack/admit.rs:62-71`); they wait, like webhooks.
  2. The runner reads over a new fenced route, because the runner stream arrives in M243, which moves this read onto it.
  3. The disposition is a prediction made at send time. The admission's link and the `event_folded` frame are the truth; a run that ends first leaves the message to start the next run.
  4. A run's reply joins every answering turn's text, so an answer given before a fold is not lost.
  5. CHORE(open) re-verifies Files Changed, the loop under `afr_agent/src/loop/`, the steer path and the schema slots against `origin/main`, because Pull Request (PR) #803 may merge first.
- **Implementer handshake:** recorded in the Pull Request Session Notes at PLAN, before any edit (SPEC-HANDSHAKE).

## Implementing agent — read these first

1. `rustd/crates/runner/afr_agent/src/loop/mod.rs` and `loop/conversation.rs` — the turn loop, and how a child joins input onto a waiting user message; the root's inbox mirrors that join.
2. `rustd/crates/daemon/afd_fleet/src/lease/sql/meter_lease.sql` and `sql/lease/claim_admission_head.sql` — the fence and settle predicates every fold step must keep.
3. `rustd/crates/daemon/afd_fleet/src/lease/context.rs`, `afd_api_runner/src/handler/runner/context.rs` and `afr_supervisor/src/context.rs` — the fenced runner route, end to end, that the folds route copies.
4. `docs/architecture/push_scheduler.md` — the decisions, the fact-to-store table and the "A message arrives" sequence this spec builds.
5. `cli/src/commands/fleet_steer_events.ts` — the CLI filters frames by its own `event_id` (`:88-89`), which a folded message must widen.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `docs/v2/pending/M242_002_P1_API_CLI_DOCS_INFRA_OBS_UI_MESSAGES_FOLD_INTO_RUNNING_RUNS.md` | CREATE, then MOVE to `active/` and `done/` | this spec |
| `schema/2710_fleet_admissions_folded_into.sql`, `schema/2720_fleet_events_folded_into.sql`, `schema/2730_runner_fleet_slots_compacting.sql` | CREATE | §1: three columns; placeholder slots, renumbered at CHORE(open) |
| `rustd/crates/daemon/afd_db/src/migration.rs` | EDIT | §1: registers the three slots |
| `rustd/crates/daemon/afd_fleet/src/lease/fold.rs`, `lease/mod.rs`, `lease/sql/lease/fold_pending.sql`, `lease/sql/queue.rs`, `lease/pull/mod.rs` | CREATE, EDIT | §1: the fenced fold verb on `Leases` and `Plane` |
| `rustd/crates/daemon/afd_fleet/src/lease/sql/lease/claim_admission_head.sql` | EDIT | §1: a claim clears the compacting marker |
| `rustd/crates/protocol/af_wire/src/paths.rs`, `af_wire/src/lease/folds.rs`, `af_wire/src/lease/mod.rs` | EDIT, CREATE | §1: `LEASE_FOLDS` and its request and response types |
| `rustd/crates/daemon/afd_http/src/route/runner.rs`, `afd_http/src/services/leasing/mod.rs`, `leasing/plane.rs` | EDIT | §1: `RunnerRoute::Folds` and the `Leasing::fold` seam |
| `rustd/crates/daemon/afd_api_runner/src/handler/runner/folds.rs`, `handler/runner/mod.rs`, `src/lib.rs`, `src/openapi.rs` | CREATE, EDIT | §1: the handler, its mount and its annotation |
| `rustd/crates/daemon/afd_api/tests/route_inventory.rs`, `route_meta_total.rs`, `tests/harness/stubs_runner.rs` | EDIT | §1: `INVENTORY`, the counts, the `NoWork` stub |
| `rustd/crates/daemon/afd_fleet/src/lease/sql/meter_lease.sql`, `sql/meter.rs`, `lease/settle.rs`, `lease/commit.rs` | EDIT | §2: `folds_waiting` from the guard; the fold settle runs only then |
| `rustd/crates/daemon/afd_fleet/src/lease/sql/lease/settle_folded.sql`, `lease/settle_folded.rs` | CREATE | §2: settle and release a head's folds |
| `rustd/crates/daemon/afd_fleet/src/lease/sql/lease/settle_terminal.sql` | EDIT | §2: a refused head releases its folds |
| `rustd/crates/daemon/afd_fleet/src/lease/report/mod.rs`, `report/steps.rs`, `afd_api_runner/src/handler/runner/report.rs` | EDIT | §2: the cursor reaches the settle; each settled fold's `event_complete` |
| `rustd/crates/protocol/af_wire/src/report/mod.rs` | EDIT | §2: `ReportRequest.folded_through_event_id` |
| `rustd/crates/daemon/afd_fleet/src/lease/history.rs`, `lease/history_tests.rs` | EDIT | §2: a run's folds join its turn |
| `rustd/crates/daemon/afd_events/src/history/row.rs`, `history/detail.rs`, every statement under `history/statement/` | EDIT | §2, §4: each page and the thread carry `folded_into_event_id`, as each carries `resumes_event_id` |
| `rustd/crates/runner/afr_agent/src/engine/mod.rs`, `engine/inbox.rs` | EDIT, CREATE | §3: the `Inbox` port, `AgentRun.inbox`, `RunOutput.folded_through_event_id` |
| `rustd/crates/runner/afr_agent/src/loop/mod.rs`, `loop/inbox.rs`, `loop/shared.rs`, `loop/context/mod.rs`, `loop/finish.rs` | EDIT, CREATE | §3: the boundary read, the compaction mark, the continuing turn, the joined reply |
| `rustd/crates/runner/afr_agent/src/error/mod.rs`, `error/raise.rs`, `src/testing.rs` | EDIT | §3: the `Inbox` error kind; a scripted inbox double |
| `rustd/crates/runner/afr_agent/src/loop/inbox_tests.rs`, `turn_tests.rs`, `compaction_tests.rs`, every test building an `AgentRun` (`engine/tests.rs`, `loop/*_tests.rs`, `nested/run_tests.rs`, `af_providers/tests/support/mod.rs`) | CREATE, EDIT | §3: the loop proofs; each literal gains `inbox` |
| `rustd/crates/runner/afr_supervisor/src/inbox.rs`, `client/folds.rs`, `client/mod.rs`, `client/verb.rs`, `lease_loop/drive.rs`, `report/mod.rs`, `lib.rs`, `test_util.rs` | CREATE, EDIT | §3: `LeaseInbox` over the plane; the report carries the cursor |
| `rustd/crates/daemon/afd_admission/src/sql/admit_one.sql` (created by M242_001), `sql/select_repeated.sql`, `admit.rs`, `repeat.rs` | EDIT | §4: the disposition facts, fresh and replayed |
| `rustd/crates/daemon/afd_events/src/steer/mod.rs`, `steer/disposition.rs`, `steer/tests.rs`, `afd_http/src/services/event.rs` | EDIT, CREATE | §4: `Disposition::of`; `Steered` and `FleetSteering` carry it |
| `rustd/crates/protocol/af_wire/src/event/steer.rs`, `af_wire/src/event/mod.rs` | EDIT | §4: `SteerAccepted`, `EventSummary`, `EventDetail` |
| `rustd/crates/daemon/afd_api_wire/src/tail/mod.rs`, `tail/tests.rs` | EDIT | §4: `TailFrame::EventFolded`; `TailRow.folded_into_event_id` |
| `rustd/crates/daemon/afd_api_tenant/src/handler/fleet/message_steer/mod.rs`, `message_steer/tests.rs` | EDIT | §4: the 202 carries the disposition |
| `rustd/crates/daemon/afd_api/src/openapi/stability/mod.rs`, `public/openapi.json` | EDIT | §1, §4: new fields published `beta`; the document regenerated with `--no-banner` |
| `rustd/crates/protocol/af_observability/src/metrics/declared/fleet.rs`, `producers/fleet/mod.rs`, `tests/producers_drive.rs`, `docs/metrics.census.tsv` | EDIT | §4, §2: two counter families |
| `rustd/crates/daemon/afd_fleet/tests/integration_lease_fold.rs`, `integration_lease_fold_settle.rs`, `fleet_suite.rs` | CREATE, EDIT | §1, §2: the store proofs |
| `rustd/crates/daemon/agentsfleetd/tests/integration_runner_folds.rs`, `integration_steer_disposition.rs`, `daemon_suite.rs` | CREATE, EDIT | §1, §3, §4: the router and real-runner proofs |
| `cli/src/commands/fleet_steer.ts`, `fleet_steer_events.ts`, `cli/test/fleet-steer-fold.integration.test.ts`, `fleet-steer-tail.unit.test.ts` | EDIT, CREATE | §5 |
| `ui/packages/app/lib/api/fleets-types.ts`, `lib/api/fleet-steer.ts`, `lib/api/events-types.ts`, `lib/api/events.ts` | EDIT | §6: the 202 and the frame, read through `zod` |
| `ui/packages/app/lib/streaming/fleet-stream-admitted.ts`, `fleet-stream-frames.ts`, `fleet-stream-row.ts`, `workspace-store.ts` and their tests | EDIT | §6: `applyEventFolded`; the wall ignores the frame as it ignores `event_admitted` |
| `ui/packages/app/components/domain/useFleetMessageDelivery.ts`, `fleetMessageStatus.ts`, `useFleetThreadEntries.ts` and their tests | EDIT | §6: the label and the thread order |
| `ui/packages/app/tests/e2e/fleet-fold.spec.ts` | CREATE | §6: the dry-lane render |
| `rustd/crates/daemon/afd_bench/src/lane/lease/drain/runner.rs`, `drain/turn.rs`, `drain/burst/mod.rs`, `burst/tests.rs`, `src/profile/mod.rs`, `profile/tests.rs`, `src/statements/paths.rs`, `statements/tests.rs`, `make/bench.mk` | EDIT | §7: model-call reads, fold pickups, the `input` path, the knob's help text |
| `bench/baselines/lease.rig.json`, `bench/baselines/lease-burst.rig.json` | EDIT | §7: median runs |
| `docs/architecture/push_scheduler.md`, `runner_fleet.md`, `lease_flow.md`, `data_flow.md`, `api_resources.md`, `context_lifecycle.md`, `runner_execution.md` | EDIT | §7: steers fold; compaction holds input; the new columns and baselines |
| `playbooks/operations/acceptance/baselines/M242_002-<sha>.md` | CREATE | §7: the manual evidence |
| `~/Projects/docs`: `fleets/running.mdx`, `cli/agentsfleet.mdx`, `changelog.mdx` on `chore/m242-002-folding-changelog` | EDIT | §7: public behaviour; API pages after this Pull Request merges |
| `ui/packages/app/app/(dashboard)/admin/runners/components/PolicyFields.tsx`, `admin/runners/[runnerId]/components/RunnerIdentityLine.tsx` and their tests | EDIT | a "Durable tasks" on/off control beside Isolation mode and Network egress, defaulting on; the daemon already stores `AssignedPolicy.durable_tasks_enabled` per runner (Indy, Discovery). CHORE(open) confirms off stops durable work reaching that runner |

## Applicable Rules

- **`domains/sql.md`** — SQL-SLOT, SQL-FKNAME, SQL-TIMECOL, SQL-READER, SQL-FILE, SQL-ROW, SQL-LOCKCLOCK, TXN, IDMP, SQL-PLAN
- **`domains/http.md`** — HTTP-NOUN, HTTP-ROUTE, HTTP-GARDE, HTTP-VALID
- **`domains/http-responses.md`** — HTTP-FIELD, HTTP-NULL, HTTP-STATEEXT
- **`domains/http-surface.md`** — HTTP-OPENAPI, HTTP-BREAK, HTTP-ENUM, HTTP-STABILITY, HTTP-SURFACE, HTTP-TESTS, JCL
- **`languages/rust.md`** — ERR-RS, RS-FROM, RS-MAPERR, RS-NEWTYPE
- **`languages/typescript.md`** — TS-SCHEMA, TS-BUNTEST
- **`tests.md`** — TEST-REAL, TEST-INJECT, TEST-RESIDUAL, TEST-ACKLOSS, TEST-KILL, TEST-CONTEND, TEST-LANE
- **`code.md`** — FLL, CODE-SEAM, OWN, TIM
- **`domains/logging.md`** — LOG-EVENT, LOG-HOIST, LOG-CODE
- **`domains/architecture.md`** — ARCH-LAND, ARCH-HOMES
- **`docs.md`** — DOC-02, DOC-03, DOC-14b

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| `docs.language` at `orly gate verify` and `orly gate pr` | yes, seven architecture pages and this spec | sentences under 25 words, three per paragraph, no prose em dashes |
| `make lint-all` (clippy `-D warnings`, `cargo shear`, TypeScript lint, OpenAPI drift) | yes | `public/openapi.json` regenerated from `rustd/` in the commit that changes a wire type |
| Route inventory and meta totals | yes, one runner route | `INVENTORY` and `route_meta_total.rs` move with the route |
| Metrics census and producer tests | yes, two families | census rows, declarations and producers in one commit |
| Coverage gates in `make test-unit-all` | yes, CLI and app modules change | each changed module's test file grows with it |
| File & Function Length (file ≤350 lines, function ≤50, method ≤70) | yes: `loop/mod.rs` is 313 lines and `drive` is near 45 | the boundary and the continuing turn live in `loop/inbox.rs`; the fold settle in `settle_folded.rs` |

## Prior-Art / Reference Implementations

- **Reference:** Codex, `~/Projects/oss/rs/codex/codex-rs/core/src`. `TurnInputQueue` (`session/input_queue.rs:90-92`) is drained by `get_pending_input` with `split_off(0)` (`:400-415`) before each model request (`session/turn.rs:418-433`). Another turn runs while input waits (`tasks/regular.rs:117-120`). Mirror all three. Divergences: the queue is the Postgres ledger, read over a fenced route. Codex defers its drain one sample past an auto-compact (`turn.rs:418-421`); here input folds at the first model call after compaction, as Indy decided. The first model call of a run also drains, because the head's followers waited in the ledger before the run began.
- **Reference:** `afr_agent/src/loop/conversation.rs` `read_input`. A child's input is scrubbed and joined onto a waiting user message so no provider sees two in a row. The root's inbox joins the same way.
- **Reference:** the fenced context trio named in read-first item 3: route, store verb, `retrying` client, and `409 superseded` on a moved fence.
- **Reference:** `schema/1700_fleet_events.sql` `resumes_event_id`, a text link between events with a partial index; `folded_into_event_id` copies its shape.

## Sections (implementation slices)

### §1 — Three columns and the fenced folds route

Three slots, numbered from the placeholder range 2710-2740 and renumbered from `origin/main` at CHORE(open), because PR #803 takes 2190-2690. `fleet.admissions.folded_into_admission_id` is a nullable self reference, `ON DELETE SET NULL`, with a partial index on unsettled linked rows; the fold, settle and thread statements read it. `fleet.events.folded_into_event_id` is nullable text with no foreign key, as `resumes_event_id` has none, plus a partial index on `(fleet_id, folded_into_event_id)`; history and the event pages read it. `runner.fleet_slots.compacting_started_at` is a nullable marker the send's disposition reads.

`fold_pending.sql` is one statement. State authority: `runner.fleet_slots.fencing_token`. Guarded transition: it locks the lease and slot rows `FOR SHARE`, reads `clock_timestamp()` once in a `MATERIALIZED` step, and writes only when the lease is active, its own token equals the slot's, and both deadlines lie ahead. The head is the admission the lease runs: unsettled, unparked, producer `steer`. It links unsettled steer rows after the head and before the first unsettled non-steer row, stamping `lease_fence`; a row is eligible when unlinked or linked to this head under a lower fence. It returns those rows, plus rows this fence linked after `after_event_id`, in `sequence` order, at most `FOLD_BATCH_MAX` (32). With `compacting: true` it sets the marker when unset and returns nothing; with `false` it clears a set marker. Commit before release: the statement commits before the 200, then the handler publishes `event_folded` per new link, best effort. Failover: a reclaim's slot upsert waits on the share lock, and afterwards the old token matches nothing, so the old runner links nothing and gets `409 superseded`.

The route mirrors the context route: `RunnerRoute::Folds` with every HTTP-ROUTE part, a `garde`-validated body with unknown fields refused, `Leasing::fold` on the seam and `NoWork`. The claim's slot upsert clears `compacting_started_at`. **Implementation default:** a route of its own rather than a renewal field, because renewal runs every 5 s and a fold must reach the next model call; M243 moves the read onto the runner stream. **Implementation default:** an `after_event_id` that names no row this fence linked reads as absent, and the runner skips an event id it already holds, so a retry is always safe.

- **Dimension 1.1** — waiting steers fold in order → Test `test_a_fold_read_takes_waiting_steers_in_order` (integration): head `h` and steers `s1`, `s2` → returns `[s1, s2]`; both rows link to `h` under the live fence.
- **Dimension 1.2** — a non-steer row stops the fold → Test `test_a_webhook_between_stops_the_fold` (integration): `h`, `s1`, webhook `w`, `s2` → returns `[s1]`; `s2` stays unlinked, so `w` runs before it.
- **Dimension 1.3** — a moved fence links nothing → Test `test_a_superseded_fold_stamps_nothing` (integration): reclaim to fence 2, then a fence-1 read → `409` with `current_state` `superseded`; no admission row changed.
- **Dimension 1.4** — a lost reply is retried safely → Test `test_a_retried_fold_read_returns_the_same_rows` (integration): two reads with no `after_event_id` both return `[s1, s2]`; one with `after_event_id = s1` returns `[s2]`; links unchanged.
- **Dimension 1.5** — compaction holds input → Test `test_compacting_marks_the_slot_and_folds_nothing` (integration): `compacting: true` with `s1` waiting returns `[]` and sets the marker; `compacting: false` returns `[s1]` and clears it.
- **Dimension 1.6** — a parked head takes no folds → Test `test_a_parked_head_takes_no_folds` (integration): `parked_at` injected on `h` under a live fence → returns `[]`; nothing linked.
- **Dimension 1.7** — a malformed body is refused → Test `test_a_fold_request_with_an_unknown_field_is_400` (integration, router): a body with `"extra": 1` → 400; no statement runs.
- **Dimension 1.8** — another runner's lease is hidden → Test `test_a_fold_on_another_runners_lease_is_404` (integration, router): runner B names runner A's lease → 404; A's slot untouched.
- **Dimension 1.9** — a runner that died compacting leaves no marker → Test `test_a_reclaim_clears_compacting` (integration): marker set, lease lapses, another runner claims → `compacting_started_at` is NULL.

### §2 — Each fold settles once, with its run

The report carries `folded_through_event_id`, the last fold the run appended. `meter_lease.sql` returns `folds_waiting`, whether an unsettled row links to the head, from its existing guard, so a chat with no folds sends no extra statement. When it is true, the report's transaction runs `settle_folded.sql` after `mark_terminal`. That statement settles rows linked to the head under this fence at or below the cursor's `sequence`, writing each one's `fleet.events` row; the row takes the head's final status and failure fields, the admission's `created_at` and the head's event id, with no reply, tokens or wall time. In the same statement it clears the link and `lease_fence` on every other row linked to the head, so each runs next in order. `settle_terminal.sql` releases every row linked to a refused head. After commit, the report publishes `event_complete` for each settled fold.

Authority is `settled_at IS NULL` beside `lease_fence`; everything rides the report's one transaction (TXN); a stale report fails `meter_lease.sql`'s guard and reaches no fold step. History renders a run as one turn: its message, then each folded message in `sequence` order, joined by a blank line as the runner joined them. **Implementation default:** release, never settle, a fold the run did not report consuming, because settling it would mark answered a message no model read.

- **Dimension 2.1** — a report settles its folds once → Test `test_a_report_settles_its_folds_once` (integration): `s1`, `s2` folded; report through `s2` → three rows settled; `s1` and `s2` each have one event row with `folded_into_event_id = h` and status `processed`.
- **Dimension 2.2** — an unconsumed fold runs next → Test `test_rows_past_folded_through_are_released` (integration): report through `s1` → `s1` settled; `s2` unlinked and unsettled; zero unsettled rows link to `h`; the next claim leases `s2`; `agentsfleet_lease_folds_total{outcome="released"}` rises by 1.
- **Dimension 2.3** — contention across a moved fence → Test `test_folds_settle_once_under_contention` (integration): 100 steers released from a barrier while runner A folds; a reclaim gives B fence 2 mid-way; A's report is `Fenced`; after B's report each admission has one `settled_at` and one event row.
- **Dimension 2.4** — a killed runner's folds re-fold once → Test `test_a_killed_runners_folds_refold_once` (integration): A links `s1`, A's task is dropped, the lease lapses; B's first read returns `s1`; B's report settles `h` and `s1`; A's late report is `Fenced`.
- **Dimension 2.5** — a refused head releases its folds → Test `test_a_refused_head_releases_its_folds` (integration): `s1` linked under fence 1, the lapsed head redelivered and refused at issue by an injected budget gate → `h` settled, `s1` unlinked and next.
- **Dimension 2.6** — a predicted fold no runner read runs next → Test `test_an_unread_fold_runs_as_the_next_head` (integration): a send answers `folded`, the run reports without reading → `s1` becomes the head and starts.
- **Dimension 2.7** — history keeps the conversation → Test `history_joins_folded_messages_into_their_run` (unit): rows `h` ("a", answer "A") and `f` ("b", linked to `h`) → one turn, message `a\n\nb`, answer `A`.

### §3 — The runner reads its inbox at each model-call boundary

`afr_agent` gains an `Inbox` port beside `ContextCheckpoints`, carried on `AgentRun`. Only `Harness::root` holds it. At each boundary the root runs preflight, reads its inbox, and runs preflight again when the read returned input, so folded text sits inside the window the call is sized for. Before `effects::compact` the root marks compacting; that read returns nothing. An answered turn reads once more before ending: input keeps the loop going with the answer kept in the conversation, as Codex continues. The reply joins each answering turn's text with a blank line. `RunOutput` carries the last folded event id, whatever the ending.

A failed read logs `lease_input_read_failed` at `warn` with its `error_code` and the run goes on; the next boundary reads again. `afr_supervisor`'s `LeaseInbox` calls the folds route through `retrying`, and the report carries the cursor. The daemon, runner, command-line interface (CLI) and web app ship together, so no build meets another's older wire (Indy, M242_001 Discovery, Nov 06, 2026: 10:22 PM). **Implementation default:** the first model call drains too, unlike Codex, because the head's followers waited in the ledger before the run began.

- **Dimension 3.1** — input joins the next call → Test `inbox_input_joins_the_next_model_call` (unit): turn 1 calls a tool; the inbox holds "m2" → turn 2's request ends with the tool result, then user "m2".
- **Dimension 3.2** — compaction holds input to the first call after → Test `input_during_compaction_folds_at_the_next_call` (unit): "m2" arrives while compaction runs → the summary input lacks "m2"; the first request after carries it; the inbox saw the compacting mark first.
- **Dimension 3.3** — an answered turn continues while input waits → Test `answered_turn_continues_while_input_waits` (unit): turn 1 answers "A1", the inbox holds "m2", turn 2 answers "A2" → `Answered("A1\n\nA2")` with the cursor at "m2".
- **Dimension 3.4** — child loops never read the inbox → Test `child_loops_never_read_the_inbox` (unit): a nested child runs while "m2" waits → no child request carries "m2"; the root's next request does.
- **Dimension 3.5** — a failed read keeps the run going → Test `failed_inbox_read_keeps_the_run_going` (unit): the first read errs, the second returns "m2" → the run completes with "m2" folded and one `lease_input_read_failed` record.
- **Dimension 3.6** — a stopped run still reports its cursor → Test `stopped_run_reports_its_folded_through` (unit): stop after "m2" folds → `Failed` output with the cursor at "m2".
- **Dimension 3.7** — two sends become one run through the shipped daemon → Test `test_runner_folds_a_second_send_into_one_run` (e2e, real runner, scripted provider holding turn 1 on a barrier): send 1 answers `started`, send 2 answers `folded`; one lease issued; both events settled; the report carried the cursor.

### §4 — The send answers its disposition

For steer sends only, M242_001's intake statement reads the fleet's slot by primary key and the earliest unsettled admission before the new row. A pure `Disposition::of` in `afd_events` decides. No earlier unsettled admission and no live lease → `started`. A live lease on an unparked steer head with no compacting marker → `folded`; with the marker → `waiting_compaction`. Anything else → `queued`. "Live" means the slot's `lease_expires_at` lies ahead of the statement clock, so a lapsed marker is ignored. A replay answers from the stored row: `folded` when linked, `started` when it was delivered as a head, otherwise from the same facts read now; `select_repeated.sql` returns them. `folded_into_event_id` accompanies `folded`, `waiting_compaction` and a linked replay.

The 202 keeps `status`, `event_id` and `replayed`. `TailFrame::EventFolded` and `folded_into_event_id` on `EventSummary`, `EventDetail`, `TailRow` and the thread rows are new; the thread's ledger half carries the link for unsettled folds. **Implementation default:** the facts are statement columns and the decision is Rust, so the truth table is a unit test. **Implementation default:** the new fields publish `beta`, because M243 may reshape the disposition.

- **Dimension 4.1** — the truth table holds → Test `disposition_follows_the_run_state` (unit): six fact sets → `started`, `folded`, `waiting_compaction`, `queued` (webhook head), `queued` (parked head), `queued` (no lease, earlier row).
- **Dimension 4.2** — each disposition through the route → Test `test_a_send_answers_each_disposition` (integration, router): idle fleet, live steer run, compacting run, live webhook run → the four values, with `folded_into_event_id` on the two fold answers; each counts once in `agentsfleet_steer_dispositions_total`.
- **Dimension 4.3** — a replay answers its fold → Test `test_a_replayed_send_answers_its_fold` (integration): resend the folded send's `operation_id` → `replayed: true`, `folded`, the same link; one admission row.
- **Dimension 4.4** — a parked head queues the send → Test `test_a_send_behind_a_parked_head_is_queued` (integration): head parked on an open gate → `queued`, no link.
- **Dimension 4.5** — a lapsed marker is ignored → Test `test_a_compacting_marker_on_a_lapsed_lease_is_ignored` (integration): marker set, lease expired, no other row → `started`.
- **Dimension 4.6** — the thread shows a fold before settle → Test `test_the_thread_shows_a_folded_message_before_settle` (integration): after a fold, the thread's queued row for `s1` carries `folded_into_event_id = h`.

### §5 — The CLI says where the message went

`agentsfleet steer` prints one line per disposition. `queued` prints "queued behind the current run". `folded` prints "added to the current run", then renders the run's frames from the `event_folded` frame for its own event onward. `waiting_compaction` prints "waiting: the fleet is compacting its context". The command still ends on `event_complete` for its own event id. A tail that times out falls back to the event poll; a polled event with `folded_into_event_id` renders that run's reply. `--json` adds `disposition` and `folded_into_event_id`, with no new error code (JCL). The six command-line practices hold as today: layers apart, handlers that neither print nor exit, one renderer, structured errors, three test tiers, JSON when stdout is not a terminal.

- **Dimension 5.1** — a folded send renders its run after the fold → Test `steer renders the run after its fold` (integration, mock API): 202 `folded`; a chunk for `h`, then `event_folded`, then a chunk for `h` → only the second chunk prints; exit 0 on `event_complete`.
- **Dimension 5.2** — compaction is named → Test `steer says compacting while it waits` (unit): 202 `waiting_compaction` → the compacting line prints once.
- **Dimension 5.3** — a lost fold frame falls back → Test `steer settles a folded send from the poll when its fold frame is lost` (integration, mock API): no `event_folded`, tail timeout → the poll reads `s1` linked to `h` and renders `h`'s reply.
- **Dimension 5.4** — CUT — every component ships together (Indy, M242_001 Discovery, Nov 06, 2026: 10:22 PM).
- **Dimension 5.5** — scripts see the disposition → Test `steer --json prints the disposition` (unit): 202 `folded` → the JSON line holds `disposition` and `folded_into_event_id`.

### §6 — The chat shows a folded message in its run

`postSteer` reads `disposition` as its four-value enum and `folded_into_event_id` as optional, through `zod`. A known value labels the pending row: "Added to the current reply", "Waiting: compacting context" or "Queued"; `started` keeps today's row. `applyEventFolded` links a waiting row to its run on the `event_folded` frame. The thread renders a run's folded messages between its message and its reply, so the reply moves below a new fold once. The existing "Context summarizing…" status is unchanged.

- **Dimension 6.1** — a folded send is labelled → Test `useFleetMessageDelivery labels a folded send` (unit): 202 `folded` → the row reads "Added to the current reply".
- **Dimension 6.2** — the frame links a waiting row → Test `applyEventFolded marks the waiting row folded` (unit): a queued row for `s1` and `event_folded(s1 → h)` → the row links to `h`.
- **Dimension 6.3** — the reply follows its folds → Test `thread entries place folded messages before the run's reply` (unit): `h`, its reply and `s1` linked to `h` → order `h`, `s1`, reply.
- **Dimension 6.4** — CUT — every component ships together (Indy, M242_001 Discovery, Nov 06, 2026: 10:22 PM).
- **Dimension 6.5** — the page renders a folded send → Test `fleet-fold.spec.ts renders a folded send` (e2e, dry lane): stubbed 202 `folded` and frames → the label shows and the reply sits below the folded message.

### §7 — The burst proves it, and the pages say it

The drain's simulated runner reads the folds verb through `Plane` every `BENCH_MODEL_CALL_MS` (default 500, at most `BENCH_TURN_MS`). After its turn it keeps making model calls while the last read returned input, then reports with the cursor. Under `BENCH_SHAPE=burst`, `BENCH_RUNNERS` defaults to `BENCH_FLEETS`, and runners read for two model calls after the last append. A fold counts as a pickup; `burst_folded` counts them; `burst_unleased` counts messages neither leased nor folded. `statements/paths.rs` attributes the fold statement to `input`. Each lane runs three times; the median run becomes the baseline, and `push_scheduler.md` §Baselines records all three.

**Implementation default:** one simulated worker per bursting fleet, because a real host runs many leases through its worker pool and the bench measures the scheduler, not host capacity. **Implementation default:** a 500 ms simulated model call, so the fold path, not the model, sets pickup; the open blocker on the 1 s target names the gap. The architecture pages change in the commit that changes the behaviour (ARCH-LAND).

- **Dimension 7.1** — a fold is a pickup → Test `burst_counts_a_fold_as_pickup` (unit): 3 appends, 1 leased, 2 folded → `burst_unleased` 0, `burst_folded` 2.
- **Dimension 7.2** — the fold statement has a path → Test `fold_statement_counts_as_input` (unit): the fold statement's normalized text → `input`, not `other`.
- **Dimension 7.3** — the knob refuses nonsense → Test `model_call_longer_than_turn_is_refused` (unit): `BENCH_MODEL_CALL_MS=3000` with `BENCH_TURN_MS=2000` fails naming the cap.
- **Dimension 7.4** — a fold-free chat costs the report path nothing → Test `make bench-lease PROFILE=rig` (integration): `drain_statements_per_lease_report` equals M241's committed baseline.
- **Dimension 7.5** — the burst meets the bar → Test `make bench-lease PROFILE=rig BENCH_SHAPE=burst` (integration): R1, R2 and R3 hold on the committed median run.
- **Dimension 7.6** — folding works on dev → Test `fold_on_dev` (manual): Indy sends two `agentsfleet steer` messages to the dev "Support triage" fleet within one run. The second prints `added to the current run`, and `agentsfleet events <fleet_id> --json` shows its link. Evidence lands in the acceptance baseline file with a chat screenshot.

## Interfaces

- **Send:** `POST /v1/workspaces/{workspace_id}/fleets/{fleet_id}/messages` answers 202 with two new fields. `disposition` is one of `started`, `folded`, `queued`, `waiting_compaction`; `folded_into_event_id` is omitted unless named above. Example: `{"status":"accepted","event_id":"0193b2c4-…-a2","replayed":false,"disposition":"folded","folded_into_event_id":"0193b2c4-…-a1"}`.
- **Folds:** `POST /v1/runners/me/leases/{lease_id}/folds`, runner bearer. Request `{"fencing_token":7,"after_event_id":"0193…-a1","compacting":false}`; `after_event_id` is optional. 200 `{"messages":[{"event_id":"0193…-a2","message":"also check the queue depth"}]}`, at most 32 messages. 400 malformed; 404 not this runner's lease; 409 `current_state` `superseded`.
- **Report:** `ReportRequest.folded_through_event_id`, optional; absent settles no fold and releases every link to the head.
- **Tail:** `{"kind":"event_folded","event_id":"…-a2","folded_into_event_id":"…-a1"}`. It is a new frame kind, so the Pull Request description names it under HTTP-ENUM; both clients skip kinds they do not know (`ui/packages/app/lib/streaming/fleet-stream-frames.ts:70-75`, `cli/src/commands/fleet_steer_events.ts:88-108`).
- **Events:** `folded_into_event_id`, optional, on `EventSummary`, `EventDetail`, `TailRow` and thread rows. Every new field is `beta` (HTTP-STABILITY). `disposition` is a new field, so no existing enum gains a value.
- **Runner port:** `afr_agent::Inbox` with `read(after: Option<&str>) -> Result<Vec<Folded>>` and `compacting() -> Result<()>`; `AgentRun.inbox`; `RunOutput.folded_through_event_id`.
- **Daemon seam:** `Leasing::fold(runner, lease, &Valid<FoldRequest>) -> Result<Folds>`.
- **Bench:** `BENCH_MODEL_CALL_MS` (default 500, cap `BENCH_TURN_MS`); result keys `burst_folded` and `drain_statements_per_lease_input`.

## Failure Modes

| Mode | Kind | Cause | Handling (response and what the caller observes) | Test |
|------|------|-------|--------------------------------------------------|------|
| Fold races a reclaim | failure | the fence moves before the read | 409 `superseded`; nothing linked; renewal stops the old run | `test_a_superseded_fold_stamps_nothing` |
| Many sends while the fence moves | failure | 100 steers, a fold and a reclaim at once | each admission settles once, by the fence that consumed it | `test_folds_settle_once_under_contention` |
| Fold reply lost | failure | network after commit | the retry returns the same rows; no second link | `test_a_retried_fold_read_returns_the_same_rows` |
| Runner killed after folding | failure | crash, `SIGKILL` | the next fence re-folds; the late report is `Fenced` | `test_a_killed_runners_folds_refold_once` |
| Run ends before consuming a link | failure | final read reply lost, stop | the link is released and runs next | `test_rows_past_folded_through_are_released` |
| Head refused at issue | failure | budget or gate refusal on redelivery | links released, each runs in order | `test_a_refused_head_releases_its_folds` |
| Runner never reads | failure | a runner that dies before its first read | the predicted fold starts the next run | `test_an_unread_fold_runs_as_the_next_head` |
| Runner died compacting | failure | crash mid-summary | a claim clears the marker; a lapsed one is ignored | `test_a_reclaim_clears_compacting` |
| Fold read fails on the runner | failure | daemon unreachable, or the read times out | `warn`, the run goes on, the next boundary retries | `failed_inbox_read_keeps_the_run_going` |
| Malformed fold body | failure | unknown field, bad bound | 400 | `test_a_fold_request_with_an_unknown_field_is_400` |
| Foreign lease named | security | runner B names A's lease | 404; nothing read | `test_a_fold_on_another_runners_lease_is_404` |
| Replayed send | failure | same `operation_id` again | same event and disposition, `replayed: true`; one row | `test_a_replayed_send_answers_its_fold` |
| Parked head | failure | an open approval gate | `queued`; no fold | `test_a_parked_head_takes_no_folds` |
| Non-steer between | failure | a webhook queued after the head | the fold stops before it; order holds | `test_a_webhook_between_stops_the_fold` |
| Fold frame lost | failure | tail drop | the CLI polls; the chat reads the event detail | `steer settles a folded send from the poll when its fold frame is lost` |
| Run stopped after folding | failure | lease stop | consumed folds settle with the run's failure | `stopped_run_reports_its_folded_through` |

## Invariants

1. A fold writes only under the live fence — `fold_pending.sql` gates every write on its fence step; the counterexample is `test_a_superseded_fold_stamps_nothing`.
2. A fold settles only with its head, under the fence that linked it, at or below the reported cursor — `settle_folded.sql`'s predicate with `settled_at IS NULL`; `test_folds_settle_once_under_contention`.
3. No unsettled row stays linked to a settled head — `settle_folded.sql` and `settle_terminal.sql` release every other link in the statement that settles the head; `test_rows_past_folded_through_are_released` counts zero.
4. Folding keeps `sequence` order — the statement links only the steer prefix before the first unsettled non-steer row; `test_a_webhook_between_stops_the_fold`.
5. Only the root loop reads the inbox — the field is an `Option` that only `Harness::root` fills, so a child holds nothing to read; `child_loops_never_read_the_inbox`.
6. One function decides a disposition — `Disposition::of` over the statement's facts; `disposition_follows_the_run_state`.

## Metrics & Observability

| Metric / event | Audience | Fires when | Properties allowed | Privacy guard | Test |
|----------------|----------|------------|--------------------|---------------|------|
| `agentsfleet_steer_dispositions_total` | operator | a steer is answered | `disposition`, four values | no fleet, actor or text | `test_a_send_answers_each_disposition` |
| `agentsfleet_lease_folds_total` | operator | a fold links or a settle releases | `outcome`: `folded`, `released` | no fleet, actor or text | `test_rows_past_folded_through_are_released` |
| `lease_input_read_failed` (runner log, `warn`) | operator | an inbox read fails | `error_code`, `lease_id` | no message text | `failed_inbox_read_keeps_the_run_going` |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Burst pickup p95 is under 1 s (§7) | `jq -e '.measurements.burst_pickup_p95_ms < 1000' bench/baselines/lease-burst.rig.json` | `true`, exit 0 | P0 | |
| R2 | At most 1% of burst messages go unpicked (§7) | `jq -e '.measurements.burst_unleased <= .measurements.burst_accepted / 100' bench/baselines/lease-burst.rig.json` | `true`, exit 0 | P0 | |
| R3 | Burst messages fold rather than queue (§3, §7) | `jq -e '.measurements.burst_folded > .measurements.burst_accepted / 2' bench/baselines/lease-burst.rig.json` | `true`, exit 0 | P0 | |
| R4 | The send publishes its disposition (§4) | `jq -e '.components.schemas.SteerAccepted.properties.disposition' public/openapi.json` | exit 0 | P0 | |
| R5 | The runner's folds route is published (§1) | `jq -e '.paths["/v1/runners/me/leases/{lease_id}/folds"].post' public/openapi.json` | exit 0 | P0 | |
| R6 | The race and kill proofs sit in the integration lane (§2) | `git grep -n -w -e test_folds_settle_once_under_contention -e test_a_killed_runners_folds_refold_once -- rustd/crates/daemon/afd_fleet/tests` | two hits; S3 runs them | P0 | |
| R7 | The chat renders a folded send (§6) | `make dry-app` | exit 0 | P1 | |
| R8 | Two CLI sends fold on dev (§7) | Manual: Dimension 7.6; evidence in `playbooks/operations/acceptance/baselines/M242_002-<sha>.md` | the second send prints `added to the current run`; its event links the first | P1 | |
| R9 | No page says a steer waits for the run (§7) | `git grep -n -e "The current run finishes first; the steer runs next" -e "It waits behind the fleet's running lease" -- docs/architecture` | no output | P1 | |
| S1 | Lint | `make lint-all` | exit 0 | P0 | |
| S2 | Unit | `make test-unit-all` | exit 0 | P0 | |
| S3 | Integration | `make test-integration-rustd` | exit 0 | P0 | |
| S4 | Version | `make check-version` | exit 0 | P0 | |
| S5 | No secrets | `gitleaks detect --redact --no-banner` | exit 0 | P0 | |

## Dead Code Sweep

N/A: no files or symbols are deleted. `read_input` stays for child loops.

Schema this milestone leaves with no runtime reader or writer is dropped in a new slot in this Pull Request, under Indy's blanket approval (Discovery). Each drop gets a row above, with a `git grep -n -w` over `rustd`, `ui` and `cli` proving nothing still reads or writes it.

## Out of Scope

- Webhook and Slack same-thread coalescing: out of the programme until a webhook burst is measured (Indy, Discovery); each owes a reply to its own destination.
- Interrupting a model call for new input: a fold waits for the call in flight, as in Codex (Indy, Discovery).
- Carrying this read on the runner stream: M243 moves the folds request onto the stream, and nothing pushes input down it.
- Gate checks at fold time: the head's gates admitted the run's tools at issue, renewal meters money, and a message adds no capability.
- A fold cap per run: runtime, budget and compaction already bound a run.
- Folding into setup-chat (authoring) leases: a different lease kind with its own route.
- Respawn from a checkpoint after #803: M247. Until then a respawned head re-folds its links from the ledger (Dimension 2.4).

---

## Product Clarity (authoring record)

1. **Successful user moment** — In a busy fleet, a person sends a second message while the first reply streams. It is marked "Added to the current reply", and that reply answers it with no second run.
2. **Preserved user behaviour** — A message to an idle fleet starts a run as today. An `operation_id` retry answers the first send. `status`, `event_id` and `replayed` keep their meaning, and a message behind a webhook, schedule or parked approval waits as today.
3. **Optimal-way check** — The direct way is Codex's: the run holding the fleet drains pending input before each model call. The gap is one HTTP round trip per model call until M243's stream carries it.
4. **Rebuild-vs-iterate** — Iterate. The ledger, the fence and the loop stay; folding adds three columns, one runner route and one loop boundary.
5. **What we build** — The folds route and statement; the root's inbox; the fold settle and release; the disposition on the send; the CLI lines and chat labels; fold pickups in the burst bench.
6. **What we do NOT build** — A model-call interrupt, because Codex waits too. Webhook and Slack folding, because each owes its own reply. A per-run fold cap, because runtime, budget and compaction bound a run. A daemon-side input queue, because Postgres is the queue.
7. **Fit with existing features** — Compounds with M242_001's intake statement, compaction progress frames and M247's respawn, which re-folds from the ledger. It must not destabilize the fence: a run still settles exactly once.
8. **Surface order** — Both at once. The CLI and the chat read the same 202 and frames, and either one alone would show a folded message as stalled.
9. **Dashboard restraint** — No fold counts or compaction timers on the dashboard. The message's own label is the only new screen element; counts stay operator metrics.
10. **Confused-user next step** — `agentsfleet events <fleet_id> --json` shows each message's `folded_into_event_id`. The chat label names where it went, and the docs page "Running fleets" explains folding.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** columns and the fenced read first, then the settle that keeps exactly-once, then the runner that calls both, then the send's answer, the two surfaces and the proof. Each Section is testable on the one before it.
- **Alternatives considered:** a fold flag on renewal, rejected because a 5 s renewal cannot reach the next model call. A daemon-side owner queue per fleet, rejected because Postgres already orders and fences, and Indy asked for the smallest design. Event rows written at fold time, rejected because a released fold would leave a row claiming a run it never joined. Folding Slack same-thread mentions, rejected for now because each mention carries its own reply obligation.
- **Patch-vs-refactor verdict:** a **patch** because the claim, fence and report paths keep their shape and gain one predicate, one statement and one loop boundary.

## Discovery (consult log)

- **Consults** — Source review on `feat/m241-push-scheduler-baselines` at `dffdbc34c` (main `14539a73e` plus M241), Nov 06, 2026:
  - Only child loops read input (`afr_agent/src/loop/conversation.rs:13-26`), before preflight (`loop/mod.rs:197-198`); a turn without tool calls ends the run (`loop/mod.rs:209-218`).
  - Compaction runs inside preflight (`loop/context/mod.rs:85-96`) and emits summarizing and resuming frames (`loop/context/effects/progress.rs:30-53`).
  - The claim stamps `lease_fence` on the head (`claim_admission_head.sql:45-58`); a report settles only that fence's head (`meter_lease.sql:36-40,55-61`); `settle_terminal.sql:1-6` settles a refused head.
  - A steer is producer `steer` with no reply (`afd_events/src/steer/mod.rs:319-344`); Slack mentions are `chat` with a thread reply (`afd_ingress/src/slack/admit.rs:62-71`); activation is a keyed steer (`afd_authoring/src/creation/start/admit.rs:25-33`).
  - The 202 is `SteerAccepted { status, event_id, replayed }` (`af_wire/src/event/steer.rs:85-96`), built at `message_steer/mod.rs:227`.
  - The thread read returns unsettled steer admissions beside events (`afd_events/src/history/statement/select_thread_page.sql:15-26`).
  - Event pages share `EventRow`, and all 16 statements under `history/statement/` select `resumes_event_id` (`git grep -l resumes_event_id -- rustd/crates/daemon/afd_events/src/history/statement`).
  - New response fields default `stable` unless listed in `afd_api/src/openapi/stability/mod.rs`.
  - `afr_agent` errors reuse registry codes (`afr_agent/src/error/mod.rs:8-14`); this spec mints no `UZ-` code.
  - M241's short burst: `burst_unleased` 480 of 500, `burst_pickup_p95_ms` 8 560.6 (`jq .measurements bench/results/lease-burst.rig.json`).
  - Cost, unmeasured arithmetic: each model call adds one `input` statement, so a 10 s simulated chat at 500 ms calls adds about 21. M243 moves the read onto the stream, and §7's baseline records the real figure.
- **Owner decisions** —
  > Indy (Nov 06, 2026: 08:50 AM): "And we are not in production yet, so i want to rethink the approach on admissions, lease, and so on as i am focussed on scale. If something fails in one host bare metal, it should automatically cordon and respawn in another runner" — context: the programme; Dimension 2.4 keeps folds safe across a respawn.
  > Indy (Nov 06, 2026: 10:43 AM): "1. Yes placement rules are via the tags and labels 2. Respawn across ownership - this is a tenant setting ... Based on the scope i would like to build this ... 4. Yes, but donot overengineer now, we can improve this." — context: no daemon-side queue, no fold cap, no gate check at fold time.
  > Indy (Nov 06, 2026: 11:57 AM): "Yes i am ok with postgres store and accepting messages, fences runs" and "The message has to wait. I assume you show message like compacting" and "assume atleast 50 fleets" — context: Postgres holds the queue and the fence; `waiting_compaction`; the 50-fleet burst.
  > Indy (Nov 06, 2026: 02:13 PM): "And i prefer we create all the spects and close M241 and push a PR?" — context: this spec lands with M241.
  > Indy (Nov 06, 2026: 04:06 PM): chose "Next model call (Recommended)" for the 1 s target — context: a folded message joins at the run's next model call, as Codex does; the in-flight call is never interrupted, and the bench's 500 ms call keeps pickup p95 under 1 s.
  > Indy (Nov 06, 2026: 04:06 PM): chose "Not now (Recommended)" for webhook and Slack coalescing — context: out of the programme until a webhook burst is measured; chat folding covers the 50 fleets × 10 messages a second burst.
  > Indy (Nov 06, 2026: 04:18 PM): "yes" — context: blanket approval, asked as "Should each milestone drop the tables and columns it leaves with no reader or writer?": a milestone drops, in a new slot in its own Pull Request, any table, column or index it leaves with no runtime reader or writer (`.orly/standards/domains/sql.md` destructive-change approval).
  > Indy (Nov 06, 2026: 09:20 PM): "Well i mean the ui currently captures on a per runner  (eg: landlock, allow network egress and so on, similarly this must be configured from the runner config) is my request." — context: `durable_tasks_v1` set per runner in the admin UI, carried in this Pull Request.
  > Indy (Nov 06, 2026: 11:25 PM): chose "Reuse #803 path (Recommended)" — context: a message sent mid-run reaches the running lease through #803's input claims (`pending_inputs.sql`, `input_claim_lease_id`/`input_claim_fence`, `settle_source.sql`) instead of a parallel `/folds` route and `Inbox`. CHORE(open) reworks §1–§3 on that path and drops `fold_pending.sql`, `settle_folded.sql` and the slots the reuse leaves unneeded; first it verifies whether every chat run is a durable task under `durable_tasks_v1`. A runner with Durable tasks off keeps today's queueing.
  > Indy (Nov 06, 2026: 11:25 PM): chose "Same gate (Recommended)" — context: no M242_002 product SQL (statements or slots, including the `admit_one.sql` disposition change) is committed without his line-by-line approval; test fixtures are exempt.
  > Indy (Nov 06, 2026: 11:25 PM): chose "Wait for the call (Recommended)" — context: a message arriving mid model call joins at the next model call; the in-flight call is never interrupted.
  > Indy (Nov 06, 2026: 11:33 PM): chose "Approve as drafted (Recommended)" — context: the disposition facts below are added to `admit_one.sql`; Rust answers `folded` (a person's send, a live durable-task run in the same conversation), `started` (nothing waiting, no live run) or `queued`. The burst mislabel is accepted: the label can read `started` when the send runs second.
  > Indy (Nov 06, 2026: 11:33 PM): chose "Drop it, no slots (Recommended)" — context: `waiting_compaction` goes, and with it slots 2710, 2720 and 2730; "folded into which run" is derived through `input_claim_lease_id`. CHORE(open) rewrites §1 and §4 to match.

  > Indy (Nov 07, 2026: 11:05 AM): chose "Plain folds too, in M242" — context: a plain chat fleet's live run (no decision policy, so not a durable task) also takes a person's later message in its conversation at its next model call, so every chat send can fold and §7's burst bar covers plain fleets. This needs a lease-only claim the runner calls between model calls, a claim statement, and the report settling and recording each claimed message; each statement goes through the same line-by-line gate before commit.
  > Indy (Nov 07, 2026: 11:05 AM): chose "Revert, map 55P03 (Recommended)" — context: `afd_fleet/src/task/sql/read_timeout.sql` returns to `origin/main`; the API login's 2 s lock bound applies to task reads, and `afd_fleet` answers SQLSTATE 55P03 as the retryable 503 instead of 500.

  > Indy (Nov 07, 2026: 11:20 AM): "Our harness is common and must be abstract and avoid assumptions like Durable Task vs Plain" and "why are we having a separate model of folds for durable task vs plain? These have to be the same, as engine takes an input and does the processing? Have you referred how codex does this?" — context: one input path in the harness for every run, modelled on Codex's turn-input queue drained before each sampling request (`codex-rs/core/src/session/turn.rs:424-446`, `tasks/regular.rs:120`); durable tasks become an extension the harness calls, never a branch.
  > Indy (Nov 07, 2026: 12:10 PM): chose "Everything in M242" — context: this Pull Request carries the full harness extraction: an `InputSource` port and one drain before every model call; every task-vs-plain branch in `afr_agent/src/loop/` moved behind an `Extension` trait; an embeddable `afr_harness` crate built from ports (`Arc<dyn …>`); `LeaseVerbs` split into plain and task verbs; and a local embedder proving the harness runs outside the runner.
  > Indy (Nov 07, 2026: 12:10 PM): "I want to clean all of them. I want to test and find facts and then fix" — context: every per-transaction Postgres timeout override (`afd_admin` `catalogue_bounds.sql`, `afd_billing` `bound_transaction.sql` and its funds-read bound, `afd_fleet` `read_timeout.sql`, `afd_authoring` `setup_lock_bound.sql`) is removed in an experiment first, the affected lanes run, and each failure is recorded as a fact before any fix.

  > Indy (Nov 07, 2026: 12:25 PM): "How about goof?" — context: the embeddable harness is named `goof` (crate `goof`, type `Goof`), after he asked for a Walt Disney-flavoured name; a plain English word, so no character trademark. Aiwa flagged that it also means "a blunder" in logs and an embedder's types; kept unless Indy changes it before the crate split.
  > Indy (Nov 07, 2026: 04:35 PM): "Can you name the harness as guffbolt" — context: supersedes `goof`; the embeddable harness is the crate `guffbolt`, type `Guffbolt`.
  > Indy (Nov 07, 2026: 05:10 PM): "I prefer guffbolt to be moved to a directory like @rustd/crates/guffbolt/ so we have it there." — context: the harness crate's home.
  > Indy (Nov 07, 2026: 05:40 PM): chose "Full regroup" — "it is time we do this and make crates individual and separate." — context: supersedes the flat `rustd/crates/` layout (`rustd/Cargo.toml:3-9`, his Aug 23, 2026 call).
  > Indy (Nov 07, 2026: 05:52 PM): chose "Five layers + lint", "Rename in this PR" and "Approve path-only edits" — context: `rustd/crates/` becomes `foundation/` → `protocol/` → `harness/` → `runner/` | `daemon/`; a crate depends only on its own folder or a lower one, and `runner/` and `daemon/` never on each other, checked in `make lint-rustd`. Shared crates (foundation, protocol, harness) take the `af_` prefix; daemon-only keep `afd_`, runner-only keep `afr_` (`afd_memory` and `afr_memory` both exist, so one prefix would collide). The path-only edits to `.github/actions/sign-toolbox/action.yml` and `deploy/local/*.sh` are approved.
  - Facts the layout rests on (Nov 07, 2026): `agentsfleetd` links seven runner crates through two edges, `afd_authoring` → `af_providers` and `afd_fleet` → `af_secrets`; the one runtime edge that breaks the layers is `af_providers` → `afr_telemetry`. Fourteen test-only edges cross runner and daemon, led by 14 of `agentsfleetd`'s 88 test files that boot a real runner; those move to a system-test crate.

  Approved disposition SQL (added to `admit_one.sql` after `already_sent`; `$17` binds `LEASE_STATUS_ACTIVE`). Built with one change, listed in the final report for Indy's morning review: the `run_live` column is dropped, and a live run is read as `run_event_id IS NOT NULL`. The two are the same fact, because `live_run` returns a row only with its event, and the change keeps the answer row under clippy's limit on bools per struct:

  ```sql
  live_run AS (
      SELECT l.task_id IS NOT NULL AS takes_input,
             head.reply_provider, head.reply_address,
             e.event_id AS run_event_id
      FROM locked_fleet
      JOIN runner.fleet_slots s ON s.fleet_id = locked_fleet.id
      JOIN runner.leases l
        ON l.fleet_id = s.fleet_id AND l.runner_id = s.last_runner_id
       AND l.fencing_token = s.fencing_token AND l.status = $17
      JOIN fleet.events e ON e.id = l.fleet_event_id
      JOIN fleet.admissions head ON head.id = e.event_id::uuid
      WHERE s.lease_expires_at > statement_timestamp()
  ),
  earlier_waiting AS (
      SELECT EXISTS (
          SELECT 1 FROM fleet.admissions a, locked_fleet
          WHERE a.fleet_id = locked_fleet.id AND a.settled_at IS NULL
      ) AS waiting
  ),
  -- answer row adds:
  --   COALESCE((SELECT takes_input FROM live_run), false) AS run_takes_input,
  --   COALESCE((SELECT reply_provider IS NOT DISTINCT FROM $12::text
  --                AND reply_address IS NOT DISTINCT FROM $13::text FROM live_run), false) AS same_conversation,
  --   (SELECT run_event_id FROM live_run) AS run_event_id,
  --   (SELECT waiting FROM earlier_waiting) AS earlier_waiting
  ```

- **CHORE(open) finding (Nov 07, 2026: 12:06 AM), answered** — a lease runs as a durable task only when its fleet's setup carries a decision policy: the claim binds `crate::task::DECISION_POLICY_PATH` (`afd_fleet/src/lease/assign/ledger.rs:101`, `claim_admission_head.sql`). A plain chat fleet's run is never a task, so #803's input claims alone never fold its sends. Indy chose plain folds too, in this Pull Request (below).
- **Correction (Nov 07, 2026: 11:05 AM), history after a fold settles** — the 12:16 AM entry was wrong and is withdrawn. #803 writes a folded message its own event before settling it: `afd_fleet/src/task/input_receipt/answer.rs:64` inserts the event (event id = the message's admission id, `resumes_event_id` = the run's), `:79` marks it succeeded with its own reply, `:115` settles it; `integration_tasks/input_answers.rs:157-178` proves it. Until then the message is an unsettled admission, which the thread already lists (`select_thread_page.sql:26`). No index or SQL is needed. The chat and CLI fold handling built overnight assumed no reply of their own; both now wait for the folded message's own reply.
- **Timeout experiment (Nov 07, 2026: 01:10 PM), facts** — all five per-transaction overrides removed; `cargo test --all-features -p afd_admin -p afd_billing -p afd_fleet -p afd_authoring -p afd_api --test '*' -- --ignored` after `make _reset-test-db _migrate-test-db`: 963 passed, 6 failed (a first run on a dirty lane showed 5 more, which passed on a fresh schema).
  - `afd_admin` `catalogue_bounds.sql`, `afd_billing` settle bound: 0 failures; identical to the API login's bounds.
  - `afd_billing` funds read 1 s: `funds_cap` saw 55P03 at 2 s instead of 57014 at 1 s; billing counts both as unanswered (`afd_billing/src/error/mod.rs:166-182`), so only the wait changes. Dropping the bound also drops the read's `BEGIN`/`COMMIT`.
  - `afd_fleet` task 3 s: four tests answered 500 or `UZ-INTERNAL-002`, because 55P03 from the login's lock bound went unclassified.
  - `afd_authoring` setup 16 s: Save failed with 55P03 at 2 s instead of answering "installing".
  - Fix applied: `afd_db::error::is_bound_reached` (55P03 or 57014), used by admission, fleet, billing and authoring; activation answers a reached bound as still installing. Seven crates then: 1037 passed, 2 failed, both #803 tests: the stall fixture now turns off its own lock bound, and the blocked-recovery test asserts a deferral, because a migration-locked `fleet.fleets` now ends at 2 s with 55P03 and reads as a busy lock (deferred to the next pass, bounded by `PASS_TIMEOUT`) instead of an outage. Both pass.
  - Answered, SQL-TIMEOUT: the runtime bounds are the API login's `ALTER ROLE` values (`afd_db::login`), with no in-code override and no boot check.
  > Indy (Nov 07, 2026: 04:35 PM): "Cut this bootcheck crap. I dont need additional verification. This is stupid." — context: the proposed boot check that `SHOW lock_timeout` / `statement_timeout` match `afd_db::login` is not built.
- **Model reviews** — skipped on Indy's call: chose "Skip it (Recommended)" (Nov 06, 2026: 11:41 PM), as for M242_001.
- **Open blockers** — none; Indy answered both on Nov 06 (above).
- **Deferrals and transfers** — none. Each one needs an owner quote below, as `> Owner (<name>, <MMM DD, YYYY HH:MM>): "<quote>" — context: <item, why>`.
