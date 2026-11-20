<!--
SPEC AUTHORING RULES (the one comment that survives):
- The spec standard (spec.md) holds every rule. Each tpl: comment names the
  rules that govern its section; delete it once the section is filled.
- Body order is the implementing agent's read order.
- No time or effort estimates, owners or implementation dates (SPEC-ESTIMATE).
-->

# M253_001: Every conversation is a thread of turns, served by runs, recorded as items, and stoppable

**Prototype:** v2.0.0

**Milestone:** M253

**Workstream:** 001

**Date:** Nov 19, 2026

**Status:** IN_PROGRESS

**Priority:** P1, every person and operator reads a conversation through these nouns

**Categories:** API, CLI, DOCS, OBS, UI

**Batch:** B1, first; M253_002 and M253_003 speak this vocabulary

**Depends on:** M247_001, whose fence and checkpoint path the runs keep

**Provenance:** LLM-drafted (Claude Opus 5.5, Nov 19, 2026) from Indy's decisions of Nov 19, 2026

**Canonical architecture:** `docs/architecture/thread_model.md`, which holds every table, column, foreign key and old-to-new mapping; `runner_fleet.md` and `postgres_schemas.md` point at it

---

## Overview

**Goal (testable):** A person stops a running fleet turn from the conversation, and the runner ends that run as stopped within one renewal interval.

**Problem:** A person sees one conversation, but the code holds it in nine tables under three vocabularies. Readers, routes, wire types and NATS subjects repeat every split. A fleet turn cannot be stopped at all.

**Solution summary:**
- **One vocabulary.** A conversation is a **thread**. Each input is a **turn**, each try at answering it is a **run**, and each thing a run does is an **item**. Each answer owed to someone is a **delivery**.
- **Everywhere at once.** The same words name the tables, crates, wire types, routes, CLI commands, screens, subjects and docs. No old word survives, and nothing aliases one.
- **Stop.** A person stops any thread's running turn with one `PATCH`.

**Design flow:**
1. **Send.** A send writes a queued turn on its thread, then publishes `af.thread.{thread_id}.wake`.
2. **Assign.** The daemon locks the thread row, raises its fence, writes a run, and sends the runner `Down::Run`.
3. **Record.** The run's items reach the daemon on the runner's connection and fan out on `af.thread.{thread_id}.items`. The report stores the settling run's items and the turn's answer.
4. **Stop.** A `PATCH` marks the turn cancelled and its run stopping. It then publishes `Command::Cancel` on `af.runner.{runner_id}.cmd`, and the runner ends the run.

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(m253): threads, turns, runs, items and deliveries everywhere, with Stop
- **Intent (one sentence):** People and operators read, follow and stop any conversation with the same few words everywhere.
- **Authoring handshake:**
  - **What Indy asked for:** "Merge tables + rename", "Rename /v1 now", "One Stop, every thread", and "a clean slate approach, no crap thinking about legacy compatibility".
  - **ASSUMPTIONS I'M MAKING:**
    1. **Thread identity.** A thread's id is its fleet's or draft's id, so nothing maps one to the other.
    2. **Sends keep `/messages`.** HTTP-MSGKEY names the `/messages` send; reads and Stop use `/turns`.
    3. **Pre-production.** Schema files are edited in place, and the dev database is reset after merge, per Indy's "Rule holds, reset dev".
    4. **Durable tasks keep their words.** `task` and its tables stay, because the glossary keeps them.
- **Implementer handshake:** recorded in the Pull Request Session Notes at PLAN, before any edit (SPEC-HANDSHAKE).

## Implementing agent — read these first

