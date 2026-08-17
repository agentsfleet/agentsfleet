<!--
SPEC AUTHORING RULES (load-bearing — the one comment that survives):
- Body order = the executing agent's read order. Fill via the kishore-spec-new
  skill (authoring order lives there); after filling, DELETE every "tpl:"
  guidance comment — the SPEC TEMPLATE GATE blocks tpl residue, unfilled
  {slots}, and missing required sections (audits/spec-template.sh --staged).
- No time/effort/hour/day estimates anywhere. No effort columns, complexity
  ratings, percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only
  sequencing signal. A section that contradicts these rules loses — delete it.
-->

# M168_001: Every dialable provider is either priced, or carries the reason it never will be

**Prototype:** v2.0.0
**Milestone:** M168
**Workstream:** 001
**Date:** Aug 17, 2026
**Status:** DONE
**Priority:** P1 — 87 of 103 dialable providers cannot be activated at all; `UZ-PROVIDER-004` refuses every model they host.
**Categories:** DOCS, INFRA
**Batch:** B1 — single-stream data curation; no parallel context.
**Branch:** feat/m168-model-library-rate-curation
**Test Baseline:** unit=4026 integration=684
**Depends on:** none
**Provenance:** LLM-drafted (claude-opus-5, Aug 17 2026) — rates read from live provider endpoints and official pricing pages during authoring, never from model memory.
**Canonical architecture:** `docs/architecture/billing_and_provider_keys.md` §9, §10

---

## Overview

**Goal (testable):** every provider key in `scripts/model-library-allowlist.json` either carries at least one priced model, or carries a machine-checkable `unpriced_reason` from a closed seven-code vocabulary naming why — and `make seed-models` emits a row for each priced one.

**Problem:** a user picks Amazon Bedrock (or Cerebras, or Venice, or any of 85 other providers) in the provider dropdown, supplies a working key, and activation fails with *"That model is not in the model library."* The provider dials fine; there is simply no catalogue row, because `scripts/gen-provider-skeleton.mjs` stamps every newly-derived provider with `models: []` and a note saying rates are "not yet curated". Nothing has ever curated them. The note reads as a queue that someone is working through; it is not — 87 keys have sat in that state since the generator landed, and nine of them (the local runtimes) can never leave it on rates alone, because a model running on the user's own hardware has neither a per-token price nor an enumerable id.

**Solution summary:** curate the catalogue in one pass and make the file state *why* for everything it still cannot price. Eight providers publish machine-readable pricing feeds — they convert from `source: "manual"` to `source: "api"`, so their rates refresh on every `make seed-models` and can never go stale. Four more get hand-verified rates from official pricing pages. Regional duplicates are resolved by a new standing rule: we price and dial the **international** endpoint of every vendor that has a China/international split, and mainland-China endpoints stay unpriced because they remain reachable through the OpenAI-compatible custom-endpoint route. Local runtimes get a minimal nonzero rate — not because we bill them (self-managed posture never charges tokens) but as a floor behind the activation path. The row alone is not enough: their model id is whatever the operator loaded, so the gate stops enforcing catalogue membership for them entirely (§6). Everything still unpriced gains a typed `unpriced_reason` in place of the uniform "not yet priced" note, so the file distinguishes *pending curation* from *permanently unpriceable*. `base_url` moves from the generator's derived set to its curated set, closing a live wrong-continent hazard. The architecture doc's prose is re-synced in place against all of it.

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(models): price every dialable provider or record why it cannot be priced
- **Intent (one sentence):** a user who brings a key for any provider we advertise can actually activate a model with it, instead of hitting a library-membership refusal for a provider we never curated.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `scripts/model-library-allowlist.json` — the `_readme` and `_generator` blocks state which fields are derived versus curated, and the `tiering_note` states why context-tiered models seed at the higher rate. Both constrain every edit in this spec.
2. `scripts/seed-models.mjs` — `fromApi` is the function the eight new `source: "api"` providers flow through; its `cachedRaw == null` fallback and its `Number.isFinite` guard are the two behaviours §2 changes.
3. `scripts/gen-provider-skeleton.mjs` — the `CURATED` array and the `base_url` assignment in the per-provider loop are what §4 moves; read the "WHAT IS DERIVED vs WHAT IS CURATED" header first.
4. `docs/architecture/billing_and_provider_keys.md` §9 and §10 — the prose §5 re-syncs, and the doc's own rule that it quotes shape rather than values.
5. `schema/400_model_library.sql` — the composite `(provider, model_id)` key is why a regional alias needs its own row rather than inheriting one.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `scripts/model-library-allowlist.json` | EDIT | The curation itself: rates, `source` flips, the international rule, `unpriced_reason` on everything still unpriced. |
| `scripts/seed-models.mjs` | EDIT | Tolerate a currency-prefixed rate string; treat a published zero cache-read as absent rather than as free. |
| `scripts/gen-provider-skeleton.mjs` | EDIT | Move `base_url` from the derived set to the curated set; carry `unpriced_reason` through a regeneration. |
| `docs/architecture/billing_and_provider_keys.md` | EDIT | Re-sync §1/§4/§8/§9/§10 prose against the curated catalogue; the platform default it names is retired. |
| `samples/fixtures/model-library/seed.sql` | EDIT | Regenerated fixture — the Zig integration tests exec it statement-by-statement. |
| `samples/fixtures/model-library/{venice,chutes,atlas-cloud,synthetic,nearai,vercel,vercel-ai,poe,ovh,ovhcloud}.json` | CREATE | One committed feed fixture per api-source provider, so the integration lane seeds without the network. |
| `scripts/seed_models_test.py` | CREATE | Direct tests for the three rate helpers `seed-models.mjs` now exports; found during REVIEW that they had no executable test. |
| `src/agentsfleetd/http/handlers/tenant_provider_cap.zig` | EDIT | Local runtimes take the existing custom-endpoint path instead of the catalogue-membership check; their served model cannot be enumerated. |
| `src/agentsfleetd/secrets/metadata.zig` | EDIT | New canonical home for `LOCAL_RUNTIME_PROVIDERS` + `isLocalRuntime`. Two gates in two layers read the list, and `state/` cannot import `http/handlers/`. |
| `src/agentsfleetd/secrets/metadata_test.zig` | EDIT | The list's own behaviour — membership, exactness, and non-collapse with the `openai-compatible` sentinel — moves here with it. |
| `src/agentsfleetd/state/secret_probe.zig` | EDIT | `requiresApiKey` replaces the inline `!is_compatible` test, so a local runtime is not asked for a key it has none of. |
| `src/agentsfleetd/state/tenant_provider.zig` | EDIT | Re-export `requiresApiKey` on the existing probe chain, beside `validateSecretEndpoint`. |
| `src/agentsfleetd/state/tenant_provider_test.zig` | EDIT | The §7 predicate's four arms, driven pure with no DB — the sibling of the existing base_url validation group. |
| `src/agentsfleetd/http/handlers/tenant_provider.zig` | EDIT | The `UZ-PROVIDER-003` detail string said "a named provider"; a custom endpoint and a local runtime now need no key. |
| `src/agentsfleetd/http/tenant_model_entries_integration_test.zig` | EDIT | End-to-end proof for both gates: a keyless, uncatalogued local activation succeeds; a keyless hosted one does not. |
| `src/agentsfleetd/errors/error_entries.zig` | EDIT | `UZ-PROVIDER-003`'s hint said `api_key` is required for "a named provider" — a local runtime is named too, so the hint would send an operator hunting for a key their server never issues. |
| `src/agentsfleetd/errors/error_registry_test.zig` | EDIT | Extend the existing hint-accuracy guard to both exemptions, and pin the retired "named provider" phrasing as a negative. |
| `cli/src/constants/custom-endpoint.ts` | EDIT | The CLI's mirror of `LOCAL_RUNTIME_PROVIDERS` + `isLocalRuntime`, beside the `OPENAI_COMPATIBLE_PROVIDER` mirror already there. |
| `cli/src/commands/fleet_secret_body.ts` | EDIT | Stop requiring `--api-key` for a local runtime — the CLI refused before the request was made. |
| `cli/src/lib/model-catalogue.ts` | EDIT | Skip the client-side model-membership check for a local runtime; its only catalogue row is an activation-floor sentinel. |
| `cli/test/custom-secret-create.integration.test.ts` | EDIT | §8's CLI dimensions. |
| `ui/packages/app/lib/types.ts` | EDIT | The dashboard's mirror of the same list, beside its `OPENAI_COMPATIBLE_PROVIDER`. |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/settings/models/components/AddModelEntryDialog.tsx` | EDIT | Save no longer gated on a key for a local runtime; the key label reads optional; the named path stops posting `api_key: ""`. |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/settings/models/components/ProviderModelSelect.tsx` | EDIT | A local runtime takes the free-text tier — a constrained picker would offer only the sentinel row. |
| `ui/packages/app/tests/models-registry-add.test.tsx` | EDIT | §8's and §10's dashboard dimensions. |
| `ui/packages/app/tests/custom-endpoint-lib.test.ts` | EDIT | The dashboard's `isLocalRuntime` driven over all nine names and a near-miss set (§10.4). |
| `scripts/check_model_allowlist_test.py` | EDIT | Parser-vacuity, drift, comment-stripping and scan-anchor tests for the two TypeScript mirrors. |
| `cli/test/model-catalogue.unit.test.ts` | EDIT | §10.5 — the catalogue bypass across every local runtime, the hosted negative, and the unseeded-catalogue window. |
| `src/runner/child_exec_input.zig` | EDIT | §9 — the lease's provider/key pair test dropped a keyless provider, so NullClaw fell back to its default and ran the wrong provider entirely. |
| `src/runner/child_exec_input_test.zig` | EDIT | §9's dimensions: a keyless local runtime and a keyless gateway both reach the engine; a key with no provider still does not. |
| `scripts/check_model_allowlist.py` | CREATE | Asserts the file's invariants (priced-or-reasoned, international rule, no zero rates, local-runtime parity with the credential layer) so they are enforced by a gate rather than by review. |
| `scripts/check_model_allowlist_test.py` | CREATE | Unit tests for the checker, matching the repository's `*_test.py` convention. |
| `make/quality.mk` | EDIT | Register the checker under `make lint-governance`. |

