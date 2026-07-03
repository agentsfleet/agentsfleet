<!--
SPEC AUTHORING RULES (load-bearing — do not delete):
- No time/effort/hour/day estimates anywhere. No effort columns, complexity ratings,
  percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only sequencing signal.
- If a section below contradicts these rules, the rule wins — delete the section.
- Enforced by SPEC TEMPLATE GATE (dispatch/write_spec.md) and audits/spec-template.sh.
-->

# M108_002: Six providers on the connector platform — Grafana, Zoho Desk, Jira, Linear, Fly, Datadog

**Prototype:** v2.0.0
**Milestone:** M108
**Workstream:** 002
**Date:** Jul 02, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — Indy's stated integration targets; each must connect with "low lift" as proof the platform base earns its keep.
**Categories:** API, DOCS, UI
**Batch:** B2 — starts after M108_001's registry + bounded-fetch gates clear
**Branch:** feat/m108-connector-platform (follows M108_001 on the same branch/PR — #468 merged before M108 implementation began; see M108_001 Discovery)
**Test Baseline:** unit=2270 integration=243
**Depends on:** M108_001 (registry, archetypes, bounded outbound — the base these entries plug into)
**Provenance:** LLM-drafted (Claude Fable 5, Jul 02, 2026) — implementing agent cross-checks each provider's endpoints against current vendor docs at EXECUTE

**Canonical architecture:** `docs/architecture/connectors.md` (created by M108_001) — registry/archetype shape; `docs/AUTH.md` §OAuth connectors — trust anchors.

---

## Implementing agent — read these first

1. `src/agentsfleetd/http/handlers/connectors/registry.zig` (after M108_001) — the table these six become entries in; slack + github entries are the shape to mirror.
2. `src/agentsfleetd/http/handlers/connectors/slack/callback.zig` post-M108_001 hook — the per-provider delta pattern (exchange-body parse → vault-handle shape).
3. `src/agentsfleetd/credentials/` (`integration.zig`, `serve_broker.zig`) — the broker's injectable-Deps mint pattern (GitHub installation-token mint is the refresh-mint's structural twin).
4. `docs/AUTH.md` §OAuth connectors — `<provider>-app` bags, `fleet:<provider>` handles, sensitive-data table (extend both tables for the new providers, same commit).
5. Vendor OAuth/token docs for Zoho Desk, Atlassian (Jira), Linear — verify endpoints/scopes at EXECUTE against live vendor docs; the spec pins *shapes*, not URLs.

---

## PR Intent & comprehension handshake

