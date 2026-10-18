# M229 §8 catalogue takeover

Claude owns integration, completion and review from this handoff. §8 remains incomplete.
Indy's latest instruction stops Codex feature edits, formatting, lint, tests, coverage and review.
Only the normal pre-commit hooks run during this handoff. No hook is bypassed.

Worktree: `/Users/kishore/Projects/agentsfleet-worktrees/m229-model-catalogue`.
Branch: `feat/m229-model-catalogue`; base: `53e3d093ece9ff39fde0290924ff30b7ac70efca`.
No push, Pull Request (PR), merge, global orly installation, or protected-path edit was made.
The installed orly and repository pin are both 0.14.7, as each commit hook reports.
Claude will move the wires onto main's `afd_wire::inference::{InferenceApi, ModelInference}` after merging.
That port is not implemented on this base. Delete this temporary handoff at CHORE(close), as Indy requested.

## Commits

| Hash | Subject | What it does |
|---|---|---|
| `df0b01018` | `chore(model): record isolated catalogue implementation scope` | Records the isolated branch scope and §8 decisions in the active spec. |
| `03ba97394` | `feat(providers): add native Gemini and Ollama model wires` | Adds rig-core 0.44 native transports, Gemini replay and mocked streaming/continuation proofs. |
| `0ee0ead84` | `feat(models): show catalogue names and opt in to advanced models` | Uses catalogue labels, requests advanced entries explicitly and displays precise/unknown prices. |
| `dcec10c58` | `feat(catalogue): store verified identities and dated price schedules` | Splits identity/prices, refreshes data and Bun seeder, and adapts shared database fixtures. |
| `40a46446e` | `feat(catalogue): expose evidence and dated prices through model services` | Adds typed metadata/price arrays, atomic administrator writes and current-base service reads. |
| `c8042ecad` | `chore(catalogue): account for native provider telemetry labels` | Updates closed provider-label assertions and metric census for Gemini and Ollama. |
| `a814472fa` | `docs(catalogue): record verified inputs and integration limits` | Records catalogue sources, exact integration handover, required tests and architecture changes. |

This handoff is committed last. Its hash is in the final response and `git log`.
The first three commits predate the new coauthor-trailer instruction; their history was not rewritten.
Every commit created during takeover includes `Co-Authored-By: OpenAI Codex <noreply@openai.com>`.

## Pending completion, including the UI

| Concern | Completed source | Pending work |
|---|---|---|
| API integration | Shared metadata/prices: `rustd/crates/afd_core/src/catalogue.rs:12`; services: `afd_admin/src/model.rs:26`, `afd_tenant/src/models/mod.rs:53` | Update all protected wire/query/handler/rendering paths below; regenerate OpenAPI. Existing handlers still expect removed fields and signatures. |
| Billing | Price storage: `schema/1305_model_platform_model_prices.sql:11`; base quote: `afd_billing/src/sql/select_platform_model_prices.sql:1` | Decide per-request reporting, then charge whole-request prompt tiers and dated rates, cache writes by category, and reasoning exactly once. Update authoring budget and fleet settlement too. |
| Administrator UI | Labels and advanced badge: `ui/packages/app/app/(dashboard)/admin/models/components/CatalogueList.tsx` | Replace the three-price add/edit forms with metadata and complete dated price arrays. Include source/time, prompt thresholds, nullable generic/5-minute/1-hour writes, conditions and clear validation errors. |
| Administrator UI precision | Current number-based forms: `ui/packages/app/app/(dashboard)/admin/models/components/AddModelDialog.tsx:35`, `EditModelDialog.tsx:32`; wire types: `ui/packages/app/lib/api/admin_model_library.ts:12` | Preserve decimal strings through conversion to integer nanos. Update API types/actions, add/edit payloads and returned table rows together; no `Number` round-trip for stored money. Editor work was not started. |
| Picker/registry UI | `ModelCatalogueProvider.tsx`, `ProviderModelSelect.tsx`, `ModelsRegistryCells.tsx` under workspace settings model components; `ui/packages/app/lib/models/display.ts:10` | Connect labels, evidence, complete price schedules and explicit advanced selection to the integrated API. Verify pagination, retries, retained selection and validation against the real handlers. Current API cannot yet supply the new shape. |
| Ollama UI | Unknown-price display in `ModelsRegistryCells.tsx`; native transport in `afr_providers/src/wire.rs:60` | Add configured-server tags and local/cloud controls only after ownership design is settled. Explain unknown price without showing zero. Keep local endpoints refused meanwhile. |
| Authoring/private chat | Labels: `afd_authoring/src/catalogue/project.rs`; actual admission: `afd_authoring/src/model/mod.rs:75` | Choose tokenizer/counting and provider dispatch for curated models. Current admission only allows recognized `gpt-4o`/`gpt-4.1` OpenAI families; all newly curated identities are ineligible. |
| Exact contexts | Six provider-response proofs: `docs/reviews/model-catalogue/provider-context-evidence.json` | Verify 12 remaining exact served limits from provider APIs. Do not infer marketing abbreviations or seed absent proof. |
| Local signoff | Existing localhost:3000 process belongs to the separate #754 worktree and points to api-dev | Start the integrated #771 frontend/backend, then walk through picker, setup, administrator editing, defaults, authoring and Ollama states with Indy. Current preview does not demonstrate #771. |
| #754 | Separate branch commits `e7a6fa1f1`, `bbd85c955`; companion docs commit `762a5d7` in `~/Projects/docs` | Deploy the local fix and rerun the affected acceptance journey. No #754 push, PR or deployment was performed during takeover. |
| Verification | Historical measured results below | Claude runs formatting, lint, tests, required Rust coverage and review after integration. No completed §8 or merge-readiness claim follows from this handoff. |

