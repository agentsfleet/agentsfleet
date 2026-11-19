# Thread model

A conversation is a **thread**. Each input on it is a **turn**, each try at answering a turn is a **run**, and each thing a run does is an **item**.

Each answer owed to someone is a **delivery**. This page defines the tables behind those nouns, every foreign key and why it exists, and what each table replaces.

Every table follows the schema rules in `.orly/standards/domains/sql.md`:
- **Identifiers.** Every minted identifier is a `UUID` holding a UUIDv7 from `af_core::id`, bound and read as `Uuid7`. `TEXT` holds only external ids, keys and digests.
- **Timestamps.** Every timestamp is `TIMESTAMPTZ(3)`, and every table has `created_at` and `updated_at`, both `NOT NULL`.
- **Values.** Status and kind columns are `TEXT`, with their values as named constants in Rust and no schema `CHECK` or string `DEFAULT`.
- **Files.** Each table is one schema file holding its whole `CREATE`. The one `ALTER` adds the foreign key that closes the turn-run cycle, in the later table's file.

## Naming convention

Every name below follows the pattern most of `schema/` already uses (75 `uq_`, 42 `ck_` and 38 `fk_` constraints). `<table>` is the bare table name, without its schema.

| Object | Name | Example |
|--------|------|---------|
| Primary key | `id UUID PRIMARY KEY` | `thread.turns.id` |
| Foreign key | `fk_<table>_<column>`, always named, so a refusal is matched by name (SQL-FKSTATE) | `fk_turns_thread_id` |
| Composite foreign key | `fk_<table>_<first column>_scope` | `fk_turns_task_id_scope` |
| Unique | `uq_<table>_<columns>` | `uq_turns_thread_id_sequence` |
| Index | `idx_<table>_<columns>` | `idx_turns_thread_id_created_at_id` |
| Partial index | `idx_<table>_<columns>_<state>` | `idx_turns_thread_id_sequence_unsettled` |
| Check | `ck_<table>_<rule>` | `ck_threads_one_owner` |
| Trigger | `trg_<table>_<what>` | `trg_history_refuse_update` |

Every foreign-key column gets its own `idx_<table>_<column>`, unless it leads another index or unique key. Postgres indexes neither side of a foreign key on its own, and every `ON DELETE CASCADE` scans the child.

## Schemas

The `thread` schema holds `threads`, `turns`, `runs`, `items` and `deliveries`. The `setup` schema replaces `authoring` and holds a draft's own rows. `runner.history` stays in the `runner` schema, because it records runners, not conversations.

## thread.threads

One row per fleet and one per draft, written in the same statement that creates its owner with fencing token 0 and a lapsed run deadline. It replaces `runner.slots` and `fleet.sessions`.

| Column | Type | Null | Came from | Why |
|--------|------|------|-----------|-----|
| `id` | `UUID` PK | no | slot id | equals the fleet's or draft's id, so no lookup maps one to the other |
| `fleet_id` | `UUID` | yes | `runner.slots.fleet_id` | FK `fleet.fleets(id)` ON DELETE CASCADE: a thread dies with its fleet |
| `draft_id` | `UUID` | yes | `runner.slots.draft_id` | FK `setup.drafts(id)` ON DELETE CASCADE: a thread dies with its draft |
| `fencing_token` | `BIGINT` | no | `runner.slots.fencing_token` | raised once per assignment |
| `last_runner_id` | `UUID` | yes | `runner.slots.last_runner_id` | FK `runner.runners(id)` ON DELETE SET NULL: the sticky runner a warm sandbox waits on; a deleted runner leaves the thread free |
| `run_expires_at` | `TIMESTAMPTZ(3)` | yes | `runner.slots.lease_expires_at` | the live run's expiry |
| `hold_expires_at` | `TIMESTAMPTZ(3)` | yes | `runner.slots.hold_expires_at` | how long the runner keeps the warm sandbox |
| `resume_context` | `JSONB` | yes | `fleet.sessions.context_json` | the conversation state a fleet's next run resumes from |

