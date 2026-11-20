# Postgres schemas — layout, tables, and naming

> Canonical for which Postgres schema a table lives in, what each table is for, and how
> tables link. Indy signed the design off table by table on Oct 17, 2026, and milestone
> M229 rebuilt `schema/*.sql` from it. Each schema file describes what is deployed.

## Why this exists

Tables had accumulated in `core` until it held 33 of 47, while the setup-chat tables alone spread
across `core`, `fleet` and `billing`. Columns were added with no reader, the same fact was stored
in two or three places, and timestamps multiplied. This file fixes placement and naming so a new
table has one obvious home and one obvious shape.

## Rules

1. **Placement.** A table lives in the schema of the product area that owns it. Its name is a
   plural noun that never repeats the schema name: `setup.drafts`, never with the schema name repeated as a prefix.
   A singleton is named for its one row (`model.catalogue_version`) and says so in its slot.
2. **Foreign keys** end `_id` and name the table they point at (`user_id`, `draft_id`). A prefix
   names the role when the column alone would not say it (`invited_by_user_id`, `active_grant_id`,
   `source_workspace_id`, `last_runner_id`). A deliberately missing foreign key is explained in the
   slot comment (money outlives an account; a charge outlives a purged fleet).
3. **Status** columns are named `status`; categories are `<thing>_type`. RULE STS applies in full:
   no literal that names a value or identifier the application also names. That rules out:
   - a `DEFAULT`, a CHECK value list, or a trigger comparison against a column;
   - a `current_setting()` name, or `RAISE EXCEPTION` text a caller matches on;
   - a state literal in a predicate: a predicate that needs a state reads a NULL marker column;
   - a session setting for a privileged path; such a path is a grant, which is why
     `fleet.approval_gates` carries no DELETE grant. A gate goes only through its fleet's cascade;
   - anything beyond the UUID version nibble `'7'`, `TG_OP` values, and exception text no caller
     matches on, which are the only literals left.
4. **Timestamps.** Every instant is `TIMESTAMPTZ(3)`, and every instant column ends `_at`; no other
   column is a timestamp. Rust holds them as `jiff::Timestamp` and the API emits RFC 3339 in UTC.

   Statements write `created_at` and `updated_at` as `now()`. The clock rule for expiry checks is
   SQL-DEADLINE in the [SQL standard](../../.orly/standards/domains/sql.md).

   Per SQL-TIMECOL, every table has `created_at`, and every table whose rows change has
   `updated_at`. Any other time column is one of four kinds:

   - a deadline, named `*expires_at`;
   - a NULL marker an index or trigger reads;
   - a domain instant the API returns that `updated_at` cannot carry;
   - an accounting cursor billing reads (`thread.runs.last_metered_at`).
5. **One fact, one home.** No JSON blob copies a column; no counter restates another table's number.
   Named exceptions, each with its consistency rule stated in the slot:
   - an aggregate maintained under the same lock or trigger as its source;
   - a delivery snapshot that must outlive its source;
   - a non-secret projection of encrypted content;
   - a scope copy a restricted role needs to filter alone.
6. **Retry keys** (`operation_id` + request digest) are enforced only by receipt tables. When an API
   returns a key for longer than its receipt lives, the row may keep that key as a plain column
   (`thread.turns.operation_id`). That column decides no replay.
7. **Every column names its reader** before it is created. Accounting and audit records are exempt
   when the slot names the export or verification that reads them.
8. **A new table** carries a written answer to "why not a column, or an existing table".
9. **Immutable tables** carry no `updated_at`, no UPDATE grant, and a trigger that refuses UPDATE.
   DELETE stays governed by grants and the deletion matrix below, so cascades keep working.

### Counter and pointer names

| Kind | Name |
|---|---|
| Position within a parent (1, 2, 3…) | `sequence` |
| Pointer to another table's position | `<thing>_sequence` |
| Highest position so far | `last_<thing>_sequence` |
| "Everything up to N" watermark | `<verb>_through_sequence` |
| Global insertion order | `sequence` (identity column) |
| Frozen content version | `revision`; pointer `<x>_revision_id`; the live one `head_revision_id` |
| Compare-and-swap edit counter | `resource_version`, or `<scope>_version` when it covers one action |
| Run epoch | `fencing_token` |
| Tally | `<thing>_count` |
| Deadline | `*expires_at` |

