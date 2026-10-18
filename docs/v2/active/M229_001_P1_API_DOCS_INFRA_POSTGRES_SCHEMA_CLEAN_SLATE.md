<!--
SPEC AUTHORING RULES (load-bearing — the one comment that survives):
- Body order = the executing agent's read order. Fill via the orly-spec-new
  skill (authoring order lives there); after filling, DELETE every "tpl:"
  guidance comment — the SPEC TEMPLATE GATE blocks tpl residue, unfilled
  {slots}, and missing required sections (.orly/audits/spec-template.sh --staged).
- No time/effort/hour/day estimates anywhere. No effort columns, complexity
  ratings, percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only
  sequencing signal. A section that contradicts these rules loses — delete it.
-->

# M229_001: Postgres rebuilds from empty into nine owned schemas with one naming rule set

**Prototype:** v2.0.0
**Milestone:** M229
**Workstream:** 001
**Date:** Oct 17, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — the rebuild is only cheap before the first production launch (M224_001) puts data in Postgres
**Categories:** API, DOCS, INFRA
**Batch:** B1 — single workstream; the schema, every reader and the docs land in one Pull Request (PR), as the owner chose
**Branch:** feat/m229-postgres-schema-clean-slate
**Baseline revision:** bb164c8557d1060b92975595256b4d2ae9d2237a
**Test Baseline:** pending — measure declared unit and integration lanes before the Pull Request
**Baseline evidence:** pending — report path or run URL with revision, commands, passed/failed/skipped counts, and environment
**Depends on:** none blocking. Ordering: lands before M224_001 puts production data in Postgres; the `fix/754-dev-acceptance-app` branch's slots 956/957 fold into the new slots if it has not merged first
**Provenance:** LLM-drafted (Claude Opus 5.5, Oct 17, 2026), from a table-by-table owner review against `main` at `a8646bb2a`, amended after an adversarial review the same day
**Canonical architecture:** `docs/architecture/postgres_schemas.md` §Rules, §Layout, one section per schema, §Deletion matrix

---

## Overview

**Goal (testable):** `test_migrate_from_empty_builds_nine_owned_schemas` — `agentsfleetd migrate` on an empty database creates exactly the tables in `docs/architecture/postgres_schemas.md`, no schema object carries a vocabulary literal, and the full integration lane passes against it.
**Problem:** 33 of 47 tables sit in `core`; the setup-chat tables span `core`, `fleet` and `billing`. Columns exist with no reader (`drafts.creation_digest`, `wallets.grant_source`), facts are stored twice or three times (`fleets.config_json`, `fleets.setup_version`), account erasure deletes billing records, and an owner reading the schema cannot tell what a field means or who fills it.
**Solution summary:** delete all 95 `schema/*.sql` slots and author one slot per table across nine schemas, exactly as signed off in the canonical doc. Every Rust query, struct and test follows the new names. The runner protocol loses one field. Money records survive every deletion and private dispatch text is scrubbed on every erasure path. The development and production databases are dropped and rebuilt from empty by a guarded one-shot procedure. The naming rules land in the schema conventions at their source in the orly repository. Three data-access fixes ride the same rewrite: typed row reads replace whole-row JSON, the Library list becomes one query, and every pooled connection carries statement and lock time limits.

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(schema): rebuild Postgres into nine owned schemas
- **Intent (one sentence):** every table has one obvious home, every column has a reader and one meaning, money outlives deletion, and the conventions stop the drift recurring.
- **Handshake** — pending until the implementing agent performs PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `docs/architecture/postgres_schemas.md` — the signed-off layout, every table's columns, the rules, the counter names and the deletion matrix; the slots are a transcription of it.
2. `docs/v2/done/M154_001_P1_API_DOCS_SQL_UI_SCHEMA_REBUILD_FOR_SCALE.md` — the previous from-empty rebuild: registry and test mechanics.
3. `.orly/dispatch/write_sql.md` — SCHEMA GUARD, RULE STS, NSQ, SGR and ITF; the guard fires on every slot edit here.
4. `rustd/crates/afd_db/src/migrate/lock.rs` — the bounded advisory lock that survives the move to SQLx's `Migrator`; the rest of `migrate/` is replaced.
5. `rustd/crates/afd_authoring/src/privacy.rs` — the draft-purge scrub every erasure path must now reach.

## Files Changed (blast radius)

Discovery grep (rerun at PLAN): `git grep -lE "(^|[^a-z_])(core|fleet|billing|vault|memory)\.[a-z_]+"` — 832 tracked files at `a8646bb2a`; it over-matches (`rig-core.workspace`), so acceptance uses the exact removed-name inventory in the rubric instead.

