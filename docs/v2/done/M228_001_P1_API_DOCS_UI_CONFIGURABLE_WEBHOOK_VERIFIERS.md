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

# M228_001: A webhook trigger declares its verifier, and every source gets one URL

**Prototype:** v2.0.0
**Milestone:** M228
**Workstream:** 001
**Date:** Oct 16, 2026
**Status:** DONE
**Priority:** P1 — customer-facing: a fleet cannot be woken by any provider outside GitHub, Linear and Slack today
**Categories:** API, DOCS, UI
**Batch:** B1 — one worktree and Pull Request (PR), with focused webhook and connector commits; expanded by the recorded user decision below
**Branch:** codex/m228-webhooks-connectors
**Baseline revision:** 289d226856e65d459f1b24de7b60519f897ef2d3
**Test Baseline:** measured at the exact comparison revision — unit passed; integration stopped with 627 passed and one concurrent authoring pool-capacity failure. Full counts and setup limits are recorded below.
**Baseline evidence:** `docs/v2/evidence/M228_VERIFICATION.md`; isolated baseline logs `/tmp/m228-baseline-unit-isolated.log` and `/tmp/m228-baseline-integration-isolated.log`.
**Depends on:** M227_001 — adds `POST /v1/webhooks/{fleet_id}/{source}` (`receive_source_webhook`), deletes the bare `/v1/webhooks/{fleet_id}`, refuses webhook sources no verifier covers, and deletes the never-read `signature.header`/`prefix`/`ts_header`
**Provenance:** LLM-drafted (Claude Opus 5.5, Oct 16, 2026), from source reads of `afd_webhook`, `afd_ingress`, `afd_api_ingress` and the Zig daemon at `1ad07eb21`
**Canonical architecture:** `docs/architecture/data_flow.md` §B (TRIGGER — three callers, one ingress); webhook authentication in `docs/AUTH.md` §Manual fleet-webhook auth

---

## Overview

**Goal (testable):** `a_declared_verifier_wakes_a_fleet_from_a_provider_outside_the_registry` — a fleet whose webhook trigger declares `verifier: {scheme: "body-hex-sha256", header: "x-acme-signature"}` accepts a correctly signed delivery at `/v1/webhooks/{fleet_id}/acme` and refuses every other signature, while a Svix-signed trigger verifies at its own `/v1/webhooks/{fleet_id}/{source}` URL.
**Problem:** Save accepts only `github`, `linear` and `slack` webhook sources (M227_001). Every other provider (an internal tool, Shopify, a Svix-fronted sender beside another webhook) cannot wake a fleet, although M28_001 promised a new Hash-based Message Authentication Code (HMAC) provider would be "configuration only" (`docs/v2/done/M28_001_P1_API_WEBHOOK_AUTH_MIDDLEWARE.md`). Svix (a webhook delivery service) senders land on `/v1/webhooks/svix/{fleet_id}`, which verifies against the fleet's FIRST webhook trigger and which Save never advertises.
**Solution summary:** A webhook trigger may declare a `verifier`: one family from a closed set whose signed bytes are written in code (`afd_webhook::Scheme`), plus the header name for families that sign the body alone. The per-source route verifies every delivery, Svix included, under the stored trigger's family; the sender's headers never choose it. The first-trigger Svix route is deleted. Save, the model prompt, the published OpenAPI schema and the docs list the families from the same constants.

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(connectors): reliable connections and declared webhook verifiers
- **Intent (one sentence):** Let owners connect applications with clear completion feedback and wire signed webhook senders to their own fleet source URLs.
- **Handshake:** Implement the existing verifier design and the reported Slack, Jira and loading failures in this worktree. Assumptions: retain provider permissions; a connection records authorization rather than promising every application action; identify Jira's failing boundary before changing it. Accepted by the user's start instruction in Discovery.

## Implementing agent — read these first

