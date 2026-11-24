# Schema convergence

Setup, chat and durable tasks each write their own tables. This page asks, table by table, whether each should fold into the thread model: threads, turns, runs and items ([`thread_model.md`](./thread_model.md)).

Indy rules on every row. His ruling replaces the verdict cell, and his words go in the Discovery of `M255_001`. The folds he signs become `M255_002`; nothing on this page changes a table by itself.

Facts are read from the schema files under `rustd/crates/daemon/afd_db/schema/` at `9dd3d0b5a`, after `M255_001` rewrote the decision path. "Writers" counts production statement files that insert, update or delete the table, by crate. A saving marked unmeasured is settled by the statement counter (`afd_db::test_util::statements`) before its fold is built.

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

| # | Table | Flows and writers | Would become | Needs there | What the fold costs | What it saves | Reviews' position | Fable review, Nov 24, 2026 | Verdict |
|---|---|---|---|---|---|---|---|---|---|
| 1 | `setup.drafts` | setup: `afd_setup` 8 | columns on its `thread.threads` row | the draft columns nullable on threads, set exactly when `draft_id` is | `PATCH` compares `resource_version`; on the thread row, every renewal and assignment rewrites it, so a `PATCH` would wait on runs and runs on `PATCH`. The draft's foreign keys (user cascade, revisions, tenant model) move onto the table every fleet's thread lives in | one unlocked read per setup turn at most: the assignment reads the draft as a hint (`thread_model.md`, `thread.threads`) | stays: the draft is the owner row, the peer of `fleet.fleets` | AGREE: a `PATCH` holds the draft `FOR NO KEY UPDATE` (`afd_setup/src/sql/draft/lock.sql:11`); on the thread row it would stop every renewal | UNDECIDED |
| 2 | `thread.threads` | all three: `afd_fleet` 15, `afd_setup` 2, `afd_runner` 1 | itself | nothing | none | none | stays, and takes the compaction document (row 3) | AGREE | UNDECIDED |
| 3 | `thread.compactions` | setup and chat: written by `afd_setup/src/context/fenced_{insert,update}.sql`, read for both kinds by `afd_fleet/src/run/context.rs` | a document and its version on `thread.threads` | nothing: one row per thread is already `idx_compactions_thread_id` | the save, fenced by its run, becomes an update of the hottest row, so it queues with renewals and assignments for one statement. The thread then holds two resume stores, `resume_context` (the fleet report writes it) and the compaction document; making them one is a separate ruling | one insert per thread's first compaction; the context load's run check and document read could be one statement (unmeasured) | fold | DISAGREE: the save's `FOR SHARE` on the thread (`afd_setup/src/context/fenced_update.sql:13-17`) becomes exclusive on a row 18 statement files write, and every renewal copies the document. The per-turn saving is a `LEFT JOIN` that needs no fold | UNDECIDED |
| 4 | `thread.runs`: `fleet_id` and `draft_id` | all three: `afd_fleet` 13, `afd_setup` 2, `afd_turns` 1, `afd_fleet_lifecycle` 1 | one `thread_id` | one index `(thread_id, status, fencing_token DESC)` replacing two; `ck_runs_one_owner` goes | a run outlives its owner with no foreign key, so once the thread is gone a settled run no longer says whether it served a fleet or a draft; keeping that needs a kind column. Readers that filter `fleet_id` alone filter `thread_id` and the kind | 12 statement files and two Rust modules lose `COALESCE(fleet_id, draft_id)`; a draft run's writes lose one index (the draft index is partial, so a fleet run is in one today); no statement per turn. `thread_model.md` describes runs with a cascading `thread_id` key and a unique `(thread_id, fencing_token)`; this row proposes neither | fold | AGREE WITH CONDITION: a kind column on the run, since 12 statement files tell the kind by which id is set; and no foreign key to the thread, or its cascade deletes settled runs kept as history | UNDECIDED |
| 5 | `thread.turns` | all three: `afd_fleet` 20, `afd_setup` 7, `afd_turns` 8, `afd_approval` 1 | itself | nothing | none: it already carries the setup columns (`operation_id`, `proposed_revision_id`, `save_requested`) and the task columns (`plan`, `durable_task_*`) | none | stays | AGREE | UNDECIDED |
| 6 | `thread.items` | chat: `afd_fleet` 3 | itself, with `fleet_id` read as the thread, so a setup run can record items too, and with the rows of 10, 11, 12 and 14 | a kind for each folded row, and the uniques rows 10, 12 and 14 name | the report deletes every other run's items when a turn settles (`commit_report.sql:189-194`); it must spare task rows | interpretation receipts, effects and decisions show in the one item detail | stays, and takes four tables | DISAGREE: the runner upserts this table on `(run_id, sequence)` and the report deletes from it; daemon write-once rows lose their column grants, and a guard "under a kind condition" needs a literal the SQL standard bars | UNDECIDED |
| 7 | `thread.deliveries` | chat: `afd_outbound` 7, `afd_fleet` 2 | itself | nothing | none | none | stays | AGREE | UNDECIDED |
| 8 | `thread.replay_receipts` | all three: `afd_setup` 3, `afd_fleet` 3, two of them an ordinary turn's checkpoint operations | itself, also holding the item reply and input apply receipts | `mode` and `response_text`, which the table lacks and two reads use; the key is `(scope, operation_id)` | the never-read writer `run_id` and `fencing_token` of each receipt are lost, with the once-per-item guard and the all-or-none check unless they stay on the item | two partial unique indexes go; two of the replay's four arms look up by item and mode, so it does not become one primary-key probe | takes the task receipts | AGREE WITH CONDITION: `mode` and `response_text` stay on the item; the scope is per fleet; a retention rule is named, since this table has no foreign key | UNDECIDED |
| 9 | `fleet.durable_tasks` | durable tasks: `afd_fleet` 19 | itself | nothing | none | none | stays: the goal, limits and generation outlive any one turn | AGREE | UNDECIDED |
| 10 | `fleet.durable_task_items` | durable tasks: `afd_fleet` 2 | `thread.items` rows of an interpretation kind | a partial unique `(turn_id, ordinal)`; a typed task column; both foreign keys re-pointed: the immediate, cascading one from `durable_tasks.originating_durable_task_item_id` and the deferred one from items to tasks; `UNIQUE (id, fleet_id, workspace_id)` on `thread.items` | the append-only triggers move to items under a kind condition; lookups by task (`decision/sql/overage.sql:8`) join through the turn or read the typed column | one table; per bind, unmeasured | fold | DISAGREE: `fk_durable_tasks_item` cascades from the item to the task, and the report deletes other runs' items; one missed predicate deletes a task | UNDECIDED |
| 11 | `fleet.durable_task_inputs` | durable tasks: `afd_fleet` 2 | columns on the item it accepts, so `thread.items` with row 10 | `UNIQUE (durable_task_id, sequence)` as a partial index on a nullable column; its two write-once triggers merge into one | none beyond row 10's | one insert per steer, and the join four task reads make (`input_receipt/sql/item.sql`, `sql/replay.sql`, `accepted_inputs.sql`, `recovered_interpretations.sql`) | fold | AGREE, as columns on `fleet.durable_task_items`: an input's key already is its item's id (`durable_task/accept/sql/input.sql:3`) | UNDECIDED |
| 12 | `fleet.durable_task_effects` | durable tasks: `afd_fleet` 5 | `thread.items` rows of an effect kind | partial uniques `(durable_task_id, ordinal)` and `(durable_task_id, operation_id)` | an effect survives fleet purge today (no foreign key); as an item it goes with the fleet. No reader of a purged fleet's effects was found (unverified). Recovery reads effects by task across turns, so it joins through the turn | one publication mechanism where there are two | fold, last | DISAGREE: an effect is found by `(durable_task_id, operation_id)` across turns, while a turn's other runs' items are deleted at settlement; opposite lifetimes | UNDECIDED |
| 13 | `runner.decision_calls` | durable tasks: `afd_fleet` 2 (`issue.sql`, `record.sql`) | itself | nothing | none | none | stays: a ledger that outlives run, fleet and model with no foreign key, as `billing.usage_lines` does | AGREE | UNDECIDED |
| 14 | `runner.decision_payloads` | durable tasks: `afd_fleet` 2 | the arguments and output of a `thread.items` row of a decision kind | a unique `decision_call_id` on items; the item names the call's source turn | privacy deletion keeps its shape: the item goes, the call stays. An item cascades from fleet and turn and has no task column, so the payload's task cascade is lost | one table; no statement, since the payload insert is already the tail of the issue statement | fold | DISAGREE: saves no statement and loses the task cascade; `result` is nullable JSONB where `output` is `TEXT NOT NULL`. Payload columns on the call would make a receipt one update | UNDECIDED |

## Not in this question

These tables hold no conversation state, so no fold is proposed: `fleet.fleets`, `fleet.revisions`, `fleet.activity_counters`, `billing.usage_lines`, `memory.entries`. `fleet.approval_gates` carries a `turn_id` and is left out as a gate, not a conversation.

The flows also write `runner.history` (one row per run issue and one per settlement), `runner.runners`, the `billing.wallets` rollups and `connector.integration_grants`. None is proposed for a fold. The `runner.checkpoint_*` tables are Indy's separate track (`M255_001`, Out of Scope).

## What the Fable review asks Indy to decide

A fresh Fable 5.2 review read this brief against the schema and statement files on Nov 24, 2026. It agrees with rows 1, 2, 5, 7, 9 and 13, and with rows 4, 8 and 11 under conditions. It disagrees with rows 3, 6, 10, 12 and 14.

1. Six durable-task tables become two, or five. Five folds inputs into items and receipts into `thread.replay_receipts`, and keeps every grant and unique.
2. Whether compactions move onto the thread row. The fold buys one table and one insert per thread's lifetime, against an exclusive lock on the hottest row.
3. What `thread_id` on runs means: a cascading key, as `thread_model.md` says, or runs kept as history with a kind column.