Rust paths abbreviated above and below start at `rustd/crates/` unless otherwise stated.
Workspace model component names above resolve under `ui/packages/app/app/(dashboard)/w/[workspaceId]/settings/models/components/`.

## §8 Dimension status

Statuses describe source on this branch, not deployed behavior. Spec names are requirements; actual existing tests are named separately.

| Dimension | Status | Source and existing test, or missing proof |
|---|---|---|
| 8.1 curated picker and exact contexts | partial | `scripts/model-catalogue/curated.ts:4`, `catalogue.ts:41`, `rows.ts:24`; complete document test `seed.integration.test.ts:26` seeds only six proved contexts. `curated.test.ts:8` checks exactly 18 identities. Full authenticated `test_curated_picker_lists_the_enabled_catalogue` is absent; 12 contexts are missing. |
| 8.2 explicit advanced Terra | partial | `afd_tenant/src/models/mod.rs:53`, `models/cursor.rs:33`, both tenant page SQL files, UI catalogue provider; `test_catalogue_walk_carries_metadata_prices_and_explicit_advanced_filter` at `afd_tenant/tests/integration_model_catalogue.rs:100`. API filtering/cursor/validator proof remains absent; Terra cannot seed without context evidence. |
| 8.3 delete pruned identities | done-in-code | `scripts/model-catalogue/sql/delete_models.sql:1`, `delete_prices.sql:1`, `seed.ts:45`; test `pruned identities and price tiers are deleted with no orphan prices` at `seed.integration.test.ts:101`. No disabled/pruned availability flag remains. |
| 8.4 whole-request context tiers | partial | Price rows and `scripts/model-catalogue/catalogue.ts:116`; test `the whole request switches above %i prompt tokens` at `catalogue.test.ts:114`. This proves quote selection only. No runtime settlement test, nor provider prompt aggregation including cached reads/writes. |
| 8.5 UTC date change | partial | Native Gemini dated rows at `scripts/model-library-allowlist.json:230`; test `effective dates switch at UTC midnight and respect offsets` at `catalogue.test.ts:121`. Coordinated Universal Time (UTC) lookup is proved; per-request settlement at the boundary is not implemented. |
| 8.6 cache writes billed separately | partial | Nullable generic/5-minute/1-hour rates in `schema/1305_model_platform_model_prices.sql:20` and `afd_core/src/catalogue.rs:44`. Storage test `an unspecified cache-write price persists exactly, changes and clears independently` at `seed.integration.test.ts:119`. No cache-write billing test; counters and charging remain absent. |
| 8.7 exact sub-cent amounts | partial | `scripts/model-catalogue/catalogue.ts:112`, BIGINT schema, precise display formatter. Decimal table at `catalogue.test.ts:149` covers every required decimal. `seed.integration.test.ts:86` proves exact storage. Extended usage settlement still needs the named billing proof. |
| 8.8 no double counting | not started | Current `afr_providers/src/turn.rs:250` subtracts cached reads only; cache creation is telemetry. `afd_wire/src/report/mod.rs:90` and `:135` still carry three counters. No new `test_usage_counts_each_token_once`. |
| 8.9 Ollama discovery, unknown price, ownership | partial | Native `afr_providers/src/wire.rs:60`; mock `test_native_ollama_streams_and_continues_the_exact_model_tag` at `tests/providers/native.rs:181`. `afd_credential/tests/ollama_endpoint_refusal.rs:6` refuses endpoint overrides; `:23` allows the named cloud route. Unknown-price UI tests exist. Tags/local-cloud metadata/customer ownership/no-daemon-dial proofs are absent. |
| 8.10 native Gemini signed continuation | done-in-code | `afr_providers/src/turn.rs:264`, `request/mod.rs:109`, `:157`; mocked `test_native_gemini_continues_tool_calls` at `tests/providers/native.rs:99`, plus `test_native_gemini_preserves_signed_answer_parts` at `:147`. No live inference; no port to main's inference API yet. |
| 8.11 price provenance and refusal | done-in-code | `schema/1305_model_platform_model_prices.sql:24`, `scripts/model-catalogue/catalogue.ts:30`; required-field table at `catalogue.test.ts:90`, missing-unit test at `:98`, official-source test at `curated.test.ts:25`. The seeder refuses missing fields; administrator API validation remains pending. |

## Indy's eight catalogue calls