Checks: exactly one of `fleet_id` and `draft_id` is set, and `id` equals the one that is set, so which one is set is the thread's kind. Indexes: `fleet_id` and `draft_id` partial, `last_runner_id` for the foreign key, and `last_runner_id` where a hold is set for the hold sweep. The report sets `resume_context` in the same update that releases the run.

Publishing a draft creates a new fleet, so the fleet gets its own thread. The draft's thread stays with the draft until the draft is deleted.

## thread.turns

One row per input on a thread. It replaces `fleet.admissions` and `fleet.events`, which split one fleet input across two rows, and `setup.messages`, a setup draft's chat inputs (`schema/20261108125100_thread_turns.sql`).

A fleet's intake writes the turn `queued`. The claim that first runs it stamps its fence and opens its event in one update, and the report settles it and closes the event in one update, since one statement may change a row once. A history-only receipt, such as an activity or a model change, is written opened and settled, with no producer, so no claim takes it.

| Column | Type | Null | Came from | Why |
|--------|------|------|-----------|-----|
| `id` | `UUID` PK | no | `admissions.id` = `events.event_id`; `messages.id` | the turn id, which every table and the wire name `turn_id` |
| `sequence` | `BIGINT` identity by default | no | `admissions.sequence`; `messages.sequence` | unique with `thread_id`. A fleet's turns draw it in intake order, a history-only receipt included; a setup draft numbers its own from 1, because its watermarks and attempts name turns by it |
| `thread_id` | `UUID` | no | `fleet_id`; `messages.draft_id` | FK `thread.threads(id)` ON DELETE CASCADE |
| `workspace_id` | `UUID` | no | `workspace_id`; the draft's | FK `identity.workspaces(id)` ON DELETE CASCADE |
| `kind` | `TEXT` | no | `event_type`; `chat` for a setup turn | `chat`, `webhook`, `schedule`, `continuation`, `activity`, and the rest of the event type vocabulary |
| `actor` | `TEXT` | no | `actor`; the sender's OIDC subject for a setup turn | who sent it |
| `status` | `TEXT` | no | `events.status`, and `queued` before the event opens; `messages.status` | see the status map below |
| `producer`, `producer_key`, `payload_digest` | `TEXT` | yes, all or none | `admissions` | retry dedup, unique `(producer, producer_key)`; a resend with another body is refused. NULL on a history-only receipt and on a setup turn, so `producer IS NOT NULL` is an admitted fleet input |
| `request_json` | `JSONB` | no | `admissions.request_json`, `events.request_json`; `messages.user_text` as `{"message": …}` | the input itself; readers that decode a string select `request_json::text` |
| `reply_provider`, `reply_address` | `TEXT` | yes | `admissions.reply_*` | where a connector answer goes; both or neither |
| `answer_text` | `TEXT` | yes | `events.response_text`, `messages.answer_text` | the answer |
| `tool_calls` | `JSONB` | yes | `events.tool_calls` | the compact trace summary the thread page reads; full detail is in `thread.items` |
| `token_count`, `wall_ms` | `BIGINT` | yes | `events` | usage shown on the turn; NULL until a run reports |
| `failure_reason`, `failure_detail` | `TEXT` | yes | `events`, `messages.failure_reason` | why a turn failed |
| `resumes_turn_id` | `UUID` | yes | `events.resumes_event_id` | FK `thread.turns(id)` ON DELETE SET NULL: a continuation names the turn it resumes |
| `run_fence`, `input_claim_run_id`, `input_claim_fence` | `BIGINT`, `UUID`, `BIGINT` | yes | `admissions` | the fence of the claim that took the turn as its head, and of the run whose conversation folded it in. No foreign key: a fence outlives its run |
| `opened_at` | `TIMESTAMPTZ(3)` | yes | new | when the fleet's event opened; history reads and the activity counter read only opened turns |
| `parked_at`, `interpreted_at`, `settled_at` | `TIMESTAMPTZ(3)` | yes | `admissions`; `messages.finished_at` | waiting on an approval; read by a task; finished. A setup turn settles when it finishes |
| `task_id`, `task_item_id`, `task_generation` | `UUID`, `UUID`, `BIGINT` | yes | `admissions` | composite FKs `(task_id, thread_id, workspace_id)` to `fleet.tasks` and `(task_item_id, thread_id, workspace_id)` to `fleet.task_items`, ON DELETE SET NULL on the id column, as today |
| `task_activity`, `task_activity_sequence`, `task_activity_truncated` | `JSONB`, `BIGINT`, `BOOLEAN` | no | `events` | bounded task progress, at most 128 entries |
| `operation_id` | `UUID` | yes | `messages.operation_id` | a setup turn's request operation. Not a `producer_key`: it is a minted `UUID` the wire returns, and it is not unique, because the retry window lives in `setup.replay_receipts` and the same key queues a new turn once it passes |
| `tenant_model_id` | `UUID` | yes | `messages` | FK `model.tenant_models(id)` ON DELETE SET NULL: the model a setup turn chose; NULL is the platform default |
| `candidate_id` | `UUID` | yes | `messages` | FK `setup.candidates(id)` ON DELETE SET NULL: the candidate a setup turn wrote |
| `save_requested`, `save_error_code` | `BOOLEAN`, `TEXT` | no, yes | `messages` | a setup turn that saves the head, and why a checked save was refused |