- **PR title (eventual):** part of PR #468 — providers slice of `feat(m106+m108)`
- **Intent (one sentence):** a workspace can connect Grafana, Zoho Desk, Jira, Linear, Fly, and Datadog from the dashboard, each landing a vaulted credential the broker can mint from — with every vendor call bounded.
- **Handshake (agent fills at PLAN, before EXECUTE):** Orly understands this as: add the six provider connector entries and surfaces on top of M108_001's connector platform, proving OAuth and API-key archetypes end-to-end with vaulted credentials, bounded vendor calls, broker minting for refresh-token providers, registry-driven dashboard cards, and docs. `ASSUMPTIONS I'M MAKING:` M108_001 is landed on `main`; the branch is `feat/m108-connector-platform`; GitHub and Slack behaviour stays unchanged while the new providers reuse the shipped mechanisms.

---

## Product Clarity

1. **Successful user moment** — an operator opens Connectors, sees eight cards (GitHub, Slack + these six), clicks Datadog, pastes an API key + site, and the card flips to connected after a live validation probe — same afternoon, no deploy.
2. **Preserved user behaviour** — GitHub + Slack connect flows and their cards unchanged; existing vault handles untouched; fleet-trigger webhooks unaffected.
3. **Optimal-way check** — optimal is also per-provider *capability* surfaces (list Jira issues, query Datadog monitors); deliberately out — this workstream is connectors (auth + credential plumbing) per the platform terminology; integrations build on it later.
4. **Rebuild-vs-iterate** — pure iterate: six data entries + two broker mint archetypes on shipped mechanism.
5. **What we build** — six registry entries (3 oauth2: Zoho Desk, Jira, Linear; all refresh-token handles per current vendor docs; 3 api_key: Datadog, Grafana, Fly); api_key connect surface with live validation probe; broker refresh-mint for Zoho + Jira + Linear; catalog endpoint; dashboard cards + api-key form; docs pages + changelog.
6. **What we do NOT build** — per-provider product capabilities (integrations); webhook/event ingress for any of the six; token rotation UI; multi-instance per provider per workspace (one handle per provider, matching GitHub/Slack).
7. **Fit with existing features** — compounds the credential broker (runner-side mints gain six providers) and the M108_001 registry; must not destabilize the GitHub/Slack connect flows (shared generic routes).
8. **Surface order** — API + minimal UI in the same slice: connectors are dashboard-initiated by design (AUTH.md — browser redirect round-trip; no CLI surface exists for connect).
9. **Dashboard restraint** — cards render from the catalog endpoint (registry-driven), never a hand-maintained list; no capability claims on a card beyond connection status + identity; no "manage" controls until real capabilities exist.
10. **Confused-user next step** — an unconfigured OAuth provider's card carries the 503 `UZ-CONN-001` docs link (platform-app provisioning); a failed api_key probe returns 400 with the vendor's rejection reason surfaced.

---

## Applicable Rules

- `docs/greptile-learnings/RULES.md` — **UFS** (provider ids in `common` constants; scope strings named), **VLT** (keys/refresh tokens vault-only, never logged/echoed back), **TGU** (per-archetype data shapes), **NDC** (no speculative provider fields), **WAUTH** (connect/status workspace-scoped), **PRI** (probe responses are vendor-controlled input — never interpolated into SQL/logs unsanitized).
- `dispatch/write_zig.md` — registry conventions; `dispatch/write_auth.md` + `docs/AUTH.md` — every flow here is auth-flow.
- `docs/REST_API_DESIGN_GUIDELINES.md` — catalog endpoint + api_key connect request shape.
- `dispatch/write_ts_adhere_bun.md` + UI/DESIGN TOKEN gates — dashboard cards/form.
- `docs/LOGGING_STANDARD.md` — probe/mint failures carry error codes; no secret material in any log record.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes | façade read; cross-compile both linux targets |
| PUB / Struct-Shape | yes — archetype data types | verdict per new pub; entries are data, not new mechanisms |
| File & Function Length | yes | one file per provider hook if any hook exceeds trivial size; registry stays a table |
| UFS | yes | provider ids/scopes/site enums named once; cross-runtime provider ids shared verbatim with the UI catalog consumption |
| UI Substitution / DESIGN TOKEN | yes — dashboard cards + form | design-system primitives only; token utilities; no raw hex/arbitrary values |
| LOGGING / ERROR REGISTRY | yes | new codes: `UZ-CONN-005` (api-key probe rejected), `UZ-CONN-006` (refresh-mint failed) registered same commit |
| SCHEMA GUARD | no | vault + existing tables only; no schema change |

---

## Overview

**Goal (testable):** each of the six providers connects end-to-end against a loopback fake vendor in integration tests — oauth2 providers complete state→authorize→callback→vault-handle (Zoho/Jira handles carry a refresh token the broker can mint a fresh access token from; Linear stores its long-lived token), api_key providers validate via a bounded live probe before the vault write and reject bad keys with 400 `UZ-CONN-005` — and the dashboard renders all cards from `GET /v1/connectors` with no hard-coded provider list.

**Problem:** Indy's next integrations (Grafana, Zoho Desk, Jira, Linear, Fly, Datadog) have no connect surface; two of them (Zoho, Jira) issue short-lived access tokens that require refresh-token minting the broker doesn't do yet; three (Datadog, Grafana, Fly) don't do OAuth at all — they need a first-class API-key mode that the platform currently lacks.

**Solution summary:** six `ConnectorSpec` entries on the M108_001 registry — the oauth2 archetype gains its first refresh-bearing entries and the api_key archetype its first three entries (connect = authed credential submission + bounded validation probe + vault write). The credential broker gains a refresh-mint archetype (mirror of the GitHub installation-token mint) for Zoho + Jira. A registry-driven catalog endpoint feeds the dashboard's connector cards and api-key form.

---

## Prior-Art / Reference Implementations

- **oauth2 entries** → the Slack entry (M108_001 migrated form) — Zoho/Jira/Linear differ only in data + hook (Jira's post-auth hook resolves and stores its cloud instance identifier the way Slack's stores `team_id`).
- **Broker mint** → `credentials/` GitHub installation-token mint (injectable Deps, cached until expiry-skew, `reconnect_required` degrade) — refresh-mint is the same shape with a token-endpoint exchange instead of an RS256-signed App mint, through M108_001's `bounded_fetch`.
- **api_key validation probe** → the M106 loopback FakeSlack test pattern for the fake vendor; the probe call itself follows `bounded_fetch`.
- **UI** → the existing GitHub/Slack connector cards in the dashboard app (`ui/packages/app`) — extend the same components; supabase `oss/supabase` packages remain the TS pattern reference.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/agentsfleetd/http/handlers/connectors/registry.zig` | EDIT | six new entries (data) |
| `src/lib/common/constants.zig` | EDIT | provider id constants for the six (UFS; single source shared with migrations/UI wire values) |
| `src/agentsfleetd/http/handlers/connectors/{zoho,jira,linear}/hook.zig` (names per local style) | CREATE | per-provider oauth2 deltas: exchange-body parse, handle shape, Jira cloud-id post-auth resolve |
| `src/agentsfleetd/http/handlers/connectors/api_key.zig` (+ per-provider probe data) | CREATE | api_key archetype connect: submission shape, bounded probe, vault write |
| `src/agentsfleetd/http/handlers/connectors/catalog.zig` + route files | CREATE/EDIT | `GET /v1/connectors` registry-driven catalog (id, archetype, configured, connected-per-workspace) |
| `src/agentsfleetd/credentials/integration.zig` | EDIT | `oauth2_refresh` Mint variant + zoho/jira/linear Id + registry entries + `OauthApp` re-export |
| `src/agentsfleetd/credentials/integration_oauth_refresh.zig` | CREATE | the refresh-mint archetype (Zoho, Jira, Linear); the declarative twin of the GitHub App mint |
| `src/agentsfleetd/credentials/integration_ctx.zig` | EDIT | `OauthApp` secret type + zoho/jira/linear platform slots; optional-bearer / content-type on `HttpRequest` (token-endpoint form POST) |
| `src/agentsfleetd/credentials/serve_broker.zig` | EDIT | `loadOauthApp` platform-secret load (`<provider>-app`) + `loadPlatformSecrets`; broker's HTTP boundary posts the form body |
| `src/agentsfleetd/credentials/testing.zig` | EDIT | `fake_oauth_app`; FakeGitHub records the request body; `brokerDeps` wires the oauth apps |
| `src/agentsfleetd/cmd/preflight.zig` | EDIT | own + free the three OAuth apps at shutdown (broker handle lifecycle) |
| `src/agentsfleetd/http/handlers/connectors/{zoho,jira,linear}/spec.zig` | EDIT | token endpoint sourced from `common.*_TOKEN_ENDPOINT` (UFS single-source with the broker) |
| `src/agentsfleetd/http/handlers/runner/credentials_mint.zig` | EDIT | **scope addition (see Discovery)** — `dispose()` made provider-aware: github→`UZ-GH-*`, oauth-refresh→`UZ-CONN-006`. §3 routes zoho/jira/linear through this path, which previously returned GitHub-only copy |
| `src/lib/common/constants.zig` | EDIT | provider id constants for the six (UFS) **+ `ZOHO/JIRA/LINEAR_TOKEN_ENDPOINT`** shared with the broker refresh-mint |
| `src/agentsfleetd/errors/error_registry.zig` | EDIT | `UZ-CONN-005`, `UZ-CONN-006` |
| `public/openapi.json` | EDIT | catalog endpoint + api_key connect body |
| `ui/packages/app` connectors page files | EDIT | cards from catalog; api-key connect form (fields per archetype data) |
| integration tests: per-provider suites (oauth trio can share one suite parameterized over entries; api_key trio likewise) + broker mint tests | CREATE | proof per Dimension |
| `docs/AUTH.md` | EDIT | provider bags + handles tables extended; refresh-mint noted |
| `~/Projects/docs/` connector pages + `changelog.mdx` | EDIT | user-facing docs (CHORE(close) discipline) |