| Call | Implemented location or exact remaining gap |
|---|---|
| Exact 17 curated entries plus advanced Terra | `scripts/model-catalogue/curated.ts:4`, `scripts/model-library-allowlist.json:5`; one `advanced` boolean at `schema/1300_model_platform_models.sql:15`. Complete input list; only six can seed. |
| Native Gemini, provider key `gemini`, display Google | `afr_providers/assets/providers.json:4`, `src/wire.rs:55`, `src/turn.rs:264`; UI provider labels in commit `0ee0ead84`. OpenRouter remains a separate Chat route at `providers.json:6`. |
| Native Ollama wire | `afr_providers/assets/providers.json:5`, `src/wire.rs:60`. Configured-server tags and local/cloud metadata are not implemented. |
| Cache writes reported and billed by lifetime | Rate columns at `schema/1305_model_platform_model_prices.sql:20`; shared rates at `afd_core/src/catalogue.rs:44`. Reporting, lifetime counters and billing are not implemented. |
| New `model.platform_model_prices` table | `schema/1305_model_platform_model_prices.sql:11`; why-many-row-table header at `:1`; registered at `afd_db/src/migration.rs:137`. Existing identity slot edited in place. |
| Delete pruned rows, including deprecated DeepSeek | `scripts/model-catalogue/seed.ts:45` and `sql/delete_models.sql:1`; DeepSeek V4 Pro 0813 is absent from `curated.ts:4`. No availability flags or preservation aliases added. |
| Exact context integers from provider models APIs | Evidence fields at `schema/1300_model_platform_models.sql:12`; URL/identity validation at `scripts/model-catalogue/catalogue.ts:94`; unproved rows filtered at `rows.ts:24` and reported at `catalogue.ts:131`. Twelve remain unseeded. |
| Runner-local exception, customer ownership, daemon never dials local | Not implemented. Indy explicitly kept local Ollama blocked pending ownership design. `schema/1800_runner_runners.sql:15` has no customer owner. Existing endpoint guards remain; refusal tests are not ownership proofs. |

Additional approved decisions: generic unspecified-lifetime cache-write rates stay separate from 5-minute and 1-hour rates.
Indy approved the exact `effective_from` / `effective_until` names as a narrow naming exception.
Indy left reporting design to Claude and the other agent, and local ownership to a further Indy decision.
These choices are recorded in the active spec's Discovery entries after the eight catalogue calls.

## Exact model-wire selection inventory

All paths in this table start at `rustd/crates/`. These are the current provider-key-based choosers Claude will replace.

| File:line | Current role |
|---|---|
| `agentsfleet_runner/src/host.rs:145` | Production bootstrap calls `Registry::builtin().and_then(Connector::new)` and supplies the connector to `afr_agent::Loop`. |
| `afr_providers/src/lib.rs:32` | Exposes `ProviderSpec`, `Registry`, `Wire`; no shared `ModelInference` selection exists here. |
| `afr_providers/assets/providers.json:2` | Anthropic → Messages; `:3` OpenAI → Responses; `:4` Gemini → Gemini; `:5` Ollama → Ollama; `:6` OpenRouter → Chat. Other named providers retain their existing Chat routes/dialects. |
| `afr_providers/src/registry/mod.rs:34` | Embeds providers.json. `:45` defines the five variants; `:62` chooses image support, including both new native wires. |
| `afr_providers/src/registry/mod.rs:73` | `ProviderSpec.wire` at `:80` selects a wire; `Route` at `:91` holds wire/base/dialect. |
| `afr_providers/src/registry/mod.rs:108` | Built-in JSON decode; `:116` builds routes; `:122` copies `spec.wire`; `:126` inserts existing names/aliases. |
| `afr_providers/src/registry/mod.rs:141` | Looks up named provider first. Custom provider URLs must pass HTTPS/host/credential/address checks and always select Chat at `:160`. They cannot select native Gemini/Ollama. |
| `afr_providers/src/connect/mod.rs:85` | `admit` resolves `policy.provider`; `:90` resolves again for `connect`; `:93` calls `wire::model`; `:94` passes `route.wire` into `Turns`. |
| `afr_providers/src/wire.rs:31` | `model` switches on `route.wire` at `:38`: Messages `:39`, Responses `:50`, Gemini `:55`, Ollama `:60`, Chat `:66`. Gemini uses rig GeminiConfig; Ollama uses native_completion with the exact model tag. |
| `afr_providers/src/request/mod.rs:37` | Builds completion requests. `:57` switches caching: Anthropic short retention, Responses prompt key, Chat/Gemini/Ollama no explicit cache-control. `hosted` at `:77` switches tools; native Gemini/Ollama receive no hosted web-search tool. |
| `afr_providers/src/request/mod.rs:109` | Maps assistant replay; `:112` restores origin. `assistant` at `:131` reuses unchanged native content; `call` at `:157` restores native tool-call identity/signature at `:170`. |
| `afr_providers/src/turn.rs:44` | `Turns` stores wire; constructor `:66`; `:93` selects request behavior; `:118` asks wire image support; `:199` passes wire to ending. |
| `afr_providers/src/turn.rs:250` | Ending maps usage. `:264` preserves all Gemini response parts, otherwise reasoning parts; `:267` stores replay content and origin. This is the Gemini-specific continuation chooser. |
| `afr_providers/src/provider.rs` | `Replay` carries rig content/origin between turns; `Usage` at `:142` still has only one unsplit telemetry cache-write counter. |
| `afr_providers/tests/providers/native.rs:20` | Mock registry chooses `ProviderSpec.wire`; Gemini cases at `:99`/`:147`, Ollama at `:181`. These are HTTP mocks, not live providers. |
| `afr_providers/tests/support/mod.rs:238` | Existing test connector builds ProviderSpec explicitly; `tests/providers/caching.rs:91` also chooses a Chat fixture. Registry/request/turn unit tests contain further explicit test-only Wire values. |

