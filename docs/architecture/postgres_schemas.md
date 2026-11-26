---
type: reference
audience: contributor
verified: 2026-11-25
product_version: 0.90.0
executable: false
---

# Postgres schemas

## What it is

This page says which Postgres schema each table lives in, what each table is for, and how tables link and delete.
Each table's columns live only in its file under `rustd/crates/daemon/afd_db/schema/`.

## Why it exists

A new table needs one obvious home and one obvious shape.
Placement, naming and deletion are decided here once, so no two features store the same fact.

## How it behaves

### Rules

The [SQL standard](../../.orly/standards/domains/sql.md) owns every schema rule.
That covers placement (SQL-PLACE), column names (SQL-FKNAME, SQL-STATUS, SQL-COUNTER), time columns (SQL-TIMECOL, SQL-TSTYPE) and one fact per home (SQL-ONEFACT).
It also covers retry keys (SQL-RETRYKEY), immutable tables (SQL-IMMUTABLE) and the deletion matrix (SQL-ERASE).

Schema files hold no status or value literals: no `CHECK` value list and no string `DEFAULT`.
Status words are named constants in Rust.

Constraint names follow one pattern, so a refusal is matched by name (SQL-FKSTATE).
`<table>` is the bare table name, without its schema.

| Object | Name | Example |
|---|---|---|
| Foreign key | `fk_<table>_<column>` | `fk_turns_thread_id` |
| Composite foreign key | `fk_<table>_<first column>_scope` | `fk_turns_durable_task_id_scope` |
| Unique | `uq_<table>_<columns>` | `uq_turns_thread_id_sequence` |
| Index | `idx_<table>_<columns>` | `idx_turns_thread_id_created_at_id` |
| Partial index | `idx_<table>_<columns>_<state>` | `idx_turns_thread_id_sequence_unsettled` |
| Check | `ck_<table>_<rule>` | `ck_checkpoints_one_owner` |
| Trigger | `trg_<table>_<what>` | `trg_history_refuse_update` |

Every foreign-key column gets an index unless it leads another index or unique key.
Postgres indexes neither side of a foreign key on its own, and every `ON DELETE CASCADE` scans the child.

### Files and migrations

Each table has one file, `<UTC yyyymmddHHMMSS>_<schema>_<table>.sql`, holding its whole `CREATE`.
That includes columns, constraints, indexes, grants, functions and triggers.
Files apply in foreign-key order from the list in `afd_db/src/migration.rs`.

The only `ALTER` adds a foreign key that closes a cycle, in the later table's file.
Three exist: `thread.turns` to `runner.checkpoints`, `fleet.durable_tasks` to `runner.checkpoints`, and `fleet.durable_task_items` to `fleet.durable_tasks`.
A fourth closes `model.platform_models` to `model.decision_revisions`.

SQLx's `Migrator` applies the files under a bounded advisory lock and keeps checksums in the `audit` schema, so an edited applied file is refused.
Until the first production deploy, a change to a table edits its file.

### Runtime SQL

Every static statement is a `.sql` file beside the use case that runs it.
A typed Rust function loads it with `include_str!` and states its parameters, authorization and locks.

One integration test `PREPARE`s every `.sql` file against the rebuilt schema, so a stale column fails before merge.
A domain store takes the caller's connection or transaction; there is no generic repository trait.

### Layout

| Schema | Owns |
|---|---|
| `identity` | Accounts, people, membership, credentials |
| `fleet` | Fleets, their triggers, schedules, approval cards and durable tasks |
| `thread` | Every conversation: threads, turns, runs, items, deliveries, compactions and retry keys |
| `runner` | Enrolled hosts, their history, decision calls and the checkpoint system |
| `connector` | Outside accounts, channel bindings, fleet-to-service grants |
| `model` | Model catalogue, prices, platform defaults, tenant models |
| `billing` | The wallet and every charge against it |
| `vault` | Encrypted secrets |
| `memory` | Fleet memory, behind its own database role |

### identity

```
 tenants ◀── workspaces
    ▲  ◀── users ◀── cli_credentials
    ▲  ◀── memberships ──▶ users
    ▲  ◀── invites ──▶ users (invited_by / accepted_by)
    ▲  ◀── api_keys
```

| Table | Purpose | Notes |
|---|---|---|
| `tenants` | Billing and ownership root | Immutable; `identity.lock_tenant` locks it for model selection writes |
| `workspaces` | Isolation unit inside a tenant | Immutable; `UNIQUE (id, tenant_id)` for composite references |
| `users` | One person, bound to a sign-in subject | Immutable; `identity.lock_user` holds a person against deletion |
| `memberships` | Person to tenant, with a role | Immutable; a role change is a delete and an insert |
| `invites` | An emailed offer to join a tenant | `resolved_at` is NULL while pending |
| `api_keys` | Tenant automation tokens: a digest and a display suffix | Outlive the admin who made them |
| `cli_credentials` | One live command-line login per machine, hash only | |
| `device_sessions` | One command-line device login in flight | No foreign key: a session exists before any user does |