---

## Decomposition & alternatives

- **Chosen shape:** three slices by archetype (oauth2 trio / api_key trio / broker minting) + catalog/UI + docs — each slice is one mechanism exercised by multiple data entries.
- **Alternatives considered:** one-provider-at-a-time workstreams (six specs) — rejected: the entries are data on one mechanism; six specs would be ceremony without independent risk. Capability integrations bundled in — rejected: connector ≠ integration (terminology is binding).
- **Patch-vs-refactor verdict:** **patch-sized additions on a fresh refactor** — the base absorbed the refactor risk in M108_001 by design.

---

## Sections (implementation slices)

### §1 — oauth2 entries: Zoho Desk, Jira, Linear

Registry data + thin hooks. Zoho Desk (`refresh=true`): handle stores `{integration, refresh_token, access_token, expires_at_ms, accounts_base}` — Zoho's data-center-specific base URL is captured at callback (from the token response's issuing host) rather than guessed. Jira (`refresh=true`, `offline_access` in scopes): post-auth hook resolves the cloud instance id via the accessible-resources probe (bounded) and stores it in the handle beside the refresh token. Linear (`refresh=true`): current Linear OAuth docs say refresh-token pairs are issued for OAuth apps; store the refresh token and use the same later broker mint path instead of a 24-hour access-token-only handle. **Implementation default:** scopes per provider start minimal-read + the write scope each integration target needs later can extend them in its own spec — connect-time scopes are the floor, not the ceiling.