Wire field names (API and runner protocol) are not renamed by these rules.

## Layout

Each table's columns live in its `schema/` slot, the only copy kept.

| Schema | Owns |
|---|---|
| `identity` | Accounts, people, membership, credentials, preferences |
| `setup` | The private setup chat, its runner work, and the fleet Plugin |
| `fleet` | Deployed fleets, their frozen setup history, approvals and durable tasks |
| `thread` | Every conversation: threads, turns, runs, items and deliveries |
| `runner` | Enrolled hosts, their history and counters |
| `connector` | Outside accounts, channel bindings, fleet-to-service grants |
| `model` | Model catalogue, prices, platform defaults, tenant models |
| `billing` | The wallet and every charge against it |
| `vault` | Encrypted secrets |
| `memory` | Fleet memory, behind its own database role |

`audit` holds the migration ledger. SQLx's `Migrator` applies `schema/*.sql` from the explicit
list in `afd_db`'s migration module. It keeps checksums, so an edited applied file is refused, and
runs inside our bounded advisory lock.

Each table has one file, `<UTC yyyymmddHHMMSS>_<schema>_<table>.sql`, applied in foreign-key
order and holding the table's whole `CREATE`: columns, constraints, indexes, grants, functions and
triggers. The only `ALTER` is a foreign key that closes a cycle, kept in the later table's file.
Until the first production deploy, a change to a table edits its file; after it, a change is a new
`<UTC>_<schema>_<table>_<what>.sql`.

## Runtime SQL

Every static statement lives in a `.sql` file beside the use case that runs it (for example
`plugin/sql/page.sql`). A typed Rust function loads it with `include_str!` and states its
parameters, authorization and locks. Rows decode into named `FromRow` types; `Json<T>` is for
real JSON columns only.

One integration test `PREPARE`s every `.sql` file against the rebuilt schema, so a stale table or
column fails before merge. A statement that must be assembled at run time is named in its module
and covered by its own test. Domain stores keep taking the caller's connection or transaction;
there is no generic repository trait.

## identity

```
 tenants ◀── workspaces ◀── user_preferences ──▶ users
    ▲  ◀── users ◀── cli_credentials
    ▲  ◀── memberships ──▶ users
    ▲  ◀── invites ──▶ users (invited_by / accepted_by)
    ▲  ◀── api_keys
```

| Table | Purpose | Notes |
|---|---|---|
| `tenants` | Billing and ownership root |  |
| `workspaces` | Isolation unit inside a tenant | `UNIQUE(id, tenant_id)` for composite references |
| `users` | One person, bound to a sign-in subject; `tenant_id` is the personal account made at signup |  |
| `memberships` | Person ↔ tenant with a role; a role change is delete + insert |  |
| `invites` | Emailed offer to join a tenant |  |
| `api_keys` | Tenant automation tokens, digest plus nullable public display suffix; outlive the admin who made them |  |
| `cli_credentials` | One live CLI login per machine, hash only | (metadata columns are in §Settled) |
| `user_preferences` | Small dashboard settings per (user, workspace) |  |
| `device_sessions` | One CLI device login in flight; ciphertext sealed to the CLI, code kept as a peppered HMAC | no foreign key: a session exists before any user does; operator roles revoked (M244) |

`invites.resolved_at` is NULL while an invite is pending and is set on accept or revoke. The
"one pending invite per address" unique index and the "what is waiting for me" index read
`resolved_at IS NULL`. That is the marker pattern `fleet.approval_gates` uses (rule 3).

## setup