1. `rustd/crates/afd_webhook/src/scheme.rs` — the closed family enum and its module rationale: signed bytes stay in code, never in configuration. Every new family is a new arm here.
2. `rustd/crates/afd_api_ingress/src/handler/webhook/source_route.rs` — the per-source route M227_001 added; it already chooses the trigger by the URL's name.
3. `rustd/crates/afd_webhook/src/vendor/svix.rs` + `rustd/crates/afd_api_ingress/src/handler/webhook/verify_svix.rs` — the vendored Svix verifier and today's first-trigger resolution being replaced.
4. `rustd/crates/afd_ingress/src/binding/mod.rs` — `Binding::scheme`, `Binding::signature` and the `webhook_trigger` selector.
5. `docs/AUTH.md` — §Manual fleet-webhook auth: guards, the scheme registry table and the `UZ-WH-0xx` error taxonomy this spec extends.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `rustd/crates/afd_webhook/src/family.rs` | CREATE | Closed verifier families and stable tokens; Svix dispatches through the per-source route |
| `rustd/crates/afd_fleet_runtime/src/config/raw/trigger.rs` | EDIT | `verifier` replaces `signature` on webhook triggers; parse-time refusals |
| `rustd/crates/afd_fleet_runtime/src/config/trigger/mod.rs` | EDIT | Typed `WebhookVerifier` (tagged union, RULE TGU) on `Webhook` |
| `rustd/crates/afd_fleet_runtime/src/config/raw/openapi.rs` | EDIT | Publish `DefinitionWebhookVerifier` with the closed family list |
| `rustd/crates/afd_ingress/src/binding/mod.rs` | EDIT | The binding's family comes from the declaration or the registry, never the request |
| `rustd/crates/afd_ingress/src/secret.rs` | EDIT | The Svix `whsec_` secret is read by `credential_name`, like every other family |
| `rustd/crates/afd_api_ingress/src/handler/webhook/verify.rs` | EDIT | One `verified` dispatching on the stored family, Svix included |
| `rustd/crates/afd_api_ingress/src/handler/webhook/source_route.rs` | EDIT | Svix deliveries claim on the signed `svix-id`; others keep the body digest |
| `rustd/crates/afd_api_ingress/src/handler/webhook/svix_route.rs` | DELETE | The first-trigger Svix route |
| `rustd/crates/afd_api_ingress/src/handler/webhook/verify_svix.rs` | DELETE | Folded into `verify.rs` |
| `rustd/crates/afd_api_ingress/src/handler/webhook/mod.rs`; `rustd/crates/afd_api_ingress/src/lib.rs`; `rustd/crates/afd_api_ingress/src/openapi.rs` | EDIT | Unregister the deleted route |
| `rustd/crates/afd_http/src/route/webhook.rs` | EDIT | Drop `ReceiveSvix`; `Guard::Svix` stays for the identity route |
| `rustd/crates/afd_api/tests/webhook_svix_route.rs` | DELETE | Replaced by per-source Svix tests |
| `rustd/crates/afd_api/tests/webhook_source_route.rs`; `rustd/crates/afd_api/tests/route_meta_total.rs`; `rustd/crates/afd_api/tests/route_inventory.rs`; `rustd/crates/afd_api/tests/harness/webhook.rs`; `rustd/crates/afd_api/tests/authoring/readiness/svix.rs` | EDIT | Route count, inventory, Svix-through-source and readiness proofs |
| `rustd/crates/afd_api_tenant/src/handler/fleet/native/render/mod.rs` + `tests.rs` | EDIT | Save advertises `/{fleet_id}/{source}` for Svix triggers too |
| `rustd/crates/afd_authoring/src/model/provider/prompt.rs`; `rustd/crates/afd_authoring/src/review/credentials/purpose.rs` | EDIT | The prompt lists families from constants; credential purposes name the verifier |
| `tests/fixtures/fleets/**/definition.json`; `tests/fixtures/fleets/definition/full.json`; `tests/fixtures/fleets/definition/with_model_and_context.json` | EDIT | Fixtures declaring `signature.secret_ref` move to `verifier` |
| `public/openapi.json`; `public/llms.txt` | EDIT | Regenerate schema and update source URL discovery |
| `docs/AUTH.md`; `docs/architecture/data_flow.md`; `docs/architecture/fleet_definitions.md`; `docs/architecture/user_flow.md` | EDIT | Families, declaration, one URL per source |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/last-delivery.ts` | EDIT | Its comment and delivery lookup assume a shared Svix URL |
| `~/Projects/docs` `fleets/webhooks.mdx`; `fleets/connectors.mdx`; `api-reference/error-codes.mdx`; `changelog.mdx`; `docs.json` (own branch) | EDIT | Publish verifier setup, connector behavior, migration, matching error recovery and API navigation |
| `ui/packages/app/app/api/connectors/[provider]/callback/route.ts`; `ui/packages/app/tests/connector-callback-route.test.ts` | EDIT | Bounded callback completion and safe dashboard failure feedback |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/integrations/page.tsx`; `loading.tsx`; `copy.ts`; `components/IntegrationsConnectors.tsx`; `components/connector-rows.tsx`; `connector-actions.ts` in that directory | EDIT | Stream connection status, show pending/retry states and precise capabilities |
| `ui/packages/app/tests/integrations-page.test.ts`; `integrations-connectors.test.ts`; `connector-actions.test.ts` in that directory | EDIT | Loading, return feedback and connection capability regressions |
| `rustd/crates/afd_connector/src/complete.rs`; `jira.rs`; `grant/mod.rs`; `grant/parse/mod.rs`; `grant/parse/tests.rs`; `error/mod.rs`; `error/detail.rs`; `error/raise.rs` in that crate | EDIT if diagnosis requires | Repair the demonstrated Jira completion boundary and preserve useful failure classification |
| `rustd/crates/afd_api_tenant/src/handler/connector/callback.rs`; `rustd/crates/afd_api/tests/integration_connector_callback/**`; `rustd/crates/afd_connector/tests/integration_connect_roundtrip/**` | EDIT | Callback completion and persistence failure proofs |
| `playbooks/operations/slack_app_registration/001_playbook.md`; `docs/architecture/connectors.md` | EDIT | Correct dashboard callback and distinguish authorization from supported actions |
| `rustd/crates/afd_webhook/src/family.rs`; `rustd/crates/afd_webhook/src/lib.rs`; `rustd/crates/afd_webhook/src/test_util.rs`; `rustd/crates/afd_webhook/tests/**` | CREATE/EDIT | Family tokens, encodings and exhaustive verification proofs |
| `rustd/crates/afd_fleet_runtime/src/config/trigger/verifier.rs`; `signature.rs`; `set.rs`; `tests.rs` in that directory | CREATE/DELETE/EDIT | Replace Svix-only declaration and remove first-trigger restriction |
| `rustd/crates/afd_fleet_runtime/src/config/mod.rs`; `config/raw/mod.rs`; `error/mod.rs`; `rustd/crates/afd_fleet_runtime/tests/definition/**` | EDIT | Re-export verifier, actionable parse errors and schema parity |
| `rustd/crates/afd_ingress/src/binding/tests/**`; `rustd/crates/afd_api/tests/webhook_wall_refusals.rs`; `rustd/crates/afd_api/tests/webhook_source_delivery.rs`; `rustd/crates/afd_api/src/openapi/mod.rs` | EDIT | Existing caller and fixture migration, verification refusal regressions |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/integrations/components/connector-status.tsx`; `ui/packages/app/app/connectors/complete/page.tsx`; `ui/packages/app/app/connectors/complete/copy.ts` | CREATE | Stream status beneath the shell and show a safe callback recovery page |
| `rustd/crates/afd_http/src/services/ingress/mod.rs`; `tests.rs` in that directory; `rustd/crates/afd_api/tests/harness/stubs_ingress/**` | EDIT | Remove the Svix-only secret reader; all recipes read the credential's webhook_secret field |
| `rustd/crates/afd_credential/src/vault/inspect/mod.rs`; `handle/mod.rs`; `handle/tests/**` in that directory | EDIT | Readiness checks the same Svix signing field that ingress reads |
| `ui/packages/app/lib/connectors/completion.ts`; `ui/packages/app/tests/connector-completion.test.ts` | CREATE | Bounded same-origin return hints and callback outcome vocabulary |
| `rustd/crates/afd_ingress/src/lib.rs`; `rustd/crates/afd_api/tests/harness/webhook.rs`; `rustd/crates/afd_api/tests/ingress_plane_suite.rs`; `rustd/crates/afd_api_ingress/src/handler/webhook/github_route.rs`; `rustd/crates/afd_api_ingress/src/handler/webhook/identity_route.rs`; `rustd/crates/afd_api/tests/integration_ingress_live.rs` | EDIT | Retire first-trigger selection, migrate source-bound fixtures and remove stale route references |
| `rustd/crates/afd_api/tests/webhook_declared_svix.rs` | CREATE | Move Svix delivery and replay proofs onto the declared per-source route |
| `rustd/crates/afd_api/tests/webhook_source_route.rs`; `rustd/crates/afd_api/tests/webhook_fleet_route/**` | EDIT | Keep source-selection regression assertions aligned with the required source argument |
| `rustd/crates/afd_api/tests/ingress_live/fixture.rs`; `rustd/crates/afd_api/tests/ingress_live/declared.rs` | EDIT/CREATE | Verify custom and registry recipes through real storage and source selection |
| `rustd/crates/afd_connector/src/connection.rs`; `rustd/crates/afd_connector/src/grant/holding.rs`; `rustd/crates/afd_connector/src/grant/catalogue.rs`; `rustd/crates/afd_vault/src/load.rs`; `rustd/crates/afd_vault/src/sql/mod.rs`; `rustd/crates/afd_vault/src/sql/select_named_envelopes.sql`; `rustd/crates/afd_api_wire/src/connector.rs`; `rustd/crates/afd_api_tenant/src/handler/connector/catalogue.rs` | EDIT/CREATE | Return workspace connector status and labels in one API response, with bounded batched storage reads |
| `ui/packages/app/lib/api/connectors.ts`; `ui/packages/app/tests/connectors-api-client.test.ts`; `rustd/crates/afd_connector/tests/integration_connect_roundtrip/**`; `rustd/crates/afd_vault/tests/integration_workspace_scope.rs`; `rustd/crates/afd_api/tests/integration_connector_status/**` | EDIT | Regress one-call rendering, label isolation and large provider token completion |

| `VERSION`; `rustd/Cargo.toml`; `rustd/Cargo.lock`; `cli/package.json` | EDIT | Minor feature release and repository version synchronization |

| `rustd/crates/afd_api/tests/integration_connector_refresh.rs` | EDIT | Assert refresh-only stored grants for Linear and Zoho |
| `docs/v2/evidence/M228_VERIFICATION.md` | CREATE | Record exact baseline and feature checks, review findings and deployment limits |
| `ui/packages/app/lib/api/connectors-types.ts` | DELETE | Remove the status constants and documentation link after their last callers moved to catalogue entries |

## Applicable Rules

- **`.orly/docs/greptile-learnings/RULES.md`** — CTM and CTC (every tag through `HmacSha256Tag::verify`), TGU (`WebhookVerifier` is a tagged union, not optional fields), RSP (refuse unsupported families and headers at parse, never at delivery), EP4 (the Svix route's removal states its client-visible 404), ORP and NDC (the deleted route, verifier and test), UFS (family tokens and header names are named constants), TST-NAM, TNM, TCF. RULE CFG argues for configuration-driven verification; it conflicts with `scheme.rs`'s recorded design and is owner decision D1 below.
- `.orly/dispatch/write_rust.md` + `docs/RUST_ERROR_STANDARD.md` — every fallible signature this touches.
- `.orly/docs/REST_API_DESIGN_GUIDELINES.md` §6 (removing an endpoint, regenerating OpenAPI) and §7 (guards; an open route documents what verifies its caller).
- `docs/AUTH.md` — read before any edit (auth-flow guard, AGENTS.md Operational defaults).

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| UFS GATE | yes — new tokens and header names | Each family token and fixed header is a `pub const` beside its arm |
| ERROR REGISTRY GATE | maybe — only if a new code is needed | Reuse `UZ-WH-010/011/020`; parse refusals reuse the definition error kind |
| MILESTONE-ID GATE | yes — Rust and docs edits | No milestone identifiers in source or tests |
| LOGGING GATE | no — no new log events | Refusals return their codes; no new events |
| File & Function Length (≤350/≤50/≤70) | yes — `verify.rs` absorbs Svix | Keep Svix verification in its own `verify/svix.rs` module if `verify.rs` nears 350 |

## Prior-Art / Reference Implementations

- **Reference:** `afd_webhook::Scheme` (closed enum, signed bytes per arm) and `afd_webhook::vendor::svix` (vendored upstream verifier with recorded local patches) — this spec adds arms and a declaration; it does not invent a verifier. The M227_001 per-source route is the routing pattern Svix moves onto.

## Sections (implementation slices)

### §1 — The trigger declares its verifier family

A webhook trigger names one family by token; families that sign the raw body alone (`body-hex-sha256`, `body-hex`, `body-base64-sha256`) also take the header the signature arrives in. `slack-v0` and `svix-v1` read fixed headers. Registry sources (`github`, `linear`, `slack`) imply their family and must not declare one. **Decided (D1):** families stay a closed enum in code, and only the header name is authored, because `scheme.rs` records that signed bytes expressed as configuration are a small language nobody reads.

- **Dimension 1.1** — a `body-hex-sha256` declaration with header `x-acme-signature` verifies a signed delivery and refuses a wrong one → Test `a_declared_verifier_wakes_a_fleet_from_a_provider_outside_the_registry`
- **Dimension 1.2** — `body-base64-sha256` verifies a base64 tag over the raw body → Test `a_base64_family_verifies_the_raw_body`
- **Dimension 1.3** — a header on `slack-v0` or `svix-v1`, a registry source declaring a verifier, or a header outside the token charset is refused at parse with its sentence → Test `a_verifier_that_cannot_hold_is_refused_at_parse`
- **Dimension 1.4** — an unknown family token is refused naming every accepted token; a source with no registry family and no declaration is refused naming the sources and families → Test `an_unverifiable_trigger_never_parses`
- **Dimension 1.5** — every `Family` arm has a token and appears in `Family::ALL` → Test `every_family_is_declarable_and_listed`

### §2 — One URL per source, Svix included

`POST /v1/webhooks/{fleet_id}/{source}` verifies under the stored trigger's family; a Svix delivery claims on its signed `svix-id`. `POST /v1/webhooks/svix/{fleet_id}` is deleted and answers 404 (RULE EP4: pre-launch, no deployed callers). **Decided (D2):** one route for every family, because the stored setup — not the path — chooses the verifier.

- **Dimension 2.1** — a Svix-signed delivery to `/{fleet_id}/clerk` verifies under the stored `svix-v1` trigger → Test `a_signed_delivery_wakes_the_fleet_under_the_senders_own_identifier`
- **Dimension 2.2** — Svix headers sent to a `body-hex-sha256` trigger, and a GitHub-style header sent to a `svix-v1` trigger, are refused `UZ-WH-010` → Test `a_family_never_borrows_another_familys_proof`
- **Dimension 2.3** — a replayed `svix-id` within the claim window produces no second event → Test `a_retry_of_one_delivery_is_suppressed_and_a_new_one_is_not`
- **Dimension 2.4** — `POST /v1/webhooks/svix/{fleet_id}` answers 404 and the route count drops by one → Test `the_first_trigger_svix_route_is_gone`
- **Dimension 2.5** — a fleet with a `github` and a `svix-v1` trigger verifies each at its own URL, and Save advertises both → Test `two_families_on_one_fleet_each_verify_at_their_own_url`

### §3 — Save, prompt, schema and docs say the same thing

- **Dimension 3.1** — the model prompt lists every family token from the `Family` constants and nothing else → Test `the_prompt_lists_exactly_the_declarable_families`
- **Dimension 3.2** — the published schema and the parser agree on the family list, the header bound and the fixed-header refusal → Test `the_verifier_schema_and_parser_agree`

### §4 — Diagnose Jira and make application connections dependable

Preparation output: identify the boundary behind the observed `UZ-INTERNAL-003` callback using a safe error trace or a reproducing persistence test. The implementing agent may select the local repair once reproduced; changing permissions or choosing among several authorized Jira sites requires a separate user decision. Slack's registered redirect must equal `https://app-dev.agentsfleet.net/api/connectors/slack/callback` without a trailing slash; the authenticated backend POST is not the browser redirect.

- **Dimension 4.1** — Jira's reproduced completion failure is repaired and a failed persistence attempt never reports connected → Test `a_jira_connect_lands_a_grant_naming_the_site_it_was_scoped_to`
- **Dimension 4.2** — callback refusals, invalid redirects and deadlines return safe dashboard feedback with a fresh connection retry; authorization codes and state never appear in the landing URL → Test `connector-callback-route.test.ts`
- **Dimension 4.3** — Connect Apps paints its heading and loading state before connector status completes; completed connections show authorization and provider-specific limits → Test `returns the page shell without waiting for connector data`
- **Dimension 4.5** — one authorized workspace list supplies every connector, connection flag and public account label; unrelated workspace handles and token values never appear → Test `the_catalogue_marks_only_what_is_held`
- **Dimension 4.4** — every provider uses its exact dashboard relay URL and an unsuccessful start becomes a visible retryable failure → Test `the_relay_spells_the_route_the_dashboard_mounts`

## Interfaces

```
Webhook trigger (setup document, config.x-agentsfleet.triggers[]):
  {"type":"webhook","source":"acme","credential_name":"acme",
   "verifier":{"scheme":"body-hex-sha256","header":"x-acme-signature"}}
  {"type":"webhook","source":"clerk","credential_name":"clerk-svix","verifier":{"scheme":"svix-v1"}}
  scheme ∈ body-hex-sha256 | body-hex | body-base64-sha256 | slack-v0 | svix-v1
  header: required for body-* families, refused for slack-v0 and svix-v1
POST /v1/webhooks/{fleet_id}/{source}   receive_source_webhook — every family
POST /v1/webhooks/{fleet_id}/github     unchanged
POST /v1/webhooks/svix/{fleet_id}       removed → 404
Refusals: UZ-WH-010 bad/missing signature · UZ-WH-011 stale timestamp · UZ-WH-020 secret not configured
Save reply webhook_urls[]: {source, url: "<deployment>/v1/webhooks/<fleet_id>/<source>"}
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Wrong secret | Sender and vault disagree | 401 `UZ-WH-010`; nothing appended |
| Missing signature header | Sender omits the declared header | 401 `UZ-WH-010` |
| Stale timestamp | `slack-v0`/`svix-v1` outside 300 s drift | 401 `UZ-WH-011` |
| Replayed `svix-id` | Same signed id resent | 202 `replayed: true`; one event |
| Sender picks a verifier | Another family's headers on the request | Verified only under the stored family → 401 `UZ-WH-010` |
| Unknown family or encoding | Setup names an unlisted token | Parse refusal naming accepted tokens; Save never stores it |
| Header on fixed family / bad header | `header` on `svix-v1`, or non-token characters | Parse refusal with its sentence |
| Unverifiable source | No registry family, no declaration | Parse refusal naming sources and families |
| Secret missing or not `whsec_` | Vault row absent or wrong shape | 401 `UZ-WH-020` before any comparison |
| Old Svix URL | Sender still posts to `/v1/webhooks/svix/{fleet_id}` | 404 |
| Callback refused or unavailable | Vendor, grant persistence or backend failure | Safe dashboard failure and fresh connection retry; never replay the spent code |
| Callback exceeds its deadline | Backend or vendor does not finish | Bounded wait, safe retry message, no raw JSON or query secret in the landing URL |
| Status is slow or fails | Optional connector status request stalls | Page shell remains usable; loading or retry state names the affected operation |

## Invariants

1. The verifier comes only from the stored trigger — `verified` takes the family from `Binding`, and no request field reaches family selection; Dimension 2.2 sends every other family's headers.
2. Signed bytes live in code — `Family` stays a closed enum without `#[non_exhaustive]`; every `match` is exhaustive, and Dimension 1.5 proves `Family::ALL` is total.
3. An unverifiable trigger never saves — refusal at parse (RULE RSP); Dimension 1.4.
4. Tags compare in constant time — only through `HmacSha256Tag::verify` (RULE CTM/CTC).
5. The body is verified before it is parsed — `verified` runs before any JSON read; Dimension 1.2 signs non-UTF-8 bytes.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| not applicable — no product/operator signal changes | not applicable | Refusals keep their `UZ-WH-0xx` codes and existing response shape | none | No secret, tag or body ever logged | `a_family_never_borrows_another_familys_proof` |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | integration | `a_declared_verifier_wakes_a_fleet_from_a_provider_outside_the_registry` | Real router: `sha256=<hex>` under `x-acme-signature` → 202 and one event; one flipped byte → 401 `UZ-WH-010` |
| 1.2 | unit | `a_base64_family_verifies_the_raw_body` | Base64 tag over non-UTF-8 bytes verifies; the hex rendering of the same tag is refused |
| 1.3 | unit | `a_verifier_that_cannot_hold_is_refused_at_parse` | `svix-v1` + header, `github` + verifier, header `x acme` → each refusal sentence |
| 1.4 | unit | `an_unverifiable_trigger_never_parses` | `sha512-hex` → sentence lists all five tokens; `acme` with no verifier → sentence lists sources and families |
| 1.5 | unit | `every_family_is_declarable_and_listed` | Each arm round-trips through its token; `Family::ALL` length equals arm count |
| 2.1 | integration | `a_signed_delivery_wakes_the_fleet_under_the_senders_own_identifier` | Svix vector at `/{id}/clerk` → 202; same body without `svix-signature` → 401 `UZ-WH-010` |
| 2.2 | integration | `a_family_never_borrows_another_familys_proof` | Svix headers on a body-hex trigger → 401; `x-hub-signature-256` on a Svix trigger → 401 |
| 2.3 | integration | `a_retry_of_one_delivery_is_suppressed_and_a_new_one_is_not` | Second delivery with the same `svix-id` → `replayed: true`; event count stays 1 |
| 2.4 | integration | `the_first_trigger_svix_route_is_gone` | `POST /v1/webhooks/svix/{id}` → 404; pinned route count drops by one |
| 2.5 | integration + unit | `two_families_on_one_fleet_each_verify_at_their_own_url` | Stored GitHub and Svix triggers accept their own deliveries; native render tests also verify Save lists both source URLs |
| 3.1 | unit | `the_prompt_lists_exactly_the_declarable_families` | Prompt names the five tokens and no other family word |
| 3.2 | unit | `the_verifier_schema_and_parser_agree` | Schema enum equals parser tokens; schema and parser both refuse a header on `svix-v1` |
| 4.1 | integration | `a_jira_connect_lands_a_grant_naming_the_site_it_was_scoped_to` | Simulated Atlassian exchange and site listing through real persistence → usable sealed grant; persistence refusal → no success redirect |
| 4.2 | unit | `connector-callback-route.test.ts` | Backend refusal, unsafe redirect and aborted fetch → same-origin error landing without code/state; success retains workspace URL |
| 4.3 | unit | `returns the page shell without waiting for connector data` | Deferred status promise → visible shell immediately; eventual data or refusal → meaningful card or retry |
| 4.4 | unit + integration | `the_relay_spells_the_route_the_dashboard_mounts` | All-provider relay spelling is pinned in unit tests; `integration_connector_callback` also verifies the real callback refuses failed persistence without a success redirect |
| 4.5 | integration | `the_catalogue_marks_only_what_is_held` | One workspace list includes every registered connector and its public label; other workspace handles and stored token bytes are absent |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | A declared verifier wakes a fleet from an unregistered provider (§1) | `cd rustd && cargo test --all-features -p afd_api --test ingress_plane a_declared_verifier_wakes` | `1 passed` | P0 | ✅ 1 passed; 0 failed; guarded capture 003 |
| R2 | Svix verifies per source and senders cannot pick (§2) | `cd rustd && cargo test --all-features -p afd_api --test ingress_plane webhook_declared_svix` | `0 failed` | P0 | ✅ 9 passed; 0 failed; guarded capture 004 |
| R3 | The first-trigger Svix route is gone everywhere | `git grep -nwE 'ReceiveSvix|verified_svix' -- rustd` | 0 matches | P0 | ✅ 0 matches |
| R4 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | ✅ implementation paths reviewed against the table; 0 missing |
| S1 | Conform gates green | `make harness-verify` | exit 0 | P0 | ✅ 82 source files; no violations in the implementation commit hook |
| S2 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | ✅ 4,793 Rust executions; all package coverage gates passed; final gate revalidates the committed candidate |
| S3 | Lint passes | `make lint-all` | exit 0 | P0 | ✅ All lint checks passed; final gate revalidates subsequent mechanical test helpers |
| S4 | Integration passes | `make test-integration-rustd` | exit 0 | P0 | ✅ 1,442 main + 3 exclusive passed; 0 failed |
| S5 | Versions agree | `make check-version` | exit 0 | P0 | ✅ all declared release versions match 0.71.0 |
| S6 | No secrets | `gitleaks detect` | exit 0 | P0 | ✅ 7,502 commits scanned; no leaks found; staged scan required before close |
| S7 | No oversize source file | `git diff --name-only --diff-filter=ACM origin/main -- '*.rs' '*.ts' '*.tsx' '*.js' '*.jsx' '*.py' '*.go' '*.sh' '*.sql' \\| xargs wc -l 2>/dev/null \\| awk '$1>350 && $2!="total"'` | no output | P0 | ✅ 83 source files; maximum 345; 0 over 350 |
| S8 | Orphan sweep | Dead Code Sweep greps | 0 matches | P0 | ✅ 0 retired-symbol matches; all four removed modules absent |

**Command source rule:** every declared `conform` and `verify.*` command from `.orly/orly.json` appears above verbatim. See `.orly/dispatch/lifecycle.md` for timing; baseline metadata is pending at opening and measured before the Pull Request.

**Grading protocol (VERIFY):** run each spec-specific Verify command verbatim; Graded = ✅/❌ + one decisive output line. Repository-command rows point to the final `orly gate pr` results in PR Session Notes. **Ship gate:** every required check passes before the PR is ready; missing evidence or any ❌ returns to EXECUTE. A P1 ❌ needs an Indy-acked deferral quote in Discovery; a P0 may only be MOVED under the template's three conditions.

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

| File to delete | Verify |
|----------------|--------|
| `rustd/crates/afd_api_ingress/src/handler/webhook/svix_route.rs` | `test ! -f rustd/crates/afd_api_ingress/src/handler/webhook/svix_route.rs` |
| `rustd/crates/afd_api_ingress/src/handler/webhook/verify_svix.rs` | `test ! -f rustd/crates/afd_api_ingress/src/handler/webhook/verify_svix.rs` |
| `rustd/crates/afd_api/tests/webhook_svix_route.rs` | `test ! -f rustd/crates/afd_api/tests/webhook_svix_route.rs` |

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| `ReceiveSvix` | `git grep -nw ReceiveSvix -- rustd` | 0 matches |
| `signature.secret_ref` on triggers | `git grep -n 'secret_ref' -- rustd/crates/afd_fleet_runtime tests/fixtures` | 0 matches |
| `verified_svix` | `git grep -nw verified_svix -- rustd` | 0 matches |

## Out of Scope

- Signing grammars with their own timestamp syntax (Stripe's `t=…,v1=…`) — each is a new `Scheme` arm in a later spec, never configuration.
- Per-fleet secret rotation tooling and multi-secret overlap windows.
- Event-kind allow-lists for non-GitHub sources (`source_route.rs` explains why a body carries no trustworthy kind).

---

## Product Clarity (authoring record)

1. **Successful user moment** — An owner adds an `acme` webhook in chat, pastes the URL Save returns into Acme's console, and the first signed delivery wakes the fleet.
2. **Preserved user behaviour** — GitHub, Linear and Slack triggers need no declaration and keep their URLs; GitHub keeps `/{fleet_id}/github`.
3. **Optimal-way check** — The unconstrained optimum verifies any provider from data; this spec stops at a closed family list plus a header name, because the signed bytes are the dangerous part and must stay readable code.
4. **Rebuild-vs-iterate** — Iterate: the per-source route, scheme enum and Svix vendor already exist; this connects them.
5. **What we build** — A `verifier` declaration, one base64 family, Svix on the per-source route, parse refusals, prompt/schema/docs from one constant set.
6. **What we do NOT build** — Custom timestamp grammars, a verifier editor in the dashboard, Bearer or URL-embedded secrets (`docs/AUTH.md` §What does NOT auth a webhook).
7. **Fit with existing features** — Compounds with chat authoring and Save's advertised URLs; must not destabilize GitHub App ingress (`/v1/ingress/github`), which is a separate surface.
8. **Surface order** — API and chat first; the dashboard shows only the URLs Save returns.
9. **Dashboard restraint** — No verifier picker or "test signature" button until delivery counters per source exist.
10. **Confused-user next step** — The parse refusal names the accepted sources and families; `UZ-WH-010/020` hints name the secret to create or rotate.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** declaration (§1) before routing (§2) because the route dispatches on the declared family; §3 keeps every surface on the same constants.
- **Alternatives considered:** a fully data-driven verifier (header, prefix, timestamp header, encoding as fields; RULE CFG) — rejected (D1) because `scheme.rs` records that configuration-defined signed bytes go unread; one arm per named provider — rejected (D1) because every new provider would wait on a release; a separate `/v1/webhooks/svix/{fleet_id}/{source}` route — rejected (D2) because the stored setup, not the path, picks the verifier.
- **Patch-vs-refactor verdict:** this is a **patch** because the enum, vendor verifier and per-source route already exist; it adds arms and one dispatch.

## Discovery (consult log)

- **Consults** — Source reads: `scheme.rs` (closed enum rationale), `binding/mod.rs`, `verify_svix.rs` (`binding(fleet, None)`), and the Zig lookup at `1ad07eb21` (`serve_webhook_lookup.zig`: registry-only verification, first-trigger stopgap). Owner decisions: **D1** — > Indy (2026-10-16): "Pick our recipe + header" — closed families plus an authored header, over RULE CFG's fully data-driven verifier and one arm per named provider; **D2** — > Indy (2026-10-16): "Same URL as others" — one per-source route for every family, over a separate per-source Svix route.
- **Expanded scope:** Indy: "So we fix the M228+ the above issues in a single worktree?" followed by "Okay lets go". Covers Slack redirect, Jira completion, Connect Apps and return loading, and accurate capability text alongside the verifier work. Main-checkout changes belonging to other streams remain there.
- **Implementation grounding:** The existing `Scheme` describes three registry providers and its verification interface has no signed identifier. A closed `Family` enum declares the five recipes and carries Svix's full proof; it delegates existing recipes to `Scheme`. The vault accepts JSON objects, so all source credentials use `{webhook_secret: "…"}`, including a Svix `whsec_` value; the retired raw-string reader cannot be populated through the vault API. No secret-store format exception is added.
- **Investigation evidence:** Brave displayed Slack's redirect mismatch naming the dashboard relay. Jira consent twice reached a failed backend completion (`UZ-INTERNAL-003`); a temporary debug capture (request req_0c17db6dc5ba) places the failure after the provider exchange and before a vault transaction. The synthetic over-limit initial-token regression reproduces that boundary. Refresh handles now retain the refresh token that the broker consumes; a patched live retry remains required to confirm the diagnosis. Development logging was restored to info. Page timing and source inspection identify blocking status reads before the Connect Apps shell; provider return timing is not yet established.
- **Chat placement:** the authoring draft is the primary connection entry; fleet chat requests reconnection or access needed for a task. A shared Apps menu and workspace account settings support both. The standalone-page replacement requires its complete chat flow and is outside this reliability change.
- **Slack registration:** In Brave, replaced the API callback with `https://app-dev.agentsfleet.net/api/connectors/slack/callback` in the `agentsfleet-dev` app's Redirect URLs. Saved the setting and reloaded the page; the dashboard callback remained registered. A new Slack authorization round trip has not been run against the changed application code.
- **Metrics review** — no product or operator signal changes; refusals keep their codes. No analytics/funnel playbook update required.
- **Skill-chain outcomes** — unit regression cases pass for signing recipes, schema parity, callback recovery and catalogue rendering. Real-storage integration proofs pass for source selection, replay, workspace label isolation and Jira grant persistence. Final review and `orly-babysit-prs` outcomes belong in the pull request's Session notes; outside Claude review is disabled. The final `orly gate pr` must pass before publication.
- **Deferrals** — > Indy (2026-10-16 00:15): "A: refuse + spec (recommended)" — context: M227_001 refuses unverifiable webhook sources and defers configurable verifiers and per-source Svix to this spec.

- Oct 28, 2026: 01:10 PM — Indy requested one workspace-scoped response for connector details and status. Extend the existing list with the connection label and remove the page's separate provider reads. Keep its storage query count constant by loading only registered grant names in one batch. Decrypting those handles occurs only in the connector service; response types carry public metadata alone.
