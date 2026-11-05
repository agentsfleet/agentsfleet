# Postgres schemas — layout, tables, and naming

> Canonical for which Postgres schema a table lives in, what each table is for, and how
> tables link. Design signed off by Indy table by table on Oct 17, 2026; implemented by
> milestone M229 (clean-slate rebuild of `schema/*.sql`). Where this file and a
> `schema/*.sql` header disagree before M229 lands, the slot file describes what is
> deployed and this file describes the target.

## Why this exists

Tables accumulated in `core` until it held 33 of 47, while the setup-chat tables alone spread
across `core`, `fleet` and `billing`. Columns were added with no reader, the same fact was stored
in two or three places, and timestamps multiplied. This file fixes placement and naming so a new
table has one obvious home and one obvious shape.

## Rules

1. **Placement.** A table lives in the schema of the product area that owns it. Its name is a
   plural noun that never repeats the schema name: `authoring.drafts`, never with the schema name repeated as a prefix.
   A singleton is named for its one row (`model.catalogue_version`) and says so in its slot.
2. **Foreign keys** end `_id` and name the table they point at (`user_id`, `draft_id`). A prefix
   names the role when the column alone would not say it (`invited_by_user_id`, `active_grant_id`,
   `source_workspace_id`, `last_runner_id`). A deliberately missing foreign key is explained in the
   slot comment (money outlives an account; a charge outlives a purged fleet).