```
 identity.workspaces/users ◀── drafts ──▶ fleet.fleets (fleet_id)
                                 │ head_candidate_id ──▶ candidates
                                 │ plugin_revision_id ──▶ plugin_revisions
                                 │ source_fleet_revision_id ──▶ fleet.revisions
              thread.threads (1:1) ──▶ drafts; thread.turns.candidate_id ──▶ candidates
 attempts ──▶ runner.runners   ON DELETE SET NULL: a revoked runner's record goes, its charges stay
          (no FK to drafts/turns: a charge outlives chat text)
   ◀── dispatches, activity_receipts, answer_receipts
 plugin_entries ◀── plugin_revisions (ON DELETE CASCADE)
 replay_receipts (no FKs: a retry key outlives erasure)
```

| Table | Purpose | Notes |
|---|---|---|
| `drafts` | One private setup chat; its turns are `thread.turns` rows on the draft's thread |  |
| `candidates` | A frozen numbered setup the model proposed | append-only |
| `attempts` | One runner run running one setup turn, money bounded, model pinned | `reserved_nanos` is the ceiling each funds check asks for, with nothing set aside; `outcome` is set once usage settles |
| `dispatches` | One physical model call: intent before, result after; `native_reply` is the provider's own reply kept for replay, private like `result` |  |
| `edit_decisions` | Setup edit-intent checks with their reservation and measured usage | no private text or secrets; opaque setup-chat accounting survives deletion; unresolved calls keep their reservation |
| `activity_receipts` | Replay proof for "runner posted a setup" | append-only |
| `answer_receipts` | Replay proof for "runner posted an answer" | append-only |
| `replay_receipts` | Every setup retry key: create draft, send message, publish | append-only, swept at a fixed age |
| `plugin_entries` | Account-owned card; tenant is required |  |
| `plugin_revisions` | A frozen published version | append-only |

- **Model pointers.** `drafts.tenant_model_id` and `thread.turns.tenant_model_id` are NULL for the
  platform default and otherwise reference a `model.tenant_models` row of the same tenant. The wire
  keeps today's string: `platform-default` maps to NULL, a tenant model id maps to the row.
- **Chat save intent.** `thread.turns.save_requested` records the accepted request to save after a completed turn. The report writer saves the setup with the answer. A checked save refusal keeps the answer and records its safe code in `save_error_code`.
- **Source and publication are separate facts**. `source_type` (blank, definition, plugin, fleet)
  plus `source_plugin_revision_id` or `source_fleet_revision_id` record where the chat started.
  `plugin_revision_id` records what it published, so a draft can start from Plugin revision A and
  publish revision B.

  All three references are `ON DELETE SET NULL`, so deleting a card or purging
  a fleet never blocks on, or deletes, someone's private draft.
- **Retention.** Closing a draft stamps `closed_at` and `expires_at = closed_at + 30 days` (an API
  field); the sweeper deletes past `expires_at`. The sweep cascades the draft's thread and turns,
  candidates and answer receipts, and keeps attempts and dispatches (result and native reply scrubbed).
- **Retry window.** A replay receipt answers its key for 24 hours from `created_at` (`afd_setup::replay::REPLAY_WINDOW`). Inside the window, the same key and body return the first result. A different body or user is refused; a deleted resource returns Gone.
- At or after the boundary, the key is new. The next claim deletes the expired receipt and files its own. `Drafts::maintain` sweeps the rest in bounded batches, waiting for concurrent receipt deletion.
- The message API returns `thread.turns.operation_id`, written at acceptance, because the turn outlives its receipt.
- **Lapse sweep.** `idx_attempts_status_deadline_id` indexes `(status, LEAST(run_expires_at,
  expires_at), id)`, the maintenance sweep's own filter and order. The sweep reads lapsed
  attempts oldest first and stops at its batch limit without a sort.
- **Running totals**. `attempts.call_count` and `output_token_count` are aggregates maintained under
  the attempt lock and read by assignment and renewal (rule 5 exception). The input total had no reader.
- The setup JSON is copied onto each frozen revision table on purpose. A shared definitions table
  would turn draft deletion from a cascade into a reference-counted sweep.

## fleet

```
 identity.workspaces ◀── fleets ──▶ revisions (head_revision_id)
 fleets ◀── revisions, schedules, activity_counters (1:1), thread.threads (1:1),
            approval_gates ──▶ connector.integration_grants,
            task_items ◀── tasks ◀── task_inputs
```

