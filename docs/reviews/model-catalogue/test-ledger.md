---
type: explanation
audience: contributor
verified: 2026-10-18
product_version: 0.61.0
executable: false
---

# Catalogue required-test ledger

## What it is

The ledger traces the diff against `53e3d093e`, including new source, to behavior and failure proofs.
This money, migration and streaming work uses the repository's unit/integration testing skills in hardening mode.
Completed local proofs are distinguished from required integration that remains blocked. A passing lookup helper is not a billing proof.

## Why it exists

The source split spans catalogue operations, runner transport, SQL readers and UI. Each boundary needs its own evidence.
Measured commands, counts and remaining gates are in the [takeover handoff](../../../HANDOFF_M229_CATALOGUE.md); exact tests below identify the behavior being asserted.

## How it behaves

| Changed surface or public unit | Required behavior / failure | Evidence and status |
|---|---|---|
| `catalogue.ts`: schemas / `parseCatalogue` | Bounded complete identity/evidence shape; context proof XOR reason; exact served model URL; sole advanced Terra; duplicate identities; missing price source/time/dates/conditions; invalid decimal; reversed/overlapping date ranges | `catalogue.test.ts`, `curated.test.ts`: local PASS. Unverified context is reported and omitted rather than guessed. |
| `catalogue.ts`: `nanosPerMtok` | Exact decimal strings through signed-64-bit nanos; 0.006, 0.015, 0.0375, 0.0625, 0.075, 0.125, 0.1875, one nano and maximum signed integer; reject zero, overflow, malformed or excessive precision | `catalogue.test.ts` decimal table: local PASS. Red/green amount evidence retained in `catalogue-money-{red,green}.log`. This proves conversion, not settlement. |
| `catalogue.ts`: `priceForRequest` | Whole quote changes strictly above 272000 / 100000; inclusive-start/exclusive-end dates; UTC midnight and offset equivalence; invalid time/token values; missing quote | `catalogue.test.ts`: local PASS. Provider usage aggregation and charged amounts remain pending. |
| `catalogue.ts`: `readCatalogue` / `catalogueReport` | Oversized, malformed or missing input cannot become an empty apply; omitted identities named safely | `catalogue.test.ts`, `command.test.ts`: local PASS. |
| `curated.ts`: exact set, official hosts, advanced identity | Exactly 17 regular plus Terra; native/routed Gemini distinct; missing/extra/duplicate/China-source/sentinel/cached-above-input refused | `curated.test.ts`: local PASS; canonical `_model_allowlist_check` also runs these bounds. |
| `rows.ts`: `desiredRows`, `catalogueDiff`, `hasChanges`, database-row schema | Only evidenced contexts become rows; exact nanos; nullable independent cache-write categories; canonical price order; additions, removals and price-only edits; malformed stored rows refused | `rows.test.ts`: local PASS. |
| `command.ts`: `runSeedCommand` | Diff by default; explicit apply; unknown/repeated flags and unavailable database refused; close connection on success/failure; never print a credential or invalid field value | `command.test.ts`, real command test in `seed.integration.test.ts`, `seeder_secrets_test.sh`: local PASS. |
| `seed.ts`: `openCatalogueDatabase` / `seedCatalogue` / `seedReport` | Empty diff is read-only; identity, all prices, deletion and generation one transaction; unchanged apply preserves IDs/clocks; complete curated document seeds only six proved contexts; return exact diff | `seed.integration.test.ts`: real Postgres proofs. The complete-document test uses the actual allowlist, not just a synthetic subset. |
| Seeder write sequence and retries | Failure at each write and deferred COMMIT leaves the whole pre-call snapshot/generation; removing failure and retrying heals once | `seed-failure.integration.test.ts`: real Postgres triggers; completed matrix below. |
| Seeder contention and connection release | 100 simultaneous identical applies commit one generation/identity, no client leak; a negative leak witness fails | `seed.integration.test.ts`: local PASS with eight one-connection pools. This is 100 operations, not 100 database connections. |
| `schema/1300`, `schema/1305`, migration registration | Clean schema has identity/price separation, UUIDv7, exact 64-bit rates, grants, date/tier checks and registered slot; exact two approved timestamp-name exceptions, still precision 3 | `afd_db` migrator shape/roles/instants suites and native seeder integration: real Postgres PASS. |
| `afd_core::catalogue::{Metadata,Price}` | Shared exact typed fields serialize into administrator SQL and deserialize from tenant/registry projections; no extra copied vocabulary | Real administrator / tenant / registry tests exercise serialization and typed decoding. Whole-file measurements are in verification. |
| `afd_admin::Models::{new,list,create,update,delete}` / typed row / price replacement | Full metadata and tiers; duplicate/not-found/in-use outcomes; retained price ID; prune removed tiers; failed price update/create rolls back identity and generation; two simultaneous creates give one row/version | `integration_admin.rs`, `admin_model/proofs.rs`: six real Postgres tests PASS. Exhaustive transaction-boundary injection remains a gap below. |
| `afd_tenant::Models::page` / `fetch` / typed projection | Exact metadata and all prices; first/next/empty pages; price-only stamp change; provider filtering; advanced hidden unless requested; query, closed-pool and unreadable context evidence failures preserve faults | `integration_model_catalogue.rs` and `quotes.integration.test.ts`: real Postgres PASS. |
| Tenant `Cursor` / `render` / `parse` | Advanced value round-trips in strict cursor representation; no aliases | Cursor unit tests PASS. HTTP filter mismatch and entity-tag behavior remain pending in the protected API paths. |
| Credential `CatalogueRate` / registry page | Catalogue display/evidence comes from database; current base quote selected once for exact pair; expired/future/tier rows do not multiply a registry row; missing credential remains degraded | `integration_rotation/registry_page.rs` plus real quote tests PASS. |
| Credential registered-model entry/default reads | Price table supplies base quote and fingerprint; catalogue row/generation locks precede SQL observation time; provider/secret/reference invalidation behavior retained | `afd_authoring` real review/admission/model-call suites PASS. These reads are not full tier/date billing. |
| `afd_authoring::catalogue::project::eligible` | Display label comes from catalogue; eligibility remains the real dispatch predicate; invalid rows omitted | `project/tests.rs`: local unit PASS. Curated authoring eligibility is a pending design blocker, not proven by these synthetic rows. |
| Billing base-quote statement | Current effective base quote only, exact provider/model pair; no removed identity-price columns | Existing `afd_billing` real integration tests PASS using shared model fixtures. Cache-write and context-tier settlement remain pending. |
| `afd_db::test_util::ModelSeed::{new,rates,verified_at,insert}` | Shared synthetic evidence and base quote; stable model/price IDs on upsert; exact changed rates/context evidence; failed quote rolls identity back and retains SQL source; retry heals | `integration_platform_model.rs`: two real Postgres proofs. Authoring, billing, credential and fleet fixtures reuse this helper. |
| Updated authoring / billing / fleet fixtures | Every test uses the registered identity/price tables; monetary regression scenarios retain their specific assertions | Affected real-service suites run against the same Postgres 19rc1 + Dragonfly lane without resetting it. Counts in verification. |
| Provider registry / `Wire` / native completion | Native `gemini` and `ollama` identified separately from OpenRouter; all registry routes construct a supported rig client | Provider unit tests and mocked provider integration PASS. |
| Provider request construction | Preserve signed Gemini text/reasoning/tool-call parts; native Ollama uses exact supplied server tag; omit Gemini cache control; keep Anthropic markers | Provider request/turn/caching tests PASS. |
| Provider streaming / continuation | Incremental tool calls and tool results across two native turns; thought signatures round-trip; redirects, malformed output, truncated streams, retries and hosted-tool refusals keep existing guards | `afr_providers/tests/providers/native.rs` and existing provider cases: mocked HTTP transport PASS. No live model inference. |
| Ollama endpoint refusal | No local/private/plain-HTTP override gains an exception; named cloud route remains allowed without an override | `ollama_endpoint_refusal.rs`: pure public-boundary tests. Local/cloud metadata, tag discovery, placement ownership and no-daemon-dial end-to-end proofs remain blocked. |
| Telemetry provider vocabulary | Gemini/Ollama appear in closed labels and census cardinalities agree | `afr_telemetry`: 41 local tests PASS. |
| Catalogue/UI label and pricing functions | Display names from catalogue; exact ID for a custom entry without a label; Google label; all supported decimal places; unknown Ollama price stays unknown including a supplied zero | `display.test.ts`, registry price/cell tests: canonical package coverage PASS. |
| Catalogue provider, selector and server actions | Advanced request explicit and default false; retain selections while loading/retrying; pagination/error and custom input behavior | Catalogue/selector/action tests: canonical package coverage PASS. Protected API still needs the query. |
| Administrator list label | Database display name and advanced badge rather than ID heuristics | Administrator UI tests PASS. Full metadata/tier mutation form remains pending, as recorded in handover. |
| Ops gate / diff / apply / verify and make callers | Bun commands, `model.*` names, explicit mutation approval; APPLY=0 is diff; no database credential in diagnostics | Shell guard tests (9), credential-redaction test (1), shellcheck error-severity pass and dry make output retained. |