3. **Status** columns are named `status`; categories are `<thing>_type`. RULE STS applies in full:
   no literal that names a value or identifier the application also names — a `DEFAULT`, a CHECK
   value list, a trigger comparison against a column, a `current_setting()` name, or `RAISE
   EXCEPTION` text a caller matches on. A predicate that needs a state reads a NULL marker column; a
   privileged path is a grant, not a session setting (so `fleet.approval_gates` carries no DELETE
   grant: a gate goes only through its fleet's cascade, `schema/1730_fleet_approval_gates.sql`). The only literals left are
   the UUID version nibble `'7'`, `TG_OP` values, and exception text no caller matches on.
4. **Timestamps.** Every instant is `TIMESTAMPTZ(3)`, and every instant column ends `_at`; no other
   column is a timestamp. Rust holds them as `jiff::Timestamp` and the API emits RFC 3339 in UTC.

   Statements write `created_at` and `updated_at` as `now()`. The clock rule for expiry checks is
   SQL-DEADLINE in the [SQL standard](../../.orly/standards/domains/sql.md).

   Per SQL-TIMECOL, every table has `created_at`, and every table whose rows change has
   `updated_at`. Any other time column is one of four kinds:

   - a deadline, named `*expires_at`;
   - a NULL marker an index or trigger reads;
   - a domain instant the API returns that `updated_at` cannot carry;
   - an accounting cursor billing reads (`runner.leases.last_metered_at`).
5. **One fact, one home.** No JSON blob copies a column; no counter restates another table's number.
   Named exceptions, each with its consistency rule stated in the slot: an aggregate maintained under
   the same lock or trigger as its source; a delivery snapshot that must outlive its source; a
   non-secret projection of encrypted content; a scope copy a restricted role needs to filter alone.
6. **Retry keys** (`operation_id` + request digest) are enforced only by receipt tables. A row may
   keep the key that created it as a plain column when an API returns that key for longer than the
   receipt lives (`authoring.messages.operation_id`); that column decides no replay.
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
| Lease epoch | `fencing_token` |
| Tally | `<thing>_count` |
| Deadline | `*expires_at` |

Wire field names (API and runner protocol) are not renamed by these rules.

## Layout

Each table's columns live in its `schema/` slot, the only copy kept.

| Schema | Owns |
|---|---|
| `identity` | Accounts, people, membership, credentials, preferences |
| `authoring` | The private setup chat, its runner work, and the fleet Library |
| `fleet` | Deployed fleets, their frozen setup history, and their work |
| `runner` | Enrolled hosts, the leases they hold, per-fleet claim slots |
| `connector` | Outside accounts, channel bindings, fleet-to-service grants |
| `model` | Model catalogue, prices, platform defaults, tenant models |
| `billing` | The wallet and every charge against it |
| `vault` | Encrypted secrets |
| `memory` | Fleet memory, behind its own database role |

`audit` holds the migration ledger. SQLx's `Migrator` applies `schema/*.sql` from the explicit
list in `rustd/crates/afd_db/src/migration.rs` (`migrate` feature only; `macros` stays off), keeps
checksums so an edited applied slot is refused, and runs inside our bounded advisory lock.

## Runtime SQL

Every static statement lives in a `.sql` file beside the use case that runs it (for example
`library/sql/page.sql`), loaded with `include_str!` and bound by a typed Rust function that states
its parameters, authorization and locks. Rows decode into named `FromRow` types; `Json<T>` is for
real JSON columns only. One integration test `PREPARE`s every `.sql` file against the rebuilt
schema, so a stale table or column fails before merge. A statement that must be assembled at run
time is named in its module and covered by its own test. Domain stores keep taking the caller's
connection or transaction; there is no generic repository trait.

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
| `api_keys` | Tenant automation tokens, hash only; outlive the admin who made them |  |
| `cli_credentials` | One live CLI login per machine, hash only | (metadata columns are in §Settled) |
| `user_preferences` | Small dashboard settings per (user, workspace) |  |

`invites.resolved_at` is NULL while an invite is pending and is set on accept or revoke. The
"one pending invite per address" unique index and the "what is waiting for me" index read
`resolved_at IS NULL`, the same marker pattern `fleet.approval_gates` uses (rule 3).

## authoring

```
 identity.workspaces/users ◀── drafts ──▶ fleet.fleets (fleet_id)
                                 │ head_candidate_id ──▶ candidates
                                 │ library_revision_id ──▶ library_revisions
                                 │ source_fleet_revision_id ──▶ fleet.revisions
              messages ──▶ drafts; candidate_id ──▶ candidates
 attempts ──▶ runner.runners   ON DELETE SET NULL: a revoked runner's record goes, its charges stay
          (no FK to drafts/messages: a charge outlives chat text)
   ◀── dispatches, activity_receipts, answer_receipts
 library_entries ◀── library_revisions (ON DELETE CASCADE)
 replay_receipts (no FKs: a retry key outlives erasure)
```

| Table | Purpose | Notes |
|---|---|---|
| `drafts` | One private setup chat |  |
| `messages` | One turn: what was typed, what came back |  |
| `candidates` | A frozen numbered setup the model proposed | append-only |
| `attempts` | One runner lease running one message, money reserved, model pinned | `reserved_nanos` is the ceiling its hold reserves; `outcome` is set once usage settles |
| `dispatches` | One physical model call: intent before, result after; `native_reply` is the provider's own reply kept for replay, private like `result` |  |
| `edit_decisions` | Setup edit-intent checks with frozen decision prices and measured usage | no private text or secrets; opaque setup-chat accounting survives deletion; unresolved calls retain their hold |
| `activity_receipts` | Replay proof for "runner posted a setup" | append-only |
| `answer_receipts` | Replay proof for "runner posted an answer" | append-only |
| `replay_receipts` | Every authoring retry key: create draft, send message, publish | append-only, swept at a fixed age |
| `library_entries` | Gallery card; NULL tenant = platform card |  |
| `library_revisions` | A frozen published version | append-only |

- **Model pointers.** `drafts.tenant_model_id` and `messages.tenant_model_id` are NULL for the
  platform default and otherwise reference a `model.tenant_models` row of the same tenant. The wire
  keeps today's string: `platform-default` maps to NULL, a tenant model id maps to the row.
- **Chat save intent.** `messages.save_requested` persists the accepted request to save after a completed turn. The report writer saves the setup with the answer. A checked save refusal retains the answer and records its safe code in `save_error_code` (`schema/2170_authoring_message_save.sql`).
- **Source and publication are separate facts.** `source_type` (blank, definition, library, fleet)
  plus `source_library_revision_id` or `source_fleet_revision_id` record where the chat started;
  `library_revision_id` records what it published. A draft can start from Library revision A and
  publish revision B. All three references are `ON DELETE SET NULL`, so deleting a card or purging
  a fleet never blocks on, or deletes, someone's private draft.
- **Retention.** Closing a draft stamps `closed_at` and `expires_at = closed_at + 30 days` (an API
  field); the sweeper deletes past `expires_at`. The sweep cascades messages, candidates and answer
  receipts and keeps attempts, dispatches (result and native reply scrubbed) and settlements.
- **Retry window.** A replay receipt answers its key for 24 hours from `created_at` (`afd_authoring::replay::REPLAY_WINDOW`). Inside the window, the same key and body return the first result. A different body or user is refused; a deleted resource returns Gone.

  At or after the boundary, the key is new.
  The next claim deletes the expired receipt and files its own.
  `Drafts::maintain` sweeps the rest in bounded batches, waiting for concurrent receipt deletion.

  The message API returns `messages.operation_id`, written at acceptance, because the turn outlives its receipt.
- **Lapse sweep.** `idx_attempts_status_deadline_id` indexes `(status, LEAST(lease_expires_at,
  expires_at), id)`, the maintenance sweep's own filter and order, so the sweep reads lapsed
  attempts oldest first and stops at its batch limit without a sort
  (`schema/1940_authoring_attempts.sql`).
- **Running totals.** `attempts.call_count` and `output_token_count` are aggregates maintained under
  the attempt lock and read by admission and renewal (rule 5 exception); the input total had no reader.
- The setup JSON is copied onto each frozen revision table on purpose: a shared definitions table
  would turn draft deletion from a cascade into a reference-counted sweep.

## fleet

```
 identity.workspaces ◀── fleets ──▶ revisions (head_revision_id)
 fleets ◀── revisions, schedules, sessions (1:1), activity_counters (1:1),
            events ◀── tool_calls, admissions, approval_gates ──▶ connector.integration_grants,
            obligations, task_items ◀── tasks ◀── task_inputs
```

| Table | Purpose | Notes |
|---|---|---|
| `fleets` | The deployed agent |  |
| `revisions` | Frozen setup history; rev 1 at Save, +1 per Apply | append-only |
| `task_items` | Immutable interpretation receipts survive admission cleanup | append-only; task association closes a deferred cycle |
| `task_adoptions` | Human goal-revision retry receipts | append-only opaque scope; goal text and native target remain on tasks |
| `tasks` | One durable objective, its accepted setup, cumulative limits and lifecycle | one originating task item |
| `task_inputs` | Accepted task input and its consumption order, separate from queue delivery | unique task/item and task/sequence |
| `task_interpretations` | Complete private source message and accepted plan | append-only; survives admission settlement and cascades with fleet |
| `task_input_receipts` | Immutable apply, queued and original-reply outcomes | unique item/mode and fleet/operation; private replies cascade with item |
| `task_waits` | One native exact-head dependency owner | scoped task/generation/stop, separate read claim deadline and monotonic signal cursor; one continuation consumes the wait |
| `task_completions` | Verified workflow completion inbox | immutable scoped run/attempt evidence; early-race backup; acknowledged rows collect after three days |
| `task_completion_fanout` | Durable signal-delivery cursor for immutable completion evidence | bounded supervisor fanout; one wait lock per transaction; cannot allocate execution |
| `task_effects` | Immutable native expected-head publication authority | retained opaque task/ordinal and task/operation identities |
| `task_effect_states` | Acknowledgement and confirmed remote commit | cannot rewrite the effect request; retained with opaque effect |
| `task_effect_payloads` | Private repository/branch/workflow binding | append-only; cascades with task |
| `task_effect_recovery` | Expiring encrypted exact-repository read custody | survives owner deletion until reconciliation or expiry; no write credential |
| `task_identity_keys` | Wrapped workspace/repository content-identity versions | one active version; independent of object encryption keys and master-key rewrapping |
| `task_storage_quota` | Unique reserved/stored bytes per workspace | aggregate mutated under its source lock; pending deletion remains charged |
| `task_bases` | Independently retained verified repository base | exact repository/commit/format/toolbox identity; outlives its first task |
| `task_checkpoints` | Task capture plan and complete compatible recovery generation | native sequence, fixed pages/deadlines; ready incremental manifest alone becomes recovery pointer |
| `task_checkpoint_operations` | Opaque capture-plan retry receipts | append-only; survives private plan retention and owner deletion |
| `task_checkpoint_pages` | Bounded immutable incremental page bytes | native page identity/position; identical replay is read without update |
| `task_base_pages` | Independently retained immutable base pages | cascades with base rather than its first task |
| `task_objects` | Random scoped encrypted object, wrapped key and frozen ciphertext descriptor | one live keyed-content identity; quota charged once; erasure records purge custody |
| `task_object_refs` | Workspace-scoped checkpoint/object or base/object edges | composite ownership keys; one owner per edge; referenced objects cannot be collected |
| `task_object_pins` | One current capture/restore operation and deadline | versioned lease/fence; retention serializes with task pin acquisition |
| `task_storage_purges` | Durable remote-delete custody and quota-release receipt | opaque object/location plus charged bytes/disposition; no owner foreign key |
| `schedules` | Cron or one-shot runs, synced to the external scheduler |  |
| `sessions` | Resume bookmark `{last_event_id, last_response}` |  |
| `events` | One row per delivery into the fleet | `UNIQUE(fleet_id, event_id)` |
| `admissions` | Sole work queue; retries cannot duplicate work; claims and reports fence each row | optional task/item association; single-use active-loop input claim |
| `approval_gates` | A risky action waiting for a person |  |
| `obligations` | An answer owed to a destination until delivered or abandoned |  |
| `tool_calls` | Full arguments and output of each tool call |  |
| `activity_counters` | Trigger-maintained dashboard totals |  |

The live setup is read from `revisions` only; `fleets` keeps no config copy.
`revisions.review_context` is the frozen review subject (creator, source pin, instance name) that
activation re-reviews against; a Library install has no draft and a swept draft cannot supply it.
`revisions.runtime_model_ref` is the model resolved at Save or Apply, kept because a definition may
omit its model and review then resolves the tenant default.

## runner

```
 runners ◀── leases ··▶ fleet.events (fleet_event_id), fleet.fleets (fleet_id): opaque, no FK
         ◀── events, lifetime_counters (1:1)
 fleet_slots (1 per fleet) ──▶ runners (last_runner_id), fleet.fleets
```

| Table | Purpose | Notes |
|---|---|---|
| `runners` | An enrolled host: policy, health, self-test | optional reviewed decision-model selection; referenced models cannot be deleted |
| `leases` | One fleet event handed to one runner. Settled history outlives a purge; the insert proves scope in the same transaction | non-secret decision snapshot; no foreign key to fleet, event, task or historical model revision |
| `decision_calls` | Retained physical-call identity, frozen prices, reservations and canonical usage | opaque original ownership survives deletion; unresolved rows prevent whole-hold release |
| `decision_payloads` | Bounded private evidence and answers for a call | fleet deletion removes payloads while accounting remains |
| `fleet_slots` | Per-fleet claim: who may lease it, next fencing token, sticky runner, sandbox hold |  |
| `events` | Append-only runner history | unique `(runner_id, last_seen_at) WHERE last_seen_at IS NOT NULL`: an offline event stamps the heartbeat it observed, so one stale episode is one row |
| `lifetime_counters` | Per-runner tallies |  |

`sandbox_config` holds tier-specific knobs as JSON validated in the application:
`{"extra_binds": [...]}` for namespace tiers today; microVM keys arrive with the tier that reads them.
Degraded and self-test pass are derived on read (`degraded_reason IS NOT NULL`, every check passed),
keeping "no self-test yet" distinct from "self-test failed". `leases.event_created_at` is the
producer's instant billing uses, which the event row does not hold; the lease's `actor` and
`event_type` come from a join to `fleet.events`, as reclaim already does.
The metering sweep finds expired leases with an unbilled tail through
`idx_leases_lease_expires_at_unbilled`, partial on `last_metered_at < lease_expires_at`: a lease
leaves the index once metering reaches its expiry, so the index holds only the backlog
(`schema/1810_runner_leases.sql`).

## connector

| Table | Purpose |
|---|---|
| `installs` | Outside account (Slack team, GitHub installation) → workspace; credentials stay in the vault |
| `channels` | Slack channel → resident fleet; rebinds when a team moves workspace |
| `integration_grants` | Whether a fleet may use a service; deny is stored as revoked |

## model

| Table | Purpose |
|---|---|
| `platform_models` | Curated identities, context evidence and current decision selection |
| `platform_model_prices` | Prices per prompt-size tier and effective date range, in integer nanos per million tokens; a separate table because one model has many prices |
| `decision_revisions` | Immutable reviewed decision definitions; a model/revision label cannot acquire different bytes |
| `decision_prices` | Separate decision-protocol rates preserve coding writers and historical revision accounting; composite revision/model reference proves quote ownership |
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
[Billing §9](billing_and_provider_keys.md#9-provider-routing--saved-model-settings) owns provider routing.

## billing

| Table | Purpose | Notes |
|---|---|---|
| `wallets` | The tenant's balance; the only row money moves on | `balance_nanos` is signed: negative while the tenant owes a debt |
| `authoring_report_receipts` | Acknowledges an interim report without settling | append-only |
| `usage_lines` | One priced provider request or runtime slice of billed work; what the charges page, fleet budgets and the fleet list's `budget_used_nanos` trigger read | `UNIQUE (work_type, work_id, seq)` · fleet runs and setup chat both write it (billing §11) |
| `holds` | The built-in biller's reservation for one piece of work | `UNIQUE (work_type, work_id)` |

Money outlives every deletion: `usage_lines` carries opaque `tenant_id`, `workspace_id`,
`fleet_id` and `authoring_attempt_id` values with no foreign key, and `authoring.attempts` keeps
its workspace, tenant and user identifiers without a foreign key. Erasing a fleet, workspace
or account therefore never deletes a charge. The slots name the export that reads them (rule 7).

## Deletion matrix

| Root deleted | Goes with it | Stays | Scrubbed immediately |
|---|---|---|---|
| Draft (user delete or 30-day sweep) | messages, candidates, answer receipts, draft context checkpoint | attempts, dispatches, settlements, receipts | `dispatches.result` and `dispatches.native_reply` of its attempts |
| User | their drafts (as above), memberships, CLI credentials, preferences | api keys they created, attempts, settlements | `dispatches.result` and `dispatches.native_reply` of their attempts |
| Library card | its revisions | fleets installed from it; drafts started from it (`source_library_revision_id` set NULL) | — |
| Fleet (purge) | revisions, sessions, events, tool calls, admissions, gates, obligations, schedules, grants, channels, memory, slot, live (`active`) runner leases; task objectives, interpreted text, input replies, private effect payloads, task checkpoints/pages/pins/refs | charges; settled runner leases for their 30-day window; opaque decision/effect/adoption/checkpoint-operation receipts; expiring encrypted effect read custody; independently retained workspace repository bases/objects; linked drafts have fleet references set NULL | Unreferenced objects enter guarded storage collection; remote purge stays tracked through acknowledgement |
| Workspace | drafts, fleets (as above), installs, secrets, preferences; private repository bases/pages, identity keys, object keys/metadata and quota | attempts, settlements, charges; opaque task/effect/checkpoint-operation and storage-purge receipts; expiring encrypted effect read custody | Authoring private results; each removed object first records its location and charged bytes for remote cleanup |
| Tenant (account erasure) | workspaces (as above), users, wallet, tenant models, selection, invites, api keys | attempts, settlements, charges (opaque identifiers only) | `dispatches.result` and `dispatches.native_reply` of its attempts |
| Runner (record retired after revoke) | leases, history, lifetime counters | attempts (`runner_id` set NULL), settlements; slots (`last_runner_id` set NULL) | — |

## vault and memory

| Table | Purpose |
|---|---|
| `vault.secrets` | One encrypted secret per (workspace, key_name), with a plain projection |
| `memory.entries` | A fleet's durable memory, behind role `memory_runtime` |
| `memory.context_checkpoints` | Private working notes owned by a draft or a fleet lease and run branch; deleted with the owner |

## Settled while implementing

Each item is decided from the rebuilt slots and the statements that read them. A kept column
cites its slot line and a reader; a dropped one cites the slot that declares its table without it.
A column kept with no reader still owes a drop under rule 7.

- **`identity.api_keys.description` — kept, no reader.** Declared at
  `schema/1150_identity_api_keys.sql:11` and written by
  `rustd/crates/afd_tenant/src/sql/apikey/insert_tenant_key.sql:1`. The key list selects
  `id, name, created_at, revoked_at` (`rustd/crates/afd_tenant/src/sql/apikey.rs:59`), and
  `ApiKeySummary` has no description field.
- **`identity.cli_credentials.credential_prefix`, `deployment`, `created_from_address` — kept,
  no reader.** Declared at `schema/1160_identity_cli_credentials.sql:13`–15 and written only by
  `rustd/crates/afd_tenant/src/sql/cli_credential/insert_cli_credential.sql:1`. The table's one
  read, `rustd/crates/afd_state/src/sql/select_cli_credential.sql:1`, selects none of them, and
  the mint response echoes the request's deployment (`rustd/crates/afd_tenant/src/cli_credential/mod.rs:211`),
  not the stored one.
- **`identity.cli_credentials.tenant_id` — kept.** Declared at
  `schema/1160_identity_cli_credentials.sql:10`; authentication reads it at
  `rustd/crates/afd_state/src/sql/select_cli_credential.sql:1`. The mint copies the user's tenant
  (`rustd/crates/afd_api_tenant/src/handler/tenant/cli_credential.rs:99`), and the reading
  statement already joins `identity.users`, so the column restates `identity.users.tenant_id`
  (rule 5) with no consistency rule in its slot.
- **`authoring.attempts.settled_nanos` — dropped.** `schema/1940_authoring_attempts.sql:24`
  declares the table without it. The settled charge is `billing.usage_lines.charged_nanos`
  (`schema/2040_billing_usage_lines.sql:42`), summed per work type by
  `rustd/crates/afd_billing/src/ledger/sql/select_owed.sql:1`.

Settled by review on Oct 17, 2026, and checked against the rebuilt slots:

- **`fleet.revisions.runtime_model_ref` — kept.** `schema/1510_fleet_revisions.sql:20`; the
  lease reads it at `rustd/crates/afd_fleet/src/lease/sql/select_fleet_with_session.sql:2`.
- **`model.platform_defaults.base_url` — kept.** `schema/1320_model_platform_defaults.sql:14`;
  read at `rustd/crates/afd_credential/src/provider/sql/select_active_platform_default.sql:4`.
- **`runner.leases.last_metered_at` — kept.** `schema/1810_runner_leases.sql:33`, the lease's
  runtime cursor (rule 4 (d)). Renewal and report meter from it and advance it
  (`rustd/crates/afd_fleet/src/lease/sql/meter_lease.sql:16` and `:35`); the expiry sweep bills
  the last slice from it (`rustd/crates/afd_fleet/src/lease/sql/select_unbilled_expired_leases.sql:5`,
  `rustd/crates/afd_fleet/src/lease/sql/bill_expired_slice.sql:9`). The review listed it as
  dropped; per-request billing (`e4dd5e37a`) kept it as the one runtime cursor and dropped the
  lease's token cursors and the slot's cursors instead.
- **`runner.runners.degraded` — dropped.** Derived on read as `degraded_reason IS NOT NULL`
  (`schema/1800_runner_runners.sql:28`; `rustd/crates/afd_runner/src/capacity/sql/lock_host.sql:1`).
- **`runner.runners.selftest_all_ok` — dropped.** Derived from `selftest_checks`
  (`schema/1800_runner_runners.sql:32`), read at
  `rustd/crates/afd_runner/src/sql/runner_view/runner_detail.sql:14`.
- **`runner.runners.capability_reported_at` — dropped.** `schema/1800_runner_runners.sql:15`
  declares the table with `capability_report` (line 27) and no instant beside it.
- **`authoring.attempts.input_token_count` — dropped.** `schema/1940_authoring_attempts.sql:42`–43
  keep `call_count` and `output_token_count` only; input tokens are counted per request on
  `billing.usage_lines` (`schema/2040_billing_usage_lines.sql:33`).
- **`authoring.dispatches.output_allowance` — dropped.** `schema/1950_authoring_dispatches.sql:14`
  declares the table without it.
- **`setup_context` — replaced** by `fleet.revisions.review_context`
  (`schema/1510_fleet_revisions.sql:22`), written by
  `rustd/crates/afd_authoring/src/creation/sql/revision.sql:3` and read by
  `rustd/crates/afd_authoring/src/creation/sql/locked.sql:14`.

## Public interface changes

- The runner protocol carries no `deletion_generation` (lease payload, activity, model calls, renew, report); a spooled report written before the change still decodes.
  The one development runner restarts on the new release; no fleets exist when it does.
- `agentsfleet tenant provider set` writes through `model.tenant_models`; its output is unchanged.
- Closed-draft retention, the 24-hour retry window and Library-card deletion (history goes with the
  card) are documented in the public docs.

## Review prompt (Codex — Aiwa CTO with Astra)

Paste as-is. This file's prose is a claim to check against source.

```text
Review docs/architecture/postgres_schemas.md (branch feat/m229-postgres-schema-clean-slate): the target Postgres
layout for a from-empty rebuild. Compare every table against today's schema/*.sql and the Rust
that reads and writes it (rustd/crates/**). Be adversarial: find what breaks, not what reads well.

Attack, with file:line evidence for every claim:
1. Readers: for each listed column, name the code that reads it. Flag any column with no reader
   (rule 7) and any dropped column that something still reads.
2. Duplicates: flag any fact stored twice — a column that copies another table's column, a JSON
   field that restates a column, a counter equal to another table's number (rule 5).
3. Placement: flag any table whose owning product area is not its schema (rule 1), and any
   cross-schema foreign key that crosses a stated boundary (runner identity, memory_runtime role).
4. Literals and timestamps: flag any planned index, trigger or default that needs a string literal
   (rule 3), and any time column outside created_at/updated_at that is not a deadline, a NULL
   marker an index reads, or an instant the API returns (rule 4).
5. Deletion: walk draft deletion, fleet purge, account erasure and Library card deletion. Flag
   any cascade that loses money records or leaves private text behind.
6. Names: flag any column breaking the counter and pointer table or the foreign-key rule (rule 2).
7. The "Settled while implementing" list: for each item, say keep or drop from source now.

Output: a table of findings, most severe first — severity (P0/P1/P2), table.column, evidence
(file:line), what fails, the smallest fix. Then a verdict: SOUND, or UNSOUND with the P0s.
No praise, no summary.
```