| Table | Purpose | Notes |
|---|---|---|
| `fleets` | The deployed agent |  |
| `revisions` | Frozen setup history; rev 1 at Save, +1 per Apply | append-only |
| `task_items` | Immutable interpretation receipts survive turn cleanup | append-only; task association closes a deferred cycle |
| `task_adoptions` | Human goal-revision retry receipts | append-only opaque scope; goal text and native target stay on tasks |
| `tasks` | One durable objective, its accepted setup, cumulative limits and lifecycle | one originating task item |
| `task_inputs` | Accepted task input and its consumption order, separate from queue delivery | unique task/item and task/sequence |
| `task_interpretations` | Complete private source message and accepted plan | append-only; survives turn settlement and cascades with fleet |
| `task_input_receipts` | Immutable apply, queued and original-reply outcomes | unique item/mode and fleet/operation; private replies cascade with item |
| `task_waits` | One native exact-head dependency owner | scoped task/generation/stop, separate read claim deadline and monotonic signal cursor; one continuation consumes the wait |
| `task_completions` | Verified workflow completion inbox | immutable scoped run/attempt evidence; early-race backup; acknowledged rows collect after three days |
| `task_completion_fanout` | Durable signal-delivery cursor for immutable completion evidence | bounded supervisor fanout; one wait lock per transaction; cannot allocate a run |
| `task_effects` | Immutable native expected-head publication authority | kept opaque task/ordinal and task/operation identities |
| `task_effect_states` | Acknowledgement and confirmed remote commit | cannot rewrite the effect request; kept with opaque effect |
| `task_effect_payloads` | Private repository/branch/workflow binding | append-only; cascades with task |
| `task_effect_recovery` | Expiring encrypted exact-repository read custody | survives owner deletion until reconciliation or expiry; no write credential |
| `task_identity_keys` | Wrapped workspace/repository content-identity versions | one active version; independent of object encryption keys and master-key rewrapping |
| `task_storage_quota` | Unique reserved/stored bytes per workspace | aggregate mutated under its source lock; pending deletion stays charged |
| `task_bases` | Independently kept verified repository base | exact repository/commit/format/toolbox identity; outlives its first task |
| `task_checkpoints` | Task capture plan and complete compatible recovery generation | native sequence, fixed pages/deadlines; ready incremental manifest alone becomes recovery pointer |
| `task_checkpoint_operations` | Opaque capture-plan retry receipts | append-only; survives private plan retention and owner deletion |
| `task_checkpoint_pages` | Bounded immutable incremental page bytes | native page identity/position; identical replay is read without update |
| `task_base_pages` | Independently kept immutable base pages | cascades with base rather than its first task |
| `task_objects` | Random scoped encrypted object, wrapped key and frozen ciphertext descriptor | one live keyed-content identity; quota charged once; erasure records purge custody |
| `task_object_refs` | Workspace-scoped checkpoint/object or base/object edges | composite ownership keys; one owner per edge; referenced objects cannot be collected |
| `task_object_pins` | One current capture/restore operation and deadline | versioned run/fence; retention serializes with task pin acquisition |
| `task_storage_purges` | Durable remote-delete custody and quota-release receipt | opaque object/location plus charged bytes/disposition; no owner foreign key |
| `schedules` | Cron or one-shot runs, synced to the external scheduler |  |
| `approval_gates` | A risky action waiting for a person | `turn_id` names its turn, with no foreign key yet |
| `activity_counters` | Trigger-maintained dashboard totals | `thread.turns` triggers count each turn once, when it opens |

The live setup is read from `revisions` only; `fleets` keeps no config copy.
`revisions.review_context` is the frozen review subject (creator, source pin, instance name) that
activation re-reviews against. A Plugin install has no draft, and a swept draft cannot supply it.

`revisions.runtime_model_ref` is the model resolved at Save or Apply, kept because a definition may
omit its model and review then resolves the tenant default.

## thread

