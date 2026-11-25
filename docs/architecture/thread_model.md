# Thread model

## What it is

A conversation is a **thread**, and each input on it is a **turn**.
Each try at answering a turn is a **run**, and each tool call a run makes is an **item**.
Each answer owed to a chat destination is a **delivery**.

These nouns are tables in the `thread` Postgres schema.
This page lists what each table holds and how the tables are keyed.
Every column is declared in its file under `rustd/crates/daemon/afd_db/schema/`; that file wins where this page disagrees.

## Why it exists

One fleet has one conversation, and every part of it needs one home.
The thread row is where two assignments of one fleet meet, so only one run holds the fleet at a time.
The turn rows are the work queue, so accepted work survives any process or bus loss.

Naming, timestamp and identifier rules live in [Postgres schemas](postgres_schemas.md) and the SQL standard.

## How it behaves

### thread.threads

One row per fleet, written in the transaction that creates the fleet (`20261108123650_thread_threads.sql`).
Its `id` equals the fleet's id, and `ck_threads_id_is_fleet` holds that.

| Column | Null | Meaning |
|---|---|---|
| `fleet_id` | no | Foreign key to `fleet.fleets`, `ON DELETE CASCADE`: a thread goes with its fleet |
| `fencing_token` | no | Raised once per assignment; every write from a run must match it |
| `last_runner_id` | yes | The sticky runner a held sandbox waits on; `ON DELETE SET NULL` |
| `run_expires_at` | no | When the live run lapses; a new thread starts already lapsed |
| `hold_expires_at` | yes | How long that runner keeps the sandbox for the fleet's next run |
| `resume_context` | yes | Conversation state the next run resumes from, set by the report |