### fleet

```
 identity.workspaces ◀── fleets
 fleets ◀── triggers, schedules, activity_counters (1:1), thread.threads (1:1),
            approval_gates ──▶ connector.integration_grants,
            durable_tasks ◀── durable_task_items, durable_task_inputs, durable_task_effects
```

| Table | Purpose | Notes |
|---|---|---|
| `fleets` | The agent a workspace talks to: name, `model_ref`, `http_egress`, `package_domains` | `UNIQUE (workspace_id, name)`; `status` is `active` or `archived`; no instructions column |
| `triggers` | One webhook source, GitHub App subscription or Slack channel that wakes a fleet | `trigger_type` and `source` select the row; `UNIQUE (fleet_id, trigger_type, source)` |
| `schedules` | Cron or one-shot runs, synced to the external scheduler | `source` is `api` for a person's schedule, `fleet` for one added by `cron_add` |
| `approval_gates` | A card waiting for a person: a run's question or a fleet's access request | No `DELETE` grant; goes only with its fleet |
| `activity_counters` | Dashboard totals maintained by triggers | `thread.turns` triggers count each turn once, when it opens |
| `durable_tasks` | One durable objective, its limits, lifecycle and one exact-head wait | `checkpoint_id` points at its recovery generation |
| `durable_task_items` | Immutable interpretation receipts, each with one reply receipt | Survive turn cleanup |
| `durable_task_inputs` | Accepted task input and its consumption order, with one apply receipt | Immutable; unique per task and item, and per task and sequence |
| `durable_task_effects` | A publication's authority, acknowledgement, confirmed commit and private target | Only the acknowledgement and target may change; a purge clears the target |

`triggers` is a table, because a run adds and removes one row at a time and ingress finds rows by workspace, trigger type and source.
A durable task's accepted plan is its source turn's `thread.turns.plan`, and its retry keys live in `thread.replay_receipts`.

### thread

```
 fleet.fleets ◀── threads (1:1, id = fleet_id) ──▶ runner.runners (last_runner_id)
 threads ◀── turns ◀── items;  threads ◀── compactions (1:1)
 turns ◀── turns (resumes_turn_id);  turns ──▶ fleet.durable_tasks (composite scope)
 turns.head_checkpoint_id ──▶ runner.checkpoints
 runs ──▶ runner.runners;  runs ··▶ fleets, turns: no foreign key
 deliveries ──▶ fleet.fleets;  deliveries ··▶ turns: no foreign key
 replay_receipts: no foreign keys
```

[Thread model](thread_model.md) owns every column and key of these tables.

| Table | Purpose |
|---|---|
| `threads` | One conversation per fleet: fence, sticky runner, sandbox hold, resume state |
| `turns` | One input on a thread and its answer; the fleet's work queue |
| `runs` | One try at a turn by one runner; settled runs outlive a fleet purge |
| `items` | Full arguments and output of each tool call a run made |
| `deliveries` | An answer owed to a destination until delivered or abandoned |
| `compactions` | One fleet's private working summary, written only by a compaction |
| `replay_receipts` | Every retry key, keyed `(scope, operation_id)`; append-only |