1. `rustd/crates/daemon/afd_fleet/src/lease/sql/commit_report.sql` — the report transaction that becomes the run report.
2. `rustd/crates/daemon/afd_bus/src/nats/mod.rs` — the subject spellings §6 replaces.
3. `rustd/crates/daemon/afd_gateway/src/session/commands.rs` — how a cancel hint becomes `Down::Cancel` after a Postgres re-read.
4. `rustd/crates/daemon/afd_http/src/route/fleet.rs` — the fleet route family §7 rewrites.
5. `docs/ux.html` — the conversation, activity and setup views the items feed.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `schema/*_runner_slots.sql`, `*_fleet_sessions.sql`, `*_fleet_admissions.sql`, `*_fleet_events.sql`, `*_authoring_messages.sql`, `*_runner_leases.sql`, `*_fleet_tool_calls.sql`, `*_fleet_obligations.sql`, `*_runner_events.sql` | DELETE | replaced by the thread tables |
| `schema/*_thread_threads.sql`, `*_thread_turns.sql`, `*_thread_runs.sql`, `*_thread_items.sql`, `*_thread_deliveries.sql`, `*_runner_history.sql` | CREATE | one file per table, each with its grants |
| `schema/*_authoring_*.sql`, `*_billing_authoring_report_receipts.sql` | EDIT or DELETE | schema `authoring` becomes `setup`; M253_002 deletes the tables it retires |
| `schema/*_schemas.sql`, `*_roles_and_privileges.sql` | EDIT | the `thread` and `setup` schemas and their grants |
| `rustd/crates/daemon/afd_db/src/migration.rs` | EDIT | the new migration list |
| `rustd/crates/daemon/afd_authoring/**` | DELETE | moved to `afd_setup` |
| `rustd/crates/daemon/afd_setup/**` | CREATE | the draft's setup thread: messages, candidates, setup runs |
| `rustd/crates/daemon/afd_admission/**`, `afd_events/**` | DELETE | merged into `afd_turns` |
| `rustd/crates/daemon/afd_turns/**` | CREATE | turns, items and their reads and streams |
| `rustd/crates/daemon/afd_fleet/**`, `afd_runner/**`, `afd_outbound/**`, `afd_gateway/**`, `afd_approval/**`, `afd_billing/**`, `afd_sse/**`, `afd_bench/**` | EDIT | every reader and writer, in the new words |
| `rustd/crates/daemon/afd_gateway/src/session/**` | DELETE | moved to `afd_gateway/src/connection/**` |
| `rustd/crates/daemon/afd_bus/src/**` | EDIT | the five subjects |
| `rustd/crates/daemon/afd_http/src/route/**`, `route_inventory.rs`, `route_meta_total.rs` | EDIT | turn, item, run and history routes; the turn `PATCH` |
| `rustd/crates/daemon/afd_api_*/src/**` | EDIT | handlers in the new words, and Stop |
| `rustd/crates/protocol/af_wire/src/**` | EDIT | `RunPayload`, `Down::Run`, `turn_id`, `run_id`, `item_id`, item kinds, `Verb::Load` |
| `rustd/crates/runner/**` | EDIT | the runner in the new words; Stop handling |
| `rustd/Cargo.toml`, `rustd/Cargo.lock` | EDIT | crate members renamed |
| `rustd/crates/**/tests/**` | EDIT | fixtures and assertions in the new words |
| `docs/architecture/thread_model.md` | CREATE | the table design, naming convention and mappings this spec builds |
| `public/openapi.json` | EDIT | regenerated with `--no-banner` |
| `ui/packages/app/**` | EDIT | routes, types and copy; the composer's Stop button |
| `cli/src/**` | EDIT | routes, fields, and `agentsfleet fleet turns` |
| `docs/ux.html` | EDIT | the Stop button in the conversation composer |
| `bench/baselines/thread-recovery.rig.json`, `thread-sql.rig.json` | CREATE | §9's measurements |
| `docs/architecture/*.md`, `docs/AUTH.md` | EDIT | every page in the new words |
| `docs/v2/active/M253_001_P1_API_CLI_DOCS_OBS_UI_ONE_THREAD_MODEL_FOR_EVERY_CONVERSATION.md` | CREATE | this spec, moved to `done/` at close |
| `CHANGELOG.md` | EDIT | the breaking vocabulary change |
| `~/Projects/docs` branch `chore/m253-one-thread-model-changelog` | EDIT | API and CLI pages and changelog; API pages are committed after merge |

**Token blast radius** (`git grep -l -w`, Nov 19, 2026, on the branch head over `59b7d03c7`):

| Retired token | Files | Test files | Production files |
|---------------|------:|-----------:|-----------------:|
| `event_id` | 851 | 410 | 441 |
| `admission` | 634 | — | — |
| `lease_id` | 467 | 185 | 282 |
| `afd_authoring` | 274 | — | — |
| `runner.leases` | 200 | 90 | 110 |
| `fleet.admissions` | 177 | 92 | 85 |
| `fleet.events` | 164 | 68 | 96 |
| `obligation` | 125 | — | — |
| `runner.slots` | 110 | 51 | 59 |
| `tool_calls` | 95 | 41 | 54 |
| `LeasePayload` | 88 | — | — |
| `authoring.messages` | 71 | 32 | 39 |
| `fleet.obligations` | 52 | 16 | 36 |
| `FleetTail` | 46 | 21 | 25 |
| `runner.events` | 41 | 14 | 27 |
| `Hydrate`, `TaggedLease` | 25 each | — | — |
| `fleet.sessions` | 16 | 5 | 11 |
| `DraftEvents`, `SessionHandle`, `Down::Offer`, `OutboundWake` | 13, 13, 12, 10 | — | — |

The implementer regenerates this table at PLAN with the test split for every row, and lists each hit's file in the Pull Request. A hit that shares a spelling but not the meaning, such as a vendor's `event_id`, is listed apart and keeps its name.