- **Dimension 1.1 DONE** — Zoho callback → handle with refresh token; no token in `connector_installs`-like rows or logs → Test `test_zoho_callback_vaults_refresh_handle`
- **Dimension 1.2 DONE** — Jira callback → handle carries cloud instance id resolved via bounded probe → Test `test_jira_callback_resolves_cloud_id`
- **Dimension 1.3 DONE** — Linear callback → handle with refresh token (live-doc correction from Linear's Apr 2026 refresh-token migration) → Test `test_linear_callback_vaults_refresh_handle`
- **Dimension 1.4 DONE** — forged/replayed state rejected for a new-provider callback exactly as Slack's (`UZ-CONN-002`) → Test `test_new_provider_state_forgery_rejected`

### §2 — api_key entries: Datadog, Grafana, Fly

Connect for the api_key archetype is an authed `POST …/connect` whose JSON body carries the archetype's declared fields (Datadog `{api_key, app_key, site}`; Grafana `{instance_url, service_account_token}`; Fly `{org_token}`). The handler runs a **bounded validation probe** against the vendor's cheapest authenticated endpoint; success → vault handle written + 200 `{status:"connected"}`; vendor rejection → 400 `UZ-CONN-005` with the vendor's reason class, **no write**. Submitted secrets are never echoed back and never logged (VLT/PRI).

- **Dimension 2.1 DONE** — valid key (fake vendor 200) → handle in vault; status endpoint reports connected → Test `test_api_key_connect_probe_success_writes_handle`
- **Dimension 2.2 DONE** — invalid key (fake vendor 401/403) → 400 `UZ-CONN-005`, zero vault rows → Test `test_api_key_probe_rejects_no_write`
- **Dimension 2.3 DONE** — hung vendor probe → bounded_fetch deadline → 502-class `UZ-CONN-003`, zero vault rows → Test `test_api_key_probe_deadline_no_write`
- **Dimension 2.4 DONE** — api_key connect requires Bearer + `connector:write` and workspace membership (WAUTH) → Test `test_api_key_connect_workspace_scoped`

### §3 — Broker refresh-minting (Zoho, Jira, Linear)

The credential broker gains a refresh-mint archetype: given a workspace's `fleet:<provider>` handle carrying a refresh token, mint a fresh access token via the provider token endpoint (through `bounded_fetch`), cache until expiry-minus-skew (mirror the GitHub mint's cache discipline), and degrade to `reconnect_required` when the refresh token is revoked/expired — never a crash, never a raw refresh-token egress to the runner. Providers in this slice: Zoho, Jira, and Linear.

