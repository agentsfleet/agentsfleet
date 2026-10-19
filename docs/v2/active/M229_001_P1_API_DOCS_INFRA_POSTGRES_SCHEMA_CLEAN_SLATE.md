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
**Branch:** feat/m229-model-catalogue (section 8; owner merges into feat/m229-postgres-schema-clean-slate)
**Baseline revision:** 53e3d093ece9ff39fde0290924ff30b7ac70efca
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
- **Section 8 handshake** — implement the curated identities, exact provider context evidence, dated request-tier prices and native runner wires in an isolated worktree. ASSUMPTIONS I'M MAKING: databases start empty; Discovery's eight catalogue calls override conflicting section text; the owner merges local commits; excluded HTTP, API, daemon, benchmark and OpenAPI edits are handed over. Per-request billing report design awaits the owner's decision; independent catalogue and wire work proceeds.

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
| `rustd/crates/afd_db/src/pool/**` | EDIT | Runtime statements as `.sql` files beside the pool |
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
| `scripts/model-library-allowlist.json`, `scripts/seed-models.mjs`, `playbooks/operations/model_catalogue/**`, `make/dev.mk`, `make/quality.mk` | EDIT | §8: curated identities, exact prices and provenance; Bun callers and the validator for this data format ([#771](https://github.com/agentsfleet/agentsfleet/issues/771)) |
| `scripts/check_model_allowlist.py`, `scripts/check_model_allowlist_test.py` | DELETE | §8: the provider-wide priced-or-reasoned format is removed by the owner's curated-only decision; source, amount, region, duplicate and sentinel refusals are covered in the Bun validator |
| `scripts/model-catalogue/**`, `package.json`, `bun.lock` | CREATE / EDIT | §8: bounded schema validation, exact decimal arithmetic and seeder helpers |
| `schema/13*_model_*.sql` | EDIT / CREATE | §8: identity with one advanced flag; separate price tiers, dates and provenance |
| `rustd/crates/afd_billing/src/{rates,meter,nanos}.rs`, `rustd/crates/afd_credential/src/provider/**`, `rustd/crates/afd_admin/src/**`, `rustd/crates/afd_authoring/src/catalogue/**` | EDIT | §8: tier, date and cache-write pricing; picker filtering |
| `rustd/crates/afd_core/src/{lib,catalogue}.rs`, `rustd/crates/afd_tenant/src/models/**`, `rustd/crates/{afd_admin,afd_tenant,afd_credential}/Cargo.toml` | EDIT / CREATE | §8: shared typed price/evidence records; advanced-filtered reads and atomic administrator replacements |
| `rustd/crates/afr_providers/{assets,src,tests}/**`, `rustd/crates/afr_providers/Cargo.toml`, `rustd/Cargo.lock`, `rustd/crates/afd_wire/src/report/**` | EDIT / CREATE | §8: native Gemini and Ollama wires, mocked transport proofs and cache-write usage |
| `ui/packages/app/**`, `cli/src/**` (model picker and provider commands) | EDIT / CREATE / DELETE | §8: catalogue display names, explicit advanced selection, precise prices, no static model suggestions; bounded test files |
| `rustd/crates/afd_billing/src/{biller,price,ledger,limits}/**` | CREATE | §9: the `Biller` trait, the pure `Rater`, `LedgerBiller`, budget limits over `billing.usage_lines` |
| `rustd/crates/afd_billing/src/{store,charge,meter,rates,budget}.rs`, `rustd/crates/afd_fleet/src/lease/**`, `rustd/crates/afd_authoring/src/{lease,model,review,maintenance}/**` | EDIT / DELETE | §9: both paths call the `Biller`; their own money SQL and rounding go |
| `rustd/crates/afd_wire/src/{usage,report}/**`, `rustd/crates/afd_api_runner/src/**`, `rustd/crates/afr_supervisor/src/**`, `rustd/crates/afr_agent/src/**` | CREATE / EDIT | §9: per-request usage lines on renew and report |
| `schema/20*_billing_*.sql` | EDIT / CREATE / DELETE | §9: `billing.usage_lines` and `billing.holds`; `fleet_charges` and `authoring_settlements` removed |
| `rustd/crates/agentsfleetd/src/plane/**` | EDIT | §9: constructs `LedgerBiller` with `OutagePolicy::Open` |
| `docs/reviews/model-catalogue/**` | CREATE | §8: provider verification, measured checks and exact handover for the other agent's paths |
| `rustd/crates/afd_db/Cargo.toml`, `rustd/crates/afd_db/tests/**` | EDIT / CREATE | §8: real seeder through the canonical migrator; price-table shape and grants |
| `rustd/crates/afr_telemetry/src/labels/tests.rs`, `docs/metrics.runner.census.tsv` | EDIT | §8: Gemini and Ollama are two added closed provider labels; retain the label-product test and declare its exact series count |
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
- **Dimension 4.5** — runner protocol bodies carry no `deletion_generation`; the runner and its report spool decode without it; the draft message response still carries `operation_id`, stored on the message → Test `test_runner_wire_drops_deletion_generation`
- **Dimension 4.6** — a resolved approval gate or invite cannot be resolved again; the pending indexes read `resolved_at` → Test `test_resolution_happens_once`
- **Dimension 4.7** — the API, the CLI and the UI send and read every instant as an RFC 3339 string in UTC through jiff's standard serialization (`…:49Z`, `…:49.123Z`); clients order instants by parsed value, never by text; the device-flow window is `expires_at` → Test `test_api_instants_are_rfc3339`
- **Dimension 4.8** — every locking validity check (lease renew, report fence, draft and schedule reads under lock) refuses a row that expired while it waited on the lock → Test `test_expiry_checks_read_clock_after_lock`
- **Dimension 4.9** — an admission event's `event_id` is its `fleet.admissions` row `id` (UUIDv7, minted in Rust before the insert); delivery, reply and inherit lookups key on it, and the history keyset `(created_at, event_id)` stays newest-first → Test `test_admission_event_id_is_row_id`

### §6 — Docs

- **Dimension 6.1** — no doc outside `docs/v2/done/` names a removed table → Test `test_docs_name_no_removed_table`
- **Dimension 6.2** — public pages for closed-draft retention, the retry window and Library deletion, plus a changelog `<Update>`, land on their own docs branch; the runner protocol gets no public page (Discovery: Runner protocol stays unpublished) → Test `test_public_docs_branch_open` (manual)

### §7 — Data access rides the rewrite

Every query is rewritten here, so two fixes land with it rather than touching each query twice. The 22 whole-row `to_jsonb(<alias>)::text` reads in 16 files (count at `a8646bb2a`) become explicit column lists into typed rows; `Json<T>` stays for real JSON columns. The Library list joins each card to its head revision in one statement. Connections set no `lock_timeout` or `statement_timeout` (Discovery: Time limits withdrawn).

- **Dimension 7.1** — no whole-row JSON read remains in runtime source → Test `test_no_whole_row_json_reads`
- **Dimension 7.2** — a Library page is one statement at any page size and stays coherent when a card is deleted mid-read → Test `test_library_list_single_statement`
- **Dimension 7.4** — every static statement is a `.sql` file beside its use case, loaded with `include_str!`, and each one `PREPARE`s against the rebuilt schema; joined-string statements become whole files where possible and the rest are named and tested → Test `test_every_sql_file_prepares`
- **Dimension 7.5** — every multi-column read decodes into a `#[derive(sqlx::FromRow)]` struct through `query_as`, instants through `#[sqlx(try_from = "jiff_sqlx::Timestamp")]`, single columns through `query_scalar`; no `try_get` remains (373 across 13 crates at `331c0afc8`) → Test `test_rows_decode_into_typed_structs`

### §8 — The curated model catalogue is pruned and priced as billed

The picker shows the brief's catalogue in [#771](https://github.com/agentsfleet/agentsfleet/issues/771) and nothing superseded; every price is the provider's published rate for the request it bills. Machine identifiers are the providers' exact API model IDs, display names are labels, and existing provider keys stay (`gemini`, displayed "Google"). Provenance and the Oct 18 verification are in Discovery (Catalogue prices verified).

- **Dimension 8.1** — the curated identity set is exactly #771's 17 entries; a picker row requires an exact context integer from its provider's own models API, with unreadable limits reported and unseeded → Test `test_curated_picker_lists_the_enabled_catalogue`
- **Dimension 8.2** — `gpt-5.6-terra` is returned only when advanced models are requested; one boolean distinguishes it, with no availability vocabulary → Test `test_advanced_picker_requires_a_request`
- **Dimension 8.3** — MiniMax M3, superseded generations and serverless-deprecated DeepSeek V4 Pro 0813 have no catalogue rows; every database starts empty, so there are no saved selections to preserve → Test `test_pruned_model_rows_are_deleted`
- **Dimension 8.4** — a context tier prices the whole request: OpenAI at 272,000 input tokens is base and at 272,001 upper; Haiku 5.5 at 100,000 prompt tokens is base and at 100,001 upper, counting cache reads and writes → Test `test_context_tier_prices_the_whole_request`
- **Dimension 8.5** — an effective date switches price at UTC midnight: Gemini 4 Argon (`gemini`) bills 2.00 / 0.10 / 10.00 at 2026-11-30T23:59:59.999Z and 4.00 / 0.20 / 20.00 at 2026-12-01T00:00:00Z; Mistral Large 4 keeps its promotional rate with no invented end date → Test `test_effective_dates_switch_at_utc_midnight`
- **Dimension 8.6** — cache writes bill at their own rate: Anthropic by 5-minute or 1-hour lifetime, OpenAI at its listed per-tier rate, never as uncached input → Test `test_cache_writes_bill_at_their_rate`
- **Dimension 8.7** — 0.006, 0.015, 0.0375, 0.0625, 0.075, 0.125 and 0.1875 USD per million tokens store and bill exactly in nanos → Test `test_sub_cent_rates_bill_exactly`
- **Dimension 8.8** — usage counts each token once: cache reads leave uncached input, cache writes are not also input, reasoning tokens bill once as output → Test `test_usage_counts_each_token_once`
- **Dimension 8.9** — Ollama records local or cloud, lists tags from its configured server, and keeps token pricing unknown; only a customer's own runner may dial a marked local endpoint, never the daemon; cloud requires https and the existing address guards → Test `test_ollama_pricing_stays_unknown`
- **Dimension 8.10** — the native `gemini` route carries tool calls across turns with their thought signatures, separate from OpenRouter's Gemini route (mocked transport) → Test `test_native_gemini_continues_tool_calls`
- **Dimension 8.11** — every price carries source URL, verification time, effective dates, unit and conditions, and the seeder refuses one without them → Test `test_price_rows_carry_provenance`

### §8 UI follow-up — user-reported fixes

These user-reported fixes extend the application and design-system scope under `ui/packages/**` and this tracking section.

All 22 reports and their subitems are recorded below. All five requested server additions are consumed by the clients.
Client details and remaining release checks are in [M229_UI_REQUESTS.md](../../reviews/M229_UI_REQUESTS.md).

`make test-coverage-all` passes every configured floor in `/tmp/m229-completion-coverage-all-final.txt`:

- App: 412 files / 3848 tests; 100% statements, branches, functions and lines.
- Website: 20 files / 133 tests; 100% on all four measures.
- Command-line interface (CLI): 214 files / 1937 passing tests / 16 existing live-test skips / zero failures; 100% functions and lines.
- Design system: 62 files / 497 tests; 100% on all four measures.

App and CLI lint pass in the scoped `c272f510d` and `777ff460d` commit hooks.
No coverage threshold, timeout, skip or bundle budget changed.

| Report | Required result | Progress / evidence |
|---|---|---|
| 1 | Meaningful setup-chat titles; preserve explicit renames. | `2ead62453` adds explicit rename. Server `1b084cd67` records meaningful initial titles; app `c272f510d` consumes them without fetching transcripts. |
| 2 | Search all private chats, with honest scope and pagination. | `c272f510d` consumes creator-only server title/message search with query-bound paging, cancellation and access-loss clearing. Saved message changes refresh an open search; busy-only changes make no extra read. |
| 3 | Selecting a chat keeps its position and avoids the new-fleet page flash. | `2ead62453`: stable ordering and selection-specific loading. |
| 3a | Delete a private setup chat with confirmation and clear failures. | `2ead62453`: shared deletion action and confirmation. |
| 3b | Show draft, saved, version, and publication state from recorded data. | `c272f510d` reads the three nullable revision counters from server `1aeb6ffb7`. Actual Save/Apply receipts and bounded publication refreshes keep independent proposed, saved and published labels current. |
| 4 | Remove Review Setup; save directly with current setup validation. | `2ead62453`: inline setup and direct Save / Save changes. |
| 5 | Edit the correct saved fleet; explain its purpose; remove Advanced settings. | `aa88f485f`, `2ead62453`: revision-bound source, saved purpose, no Advanced settings; app coverage gate passes. |
| 6 | Combine Setup and Trigger; show triggers and schedules read-only. | `aa88f485f`: combined under Setup. Simplified to a schedule summary; server `9deadc1c3` now supplies description, run state and next run. The app consumes these directly and refuses missing or malformed fields. |
| 7 | Use the shared table row hover on runner leases. | `414571be3`: shared hover and keyboard focus. |
| 8 | Retain lease history and prevent missing Open fleet destinations. | Server `bff5ec896` retains leases and supplies authorized destinations. The app validates the page, displays missing work coherently and disables unavailable fleet/workspace links, with no extra probe. |
| 9 | Rename runner lease Outcome to Status. | `414571be3`: Status column and regression tests. |
| 10 | Move lease inspection beneath its fleet label; remove the chevron column. | `414571be3`: View lease beneath the agent label. |
| 11 | Keep Status / Created placement and row hover consistent across tables. | `414571be3`: shared hover; lease Created precedes Duration and Status, matching workspace Activity. |
| 12 | Rename This workspace to Workspace; explain library visibility. | `4662cda2a`: scope explanation and Workspace label. |
| 13 | Add a New fleet action to the empty workspace library. | `4662cda2a`. |
| 14 | Show the full Secrets search focus border on the first click. | `492e115f6`: shared inset focus ring and focus border; browser regression written. |
| 15 | Use the agent callsign across recents, fleets, leases, and billing; billing shows only the callsign. | `e6184d1ad`: callsign across recents, fleet headings/cards, leases and billing; description beside the callsign on saved fleets. Server commit `18ac1e074` supplies saved purpose on lists; cards and recents consume it, with null omitted and no extra read. |
| 16 | Make the credit balance stand out with a contrasting accent. | `492e115f6`: information-blue balance treatment and stronger amount. |
| 17 | Improve Platform menu icons, alignment, and active destinations. | `ffc5cfd3b`: shared menu navigation, consistent icons, active destination, keyboard focus tests. |
| 18 | Improve Settings menu icons, alignment, and active destinations. | `ffc5cfd3b`: the same menu navigation and destination tests. |
| 19 | Remove repeated Encrypted and hidden rows; emphasize Encrypted in the Secrets subtitle. | `492e115f6`: shared subtitle with emphasized Encrypted; repeated row text removed. |
| 20a | Standardize runner Activity headings: Activity, Details, and Occurred. | `0645aedcc`: Occurred names the recorded activity time, preserving its API meaning. |
| 20b | Show descriptive runner activities for both existing and future rows. | `0645aedcc`: one typed formatter reads recorded facts, including access-token changes. |
| 20c | Explain activity details without repeating the activity timestamp. | `0645aedcc`: descriptive details, current metadata keys, one timestamp column, regression tests. |
| 21 | Fix Workspace Activity page loading and preserve useful failure recovery. | `ff00608e2`: parse the published Activity payload and keep recovery within the page; app regression tests pass. Deployed reproduction remains unverified. |
| 22 | Simplify Platform Fleet library subtitle, terminology, and empty-state action. | `ffc5cfd3b`: one-line description, Fleet setups heading, workspace action, empty-state regression. |
| CLI | Audit the matching model, billing, setup and schedule commands for completeness. | `e0c15db29` fixes exact money, inclusive advanced models and exact-event recovery. `e7c5c55f9` validates fleet lists; `dc65550b6` validates schedule summaries. `777ff460d` adds bounded chat list/search, idle rename and confirmed delete. Final gate: 1937 pass / 16 existing live-test skips / zero failures; 100% functions and lines. |

The catalogue editor and picker landed in `43b537a19`, `7152af4d2`, `c50b4b630`, and `98c184bdc`.
`78118fd17` separates intent-loaded sidebar actions. `c272f510d` separates editor state from shared sidebar context and adds bounded private-chat search.
The shared row list keeps ordinary history and search selection/deletion behavior aligned.

Production `bun run build` and `bun run size --json` pass all 25 budgets and the server-module guard.

Shared compressed JavaScript is 274265 / 276480 bytes. New-fleet total is 334933 bytes; fleet-detail total is 364328 bytes.

Baseline `4e0bafb45502bad3b3b88b631a3690bb80ec2496` measured 364917 and 366692 bytes for those pages.
The reductions are 29984 and 2364 bytes.

The baseline failed the server-module guard; current limits remain unchanged.
Evidence: `/tmp/m229-sidebar-build-split.txt` and `/tmp/m229-sidebar-size-split.json`.
Claude refreshed the root lockfile in `8a194eda3`, removing `cron-parser`.

`make dry-app-smoke` passes four desktop/mobile sign-in and protected-route checks (`/tmp/m229-completion-browser-smoke.txt`).
Duplicate audit: one weaker case removed; two overlapping cases merged into one, retaining their assertions. Net reduction: two cases.

Native source rereviews are clean for money readers, schedule/lease readers and the new CLI history source.
Sidebar review stopped after three cycles without convergence. Its final message-only search finding has a failing-before regression and a repair.
Claude must inspect the final saved entity-tag propagation. Receipts and exact scope are recorded under `.tmp/reviews/`.

Local daemon health and readiness return HTTP 200. Authenticated priming and reported-screen browser checks still need the dedicated testing identity decision.
No shared Clerk identity was changed.

Evidence: `.tmp/audits/local-priming-deviations.md`.
Public documentation remains in Claude's lane until docs ready. The review checklist is `.tmp/audits/m229-docs-review-checklist.md`.

The UI work is handed to Claude with these remaining release checks explicit; the shared spec moves to `done/` when required checks finish.

### §9 — One billing lifecycle behind a `Biller` trait

Fleet runs and setup chat bill through one lifecycle. Work reports usage per provider request; one pure `Rater` prices each request by its whole-request prompt tier and its request time; the priced lines land in `billing.usage_lines` inside the work's own transaction; then an `Arc<dyn Biller>` moves money after commit. `LedgerBiller` (wallets and holds in Postgres) is the built-in implementation; a queue or third-party biller is another implementation of the same trait. Design and the before/after map: [`billing_and_provider_keys.md`](../../architecture/billing_and_provider_keys.md) §11. Decisions: Discovery (Billing revamp).

- **Dimension 9.1** — usage is reported per provider request as `{seq, at, prompt, cached_read, cache_write, cache_write_5m, cache_write_1h, output}` plus runtime, and a line repeated with the same `(work, seq)` charges once → Test `test_usage_lines_deduplicate_on_sequence`
- **Dimension 9.2** — one `Rater` prices every line: tier by whole-request prompt size, price by request time, self-managed charges runtime only, each token category rounds up per line → Test `test_rater_prices_each_line_by_tier_and_date`
- **Dimension 9.3** — a reported request time outside its work's window is clamped to that window before pricing → Test `test_request_time_is_clamped_to_the_work`
- **Dimension 9.4** — `Biller` has `reserve`, `settle`, `release` and `balance`; one conformance suite (idempotency, release, short balance, racing holds) passes for every implementation → Test `test_every_biller_passes_the_conformance_suite`
- **Dimension 9.5** — a fleet run reserves at admission, settles and reserves again at each renewal, settles and releases at report, and an expired lease bills its last runtime → Test `test_fleet_expiry_bills_its_last_slice`
- **Dimension 9.6** — setup chat reserves at claim and per model call and settles at report through the same `Biller`, with no money SQL of its own → Test `test_setup_chat_settles_through_the_biller`
- **Dimension 9.7** — `billing.usage_lines` replaces `billing.fleet_charges` and `billing.authoring_settlements`; the tenant charges page reads that one table, and fleet budgets count its rows → Test `test_charges_page_reads_one_ledger`
- **Dimension 9.8** — `OutagePolicy::Open` is chosen at the biller's construction site: a biller that cannot answer admits the work and its lines settle later → Test `test_billing_outage_fails_open`
- **Dimension 9.9** — concurrent holds for one payer never exceed the balance the biller can see → Test `test_concurrent_holds_cannot_exceed_balance`
- **Dimension 9.10** — lines committed without a settlement (a crash between commit and `settle`) are settled by the sweeper exactly once → Test `test_unsettled_lines_are_resettled`

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
Model picker (operator and tenant catalogue reads): each entry adds `display_name`, boolean `advanced`, context provenance and priced tiers with effective dates; pruned entries are absent. Tenant `GET /v1/models?advanced=true` explicitly requests advanced entries; default reads omit them.
Runner renew and report usage: per-request lines `{seq, at, prompt, cached_read, cache_write, cache_write_5m, cache_write_1h, output}` replace the cumulative `input_tokens` / `cached_input_tokens` / `output_tokens` totals (§9).
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
| Pruned route selected | A tenant or setup names an identity absent from the catalogue | Selection is refused; no catalogue row or replacement is fabricated (8.3) |
| Unknown price | An Ollama model has no published token price | Price stays unknown; the spend view shows unknown, never 0 (8.9) |
| Promotion ends | Gemini's Dec 1, 2026 date passes, or Mistral publishes an end date | The effective-dated row takes over at UTC midnight; an undated promotion stays until the catalogue is edited (8.5) |
| Context cannot be verified | The provider models API cannot supply the exact served limit | Seeder reports the identity and omits its row; marketing shorthand is never converted (8.1) |
| Ollama boundary crossed | A daemon or shared runner is asked to dial a local endpoint, or cloud uses http | Refuse before network access; no private endpoint is discovered by the daemon (8.9) |

## Invariants

1. No schema object carries a vocabulary literal — catalogue scan in `test_schema_objects_carry_no_vocabulary_literals`.
2. Immutable tables cannot be updated — no UPDATE grant plus a refusing trigger; `test_immutable_tables_refuse_update`.
3. The migration list equals the tracked slots — `rust-migration-list` in `make lint-all`.
4. No applied slot changes silently — SQLx checksums; `test_migrate_refuses_edited_slot`.
7. Every static statement prepares against the schema — `test_every_sql_file_prepares` in the integration lane.
5. No money row has a foreign key to an erasable owner — catalogue assertion in `test_deletion_matrix_keeps_money`.

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
| 6.2 | manual | `test_public_docs_branch_open` | docs branch URL with the three pages and the `<Update>` recorded in Session Notes, and no runner-protocol page |
| 7.1 | unit | `test_no_whole_row_json_reads` | scan of `rustd/crates/*/src` → 0 matches for `to_jsonb(<alias>)` |
| 7.2 | integration | `test_library_list_single_statement` | pages of 1 and 25 cards → one statement each; card deleted between pages → no error, no stale card |
| 7.4 | integration | `test_every_sql_file_prepares` | every `**/sql/*.sql` → `PREPARE` succeeds; a file naming a dropped column → fails naming the file |
| 7.5 | unit | `test_rows_decode_into_typed_structs` | scan of `rustd/crates/*/src` → 0 matches for `try_get`; a seeded `row.try_get(0)` fails the scan naming its file |
| 8.1 | integration | `test_curated_picker_lists_the_enabled_catalogue` | all 17 identities with provider-API context proofs → exactly 17 default picker entries, `gemini` displayed "Google"; missing proof → reported and unseeded |
| 8.2 | integration | `test_advanced_picker_requires_a_request` | default → no Terra; explicit advanced read → Terra with `advanced=true`, all curated rows `false` |
| 8.3 | integration | `test_pruned_model_rows_are_deleted` | seed a non-curated identity and extra price tier in a test database; apply the curated set → both are deleted with no orphan price; repeat diff → no changes |
| 8.4 | unit | `test_context_tier_prices_the_whole_request` | 272,000 / 272,001 OpenAI input; Haiku 99,000 input + 1,000 cache read vs + 1,001 → base vs upper on every token |
| 8.5 | unit | `test_effective_dates_switch_at_utc_midnight` | 2026-11-30T23:59:59.999Z → 2.00/0.10/10.00; 2026-12-01T00:00:00Z → 4.00/0.20/20.00 |
| 8.6 | unit | `test_cache_writes_bill_at_their_rate` | Sonnet 1M 5-minute writes → 2.50 USD; 1-hour → 4.00 USD; Sol over 272K writes → 5.00 USD per million |
| 8.7 | unit | `test_sub_cent_rates_bill_exactly` | decimal strings × 1,000,000,000 → exact integer nanos for 0.006, 0.015, 0.0375, 0.0625, 0.075, 0.125 and 0.1875; UI retains each digit |
| 8.8 | unit | `test_usage_counts_each_token_once` | 1,000 input of which 400 cache read and 100 cache write, 50 reasoning → 500 uncached + 400 read + 100 write + output once |
| 8.9 | integration | `test_ollama_pricing_stays_unknown` | mocked `/api/tags` → exact server tags with endpoint kind; price stays unknown; daemon never dials local; shared runner and non-local http are refused |
| 8.10 | unit | `test_native_gemini_continues_tool_calls` | mocked stream with a function call and thought signature → next request echoes the signature |
| 8.11 | unit | `test_price_rows_carry_provenance` | allowlist row missing `source_url` or `verified_at` → seeder refuses it by name |
| 9.1 | integration | `test_usage_lines_deduplicate_on_sequence` | the same `(work, seq)` line logged and settled twice → one `billing.usage_lines` row, one debit; a new `seq` → a second row |
| 9.2 | unit | `test_rater_prices_each_line_by_tier_and_date` | 272,000 prompt tokens → base tier, 272,001 → upper tier for the whole request; cache reads and writes count toward the prompt; a self-managed line → runtime only; 1 token at 0.006 USD/Mtok → 6 nanos rounded up from 0.006 |
| 9.3 | unit | `test_request_time_is_clamped_to_the_work` | `at` before the work's start → priced at the start; after its end → priced at the end |
| 9.4 | integration | `test_every_biller_passes_the_conformance_suite` | `LedgerBiller` and `RecordingBiller` → idempotent reserve, settle and release; a short balance answers `Short`; release after settle frees the rest |
| 9.5 | integration | `test_fleet_expiry_bills_its_last_slice` | lease expires after 30 s unreported → one runtime line of 30 s charged and the hold released |
| 9.6 | integration | `test_setup_chat_settles_through_the_biller` | a setup-chat turn → its lines in `billing.usage_lines`, its hold released, no row in any authoring money table |
| 9.7 | integration | `test_charges_page_reads_one_ledger` | fleet and setup-chat lines for one tenant → one page newest-first; the fleet budget's day sum equals its lines |
| 9.8 | integration | `test_billing_outage_fails_open` | biller unavailable under `OutagePolicy::Open` → the work is admitted (`Unconfirmed`); under `Closed` → refused |
| 9.9 | integration | `test_concurrent_holds_cannot_exceed_balance` | two concurrent reservations of 60% of the balance → exactly one `Held` |
| 9.10 | integration | `test_unsettled_lines_are_resettled` | a line committed without settlement → the sweeper settles it once; a second pass charges nothing |

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

R2 inventory check — removed names are the tables `CREATE`d at `a8646bb2a` and absent at `HEAD` (the rebuilt slots write `CREATE TABLE` without `IF NOT EXISTS`, so the `HEAD` side matches both spellings):

```bash
old=$(git grep -hoE "CREATE TABLE IF NOT EXISTS [a-z_]+\.[a-z_]+" a8646bb2a -- schema | awk '{print $NF}' | sort -u)
new=$(git grep -hoE "CREATE TABLE (IF NOT EXISTS )?[a-z_]+\.[a-z_]+" HEAD -- schema | awk '{print $NF}' | sort -u)
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
5. **What we build** — new slots, renamed queries with typed reads, the one-query Library list, erasure that keeps money and scrubs text, one runner wire removal, the conventions at source, the docs.
6. **What we do NOT build** — new features; the remaining data-access work; per-schema roles; Firecracker; wire renames.
7. **Fit with existing features** — compounds with M224_001 launch readiness; must not destabilise the fleet lease path (`runner.fleet_slots`, fencing).
8. **Surface order** — API and runner first; the CLI and dashboard are unchanged from outside.
9. **Dashboard restraint** — §8 adds catalogue labels and explicit advanced selection; other dashboard changes follow §4.
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
- **Deadline outcome** — Indy (2026-10-18), via AskUserQuestion: "Always timed_out" — context: `author`'s unbiased `tokio::select!` raced the attempt deadline against renewal, which stops at the same instant, and reported `timed_out` 8 times and `failed`/`provider_usage_unknown` 4 times in 12; `biased;` with the deadline first makes a turn that reaches its deadline always report `timed_out`.
- **Renew half-move branch** — Indy (2026-10-18), via AskUserQuestion: "Delete it" — context: `meter_lease.sql` locks the lease and its slot (`FOR UPDATE OF l, a`) before either moves, so `slot_updated != 1` could not occur; the branch, its column and the doc calling a half-applied renewal a real outcome go (RULE NDC).
- **Orphan charge name** — Indy (2026-10-18), via AskUserQuestion: "Fleet id as name", asked twice — the second time after I read that `test_m201_charge_survives_unreadable_fleet_row` asserted NULL ("a fabricated name a reader cannot tell from a captured one") and that the old column was nullable — context: slot 2010 keeps `fleet_name NOT NULL`; `insert_fleet_charge.sql` falls back to `fleet_id::text` when the fleet row is gone, so the money always lands and a reader detects the fallback because it equals `fleet_id` on the row.
- **Time limits withdrawn** — Indy (2026-10-18), via AskUserQuestion: "No limits on pooled path", then "Remove limits" — context: a read-only probe of dev's PlanetScale PgBouncer (port 6432, the API role) refused every connection carrying `lock_timeout` or `statement_timeout` as a startup option (`FATAL: unsupported startup parameter in options: lock_timeout`, 7 of 7; `statement_timeout` alone likewise), and the login role has no role-level defaults. Deployed daemons run only the pooled API role, so no runtime connection could carry the limits; `afd_db::pool::limits`, `gave_up_on_lock` and Dimension 7.3 go. Supersedes item 2 of the Data-access fold. Neither limit existed on `main` before this branch (`640bc2e95`).
- **Model catalogue fold** — > Indy (2026-10-18): "I want the Model library to prune … add this to your spec", then "open an issue on the gh for the model stuff … and link the issue in the spec too" — context: the curated picker, provider mappings and pricing are refreshed in this PR as §8; the full brief, sources and the first pricing verification are [issue #771](https://github.com/agentsfleet/agentsfleet/issues/771).
- **Catalogue prices verified** — Oct 18, 2026, 12:33 UTC, official pages and OpenRouter's `/api/v1/models`: Anthropic, OpenAI (both tiers, cache writes 1.25× input), Google, Mistral, Fireworks GLM 5.3 Flash 0.15/0.03/0.50, Kimi K3 3.00/0.30/15.00, DeepSeek V4.1 Flash 0.30/0.006/1.20, OpenRouter Muse Wonder 4.00/0.20/20.00, Gemini 4 Argon 2.00/0.10/10.00 (cache write 0.1875), Qwen3.8 Max 0902 2.00/0.25/6.00, and Groq gpt-oss-120b 0.15/0.075 (50% cache discount)/0.60 all match #771. Differences: Fireworks states context as "1040k" (Kimi K3's FAQ says 1,048,576), so those three stay unverified integers; OpenRouter reports Muse Wonder at 1,048,576, not 1,000,000; Google and Mistral pages state no context window; DeepSeek V4 Pro 0813 serverless is deprecated from Sep 25, 2026, still showing 1.32/0.044/3.96; Anthropic adds a 1.1× multiplier for US-only inference.
- **§8 catalogue calls** — Indy (2026-10-18), via AskUserQuestion, eight answers: picker = exactly #771's 17 curated entries plus `gpt-5.6-terra` as advanced ("Exactly the brief"); native Gemini runner wire built in this PR through Rig 0.44's `gemini` module ("Build it now"); Ollama gets its own runner wire through Rig's `ollama` module ("New Ollama wire"); cache-write tokens are reported and billed by lifetime ("Bill them now"); prices move to a new `model.platform_model_prices` table, one row per prompt-size tier and date range ("New price table", after asking "do you need a new table"); rows that leave the picker are deleted, not flagged ("Delete pruned rows") — every database starts empty under §2/§5, so no saved configuration survives to preserve, and serverless-deprecated DeepSeek V4 Pro 0813 is left out the same way, leaving one advanced flag; context integers come from each provider's own models API, recorded with their source, and a model with no exact served limit is not seeded ("From provider API"); a local Ollama endpoint may be `http`/private only when marked local and dialled by the runner on the customer's host, never by the daemon, while cloud Ollama keeps https and the full SSRF guard ("Runner-local exception"). Where §8's text assumes an availability column with pruned/disabled values, this entry supersedes it until §8 is reconciled.
- **Charge shape after the `main` merge** — Indy (2026-10-18), relayed by the coordinator: "Accept fork's shape". `main` (`66ba35918`, PR #770) put setup-chat settlements on the charges page; this branch (`55d79e488`) had made every charge name its workspace and fleet. Merged shape: `workspace_id` stays required (both sources store it `NOT NULL`); `fleet_id`, `fleet_name` and `event_id` are optional because a setup chat may precede its fleet and is no fleet event; `authoring_attempt_id` is added; the billing tab labels a fleetless setup row "Fleet setup" and still shows the callsign alone for a purged fleet's charge.
- **§8 implementation decisions** — Indy (2026-10-18), answering the isolated catalogue worktree: include a separate unspecified-lifetime cache-write rate and token category; keep `effective_from` and `effective_until` with a narrow naming exception for those two price-table columns. Per-request usage, pricing and deduplication design is left to Indy and the other agent. Local Ollama stays blocked until Indy chooses customer-runner ownership. Neither deferred design is claimed complete here.
- **§8 isolated implementation** — owner instruction (Oct 18, 2026): branch `feat/m229-model-catalogue` starts at `53e3d093ece9ff39fde0290924ff30b7ac70efca`; local commits only, no push or Pull Request. The sibling agent owns `afd_http`, `afd_api_*`, `agentsfleetd`, `afd_bench` and `public/openapi.json`; required edits there are a file-and-line handover. Service tiers (OpenAI Batch, Flex and Fast) and Anthropic's US-only 1.1× pricing are outside this section. Whole-file Rust coverage must reach 99%; TypeScript coverage 100%.
- **Stale architecture docs** — Indy (2026-10-18), via AskUserQuestion: "Delete it" for `docs/architecture/tenant_provider_v2.md` (a selection shape M229 did not build) and "Cut §5–§9" for `scenarios/production-deploy-repair.md` (a repair-record design with no tables or code).
- **Docs branch ahead of code** — `chore/m229-schema-clean-slate-changelog` at `c3d4c8a` in `~/Projects/agentsfleet-worktrees/m229-docs` (docs `make test` 22 OK, `make lint` clean) states, from this spec, behaviour the code does not have yet; each is re-checked against code before the PR: 4.7 most API types still send milliseconds and `public/openapi.json` still carries `expires_at_ms` (`MintCredentialResponse`, `PollSessionResponse`) with no `format: date-time`; 4.5 `deletion_generation` still in 31 places under `crates/*/src`; 4.2 the replay key expires 24 hours after purge today, not 24 hours from the first attempt (`replay/sql.rs` PURGE); 4.3 Library removal keeps revisions a fleet references today (`library/remove.rs` UNREFERENCED); runner `status` replaces `admin_state` in OpenAPI `RunnerItem` and `RunnerAdminPatchResponse`.
- **Billing revamp** — Indy (2026-10-18/19), via AskUserQuestion and chat: usage reporting "I choose A" (per-request records, priced by the daemon, deduplicated on work and sequence); "Biller System must be abstracted by trait, one of the Trait Object is the inbuilt billing system", so a queue or a third party can bill later; the trait owns both gates and charges ("Both: admit + settle"); "Think on a large refactor and i command you to do so, since we have an opportunity to revamp"; one ledger ("One usage_lines"); rounding "Round up per line"; outage "keep it fail OPEN with an ability to flip later … since when we are early we must adopt the optimistic eagar path to promote usage from users". Setup chat moves from failing closed to failing open with fleets. Indy reviews the abstraction before implementation: architecture §11.
- **Gemini key out of the URL** — Indy (2026-10-19): "do what is needed to cover the security loophole. i leave it to you." rig-core 0.44 builds `?key=` (`providers/gemini/mod.rs:91`); `afd_inference`'s `HeaderKey` transport moves it to `x-goog-api-key` for both runner and daemon (`55eaf1d76`, `df93412df`).
- **Runner protocol stays unpublished** — Oct 19, 2026: Dimension 6.2 asked for a public runner-protocol page, but the docs repository's rules forbid one (`~/Projects/docs/AGENTS.md:54`: "Do not document internal agent pipeline internals (NullClaw config details, runner RPC protocol)"). A spec that contradicts a rule is amended, so 6.2 drops that page and keeps closed-draft retention, the retry window and Library deletion; the runner wire stays described in `docs/architecture/` and the generated `public/openapi.json`.
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