## Applicable Rules

- **`domains/sql.md`** — one file per table with its whole `CREATE`; no static strings in schema; minted identifiers are `uuid`
- **`domains/http.md`** — HTTP-NOUN, HTTP-NEST, HTTP-VERB, HTTP-PARAM, HTTP-MSGKEY, HTTP-PATCH, HTTP-ROUTE
- **`domains/http-surface.md`** — OpenAPI regenerated; clients change in the same PR
- **`domains/architecture.md`** — crate layer folders; `runner/` and `daemon/` never depend on each other
- **`languages/rust.md`** — ERR-RS and RS-MAPERR on every changed fallible signature
- **`languages/typescript.md`** — app and CLI types follow the wire
- **`tests.md`** — a negative test per failure mode; the e2e scenario below
- **Legacy family** — NLG: no aliases, no compatibility branch, per Indy's clean-slate words

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| `make lint-all` | yes | the rename lands whole; the retired-vocabulary rule runs here |
| route inventory and meta totals | yes | `INVENTORY` and the counts change with the routes |
| `docs.language` | yes | every rewritten page in short sentences |
| codecov patch (95%) and rust line floor (98%) | yes | renamed code keeps its tests; Stop and items get new ones |
| File & Function Length (file ≤350 lines, function ≤50, method ≤70) | yes | the report statement stays split by table |

## Prior-Art / Reference Implementations

- **Reference:** `~/Projects/oss/rs/codex/codex-rs/app-server-protocol` — thread, turn and item are the only conversation nouns, and `turn/interrupt` stops a running turn. Mirror the nouns. Diverge on Stop: HTTP-VERB makes it a `PATCH` of the turn's status.
- **Reference:** `rustd/crates/daemon/afd_bus/src/command.rs:7-12` — the durable mark commits first, then the hint publishes.

## Sections (implementation slices)

### §1 — One glossary, applied everywhere

Every retired word gets one replacement, in code, schema, wire, routes, CLI, screens, metrics and docs:

| Retired | Replacement |
|---------|-------------|
| `afd_authoring`, schema `authoring` | `afd_setup`, schema `setup` |
| `afd_admission`, `afd_events` | `afd_turns` |
| `LeasePayload`, `TaggedLease` | `RunPayload`, which names its thread |
| `Down::Offer` | `Down::Run` |
| `Down::Assignment` (heartbeat policy) | `Down::Policy`, so "assign" means only giving a run to a runner |
| `Verb::Hydrate`, `Verb::Records`, `Up::Activity` | `Verb::Load`, `Verb::Items`, `Up::Progress` |
| gateway `session`, `SessionHandle` | `connection`, `ConnectionHandle` |
| `FleetTail`, `DraftEvents`, `Wake(shard)`, `OutboundWake` | `ThreadItems`, `ThreadWake`, `DeliveryWake` |
| `RUN_LEASE_CANCELLED` | `RUN_CANCELLED`, code `UZ-RUN-022` unchanged |
| af_wire checkpoint `TurnId` | `CheckpointOperationId` |
| guffbolt per-call `Turn`, `turn_started`, `TurnEnd`, `TurnOutcome` | `Step`, `step_started`, `StepEnd`, `StepOutcome` |
| `agentsfleet_admissions_total`, `agentsfleet_admission_backlog*`, `admission_wake_*` | `agentsfleet_turns_total`, `agentsfleet_turn_backlog*`, `turn_wake_*` |
| `bench-lease`, `bench-tail` | `bench-run`, `bench-items` |

Words that share a spelling but not the meaning keep it: login and device sessions, token claims, the vendor webhook `/v1/connectors/{provider}/events`, and "offered" tools. After each Workstream, a glossary review reads every file the Workstream touched, per Indy.

- **Dimension 1.1** — DONE — no retired identifier remains → Test `retired_identifiers_are_gone` (lint): `git grep -n -w -E "LeasePayload|TaggedLease|SessionHandle|FleetTail|DraftEvents|OutboundWake|afd_authoring|afd_admission|afd_events|RUN_LEASE_CANCELLED" -- . ':!docs/v2' ':!docs/reviews' ':!bench/baselines' ':!CHANGELOG.md' ':!docs/architecture/thread_model.md' ':!playbooks/operations/acceptance/baselines'`. Expected: no output. A test never reads the repository (`tests-*-repo-read`), so the check is the `vocab-retired-identifiers-*` rules under `.ast-grep/rules/vocabulary/`, one per grammar, which `make lint-all` runs.
- **Dimension 1.2** — DONE — guffbolt counts steps → Test `a_run_reports_each_model_call_as_a_step` (unit): a scripted model making three calls. Expected: `step_started` 1, 2 and 3.

### §2 — A thread holds a conversation's fence and resume bookmark