- **Dimension 3.1 DONE** — mint returns a fresh access token from a refresh handle; second mint within expiry serves the cache (fake vendor sees one exchange) → Test `broker_test.zig` "mint: an oauth2_refresh token caches within validity, re-mints past the skew (Dimension 3.1)"
- **Dimension 3.2 DONE** — revoked refresh token (fake vendor `invalid_grant`) → `reconnect_required` outcome, single bounded attempt (no retry storm); the runner mint surfaces `UZ-CONN-006` for zoho/jira/linear via provider-aware `dispose()` → Tests `integration_oauth_refresh.zig` "…invalid_grant → reconnect_required in a single attempt (Dimension 3.2)" + `credentials_mint.zig` "dispose: oauth2-refresh connectors surface UZ-CONN-006, not GitHub copy"
- **Dimension 3.3 DONE** — the runner-facing mint response carries only the short-lived access token, never the refresh token (refresh token is posted as the request credential but absent from the result) → Test `integration_oauth_refresh.zig` "…200 → access token with local expiry; refresh token posted, not returned (Dimension 3.3)"

### §4 — Catalog endpoint + dashboard cards

`GET /v1/connectors` (Bearer, `connector:read`, workspace-scoped query) renders from the registry: provider id, archetype, display name, configured (platform bag present — oauth2 only), connected (workspace handle present). Dashboard cards + the api-key form render from it; no provider list is hard-coded in the app. UI copy for an unconfigured OAuth provider links the `UZ-CONN-001` docs anchor.

- **Dimension 4.1** — catalog lists all registry entries with correct configured/connected flags → Test `test_catalog_reflects_registry_and_state`
- **Dimension 4.2** — dashboard renders cards from the catalog (unit/component tier) and the api-key form posts the archetype's declared fields → Test `test_ui_connectors_cards_from_catalog` (app unit suite)

### §5 — Docs + changelog

`docs.agentsfleet.net` connector pages gain the six providers (per-provider connect walkthrough + required credentials); AUTH.md tables extended; changelog `<Update>` per CHANGELOG_VOICE (docs-repo work on its `chore/m108-*` branch).

- **Dimension 5.1** — docs pages + changelog land in the docs repo PR; AUTH.md tables extended in this repo → verify at CHORE(close) (docs diff linked in Session Notes)

---

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| `connector_connected` | product | any provider connect completes (callback success or probe success) | provider, archetype, workspace_id | no token/key material, no vendor account identifiers beyond provider | `test_api_key_connect_probe_success_writes_handle` (asserts emit) |
| `connector_connect_failed` | product | probe rejection / exchange failure | provider, reason class (`probe_rejected`/`exchange_failed`/`vendor_timeout`) | no submitted secret, no vendor response body | `test_api_key_probe_rejects_no_write` |
| refresh-mint outcome (existing broker `metricsSink`) | ops | every mint | provider, outcome, latency | no token material (shipped invariant) | `test_refresh_mint_revoked_reconnect_required` |

