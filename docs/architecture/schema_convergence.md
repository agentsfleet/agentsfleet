# Schema convergence

Setup, chat and durable tasks each write their own tables. This page asks, table by table, whether each should fold into the thread model: threads, turns, runs and items ([`thread_model.md`](./thread_model.md)).

Indy rules on every row. His ruling replaces the verdict cell, and his words go in the Discovery of `M255_001`. The folds he signs become `M255_002`; nothing on this page changes a table by itself.

Facts are read from the schema files under `rustd/crates/daemon/afd_db/schema/` at `2da483d8b`. "Writers" counts production statement files that insert, update or delete the table, by crate. A saving marked unmeasured is settled by the statement counter (`afd_db::test_util::statements`) before its fold is built.

## The picture

```
 today                                         the reviews' end state
 setup.drafts ─────────── owner row            setup.drafts             (stays)
 thread.threads ────────┐                      thread.threads           (+ compaction document)
 thread.compactions ────┘ one row per thread
 thread.runs  fleet_id | draft_id              thread.runs              (one thread_id)
 thread.turns                                  thread.turns             (stays)
 thread.items ──────────┐                      thread.items             (+ task items, inputs,
 fleet.durable_task_items ─┤                                               effects, decision payloads)
 fleet.durable_task_inputs ┤
 fleet.durable_task_effects┤
 runner.decision_payloads ─┘
 thread.replay_receipts ◄── item and input receipts
 thread.deliveries                             thread.deliveries        (stays)
 fleet.durable_tasks                           fleet.durable_tasks      (stays)
 runner.decision_calls                         runner.decision_calls    (stays: a ledger)
```

Fourteen tables become nine, and the six durable-task tables become two. Zero is rejected: one long-lived turn per task would block the queue head ([`durable_tasks.md`](./durable_tasks.md), "A wait is separate from its settled turn").

## One row per table