## Applicable Rules

- **`~/Projects/dotfiles/docs/greptile-learnings/RULES.md`** — **UFS** (the five `unpriced_reason` values and the local-runtime minimal rate are named constants in the checker, never repeated literals), **NDC** (no dead code: the retired `nvidia-nim` note text is replaced, not left beside its successor), **NLR** (touch-it-fix-it: the stale `nullclaw_alias_collisions` block is corrected in the same diff that makes `base_url` curated, not left describing a state that no longer exists), **ORP** (orphan sweep on the uniform "not yet priced" note string once it stops being uniform).
- `~/Projects/dotfiles/dispatch/write_python.md` — `scripts/check_model_allowlist.py` is new Python: standard-library parsing, context-managed file reads, specific exceptions.
- `~/Projects/dotfiles/dispatch/write_ts_adhere_bun.md` — `scripts/seed-models.mjs` and `scripts/gen-provider-skeleton.mjs` are JavaScript; `const` discipline and the TS FILE SHAPE DECISION apply at PLAN.
- `~/Projects/dotfiles/dispatch/write_documentation.md` → `docs/DOCUMENTATION_RULES.md` — `billing_and_provider_keys.md` is a published architecture page.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes — `tenant_provider_cap.zig` gains a provider list, a pure predicate and three test blocks | `zig fmt --check` clean; inline tests reachable from the existing test root; no allocation, no error union, so no `errdefer`/drain surface |
| PUB / Struct-Shape | no — `LOCAL_RUNTIME_PROVIDERS` and `isLocalRuntime` are file-private | the only `pub` in the file, `resolveSelfManagedCap`, keeps its signature |
| File & Function Length (≤350/≤50/≤70) | yes — `check_model_allowlist.py` is new | One check function per invariant, each well under 50 lines; the file stays under 350 or splits its checks into a sibling module. |
| UFS (repeated/semantic literals) | yes — reason codes and the minimal rate repeat across JSON and checker | The five reason codes and `LOCAL_RUNTIME_MINIMAL_USD_PER_MTOK` are module constants in the checker; the JSON side is asserted against them so the two cannot drift. |
| UI Substitution / DESIGN TOKEN | no — no UI files | N/A |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | no — no new error codes, no schema change, no allocator wiring | `UZ-PROVIDER-004` already exists and its text is unchanged; `core.model_library` gains rows, not columns. |
| DOC READ GATE | yes — architecture page and Python both touched | `bash audits/doc-read.sh log` for `docs/DOCUMENTATION_RULES.md` and `dispatch/write_python.md` before the first triggering edit. |

## Prior-Art / Reference Implementations

- **Reference:** `scripts/model-library-allowlist.json` `openrouter` and `pioneer` entries — the two `source: "api"` providers already in the file. Every new api-source provider mirrors their `endpoint` / `rate_unit` / `field_map` shape exactly; `openrouter` is the `per_token` template and `pioneer` the `per_million` one. No new mechanism is invented — this spec only widens an existing one.
- **Reference:** `scripts/check_zig_coverage.py` and its `check_zig_coverage_test.py` — the repository's established shape for a Python invariant checker plus its unit test, including how it reports and how `make lint-governance` calls it.
- **Divergence:** none. The one genuinely new artifact is the `unpriced_reason` vocabulary, which has no prior art because the file has never before distinguished *pending* from *impossible*.

## Sections (implementation slices)

### §1 — The international-endpoint rule, and regional duplicates resolved

103 provider keys collapse to roughly 25 real vendors; the rest are regional and spelling variants that NullClaw dials as independent names. Today the file prices whichever spelling the generator happened to derive, which is how `kimi` came to be priced from the international price page while pointing at the mainland-China endpoint. This slice adopts one rule and applies it everywhere: **we price and dial the international endpoint; a mainland-China endpoint is never priced, because it stays reachable through the OpenAI-compatible custom-endpoint route.** The rule is recorded in the file's `_readme` so a future curation pass cannot quietly reverse it.

**Implementation default:** where a vendor's international arm carries a different provider name than its China arm (BytePlus versus Volcengine/Ark/Doubao), the international name is the one that gets rows.

- **Dimension 1.1 — DONE** — `_readme` carries the international rule as prose, and `nullclaw_alias_collisions` describes the endpoints actually shipped rather than the ones intended → Test `test_readme_states_international_rule`
- **Dimension 1.2 — DONE** — every provider whose `base_url` is a mainland-China host has empty `models` and `unpriced_reason: "cn_endpoint"` → Test `test_cn_endpoints_are_unpriced_with_reason`
- **Dimension 1.3 — DONE** — `kimi` and `qwen` carry their international `base_url`, matching the `source_url` their rates were read from → Test `test_priced_provider_base_url_matches_rate_region`
- **Dimension 1.4 — DONE** — international aliases of an already-priced vendor carry that vendor's rates as their own rows, because `(provider, model_id)` is the catalogue's composite key and an alias inherits nothing → Test `test_intl_aliases_carry_own_rows`

### §2 — Eight providers converted to live pricing feeds

Venice, Chutes, Atlas Cloud, Synthetic, NEAR AI, Vercel AI Gateway, Poe, and OVHcloud each publish per-model pricing on a public, unauthenticated endpoint. Hand-typing their rates would put eight more `verified_at` obligations on a file that already warns about staleness. They become `source: "api"` instead, which is the mechanism `openrouter` and `pioneer` already use, so their rates refresh on every seed run.