| File | Action | Why |
|------|--------|-----|
| `schema/*.sql` (95 slots) | DELETE | Clean slate; slot history is not migrated |
| `schema/*.sql` (one per table) | CREATE | The signed-off design, ordered per table by foreign key |
| `rustd/crates/afd_db/src/migration.rs`, `rustd/crates/afd_db/src/migrate/**` | EDIT | SQLx `Migrator` over the explicit slot list; bounded lock kept; hand-rolled ledger deleted |
| `rustd/crates/*/src/**/sql/*.sql` | CREATE | Every static runtime statement as a file beside its use case |
| `rustd/crates/afd_db/src/pool/**` | EDIT | Statement and lock time limits on every pooled connection; migrate exempt |
| `rustd/crates/*/src/**/*.rs` with a schema-qualified table name | EDIT | Queries and row structs follow new names; dropped columns lose their writers |
| `rustd/crates/afd_authoring/src/privacy.rs`, `rustd/crates/afd_tenant/src/**` erasure paths | EDIT | Every deletion root scrubs dispatch text; money is not cascaded |
| `rustd/crates/afd_wire/src/authoring/**`, `rustd/crates/afr_supervisor/src/**` | EDIT | `deletion_generation` leaves the runner protocol and its report spool |
| `rustd/crates/*/tests/**`, `rustd/crates/afd_db/src/test_util/**`, `rustd/crates/afd_bench/**` | EDIT | Fixtures and assertions use the new schema |
| `rustd/crates/afd_memory/src/{flip,flip_fixture,flip_tests,flip_prune_tests,flip_race_tests,in_memory,in_memory_tests,route,store}.rs`, `src/sql/{import_entry,delete_stale_entry,select_workspace_entries}.sql`, `tests/{integration_flip,integration_prune,support/paused}.rs` | DELETE | The memory flip, its route table, the `MemoryStore` trait and the in-memory store had no production caller; `Memories` holds `PgStore` directly (M-DI-HIERARCHY) |
| `rustd/Cargo.toml` | EDIT | SQLx `migrate` feature on, `macros` stays off; comment naming `core.fleet_events` |
| `public/openapi.json` | EDIT | Regenerated for the runner protocol removal |
| `docker-compose.yml`, `compose.dev-infra.yml` | EDIT | Local and test lanes run `postgres:19rc1-alpine` and `dragonfly:v2.0.2` (Discovery: Postgres 19 lanes, Dragonfly 2.0.2) |
| `deploy/fly/dragonfly-{dev,prod}/Dockerfile` | EDIT | `dragonfly:v2.0.2` (Discovery: Dragonfly 2.0.2) |
| `cli/src/**`, `ui/packages/app/**`, `scripts/**`, `playbooks/**`, `make/*.mk` | EDIT | Only where the inventory check finds a removed name |
| `docs/AUTH.md`, `docs/AUTH_DEVICE_LOGIN.md`, `docs/architecture/*.md` | EDIT | Old table names replaced; `postgres_schemas.md` §Settled outcomes |
| `docs/v2/pending/*.md`, `docs/v2/active/*.md` | EDIT | Live specs naming a removed table; `done/` stays frozen |
| `.orly/docs/SCHEMA_CONVENTIONS.md`, `.orly/AGENTS.md` | EDIT | Only through `orly update` after the source change (the latter mandates `core.fleet_*`) |
| `~/Projects/orly/**` (conventions doc and the pack carrying `core.fleet_*`) | EDIT | Cross-repository, own branch and PR (owner-approved) |
| `~/Projects/docs/**` | EDIT | Runner protocol, retention, retry window, Library deletion, changelog `<Update>`; own branch |

## Applicable Rules

- **`.orly/docs/greptile-learnings/RULES.md`** — STS (no vocabulary literals in schema objects), NSQ, SGR (every slot grants), ITF (integration tests on the real schema), NDC (dropped columns lose every writer), NLR, NLG (no alias for an old table or column name), UFS, ORP (orphan sweep across Rust, TypeScript, docs), VLT, KYS.
- `.orly/docs/SCHEMA_CONVENTIONS.md` — identity, timestamps and standard columns, as amended by §1.
- `docs/RUST_ERROR_STANDARD.md` — any new refusal is an `ErrorKind` arm, never a hand-written error type.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| SCHEMA GUARD | yes — every slot deletion, `migration.rs` edit; `VERSION` is 0.62.0 | Owner records `SCHEMA GUARD: SKIPPED per user override (reason: …)` before the first slot deletion; guard output lists every removed and created slot |
| File & Function Length (≤350/≤50/≤70) | yes | One table per slot; split any Rust file a rename pushes over the cap |
| UFS | yes | Status, type and scope values stay named constants bound as parameters |
| MILESTONE-ID | yes | No `M229` in source, test names or slot comments |
| GREPTILE | yes | End-of-turn read of the rules named above |
| LOGGING | yes, where a renamed query logs its table | Event names keep their meaning; only table text changes |

## Prior-Art / Reference Implementations

- **Reference:** `docs/v2/done/M154_001_P1_API_DOCS_SQL_UI_SCHEMA_REBUILD_FOR_SCALE.md` — same from-empty posture; this rebuild diverges by ordering slots per table, moving to SQLx's `Migrator`, and requiring an owner override because `VERSION` is past 0.30.0.

## Sections (implementation slices)

### §1 — Conventions land at their source — DONE

The rules, the counter table and the deletion-matrix requirement land in `~/Projects/orly` on their own branch and PR: the conventions doc (dropping its stale Zig timestamp and `schema/embed.zig` lines) and the pack text that mandates `core.fleet_*`. After it merges, `orly update --no-hooks` refreshes the generated copies here.

- **Dimension 1.1** — DONE — generated conventions carry the rules and counter table, and no Zig, `embed.zig` or `core.fleet_*` text → Test `test_conventions_carry_naming_rules`
- **Dimension 1.2** — DONE — `orly doctor` reports no drift after the update → Test `test_orly_doctor_clean_after_update`
- **Evidence** — orly PR #65 "docs(conventions): agentsfleet schema naming rules…" merged 2026-10-17 (`gh pr list` in `~/Projects/orly`); `.orly/docs/SCHEMA_CONVENTIONS.md:44` "Counter and pointer names"; `grep -rln 'embed.zig\|core\.fleet_' .orly/` → 0 files; `orly doctor` → "🟢 installed rules match .orly/orly.json".

### §2 — Slots rebuilt from empty — DONE

All 95 slots go (an earlier count of 97 was wrong; `git ls-files 'schema/*.sql'` listed 95). 52 replace them: two substrate slots, 47 tables and three cycle closers. SQLx's `Migrator` applies the new ones from the explicit list (`with_migrations`; `migrate` feature only), with checksums, its ledger in `audit`, and our bounded advisory lock around it (`set_locking(false)`). Every database starts empty (§5), so migrate carries no knowledge of the old ledger. Slot files are named `NNNN_<schema>_<table>.sql`, numbered from 1000. One slot per table, ordered by foreign key (a table follows everything it references, across schemas). Each cycle — `fleets.head_revision_id`, `drafts.head_candidate_id`, `library_entries.head_revision_id`, `approval_gates.active_grant_id` if `integration_grants` follows `fleets` — gets its foreign key in its own registered slot after both tables exist, `DEFERRABLE INITIALLY DEFERRED`. **Implementation default:** `fleet.revisions.source_candidate_id` carries no foreign key, because candidates are swept after 30 days.