Source inventory used `rg` for `Registry::builtin`, `Connector::new`, `route.wire`, `registry.route`, and all five Wire variants.
`afr_supervisor/src/test_support/mod.rs:130` is another test-only connector bootstrap.
Rig 0.44.0 remains pinned in `rustd/Cargo.lock`; no dependency upgrade or handwritten native protocol was added.

## Schema and seeder changes

| File | Change |
|---|---|
| `schema/1300_model_platform_models.sql:6` | Identity only: UUIDv7 primary key, exact model_name/provider, display name, positive context integer with source/time, advanced, SQL-written created/updated instants, unique provider/model. Removes its three price columns. Runtime CRUD grants retained. |
| `schema/1305_model_platform_model_prices.sql:11` | New one-to-many prompt-tier/date price table. Header explains why this needs rows rather than more identity columns. Foreign key cascades identity deletion; runtime CRUD grants included. |
| `afd_db/src/migration.rs:137` | Registers slot 1305 in the existing clean-slate slot list; no forward-migration slot appended for an old layout. |
| `afd_db/src/test_util/platform_model.rs:17` and `sql/insert_platform_model.sql:1` | Shared `ModelSeed` builder supplies synthetic identity evidence and base prices. One statement preserves identifiers, updates values and rolls back identity on price failure. |
| `afd_db/src/test_util/mod.rs:74` | Exports ModelSeed; adds its module. Existing database lane/error/cleanup code was not otherwise repaired. |
| `afd_db/src/test_util/sql/insert_platform_default.sql:1` | Seeds separate identity and base-price rows before platform default insertion. Fixture timestamps come from SQL. |
| `afd_db/Cargo.toml` | Adds required timestamp/derive and test-process features for typed helper and native Bun seeder proof. |
| `afd_db/tests/db_suite.rs:32` / `:39` | Registers `integration_catalogue_seeder` and `integration_platform_model`. |
| `afd_db/tests/integration_platform_model.rs:33` / `:81` | Stable identifiers/exact rates and atomic refusal/source/retry proofs; dedicated read.sql and delete.sql. |
| `afd_db/tests/integration_catalogue_seeder.rs:18` | Runs the Bun seeder suite against a private database created by the canonical registered migrator. |
| `afd_db/tests/integration_platform_default.rs` | Adapts existing default-fixture assertions to identity/price separation. |
| `afd_db/tests/integration_migrate/{shape,roles,instants}.rs` | Expected slot count 47 → 48, price-table grants, and exactly two approved instant-name exceptions. Both exceptions still require TIMESTAMPTZ(3). |
| `scripts/model-library-allowlist.json:1` | Replaces provider-wide input with 18 exact identities and 24 price rows; only six have readable exact contexts. Missing contexts carry reasons, not guessed numbers. |
| `scripts/model-catalogue/{catalogue,curated,rows,seed,command}.ts` and `sql/*.sql` | Bounded input, exact decimals, read-only diff by default, explicit transactional apply, prune and generation update; idempotent no-op preserves clocks/identifiers. |
| `scripts/seed-models.mjs` and `scripts/model-catalogue/check.mjs` | Bun entrypoints replace stale core-table/epoch writes. `scripts/check_model_allowlist{,_test}.py` are deleted; their refusal cases moved to Bun tests. |
| `playbooks/operations/model_catalogue/{00_gate,01_diff,02_apply,03_verify}.sh` and playbook | Use Bun/model.* with explicit apply approval; redact credentials and require nonempty verified state. See exact filenames in `git show dcec10c58 --stat`. |
| `make/{dev,quality}.mk`, root `package.json`, `bun.lock` | Adapt callers to the bounded validator and pin decimal.js 10.6.0 for exact conversion; no global dependency installed. |

Only schema slots 1300 and 1305 changed relative to the supplied base (`git diff --name-only 53e3d093e HEAD -- schema`).
No DEFAULT/CHECK vocabulary strings were added; UUIDv7's existing `'7'` shape check remains.