```
 fleet.fleets / setup.drafts ◀── threads (1:1, id = owner id) ──▶ runner.runners (last_runner_id)
 threads ◀── turns ◀── items
 turns ◀── turns (resumes_turn_id); turns ──▶ fleet.tasks, fleet.task_items (composite scope)
 runs ──▶ runner.runners; ··▶ fleets, drafts, turns: opaque, no FK
 deliveries ──▶ fleet.fleets; ··▶ turns (turn_id): opaque, no FK yet
```

[Thread model](thread_model.md) owns each column, foreign key and the mapping from the tables these replace.

| Table | Purpose | Notes |
|---|---|---|
| `threads` | One conversation per fleet and per draft: next fencing token, sticky runner, warm sandbox hold, resume bookmark | `ck_threads_one_owner`: exactly one of `fleet_id` and `draft_id`, and `id` equals it; `resume_context` is the report's resume bookmark |
| `turns` | One input on a thread and its answer; the fleet's sole work queue | `UNIQUE (producer, producer_key)` so retries cannot duplicate work; `UNIQUE (thread_id, sequence)`; assignments and reports fence each row; `input_claim_run_id` and `input_claim_fence` stamp a message a live run took; `task_activity` keeps at most 128 public stages / 120 KB |
| `runs` | One turn handed to one runner. Settled history outlives a purge; the insert proves scope in the same transaction | non-secret decision snapshot; no foreign key to fleet, draft, turn, task or historical model revision |
| `items` | Full arguments and output of each tool call a run made | `UNIQUE (turn_id, fencing_token, sequence)`; cascades with its turn |
| `deliveries` | An answer owed to a destination until delivered or abandoned | The outbound queue: a worker claims lane heads with its token in `receipt` until `claim_expires_at`; `next_attempt_at` defers a released row |

`runs.turn_created_at` is the producer's instant billing uses, copied so billing reads it without a join.
The run's `actor` and turn `kind` come from a join to `thread.turns`, as reassignment already does.

## runner

```
 runners ◀── history, lifetime_counters (1:1)
 runners ◀── thread.runs, thread.threads (last_runner_id)
 runners ──▶ identity.tenants (tenant_id, NULL for a platform runner)   ON DELETE CASCADE
 registration_receipts (no FKs: a retry key outlives its runner and its tenant)
```

| Table | Purpose | Notes |
|---|---|---|
| `runners` | An enrolled host: policy, health, self-test, and the tenant that owns it | `platform_owned` says who owns it and `tenant_id` is set exactly when it is false; both fixed after insert by a trigger; a check refuses a decision model on a tenant row; optional reviewed decision-model selection; referenced models cannot be deleted |
| `registration_receipts` | One tenant registration's retry key and request digest, so a retry finds the runner it created | append-only; unique `(tenant_id, operation_id)`; no foreign keys, so a receipt outlives its runner and its tenant |
| `decision_calls` | Kept physical-call identity, frozen prices, reservations and canonical usage | opaque original ownership survives deletion; unresolved rows prevent whole-reservation release |
| `decision_payloads` | Bounded private evidence and answers for a call | fleet deletion removes payloads while accounting stays |
| `history` | Append-only runner history | unique `(runner_id, last_seen_at) WHERE last_seen_at IS NOT NULL`: an offline event stamps the heartbeat it observed, so one stale episode is one row |
| `lifetime_counters` | Per-runner tallies |  |

`sandbox_config` holds tier-specific knobs as JSON validated in the application. Namespace tiers use `{"extra_binds": [...]}` today, and microVM keys arrive with the tier that reads them.
Degraded and self-test pass are derived on read (`degraded_reason IS NOT NULL`, every check passed),
keeping "no self-test yet" distinct from "self-test failed".

The metering sweep finds expired runs with an unbilled tail through
`idx_runs_run_expires_at_unbilled`, partial on `last_metered_at < run_expires_at`. A run
leaves the index once metering reaches its expiry, so the index holds only the backlog.

## connector