- **Dimension 2.1** — DONE — migrate from empty yields exactly the documented tables and columns → Test `test_migrate_from_empty_builds_nine_owned_schemas`
- **Dimension 2.2** — DONE — no index predicate, trigger, CHECK or DEFAULT carries a vocabulary literal; structural literals match the allowed list → Test `test_schema_objects_carry_no_vocabulary_literals`
- **Dimension 2.3** — DONE — mutable tables carry `updated_at`; immutable tables lack it, lack UPDATE grants and refuse UPDATE, and cascades still delete them → Test `test_immutable_tables_refuse_update`
- **Dimension 2.4** — DONE — no table repeats its schema name; every foreign-key column ends `_id` and names its table or a stated role → Test `test_table_and_key_names_follow_rules`
- **Dimension 2.5** — DONE — an applied slot whose file changed is refused by checksum, naming the version → Test `test_migrate_refuses_edited_slot`
- **Dimension 2.6** — DONE — every instant is `TIMESTAMPTZ(3)` and every instant column ends `_at`, both ways round; statements stamp lifecycle columns with `now()` → Test `test_instant_columns_are_timestamptz_ms`
- **Evidence** — (inner loop: `cargo test -p afd_db --all-features --test db_suite -- --ignored integration_migrate --skip statements` against an empty Postgres 18: 19 passed, 0 failed; the repository lane goes green with §3). Every product table's grants are probed role by role in `test_every_table_grants_exactly_its_role`.

### §3 — Every reader and writer follows the new names

Every query, row struct and fixture moves to the new tables and columns. Each item in the canonical doc's §Settled list is resolved from source: a cited reader keeps it, otherwise it is dropped, and the outcome is written into that section.

- **Dimension 3.1** — the full integration lane passes on the rebuilt schema → Test `test_integration_lane_on_rebuilt_schema`
- **Dimension 3.2** — every §Settled item carries a recorded outcome with a citation → Test `test_settled_columns_have_outcomes`
- **Dimension 3.3** — GitHub App JWT minting, the installation read, the scoped token mint and the `pull_request` and `workflow_run` webhook reads run on `reqwest` and `jsonwebtoken` 11; `octocrab` is absent from `Cargo.lock` → Test `test_github_app_without_octocrab`
- **Dimension 3.4** — no converted crate holds a span or instant as integer milliseconds outside a protocol edge (Redis `PX`, Lua `ARGV`, a `BIGINT` span column, a wire span field), each edge converting in one place → Test `test_no_integer_millis_outside_edges`

### §4 — Behaviour that changes with the schema

- **Dimension 4.1** — each deletion root in the matrix keeps attempts, settlements and charges and scrubs `dispatches.result` at once; an open draft and an installing fleet survive their sweeps → Test `test_deletion_matrix_keeps_money`
- **Dimension 4.2** — inside 24 hours from `created_at` the same key and body replay, a changed body or a different user is refused, and a deleted resource answers Gone; at the boundary and after it the key is new → Test `test_replay_receipt_window`
- **Dimension 4.3** — deleting a Library card deletes its revisions; a fleet installed from it keeps its setup; drafts started from or published to it keep existing with the reference NULL; another tenant's card cannot be deleted → Test `test_library_delete_cascades_revisions`
- **Dimension 4.4** — `tenant provider set` writes one `tenant_models` row and points the selection at it; reset stores NULL; a pointer to another tenant's model is refused → Test `test_tenant_selection_points_at_model`
- **Dimension 4.5** — runner protocol bodies carry no `deletion_generation`; the runner and its report spool decode without it; the draft message response still carries `operation_id`, joined from the receipt → Test `test_runner_wire_drops_deletion_generation`
- **Dimension 4.6** — a resolved approval gate or invite cannot be resolved again; the pending indexes read `resolved_at` → Test `test_resolution_happens_once`
- **Dimension 4.7** — the API, the CLI and the UI send and read every instant as an RFC 3339 string in UTC through jiff's standard serialization (`…:49Z`, `…:49.123Z`); clients order instants by parsed value, never by text; the device-flow window is `expires_at` → Test `test_api_instants_are_rfc3339`
- **Dimension 4.8** — every locking validity check (lease renew, report fence, draft and schedule reads under lock) refuses a row that expired while it waited on the lock → Test `test_expiry_checks_read_clock_after_lock`
- **Dimension 4.9** — an admission event's `event_id` is its `fleet.admissions` row `id` (UUIDv7, minted in Rust before the insert); delivery, reply and inherit lookups key on it, and the history keyset `(created_at, event_id)` stays newest-first → Test `test_admission_event_id_is_row_id`

### §6 — Docs

- **Dimension 6.1** — no doc outside `docs/v2/done/` names a removed table → Test `test_docs_name_no_removed_table`
- **Dimension 6.2** — public pages for the runner protocol, closed-draft retention, the retry window and Library deletion, plus a changelog `<Update>`, land on their own docs branch → Test `test_public_docs_branch_open` (manual)

### §7 — Data access rides the rewrite

Every query is rewritten here, so three fixes land with it rather than touching each query twice. The 22 whole-row `to_jsonb(<alias>)::text` reads in 16 files (count at `a8646bb2a`) become explicit column lists into typed rows; `Json<T>` stays for real JSON columns. The Library list joins each card to its head revision in one statement. Each pooled connection sets `lock_timeout` and `statement_timeout`; the migrate connection sets neither. **Implementation default:** `lock_timeout` 2 s and `statement_timeout` 15 s as named constants in `afd_db`, because 15 s is the whole setup budget (`SETUP_BUDGET`, `afd_authoring/src/creation/activate.rs:16`) and a lock wait must fail well inside it.