Price columns, exactly: `id UUID`; `platform_model_id UUID`; `effective_from TIMESTAMPTZ(3)`;
nullable `effective_until TIMESTAMPTZ(3)`; `prompt_tokens_above INTEGER`; `input_nanos_per_mtok BIGINT`;
`cached_input_nanos_per_mtok BIGINT`; nullable `cache_write_nanos_per_mtok BIGINT`;
nullable `cache_write_5m_nanos_per_mtok BIGINT`; nullable `cache_write_1h_nanos_per_mtok BIGINT`;
`output_nanos_per_mtok BIGINT`; `source_url TEXT`; `verified_at TIMESTAMPTZ(3)`; `conditions TEXT`;
`created_at TIMESTAMPTZ(3)`; `updated_at TIMESTAMPTZ(3)`.
Unique key: `(platform_model_id, prompt_tokens_above, effective_from)`; end must follow start; thresholds/rates cannot be negative.
The source document declares the price unit; the SQL column names and header fix it to nanodollars per million tokens.

Decimal strings are multiplied by exactly 1,000,000,000 using decimal.js, then bound as integer strings to BIGINT.
For example, `0.006 → 6000000`, `0.015 → 15000000`, `0.0375 → 37500000`, `0.0625 → 62500000`,
`0.075 → 75000000`, `0.125 → 125000000`, `0.1875 → 187500000` nanos per million tokens.
No JavaScript floating-point amount enters seeder storage. This does not repair the pending administrator forms or settlement.

### Every price-table query

Inventory: `git grep -n 'model.platform_model_prices' -- rustd scripts playbooks schema`.
The following lists include runtime statements, test statements, cascades and the operational verifier.

| Path:line | Read/write |
|---|---|
| `scripts/model-catalogue/sql/select_rows.sql:19`, `upsert_prices.sql:1`, `delete_prices.sql:1` | Read full saved schedule; upsert exact dated/tiered quotes; prune obsolete quotes. `delete_models.sql:1` deletes identities and cascades prices. |
| `rustd/crates/afd_admin/src/sql/list_platform_models.sql:17`, `upsert_platform_model_prices.sql:1`, `delete_platform_model_prices.sql:1` | Read full schedules; atomic administrator replacement upserts retained keys and prunes removed keys. Model deletion cascades prices. |
| `rustd/crates/afd_credential/src/provider/entries/sql/select_rates_for_pairs.sql:7` | Read one currently effective base quote and identity evidence per exact provider/model pair. |
| `rustd/crates/afd_credential/src/provider/registered/sql/platform.sql:31`, `entry.sql:34` | Read current base quote after catalogue locks; fingerprint includes quote identity/rates/generation. |
| `rustd/crates/afd_billing/src/sql/select_platform_model_prices.sql:3` | Read the current base quote's three rates only. Does not implement per-request tier/date settlement. |
| `rustd/crates/afd_tenant/src/sql/models/select_library_page_first.sql:10` and `:26`; `select_library_page_after.sql:10` and `:26` | Read current base quote plus all ordered schedules and their modification stamp. |
| `rustd/crates/afd_authoring/src/catalogue/sql/page.sql:34` | Read a current base quote for eligibility and labels. |
| `rustd/crates/afd_db/src/test_util/sql/insert_platform_model.sql:12`; `insert_platform_default.sql:10` and `:19` | Shared fixture upsert; default fixture writes a missing base quote and checks existence. |
| `rustd/crates/afd_db/tests/integration_platform_model/read.sql:5`; sibling `delete.sql:1` | Read saved exact rates/identifiers; delete fixture identity with cascade. |
| `rustd/crates/afd_admin/tests/admin_model/proofs.rs:114`; `afd_authoring/tests/draft/admission/obligations.rs:89`; `afd_authoring/tests/draft/model_calls/replay.rs:61` | Read retained price IDs; alter prices for admission and replay regressions. The last two abbreviated paths start at `rustd/crates/`. |
| `scripts/model-catalogue/seed.integration.test.ts:69,95,108,116,124,129,131,149` | Direct read assertions for provenance, precision, pruning, omissions, nullable writes and contention. |
| `scripts/model-catalogue/quotes.integration.test.ts:52`; `integration-fixtures.ts:24` and `:43`; `seed-failure.integration.test.ts:9` | Price-only update, complete-state snapshot and generated fault triggers for insert/delete/deferred commit. |
| `playbooks/operations/model_catalogue/03_verify.sh:22` | Read existence of an effective base quote when checking seeded state. |

Generation locking/bumping uses `scripts/model-catalogue/sql/{lock_version,bump_version}.sql` and administrator version SQL, not the price table.
Schema DDL and migration grant/name assertions are listed above; they are not additional runtime price reads.

## Known state without another build