| Table | Purpose |
|---|---|
| `installs` | Outside account (Slack team, GitHub installation) → workspace; credentials stay in the vault |
| `person_links` | OAuth-proved Slack person → current workspace member; installation deletion cascades the link |
| `channels` | Stable Slack channel notice owner; current workspace/team checks reject stale bindings |
| `integration_grants` | Whether a fleet may use a service; deny is stored as revoked |
| `oauth_nonces` | A connect's single-use state nonce, as its SHA-256 under its provider; spent by `DELETE … RETURNING`, refused after ten minutes (M244) |

## model

| Table | Purpose |
|---|---|
| `platform_models` | Curated identities, context evidence and current decision selection |
| `platform_model_prices` | Prices per prompt-size tier and effective date range, in integer nanos per million tokens; a separate table because one model has many prices |
| `decision_revisions` | Immutable reviewed decision definitions; a model/revision label cannot acquire different bytes |
| `decision_prices` | Separate decision-protocol rates keep coding writers and historical revision accounting; composite revision/model reference proves quote ownership |
| `catalogue_version` | Singleton edit counter, bumped by every catalogue edit and read with each runtime model resolution |
| `platform_defaults` | Default model per provider and the admin workspace holding its key |
| `tenant_models` | A tenant's own models, each pointing at a vault key |
| `tenant_selections` | The tenant's default; NULL means platform default |

The current decision pointer uses a deferred identity reference and an ownership constraint trigger.
The trigger checks final rows and locks their immutable revision, without granting update access or public access to the trigger function.

`model_name` is the provider's name for a model and never a foreign key. `installs.scopes` is read
by the setup review, which blocks a connection whose granted scopes fall short.