- **Dimension 7.1** — no whole-row JSON read remains in runtime source → Test `test_no_whole_row_json_reads`
- **Dimension 7.2** — a Library page is one statement at any page size and stays coherent when a card is deleted mid-read → Test `test_library_list_single_statement`
- **Dimension 7.4** — every static statement is a `.sql` file beside its use case, loaded with `include_str!`, and each one `PREPARE`s against the rebuilt schema; joined-string statements become whole files where possible and the rest are named and tested → Test `test_every_sql_file_prepares`
- **Dimension 7.3** — a request waiting on a held wallet lock fails within the lock limit with a retryable error, and the pool serves the next request → Test `test_lock_wait_is_bounded`
- **Dimension 7.5** — every multi-column read decodes into a `#[derive(sqlx::FromRow)]` struct through `query_as`, instants through `#[sqlx(try_from = "jiff_sqlx::Timestamp")]`, single columns through `query_scalar`; no `try_get` remains (373 across 13 crates at `331c0afc8`) → Test `test_rows_decode_into_typed_structs`

### §5 — Rebuilt from empty, proven locally, migrated on merge

Runs after §6 and §7. The proof is local: the Docker lane's Postgres and Dragonfly are torn down to nothing, `agentsfleetd migrate` builds the schema from empty, and the full integration lane and a daemon-plus-runner smoke run pass against it. No hand procedure touches a deployed database. Development is torn down by the owner, and when the PR merges with this spec in `done/`, the release command migrates it from empty like any deploy.

- **Dimension 5.1** — a torn-down local Docker lane migrates from empty and passes `make test-integration-rustd` and a daemon-plus-runner smoke run → Test `test_local_lane_rebuilt_from_empty` (manual, output in Session Notes)

## Interfaces