The seeder's failure matrix selects a table/operation trigger within its own private database. Each attempt first seeds the same
pre-call snapshot; the refused statement emits SQLSTATE `P0001`, snapshot equality includes all identity/price/version fields,
then the trigger is removed and the same request commits one generation. No remote model call occurs.

| Workflow order | Boundary order | Interaction / injection | Residual state and next read | Retry |
|---|---|---|---|---|
| 1 | 1 | Identity upsert: BEFORE INSERT trigger | Previous identities, prices and generation unchanged | Complete replacement commits once |
| 2 | 2 | Price upsert: BEFORE INSERT trigger after identity work | Identity work rolls back with all price/version effects | Complete replacement commits once |
| 3 | 3 | Price prune: BEFORE DELETE trigger | New quote/identity work rolls back; no pruned price survives partially | Complete replacement commits once |
| 4 | 4 | Identity prune: BEFORE DELETE trigger | All earlier write work rolls back; no orphan prices | Complete replacement commits once |
| 5 | 5 | Generation update: BEFORE UPDATE trigger | Complete write set rolls back; caller still sees previous generation | Complete replacement commits once |
| 6 | 6 | COMMIT: deferred constraint trigger after price insert | Transaction refuses before durable commit; complete previous snapshot remains | Complete replacement commits once |