`thread.threads` has one row per fleet and one per draft, keyed by that id, as `thread_model.md` defines. It holds the fence, the current runner, the warm-sandbox hold and the resume context. Assigning a run locks the thread row and raises its fence.

- **Dimension 2.1** — DONE — a fleet and a draft each get one thread → Test `creating_a_fleet_or_draft_creates_its_thread` (integration): create a draft, then publish it. Expected: two threads, kinds `draft` and `fleet`, each at fence 0.
- **Dimension 2.2** — DONE — one assignment per fence → Test `two_assignments_on_one_thread_raise_its_fence_once_each` (integration): two runners ask for one queued turn at once. Expected: one run at fence 1, and the other runner gets nothing.

### §3 — A turn is one input and its answer

`thread.turns` has one row per input on a thread: a message, a schedule wake or a continuation. It merges the three input tables column by column, per `thread_model.md`, including retry dedup, reply routing and task links.

Status is `queued`, `running`, `waiting`, `answered`, `failed`, `cancelled` or `recorded`, held as named constants. A turn names the run serving it in `run_id`, which several turns share when one run serves them.

- **Dimension 3.1** — DONE — a send writes a queued turn → Test `a_fleet_message_writes_a_queued_turn` (integration): `POST /fleets/{fleet_id}/messages`. Expected: one turn, `queued`, and one wake published after commit.
- **Dimension 3.2** — DONE — one run can answer two turns → Test `a_run_serving_two_turns_answers_both` (integration): a second message arrives mid-run. Expected: the run's report answers both turns.

### §4 — A run is one try at a turn, and the runner keeps a history

`thread.runs` has one row per try at a turn, with its thread, turn, fence, runner, expiry and status (`active`, `reported`, `expired`, `stopped`). The owner is the thread's kind. `runner.history` records each runner's registration, connection and run events.

- **Dimension 4.1** — DONE — an expired run is followed by the next fence → Test `an_expired_run_is_followed_by_a_run_at_the_next_fence` (integration): a runner stops renewing. Expected: a second run on the turn at fence +1, and the first marked expired.
- **Dimension 4.2** — DONE — operators read runs and history → Test `the_operator_reads_a_runners_runs_and_history` (integration): `GET /admin/runners/{runner_id}/runs` and `/history`. Expected: both 200, with the run and its history rows.

### §5 — Items record what a run did

`thread.items` has one row per thing a run did. Its `kind` is `tool_call`, `edit`, `integration`, `approval` or `agent`.

It names its tool and status, and its changes as path, added and removed counts, and diff. It also holds the integration used and written, and the sub-agent started and finished.

An item is written when its call finishes and is unique on `(run_id, sequence)`, so a retried post writes once.

Text deltas fan out and are never items. A reconnect replays the turn's state and its stored items. Two rules decide ownership:
- **One run, two turns.** An item names the turn that was newest when its step began. guffbolt adds arrived input at the top of each step (`rustd/guffbolt/src/drive/mod.rs:104`).
- **Expiry.** When a resumed run settles a turn, only that run's items stay, per Indy's "Drop them, as today".

- **Dimension 5.1** — an edit records its counts → Test `an_edit_item_records_path_and_line_counts` (integration): a run editing `src/a.rs` by +24 −8. Expected: one `edit` item with those counts.
- **Dimension 5.2** — items name the newest turn → Test `items_after_a_second_message_name_the_second_turn` (unit): input arrives before step 3. Expected: steps 1 and 2 name turn 1, and step 3 names turn 2.
- **Dimension 5.3** — DONE — a dead run's items go → Test `a_settling_resumed_run_drops_the_dead_runs_items` (integration): run 1 records 2 items and dies, then run 2 records 1 and settles. Expected: one item remains.
- **Dimension 5.4** — DONE — deltas are never stored → Test `text_deltas_fan_out_and_are_not_stored` (integration): a streamed answer. Expected: the stream delivers the deltas, and no text row exists in `thread.items`.

### §6 — Deliveries and the bus speak the same words

`thread.deliveries` holds what a settled turn owes someone, with its lane, target and state. Only the daemon uses the bus; the runner reaches the daemon on its connection.

The subjects are `af.thread.{thread_id}.wake`, `af.thread.{thread_id}.items`, `af.runner.{runner_id}.cmd`, `af.delivery.wake` and `af.bus.gap`. Each daemon replica subscribes once to `af.thread.*.wake`, through a wildcard subscribe that `Bus` gains, and the 16 wake shards are deleted.

`af.bus.gap` becomes a `Subject` variant. A lost message is recovered by the backstop re-read.