```
Runner protocol (POST /v1/runners/me/authoring-leases/{lease_id}/{activity|model-calls|renew|reports},
  lease payload): field `deletion_generation` removed. One development runner restarts on the release.
Draft messages: unchanged on the wire; `operation_id` is stored on authoring.messages (Discovery: Message operation key).
Drafts: `closed_at` and `expires_at` unchanged on the wire.
CLI: `agentsfleet tenant provider set` unchanged from outside; server writes model.tenant_models.
Tables and columns: docs/architecture/postgres_schemas.md is the interface; renames there are final.
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Edited slot | An applied slot's file changes after release | SQLx checksum mismatch; migrate refuses, naming the version |
| Old name survives | A query still names a removed table or column | Integration lane fails on the undefined relation; the rubric's inventory check reports it |
| Cycle order | Head pointer written before its target in one transaction | Deferred foreign key checks at commit; an orphan pointer rolls back |
| Wrong environment | A rebuild pointed at a database with real users | No step drops a deployed database: §5 tears down only the local lane, and deployed databases migrate forward on merge |
| Production holds data | Rebuild attempted after M224_001 launched | Procedure retired; owner decides a forward migration |
| Erasure loses money | A deletion root cascades into attempts, settlements or charges | No foreign key from money rows to an erasable owner; `test_deletion_matrix_keeps_money` fails |
| Late 754 merge | Slots 956/957 land on `main` after this branch opens | Rebase folds them into the new slots; `rust-migration-list` fails until folded |
| Concurrent resolution | Two people resolve one gate or invite | Guarded UPDATE on `resolved_at IS NULL`; second sees zero rows and a conflict |
| Lock held too long | A wallet or fleet row stays locked past the lock limit | The waiting statement fails; the caller gets a retryable dependency error; the connection returns to the pool |
| Migrate hits a limit | Long DDL on the migrate connection | Migrate's connection carries no limits, so it is never cut off mid-slot |

## Invariants

1. No schema object carries a vocabulary literal — catalogue scan in `test_schema_objects_carry_no_vocabulary_literals`.
2. Immutable tables cannot be updated — no UPDATE grant plus a refusing trigger; `test_immutable_tables_refuse_update`.
3. The migration list equals the tracked slots — `rust-migration-list` in `make lint-all`.
4. No applied slot changes silently — SQLx checksums; `test_migrate_refuses_edited_slot`.
7. Every static statement prepares against the schema — `test_every_sql_file_prepares` in the integration lane.
5. No money row has a foreign key to an erasable owner — catalogue assertion in `test_deletion_matrix_keeps_money`.
6. Every pooled runtime connection carries both time limits — `SHOW lock_timeout` on a pooled connection in `test_lock_wait_is_bounded`.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| not applicable — no product/operator signal changes | not applicable | not applicable | not applicable | not applicable | `test_integration_lane_on_rebuilt_schema` |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_conventions_carry_naming_rules` | generated conventions → contains the counter table heading; 0 matches for `embed.zig` and `core.fleet_*` |
| 1.2 | manual | `test_orly_doctor_clean_after_update` | `orly doctor` after update → no drift line; owner merges the orly PR |
| 2.1 | integration | `test_migrate_from_empty_builds_nine_owned_schemas` | empty database → table and column set equals the canonical doc, nine schemas plus `audit` |
| 2.2 | integration | `test_schema_objects_carry_no_vocabulary_literals` | catalogue scan → only `'7'`, `TG_OP` values and unmatched exception text; a seeded `'pending'` predicate or `current_setting()` name fails the scan |
| 2.3 | integration | `test_immutable_tables_refuse_update` | UPDATE on each append-only table → refused; parent DELETE still cascades; mutable tables all have `updated_at` |
| 2.4 | integration | `test_table_and_key_names_follow_rules` | catalogue → no schema-prefixed table; foreign-key columns end `_id` and match a table or a listed role |
| 2.5 | integration | `test_migrate_refuses_edited_slot` | applied slot text changed → checksum refusal naming the version |
| 2.6 | integration | `test_instant_columns_are_timestamptz_ms` | every `_at` column is `timestamp with time zone` at precision 3, and no other column is |
| 3.1 | integration | `test_integration_lane_on_rebuilt_schema` | `make test-integration-rustd` on the rebuilt schema → exit 0 |
| 3.2 | unit | `test_settled_columns_have_outcomes` | each §Settled bullet → kept or dropped plus a citation |
| 3.3 | unit | `test_github_app_without_octocrab` | `cargo tree -i octocrab` → not found; an App JWT minted with `jsonwebtoken` 11 → RS256 header, `iss` the App id, `exp` ≤ 10 minutes after `iat`; a `pull_request` and a `workflow_run` payload → decode into the fields the handlers read |
| 3.4 | unit | `test_no_integer_millis_outside_edges` | per-crate scan of every converted crate → 0 integer-millisecond spans, instants or `*_MS` constants outside the listed edges (Redis `PX`, Lua `ARGV`, a `BIGINT` span column, a wire span field, the cursor token, the UUIDv7 layout, JWT NumericDate); a seeded `const X_MS: i64` fails the scan naming its file |
| 4.1 | integration | `test_deletion_matrix_keeps_money` | delete each root → money rows remain with opaque ids, `dispatches.result` NULL; open draft and installing fleet untouched |
| 4.2 | integration | `test_replay_receipt_window` | same key at 23h59m → replay; changed body → refused; other user → refused; deleted resource → Gone; at 24h → new |
| 4.3 | integration | `test_library_delete_cascades_revisions` | card A→draft→B, installed fleet → card and revisions gone; fleet intact; draft references NULL; foreign tenant → refused |
| 4.4 | integration | `test_tenant_selection_points_at_model` | provider set → one row, selection points at it; reset → NULL row; another tenant's model id → refused |
| 4.5 | unit | `test_runner_wire_drops_deletion_generation` | lease payload and report spool entry without the field → decode; message response → has `operation_id` |
| 4.6 | integration | `test_resolution_happens_once` | two concurrent resolutions → one success, one conflict; second pending invite for the same address → refused |
| 4.7 | unit | `test_api_instants_are_rfc3339` | every published `_at` field → `type: string, format: date-time`; a whole-second response instant → `…:49Z`, a millisecond one → `…:49.123Z`; the CLI and UI parse both and order `…:49Z` before `…:49.123Z` |
| 4.8 | integration | `test_expiry_checks_read_clock_after_lock` | per locking shape: hold the row lock past its expiry, release → the check refuses |
| 4.9 | integration | `test_admission_event_id_is_row_id` | admit → `event_id` equals the row `id`; mark delivered and reply lookup by it → one row; two admissions in one millisecond → both listed once, newest-first by `(created_at, event_id)`; a `{millis}-{seq}` id → not found |
| 5.1 | manual | `test_local_lane_rebuilt_from_empty` | local lane torn down; `agentsfleetd migrate` from empty, `make test-integration-rustd` and the smoke run's output pasted in Session Notes |
| 6.1 | unit | `test_docs_name_no_removed_table` | inventory check over docs outside `done/` → 0 hits |
| 6.2 | manual | `test_public_docs_branch_open` | docs branch URL with the four pages and the `<Update>` recorded in Session Notes |
| 7.1 | unit | `test_no_whole_row_json_reads` | scan of `rustd/crates/*/src` → 0 matches for `to_jsonb(<alias>)` |
| 7.2 | integration | `test_library_list_single_statement` | pages of 1 and 25 cards → one statement each; card deleted between pages → no error, no stale card |
| 7.4 | integration | `test_every_sql_file_prepares` | every `**/sql/*.sql` → `PREPARE` succeeds; a file naming a dropped column → fails naming the file |
| 7.3 | integration | `test_lock_wait_is_bounded` | wallet row held in another transaction → request fails under 3 s, retryable; next request succeeds |
| 7.5 | unit | `test_rows_decode_into_typed_structs` | scan of `rustd/crates/*/src` → 0 matches for `try_get`; a seeded `row.try_get(0)` fails the scan naming its file |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Schema rebuilds from empty into the documented layout (§2–§4) | `make test-integration-rustd` | exit 0 | P0 | |
| R2 | No code or live doc names a removed table (§3, §6) | the inventory check below | 0 | P0 | |
| R3 | Conventions carry the rules (§1) | `grep -c "Counter and pointer names" .orly/docs/SCHEMA_CONVENTIONS.md` | 1 | P1 | |
| R5 | No whole-row JSON reads remain (§7) | `git grep -cE "to_jsonb\([a-z]+\)" -- 'rustd/**/src/**/*.rs' \| wc -l` | 0 | P1 | |
| R4 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | |
| S1 | Conform gates green | `make harness-verify` | exit 0 | P0 | |
| S2 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S3 | Lint green | `make lint-all` | exit 0 | P0 | |
| S4 | Version in sync | `make check-version` | exit 0 | P0 | |
| S5 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S6 | No oversize source file | `git diff --name-only origin/main...HEAD \| grep -v '\.md$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | |

R2 inventory check — removed names are the tables `CREATE`d at `a8646bb2a` and absent at `HEAD`:

```bash
old=$(git grep -hoE "CREATE TABLE IF NOT EXISTS [a-z_]+\.[a-z_]+" a8646bb2a -- schema | awk '{print $NF}' | sort -u)
new=$(git grep -hoE "CREATE TABLE IF NOT EXISTS [a-z_]+\.[a-z_]+" HEAD -- schema | awk '{print $NF}' | sort -u)
comm -23 <(echo "$old") <(echo "$new") | sed 's/\./\\./' | paste -sd'|' \
  | xargs -I{} git grep -nwE "{}" -- rustd ui cli scripts playbooks make docs/architecture docs/AUTH.md docs/AUTH_DEVICE_LOGIN.md docs/v2/pending docs/v2/active | wc -l