A fleet create answers its retry key for 24 hours by default (`[setup] replay_window_hours`; `afd_setup`'s `DEFAULT_REPLAY_WINDOW`).
After that, the next claim deletes the expired receipt and files its own (`afd_setup`'s `replay/sql/expire.sql`).

### runner

```
 runners ◀── history;  runners ◀── thread.runs, thread.threads (last_runner_id)
 runners ──▶ identity.tenants (NULL for a platform runner)
 checkpoints ──▶ thread.turns XOR fleet.durable_tasks;  checkpoints ──▶ checkpoint_bases
 checkpoints, checkpoint_bases ◀── checkpoint_pages, checkpoint_object_refs ──▶ checkpoint_objects
 checkpoints ◀── checkpoint_pins;  checkpoint_objects ──▶ checkpoint_identity_keys
 checkpoint_purges: no foreign key, written by the object delete trigger
```

| Table | Purpose | Notes |
|---|---|---|
| `runners` | An enrolled host: policy, health, self-test, the facts its latest beats read, and owner | `ck_runners_owner`: `tenant_id` is set exactly when `platform_owned` is false; `uq_runners_tenant_id_host_id` registers a host once per owner |
| `history` | Append-only runner events | One row per offline episode, keyed by the heartbeat it observed |
| `decision_calls` | A decision call's identity, frozen prices, reservation and usage | Survives deletion of its run and turn |
| `decision_payloads` | Private evidence and answers for a call | Fleet deletion removes them; accounting stays |
| `checkpoints` | A turn's or a durable task's capture plan and recoverable generation | `ck_checkpoints_one_owner`: exactly one of `turn_id`, `durable_task_id` |
| `checkpoint_pages` | Immutable manifest pages | Owned by a checkpoint or a base |
| `checkpoint_bases` | A verified repository base kept on its own | Outlives its first task |
| `checkpoint_objects` | Encrypted object custody for every checkpoint | Erasure records a purge |
| `checkpoint_identity_keys` | Wrapped content-identity keys per workspace and repository | Immutable; written under `identity.lock_workspace_storage` |
| `checkpoint_object_refs` | Checkpoint-to-object and base-to-object edges | Immutable; a referenced object is never collected |
| `checkpoint_pins` | A capture's or restore's hold on one checkpoint | `ck_checkpoint_pins_shape` |
| `checkpoint_purges` | Remote-delete custody for a removed object | No owner foreign key |

`idx_runs_run_expires_at_unbilled` lets a runner's retirement refuse a runner that still owes an unbilled runtime tail (`delete_runner_if_in_state.sql`).
Run counts per runner are the metric `agentsfleet_runner_runs_total`; no table holds them.

### connector

| Table | Purpose |
|---|---|
| `installs` | An outside account (Slack team, GitHub installation) bound to a workspace; credentials stay in the vault |
| `person_links` | An OAuth-proved Slack person bound to a workspace member; cascades with the install |
| `channels` | The Slack channel that owns a fleet's notices |
| `integration_grants` | A fleet's request for one secret or integration and a person's answer; `UNIQUE (fleet_id, service)` |
| `oauth_nonces` | A connect's single-use state nonce, stored as a digest; spent by `DELETE … RETURNING` |

### model

| Table | Purpose |
|---|---|
| `platform_models` | Curated model identities, context limits and the current decision selection |
| `platform_model_prices` | Prices per prompt-size tier and date range, in integer nanos per million tokens |
| `decision_revisions` | Immutable reviewed decision definitions |
| `revision` | Singleton counter, bumped by every catalogue edit |
| `platform_defaults` | Default model per provider, and the admin workspace holding its key |
| `tenant_models` | A tenant's own models, each pointing at a vault key; at most one `selected` |

`model_name` is the provider's name for a model and never a foreign key.
[Models](models.md#routing-from-saved-settings) owns provider routing.

### billing

| Table | Purpose | Notes |
|---|---|---|
| `wallets` | The tenant's balance, the only row money moves on | `balance_nanos` goes negative while the tenant owes |
| `usage_lines` | One priced request or runtime slice | Immutable; `UNIQUE (work_type, work_id, kind, seq)`; triggers debit the wallet |
| `usage_daily` | Usage per tenant, day, work type, fleet and model | Written by a `usage_lines` trigger; rebuildable |
| `runs_daily` | Runs per tenant, day, work type and fleet | A run counts once, on its first runtime slice |

Money outlives every deletion.
These tables hold tenant, workspace, fleet and run ids with no foreign key.
[Billing](billing.md) owns the write path and trigger order.

### vault and memory

| Table | Purpose |
|---|---|
| `vault.secrets` | One encrypted secret per workspace and key name |
| `memory.entries` | A fleet's durable memory, reached only through the `memory_runtime` role |

### Deletion matrix

- **User.** Goes: memberships, command-line credentials. Stays: API keys they made, usage lines.
- **Fleet purge.** Goes: thread, turns, items, gates, deliveries, schedules, triggers, grants, channels, memory, active runs, task rows and checkpoints.
  Stays: usage lines, settled runs, retry receipts, decision and effect receipts, and repository bases.
  Unreferenced objects enter storage collection; remote purges stay tracked.
- **Workspace.** Goes: its fleets as above, installs, secrets, private bases, pages and identity keys.
  Stays: usage lines, task and effect receipts, retry receipts, checkpoint purges.
- **Tenant (account erasure).** Goes: workspaces as above, users, wallet, tenant models, invites, API keys, and runners with their history.
  Stays: usage lines and daily rows; runs, naming no runner.
- **Runner retired after revoke.** Goes: its history. Stays: usage lines and settled runs, with `runner_id` set NULL; threads lose `last_runner_id`.
- **Device session or OAuth nonce.** An expired session stays and every read refuses it; a nonce is deleted when spent.

## Limits

- `identity.cli_credentials.tenant_id` restates `identity.users.tenant_id` with no consistency rule in its file.
- `fleet.approval_gates.turn_id` and `thread.deliveries.turn_id` hold a turn id with no foreign key.
- No retention policy exists; nothing deletes settled runs or runner history.

## Related

- [Thread model](thread_model.md): the conversation tables column by column.
- [Billing](billing.md): the money tables' write path.
- [Checkpoints](checkpoints.md): the checkpoint system.
- [SQL standard](../../.orly/standards/domains/sql.md): every schema rule.