- **Dimension 6.1** — DONE — every subject has its spelling → Test `each_subject_spells_its_thread_noun` (unit): every `Subject` variant. Expected: exactly the five spellings above, and the wildcard `af.thread.*.wake`.
- **Dimension 6.2** — DONE — a lost wake still assigns → Test `a_turn_whose_wake_was_lost_is_assigned_by_the_backstop` (integration): drop the wake publish. Expected: the turn is assigned within one backstop interval.

### §7 — The public surface speaks the same words

Turn routes sit under the fleet or draft that owns them, and admin routes list each runner's runs and history. The runner plane moves from `/v1/runners/me/leases/{lease_id}/…` to `/v1/runners/me/runs/{run_id}/…`, and its `tool-calls` path to `items`.

Every field is `turn_id`, `run_id`, `item_id` or `delivery_id`. The charge API's setup charges carry `run_id` and `charge_type: setup`.

The CLI's `fleet events` becomes `fleet turns` with the same flags, `fleet logs` tails the turn stream, and `chat`'s Stop uses the turn `PATCH`. The UI, CLI, OpenAPI document and TypeScript types change in this PR. Nothing answers an old route or accepts an old field.

- **Dimension 7.1** — DONE — the inventory holds only new routes → Test `the_route_inventory_names_turns_items_and_runs` (unit): `INVENTORY`. Expected: no path contains `/tool-calls`, `/leases` or `/cancellations`, and `/events` appears only in `/v1/connectors/{provider}/events`.
- **Dimension 7.2** — DONE — an unknown field is refused → Test `a_body_naming_an_unknown_field_is_refused` (integration): a request carrying `event_id`. Expected: 400 with `UZ-REQ-001`, as today's handler refuses an unknown field.
- **Dimension 7.3** — DEFERRED to the end-of-M253 acceptance run — the CLI reads turns → Test `fleet_turns_lists_turns` (acceptance, `cli/test/acceptance/logs-turns-live.spec.ts`): `agentsfleet fleet turns <fleet_id>` against a live daemon. Expected: JSON rows with `turn_id`.

### §8 — One Stop ends any thread's running turn

`PATCH /fleets/{fleet_id}/turns/{turn_id}` and `PATCH /drafts/{draft_id}/turns/{turn_id}` share one handler, with body `{"status":"cancelled"}`. The transaction marks the turn `cancelled` and sets `cancel_requested_at` on its live run. After commit, it publishes `Command::Cancel { run_id }`.

The gateway holding that runner's connection re-reads the mark and sends `Down::Cancel` (`afd_gateway/src/session/commands.rs:107-126`). The supervisor trips the run's interrupt token (`afr_supervisor/src/downlink/dispatch.rs:57-64`), and guffbolt ends as `Stopped`. A lost hint is caught by the next renewal, which reads the same mark.

Stop cancels the run and every turn it serves; a queued turn it has not taken stays queued. A stopped run still reports with outcome `Stopped`, a new `af_wire::report::Outcome` variant: usage is billed and its items stay, but its turns get no answer.

A queued turn becomes `cancelled` with no run. Stop on a `waiting` turn cancels it and closes its open approval.

Stop on a `failed` turn returns 409, as on an answered one. The composer shows Stop while a turn runs.

- **Dimension 8.1** — DONE — Stop ends a running fleet turn → Test `stopping_a_running_fleet_turn_ends_its_run` (integration): a run that hangs, then the `PATCH`. Expected: the run reports stopped within one renewal interval, and the turn stays `cancelled`.
- **Dimension 8.2** — DONE — a queued turn needs no runner → Test `stopping_a_queued_turn_cancels_it_without_a_run` (integration): a queued turn. Expected: `cancelled`, and no run is ever assigned.
- **Dimension 8.3** — DONE — Stop survives a lost hint → Test `a_stop_whose_hint_was_lost_ends_at_the_next_renewal` (integration): drop the `Command::Cancel` publish. Expected: the renewal answer cancels the run.
- **Dimension 8.4** — DONE — Stop after the answer conflicts → Test `stopping_an_answered_turn_is_a_conflict` (integration): an answered turn. Expected: 409, and the answer is unchanged.
- **Dimension 8.5** — DONE — Stop is repeatable → Test `stopping_a_cancelled_turn_again_answers_the_same_state` (integration): two `PATCH` calls. Expected: both 200 with `cancelled`.
- **Dimension 8.6** — DONE — the screen shows Stop only while a turn runs → Test `composer_shows_stop_while_a_turn_runs` (e2e): a Playwright run of the conversation view. Expected: Stop appears while the turn is running and is gone after the answer. The spec mocks the turn routes with `page.route`.
- **Dimension 8.8** — DONE — a draft's running turn stops → Test `stopping_a_running_draft_turn_ends_its_setup_run` (integration): a setup run that hangs, then `PATCH …/drafts/{draft_id}/turns/{turn_id}`. Expected: the run reports stopped and the turn is `cancelled`. The test drives the draft door at the run service (`Drafts::stop_turn`, then `report_draft`), because no fixture yet runs a draft-owned run end to end.
- **Dimension 8.9** — DONE — Stop ends every turn a run serves → Test `stopping_one_of_two_served_turns_cancels_both` (integration): a run serving two turns. Expected: both `cancelled`, and one run report.
- **Dimension 8.7** — DONE — another workspace cannot stop it → Test `a_member_of_another_workspace_cannot_stop_a_turn` (integration): a token for workspace B. Expected: 404 through workspace B's path, and the turn keeps running. Through workspace A's path, the shared ownership layer answers 403 first, as on every workspace route.