`platform_models.inference` holds the saved API format, endpoint, dialect and output limit.
Routing follows those settings; credential endpoint restrictions must match them.
[Billing §9](billing_and_provider_keys.md#9-provider-routing-saved-model-settings) owns provider routing.

## billing

| Table | Purpose | Notes |
|---|---|---|
| `wallets` | The tenant's balance; the only row money moves on | `balance_nanos` is signed: negative while the tenant owes a debt |
| `setup_report_receipts` | Acknowledges a setup run's interim report without settling | append-only |
| `usage_lines` | One priced provider request or runtime slice of billed work; what the charges page, fleet budgets and the fleet list's `budget_used_nanos` trigger read | `UNIQUE (work_type, work_id, kind, seq)` · fleet runs and setup chat both write it (billing §11) |
| `usage_daily` | Settled usage per tenant, UTC day, work type, fleet and model; what the Usage report reads | natural key `UNIQUE NULLS NOT DISTINCT (tenant_id, day, work_type, fleet_id, model)` · written in the settle transaction · rebuildable from `usage_lines` |
| `runs_daily` | Settled runs per tenant, UTC day, work type and fleet | natural key `UNIQUE NULLS NOT DISTINCT (tenant_id, day, work_type, fleet_id)` · a run counts once, on its first runtime slice |

Nothing is set aside before work runs. A funds check reads the balance less the payer's unsettled
lines, and no `holds` table exists (billing §11).

Money outlives every deletion. `usage_lines` carries opaque `tenant_id`, `workspace_id`,
`fleet_id` and `run_id` values with no foreign key, and `usage_daily` and
`runs_daily` carry opaque `tenant_id` and `fleet_id` the same way. `setup.attempts` keeps
its workspace, tenant and user identifiers without a foreign key.

Erasing a fleet, workspace or account therefore never deletes a charge. The slots name the export
that reads them (rule 7).

## Deletion matrix

| Root deleted | Goes with it | Stays | Scrubbed immediately |
|---|---|---|---|
| Draft (user delete or 30-day sweep) | its thread and turns, candidates, answer receipts, draft context checkpoint | attempts, dispatches, usage lines, receipts | `dispatches.result` and `dispatches.native_reply` of its attempts |
| User | their drafts (as above), memberships, CLI credentials, preferences | api keys they created, attempts, usage lines | `dispatches.result` and `dispatches.native_reply` of their attempts |
| Plugin card | its revisions | fleets installed from it; drafts started from it (`source_plugin_revision_id` set NULL) | none |
| Fleet (purge) | revisions, its thread, turns, items, gates, deliveries, schedules, grants, channels, memory, live (`active`) runs; task objectives, interpreted text, input replies, private effect payloads, task checkpoints/pages/pins/refs | usage lines; settled runs for their 30-day window; opaque decision/effect/adoption/checkpoint-operation receipts; expiring encrypted effect read custody; independently kept workspace repository bases/objects; linked drafts have fleet references set NULL | Unreferenced objects enter guarded storage collection; remote purge stays tracked through acknowledgement |
| Workspace | drafts, fleets (as above), installs, secrets, preferences; private repository bases/pages, identity keys, object keys/metadata and quota | attempts, usage lines; opaque task/effect/checkpoint-operation and storage-purge receipts; expiring encrypted effect read custody | Setup private results; each removed object first records its location and charged bytes for remote cleanup |
| Tenant (account erasure) | workspaces (as above), users, wallet, tenant models, selection, invites, api keys, runners with their runs, history and counters | attempts, usage lines, daily usage and run rows, registration receipts (opaque identifiers only) | `dispatches.result` and `dispatches.native_reply` of its attempts |
| Device session (past `expires_at`) | nothing: the row stays, and every read refuses it | none | none; the row holds a sealed ciphertext and a code digest |
| OAuth nonce (spent at the callback, or presented past `expires_at`) | the row, at its spend; one never presented stays | none | none; the row holds only a digest |
| Runner (record retired after revoke) | runs, history, lifetime counters | attempts (`runner_id` set NULL), usage lines; threads (`last_runner_id` set NULL) | none |

## vault and memory

| Table | Purpose |
|---|---|
| `vault.secrets` | One encrypted secret per (workspace, key_name), with a plain projection |
| `memory.entries` | A fleet's durable memory, behind role `memory_runtime` |
| `memory.context_checkpoints` | Private working notes owned by a setup draft; deleted with the draft |

## Settled while implementing

Each item is decided from the rebuilt slots and the statements that read them. A kept column
names its slot and a reader; a dropped one names the slot that declares its table without it.
A column kept with no reader still owes a drop under rule 7.

- **`identity.api_keys.description`: kept, no reader.** The file `20261108120700_identity_api_keys.sql` declares it and
  `afd_tenant`'s `insert_tenant_key.sql` writes it. The key list selects
  `id, name, created_at, revoked_at`, and `ApiKeySummary` has no description field.
- **`identity.cli_credentials.credential_prefix`, `deployment`, `created_from_address`: dropped.**
  The file `20261108120800_identity_cli_credentials.sql` declares the table without them. The
  table's one read, `afd_state`'s `select_cli_credential.sql`, selected none of them, and the mint
  response echoes the deployment answering the request.
- **`identity.cli_credentials.tenant_id`: kept.** The file `20261108120800_identity_cli_credentials.sql` declares it and
  `select_cli_credential.sql` reads it during authentication. The mint copies the user's tenant,
  and the reading statement already joins `identity.users`. The column therefore restates
  `identity.users.tenant_id` (rule 5) with no consistency rule in its slot.
- **`setup.attempts.settled_nanos`: dropped.** The file `20261108123900_setup_attempts.sql` declares the table without it. The
  settled charge is `billing.usage_lines.charged_nanos` (the file `20261108124600_billing_usage_lines.sql`), summed per work type by
  `afd_billing`'s `select_owed.sql`.

Settled by review on Oct 17, 2026, and checked against the rebuilt slots:

- **`fleet.revisions.runtime_model_ref`: kept.** The file `20261108122000_fleet_revisions.sql` declares it; the run reads it in
  `afd_fleet`'s `select_fleet_with_session.sql`.
- **`model.platform_defaults.base_url`: kept.** The file `20261108121400_model_platform_defaults.sql` declares it; `afd_credential`'s
  `select_active_platform_default.sql` reads it.
- **`thread.runs.last_metered_at`: kept.** The file `20261108123200_thread_runs.sql` declares it as the run's runtime cursor
  (rule 4 (d)). Renewal and report meter from it and advance it (`afd_fleet`'s `renew_batch.sql` and `commit_report.sql`). The expiry sweep bills the last slice from it (`select_unbilled_expired_runs.sql` and
  `bill_expired_slice.sql`).

  The review listed it as dropped. Per-request billing kept this one
  cursor and dropped the run's token cursors and the slot's cursors instead.
- **`runner.runners.selftest_all_ok`: dropped.** Derived from `selftest_checks` (the file `20261108123100_runner_runners.sql`), read
  by `afd_runner`'s `runner_detail.sql`.
- **`runner.runners.capability_reported_at`: dropped.** The file `20261108123100_runner_runners.sql` declares the table with
  `capability_report` and no instant beside it.
- **`setup.attempts.input_token_count`: dropped.** The file `20261108123900_setup_attempts.sql` keeps `call_count` and
  `output_token_count` only; input tokens are counted per request on `billing.usage_lines` (the file `20261108124600_billing_usage_lines.sql`).
- **`setup.dispatches.output_allowance`: dropped.** The file `20261108124000_setup_dispatches.sql` declares the table without it.
- **`setup_context`: replaced** by `fleet.revisions.review_context` (the file `20261108122000_fleet_revisions.sql`), written by
  `afd_setup`'s creation `revision.sql` and read by its `locked.sql`.

Added by this milestone:

- **`billing.usage_lines.kind`: added by M245.** What the line bills is one of three kinds. `model` is a call the runner reported, numbered by its
  request. `runtime` is a slice the daemon measured, numbered by the milliseconds from its work's start to the slice's start.

  `decision` is a native decision call, numbered by its call. The unique key is `(work_type, work_id, kind, seq)`, so no
  kind's numbers are carved out of another's. The `u32` bands `DAEMON_SEQ_BASE` and `DECISION_SEQ_BASE` that kept them
  apart are gone.

  Spellings are `af_wire::usage::LineKind`.

- **`runner.runners.platform_owned`, `tenant_id` and `runner.registration_receipts`: added by M246.** A tenant registers
  runners of its own, and a runner takes only its tenant's work. Ownership is stated at insert, and
  `ck_runners_owner` requires `platform_owned = (tenant_id IS NULL)`, so a missing tenant never reads as the platform. The partial index
  `idx_runners_tenant_id_created_at_id` serves a tenant's list and the erasure cascade.
- **`idx_runners_created_at_id`: added by M246.** The runner list pages newest first. A volume bench measured the list at
  3.7 ms before the index and 0.1 ms after.
- **Unused indexes dropped by the M246 audit.** `idx_task_effects_source_item`, `idx_task_effect_states_reconcile` and
  `idx_task_checkpoints_expiry`: no statement filters or orders on them, and each cost a write per row.
- **Write-only columns dropped by the M246 audit.** Each was written by a statement and read by none.
  - `fleet.approval_gates`: `spend_count`, `spend_ceiling`, `stated_binding`.
  - `identity.cli_credentials`: `credential_prefix`, `deployment`, `created_from_address`.
  - `setup.edit_decisions.snapshot`, `thread.deliveries.abandon_reason`, `fleet.task_bases.retained_bytes`.
  - `fleet.task_completions`: `conclusion`, `body_digest`.
  - `fleet.task_effect_states.confirmed_at`, `fleet.task_waits.consumed_admission_id`, `fleet.tasks.last_transition_at`.
  - `fleet.task_inputs` and `fleet.task_interpretations`: `accepting_run_id`, `accepting_fence`.
  - `fleet.task_storage_purges.delete_after_at`.
- **Write-only by name, kept by evidence.** `identity.api_keys.display_suffix` is read by the key list and shown in the API
  and the dashboard. `fleet.task_effects.source_item_id` is a field of the public task-effect input.

## Public interface changes

- The runner protocol carries no `deletion_generation` (run payload, activity, model calls, renew, report); a spooled report written before the change still decodes.
  The one development runner restarts on the new release; no fleets exist when it does.
- `agentsfleet models use` writes through `model.tenant_models`; its output is unchanged.
- Closed-draft retention, the 24-hour retry window and Plugin-card deletion (history goes with the
  card) are documented in the public docs.
