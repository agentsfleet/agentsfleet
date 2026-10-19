---
type: explanation
audience: contributor
verified: 2026-10-18
product_version: 0.61.0
executable: false
---

# Catalogue integration handover

## What it is

This is the exact remaining integration list for M229 §8, issue [#771](https://github.com/agentsfleet/agentsfleet/issues/771).
The model-catalogue worktree does not edit the other agent's HTTP, API-plane, daemon, benchmark or generated OpenAPI files.
Line numbers below refer to the pinned base `53e3d093e`; find the named symbol after the other agent's changes land.

## Why it exists

The owned service signatures now carry catalogue evidence and dated prices. Their API callers must move with them.
The full daemon does not build independently on this branch. These entries are required integration work, not completed features.

## How it behaves

Shared source types are `afd_core::catalogue::{Metadata, Price}`. `Metadata` has `display_name`, `context_cap_tokens`,
`context_source_url`, `context_verified_at` and `advanced`. `Price` has exact `effective_from` / `effective_until`,
`prompt_tokens_above`, input, cached read, unspecified-lifetime cache write, 5-minute cache write, 1-hour cache write,
output, `source_url`, `verified_at` and `conditions`. All rates are integer nanodollars per million tokens.
Unknown write rates stay optional. Transport rates should be decimal integer strings, never floating-point currency values.

| Owner file and pinned line | Exact required change |
|---|---|
| `rustd/crates/afd_api_wire/src/models.rs:23` (`CatalogueModel`) | Add display name, advanced flag, context source/time and the full ordered price array. Render `LibraryRow.metadata` and `LibraryRow.prices`. Existing three-price summary fields, if retained for the picker, are derived current base quotes; they cannot price a request. Serialize integer nanos without losing precision. |
| `rustd/crates/afd_api_wire/src/admin_catalogue.rs:112` (`ModelRates`), `:156` (`AdminModelCreate`), `:175` (`AdminModelItem`) | Replace the three-price mutation body with identity metadata plus a complete dated/tiered price set. The service exports `ModelUpdate { metadata, prices }`; `ModelInput` and returned `Model` contain `details: ModelUpdate`. There is no service `ModelRates`. Bound untrusted fields through garde: nonempty bounded names, official HTTPS provenance, positive context, at most 16 prices, nonnegative signed-64-bit nanos, nonnegative thresholds, valid ordered date ranges, exactly one current base quote, no duplicate or overlapping ranges within a tier. Refuse identities outside the curated set and advanced flags other than Terra. Do not accept old mutation aliases. |
| `rustd/crates/afd_http/src/openapi/query.rs:107` (`ModelEntryFilter`) | Add optional boolean `advanced`, default false, to the tenant catalogue query and its documentation. |
| `rustd/crates/afd_api_tenant/src/handler/tenant/models/input.rs:29` (`ProviderFilter`), `:76` (`parse_cursor`) | Parse a strict boolean advanced parameter with the existing duplicate/escape refusal machinery. Pass it into cursor validation. Reject a cursor whose advanced value disagrees with the request, just as provider/limit mismatches are refused. |
| `rustd/crates/afd_api_tenant/src/handler/tenant/models/mod.rs:96` (`handle`), `:142` (page call), `:164` (cursor), `:186` (model mapper) | Call `Models::page(provider, advanced, boundary, limit)`. Populate and bind `Cursor.advanced`. Map the new metadata/prices fields into the response. Hash the complete served representation for its entity tag; include advanced filtering in validators. Default reads omit advanced rows; explicit advanced reads include regular and advanced rows. |
| `rustd/crates/afd_api_operator/src/handler/admin/models.rs:68` (`list`), `:104` (`create`), `:164` (`update`), `:252` (`rates_request`), `:280` (`store_rates`), `:295` (renderer) | Validate the new body before constructing `ModelUpdate` / `ModelInput.details`. Render `Model.details`. Preserve duplicate, not-found and in-use outcomes. A malformed replacement must change neither identity, prices nor catalogue generation. The service transaction is implemented; API validation/representation is not. |
| `rustd/crates/afd_api_wire/src/tenant_model_entry.rs:45` (`row`), `:89` (default), `:110` (response) | Carry catalogue display name and context evidence from `CatalogueRate.metadata`. Custom entries retain their exact user-supplied identifier when no catalogue metadata exists. Ollama prices remain unknown. |
| `rustd/crates/afd_api_tenant/src/handler/tenant/model_entry/render.rs:59`, `:84`, `:128` (test literal) | Render the catalogue label from `CatalogueRate.metadata.display_name`; update fixture literals with complete metadata. `CatalogueRate` is Clone rather than Copy. Do not synthesize display labels from identifier prefixes. |
| `rustd/crates/afd_api_runner/src/handler/runner/report.rs:85` (`handle`), `:100` (`leases.report`), `:119` (token telemetry) | PENDING INDY'S REPORTING DESIGN. Existing run totals cannot select a context tier or a price across a date change per request. Indy left the per-request usage/time, deduplication and settlement shape to the other agent. Preserve that decision; do not invent a final shared payload here. It must represent uncached input, cached reads, all three cache-write categories and output/reasoning without double counting. |
| `rustd/crates/afd_api_runner/src/handler/runner/authoring/report.rs:31` (`handle`), `:39` (strict report), `:43` (report call) | Follow the same pending per-request usage/time decision for authoring. Current aggregate settlement is not proof of dated/tiered pricing. |
| `rustd/crates/afd_api_runner/src/handler/runner/enrolment.rs:86` (`register`), `:98` (`AssignedPolicy`) | BLOCKED BY INDY'S OWNERSHIP DECISION. No customer-owned runner assignment exists. Keep local Ollama refused until placement can prove customer ownership. A local marker alone is not authority. The daemon must never dial a local endpoint; cloud endpoints keep HTTPS and the complete existing Server-Side Request Forgery (SSRF) guard. |
| `rustd/crates/afd_bench/src/lane/lease/drain/platform.rs:76`, `:134`, `:166`, `:173` | Replace removed `core.model_library` / default inserts and cleanup with `model.platform_models`, `model.platform_model_prices`, and `model.platform_defaults`. Reuse `afd_db::test_util::ModelSeed` for synthetic identity/base-price fixtures where appropriate; SQL owns storage timestamps and price cleanup cascades from identity. |
| `rustd/crates/agentsfleetd/src/plane/services.rs:221` (`Catalogue`) | Wire the existing service types with their new model signatures. No second catalogue adapter or rate cache is needed. |
| `public/openapi.json:604` (admin paths), `:3858` (catalogue), `:18212` (`CatalogueModel`), `:22443` (registry row), `:24194` (`Report`), `:26525` (`Usage`) | Regenerate from the updated Rust annotations. Include the metadata, dated prices, advanced query, strict mutation fields and the reporting shape once Indy settles it. Do not hand-edit generated JSON. |

`afd_http/src/openapi/mod.rs` only holds common response/tag constants. It does not register these model schemas;
change the owning wire types, query types and plane annotations, then use the repository's generator.

The service reads and administrator writes use SQLx FromRow records and explicit JSON projections.
Seeder updates preserve existing model/price identifiers and unchanged timestamps. Administrator replacements advance their generation.

## Limits

The following remaining work is in the model-catalogue stream, rather than hidden inside the protected-path list:

- The administrator forms still submit three flat prices. Their types and editor must move to the agreed metadata/price-array API before mutation works with the new handler. The current UI commit only fixes labels, precise presentation and explicit advanced selection.
- Authoring dispatch still admits only its existing OpenAI model/tokenizer families and calls OpenAI Responses directly. None of the new curated identifiers is currently authoring-eligible. Choosing a tokenizer/counting strategy and provider dispatch for the curated models is an unsettled design call; do not silently guess it or claim private-chat readiness after pruning.
- Billing and authoring budget/settlement still consume aggregate three-counter usage. The current base-quote SQL adaptation restores reads; it does not implement context-tier selection, date splitting or cache-write charging.
- Configured-server Ollama tag discovery, local/cloud metadata and customer-owned placement refusals are not wired end to end. Native rig transport mocks do not authorize a local endpoint.
- Twelve models lack exact readable provider API context limits and are deliberately unseeded. Source data contains all 18 curated identities; a default apply seeds the six evidenced identities.
- The localhost:3000 walkthrough belongs to #754 and uses api-dev. It does not demonstrate this branch's catalogue or backend changes.

Required completion tests include full authenticated route/serialization checks, advanced-filter cursor/ETag mismatches,
invalid catalogue mutations, per-request pricing at both prompt boundaries and the December UTC boundary, usage conservation,
Ollama local/cloud refusal paths, and the integrated browser walkthrough. Do not sign off §8 from service mocks or schema tests alone.

## Related pages

- [Verified catalogue inputs](catalogue.md)
- [Measured verification](../../../HANDOFF_M229_CATALOGUE.md)
- [Required test ledger](test-ledger.md)