### §9 — The measurements run on the thread tables

Indy moved these here from #853: "PR853 - descope with feedback that the next large refactor will do this".
- **Benchmarks.** Rig `dist` recovery benchmarks run API-only and file-backed, at 1, 10, 100 and 500 fleets. Assignment concurrency runs at 2, 4 and 8 runners.
- **Query plans.** `EXPLAIN ANALYZE` runs on populated tables for every assign, renew, report and Stop statement.
- **Docs.** Every architecture page speaks the new words, which §1's rule checks.

- **Dimension 9.1** — recovery is measured → Test `bench_thread_recovery_api_and_files` (manual): Indy reviews `bench/baselines/thread-recovery.rig.json`. Expected: p50 and last-fleet times for each size, with source hashes.
- **Dimension 9.2** — each changed statement has a plan → Test `explain_plans_for_thread_statements` (manual): Indy reviews `bench/baselines/thread-sql.rig.json`. Expected: one plan per statement, none a sequential scan on `thread.turns` or `thread.runs`.

## Interfaces

| Surface | Shape |
|---------|-------|
| Fleet turns | `GET …/fleets/{fleet_id}/turns`, `…/turns/stream`, `…/turns/{turn_id}`, `…/turns/{turn_id}/items/{item_id}` |
| Draft turns | `GET …/drafts/{draft_id}/turns`, `…/turns/stream`, `…/turns/{turn_id}` |
| Workspace turns | `GET /v1/workspaces/{workspace_id}/turns`, `…/turns/stream` |
| Stop | `PATCH …/fleets/{fleet_id}/turns/{turn_id}` and `PATCH …/drafts/{draft_id}/turns/{turn_id}`, body `{"status":"cancelled"}` |
| Sends | `POST …/fleets/{fleet_id}/messages`, `POST …/drafts/{draft_id}/messages` (HTTP-MSGKEY) |
| Operators | `GET /v1/admin/runners/{runner_id}/runs`, `…/history` |
| CLI | `agentsfleet fleet turns <fleet_id>` |
| Runner wire | `Down::Run { run: RunPayload }`, `Down::Cancel { run_id, error_code }`, `Verb::Load`; `RunPayload` names one owner, a fleet or a draft |

An item on the public wire looks like this:

```json
{"item_id":"019f…","turn_id":"019f…","run_id":"019f…","kind":"edit","tool":"file_edit","status":"completed",
 "changes":{"path":"src/a.rs","added":24,"removed":8}}
```

## Failure Modes

| Mode | Kind | Cause | Handling (response and what the caller observes) | Test |
|------|------|-------|--------------------------------------------------|------|
| Two runners at once | race | two runners ask for one turn | one run; the other gets nothing | `two_assignments_on_one_thread_raise_its_fence_once_each` |
| Lost wake | dependency unavailable | NATS drops the wake | the backstop assigns it | `a_turn_whose_wake_was_lost_is_assigned_by_the_backstop` |
| Lost Stop hint | dependency unavailable | NATS drops `Command::Cancel` | the next renewal cancels | `a_stop_whose_hint_was_lost_ends_at_the_next_renewal` |
| Stop after answer | race | the answer commits first | 409; the answer stands | `stopping_an_answered_turn_is_a_conflict` |
| Repeated Stop | replay | the client retries | 200, same state | `stopping_a_cancelled_turn_again_answers_the_same_state` |
| Cross-workspace Stop | security | a token for another workspace | 404 | `a_member_of_another_workspace_cannot_stop_a_turn` |
| Unknown field | malformed input | a client sends a field the wire lacks | 400 `UZ-REQ-001` | `a_body_naming_an_unknown_field_is_refused` |
| Dead run's items | failure | a run dies after recording items | only the settling run's items remain | `a_settling_resumed_run_drops_the_dead_runs_items` |
| Retired word returns | failure | a later change reintroduces one | Dimension 1.1's search fails | `retired_identifiers_are_gone` |

## Invariants

