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
   Statements write `created_at` and `updated_at` as `now()`; the clock rule for expiry checks is in
   [schema conventions](../../.orly/docs/SCHEMA_CONVENTIONS.md) §"Timestamps". Every table has `created_at`. Every table whose rows change has `updated_at`.
   Any other time column is one of: (a) a deadline, named `*expires_at`; (b) a NULL marker an index
   or trigger reads; (c) a domain instant the API returns that `updated_at` cannot carry; (d) an
   accounting cursor billing reads (`runner.leases.last_metered_at`).
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

| Table | Purpose | Columns |
|---|---|---|
| `tenants` | Billing and ownership root | `id, name, created_at, updated_at` |
| `workspaces` | Isolation unit inside a tenant | `id, tenant_id, name, created_at` · `UNIQUE(id, tenant_id)` for composite references |
| `users` | One person, bound to a sign-in subject; `tenant_id` is the personal account made at signup | `id, tenant_id, oidc_subject, email, display_name, created_at, updated_at` |
| `memberships` | Person ↔ tenant with a role; a role change is delete + insert | `id, tenant_id, user_id, role, created_at` |
| `invites` | Emailed offer to join a tenant | `id, tenant_id, email, role, status, invited_by_user_id, accepted_by_user_id, expires_at, resolved_at, email_status, email_attempt_count, email_sent_at, created_at, updated_at` |
| `api_keys` | Tenant automation tokens, hash only; outlive the admin who made them | `id, tenant_id, name, description, key_hash, created_by_subject, revoked_at, created_at, updated_at` |
| `cli_credentials` | One live CLI login per machine, hash only | `id, user_id, tenant_id, machine_name, credential_hash, credential_prefix, deployment, created_from_address, revoked_at, created_at, updated_at` (metadata columns are in §Settled) |
| `user_preferences` | Small dashboard settings per (user, workspace) | `id, user_id, workspace_id, name, value, created_at, updated_at` |

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

| Table | Purpose | Columns |
|---|---|---|
| `drafts` | One private setup chat | `id, workspace_id, tenant_id, user_id, title, status, tenant_model_id, head_candidate_id, source_type, source_library_revision_id, source_fleet_revision_id, instance_name, fleet_id, library_revision_id, last_message_sequence, cancelled_through_sequence, resource_version, closed_at, expires_at, created_at, updated_at` |
| `messages` | One turn: what was typed, what came back | `id, draft_id, sequence, operation_id, status, tenant_model_id, user_text, answer_text, candidate_id, failure_reason, finished_at, created_at, updated_at` |
| `candidates` | A frozen numbered setup the model proposed | `id, draft_id, revision, definition, canonical_digest, policy_semantics, created_at` · append-only |
| `attempts` | One runner lease running one message, money reserved, model pinned | `id, lease_id, runner_id, workspace_id, tenant_id, user_id, draft_id, message_sequence, fencing_token, model_token_hash, model_snapshot, status, lease_expires_at, expires_at, reserved_nanos, usage_unknown, call_count, output_token_count, reported_runtime_ms, last_activity_sequence, failure_reason, outcome, created_at, updated_at` · `reserved_nanos` is the ceiling its hold reserves; `outcome` is set once usage settles |
| `dispatches` | One physical model call: intent before, result after; `native_reply` is the provider's own reply kept for replay, private like `result` | `id, attempt_id, sequence, request_digest, status, result, usage, native_reply, private_expires_at, created_at, updated_at` |
| `activity_receipts` | Replay proof for "runner posted a setup" | `id, attempt_id, sequence, request_digest, candidate_id, created_at` · append-only |
| `answer_receipts` | Replay proof for "runner posted an answer" | `id, draft_id, attempt_id, report_id, answer_digest, expires_at, created_at` · append-only |
| `replay_receipts` | Every authoring retry key: create draft, send message, publish | `id, workspace_id, user_id, scope, operation_id, request_digest, resource_id, created_at` · append-only, swept at a fixed age |
| `library_entries` | Gallery card; NULL tenant = platform card | `id, tenant_id, visibility, platform_slug, name, description, credential_reasons, head_revision_id, resource_version, created_at, updated_at` |
| `library_revisions` | A frozen published version | `id, library_entry_id, revision, definition, canonical_digest, policy_semantics, created_at` · append-only |

- **Model pointers.** `drafts.tenant_model_id` and `messages.tenant_model_id` are NULL for the
  platform default and otherwise reference a `model.tenant_models` row of the same tenant. The wire
  keeps today's string: `platform-default` maps to NULL, a tenant model id maps to the row.