Indexes: `(thread_id, sequence)` unique; `(producer, producer_key)` unique; `(thread_id, created_at, id)`, `(thread_id, actor, created_at, id)` and `(thread_id, kind, created_at, id)` for history pages; `(workspace_id, created_at, id)` for the workspace stream; `(thread_id, sequence) WHERE settled_at IS NULL` for assignment; `(thread_id, reply_provider, reply_address, sequence) WHERE settled_at IS NULL` for one conversation's next turns; `(task_id, task_generation, settled_at, sequence)` for tasks; `(status, thread_id) WHERE producer IS NULL AND settled_at IS NULL` for the setup drafts with a turn waiting or running; `task_item_id`, `resumes_turn_id`, `tenant_model_id` and `candidate_id` for their foreign keys.

Two triggers, `trg_turns_count_inserted_opened` and `trg_turns_count_opened`, bump `fleet.activity_counters.event_count` once per turn, when its event opens.

A fleet's thread is written with the fleet, and a draft's with the draft, so a turn always has one. A setup turn never opens a fleet event, so the history reads and the activity counter skip it, and the fleet queue reads that scan across threads require `producer IS NOT NULL`. A turn's `created_at` is its intake instant, so a webhook's event now sorts at intake rather than at its first claim.

`admissions.delivered_at` is dropped: a redelivery is a turn whose `run_fence` is already set, and `thread.deliveries` is the one record of delivery. `admissions.event_created_at` is dropped, because it always equalled `created_at`.

Still to come, from the design: one `run_id` replacing `run_fence` and the input claim pair, and the status renames below, which wait for the public-surface rename because the stored words reach the wire.

### Status map

A fleet turn stores the event words today, plus `queued`; a setup turn stores the setup words. The target column is the rename.

| Stored today | Meaning | Target |
|--------------|---------|--------|
| `queued` | admitted, event not opened | `queued` |
| `received` | event opened, a run holds it | `running` |
| `recorded` | a history-only input that never gets a run | `recorded` |
| `gate_blocked` | refused at a gate, or waiting with `parked_at` set | `waiting` |
| `processed` | answered | `answered` |
| `fleet_error` | failed | `failed` |
| `queued`, `running`, `completed`, `failed`, `cancelled` on a setup turn | a setup draft's turn | `queued`, `running`, `answered`, `failed`, `cancelled` |

## thread.runs

One row per try at a turn. It replaces `runner.leases` and `setup.attempts`.