## Limits

The following are required gaps, not waived checks:

- **Needs integration:** per-request token/time reporting and daemon deduplication; prompt totals counting cache reads and all writes; whole-request tier prices; UTC date selection at settlement; cache writes and reasoning billed once. Indy deferred the reporting design to the other agent. Lookup/conversion tests cannot close Dimensions 8.4–8.8.
- **Needs design:** curated authoring tokenizer/counting and provider dispatch; local Ollama customer-owned placement. The current native transport and endpoint refusal tests do not settle those designs.
- **Needs API/UI integration:** authenticated picker/registry/administrator representations, advanced cursor/entity-tag validation, strict garde mutation refusals, complete price-array editor and OpenAPI generation.
- **Needs transport failure proof:** commit acknowledgement loss after a successful remote database commit. Deferred-COMMIT refusal proves rollback, not acknowledgement loss. Repeated successful apply proves idempotency but does not replace that failure injection.
- **Needs test hardening:** exhaustive administrator transaction-boundary failures, randomized suite order, deterministic complexity counters and pinned latency budget, a 100-distinct-connection contention test, and diff-scoped mutation testing. The measured 100-operation/eight-pool test must not be relabeled.
- **Needs full repository gates:** daemon/API compilation and unrelated base formatting prevent full canonical verification on the isolated branch. Package tests cannot fill that gap.
- **Not applicable:** Zig allocation proofs; the changed implementation is Rust, TypeScript and SQL. Live inference was not authorized beyond #754's already-spent single message.

These entries keep the testing skills' completion checklist open. No production-readiness or §8 completion claim follows from this ledger.

## Related pages

- [Verification](../../../HANDOFF_M229_CATALOGUE.md)
- [Exact API handover and design blockers](handover.md)
- [Provider catalogue and source matrix](catalogue.md)
