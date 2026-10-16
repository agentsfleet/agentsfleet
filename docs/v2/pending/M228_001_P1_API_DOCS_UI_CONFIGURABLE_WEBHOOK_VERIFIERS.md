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
**Status:** PENDING
**Priority:** P1 — customer-facing: a fleet cannot be woken by any provider outside GitHub, Linear and Slack today
**Categories:** API, DOCS, UI
**Batch:** B1 — single workstream; webhook authentication changes ship in their own Pull Request (PR)
**Branch:** pending — set at CHORE(open)
**Baseline revision:** pending — record the full comparison commit at CHORE(open)
**Test Baseline:** pending — measure declared unit and integration lanes before the Pull Request
**Baseline evidence:** pending — report path or run URL with revision, commands, passed/failed/skipped counts, and environment
**Depends on:** M227_001 — adds `POST /v1/webhooks/{fleet_id}/{source}` (`receive_source_webhook`), deletes the bare `/v1/webhooks/{fleet_id}`, refuses webhook sources no verifier covers, and deletes the never-read `signature.header`/`prefix`/`ts_header`
**Provenance:** LLM-drafted (Claude Opus 5.5, Oct 16, 2026), from source reads of `afd_webhook`, `afd_ingress`, `afd_api_ingress` and the Zig daemon at `1ad07eb21`
**Canonical architecture:** `docs/architecture/data_flow.md` §B (TRIGGER — three callers, one ingress); webhook authentication in `docs/AUTH.md` §Manual fleet-webhook auth

---

## Overview