```

**Grading protocol (VERIFY):** run each spec-specific Verify command verbatim; Graded = ✅/❌ + one decisive output line. Repository-command rows point to the final `orly gate pr` results in Pull Request Session Notes. A P1 ❌ requires an owner-acked deferral quote in Discovery.

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

| File to delete | Verify |
|----------------|--------|
| every pre-rebuild slot (`schema/100_schemas.sql` … `schema/955_runner_policy_required.sql`) | `git ls-files schema \| grep -cE '^schema/[0-9]{3}_'` → 0 |

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| `deletion_generation` | `git grep -nw deletion_generation -- rustd ui cli public \| head` | 0 matches |
| `setup_version`, `config_json`, `setup_context` | `git grep -nwE "setup_version\|config_json\|setup_context" -- rustd \| head` | 0 matches |
| `creation_digest`, `grant_source`, `checkpoint_at`, `selftest_all_ok` | `git grep -nwE "creation_digest\|grant_source\|checkpoint_at\|selftest_all_ok" -- rustd \| head` | 0 matches |
| removed table names | the R2 inventory check | 0 |
| `MemoryStore`, `InMemory`, `Memories::flip`, `Flipped`, `Route` | `git grep -nwE "MemoryStore\|InMemory\|Flipped\|Route\|arc_swap\|async_trait" -- rustd/crates/afd_memory \| head` | 0 matches |

## Out of Scope

- The pool-saturation benchmark, chat-claim contention and live-set cache coalescing — `M229_002`, which follows this workstream. Compile-checked query macros and `dispatches.result` holding only private content follow after that.
- Narrowing `api_runtime` to per-schema runtime roles — its own milestone, as `300_vault_secrets.sql` and `700_tenant_wallet.sql` record.
- A Firecracker or SPACE sandbox tier — `runners.sandbox_config` is ready for its keys.
- Renaming wire fields to the new column names (`fence`, `call_index`, `activity_sequence`) — owner chose column names only.

---

## Product Clarity (authoring record)

1. **Successful user moment** — the owner opens `docs/architecture/postgres_schemas.md`, picks any column, and can say what it holds, who fills it and why, without reading Rust.
2. **Preserved user behaviour** — every dashboard page, CLI command and runner keeps working; chat, save, publish, install, schedules, approvals and billing behave as before apart from §4.
3. **Optimal-way check** — a from-empty rebuild is the most direct route while no production data exists; forward migrations would keep 95 slots of history and every copy column.
4. **Rebuild-vs-iterate** — rebuild; the owner chose it, and iterating would mean dozens of `ALTER` slots that freeze the mistakes.
5. **What we build** — new slots, renamed queries with typed reads, the one-query Library list, connection time limits, erasure that keeps money and scrubs text, one runner wire removal, the conventions at source, the docs.
6. **What we do NOT build** — new features; the remaining data-access work; per-schema roles; Firecracker; wire renames.
7. **Fit with existing features** — compounds with M224_001 launch readiness; must not destabilise the fleet lease path (`runner.fleet_slots`, fencing).
8. **Surface order** — API and runner first; the CLI and dashboard are unchanged from outside.
9. **Dashboard restraint** — no new UI.
10. **Confused-user next step** — `docs/architecture/postgres_schemas.md` for contributors; the changelog `<Update>` for runner operators.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** one workstream, one PR, Sections in dependency order: conventions, slots, readers, behaviour, databases, docs.
- **Alternatives considered:** forward migrations per renamed table (rejected: freezes the copies and dead columns as history); one PR per schema (rejected by the owner: "do write it on big bang").
- **Patch-vs-refactor verdict:** this is a **refactor** because placement, naming and duplication are structural; a patch would rename without removing copies.

## Discovery (consult log)

- **Consults** — Owner decisions via AskUserQuestion, Oct 17, 2026, recorded as the chosen option: clean slate "Yes, whole repo"; layout "Fold library into authoring"; shared definitions table "Drop it"; invite email table "Drop it"; library delete "Cascade delete"; deadline rename "Rename"; conventions "Orly repo + orly update"; counter names "Apply everywhere"; runner knobs "Add Firecracker knobs now" then "Fold it in"; fleet "Keep required_tags"; model "Approve with fold"; invites "Add resolved_at"; each schema "Approve". After the adversarial review: operation_id "Keep, serve via join"; erasure "Keep money, scrub text"; tiers "Yes i agree to fix Tier 1, Tier 2". Architecture: `ARCH: grounded in schema/600_runners.sql | proposal: nine owned schemas | status: silent | landing: a` — doc-only commit `a8646bb2a`.
- **Adversarial review (Oct 17, 2026)** — 14 P1 findings verified against source; seven corrected the design (`installs.scopes`, mutable `channels`, API instants, frozen `review_context`, separate draft source and publication, `leases.event_created_at`, the CLI's required `operation_id`). Data-access findings moved to their own milestone.
- **SCHEMA GUARD override** — > Indy (2026-10-17): "Schema guard: skipped per user override (reason: we are refactoring the schema again as we moved to a chat based model to start agent fleets as per spec @docs/v2/active/M227_001_P0_API_CLI_DOCS_UI_WORKSPACE_CHAT_CUTOVER.md, hence a teardown of schema, db, and sqlx betterment in abstraction)" — context: authorises deleting every slot and the §5 rebuild.
- **Sign-off rule** — > Indy (2026-10-17): "When you start on the spec changes I want you to get a signoff from indy on every pre and post dimension on the code abstraction and get signoff." — clarified the same day: "the presignoff on a dimension i needed was on the code abstraction change you plan to do a mini write up inline to indy and post completion, the code abstraction can be verified by indy too." Before each Dimension that changes code abstraction, a short inline write-up for sign-off; after it, the landed shape for Indy to verify. Docs-only Dimensions are exempt.
- **Runner upgrade** — > Indy (2026-10-17): "Well when we deploy, there is just 1 runner in dev, so its easy to restart. Currently there are no fleets and we start with clean db migration. So there is no issue here on coordinated upgrade to fear or think about." — context: `deletion_generation` removal needs no compatibility window.
- **Data-access fold** — > Indy (2026-10-17): "I think fold these for Tier 3 1. First 2 2. Statement and lock time limits" — context: typed row reads, the one-query Library list and connection time limits join this PR (§7); the rest is its own milestone.
- **Query checking and migrator** — > Indy (2026-10-17): "Signed off by Indy: Approved for this. 1. Query checking: C 2. Migrator: switching to SQLx Migrator" — context: §7.4 and §2.
- **§2 sign-off and empty start** — > Indy (2026-10-17): "Well my dev database will be tear downed. So you can think of that way and its a scratch db with nothing exists in dev. Signed off = approved" and "indy will teardown and ensure when your PR goes in there is no need to think about legacy migration ledger crap" — context: §2 approved; no old-ledger refusal, no compatibility with `audit.schema_migrations`.
- **No legacy, anywhere** — > Indy (2026-10-17): "Record this i never asked for compatibilty to legacy, i said clearly we are tearing down db, period" and "that clearly says you have more open freedom to change" — context: the database is torn down and every client ships in this PR, so no compatibility window, versioning prefix, alias or adapter is designed, asked about, or kept for any surface: schema, wire or API. The 80 integer `_at` fields become RFC 3339 strings in place under `/v1`.
- **Timestamps and clock** — signed off by Indy (2026-10-17) after review by Tarzy: instants are `TIMESTAMPTZ(3)`; Rust holds `jiff::Timestamp` end to end, `afd_core` included, with `UnixMillis` deleted; rows bridge through `jiff-sqlx`; the API emits RFC 3339 in UTC through jiff's standard serialization, with no integer adapter. Clock rule: a locking validity check locks first, then reads `clock_timestamp()` once; non-locking reads and `SKIP LOCKED` sweeps use `statement_timestamp()`; lifecycle stamps are `now()`; `now()` never decides an expiry. Evidence: scratch-container contention and `EXPLAIN` runs on Postgres 18.6. Rules land in orly 0.14.7.
- **§5 order and shape** — > Indy (2026-10-18): "I think Dimension 5 must be started after 6 and 7. 5 must happen and get tested on the docker container locally, upon PR merged when the spec is moved to done as well the migration happens auto." — context: the owner-present procedure on deployed databases is replaced by a local from-empty proof; deployed databases migrate on merge.
- **§4–§7 prior approval** — > Indy (2026-10-18): "approved 4,5,6,7" — context: §4 including the new 4.7 (RFC 3339 instants in API, CLI and UI) and 4.8 (clock read after the lock wait), §6 including the `~/Projects/docs` branch, §7 as written, and §5 as the local from-empty proof after §6 and §7. Post write-ups remain due after each Dimension.
- **Dependencies at their latest** — > Indy (2026-10-18): "Also ensure you use the latest crates in Cargo.toml of rustd" and "donot pin to a downlevel" — context: Rust 1.99.1 and every crate release taken in `9e08f1e25`. Two holds, each chosen via AskUserQuestion: `jsonwebtoken` stays on 10.x while `octocrab` (latest 0.54.5) depends on it ("Hold 10.x, note why"), and `gix` stays on 0.88 because 0.89's `worktree-mutation` pulls nine uncalled crates ("Hold 0.88"; "Hold gix on 0.88 in this PR, indy decides later").
- **octocrab replaced in this PR** — Indy (2026-10-18), via AskUserQuestion: "Replace inside M229" — context: the workspace uses three slices of its 75,205 lines (App JWT, two calls, two webhook payloads) and already bypasses its typed request and response; it brings eight crates nothing else uses and holds `jsonwebtoken` at 10. Lands with afd_credential and afd_api_ingress in §3 (Dimension 3.3), and `jsonwebtoken` moves to 11 in the same commit.
- **Integer milliseconds** — > Indy (2026-10-18): "confirm in every crate there are not handrolled serde millisecons, any mills still available, or i64 based milliseconds, or i64 millisecond constants being used and so on - on a per crate level" — context: Dimension 3.4; spans become `std::time::Duration`, `afd_core::clock::{add_millis, millis_between, from_millis}` are deleted for `after` and `before`.
- **Memory flip removed** — Indy (2026-10-18), asked "are there production callers, you said there we none, if so why keep", then via AskUserQuestion: "Delete it" — context: `Memories::flip` had no caller outside its tests, `InMemory` compiled only under `test-util`, and `PgStore` was the only production `MemoryStore`. The flip, its route table (`arc-swap`), the `dyn` trait (`async-trait`), the in-memory store, the flip-only error kinds (`Moving`, `MissedWrite`, `Refused`) and the `test-util` feature are deleted; `Memories` holds `PgStore` directly per M-DI-HIERARCHY. `runner_fleet.md` §"Memory backends and scope" keeps the vendor-store decision and records that the trait and flip return with the first vendor store.
- **Per-batch sign-off** — Indy (2026-10-18), via AskUserQuestion: "Pre and post" — context: before each §3 batch a plan per crate, after it a write-up (shape before and after, helpers, millisecond edges kept, tests and coverage with commands).
- **API instant shape** — Indy (2026-10-18), via AskUserQuestion: "jiff standard" — context: `afd_wire::instant` serializes through jiff (`…:49Z`, `…:49.123Z`) and keeps the sub-millisecond refusal (REST guide §4 rule 6); evidence: GitHub's `created_at` reads `2026-10-17T13:24:54Z` (`gh api repos/agentsfleet/agentsfleet/pulls/769`). The fixed-width form was dropped, so text order is no longer time order and clients compare parsed values (`cli/src/commands/billing.ts:143` is the first such site, still `number` until 4.7 lands).
- **§3 batch plans** — Indy (2026-10-18), via AskUserQuestion: "Approve all" (afd_tenant, fleet lifecycle/ops/admission, afd_credential, the instant module), then for the re-spawned forks "Approve all three" — context: a stored provider `mode` stays a permanent fault when decoded as text inside a typed row; a GitHub 2xx body that does not decode is permanent; Dragonfly-held connector deadlines keep their time-to-live; `REPLAY_TOMBSTONE_MS` is live (`afd_authoring/src/replay/mod.rs:116`) and moves with authoring; `GATE_PURGE_*` has no reader and is deleted.
- **Clock owner** — Indy (2026-10-18), via AskUserQuestion: "SQL owns it" — context: lifecycle stamps and expiry checks read the database clock; Rust passes `now` only where it mints an identifier.
- **No-expiry credential** — Indy (2026-10-18), via AskUserQuestion: "Far-future date"; > Indy: "i donot like null" — context: `NEVER_EXPIRES = Timestamp::constant(253_402_207_200, 999_000_000)` (`9999-12-30T22:00:00.999Z`); the lease-ceiling option was rejected.
- **Typed rows** — Indy (2026-10-18), via AskUserQuestion: "Yes, all crates" — context: Dimension 7.5; baseline 373 `try_get` across 13 crates at `331c0afc8`, 264 positional.
- **Admission event identifier** — Indy (2026-10-18), via AskUserQuestion: "Re-key it", then on the plan "UUIDv7 row id" — context: Dimension 4.9; the `created_at_ms || '-' || seq` key tied the identifier to the database clock and could not be known before the insert. No published page or OpenAPI example spells the old shape (grep of `~/Projects/docs` and the repository; one hit in `docs/v2/done/M42_001…`, frozen). Readers outside afd_admission change in the downstream batch.
- **Standard calls only** — > Indy (2026-10-18): "just keep it standard, you always harrow some crap and take a lower end call to create a tech debt, a classic example is BIGNIT for TIMESTAMPZ and fleets.events.id" — context: an instant is `TIMESTAMPTZ(3)` in Postgres and a jiff `Timestamp` in Rust everywhere, with no millisecond number standing in for one: not in a column, a cursor token, a dedup key or event metadata. A judgment call that has a standard answer is taken, not offered. Applied below.
- **Runner, cron and events batch calls** — Indy (2026-10-18), via AskUserQuestion: `fleet.events.id` "Postgres uuidv7()" (column `DEFAULT uuidv7()`, the insert reads it back with `RETURNING id`); runner event metadata `last_seen_at` "Time string" (RFC 3339, changelog line); one-shot schedule fire time "Row's own stamp"; schedule fields "Rename to match" (the §2 schema already renamed the columns); sandbox tool spans "Duration inside" (the model-facing `timeout_ms`/`yield_time_ms` argument names stay); `since=` "Postgres" (`statement_timestamp() - $ago::interval`). Taken as standard without a question, per the entry above: `runner.events.dedup_key BIGINT` becomes `last_seen_at TIMESTAMPTZ(3)` (slot 1830, unique on `(runner_id, last_seen_at)`); the events cursor token carries the RFC 3339 instant (every database starts empty, so no issued cursor survives); a never-seen runner reads `NEVER_SEEN = Timestamp::UNIX_EPOCH` (no NULL, per "i donot like null"); the operator API's `admin_state` field becomes `status`, the column's name; `RunnerSeed` and `FleetEventSeed` join `afd_db::test_util`.
- **Postgres 19 lanes** — Indy (2026-10-18): "i think move postgres:19rc1" — context: PostgreSQL 19 RC 1 shipped 2026-10-15, GA is planned for 2026-11-19, and `postgres:19-alpine` is not yet tagged. The local and test compose lanes move to `19rc1-alpine`; all 52 slots apply from empty on it. The production playbooks (`playbooks/operations/database_region_move/`, `teardown/database/`) stay on `18-alpine`, because they `pg_dump` and `psql` against PlanetScale's Postgres 18 servers. `PGDATA` is a fixed path, so a worktree's existing lane volume must be reset once (`docker compose -p <project> down -v`, then `make _ensure-test-infra`): 19 refuses an 18 data directory.
- **Dragonfly 2.0.2** — Indy (2026-10-18), via AskUserQuestion: "Bump, no phone-home" — context: v2.0.2 (2026-10-07) is the latest release; it adds anonymous deployment info to the daily version check. Every node already starts with `--version_check=false` (`scripts/dragonfly-cluster.sh:124,156`), so only the image tag moves, in both compose lanes and both Fly Dockerfiles. Evidence: `afd_dragonfly --include-ignored` on v2.0.2, 3 full runs: 207, 208 and 206 passed out of 208. The baseline on v2.0.0, 3 full runs: 207, 208 and 207. Both versions fail intermittently in the node-disconnect hub tests (`test_node_loss_is_a_gap_not_a_reconnect`, `namespaced_subscriptions_survive_a_server_disconnect`, `namespaces_isolate_subscriptions`), so the flake predates the bump and still blocks a green `make test-integration-rustd`.
- **Message operation key** — Indy (2026-10-18), via AskUserQuestion: "A" (column on messages) — context: a message read its `operation_id` only through `LEFT JOIN authoring.replay_receipts` (`afd_authoring/src/sql/message/page.sql:18`) and failed without one (`message/read.rs:104`), so Dimension 4.2's sweep and key reuse after 24 hours would make every message older than a day unreadable while drafts live 30 days. `authoring.messages` gains `operation_id UUID NOT NULL`, written at insert; receipts only enforce the window. Rejected: a nullable public `Message.operation_id`, and exempting messages from row 224.
- **Docs branch ahead of code** — `chore/m229-schema-clean-slate-changelog` at `c3d4c8a` in `~/Projects/agentsfleet-worktrees/m229-docs` (docs `make test` 22 OK, `make lint` clean) states, from this spec, behaviour the code does not have yet; each is re-checked against code before the PR: 4.7 most API types still send milliseconds and `public/openapi.json` still carries `expires_at_ms` (`MintCredentialResponse`, `PollSessionResponse`) with no `format: date-time`; 4.5 `deletion_generation` still in 31 places under `crates/*/src`; 4.2 the replay key expires 24 hours after purge today, not 24 hours from the first attempt (`replay/sql.rs` PURGE); 4.3 Library removal keeps revisions a fleet references today (`library/remove.rs` UNREFERENCED); runner `status` replaces `admin_state` in OpenAPI `RunnerItem` and `RunnerAdminPatchResponse`.
- **Metrics review** — no analytics/funnel playbook update required: no event, funnel or signal changes.
- **Skill-chain outcomes** — pending: `/orly-write-unit-test`, `/orly-write-integration-test`, `/review`, `orly-babysit-prs`.
- **Deferrals** — none.

## Review prompt (Codex — Aiwa CTO with Astra)

Paste as-is. Prose is a claim to check against source, never evidence.

```text
Re-review docs/v2/active/M229_001_P1_API_DOCS_INFRA_POSTGRES_SCHEMA_CLEAN_SLATE.md with
docs/architecture/postgres_schemas.md, branch feat/m229-postgres-schema-clean-slate, against source.
Check each earlier fix, then attack with file:line evidence: slot and cycle order; SQLx Migrator use
and checksum refusal; the PREPARE-every-file test; the deletion matrix; every dropped column's last
reader; §5's guard; the R2 inventory check; each §4 negative case.
Output: findings table, most severe first (severity, section, evidence, what fails, smallest fix),
then spec READY or BLOCKED and design SOUND or UNSOUND, naming the P0s. No praise.
```