| # | Table | Flows and writers | Would become | Needs there | What the fold costs | What it saves | Reviews' position | Verdict |
|---|---|---|---|---|---|---|---|---|
| 1 | `setup.drafts` | setup: `afd_setup` 8 | columns on its `thread.threads` row | the draft columns nullable on threads, set exactly when `draft_id` is | `PATCH` compares `resource_version`; on the thread row, every renewal and assignment rewrites it, so a `PATCH` would wait on runs and runs on `PATCH`. The draft's foreign keys (user cascade, revisions, tenant model) move onto the table every fleet's thread lives in | one unlocked read per setup turn at most: the assignment reads the draft as a hint (`thread_model.md`, `thread.threads`) | stays: the draft is the owner row, the peer of `fleet.fleets` | UNDECIDED |
| 2 | `thread.threads` | all three: `afd_fleet` 15, `afd_setup` 2, `afd_runner` 1 | itself | nothing | none | none | stays, and takes the compaction document (row 3) | UNDECIDED |
| 3 | `thread.compactions` | setup and chat: written by `afd_setup/src/context/fenced_{insert,update}.sql`, read for both kinds by `afd_fleet/src/run/context.rs` | a document and its version on `thread.threads` | nothing: one row per thread is already `idx_compactions_thread_id` | the save, fenced by its run, becomes an update of the hottest row, so it queues with renewals and assignments for one statement. The thread then holds two resume stores, `resume_context` (the fleet report writes it) and the compaction document; making them one is a separate ruling | one insert per thread's first compaction; the context load's run check and document read could be one statement (unmeasured) | fold | UNDECIDED |
| 4 | `thread.runs`: `fleet_id` and `draft_id` | all three: `afd_fleet` 13, `afd_setup` 2, `afd_turns` 1, `afd_fleet_lifecycle` 1 | one `thread_id` | one index `(thread_id, status, fencing_token DESC)` replacing two; `ck_runs_one_owner` goes | a run outlives its owner with no foreign key, so once the thread is gone a settled run no longer says whether it served a fleet or a draft; keeping that needs a kind column. Readers that filter `fleet_id` alone filter `thread_id` and the kind | 14 statement files lose `COALESCE(fleet_id, draft_id)`; one index fewer on every run write; no statement per turn. `thread_model.md` already describes runs with `thread_id`, and the schema diverged from it | fold | UNDECIDED |
| 5 | `thread.turns` | all three: `afd_fleet` 20, `afd_setup` 7, `afd_turns` 8, `afd_approval` 1 | itself | nothing | none: it already carries the setup columns (`operation_id`, `proposed_revision_id`, `save_requested`) and the task columns (`plan`, `durable_task_*`) | none | stays | UNDECIDED |
| 6 | `thread.items` | chat: `afd_fleet` 3 | itself, with `fleet_id` read as the thread, so a setup run can record items too, and with the rows of 10, 11, 12 and 14 | a kind for each folded row, and the uniques rows 10, 12 and 14 name | the report deletes every other run's items when a turn settles (`commit_report.sql:189-194`); it must spare task rows | interpretation receipts, effects and decisions show in the one item detail | stays, and takes four tables | UNDECIDED |
| 7 | `thread.deliveries` | chat: `afd_outbound` 7, `afd_fleet` 2 | itself | nothing | none | none | stays | UNDECIDED |
| 8 | `thread.replay_receipts` | setup and durable tasks: `afd_setup` 3, `afd_fleet` 3 | itself, also holding the item reply and input apply receipts | nothing: the key is `(scope, operation_id)` | the never-read writer `run_id` and `fencing_token` of each receipt are lost | two partial unique indexes go, and the receipt replay's four-arm `UNION` (`durable_task/input_receipt/sql/replay.sql`) becomes one primary-key probe | takes the task receipts | UNDECIDED |
| 9 | `fleet.durable_tasks` | durable tasks: `afd_fleet` 19 | itself | nothing | none | none | stays: the goal, limits and generation outlive any one turn | UNDECIDED |
| 10 | `fleet.durable_task_items` | durable tasks: `afd_fleet` 2 | `thread.items` rows of an interpretation kind | a partial unique `(turn_id, ordinal)`; a typed task column; the deferred foreign key from `durable_tasks.originating_durable_task_item_id` re-pointed | the append-only triggers move to items under a kind condition; lookups by task (`decision/sql/overage.sql:8`) join through the turn or read the typed column | one table; per bind, unmeasured | fold | UNDECIDED |
| 11 | `fleet.durable_task_inputs` | durable tasks: `afd_fleet` 2 | columns on the item it accepts, so `thread.items` with row 10 | `UNIQUE (durable_task_id, sequence)` as a partial index on a nullable column; its two write-once triggers merge into one | none beyond row 10's | one insert per steer, and the join four task reads make (`input_receipt/sql/item.sql`, `sql/replay.sql`, `accepted_inputs.sql`, `recovered_interpretations.sql`) | fold | UNDECIDED |
| 12 | `fleet.durable_task_effects` | durable tasks: `afd_fleet` 5 | `thread.items` rows of an effect kind | partial uniques `(durable_task_id, ordinal)` and `(durable_task_id, operation_id)` | an effect survives fleet purge today (no foreign key); as an item it goes with the fleet. No reader of a purged fleet's effects was found (unverified). Recovery reads effects by task across turns, so it joins through the turn | one publication mechanism where there are two | fold, last | UNDECIDED |
| 13 | `runner.decision_calls` | durable tasks: `afd_fleet` 3 | itself | nothing | none | none | stays: a ledger that outlives run, fleet and model with no foreign key, as `billing.usage_lines` does | UNDECIDED |
| 14 | `runner.decision_payloads` | durable tasks: `afd_fleet` 2 | the arguments and output of a `thread.items` row of a decision kind | a unique `decision_call_id` on items; the item names the call's source turn | privacy deletion keeps its shape: the item goes, the call stays. Its cascades (fleet, task) already match an item's | one table; no statement per decision, since the insert moves | fold | UNDECIDED |

## Not in this question

These tables hold no conversation state, so no fold is proposed: `fleet.fleets`, `fleet.revisions`, `fleet.approval_gates`, `fleet.activity_counters`, `billing.usage_lines`, `memory.entries`. The `runner.checkpoint_*` tables are Indy's separate track (`M255_001`, Out of Scope).