- **Source and publication are separate facts.** `source_type` (blank, definition, library, fleet)
  plus `source_library_revision_id` or `source_fleet_revision_id` record where the chat started;
  `library_revision_id` records what it published. A draft can start from Library revision A and
  publish revision B. All three references are `ON DELETE SET NULL`, so deleting a card or purging
  a fleet never blocks on, or deletes, someone's private draft.
- **Retention.** Closing a draft stamps `closed_at` and `expires_at = closed_at + 30 days` (an API
  field); the sweeper deletes past `expires_at`. The sweep cascades messages, candidates and answer
  receipts and keeps attempts, dispatches (result scrubbed) and settlements.
- **Retry window.** A replay receipt answers its key for 24 hours from `created_at`
  (`afd_authoring::replay::REPLAY_WINDOW`). Inside the window the same key and body return the first
  result, a different body or user is refused, and a key whose resource was deleted returns Gone.
  At the boundary and after it the key is new: the next claim deletes the expired receipt and files
  its own, and `Drafts::maintain` sweeps the rest in bounded `SKIP LOCKED` batches. The draft
  message API returns `operation_id` from `messages.operation_id`, written when the turn is
  accepted, because the turn outlives its receipt.
- **Running totals.** `attempts.call_count` and `output_token_count` are aggregates maintained under
  the attempt lock and read by admission and renewal (rule 5 exception); the input total had no reader.
- The setup JSON is copied onto each frozen revision table on purpose: a shared definitions table
  would turn draft deletion from a cascade into a reference-counted sweep.

## fleet

```
 identity.workspaces ◀── fleets ──▶ revisions (head_revision_id)
 fleets ◀── revisions, schedules, sessions (1:1), activity_counters (1:1),
            events ◀── tool_calls, admissions, approval_gates ──▶ connector.integration_grants,
            obligations
```

| Table | Purpose | Columns |
|---|---|---|
| `fleets` | The deployed agent | `id, workspace_id, tenant_id, name, status, head_revision_id, required_tags, setup_stream_ready, initial_start, memory_reads_workspace, memory_publishes_workspace, resource_version, created_at, updated_at` |
| `revisions` | Frozen setup history; rev 1 at Save, +1 per Apply | `id, fleet_id, revision, definition, canonical_digest, policy_semantics, runtime_model_ref, review_digest, review_context, source_candidate_id, created_at` · append-only |
| `schedules` | Cron or one-shot runs, synced to the external scheduler | `id, fleet_id, source, provider_schedule_id, cron_expression, timezone, message, once, fire_at, recipe, desired_status, sync_status, resource_version, sync_token, sync_lease_expires_at, last_error, created_at, updated_at` |
| `sessions` | Resume bookmark `{last_event_id, last_response}` | `fleet_id (PK), context_json, created_at, updated_at` |
| `events` | One row per delivery into the fleet | `id, fleet_id, workspace_id, event_id, actor, event_type, status, request_json, response_text, tool_calls, token_count, wall_ms, failure_reason, failure_detail, resumes_event_id, created_at, updated_at` · `UNIQUE(fleet_id, event_id)` |
| `admissions` | Work ledger; retries cannot duplicate work | `id, sequence, fleet_id, workspace_id, producer, producer_key, payload_digest, actor, event_type, request_json, event_created_at, reply_provider, reply_address, receipt, delivered_at, replay_count, replay_claim_expires_at, created_at, updated_at` |
| `approval_gates` | A risky action waiting for a person | `id, fleet_id, workspace_id, event_id, active_grant_id, action_id, tool_name, action_name, gate_type, proposed_action, evidence, stated_binding, blast_radius, spend_count, spend_ceiling, expires_at, status, resolved_at, resolved_by, resolved_by_name, detail, created_at, updated_at` |
| `obligations` | An answer owed to a destination until delivered or abandoned | `id, sequence, fleet_id, workspace_id, provider, destination, event_id, answer, receipt, delivered_at, attempt_count, abandoned_at, abandon_reason, created_at, updated_at` |
| `tool_calls` | Full arguments and output of each tool call | `id, workspace_id, fleet_id, fleet_event_id, fencing_token, sequence, arguments, truncated_arguments, output, output_line_count, truncated, byte_count, created_at, updated_at` |
| `activity_counters` | Trigger-maintained dashboard totals | `fleet_id (PK), event_count, budget_used_nanos, created_at, updated_at` |

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