- Expected integration compile failures: `afd_api_operator/src/handler/admin/models.rs:252` still names removed `afd_admin::ModelRates`; `:280` constructs it. It must use `ModelUpdate` instead. Its item mapper still reads removed flat fields.
- `afd_api_tenant/src/handler/tenant/models/mod.rs:142` still calls the old Models::page signature. Cursor construction at `:164` lacks the new advanced field. Response mappers do not carry the new metadata/prices.
- The last full builds stopped earlier in `afd_http`: 18 missing `afd_core::clock::UnixMillis` imports and one private `afd_billing::Wallet` import. Historical logs below contain all 19 errors. Further API errors may be masked.
- Current billing charge at `afd_billing/src/nanos.rs:150` still uses three rates/counters. `afd_fleet/src/lease/sql/meter_lease.sql` and `afd_authoring/src/model/budget.rs:53` remain aggregate pricing. Price lookup tests do not close these gaps.
- `afr_providers/src/turn.rs:257` subtracts cache reads but not writes from prompt input. `:259` is one unsplit cache-write telemetry count. `afr_providers/src/provider.rs:142` aggregates usage across a run. Do not claim usage conservation.
- No feature stubs or intentional `todo!` / `unimplemented!` were introduced in the completed runtime edits. Missing features remain absent and are enumerated here; no placeholder implementation claims success.
- All newly added tests ran before takeover except the final mechanical tenant-test assertions and curated input-type cleanup. They have not been rerun. The hook checks conform, not their compilation or behavior.
- Last tenant test edits: `afd_tenant/tests/integration_model_catalogue.rs:100` onward replaced indexing/empty/result assertions to address test lint. Final `curated.ts:19` uses the Provider type to remove a duplicated vocabulary literal. Format/lint status after those edits is unverified.
- No new administrator editor feature was started. Its temporary unused app dependency addition was removed before committing. Root decimal.js remains necessary for the seeder; app package.json has no new dependency.
- No final review was run. An earlier review skill read did not produce findings or an approval. Indy assigned review, test lint and coverage repair to Claude.
- Deliberately uncommitted files: none intended. The final `git status --porcelain` output is authoritative; any exception discovered by the last hook must be listed before stopping.

Changes outside the narrow §8-specific rows in the spec's Files Changed table are explicit:
`docs/architecture/{billing_and_provider_keys,postgres_schemas}.md`; `afd_db/src/migration.rs` and `src/test_util/**`;
`afd_tenant/src/sql/models.rs` and its page statements; `afd_billing/src/sql/select_platform_model_prices.sql`;
authoring/billing/credential/fleet service test fixtures. They use the wider table's architecture, migration, SQL and fixture scope.
`HANDOFF_M229_CATALOGUE.md` is specifically authorized by Indy's takeover prompt. No protected-path edits were made.
The expanded §8 table already names root manifests, core metadata, tenant models, transport tests and metric census changes.

## Protected-path integration handover

These files remain unchanged on this branch. Line anchors refer to this supplied-base worktree, not Claude's evolving branch.
The detailed companion inventory is [docs/reviews/model-catalogue/handover.md](docs/reviews/model-catalogue/handover.md).

| File:line, under `rustd/crates/` unless stated | Exact change Claude needs |
|---|---|
| `afd_api_wire/src/models.rs:23` | Extend CatalogueModel with display_name, advanced, context source/time and ordered full prices. Use decimal-string wire money where i64 precision requires it; current-base summaries cannot price a request. |
| `afd_api_wire/src/admin_catalogue.rs:112`, `:156`, `:175` | Replace flat ModelRates/create/item shapes with validated metadata and price arrays matching `afd_admin::ModelUpdate { metadata, prices }` and `ModelInput.details`. Enforce garde bounds, dates, nonoverlapping tiers, current base, curated identity and advanced-Terra rules. |
| `afd_http/src/openapi/query.rs:107` | Add strict advanced boolean query, default false, to ModelEntryFilter. |
| `afd_api_tenant/src/handler/tenant/models/input.rs:29`, `:76` | Parse advanced with existing strict query rules; refuse duplicates/invalid encodings; bind cursor provider and advanced filter. |
| `afd_api_tenant/src/handler/tenant/models/mod.rs:96`, `:142`, `:164`, `:186` | Call Models::page(provider, advanced, boundary, limit), populate Cursor.advanced and map complete metadata/prices. Include advanced and complete representation in entity tags and conditional responses. |
| `afd_api_operator/src/handler/admin/models.rs:68`, `:104`, `:164`, `:252`, `:280`, `:289` | Adapt list/create/update parsing and rendering to ModelUpdate/ModelInput.details/Model.details. Validate before service calls. Preserve duplicate/not-found/in-use outcomes; malformed replacement changes no identity, price or generation. |
| `afd_api_wire/src/tenant_model_entry.rs:45`, `:89`, `:110` | Add CatalogueRate.metadata to registry/default representations. Use catalogue display label; retain exact identifier for custom entries without metadata. Keep Ollama pricing unknown. |
| `afd_api_tenant/src/handler/tenant/model_entry/render.rs:59`, `:84`, `:128` | Render metadata display names; update literals and Clone handling because CatalogueRate is no longer Copy. |
| `afd_api_runner/src/handler/runner/report.rs:85`, `:100`, `:119` | Follow Indy's still-pending per-request usage/time/deduplication decision. Carry disjoint uncached/read/generic-write/5-minute-write/1-hour-write/output counts into settlement and telemetry. Aggregate run totals cannot reconstruct request tiers/date changes. |
| `afd_api_runner/src/handler/runner/authoring/report.rs:31`, `:39`, `:43` | Follow the same reporting decision for authoring, strict validation and charging. No shared report payload was invented by Codex. |
| `afd_api_runner/src/handler/runner/enrolment.rs:86`, `:98` | After Indy settles ownership, connect customer-owned assignment and placement checks. Keep local blocked until then; the daemon never dials it. Cloud retains HTTPS and full Server-Side Request Forgery (SSRF) protection. |
| `afd_bench/src/lane/lease/drain/platform.rs:76`, `:134`, `:166`, `:173` | Replace removed core model/default fixture inserts and cleanup with model.platform_models, platform_model_prices and platform_defaults. Reuse afd_db::test_util::ModelSeed. |
| `agentsfleetd/src/plane/services.rs:221` | Wire the updated catalogue services/signatures; avoid a second rate adapter or cache. |
| `public/openapi.json:604`, `:3858`, `:18212`, `:22443`, `:24194`, `:26525` | Regenerate from owning Rust annotations after model/admin/query/report types are settled. These anchors locate admin paths, catalogue, CatalogueModel, registry row, Report and Usage. Do not hand-edit generated JSON. |