Two of the eight expose a shape `fromApi` mishandles today, and both failures are silent, so this slice fixes the reader rather than working around it per-provider. Synthetic prefixes its rates with a dollar sign (`"$0.000001"`), which `Number()` reads as `NaN` — the current code warns and skips the model, seeding the provider with fewer rows than allowlisted. OVHcloud publishes `"0"` for cache reads to mean *no cache discount offered*; the existing fallback only triggers on `null` or empty string, so a literal zero would be seeded verbatim and silently zero-rate every cached read — precisely the hazard the file's own `_readme` warns about.

**Implementation default:** normalise a leading currency symbol before `Number()`, and treat a cache-read rate that parses to zero the same as an absent one (fall back to the input rate). Both are corrections to the reader's contract with providers, not per-provider special cases, so neither takes a provider name.

- **Dimension 2.1 — DONE** — a rate string carrying a leading `$` parses to its numeric value rather than being skipped → Test `test_currency_prefixed_rate_parses`
- **Dimension 2.2 — DONE** — a published cache-read rate of zero falls back to the input rate and never seeds a zero → Test `test_zero_cache_read_falls_back_to_input`
- **Dimension 2.3 — DONE** — each of the eight providers carries `endpoint`, `rate_unit`, and a `field_map` whose paths resolve against that provider's committed fixture → Test `test_api_provider_field_maps_resolve`
- **Dimension 2.4 — DONE** — each of the eight has a committed fixture under `samples/fixtures/model-library/`, so the integration lane seeds deterministically without network → Test `test_every_api_provider_has_a_fixture`

### §3 — Manual rates, local runtimes, and a reason for everything else

Four providers publish pricing only as human-readable pages and get hand-verified rates against a cited `source_url`: Cerebras, Perplexity, Cohere, and Amazon Bedrock (both the `bedrock` and `aws-bedrock` spellings, which are distinct dialable names and therefore distinct rows).

The nine local runtimes are the case the file has never had vocabulary for. Ollama, vLLM, llama.cpp, LM Studio, SGLang, LiteLLM, and Osaurus run on hardware the user owns; there is no per-token price to find, ever. But an empty `models` array means `UZ-PROVIDER-004` refuses activation, so a user running their own vLLM cannot select it at all. They get a minimal nonzero rate, and the note states plainly that the row exists to satisfy the activation gate rather than to bill anyone — self-managed posture charges no token cost, and a local runtime is self-managed by construction.

Everything still unpriced gains a typed reason, replacing the uniform note that currently reads as a queue.

**Implementation default:** the reason vocabulary is exactly five values — `cn_endpoint` (§1), `local_runtime`, `subscription_plan` (Copilot, the Kimi and Qwen coding portals, the `-plan` endpoints — billed per seat, not per token), `gateway_passthrough` (Hugging Face and Cloudflare route to partner infrastructure at partner rates, so the partner is the thing to price), and `deployment_scoped` (Azure and Vertex price per customer deployment and region, and carry no `base_url` at all). A sixth value means the vocabulary is wrong — extend it deliberately or classify correctly.

- **Dimension 3.1 — DONE** — Cerebras, Perplexity, Cohere, and both Bedrock spellings carry rates and a `source_url` that resolves → Test `test_manual_providers_carry_source_url`
- **Dimension 3.2 — DONE** — every local runtime carries the minimal rate and `unpriced_reason: "local_runtime"`, and the rate is nonzero → Test `test_local_runtimes_priced_at_minimum`
- **Dimension 3.3 — DONE** — every provider in the file either has a non-empty `models` array or a valid `unpriced_reason`; none has both, none has neither → Test `test_priced_xor_reasoned`
- **Dimension 3.4 — DONE** — no priced model anywhere in the file carries a zero in any of the three rate columns → Test `test_no_zero_rates`

### §4 — `base_url` becomes curated, not derived

The generator lists `base_url` among the fields it derives from vendored NullClaw and rewrites on every run. That is why the curated intent recorded in `nullclaw_alias_collisions` — that `kimi` and `qwen` point at their international endpoints — was silently overwritten by the mainland-China values NullClaw's table happens to carry for those two names. The file's own warning describes the outcome exactly: *the failure is silent — right price, wrong continent.*

Moving `base_url` into the curated set makes §1's rule survive a dependency bump. The generator still supplies a `base_url` for a newly-derived provider that has none, because a provider with no endpoint is unroutable; it simply stops overwriting one that is already there.

- **Dimension 4.1 — DONE** — regenerating the skeleton against unchanged vendored NullClaw leaves every existing `base_url` byte-identical → Test `test_regeneration_preserves_curated_base_url`
- **Dimension 4.2 — DONE** — a newly-derived provider absent from the current file still receives its derived `base_url` → Test `test_new_provider_receives_derived_base_url`
- **Dimension 4.3 — DONE** — `unpriced_reason` survives a regeneration, like every other curated field → Test `test_regeneration_preserves_unpriced_reason`

### §5 — The architecture doc re-synced in place

`docs/architecture/billing_and_provider_keys.md` names Fireworks Kimi K2.6 as the platform default in roughly ten places. The allowlist retired K2.6 as superseded, and `accounts/fireworks/models/kimi-k2.6` has no rate row — so the doc's own headline example is the `error.ModelNotPriced` case it documents in §4.2. §9's routing table is an eight-row hand-copy of NullClaw's `factory.zig`, which the generator's header names as the drift pattern it exists to eliminate; it is already wrong by omission, listing eight of 103 dialable names. §10 claims the provider is encoded in the `model_id` string, which contradicts `schema/400_model_library.sql`, where `provider` is an explicit column precisely so one model can appear under several hosts at different rates.

The prose is corrected in place rather than replaced with a pointer, per the authoring decision recorded in Discovery.

- **Dimension 5.1 — DONE** — no reference to `kimi-k2.6` survives anywhere in the doc; the named platform default is a model the allowlist prices → Test `test_doc_names_no_retired_model`
- **Dimension 5.2 — DONE** — §9's routing table states the count of dialable providers and cites the allowlist as the enumeration, rather than presenting eight rows as the set → Test `test_doc_routing_table_cites_allowlist`
- **Dimension 5.3 — DONE** — §10's provider-origin paragraph agrees with the schema: `provider` is a column, not an inference from `model_id` → Test `test_doc_provider_origin_matches_schema`
- **Dimension 5.4 — DONE** — the doc quotes no dollar amounts, unchanged from today → Test `test_doc_quotes_no_dollar_amounts`

### §6 — The activation gate stops enforcing membership for local runtimes

Seeding a rate row for `vllm` does not make `vllm` activatable. `UZ-PROVIDER-004` resolves `(provider, model_id)`, and a local runtime's model id is whatever the operator loaded — `--served-model-name`, an `ollama pull`. NullClaw itself carries no model name for any of the nine (its `compat_providers` table gives each a name, a localhost URL and a display label, nothing more), because the set is unbounded and per-install. So any id we seed is a guess, and the floor rows alone leave every local activation refused exactly as before.

The gate already has the right shape for this. A custom OpenAI-compatible endpoint bypasses the catalogue and takes the unknown/auto cap sentinel, precisely because its user-hosted model is absent from the platform catalogue by design. A local runtime is the same case with a name attached, so it takes the same path. Billing is untouched: self-managed charges a run fee only, and a local runtime is self-managed by construction.

**Implementation default:** membership is decided by a named-constant provider list rather than a new schema column, mirroring how `OPENAI_COMPATIBLE_PROVIDER` is already handled — and living beside it in `secrets/metadata.zig` once §7 gave it a second reader. The drift risk that introduces — two places listing the same nine providers — is closed by making the checker parse the Zig list and require set equality with the allowlist's `rate_basis: "activation_floor"` set.