| Table | Purpose | Columns |
|---|---|---|
| `runners` | An enrolled host: policy, health, self-test | `id, host_id, token_hash, sandbox_tier, status, labels, network_policy, registry_allowlist, worker_count, sandbox_config, capability_report, degraded_reason, last_seen_at, selftest_requested_at, selftest_completed_at, selftest_checks, selftest_sandbox_tier, selftest_network_policy, lease_version, last_lease_type, created_at, updated_at` |
| `leases` | One fleet event handed to one runner. No foreign key to the fleet or its event: a settled lease outlives a purge as runner history; the insert proves fleet, workspace and tenant in the same statement | `id, runner_id, fleet_id, workspace_id, tenant_id, fleet_event_id, event_created_at, receipt, posture, provider, model, last_metered_at, fencing_token, lease_expires_at, status, posted_message_count, created_at, updated_at` |
| `fleet_slots` | Per-fleet claim: who may lease it, next fencing token, sticky runner, sandbox hold | `fleet_id (PK), last_runner_id, fencing_token, lease_expires_at, hold_expires_at, created_at, updated_at` |
| `events` | Append-only runner history | `id, runner_id, event_type, metadata, last_seen_at, created_at` · unique `(runner_id, last_seen_at) WHERE last_seen_at IS NOT NULL`: an offline event stamps the heartbeat it observed, so one stale episode is one row |
| `lifetime_counters` | Per-runner tallies | `runner_id (PK), acquired_count, succeeded_count, failed_count, expired_count, created_at, updated_at` |

`sandbox_config` holds tier-specific knobs as JSON validated in the application:
`{"extra_binds": [...]}` for namespace tiers today; microVM keys arrive with the tier that reads them.
Degraded and self-test pass are derived on read (`degraded_reason IS NOT NULL`, every check passed),
keeping "no self-test yet" distinct from "self-test failed". `leases.event_created_at` is the
producer's instant billing uses, which the event row does not hold; the lease's `actor` and
`event_type` come from a join to `fleet.events`, as reclaim already does.

## connector

| Table | Purpose | Columns |
|---|---|---|
| `installs` | Outside account (Slack team, GitHub installation) → workspace; credentials stay in the vault | `id, provider, external_account_id, workspace_id, scopes, created_at, updated_at` |
| `channels` | Slack channel → resident fleet; rebinds when a team moves workspace | `id, provider, external_account_id, external_channel_id, fleet_id, binding_type, created_at, updated_at` |
| `integration_grants` | Whether a fleet may use a service; deny is stored as revoked | `id, fleet_id, service, status, requested_reason, approved_at, revoked_at, created_at, updated_at` |

## model

| Table | Purpose | Columns |
|---|---|---|
| `platform_models` | Curated identities with exact provider API identifiers, display names and context evidence | `id, model_name, provider, display_name, context_cap_tokens, context_source_url, context_verified_at, advanced, inference, created_at, updated_at` |
| `platform_model_prices` | Prices per prompt-size tier and effective date range, in integer nanos per million tokens; a separate table because one model has many prices | `id, platform_model_id, effective_from, effective_until, prompt_tokens_above, input_nanos_per_mtok, cached_input_nanos_per_mtok, cache_write_nanos_per_mtok, cache_write_5m_nanos_per_mtok, cache_write_1h_nanos_per_mtok, output_nanos_per_mtok, source_url, verified_at, conditions, created_at, updated_at` |
| `catalogue_version` | Singleton edit counter, bumped by every catalogue edit and read with each runtime model resolution | `id (=1), resource_version, created_at, updated_at` |
| `platform_defaults` | Default model per provider and the admin workspace holding its key | `provider (PK), source_workspace_id, active, platform_model_id, base_url, created_at, updated_at` |
| `tenant_models` | A tenant's own models, each pointing at a vault key | `id, tenant_id, model_name, secret_ref, created_at, updated_at` |
| `tenant_selections` | The tenant's default; NULL means platform default | `tenant_id (PK), tenant_model_id, created_at, updated_at` |

`model_name` is the provider's name for a model and never a foreign key. `installs.scopes` is read
by the setup review, which blocks a connection whose granted scopes fall short. `platform_defaults.base_url`
is the custom endpoint the platform default calls. An `inference` column (endpoint, dialect, output
limit) arrives only with the change that reads it.

## billing