| Column | Type | Null | Came from | Why |
|--------|------|------|-----------|-----|
| `id` | `UUID` PK | no | `leases.id` | the run id |
| `thread_id` | `UUID` | no | `leases.fleet_id` / `draft_id` | FK `thread.threads(id)` ON DELETE CASCADE |
| `turn_id` | `UUID` | no | `leases.event_id` | FK `thread.turns(id)` ON DELETE CASCADE: the turn the run was assigned for |
| `runner_id` | `UUID` | no | `leases.runner_id` | FK `runner.runners(id)` ON DELETE CASCADE, as today |
| `workspace_id`, `tenant_id` | `UUID` | no | `leases` | FKs to `identity.workspaces` and `identity.tenants` ON DELETE CASCADE |
| `fencing_token` | `BIGINT` | no | `leases.fencing_token` | unique with `thread_id`: one run per fence |
| `status` | `TEXT` | no | `leases.status` | `active`, `reported`, `expired`, `stopped` |
| `posture`, `provider`, `model` | `TEXT` | no | `leases` | what the run is allowed and which model it uses |
| `run_expires_at` | `TIMESTAMPTZ(3)` | no | `lease_expires_at` | renewal deadline |
| `max_runtime_seconds` | `INTEGER` | no | `leases` | hard cap |
| `last_metered_at` | `TIMESTAMPTZ(3)` | no | `leases` | metering cursor |
| `cancel_requested_at` | `TIMESTAMPTZ(3)` | yes | `leases` | the durable Stop mark |
| `posted_message_count` | `INTEGER` | no | `leases` | the per-run message cap |
| `decision_snapshot`, `setup_revision_id` | `JSONB`, `UUID` | yes | `leases` | decision budget and the fleet revision run |
| `task_id`, `task_generation`, `task_stop_generation`, `task_started_at`, `task_runtime_accounted_at`, `task_execution_deadline_at` | as today | yes | `leases` | task slice accounting |

The owner is the thread's `kind`, so runs carry no `fleet_id` or `draft_id`. `leases.event_created_at` is dropped, because the turn holds its own `created_at`.

The cycle `turns.run_id` → `runs` and `runs.turn_id` → `turns` closes with one `ALTER TABLE thread.turns ADD CONSTRAINT … FOREIGN KEY (run_id)` in the runs file.

## thread.items

One row per thing a run did. It replaces `fleet.tool_calls`.

| Column | Type | Null | Came from | Why |
|--------|------|------|-----------|-----|
| `id` | `UUID` PK | no | `tool_calls.id` | the item id |
| `run_id` | `UUID` | no | `event_id` + `fencing_token` | FK `thread.runs(id)` ON DELETE CASCADE |
| `turn_id` | `UUID` | no | `event_id` | FK `thread.turns(id)` ON DELETE CASCADE: the turn newest when the item's step began |
| `thread_id`, `workspace_id` | `UUID` | no | `fleet_id`, `workspace_id` | FKs ON DELETE CASCADE: the detail route authorizes without a join |
| `sequence` | `BIGINT` | no | `sequence` | order within the run; unique with `run_id`, which makes a retried post write once |
| `kind` | `TEXT` | no | new | `tool_call`, `edit`, `integration`, `approval`, `agent` |
| `tool` | `TEXT` | yes | trace JSON | the tool's name |
| `status` | `TEXT` | no | trace JSON | `running`, `succeeded`, `failed` |
| `arguments`, `truncated_arguments`, `output`, `output_line_count`, `truncated`, `byte_count` | as today | no | `tool_calls` | the "show all" detail |
| `changes` | `JSONB` | yes | new | for `edit`: an array of `{path, added, removed}` |

An item is written when its call finishes, as records are posted today. When a resumed run settles a turn, the report deletes that turn's items from every other run.

## thread.deliveries

One row per answer owed to one destination. It replaces `fleet.obligations`.

