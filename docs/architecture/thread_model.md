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

One row per fleet and one per draft. It replaces `runner.slots` and `fleet.sessions`.

| Column | Type | Null | Came from | Why |
|--------|------|------|-----------|-----|
| `id` | `UUID` PK | no | slot id | equals the fleet's or draft's id, so no lookup maps one to the other |
| `kind` | `TEXT` | no | new | `fleet` or `draft` |
| `fleet_id` | `UUID` | yes | `runner.slots.fleet_id` | FK `fleet.fleets(id)` ON DELETE CASCADE: a thread dies with its fleet |
| `draft_id` | `UUID` | yes | `runner.slots.draft_id` | FK `setup.drafts(id)` ON DELETE CASCADE: a thread dies with its draft |
| `workspace_id` | `UUID` | no | new | FK `identity.workspaces(id)` ON DELETE CASCADE: reads authorize without a join |
| `fencing_token` | `BIGINT` | no | `runner.slots.fencing_token` | raised once per assignment |
| `runner_id` | `UUID` | yes | `runner.slots.last_runner_id` | FK `runner.runners(id)` ON DELETE SET NULL: a deleted runner leaves the thread free |
| `run_expires_at` | `TIMESTAMPTZ(3)` | yes | `runner.slots.lease_expires_at` | the live run's expiry |
| `hold_expires_at` | `TIMESTAMPTZ(3)` | yes | `runner.slots.hold_expires_at` | how long the runner keeps the warm sandbox |
| `resume_context` | `JSONB` | yes | `fleet.sessions.context_json` | the conversation state a fleet's next run resumes from |

Checks: exactly one of `fleet_id` and `draft_id` is set, and `id` equals the one that is set. Indexes: `fleet_id` and `draft_id` partial, and `(runner_id, hold_expires_at)` for the hold sweep.

Publishing a draft creates a new fleet, so the fleet gets its own thread. The draft's thread stays with the draft until the draft is deleted.

## thread.turns

One row per input on a thread. It replaces `fleet.admissions`, `fleet.events` and `authoring.messages`, which today split one input across three rows.

| Column | Type | Null | Came from | Why |
|--------|------|------|-----------|-----|
| `id` | `UUID` PK | no | `admissions.id` = `events.event_id`; `messages.id` | the turn id |
| `thread_id` | `UUID` | no | `fleet_id` / `draft_id` | FK `thread.threads(id)` ON DELETE CASCADE |
| `workspace_id` | `UUID` | no | `admissions.workspace_id` | FK `identity.workspaces(id)` ON DELETE CASCADE |
| `sequence` | `BIGINT` | no | `admissions.sequence`, `messages.sequence` | order on the thread; unique with `thread_id` |
| `kind` | `TEXT` | no | `event_type` | `message`, `schedule`, `webhook`, `continuation`, `task` |
| `actor` | `TEXT` | no | `actor` | who sent it |
| `status` | `TEXT` | no | see the status map below | `queued`, `running`, `waiting`, `answered`, `failed`, `cancelled`, `recorded` |
| `producer`, `producer_key` | `TEXT` | no | `admissions.producer`, `producer_key`; `messages.operation_id` | retry dedup: unique together, so a resent input writes nothing |
| `payload_digest` | `TEXT` | no | `admissions.payload_digest` | a resend with another body is refused |
| `request_json` | `JSONB` | no | `admissions.request_json`, `events.request_json`, `messages.user_text` | the input itself |
| `answer_text` | `TEXT` | yes | `events.response_text`, `messages.answer_text` | the answer |
| `reply_provider`, `reply_address` | `TEXT` | yes | `admissions.reply_*` | where a connector answer goes; both or neither |
| `run_id` | `UUID` | yes | `lease_fence`, `input_claim_lease_id`, `input_claim_fence` | FK `thread.runs(id)` ON DELETE SET NULL: the run serving this turn, shared by every turn one run serves |
| `resumes_turn_id` | `UUID` | yes | `events.resumes_event_id` | FK `thread.turns(id)` ON DELETE SET NULL: a continuation names the turn it resumes |
| `parked_at`, `interpreted_at`, `settled_at` | `TIMESTAMPTZ(3)` | yes | `admissions` | waiting on an approval; read by a task; finished |
| `token_count`, `wall_ms` | `BIGINT` | no | `events` | usage shown on the turn |
| `failure_reason`, `failure_detail` | `TEXT` | yes | `events`, `messages.failure_reason` | why a turn failed |
| `task_id`, `task_item_id`, `task_generation` | `UUID`, `UUID`, `BIGINT` | yes | `admissions` | composite FKs to `fleet.tasks` and `fleet.task_items` ON DELETE SET NULL on the id column, as today |
| `task_activity`, `task_activity_sequence`, `task_activity_truncated` | `JSONB`, `BIGINT`, `BOOLEAN` | no | `events` | bounded task progress, at most 128 entries |
| `tenant_model_id` | `UUID` | yes | `messages` | FK `model.tenant_models(id)` ON DELETE SET NULL: the model a setup turn chose |
| `candidate_id` | `UUID` | yes | `messages` | FK `setup.candidates(id)` ON DELETE SET NULL: the candidate a setup turn wrote |
| `save_requested`, `save_error_code` | `BOOLEAN`, `TEXT` | no, yes | `messages` | a setup turn that saves the head |

Indexes: `(thread_id, sequence)` unique; `(thread_id, created_at, id)` for history pages; `(workspace_id, created_at, id)` for the workspace stream; `(thread_id, sequence) WHERE settled_at IS NULL` for assignment; `(task_id, task_generation, settled_at, sequence)` for tasks.

`admissions.delivered_at` is dropped, because `thread.deliveries` is the one record of delivery.

### Status map

| Old value | Table | New status |
|-----------|-------|------------|
| `received` | events | `queued` |
| `recorded` | events | `recorded`: a history-only input that never gets a run |
| `gate_blocked` | events | `waiting`, with `parked_at` set |
| `processed` | events | `answered` |
| `fleet_error` | events | `failed` |
| `queued`, `running`, `completed`, `failed`, `cancelled` | messages | `queued`, `running`, `answered`, `failed`, `cancelled` |

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

| Table | Old column | New column and FK |
|-------|------------|-------------------|
| `runner.turn_recovery` | `event_id` → `fleet.events` | `turn_id` → `thread.turns(id)` ON DELETE CASCADE |
| `runner.turn_checkpoints`, `turn_checkpoint_pins`, `turn_checkpoint_operations`, `turn_checkpoint_refs` | `event_id`, `lease_id` | `turn_id`, `run_id`, FKs ON DELETE CASCADE |
| `fleet.approval_gates` | `event_id` | `turn_id` → `thread.turns(id)` ON DELETE CASCADE, null for an install grant card, which has no turn |
| `billing.usage_lines` | `event_id`, `authoring_attempt_id`, `work_type` | `turn_id`, `run_id`, `work_type` values `turn`, `stage`, `setup` |
| `runner.decision_calls` | `lease_id` | `run_id` |
| `fleet.task_effects`, `task_input_receipts`, `task_checkpoints`, `task_objects`, `task_object_pins`, `task_waits` | `event_id`, `lease_id` | `turn_id`, `run_id` |

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