An assignment claims the row with one conditional upsert (`afd_fleet`'s `run/sql/shared/assign_thread.sql`).
It raises the fence only when no live run or other runner's hold blocks it.

### thread.turns

One row per input on a thread (`20261108125100_thread_turns.sql`).
Intake writes a turn `queued`. The assignment that first runs it stamps its fence and sets `opened_at`.

The report settles it by setting `settled_at`.
A history-only receipt, such as a model change, is written opened and settled with no producer, so nothing runs it.

It is written on the change's own transaction (`afd_turns::view::activity::record_on`).
`afd_turns::view::activity::commit` commits, then tells open tails; a refused commit announces nothing.

| Group | Columns | Meaning |
|---|---|---|
| Key | `id`, `thread_id`, `workspace_id`, `sequence` | `sequence` orders a thread's turns; `UNIQUE (thread_id, sequence)` |
| Intake | `kind`, `actor`, `request_json` | What arrived and who sent it |
| Retry key | `producer`, `producer_key`, `payload_digest` | All set or all NULL; `UNIQUE (producer, producer_key)` stops a duplicate |
| Reply route | `reply_provider`, `reply_address` | Where a connector answer goes; both set or both NULL |
| Run stamps | `run_fence`, `input_run_id`, `input_fence` | The run that took the turn as its head, and the run that folded it in |
| Markers | `opened_at`, `interpreted_at`, `settled_at`, `parked_at` | Opened as an event, read by a durable task, finished; statements only clear `parked_at` |
| Answer | `status`, `answer_text`, `tool_calls`, `token_count`, `wall_ms`, `failure_reason`, `failure_detail` | The outcome; `tool_calls` is a summary, and `thread.items` holds the detail |
| Continuation | `resumes_turn_id` | The turn this one continues; `ON DELETE SET NULL` |
| Durable task | `durable_task_id`, `durable_task_generation`, `plan`, `plan_digest`, `durable_task_activity*` | Task link, its accepted plan (written once) and public progress |
| Recovery | `head_checkpoint_id`, `next_checkpoint_generation` | The checkpoint a replacement run restores |

Foreign keys go to `thread.threads` and `identity.workspaces` with `ON DELETE CASCADE`.
`fk_turns_durable_task_id_scope` points at `fleet.durable_tasks` by `(id, fleet_id, workspace_id)`.
`fk_turns_head_checkpoint` is added by `ALTER` in the checkpoint table's file, because the two tables point at each other.

Two triggers count a turn in `fleet.activity_counters` once, when it opens.
`trg_turns_refuse_plan_change` refuses a second write of `plan`.

The stored statuses are named in `af_core::turn::status`:

| Status | Meaning |
|---|---|
| `queued` | Accepted; no run has opened it |
| `received` | Opened; a run holds it |
| `recorded` | History-only; never runs |
| `processed` | Answered |
| `fleet_error` | The run failed, or lapsed with no holder left to report |
| `gate_blocked` | The daemon refused the assigned work, so no runner reports it |
| `waiting` | The run asked the person a question; the answer arrives as a new turn |
| `cancelled` | A person pressed Stop |

### thread.runs

One row per try at a turn (`20261108123200_thread_runs.sql`).
Its statuses are `active`, `reported`, `expired` and `stopped`.

| Column | Meaning |
|---|---|
| `fleet_id`, `workspace_id`, `tenant_id`, `turn_id`, `turn_created_at` | Scope, copied so billing and history read without a join |
| `runner_id` | The runner; `ON DELETE SET NULL` |
| `fencing_token`, `run_expires_at` | The assignment's fence and the renewal deadline |
| `posture`, `provider`, `model` | What the run may do and which model it uses |
| `last_metered_at` | Metering cursor; never past `run_expires_at` |
| `cancel_requested_at` | The durable Stop mark |
| `decision_snapshot` | The decision revision this run delivered |
| `durable_task_*` | Task generation, stop generation and runtime accounting |

`runner_id` is the only foreign key.
A settled run is history that outlives its fleet, turn and task, so those ids are held without a foreign key.

### thread.items

One row per tool call a run made, written when the call finishes (`20261108125110_thread_items.sql`).
`kind` is `tool_call` or `edit`, and `status` is `succeeded` or `failed` (`af_wire::tool_detail::item`).

The row holds `tool`, `arguments`, `output`, line counts, truncation flags, and for an edit, `changes` as `[{path, added, removed}]`.
`fleet_id` cascades from the fleet; `turn_id` cascades from the turn.

`run_id` is `ON DELETE SET NULL`, so an item outlives its run row.
`fencing_token` then finds the run's head turn.
`UNIQUE (run_id, sequence)` makes a retried post write each item once.

### thread.deliveries

One row per answer owed to one destination (`20261108122700_thread_deliveries.sql`).
A lane is one `(provider, workspace_id)` pair, ordered by `sequence`; Slack is the only provider.

A worker claims a lane head by writing a fresh `receipt` and `claim_expires_at`.
Every later write matches that receipt, so a lapsed holder's late write changes nothing.
`delivered_at` or `abandoned_at` ends the row; `attempt_count` counts tries.

`fleet_id` cascades from the fleet. `UNIQUE (fleet_id, turn_id)` allows one delivery per turn.
`turn_id` has no foreign key, because a Slack notice owes a delivery with no turn behind it.

### thread.compactions and thread.replay_receipts

`thread.compactions` holds one fleet's private working summary, one row per thread.
Only a context compaction writes it, fenced by the run, and it cascades from the thread.

`thread.replay_receipts` holds every retry key, keyed `(scope, operation_id)`.
It is append-only and has no foreign keys, so a key outlives what it created.

`runner.history` records runner events, not conversations, so it stays in the `runner` schema.

## Limits

- Two turn references carry no foreign key; [Postgres schemas](postgres_schemas.md#limits) lists them.
- `thread.runs` names no fleet or turn by foreign key, so a fleet purge deletes its active runs in its own statement.
- Nothing deletes `runner.history` rows or settled runs; no retention policy exists.
- `durable_task_activity` has no length cap in the schema.

## Related

- [Postgres schemas](postgres_schemas.md): every schema, the naming rules and the deletion matrix.
- [Data flow](data_flow.md): how a turn moves from intake to answer.
- [Datastore scaling](datastore_scaling.md): why the turn rows are the queue.
- [Checkpoints](checkpoints.md): the checkpoint head on a turn.