| Column | Type | Null | Came from | Why |
|--------|------|------|-----------|-----|
| `id` | `UUID` PK | no | `obligations.id` | the delivery id |
| `turn_id` | `UUID` | no | `event_id` (no FK today) | FK `thread.turns(id)` ON DELETE CASCADE: a delivery needs its turn |
| `thread_id`, `workspace_id` | `UUID` | no | `fleet_id`, `workspace_id` | FKs ON DELETE CASCADE |
| `sequence` | `BIGINT` | no | `sequence` | lane order |
| `provider`, `destination` | `TEXT` | no | as today | `slack` or `email`, and where |
| `answer` | `TEXT` | no | as today | the text sent |
| `receipt` | `UUID` | yes | as today | the worker holding it |
| `taken_until` | `TIMESTAMPTZ(3)` | yes | `claim_expires_at` | the worker's hold |
| `next_attempt_at`, `attempt_count`, `delivered_at`, `abandoned_at` | as today | as today | as today | retry state |

Unique `(turn_id, provider, destination)`, so one turn can owe a Slack answer and an email to each recipient.

## runner.history

It replaces `runner.events`, column for column, except that `event_type` becomes `kind`. It keeps the append-only trigger, the retention sweep, and the unique `(runner_id, last_seen_at)` index for offline events.

## Tables that point at the replaced ones

Every column that holds a turn id is named `turn_id` (or `source_turn_id`), and the wire says `turn_id` too. A foreign key is added only where nothing outlives the turn; a table that keeps its row after the turn holds the id opaque.

| Table | Old column | New column and FK |
|-------|------------|-------------------|
| `runner.turn_recovery` | `event_id` → `fleet.events` | `turn_id` → `thread.turns(id)` ON DELETE CASCADE (done) |
| `runner.turn_checkpoints`, `turn_checkpoint_pins`, `turn_checkpoint_refs` | `event_id` | `turn_id`, which points at `turn_recovery` (done) |
| `runner.turn_checkpoint_operations` | `event_id` | `turn_id`, `fk_turn_checkpoint_operations_turn_id` → `thread.turns(id)` ON DELETE CASCADE (done): an allocation is made only under the turn's recovery head, so the turn always exists |
| `thread.items` | `event_id` → `fleet.events` | `turn_id` → `thread.turns(id)` ON DELETE CASCADE (done; the file moved to `20261108125110` to follow `thread.turns`) |
| `fleet.approval_gates` | `event_id` | `turn_id` (done), null for an install grant card. The FK to `thread.turns(id)` ON DELETE CASCADE waits for M253_003: the gate fixtures still bind a minted id with no turn behind it |
| `thread.deliveries` | `event_id` | `turn_id` (done). No FK yet: a Slack notice owes a delivery that names its own row id (`afd_ingress/src/slack/notice.rs`), so it has no turn |
| `thread.runs` | `event_id`, `event_created_at` | `turn_id`, `turn_created_at` (done). No FK yet: a settled run outlives its owner, so the FK arrives with the `thread_id` change |
| `billing.usage_lines` | `event_id`, `authoring_attempt_id` | `turn_id`, `run_id` (done; `run_id` names a setup run, a fleet line names its run in `work_id`). No FK: a line outlives what it names. `work_type` values are unchanged |
| `runner.decision_calls` | `source_admission_id` | `source_turn_id` (done). No FK: the call outlives its run and turn |
| `fleet.task_items`, `fleet.task_interpretations` | `admission_id` | `turn_id` (done). No FK: an interpretation receipt outlives its source turn |

## Bus subjects

Only the daemon uses NATS. The runner reaches the daemon on its connection.

| Subject | Replaces | Who subscribes |
|---------|----------|----------------|
| `af.thread.{thread_id}.wake` | `af.wake.{shard}` (16 shards) | every daemon replica, once, with the wildcard `af.thread.*.wake`; the wake queue is keyed by thread |
| `af.thread.{thread_id}.items` | `af.fleet.{fleet_id}.tail`, `af.draft.{draft_id}.events` | each open stream, for its one thread |
| `af.runner.{runner_id}.cmd` | unchanged | the gateway holding that runner's connection |
| `af.delivery.wake` | `af.outbound.wake` | the delivery worker |
| `af.bus.gap` | the private `GAP_ALL` constant | every listener; a gap triggers the Postgres re-read |

A lost message is recovered by the existing backstop re-read, so Postgres stays the authority.