- **Dimension 6.1 — DONE** — Every provider in `LOCAL_RUNTIME_PROVIDERS` is recognised → Test `every local runtime is recognised`
- **Dimension 6.2 — DONE** — A hosted, billable provider still enforces catalogue membership, so an uncatalogued model fails closed → Test `a hosted provider is not a local runtime`
- **Dimension 6.3 — DONE** — Matching is exact: no prefix, suffix, case-fold or padded near-miss buys a catalogue bypass → Test `local-runtime matching is exact, never a prefix or case fold`
- **Dimension 6.4 — DONE** — The gate's list and the allowlist's activation-floor set are the same set, and the parser that compares them cannot silently return empty → Test `LocalRuntimeParity` (4 tests)
- **Dimension 6.5 — DONE** — An uncatalogued model on a local runtime activates through the real handler, a hosted provider naming the same model still fails closed, and a blank model is still refused → Test `test_activate_local_runtime_skips_catalogue`

### §7 — The credential gate stops requiring a key a local runtime does not have

Catalogue membership was only the second of two gates between an operator and their own box. The first is in the credential probe: `probeSelfManagedSecret` requires a non-empty `api_key` for every named provider, exempting only `openai-compatible`. A local server authenticates nobody, so there is no key to supply — the operator typed a placeholder to get past a check that measured nothing. Fixing §6 alone would have shipped "local runtimes work" with a lie in it.

The exemption follows the same reasoning as the catalogue one and reads from the same list, so the two cannot drift: `requiresApiKey` returns false for `openai-compatible` and for any `metadata.LOCAL_RUNTIME_PROVIDERS` member, true for everything else. A hosted provider still fails at the parse boundary, which is the right place — a blank key reaches the vendor as an unauthenticated request and comes back a 401 the tenant cannot read.

**Implementation default:** `LOCAL_RUNTIME_PROVIDERS` moves from the activation handler down to `secrets/metadata.zig`. Two gates now read it and `state/` cannot import `http/handlers/`; `metadata.zig` is the leaf that already owns `OPENAI_COMPATIBLE_PROVIDER`. The parity checker follows the list to its new home, so §6.4's set-equality guarantee is unchanged and now covers both gates. The two exemptions stay independent — waiving the key does not waive `validateSecretEndpoint`, so a local runtime still cannot smuggle a `base_url`.

- **Dimension 7.1 — DONE** — Every hosted provider still requires a non-empty `api_key` → Test `test_key_required_for_a_hosted_provider`
- **Dimension 7.2 — DONE** — An `openai-compatible` custom endpoint remains keyless, unchanged → Test `test_key_optional_for_a_custom_endpoint`
- **Dimension 7.3 — DONE** — Every provider in `LOCAL_RUNTIME_PROVIDERS` is exempt from the key requirement → Test `test_key_optional_for_a_local_runtime`
- **Dimension 7.4 — DONE** — The exemption is exact-match: a near-miss provider id still needs a key, so no hosted provider inherits it → Test `test_key_exemption_does_not_leak_to_a_near_miss_name`
- **Dimension 7.5 — DONE** — The two exemptions do not collapse into each other: the `openai-compatible` sentinel is not a local runtime → Test `the openai-compatible sentinel is not a local runtime`
- **Dimension 7.6 — DONE** — A credential carrying no `api_key` at all activates on a local runtime through the real handler, while a keyless hosted credential is still refused `UZ-PROVIDER-003` → Test `test_activate_local_runtime_skips_catalogue`
- **Dimension 7.7 — DONE** — `UZ-PROVIDER-003`'s registry hint and the handler's detail string describe the rule the validator now enforces, naming both exemptions rather than "a named provider" → Test `UZ-PROVIDER-003 hint states api_key is conditional, not unconditionally required`

### §8 — The client surfaces stop enforcing rules the server dropped

Both client surfaces re-implement the credential rules so an operator gets a local error instead of a round-trip. That is the right design and it is why fixing the server alone would have shipped nothing: the CLI and the dashboard each refused the credential before the request was made. Three client-side refusals, all of them now aligned with the server:

1. `cli/src/commands/fleet_secret_body.ts` rejected `--provider ollama` with no `--api-key`.
2. `cli/src/lib/model-catalogue.ts` checked `--model` against the catalogue for every non-custom provider — and a local runtime's only row is the `local` sentinel, so a real model id was always rejected.
3. `AddModelEntryDialog.tsx` gated Save on a non-empty key, and `ProviderModelSelect.tsx` rendered a constrained `<Select>` whose sole option was that same sentinel.

**Implementation default:** each surface restates `LOCAL_RUNTIME_PROVIDERS` once, under the same identifier, because `cli/` and `ui/` share no module graph with the server or with each other — the pattern `OPENAI_COMPATIBLE_PROVIDER` already follows. Three mirrors is three chances to drift, so `check_model_allowlist.py` now requires all four lists (allowlist floor set, Zig, CLI, dashboard) to be equal, and each parser has a vacuity test so a silently-empty scrape cannot report clean forever. The dashboard also stops writing `api_key: ""` on the named path, matching the custom path: a blank string is a key the vault reports as present.

- **Dimension 8.1 — DONE** — The CLI stores a local-runtime credential with no `--api-key` and an uncatalogued `--model` → Test `a LOCAL RUNTIME without --api-key succeeds, and its uncatalogued model is accepted`
- **Dimension 8.2 — DONE** — The CLI still rejects `--base-url` on a local runtime: the key waiver is not an endpoint waiver → Test `a LOCAL RUNTIME with --base-url is still rejected — the key waiver is not an endpoint waiver`
- **Dimension 8.3 — DONE** — The dashboard enables Save for a keyless local runtime, labels the key optional, and posts a body with no `api_key` and no `base_url` → Test `enables Save for a local runtime with no key, and stores a body carrying no api_key`
- **Dimension 8.4 — DONE** — The dashboard's exemption is exact-match, so a near-miss provider name still requires a key → Test `still disables Save for a near-miss provider name that only looks local`
- **Dimension 8.5 — DONE** — All four local-runtime lists are equal, and each parser is proven non-vacuous → Tests `test_cli_mirror_is_actually_parsed`, `test_cli_mirror_drift_is_caught`, `test_ui_mirror_is_actually_parsed`, `test_ui_mirror_drift_is_caught`

### §9 — The lease stops discarding a keyless provider

Found by the REVIEW security specialist, and it is the gate that made §7 and §8 inert. `buildCallArgs` treated the provider and the key as an atomic pair — `if (provider.len > 0 and api_key.len > 0)` — on the reasoning that "the resolver always produces both or neither." That was true until `requiresApiKey` waived the key. A keyless activation delivered `{provider: "ollama", api_key: ""}`, the pair test failed, and BOTH halves were dropped with a `fleet_provider_key_incomplete` warning. NullClaw then fell back to whatever `Config.load` resolved — so the tenant's self-managed run went to a provider they never chose, which is the exact wrong-provider outcome the pairing rule existed to prevent.

The rule is now asymmetric, because the two directions are not equivalent. A provider with no key is legitimate (that is what §7 established). A key with no provider is still malformed — it would authenticate against a provider nobody selected — so that direction still injects nothing.

This also un-breaks the openai-compatible optional-key design, which shipped before local runtimes existed: a keyless gateway credential was dropped by the same pair test and had never actually reached the engine.

- **Dimension 9.1 — DONE** — A keyless local-runtime lease carries its provider and an empty key to the engine, and still no `base_url` → Test `buildCallArgs carries a KEYLESS provider — an empty key is legitimate, not malformed`
- **Dimension 9.2 — DONE** — A keyless openai-compatible gateway carries provider, empty key, and its `base_url` → Test `buildCallArgs carries a keyless openai-compatible gateway too`
- **Dimension 9.3 — DONE** — A key with no provider is still refused outright → Test `buildCallArgs injects neither half of an incomplete provider key pair` (unchanged, now the only malformed direction)

### §10 — The tests the REVIEW specialists found missing

Both specialists independently reported coverage that would have let this diff's behaviour regress green. Each gap is closed rather than noted:

- **The dashboard's free-text carve-out was tested against an EMPTY catalogue**, where the branch is a no-op — production seeds one activation-floor row per local runtime, so the real path was untested and deleting the carve-out kept every test green.
- **The REPLACE arm of the dashboard's `namedData` change had no assertion**; only the create arm did, so a revert on the replace path alone would have shipped `api_key: ""` again.
- **Neither TypeScript `isLocalRuntime` had a direct test.** Narrowing either to `provider === "ollama"` left the parity gate green (it scrapes the array, never the function) while the other eight silently lost both exemptions.
- **The Zig membership tests looped over the array the implementation scans** — a form that cannot fail for any implementation that reads the list, including one that has lost a member.
- **The parser had two defects the security specialist demonstrated**: `line.split("//")[0]` cuts inside a string literal (a `"http://…"` member truncates and the regex re-pairs quotes across the window, yielding a wrong-but-non-empty set), and anchoring the scan on the declaration rather than the assignment lets a `: readonly string[]` annotation close the window on its own empty brackets. Both are fixed and both are now driven by a test that fails against the old parser.

- **Dimension 10.1 — DONE** — The model picker stays free text for a local runtime whose floor row IS catalogued, and the sentinel is offered nowhere → Test `keeps the model field free text for a local runtime whose floor row IS catalogued`
- **Dimension 10.2 — DONE** — A catalogued hosted provider still gets the constrained picker → Test `a hosted provider with catalogue rows still gets a constrained picker`
- **Dimension 10.3 — DONE** — Replacing a held local-runtime credential omits `api_key` → Test `replacing a held local-runtime credential also omits api_key`
- **Dimension 10.4 — DONE** — Both TypeScript mirrors are driven over all nine names and a near-miss set → Tests `isLocalRuntime is exact-match — a near miss buys no exemption` (CLI), `describe("isLocalRuntime")` (dashboard)
- **Dimension 10.5 — DONE** — The CLI's catalogue bypass is driven for every local runtime, for a hosted provider that must still fail, and for the unseeded-catalogue window → Tests `every local runtime takes the bypass, not just ollama`, `a HOSTED provider's uncatalogued model is still refused`, `a local-runtime provider absent from the catalogue is still refused`
- **Dimension 10.6 — DONE** — Comment stripping preserves a URL inside a string literal, and the array scan anchors on the assignment → Tests `test_comment_stripping_does_not_cut_inside_a_string`, `test_comment_stripping_handles_an_escaped_quote`, `test_ts_scan_starts_at_the_array_not_the_declaration`
- **Dimension 10.7 — DONE** — A local runtime is still refused a `base_url` server-side, asserted rather than only claimed in a comment → Test `test_local_runtime_is_still_forbidden_a_base_url`
- **Dimension 10.8 — DONE** — The seeded activation floors are asserted for all nine local runtimes, not a hard-coded three → Test `test_local_runtime_floor_is_nonzero_after_nanos_rounding`

## Interfaces

```
scripts/model-library-allowlist.json — provider entry, the two shapes

  priced, manual source:
    "cerebras": {
      "dial": "native",
      "base_url": "https://api.cerebras.ai/v1",
      "display": "Cerebras",
      "source": "manual",
      "source_url": "<official pricing page>",
      "models": [ { "model_id": …, "context_cap_tokens": …,
                    "input": …, "cached_input": …, "output": … } ]
    }

  priced, api source (rates never hand-typed):
    "venice": {
      "dial": "native",
      "base_url": "https://api.venice.ai",
      "source": "api",
      "endpoint": "https://api.venice.ai/api/v1/models",
      "rate_unit": "per_million" | "per_token",
      "field_map": { "model_id": …, "context_cap_tokens": …,
                     "input": …, "cached_input": …, "output": … },
      "models": [ "<id>", … ]
    }

  unpriced — models MUST be empty and the reason MUST be one of five:
    "unpriced_reason": "cn_endpoint"        reachable via custom endpoint; intl arm is priced
                     | "local_runtime"      user's own hardware; minimal rate seeded for activation
                     | "subscription_plan"  per-seat billing; no per-token rate exists
                     | "gateway_passthrough" routes to partners at partner rates; price the partner
                     | "deployment_scoped"  per-customer deployment and region

scripts/check_model_allowlist.py
  exit 0  every invariant holds
  exit 1  one line per violation on stderr: "<provider>: <invariant> — <detail>"

Unchanged: core.model_library shape, GET /v1/models response, UZ-PROVIDER-004 text,
and the emitted SQL's INSERT … ON CONFLICT arbitration.
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Pricing endpoint unreachable | An api-source provider is down during `make seed-models` | `fromApi` already fails the whole run rather than seeding the provider empty; unchanged. Operator sees `✗ <provider>: <endpoint> returned <status>` and no rows are written. |
| Pricing endpoint returns a new shape | Provider renames its pricing fields | `field_map` paths resolve to `undefined`, `Number.isFinite` rejects, and the existing "none of the allowlisted ids matched" guard fails the run. No partial seed. |
| Currency-prefixed rate | Synthetic publishes `"$0.000001"` | Normalised before parse (2.1). Regression: a string that is *not* a parseable rate after normalisation still warns and skips, never seeds zero. |
| Zero cache-read rate | OVHcloud publishes `"0"` meaning no discount | Falls back to the input rate (2.2). A cached read is never billed at zero. |
| Allowlisted id retired upstream | Provider drops a model between curation passes | Existing per-id warning; the run continues for the remaining ids. Unchanged. |
| Reason code typo | Curator writes `local-runtime` for `local_runtime` | `check_model_allowlist.py` exits 1 naming the provider and the invalid value; `make lint-governance` fails. |
| Priced *and* reasoned | Curator prices a provider but leaves its reason behind | Checker exits 1 — the two states are exclusive, and a stale reason beside real rates is how a "not yet priced" note outlives its truth. |
| Zero rate reaches the catalogue | A manual entry omits a column, or a feed publishes zero | Checker exits 1 on any zero in a priced row; a zero rate must never enter the cost path. |
| Generator overwrites a curated endpoint | Dependency bump changes NullClaw's table for a name we curated | §4 makes `base_url` curated; regeneration preserves it and 4.1 asserts byte-identity. |
| Rate cache serves stale rows after seeding | Direct SQL write does not rebuild the in-process cache | Pre-existing and already printed by the seeder; §6 of the doc restates it. Operator restarts `agentsfleetd`. |

## Invariants