**Goal (testable):** `a_declared_verifier_wakes_a_fleet_from_a_provider_outside_the_registry` — a fleet whose webhook trigger declares `verifier: {scheme: "body-hex-sha256", header: "x-acme-signature"}` accepts a correctly signed delivery at `/v1/webhooks/{fleet_id}/acme` and refuses every other signature, while a Svix-signed trigger verifies at its own `/v1/webhooks/{fleet_id}/{source}` URL.
**Problem:** Save accepts only `github`, `linear` and `slack` webhook sources (M227_001). Every other provider (an internal tool, Shopify, a Svix-fronted sender beside another webhook) cannot wake a fleet, although M28_001 promised a new Hash-based Message Authentication Code (HMAC) provider would be "configuration only" (`docs/v2/done/M28_001_P1_API_WEBHOOK_AUTH_MIDDLEWARE.md`). Svix (a webhook delivery service) senders land on `/v1/webhooks/svix/{fleet_id}`, which verifies against the fleet's FIRST webhook trigger and which Save never advertises.
**Solution summary:** A webhook trigger may declare a `verifier`: one family from a closed set whose signed bytes are written in code (`afd_webhook::Scheme`), plus the header name for families that sign the body alone. The per-source route verifies every delivery, Svix included, under the stored trigger's family; the sender's headers never choose it. The first-trigger Svix route is deleted. Save, the model prompt, the published OpenAPI schema and the docs list the families from the same constants.

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(webhooks): triggers declare their verifier; one URL per source
- **Intent (one sentence):** Let a fleet owner wire any HMAC-signing provider or Svix sender to a fleet by declaring how it signs, without a daemon release and without a URL that guesses the trigger.
- **Handshake** — pending until the implementing agent performs PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `rustd/crates/afd_webhook/src/scheme.rs` — the closed family enum and its module rationale: signed bytes stay in code, never in configuration. Every new family is a new arm here.
2. `rustd/crates/afd_api_ingress/src/handler/webhook/source_route.rs` — the per-source route M227_001 added; it already chooses the trigger by the URL's name.
3. `rustd/crates/afd_webhook/src/vendor/svix.rs` + `rustd/crates/afd_api_ingress/src/handler/webhook/verify_svix.rs` — the vendored Svix verifier and today's first-trigger resolution being replaced.
4. `rustd/crates/afd_ingress/src/binding/mod.rs` — `Binding::scheme`, `Binding::signature` and the `webhook_trigger` selector.
5. `docs/AUTH.md` — §Manual fleet-webhook auth: guards, the scheme registry table and the `UZ-WH-0xx` error taxonomy this spec extends.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `rustd/crates/afd_webhook/src/scheme.rs` | EDIT | Add the base64-over-body family and a stable token per family; Svix becomes a family the per-source route dispatches |
| `rustd/crates/afd_fleet_runtime/src/config/raw/trigger.rs` | EDIT | `verifier` replaces `signature` on webhook triggers; parse-time refusals |
| `rustd/crates/afd_fleet_runtime/src/config/trigger/mod.rs` | EDIT | Typed `WebhookVerifier` (tagged union, RULE TGU) on `Webhook` |
| `rustd/crates/afd_fleet_runtime/src/config/raw/openapi.rs` | EDIT | Publish `DefinitionWebhookVerifier` with the closed family list |
| `rustd/crates/afd_fleet_runtime/src/provider.rs` | EDIT | Registry sources imply their family; declared families complete the rest |
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
| `tests/fixtures/fleets/**/definition.json` | EDIT | Fixtures declaring `signature.secret_ref` move to `verifier` |
| `public/openapi.json` | EDIT | Regenerated |
| `docs/AUTH.md`; `docs/architecture/data_flow.md`; `docs/architecture/fleet_definitions.md` | EDIT | Families, declaration, one URL per source |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/last-delivery.ts` | EDIT | Its comment and delivery lookup assume a shared Svix URL |
| `~/Projects/docs` `fleets/webhooks.mdx` (own branch) | EDIT | Published webhook guide: declaring a verifier, error rows |

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
- **Dimension 1.5** — every `Scheme` arm has a token and appears in `Scheme::ALL` → Test `every_family_is_declarable_and_listed`

### §2 — One URL per source, Svix included

`POST /v1/webhooks/{fleet_id}/{source}` verifies under the stored trigger's family; a Svix delivery claims on its signed `svix-id`. `POST /v1/webhooks/svix/{fleet_id}` is deleted and answers 404 (RULE EP4: pre-launch, no deployed callers). **Decided (D2):** one route for every family, because the stored setup — not the path — chooses the verifier.

- **Dimension 2.1** — a Svix-signed delivery to `/{fleet_id}/clerk` verifies under the stored `svix-v1` trigger → Test `a_svix_trigger_verifies_at_its_own_source_url`
- **Dimension 2.2** — Svix headers sent to a `body-hex-sha256` trigger, and a GitHub-style header sent to a `svix-v1` trigger, are refused `UZ-WH-010` → Test `a_sender_cannot_choose_the_verifier`
- **Dimension 2.3** — a replayed `svix-id` within the claim window produces no second event → Test `a_replayed_svix_id_is_a_duplicate`
- **Dimension 2.4** — `POST /v1/webhooks/svix/{fleet_id}` answers 404 and the route count drops by one → Test `the_first_trigger_svix_route_is_gone`
- **Dimension 2.5** — a fleet with a `github` and a `svix-v1` trigger verifies each at its own URL, and Save advertises both → Test `two_families_on_one_fleet_each_verify_at_their_own_url`

### §3 — Save, prompt, schema and docs say the same thing

- **Dimension 3.1** — the model prompt lists every family token from the `Scheme` constants and nothing else → Test `the_prompt_lists_exactly_the_declarable_families`
- **Dimension 3.2** — the published schema and the parser agree on the family list, the header bound and the fixed-header refusal → Test `the_verifier_schema_and_parser_agree`

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
| Replayed `svix-id` | Same signed id resent | 200 `status: duplicate`; one event |
| Sender picks a verifier | Another family's headers on the request | Verified only under the stored family → 401 `UZ-WH-010` |
| Unknown family or encoding | Setup names an unlisted token | Parse refusal naming accepted tokens; Save never stores it |
| Header on fixed family / bad header | `header` on `svix-v1`, or non-token characters | Parse refusal with its sentence |
| Unverifiable source | No registry family, no declaration | Parse refusal naming sources and families |
| Secret missing or not `whsec_` | Vault row absent or wrong shape | 401 `UZ-WH-020` before any comparison |
| Old Svix URL | Sender still posts to `/v1/webhooks/svix/{fleet_id}` | 404 |

## Invariants

1. The verifier comes only from the stored trigger — `verified` takes the family from `Binding`, and no request field reaches family selection; Dimension 2.2 sends every other family's headers.
2. Signed bytes live in code — `Scheme` stays a closed enum without `#[non_exhaustive]`; every `match` is exhaustive, and Dimension 1.5 proves `Scheme::ALL` is total.
3. An unverifiable trigger never saves — refusal at parse (RULE RSP); Dimension 1.4.
4. Tags compare in constant time — only through `HmacSha256Tag::verify` (RULE CTM/CTC).
5. The body is verified before it is parsed — `verified` runs before any JSON read; Dimension 1.2 signs non-UTF-8 bytes.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| not applicable — no product/operator signal changes | not applicable | Refusals keep their `UZ-WH-0xx` codes and existing response shape | none | No secret, tag or body ever logged | `a_sender_cannot_choose_the_verifier` |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | integration | `a_declared_verifier_wakes_a_fleet_from_a_provider_outside_the_registry` | Real router: `sha256=<hex>` under `x-acme-signature` → 202 and one event; one flipped byte → 401 `UZ-WH-010` |
| 1.2 | unit | `a_base64_family_verifies_the_raw_body` | Base64 tag over non-UTF-8 bytes verifies; the hex rendering of the same tag is refused |
| 1.3 | unit | `a_verifier_that_cannot_hold_is_refused_at_parse` | `svix-v1` + header, `github` + verifier, header `x acme` → each refusal sentence |
| 1.4 | unit | `an_unverifiable_trigger_never_parses` | `sha512-hex` → sentence lists all five tokens; `acme` with no verifier → sentence lists sources and families |
| 1.5 | unit | `every_family_is_declarable_and_listed` | Each arm round-trips through its token; `Scheme::ALL` length equals arm count |
| 2.1 | integration | `a_svix_trigger_verifies_at_its_own_source_url` | Svix vector at `/{id}/clerk` → 202; same body without `svix-signature` → 401 `UZ-WH-010` |
| 2.2 | integration | `a_sender_cannot_choose_the_verifier` | Svix headers on a body-hex trigger → 401; `x-hub-signature-256` on a Svix trigger → 401 |
| 2.3 | integration | `a_replayed_svix_id_is_a_duplicate` | Second delivery with the same `svix-id` → `status: duplicate`; event count stays 1 |
| 2.4 | integration | `the_first_trigger_svix_route_is_gone` | `POST /v1/webhooks/svix/{id}` → 404; pinned route count drops by one |
| 2.5 | e2e | `two_families_on_one_fleet_each_verify_at_their_own_url` | Save reply lists both URLs; GitHub and Svix deliveries each append one event to the right trigger |
| 3.1 | unit | `the_prompt_lists_exactly_the_declarable_families` | Prompt names the five tokens and no other family word |
| 3.2 | unit | `the_verifier_schema_and_parser_agree` | Schema enum equals parser tokens; schema and parser both refuse a header on `svix-v1` |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | A declared verifier wakes a fleet from an unregistered provider (§1) | `cd rustd && cargo test --all-features -p afd_api a_declared_verifier_wakes -- --include-ignored` | `1 passed` | P0 | |
| R2 | Svix verifies per source and senders cannot pick (§2) | `cd rustd && cargo test --all-features -p afd_api svix -- --include-ignored` | `0 failed` | P0 | |
| R3 | The first-trigger Svix route is gone everywhere | `git grep -n 'webhooks/svix/{fleet_id}' -- rustd docs public ':!docs/v*/done'` | 0 matches | P0 | |
| R4 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | |
| S1 | Conform gates green | `make harness-verify` | exit 0 | P0 | |
| S2 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S3 | Lint passes | `make lint-all` | exit 0 | P0 | |
| S4 | Integration passes | `make test-integration-rustd` | exit 0 | P0 | |
| S5 | Versions agree | `make check-version` | exit 0 | P0 | |
| S6 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S7 | No oversize source file | `git diff --name-only origin/main...HEAD \| grep -v '\.md$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | |
| S8 | Orphan sweep | Dead Code Sweep greps | 0 matches | P0 | |

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
- **Metrics review** — no product or operator signal changes; refusals keep their codes. No analytics/funnel playbook update required.
- **Skill-chain outcomes** — pending: `/orly-write-unit-test`, `/orly-write-integration-test`, `/review`, `orly-babysit-prs`.
- **Deferrals** — > Indy (2026-10-16 00:15): "A: refuse + spec (recommended)" — context: M227_001 refuses unverifiable webhook sources and defers configurable verifiers and per-source Svix to this spec.