1. A thread has one live run per fence. Assignment updates the thread `WHERE fence = $n` once, and a unique index on `thread.runs (thread_id, fence)` refuses a second.
2. A cancelled turn is never answered. The report settles a turn only while its status equals the bound running constant.
3. Only the settling run's items remain. The report deletes a turn's items whose `run_id` differs from the settling run.
4. No retired identifier exists outside the exempt paths. Dimension 1.1's search runs in `make lint-all`.

## Metrics & Observability

| Metric / event | Audience | Fires when | Properties allowed | Privacy guard | Test |
|----------------|----------|------------|--------------------|---------------|------|
| `turn_stopped` | product | a person stops a turn | `thread_kind`, `turn_id`, was-running | no message text | `stopping_a_running_fleet_turn_ends_its_run` |
| `run_cancel_sent` | operator | the gateway sends `Down::Cancel` | `run_id`, `runner_id`, `error_code` | no key | `a_stop_whose_hint_was_lost_ends_at_the_next_renewal` |
| `items_dropped_for_dead_run` | operator | a settling run removes another run's items | `turn_id`, count | none needed | `a_settling_resumed_run_drops_the_dead_runs_items` |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Stop ends a running fleet turn (§8) | `make test-integration-rustd` | `stopping_a_running_fleet_turn_ends_its_run` passes | P0 | |
| R2 | The conversation shows Stop while a turn runs (§8) | `make dry-app` | `composer_shows_stop_while_a_turn_runs` passes | P0 | |
| R3 | No retired identifier remains (§1) | `git grep -n -w -E "LeasePayload|TaggedLease|SessionHandle|FleetTail|DraftEvents|OutboundWake|afd_authoring|afd_admission|afd_events" -- . ':!docs/v2' ':!docs/reviews' ':!bench/baselines' ':!CHANGELOG.md' ':!docs/architecture/thread_model.md' ':!playbooks/operations/acceptance/baselines'` | no output | P0 | |
| R4 | No retired table remains (§2–§6) | `git grep -n -w -E "fleet\.events|fleet\.admissions|runner\.leases|runner\.slots|fleet\.tool_calls|fleet\.obligations|fleet\.sessions|runner\.events" -- . ':!docs/v2' ':!docs/reviews' ':!bench/baselines' ':!CHANGELOG.md' ':!docs/architecture/thread_model.md'` | no output | P0 | |
| R5 | Items keep the right owner (§5) | `make test-integration-rustd` | both §5 integration cases pass | P0 | |
| R6 | Measurements recorded (§9) | `ls bench/baselines/thread-recovery.rig.json bench/baselines/thread-sql.rig.json` | exit 0 | P1 | |
| S1 | Declared commands green | `make lint-all` | exit 0 | P0 | |
| S2 | Declared commands green | `make test-unit-all` | exit 0 | P0 | |
| S3 | Declared commands green | `make test-integration-rustd` | exit 0 | P0 | |
| S4 | Declared commands green | `make check-version` | exit 0 | P0 | |
| S5 | No secrets | `gitleaks detect --redact --no-banner` | exit 0 | P0 | |

## Dead Code Sweep

| Deleted file or symbol | Verify | Expected |
|------------------------|--------|----------|
| `LeasePayload`, `TaggedLease` | `git grep -n -w -E "LeasePayload|TaggedLease" -- . ':!docs/v2' ':!docs/reviews' ':!bench/baselines' ':!CHANGELOG.md' ':!docs/architecture/thread_model.md'` | no output |
| `Down::Offer` | `git grep -n "Down::Offer" -- . ':!docs/v2' ':!docs/reviews' ':!bench/baselines' ':!CHANGELOG.md' ':!docs/architecture/thread_model.md'` | no output |
| `Verb::Hydrate` | `git grep -n -w "Hydrate" -- . ':!docs/v2' ':!docs/reviews' ':!bench/baselines' ':!CHANGELOG.md' ':!docs/architecture/thread_model.md'` | no output |
| `afd_authoring`, `afd_admission`, `afd_events` | `git grep -n -w -E "afd_authoring|afd_admission|afd_events" -- . ':!docs/v2' ':!docs/reviews' ':!bench/baselines' ':!CHANGELOG.md' ':!docs/architecture/thread_model.md'` | no output |
| `SessionHandle` | `git grep -n -w "SessionHandle" -- . ':!docs/v2' ':!docs/reviews' ':!bench/baselines' ':!CHANGELOG.md' ':!docs/architecture/thread_model.md'` | no output |
| `FleetTail`, `DraftEvents`, `OutboundWake`, `DraftCancellations` | `git grep -n -w -E "FleetTail|DraftEvents|OutboundWake|DraftCancellations" -- . ':!docs/v2' ':!docs/reviews' ':!bench/baselines' ':!CHANGELOG.md' ':!docs/architecture/thread_model.md'` | no output |

## Out of Scope