Funnel: connectors page → connect click → connected is a new activation funnel; update the analytics/funnel playbook in the same PR (or Discovery records the explicit no-change reason if the playbook shape doesn't cover connectors yet).

---

## Interfaces

```
GET  /v1/connectors?workspace_id=…                       (Bearer, connector:read) → [{id, archetype, display_name, configured, connected}]
POST /v1/workspaces/{ws}/connectors/{provider}/connect   oauth2 → 200 {authorize_url}; api_key → body {fields…} → 200 {status:"connected"} | 400 UZ-CONN-005
GET  /v1/connectors/{provider}/callback                  oauth2 only (state-authed)
GET  /v1/workspaces/{ws}/connectors/{provider}           → {status, identity?}   (identity: team/site/org label, never a secret)

Broker mint (runner-facing, existing surface): provider ∈ {github, zoho, jira, …} → {access_token, expires_at_ms} | reconnect_required
```

Vault handle shapes (prose-locked): oauth2-refresh `{integration, refresh_token, access_token?, expires_at_ms?, provider-instance fields}`; api_key `{integration, key fields as submitted}`.

---

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Platform bag missing (oauth2 trio) | `<provider>-app` unprovisioned | connect → 503 `UZ-CONN-001`; catalog shows `configured:false` |
| Probe rejected (api_key trio) | bad key / wrong site | 400 `UZ-CONN-005`, vendor reason class, no vault write |
| Vendor hang (probe/exchange/mint) | upstream stalls | bounded_fetch deadline → `UZ-CONN-003` path from M108_001; no partial write |
| Refresh token revoked | user revoked at vendor | mint → `reconnect_required`; status stays connected-with-warning until reconnect (matches GitHub degrade) |
| State forged/replayed | attacker callback | 400 `UZ-CONN-002` (shipped mechanism) |
| Malformed api_key body | missing declared fields | 400 request-validation (existing taxonomy), field named, no secret echoed |
| Replayed api_key connect | double-submit | idempotent upsert of the handle (latest wins), single `connector_connected` per transition — test-pinned |
| Quota/429 from vendor probe | vendor rate limit | treated as probe rejection class with `retry` hint; no write |

---

## Invariants

1. Registry entries for the six use `common` provider constants — comptime (M108_001 validation covers new entries automatically).
2. No secret material (submitted keys, refresh/access tokens) in logs, error bodies, catalog, or status responses — enforced by handle-shape tests + LOGGING audit; status returns only identity labels.
3. Refresh tokens never leave the vault/broker boundary (runner receives only short-lived access tokens) — mint-response test.
4. Every vendor call in this workstream goes through `bounded_fetch` — the M108_001 grep invariant (E8) extends unchanged to the new files.
5. api_key connect writes the vault handle only after a successful probe — negative tests pin the no-write paths.

---

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts |
|-----------|------|------|---------|
| 1.1 | integration | `test_zoho_callback_vaults_refresh_handle` | fake token endpoint → handle fields; no secret outside vault |
| 1.2 | integration | `test_jira_callback_resolves_cloud_id` | accessible-resources fake → cloud id in handle |
| 1.3 | integration | `test_linear_callback_vaults_refresh_handle` | refresh handle stored for Linear's current token response shape |
| 1.4 | integration | `test_new_provider_state_forgery_rejected` | tampered state → 400 `UZ-CONN-002`, no rows |
| 2.1 | integration (e2e-shaped: real HTTP + fake vendor) | `test_api_key_connect_probe_success_writes_handle` | 200, handle present, `connector_connected` emitted |
| 2.2 | integration | `test_api_key_probe_rejects_no_write` | 400 `UZ-CONN-005`, zero vault rows |
| 2.3 | integration | `test_api_key_probe_deadline_no_write` | stalling fake → bounded error, zero writes |
| 2.4 | integration | `test_api_key_connect_workspace_scoped` | foreign-workspace Bearer → 403 |
| 3.1 | integration | `test_refresh_mint_caches_until_skew` | one vendor exchange for two mints inside expiry |
| 3.2 | integration | `test_refresh_mint_revoked_reconnect_required` | `invalid_grant` → `reconnect_required`, single attempt |
| 3.3 | integration | `test_mint_response_never_carries_refresh_token` | response body grep-free of refresh token bytes |
| 4.1 | integration | `test_catalog_reflects_registry_and_state` | entries + flags match seeded state |
| 4.2 | unit (app) | `test_ui_connectors_cards_from_catalog` | cards render per catalog; form posts declared fields |

Regression: GitHub/Slack suites stay green (shared routes). Idempotency/replay: api_key double-submit (Failure Modes row) → `test_api_key_connect_idempotent_upsert`.

---

## Acceptance Criteria

- [ ] All six providers connect against fakes; probes gate writes — verify: `make test-integration`
- [ ] Broker mints Zoho/Jira access tokens; refresh token never egresses — verify: `make test-integration`
- [ ] Catalog + cards registry-driven — verify: `make test-integration` + `make test` (app suite)
- [ ] `make lint` · `make test` · `make test-integration` · cross-compile both targets · `gitleaks detect` clean
- [ ] `make check-openapi` passes; E8 bounded-outbound grep still empty

---

## Eval Commands (post-implementation)

```bash
# E1-E7: same ladder as M108_001 (build, unit, integration, lint, cross-compile, gitleaks, 350-line)
# E8: bounded-outbound invariant (inherited, empty = pass)
grep -rn "std.http.Client" src/agentsfleetd/http/handlers/connectors/ src/agentsfleetd/credentials/ --include="*.zig" | grep -v bounded_fetch.zig | grep -v "_test.zig"
# E9: OpenAPI — make check-openapi
# E10: no hard-coded provider list in the app (empty = pass)
grep -rn "zoho\|datadog\|grafana" ui/packages/app --include="*.tsx" -l | xargs grep -ln "catalog\|/v1/connectors" -L 2>/dev/null
```

---

## Dead Code Sweep

N/A — no files deleted (additive data + hooks on the M108_001 base).

---

## Discovery (consult log)

- Fold + terminology + provider set inherited from M108_001 Discovery (Indy quotes there). Provider list source:
  > Indy (session, Jul 02, 2026): need Grafana, Zoho Desk, Jira, Linear, Fly, Datadog "with low lift".
- **Metrics review** — `connector_connected` / `connector_connect_failed` added (table above); funnel playbook update decision recorded at implementation.
- **§3 scope addition — runner mint error mapping (agent-initiated, pending Indy review).** `dispose()` in `http/handlers/runner/credentials_mint.zig` mapped every broker outcome to GitHub-specific copy (`ERR_GH_RECONNECT_REQUIRED` "GitHub App installation needs reconnect", `ERR_GH_MINT_FAILED`). §3 routes zoho/jira/linear through this same runner mint path, so their failures would return GitHub copy — a bug §3 activates — and Dimension 3.2 mandates `UZ-CONN-006` here. `dispose()` is now provider-aware (github→`UZ-GH-*`, oauth-refresh→`UZ-CONN-006`). This file was **not** in the original Files-Changed table; it was added because §3's mechanism directly reaches it. Surfaced to Indy at implementation (question posed; proceeded on the recommended "fix now" after no response within the window). **Reversible if Indy prefers a follow-up spec** — revert the `credentials_mint.zig` diff; the refresh mint still works, only the wire copy regresses to GitHub-labelled.

---

## Skill-Driven Review Chain (mandatory)

Same chain as M108_001 (`/write-unit-test` → `/review` → `/review-pr` per Indy's standing skip unless asked → `kishore-babysit-prs` after each push); outcomes recorded here.

---

## Verification Evidence

_§3 (broker refresh-minting) evidence — §4/§5 rows filled when those slices land._

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Unit tests (Zig) | `zig build test` | all pass (incl. new oauth2_refresh + dispose + cache tests); exit 0 | ✅ §3 |
| Integration tests | `make test-integration` | not re-run for §3 (broker mints proven with injected fake vendor, the shipped github-mint pattern; no DB-backed handler added) | ⏳ §4 |
| App unit (UI) | `make test` (app lane) | N/A for §3 (no UI in this slice) | ⏳ §4 |
| Lint | `make lint-zig` | ✓ [zig] Lint passed | ✅ §3 |
| Cross-compile | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` | both clean, exit 0 | ✅ §3 |
| Gitleaks | `gitleaks detect` | 3119 commits scanned, no leaks found | ✅ §3 |
| Bounded-outbound grep | eval E8 | one hit (`serve_broker.zig` `HttpClientExchange`) — the pre-existing sanctioned credentials-layer boundary (M102 github mint); §3 adds none, uses injected `ctx.http` | ✅ unchanged |

---

## Out of Scope

- Per-provider product capabilities (integrations — future specs per the connector/integration terminology).
- Inbound event ingress for any of the six (no webhook surfaces here).
- Token rotation/reveal UI; multi-account-per-provider; org-level (cross-workspace) connects.
- Real-vendor end-to-end runs (staging eval, M106 precedent — integration proof is loopback fakes).