`afd_http/src/openapi/mod.rs` contains common constants, not the catalogue schema registry.
Runner report types outside the protected list, `afd_wire/src/report/mod.rs:90` and `:135`, also still need the chosen accounting shape.
Claude owns the new inference-API port; the chooser inventory above is the exact starting point.

## Open design questions and decisions

| Question | Evidence / status |
|---|---|
| How should per-request usage, time and deduplication be reported and settled? | `afd_wire/src/report/mod.rs:90`, `:135`; `afr_providers/src/provider.rs:142`; `afd_api_runner/src/handler/runner/report.rs:100`. Indy explicitly left this design to Claude/the other agent. Not implemented. |
| How is customer ownership assigned and enforced for local Ollama? | `schema/1800_runner_runners.sql:15`; enrolment at `afd_api_runner/src/handler/runner/enrolment.rs:86`. Indy kept local blocked until choosing ownership. A local marker alone grants no authority. |
| Which token counting and transport should authoring use for the curated set? | `afd_authoring/src/model/mod.rs:75`, `:84`; `model/provider/network.rs:8`; `model/budget.rs:53`. No approved strategy; existing tokenizer recognition and OpenAI-only network are insufficient. |
| Can the remaining provider contexts be read exactly? | Null evidence/reasons for 12 entries in `scripts/model-library-allowlist.json`; omission at `scripts/model-catalogue/rows.ts:24`. Needs authenticated provider models responses, not a guessed integer. |
| How are schedules and exact i64 amounts exposed in the final HTTP body? | Service structs at `afd_core/src/catalogue.rs:12`, `:32`; pending API types at `afd_api_wire/src/admin_catalogue.rs:112`. Wire conversion/validation belongs to Claude; no final public shape approved here. |
| Did Codex make another unsettled product decision? | No new reporting, authoring or ownership design was chosen. Approved exceptions are the generic cache-write lifetime and exact two timestamp names. Synthetic fixture evidence URLs are tests only, not real context proofs. |

The final curated catalogue, display names, all 24 quote rows and provider URLs are in
[docs/reviews/model-catalogue/catalogue.md](docs/reviews/model-catalogue/catalogue.md), backed by `scripts/model-library-allowlist.json`.
Six exact contexts are proved: three Fireworks identifiers at 1,048,576; OpenRouter Muse Wonder and Gemini at 1,048,576;
OpenRouter Qwen3.8 Max 0902 at 1,000,000. Remaining entries are not seeded.
The 12 missing proofs are Anthropic Fable/Opus/Sonnet/Haiku, OpenAI Sol/Astra/Luna/Terra, Groq OSS 120B,
Mistral Large 4/Small 4, and native Gemini 4 Argon. Their exact API IDs are in that table.
Re-verification difference: OpenRouter Muse Wonder is 1,048,576, while #771 states 1,000,000.
Fireworks marketing “1040k” was not converted; the recorded provider models responses supply the integers.
OpenRouter's public models endpoint was read again and agreed; later unauthenticated Fireworks requests returned 401 and supplied no replacement proof.
No other quoted-price disagreement was recorded. Native Google December rows remain source data, not settled billing.
OpenAI Batch/Flex/Fast/Ultrafast and regional modifiers, and Anthropic's US-only 1.1× modifier, remain outside scope.

## Historical verification; nothing rerun after takeover

Logs remain at `/tmp/agentsfleet-771-xXFr5avp/` on this machine. These measurements predate the last mechanical edits.
They describe their recorded inputs, not a newly built final handoff candidate. No new test/lint/coverage/review was launched during takeover.
The normal hook ran its required conform and scoped playbook checks, as Indy explicitly allowed.