- guffbolt and setup on runs: M253_002.
- One human-input primitive, effect keys, no task reservation, and email delivery: M253_003.
- A CLI Stop command: not planned; the screen and API carry Stop.

---

## Product Clarity (authoring record)

1. **Successful user moment** — A person presses Stop on a fleet that is "Thinking", and the row turns to "Stopped" within seconds.
2. **Preserved user behaviour** — Sending messages, reading the conversation and the activity view, and following live work all keep working.
3. **Optimal-way check** — One `PATCH` on the turn reuses the cancel path; nothing shorter stops a runner safely.
4. **Rebuild-vs-iterate** — Rebuild the words in one PR, because a partial rename leaves two vocabularies.
5. **What we build** — five thread tables, runner history, five subjects, two renamed crates, the new wire types, routes and fields, Stop, the source rule, and the measurements.
6. **What we do NOT build** — aliases or compatibility branches, because Indy chose a clean slate; a CLI Stop.
7. **Fit with existing features** — compounds with M247 respawn; it must not destabilise fencing, which every report relies on.
8. **Surface order** — schema and wire first, then API, then UI and CLI, all in this PR.
9. **Dashboard restraint** — Stop appears only while a turn runs.
10. **Confused-user next step** — a 409 on Stop says the turn already finished, and its answer is in the conversation.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** words and the rule first (§1), tables (§2–§6), the public surface (§7), Stop (§8), measurements (§9).
- **Alternatives considered:**
  - **Rename only the public API.** That keeps the internal vocabularies Indy wants gone, so it was rejected.
  - **A `/threads/{thread_id}` route family.** HTTP-NEST nests turns under the fleet or draft that owns them, so it was rejected.
- **Patch-vs-refactor verdict:** a **refactor** because tables, crates, wire, routes and subjects all change.

## Discovery (consult log)

- **Consults** — Indy's decisions, Nov 19, 2026, quoted:
  - > Owner (Indy, Nov 19, 2026): "Merge tables + rename (Recommended)" — context: §2–§6.
  - > Owner (Indy, Nov 19, 2026): "I want all the renames to happen here. We dont use any of the old stupid terminology you had. so do its wa clean slate approach, no crap thinking about legacy compatibility" — context: §1 and the whole spec.
  - > Owner (Indy, Nov 19, 2026): "Adopt this glossary" — context: the words in §1 and Interfaces.
  - > Owner (Indy, Nov 19, 2026): "Final answer + items (Recommended)" — context: §5, deltas fan out only.
  - > Owner (Indy, Nov 19, 2026): "Drop them, as today" — context: §5, a dead run's items.
  - > Owner (Indy, Nov 19, 2026): "Rename /v1 now" — context: §7.
  - > Owner (Indy, Nov 19, 2026): "Rename to run_id, setup (Recommended)" — context: §7, the charge API.
  - > Owner (Indy, Nov 19, 2026): "One Stop, every thread (Recommended)" — context: §8.
  - > Owner (Indy, Nov 19, 2026): "PR853 - descope with feedback that the next large refactor will do this" — context: §9.
  - > Owner (Indy, Nov 19, 2026): "i need 1 worktree with the guffbolt changes and everything (3 workstreams) in it." — context: one branch and one PR for M253.
  - > Owner (Indy, Nov 19, 2026): "donot do the lint, test-unit, test-integration check but rather do it at the end of every workstream. but for every dimension do cargo check, cargo fmt (clippy) so we know code compiles." — context: per-Dimension compile checks; S rows at the end of each Workstream.
  - > Owner (Indy, Nov 19, 2026): "Ensure the rust code for individual files is 99% and overall is 99% if possible or else stick to the numbers we have currently." — context: coverage targets 99% per file and overall, never below the 98% line and 95% patch floors.
- **Consults, agent defaults** — Stop is a `PATCH` of turn status, per HTTP-VERB. Sends keep `/messages`, per HTTP-MSGKEY. New tables live in a `thread` Postgres schema, as the glossary shows.
- **Model reviews** — EGM Stage 9 (implementation readiness), three reviewers, Claude Opus 5.5, Nov 19, 2026: 8 Blockers, 47 Majors, 29 Nits. Every Blocker is resolved in the Sections as an implementation default, recorded in `.egm/implementation_readiness_report.md`; wording findings are applied, and none is declined.
- **Open blockers** — none.
- **Deferrals and transfers** —
  - Dimension 7.3 runs only in acceptance, which needs an https target and costs money. It is proven by the acceptance run at the end of M253, started beside the §9 benchmarks.
    > Owner (Indy, Nov 20, 2026): "7.3 - prove it in the post-merge acceptance run. Yes so it could be done when M252_003 is run where the performance bench and the acceptance (either in burst e2e or gh are kicked off parallel)"