| Table | Purpose | Columns |
|---|---|---|
| `wallets` | The tenant's balance; the only row money moves on | `tenant_id (PK), balance_nanos, exhausted_at, created_at, updated_at` · `CHECK (balance_nanos >= 0)` |
| `authoring_report_receipts` | Acknowledges an interim report without settling | `id, attempt_id, report_id, accounting_digest, acknowledgement, created_at` · append-only |
| `usage_lines` | One priced provider request or runtime slice of billed work; what the charges page, fleet budgets and the fleet list's `budget_used_nanos` trigger read | `id, work_type, work_id, seq, tenant_id, workspace_id, fleet_id, fleet_name, event_id, authoring_attempt_id, provider, model, posture, requested_at, input_token_count, cached_input_token_count, cache_write_token_count, cache_write_5m_token_count, cache_write_1h_token_count, output_token_count, runtime_ms, platform_model_price_id, unpriced_token_count, charged_nanos, settled_at, created_at, updated_at` · `UNIQUE (work_type, work_id, seq)` · fleet runs and setup chat both write it (billing §11) |
| `holds` | The built-in biller's reservation for one piece of work | `id, work_type, work_id, tenant_id, ceiling_nanos, captured_nanos, released_at, created_at, updated_at` · `UNIQUE (work_type, work_id)` |

Money outlives every deletion: `usage_lines` carries opaque `tenant_id`, `workspace_id`,
`fleet_id` and `authoring_attempt_id` values with no foreign key, and `authoring.attempts` keeps
its workspace, tenant and user identifiers without a foreign key. Erasing a fleet, workspace
or account therefore never deletes a charge. The slots name the export that reads them (rule 7).

## Deletion matrix

| Root deleted | Goes with it | Stays | Scrubbed immediately |
|---|---|---|---|
| Draft (user delete or 30-day sweep) | messages, candidates, answer receipts | attempts, dispatches, settlements, receipts | `dispatches.result` of its attempts |
| User | their drafts (as above), memberships, CLI credentials, preferences | api keys they created, attempts, settlements | `dispatches.result` of their attempts |
| Library card | its revisions | fleets installed from it; drafts started from it (`source_library_revision_id` set NULL) | — |
| Fleet (purge) | revisions, sessions, events, tool calls, admissions, gates, obligations, schedules, grants, channels, memory, slot, live (`active`) runner leases | charges; settled runner leases, for their own 30-day window (fleet and event shown as deleted); drafts linked to it (`fleet_id`, `source_fleet_revision_id` set NULL) | — |
| Workspace | drafts, fleets (as above), installs, secrets, preferences | attempts, settlements, charges | `dispatches.result` of its attempts |
| Tenant (account erasure) | workspaces (as above), users, wallet, tenant models, selection, invites, api keys | attempts, settlements, charges (opaque identifiers only) | `dispatches.result` of its attempts |
| Runner (record retired after revoke) | leases, history, lifetime counters | attempts (`runner_id` set NULL), settlements; slots (`last_runner_id` set NULL) | — |

## vault and memory

| Table | Purpose | Columns |
|---|---|---|
| `vault.secrets` | One encrypted secret per (workspace, key_name), with a plain projection | `id, workspace_id, key_name, kek_version, dek_ciphertext, dek_nonce, dek_tag, secret_nonce, secret_ciphertext, secret_tag, meta_kind, meta_provider, meta_base_url, meta_has_key, created_at, updated_at` |
| `memory.entries` | A fleet's durable memory, behind role `memory_runtime` | `id, fleet_id, workspace_id, key, content, category, workspace_visible, created_at, updated_at` |

## Settled while implementing

Each is decided from source during M229 and reported back; a column whose reader cannot be found
is dropped.

- `identity.api_keys.description`, `identity.cli_credentials.credential_prefix`, `deployment`,
  `created_from_address`: no stored-value reader found in review; kept only if an API response or
  display reads them.
- `identity.cli_credentials.tenant_id`: CLI authentication reads the user's tenant; dropped unless a
  reader needs the credential's own copy.
- `authoring.attempts.settled_nanos`: dropped unless a reader distinct from
  `billing.usage_lines.charged_nanos` is found.

Settled by review on Oct 17, 2026: `runtime_model_ref` and `base_url` kept; `degraded`,
`selftest_all_ok`, `capability_reported_at`, `leases.last_metered_at`, `attempts.input_token_count`,
`dispatches.output_allowance` dropped; `setup_context` replaced by `revisions.review_context`.

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