| Required command | Recorded result and decisive output | Log under the directory above |
|---|---|---|
| `make harness-verify` | Earlier full run failed the duplicated provider literal. The last TypeScript type edit addressed it without a full rerun. Normal staged hooks now report `ALL GATES GREEN`: catalogue/schema 24 source files, services 27, telemetry 1, docs no source. This is staged conform evidence only. | `make-harness-verify-second.log`; `handoff-commit-{catalogue,services,telemetry,docs}.log` |
| `make lint-all` | FAIL: `✗ [rustd] rustfmt --check failed after 2s (exit 1)`. Recorded differences in unchanged `afd_crypto/src/entropy.rs` and `tests/entropy_uuid7.rs`. Scoped Clippy also had tenant test-lint failures before the last unrerun assertion edits. | `make-lint-all-owned-final.log`; `catalogue-owned-clippy-second.log` |
| `make test-unit-all` | FAIL: runner subtotal 1,292 passed; daemon then `error: could not compile afd_http (lib) due to 19 previous errors; 1 warning emitted`, followed by `unit: daemon failed (exit 101)`. | `make-test-unit-all-current.log` |
| `make test-coverage-all` | PASS: `✓ All package coverage gates passed`. App 3,463 tests/376 files; website 134/20; command-line interface (CLI) 1,697 pass/16 skipped; design 495/62. Package line/function coverage gates reached 100%. These gates do not establish seeder or whole-file Rust coverage. | `make-test-coverage-all-green.log` |
| `KEEP_TEST_STATE=1 make test-integration-rustd` | FAIL before integration tests: same 19 afd_http compile errors, then `✗ [infra] migrate failed`. The owned local datastore lane remained healthy and was not reset. | `make-test-integration-rustd-current.log` |
| `make check-version` | PASS: `✓ all versions match 0.61.0`. | `make-check-version-current.log` |
| Required review | NOT RUN. Indy explicitly transferred review to Claude after integration. There are no review findings or approval to claim. | None |

Affected-crate inner-loop results are narrower evidence, not replacements for failed make targets:

- `catalogue-owned-unit-clean.log`: 747 Rust tests passed, zero failed, 499 ignored across the affected crates; sum of its `test result` rows.
- `catalogue-owned-integration-clean-{9,10,11}.log`: each run passed 499 real-service Rust tests, zero failed, against the same Postgres 19rc1 + Dragonfly 2.0.2 lane, without reset.
- Each real-service run also reports `80 pass`, `0 fail`, `Ran 80 tests across 7 files` from Bun: 61 pure tests and 19 native Postgres seeder/quote/failure tests.
- Real Rust integration subtotal per run: administrator 6, authoring 109, billing 14, credential 29, database 37, fleet 239, tenant 65. Fleet includes one exclusive test plus 238 other tests.
- Provider subset in the unit log: 56 unit tests and 34 mocked HTTP transport tests passed. Both new wires' request construction, streamed calls and continuation use mocks. No live model inference was performed for #771.
- Telemetry subset: 41 tests passed. Operations hook reports model-catalogue shell tests `9 passed, 0 failed` and credential redaction `1 passed, 0 failed`.
- Seeder faults cover identity upsert, price upsert, price pruning, identity pruning, generation update and deferred COMMIT refusal, with snapshot equality and healing retry. They do not cover lost commit acknowledgement.
- Contention proof is 100 operations through eight one-connection pools, not 100 independent connections. Randomized ordering, full administrator boundary injection, pinned latency, complexity counters and mutation testing remain missing; see the test ledger.

Historical whole-file executable line coverage, from `catalogue-owned-clean.lcov` and `catalogue-owned-clean-grades.log`:

| Touched measured Rust file, under `rustd/crates/` | Covered / executable lines | Missed lines |
|---|---|---|
| `afd_admin/src/model.rs` | 108/108, 100% | None |
| `afd_authoring/src/catalogue/project.rs` | 31/31, 100% | None |
| `afd_credential/src/provider/entries/mod.rs` | 50/50, 100% | None |
| `afd_credential/src/provider/entries/page.rs` | 140/141, 99.2908% | 163 |
| `afd_db/src/migration.rs` | 34/37, 91.8919% | 55,56,57 |
| `afd_db/src/test_util/mod.rs` | 94/102, 92.1569% | 197,198,199,228,231,251,255,319 |
| `afd_db/src/test_util/platform_model.rs` | 37/37, 100% | None |
| `afd_tenant/src/models/cursor.rs` | 32/32, 100% | None |
| `afd_tenant/src/models/mod.rs` | 78/78, 100% | None |
| `afr_providers/src/registry/mod.rs` | 67/67, 100% | None |
| `afr_providers/src/request/mod.rs` | 139/139, 100% | None |
| `afr_providers/src/turn.rs` | 130/130, 100% | None |
| `afr_providers/src/wire.rs` | 44/44, 100% | None |

Decisive grader line: `2 of 13 files the patch touches are < 99%`.
Unhit migration accessor and database test-helper failure/cleanup paths remain Claude's coverage work.
These measurements do not assign fictitious coverage to serde-only or module-export files without measured executable lines.

## Preserved local state

The owned #771 datastore lane was not reset or stopped: Postgres port 28244, Dragonfly cluster ports 28247–28251.
Other reserved lane ports: QStash 28246, API 28252, Mailpit 28253/28254. A reserved API port does not mean the integrated daemon runs.
Ignored local tools and coverage files remain in this worktree's `.gstack/`; no credentials are included in this handoff.
No paid #771 model call was made. The earlier #754 journey already used its approved private message.
The #754 frontend process at localhost:3000 was left alone; integrated #771 browser signoff remains pending.
Claude receives local branch commits only. Codex stops after reporting the handoff commit, complete branch log and final worktree status.