1. **Priced XOR reasoned** — every provider has either a non-empty `models` array or exactly one valid `unpriced_reason`, never both and never neither. Enforced by `check_model_allowlist.py` under `make lint-governance`.
2. **No zero rates in a priced row** — a zero in `input`, `cached_input`, or `output` fails the checker. Enforced mechanically; the schema comment's "zero rates never enter the cost path" stops being a convention.
3. **Reason codes are closed** — the five values are module constants in the checker; an unrecognised value fails. Enforced by the checker, not by review.
4. **A priced provider's `base_url` region matches its `source_url` region** — a rate read from an international price page may not be attached to a mainland-China endpoint. Enforced by the checker's host-versus-source comparison.
5. **`base_url` survives regeneration** — asserted by re-running `gen-provider-skeleton.mjs` in diff mode and requiring no `base_url` change. Enforced by 4.1 in the test suite.
6. **Every api-source provider has a committed fixture** — otherwise the integration lane silently depends on the network. Enforced by the checker.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| not applicable — no product/operator signal changes | not applicable | This workstream curates catalogue data and documentation; it adds no route, no user action, and no counter. The one operator-visible surface is `make seed-models`' existing diff report, whose format is unchanged. | not applicable | not applicable | not applicable |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_readme_states_international_rule` | The `_readme` block contains the international-endpoint rule and `nullclaw_alias_collisions` entries match the `base_url` actually shipped for those providers. |
| 1.2 | unit | `test_cn_endpoints_are_unpriced_with_reason` | Every provider whose `base_url` host is a mainland-China host has empty `models` and `unpriced_reason == "cn_endpoint"`. |
| 1.3 | unit | `test_priced_provider_base_url_matches_rate_region` | For `kimi` and `qwen`, the `base_url` host and the `source_url` host are both international; a CN host with an international source URL fails. |
| 1.4 | unit | `test_intl_aliases_carry_own_rows` | Each international alias of a priced vendor carries its own `models` entries; an empty array on such an alias fails. |
| 2.1 | unit | `test_currency_prefixed_rate_parses` | `"$0.000001"` → `0.000001`; `"0.000001"` unchanged; `"abc"` still rejected as unparseable. |
| 2.2 | unit | `test_zero_cache_read_falls_back_to_input` | A feed row with `input=0.0000001` and cache-read `"0"` seeds `cached_input == input`, never `0`. |
| 2.3 | integration | `test_api_provider_field_maps_resolve` | For each of the eight, running the seeder against its committed fixture yields one row per allowlisted id, all three rates finite and positive. |
| 2.4 | unit | `test_every_api_provider_has_a_fixture` | Every `source: "api"` provider has a file at `samples/fixtures/model-library/<provider>.json`. |
| 3.1 | unit | `test_manual_providers_carry_source_url` | Cerebras, Perplexity, Cohere, `bedrock`, `aws-bedrock` each have a non-empty `source_url` and ≥1 model. |
| 3.2 | unit | `test_local_runtimes_priced_at_minimum` | All nine local runtimes carry the minimal rate constant on all three columns and `unpriced_reason == "local_runtime"`. |
| 3.3 | unit | `test_priced_xor_reasoned` | Iterating all providers, exactly one of (non-empty `models`, valid `unpriced_reason`) holds for each; a fixture with both set fails, and one with neither fails. |
| 3.4 | unit | `test_no_zero_rates` | No priced model in the real file has a zero in any rate column. |
| 4.1 | integration | `test_regeneration_preserves_curated_base_url` | `node scripts/gen-provider-skeleton.mjs` in diff mode against unchanged vendored NullClaw reports zero `base_url` changes. |
| 4.2 | unit | `test_new_provider_receives_derived_base_url` | A synthetic "provider absent from the current file" still gets its derived `base_url` in the output. |
| 4.3 | unit | `test_regeneration_preserves_unpriced_reason` | `unpriced_reason` is in the generator's `CURATED` list and round-trips unchanged. |
| 5.1 | unit | `test_doc_names_no_retired_model` | `grep -c 'kimi-k2\.6' docs/architecture/billing_and_provider_keys.md` is 0, and every model the doc names resolves to a priced allowlist row. |
| 5.2 | unit | `test_doc_routing_table_cites_allowlist` | §9 references `scripts/model-library-allowlist.json` and does not present its table as the complete provider set. |
| 5.3 | unit | `test_doc_provider_origin_matches_schema` | §10 does not claim the provider is inferable from `model_id`. |
| 5.4 | unit | `test_doc_quotes_no_dollar_amounts` | No `$<digit>` occurrence outside the shape-only CLI output block — a regression guard on behaviour the doc already has. |
| 6.5, 7.6 | integration | `test_activate_local_runtime_skips_catalogue` | `provider=ollama`, no `api_key` at all, uncatalogued model → 200; `provider=anthropic` + same model → 400 UZ-PROVIDER-004; `provider=anthropic` with no key → 400 UZ-PROVIDER-003; blank model on a local runtime → 400. |
| 7.1 | unit | `test_key_required_for_a_hosted_provider` | `requiresApiKey` is true for fireworks, anthropic, openai, kimi, bedrock, groq. |
| 7.2 | unit | `test_key_optional_for_a_custom_endpoint` | `requiresApiKey(openai-compatible)` is false — unchanged behaviour. |
| 7.3 | unit | `test_key_optional_for_a_local_runtime` | `requiresApiKey` is false for every `LOCAL_RUNTIME_PROVIDERS` member. |
| 7.4 | unit | `test_key_exemption_does_not_leak_to_a_near_miss_name` | `""`, `Ollama`, `VLLM`, `vllm2`, `xvllm`, `lm-studio `, `llama.cppx`, `openai-compatible-x` all still require a key. |
| 7.5 | unit | `the openai-compatible sentinel is not a local runtime` | The two exemption branches stay distinct, so a local runtime cannot reach the `base_url`-carrying path. |
| 7.7 | unit | `UZ-PROVIDER-003 hint states api_key is conditional, not unconditionally required` | The hint names both exemptions and no longer says "required for a named provider". |
| 8.1 | integration (CLI) | `a LOCAL RUNTIME without --api-key succeeds, and its uncatalogued model is accepted` | `secret create --provider ollama --model llama-3.3-70b-my-finetune` with no key exits 0; the POST body carries neither `api_key` nor `base_url`. |
| 8.2 | integration (CLI) | `a LOCAL RUNTIME with --base-url is still rejected — the key waiver is not an endpoint waiver` | Non-zero exit naming `--base-url`. |
| 8.3 | unit (dashboard) | `enables Save for a local runtime with no key, and stores a body carrying no api_key` | Save enabled with an empty key field; the label reads "API key (optional)"; the posted body omits `api_key` and `base_url`. |
| 8.4 | unit (dashboard) | `still disables Save for a near-miss provider name that only looks local` | Provider `Ollama` keeps Save disabled until a key is typed. |
| 8.5 | unit | `test_{cli,ui}_mirror_is_actually_parsed`, `test_{cli,ui}_mirror_drift_is_caught` | Each TypeScript scrape returns the nine names; a one-name divergence on either surface is reported. |
| regression | integration | `test_seed_fixture_sql_regenerates_byte_identical` | `node scripts/seed-models.mjs --fixtures --emit-fixture-sql` twice yields an unchanged file; the stamp is pinned to `verified_at`. |
| regression | integration | `test_existing_sixteen_providers_unchanged` | The 16 providers priced before this workstream seed the same rates afterwards, except the deliberate `kimi`/`qwen` `base_url` correction. |
| idempotency | integration | `test_reseed_is_a_noop` | Running `make seed-models` twice against the same catalogue reports "no changes" on the second run. |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Every provider is priced or carries a valid reason (§1, §3) | `python3 scripts/check_model_allowlist.py` | exit 0 | P0 | |
| R2 | No provider is left in the old uniform "not yet priced" state (§3) | `grep -c 'not yet priced' scripts/model-library-allowlist.json` | `0` | P0 | |
| R3 | The catalogue seeds every priced provider (§2, §3) | `node scripts/seed-models.mjs --fixtures` | exit 0, `0 unmanaged` in the diff report | P0 | |
| R4 | Regeneration preserves curated endpoints (§4) | `node scripts/gen-provider-skeleton.mjs` | exit 0, no `base_url` in the reported diff | P0 | |
| R5 | The architecture doc names no retired model (§5) | `grep -c 'kimi-k2\.6' docs/architecture/billing_and_provider_keys.md` | `0` | P0 | |
| R6 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | |
| S1 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S2 | Lint clean | `make lint-all` | exit 0 | P0 | |
| S3 | Integration passes (seeder + fixture SQL touched) | `make test-integration` | exit 0 | P0 | |
| S7 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S8 | No oversize source file | `git diff --name-only origin/main...HEAD \| grep -v '\.md$' \| grep -v '\.json$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | |
| S9 | Orphan sweep | Dead Code Sweep greps | 0 matches | P0 | |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line (`342 passed`); long evidence goes to PR Session Notes with a pointer here. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE; a P1 ❌ ships only with an Indy-acked deferral quote in Discovery.

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

| File to delete | Verify |
|----------------|--------|
| N/A — no files deleted | N/A |

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| the uniform unpriced note | `grep -c 'Dialable, not yet priced' scripts/model-library-allowlist.json` | 0 matches |
| retired platform-default model | `grep -rn 'kimi-k2\.6' docs/ scripts/` | 0 matches |
| `nvidia-nim` duplicate rationale | `grep -c 'credits retired Sep 2025' scripts/model-library-allowlist.json` | 1 match — the `retired` block only, not also a per-provider note |

## Out of Scope

- **Test fixtures and CLI test data still naming `kimi-k2.6`** (`cli/test/*.unit.test.ts`, `src/lib/contract/protocol_test.zig`, `tests/fixtures/fleetbundle/`). They are opaque strings to those tests and carry no rate lookup; renaming them is churn without a behavioural claim. Follow-up if the platform default changes in code.
- **The `schema/400_model_library.sql` header comment**, which still describes an unauthenticated "cryptic-prefix endpoint" and an `agentctl` command name. Both are stale, both are outside this workstream's Files Changed, and correcting them is a schema-file edit that trips SCHEMA GUARD for no rate reason.
- **Writing the api-dev database.** This workstream lands the curated file; applying it to a live environment is an operational step gated on Indy's approval, taken after merge with the existing `make seed-models APPLY=1` path.
- **A non-secret metadata sidecar for `base_url`**, which would let the provider dropdown render endpoints without decrypting credentials. Named in `billing_and_provider_keys.md` §8.3 as Option B; unchanged here.
- **Pricing the four vendors whose feeds require authentication** (SiliconFlow, Evolink, Telnyx, Xiaomi MiMo) — each returns 401 without a key, so a rate cannot be verified during curation. They carry `unpriced_reason` and a note naming the blocker.

---

## Product Clarity (authoring record)

1. **Successful user moment** — a user with an Amazon Bedrock account picks Bedrock in the provider dropdown, pastes their key, selects Claude Sonnet, and the fleet runs. Today that same walk ends in *"That model is not in the model library."*
2. **Preserved user behaviour** — every one of the 16 already-priced providers keeps its exact rates and continues to activate unchanged. The one deliberate exception is `kimi` and `qwen`, whose endpoint moves to the international host their rates were always read from; that is a correction to a mismatch, not a change to what a user pays.
3. **Optimal-way check** — the unconstrained-optimal shape is that every provider's rates come from a live feed, so curation is never a recurring obligation. Eight providers reach that shape here. The remaining manual entries stay manual because their vendors publish only human-readable pages; the gap is acceptable because `stale_after_days` already warns when they age.
4. **Rebuild-vs-iterate** — iterate. The allowlist's mechanism is sound; it was simply never exercised past the flagship providers. A rebuild was considered and rejected: nothing about the file's shape caused the gap.
5. **What we build** — curated rates in the allowlist, two small correctness fixes in the seeder, one field moved in the generator, a Python invariant checker with its test, and the doc re-sync.
6. **What we do NOT build** — no schema change, no new endpoint, no UI for browsing the catalogue, no admin flow for adding a provider from the dashboard, and no automated monthly refresh job. Each is adjacent and none is needed for moment #1.
7. **Fit with existing features** — this compounds with the self-managed posture: more priced providers means more users can bring their own key and stop paying us for tokens. The feature it must not destabilize is platform-posture billing, which is why Invariant 2 forbids a zero rate reaching the cost path.
8. **Surface order** — neither. This is data and documentation; the user-facing surfaces (provider dropdown, `agentsfleet tenant provider create`) already exist and gain more valid options without changing shape.
9. **Dashboard restraint** — the provider dropdown should not advertise a provider it cannot activate. After this workstream, an unpriced provider is one the file explicitly says is unpriceable, so hiding versus showing becomes a decision the data can drive rather than a guess.
10. **Confused-user next step** — `UZ-PROVIDER-004` already tells the user to pick a listed model from `GET /v1/models` or ask for one to be added. That message stays correct and becomes far less frequent.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** five Sections split by *kind of change* rather than by provider, because the risky work is concentrated: §2's two seeder fixes and §4's generator change affect every provider at once, while §1 and §3 are data. Splitting by provider would scatter the two code changes across dozens of Dimensions and hide them.
- **Alternatives considered:** (a) price only the eleven regional duplicates of already-priced vendors — the cheapest slice, rejected because it leaves Bedrock and every other real vendor still unactivatable, which is the actual reported symptom; (b) replace the doc's §9 table with a pointer at the allowlist rather than re-syncing prose — rejected on Indy's explicit instruction, recorded below; (c) add an `unpriced` boolean rather than a typed reason — rejected because a boolean cannot distinguish *pending curation* from *permanently unpriceable*, which is the distinction that makes the file honest.
- **Patch-vs-refactor verdict:** this is a **patch**. The mechanism is correct and stays; only its inputs and two silent-failure paths change. The one structural move — `base_url` from derived to curated — is a one-line change to an existing list, made because a documented invariant is currently violated, not because the design is wrong.

## Discovery (consult log)

- **Consults** — Architecture / Legacy-Design / gate-flag triage: the question asked + Indy's decision.
  - Curation scope, asked before any edit. Indy chose to price everything findable and to give local runtimes a floor rather than leaving them empty:
    > Indy (2026-08-16): "I would like add the endpoints with the endpints we can find the price for local like vllm, we just have to say a minimal default cost. keep going and finally audit and provide a tabulated output to me on what you did in the *allowlist.json" — context: §2, §3; sets the scope at "everything with a findable price" and mandates the closing audit table.
  - Doc-versus-allowlist relationship. Indy chose prose re-sync over replacing the section with a pointer, which is why §5 corrects §9's table in place and alternative (b) above was rejected:
    > Indy (2026-08-16): "Re-sync the prose in place" — context: §5.
  - Regional endpoint policy, raised as the `kimi`/`qwen` `base_url` mismatch and answered as a general rule:
    > Indy (2026-08-16): "use international than CN" — context: §1, §4.
    > Indy (2026-08-16): "if someone wants CN they can us the endpoint via openapi compatible technique" — context: §1; this is why `cn_endpoint` is a permanent reason rather than a pending-curation state.
- **Metrics review** — no analytics/funnel playbook update required: this workstream adds no route, no user action, and no counter; the only operator-visible output is the seeder's existing diff report, whose format is unchanged.
- **Evidence map** — the spec's Dimension test names were authored before the tests existed; these are the artifacts that actually cover them. Committed tests live in `scripts/check_model_allowlist_test.py` (34 tests, all green).

  | Dimension | Covered by | Kind |
  |---|---|---|
  | 1.1, 1.3 | `RealFile.test_kimi_and_qwen_point_at_their_international_endpoints` | committed test |
  | 1.2 | `RegionAgreement.*` (5 tests) + `RealFile.test_no_cn_endpoint_carries_rates` | committed test |
  | 1.4 | `RealFile.test_every_provider_is_priced_or_reasoned` | committed test |
  | 2.1 | `SeededRates.test_currency_prefixed_provider_seeds_every_allowlisted_model` | committed test |
  | 2.2 | `SeededRates.test_zero_cache_read_seeded_as_input_not_as_free` | committed test |
  | 2.3, 2.4 | `check_api_has_fixture` + `node scripts/seed-models.mjs --fixtures` → `173 new · 0 unmanaged` | committed test + command |
  | 3.1 | `node scripts/seed-models.mjs` — every manual provider emits its rows | command |
  | 3.2 | `SeededRates.test_local_runtime_floor_is_nonzero_after_nanos_rounding` + `ActivationFloor.*` | committed test |
  | 3.3 | `PricedXorReasoned.*` (5 tests) + `RealFile.test_every_provider_is_priced_or_reasoned` | committed test |
  | 3.4 | `ZeroRates.*` (5) + `SeededRates.test_no_seeded_row_carries_a_zero_rate` | committed test |
  | 4.1, 4.3 | `node scripts/gen-provider-skeleton.mjs` twice → byte-identical output | command |
  | 4.2 | generator's `newly added` arm, exercised at 0 newcomers this pass | command |
  | 5.1 | `RealFile.test_no_provider_still_carries_the_old_uniform_note` + `grep -c 'K2\.6' docs/architecture/billing_and_provider_keys.md` → 0 | committed test + command |
  | 5.2, 5.3, 5.4 | `bash scripts/check_architecture_doc.sh` → all OK | command |

  Dimensions covered by command rather than a committed test (2.3 partial, 3.1, 4.1–4.3, 5.2–5.4) are re-run by `make lint-governance` and `scripts/check_architecture_doc.sh` in CI, so none depends on a one-off local run. The generator round-trip (4.1–4.3) is the one gap with no CI caller — a fresh worktree has no `zig-pkg`, so the generator cannot run there.
- **Skill-chain outcomes**
  - gstack `/review` — ran with three specialists (security, data-migration, testing) plus a self-run critical pass. Six findings, all dispositioned:
    - **Zero-rate seeding (critical, found in the self-run pass).** `Number(null)`, `Number("")`, `Number("0")` and `rate("$")` all yield `0`, which is finite, so the existing `Number.isFinite` check seeded a zero rate — free inference under platform posture. Reachable before M168; this workstream took the api-source path from 2 providers to 12, which is what changed the blast radius. Fixed with `isBillable`, plus 19 direct tests.
    - **Generation bump had zero coverage (critical, testing).** The committed fixture is emitted with `no_transaction: true`, so it structurally cannot contain the block. `emit()` is now exported and tested on both paths, asserting lock-before-rows and bump-after-rows by index rather than by presence.
    - **`resolveSelfManagedCap` local-runtime path untested end-to-end (critical, testing).** Fixed: `test_activate_local_runtime_skips_catalogue` activates `ollama` with an uncatalogued model, asserts a hosted provider naming the same model still fails closed, and asserts a blank model is still refused for a local runtime.
    - **Allowlist checker scraped Zig source without stripping comments (informational, security).** Fixed; a missing closing brace now raises instead of scanning to end-of-file.
    - **SQL `--` comments interpolated unescaped (informational, security).** Not reachable — all three fields are allowlist-config-derived, never taken from a remote feed body. Fixed anyway so that stays a property rather than a dependency.
    - **Generator merge logic not unit-testable (informational, testing).** Open. `nullclawSrc()` runs at module top level and throws without a vendored `zig-pkg/`, so the module cannot be imported in a fresh worktree. Regeneration idempotency is currently proven by command (two runs, byte-identical output), not by a committed test.
  - Security specialist found **no critical issue** in the activation-gate bypass. It traced `provider` to tenant-controlled credential JSON, then confirmed `validateSecretEndpoint` forbids `base_url` for any non-`openai-compatible` provider — so claiming `provider="ollama"` cannot smuggle a dial target, because the destination is NullClaw's hardcoded localhost table. `context_cap_tokens` is a sizing hint, not a rate, and self-managed never reads the rate cache.
  - Data-migration specialist returned **NO FINDINGS**, independently verifying that the generation bump's lock/write/bump order matches the admin handler's (`revision_state.beginMutation` before `model_library_store.create`), so both writers contend for the same singleton lock in the same order.
  - gstack `/review`, second run (§7–§10) — security + testing specialists over the credential boundary. Both returned findings; every one is dispositioned:
    - **The lease discarded a keyless provider (critical, security).** Fixed as §9 — this is the finding that mattered, because §7 and §8 were inert without it.
    - **The dashboard's free-text carve-out was tested against an empty catalogue (critical, testing).** Fixed as §10.1; the first version of the new test failed against the real fixture, which is the proof the old coverage was vacuous.
    - **The REPLACE arm had no assertion (critical, testing).** Fixed as §10.3.
    - **Neither TypeScript `isLocalRuntime` had a direct test (critical, testing + informational, security).** Fixed as §10.4.
    - **Two parser defects — comment stripping cuts inside string literals, and the scan anchors on the declaration rather than the assignment (informational, security).** Both fixed, both now driven by tests that fail against the old parser (§10.6).
    - **Tautological Zig membership tests (informational, testing).** Fixed — the nine names are now literal.
    - **Local-runtime `base_url` refusal was claimed in a comment, never asserted (informational, testing).** Fixed as §10.7.
    - **Local runtimes now dial NullClaw's hardcoded localhost ports, and under `allow_all` the child shares the host netns (critical, security — confidence 6).** OPEN, surfaced to Indy rather than acted on. This is §6's consequence (catalogue membership), not §7's: the credential layer never sees the URL because `validateSecretEndpoint` forbids `base_url` for these providers, so the dial target is NullClaw's fixed table and not tenant-supplied. The prior REVIEW's security specialist traced the same path and returned no critical. Treated as a judgment call on the network posture, not a defect this diff introduces.
    - Untouched pre-existing gaps the testing specialist also raised, all outside §7–§10's changed lines: no test for `gen-provider-skeleton.mjs`'s merge precedence, `sqlComment` unexported and untested, and a loop-invariant assertion in `seed_models_test.py`. Named here so they are not mistaken for covered.
- **Orphan sweep — one live residue, deliberately not swept.** `not yet priced` is gone from the allowlist (0 hits). `kimi-k2.6` still appears in two LIVE architecture pages — `docs/architecture/user_flow.md` (4) and `docs/architecture/capabilities.md` (1) — as the example model in flow diagrams. It is the same staleness §5 corrected in `billing_and_provider_keys.md`: a model the allowlist prices nowhere. Dimension 5.1 scoped the sweep to the billing page, and these two are outside this spec's Files Changed, so they are flagged for Indy rather than edited unilaterally. The remaining hits are archived `done/` specs (history, append-only) and one deliberate allowlist note explaining the retirement.
  - `kishore-babysit-prs` — to run after the first push.
- **Deferrals**
  - Changelog entry in `~/Projects/docs`. A 39-provider addition is a user-visible behaviour change, so CHORE(close) would normally require an `<Update>`. Indy waived it:
    > Indy (2026-08-17): "skip changelog" — context: CHORE(close) changelog requirement; asked because the write is cross-repo and needs explicit approval.

    The waiver is honoured for §7–§10 too. Those sections DID land docs-repo changes (agentsfleet/docs#178) — the `UZ-PROVIDER-003` copy and the provider guide — because both described behaviour that no longer exists, which is a correction rather than an announcement. No `<Update>` block was added.
  - The generator merge-logic test above is recorded as open, not deferred — it has no Indy-acked quote, so it is incomplete scope rather than a deferral, and it is named in Out of Scope with the reason it is awkward to test as written.
- **Resolved decision — the local-runtime activation gap, both gates.** Seeding `vllm`/`ollama`/etc. at an activation floor makes the catalogue row exist, but `UZ-PROVIDER-004` matches on `(provider, model_id)` and the seeded id is the literal `local`, so a user serving `llama-3.3-70b` from their own vLLM still failed the membership check. §6 closed that. A second gate then remained: `probeSelfManagedSecret` required a non-empty `api_key` for every named provider, so a local activation still needed a placeholder key. Both are Zig changes to validation boundaries and neither was taken unilaterally — the first was scoped in this spec from the start, the second on Indy's in-session direction:
  > Indy (2026-08-17): "No, just update that fake key like unused-local thing and move on" — context: whether to fold the keyless-credential exemption into M168 or spec it separately; the agent's recommendation had been a separate spec.

  Folded in as §7 rather than deferred, because "local runtimes work" is only true when both gates yield. Tracing the same claim through the surfaces an operator actually uses then turned up three more client-side refusals (§8) — the CLI's own key check, its catalogue-membership check on `--model`, and the dashboard's Save gate plus its sentinel-only model picker. Those are not scope creep on §7; without them §7 ships a server that accepts a credential no shipped client will send.
