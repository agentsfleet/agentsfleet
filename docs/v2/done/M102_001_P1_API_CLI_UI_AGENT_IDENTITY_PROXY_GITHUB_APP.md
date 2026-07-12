<!--
SPEC AUTHORING RULES (load-bearing — do not delete):
- No time/effort/hour/day estimates anywhere. No effort columns, complexity ratings,
  percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only sequencing signal.
- Enforced by SPEC TEMPLATE GATE (dispatch/write_spec.md) and audits/spec-template.sh.
-->

# M102_001: Agent identity proxy — a workspace connects GitHub once, fleets mint short-lived App tokens on demand through one config-driven broker

**Prototype:** v2.0.0
**Milestone:** M102
**Workstream:** 001
**Date:** Jun 26, 2026
**Status:** DONE — §1–§4 (mint broker + tool boundary) + §5-connect shipped (PR #458). The four deferred dimensions (§5.2/§5.4 ingress, §6 grant enforcement, §7 CLI, §8 docs) are carved into **M102_005** (Indy's "split + defer the tail", Jul 05, 2026 — see Discovery). Marked per-Dimension below.
**Priority:** P1 — operator-facing: one-click "Connect GitHub" replaces the hand-pasted long-lived Personal Access Token (PAT); the broker forestalls a per-connector minter junk drawer in `agentsfleetd`.
**Categories:** API, CLI, UI
**Batch:** B1 — backend mint wire (§1–§4) before the UI-first connect surface (§5–§6); CLI ops + docs sweep (§7–§8) ride after.
**Branch:** feat/m102-agent-identity-proxy
**Depends on:** none hard. **Supersedes** M99_001 (DEFERRED — "re-spec under the agent-identity-proxy framing"); composes with M98_001 (vault UI).
**Provenance:** agent-generated (interactive CTO design session with Indy, Jun 25–26 2026; reborn from M99_001 with two refinements — App-level webhook ingress + explicit grant/approval placement). Re-confirm at PLAN.
**Test Baseline:** unit=2145 integration=206

> **Agent identity, honest scope.** The GitHub App *installation* IS the agent's non-human identity here: fleets act as the App (scoped, revocable, attributable to the App — not a human PAT). Per-fleet cryptographic identity / Agent Auth wire-format alignment is the v3 layer and is Out of Scope.

**Canonical architecture:** `docs/AUTH.md` (credential boundary — secrets ride the lease, the App private key is platform-side, the sandbox child holds no control-plane key) + `docs/architecture/data_flow.md` §B/§C + `docs/architecture/runner_fleet.md` (the `agt_r` plane). This spec adds a daemon-side broker + a child→runner→daemon mint request on the existing `agt_r` plane; **it introduces no new trust plane.**

---

## Implementing agent — read these first

1. `docs/v2/done/M99_001_P1_API_CLI_UI_CONNECTORS_CREDENTIAL_BROKER.md` — the deferred predecessor; its Interfaces/Failure-Modes/Invariants are the blueprint. **Do not edit it.**
2. `docs/AUTH.md` §"credential boundary", §"Runner token", §"Webhook auth" — the boundary, the `agt_r` plane (`/v1/runners/me/*`), the existing GitHub webhook. **Auth-flow file — read before any connect/callback/token-mint code.**
3. `src/agentsfleetd/fleet/secrets_resolve.zig` + `src/lib/contract/execution_policy.zig` — `resolveSecretsMap` is what generalizes from "load static JSON at lease" to "resolve-or-mint on demand."
4. `src/runner/engine/runtime/policy_http_request.zig` + `src/runner/engine/tool_bridge.zig` — where `${secrets.X.field}` is substituted at the tool boundary; the substitution step gains a mintable-integration path.
5. `docs/v2/reviews/m102-doc-shape-review.md` — the adversarial doc-shape review (C1–C9) the §8 sweep absorbs.

---

## PR Intent & comprehension handshake

- **PR title (eventual):** `feat(m102): agent identity proxy — GitHub App connector + on-demand token mint`
- **Intent (one sentence):** a workspace clicks "Connect GitHub" once, and from then on its fleets mint a short-lived, workspace-scoped GitHub App installation token **on demand at the moment a tool needs it** — through one config-driven broker whose next connector is a descriptor, never a new daemon branch.
- **Handshake (agent fills at PLAN):** restate intent; list `ASSUMPTIONS I'M MAKING:`. Confirm: (a) the `agt_r` plane hosts `…/credentials/mint`; (b) the child→runner local-pipe request shape; (c) GitHub App registration ownership + admin-vault key storage; (d) the App-level webhook (one URL) routed by `installation_id`, not a per-fleet URL. A `[?]` blocks EXECUTE.

---

## Product Clarity

1. **Successful user moment** — connect GitHub once at 11:00; next day a teammate opens a Pull Request (PR), the fleet wakes, mints a fresh token at that instant, reads the diff, posts a review; 6h later the operator steers "check PR #500" and it mints again on the spot. The operator never saw a token, a webhook URL, or a paste field.
2. **Preserved behaviour** — static custom secrets still resolve; the inbound webhook Hash-based Message Authentication Code (HMAC) path still verifies; model routing is untouched; a fleet with no connector behaves as today.
3. **Optimal-way check** — on-demand mint through a broker is the most direct path: the token is born at use, so idle time and trigger source stop mattering. The gap to perfect (a generic OAuth-refresh integration, per-fleet identity) is deferred — the abstraction lands now, depth later.
4. **Rebuild-vs-iterate** — iterate. `secrets_resolve` + the `agt_r` plane + the tool-boundary substitution all exist; this generalizes them. Verdict: targeted refactor (full rationale below).
5. **What we build** — a daemon-side `CredentialBroker` + config-driven integration registry; the `github` integration; a child→runner→daemon mint request; tool-boundary resolve-or-mint; the GitHub App connect/reconnect surface (UI-first) + the App-level webhook ingress routed by `installation_id`; grant + approval placement; CLI connector ops + vault surfacing; the docs sweep.
6. **What we do NOT build** — the `oauth_refresh` integration (Slack/Zoho/Jira/Linear stay "Planned", bridged by custom secrets); per-fleet cryptographic identity / Agent Auth wire format; Stripe Agentic Commerce Protocol; exact-action approval-hash binding (reuse the coarse integration grant); a standalone credentials microservice; rotation automation beyond mint-on-demand.
7. **Fit** — compounds with the M98 vault UI, the `agt_r` lease envelope, and the existing approval inbox; must not destabilize the sandbox env-allowlist, the inbound webhook verifier, or static-secret resolution.
8. **Surface order — UI-first** (Indy's call): the "Connect GitHub" button + approval inbox lead. The mint wire (§1–§4) is backend-first because every surface depends on it; CLI ops (§7) fast-follow. Divergence from CLI-first is deliberate — connect is a browser-native OAuth consent, and approvals already live in the UI.
9. **Dashboard restraint** — only GitHub shows a live "Connect"; Slack/Zoho/Linear render "Planned" with the custom-secret hint; no token, webhook URL, App key, or secret is ever displayed; connector state is real, never fabricated.
10. **Confused-user next step** — a fleet that can't reach GitHub shows a typed "Reconnect GitHub" state (CLI JSON + UI), never a silent 401; a user wanting Slack reads the inline custom-secret hint and self-serves.

---

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — **RULE CFG** (integration registry + integrations are data, never per-integration branches — the core), **VLT** (App key / minted tokens never logged or returned), **PRI/NTP** (treat connector + webhook inputs as hostile), **ECL** (reconnect / mint-failed / unknown-integration are typed classes), **EMS** (standard error structure), **TGU** (tagged-union mint result), **JCL** (CLI JSON contract), **UFS** (integration ids `"github"`/`"static"`, mint route, `${secrets.…}` shape → named constants shared verbatim with tests), **NDC/NLR/ORP**.
- **`docs/AUTH.md`** — auth-flow: mirror the boundary (App key platform-side, sandbox holds no control-plane key); reuse the `agt_r` plane + the webhook verifier.
- **`dispatch/write_zig.md`** — tagged-union results, multi-step `errdefer`, pg-drain, file ≤350 / fn ≤50, cross-compile both linux targets.
- **`dispatch/write_ts_adhere_bun.md`** — TS FILE SHAPE per new component, primitive substitution, DESIGN TOKEN gate (UI rows).
- **`docs/REST_API_DESIGN_GUIDELINES.md`** — the mint + App-ingress routes live under `src/agentsfleetd/http/handlers/**`.
- No schema Data Definition Language anticipated → handle + webhook secret ride existing vault JSON; `docs/SCHEMA_CONVENTIONS.md` applies only if an `installation_id ↔ workspace` index proves necessary.

---

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes — broker, integration, mint endpoint, channel, ingress | tagged-union results; `errdefer`; cross-compile both linux targets |
| PUB / Struct-Shape | yes — `CredentialBroker`, `Spec`, mint req/resp | shape verdict per new pub surface; tagged-union mint result |
| File & Function Length (≤350/≤50/≤70) | yes — broker + integrations | one file per integration; broker dispatch table extracted |
| UFS (repeated/semantic literals) | yes — integration ids, mint route, placeholder shape, error strings | named constants in one module per side; tests import them |
| UI Substitution / DESIGN TOKEN | yes — Connect/reconnect rows | design-system primitives; theme tokens only |
| LOGGING / ERROR REGISTRY (`UZ-XXX-NNN`) | yes — connect, mint, reconnect-required | register `UZ-GH-*` / `UZ-CRED-*` + `hint()`; no secret in any frame |
| SCHEMA | no — handle + secret ride vault JSON | — |

---

## Overview

**Goal (testable):** `CredentialBroker.mint(workspace_id, integration, scope)` returns a short-lived `{token, expires_at}` via a config-driven integration registry (`github` first); a sandboxed child obtains that token **on demand** by asking its runner, which forwards over the `agt_r` plane; the App private key never leaves the daemon; `${secrets.github.token}` transparently triggers the mint at the tool boundary; the GitHub App's single webhook routes to the right fleet by `installation_id`; and adding a connector adds a descriptor with **zero** new branches in the mint dispatch.

**Problem:** GitHub fleets need a hand-pasted PAT (long-lived, broad, at-rest) and the user must manually register a per-repo webhook. The obvious fix — mint per connector inline in `agentsfleetd` — doesn't scale: each integration bolts bespoke minting into the daemon.

**Solution summary:** a daemon-side broker with a data-driven integration registry mints short-lived workspace-scoped tokens; the child fetches them on demand through the runner so idle time and trigger source stop mattering; the App key stays platform-side; the App's single webhook is routed internally by `installation_id`; the existing verifier and `secrets_map` resolution are reused. GitHub ships as the first integration + connector; the next is a descriptor.

---

## Prior-Art / Reference Implementations

- **Backend** → generalize `secrets_resolve.zig` (name→vault-JSON) into resolve-or-mint; mirror `resolveActiveProvider`'s just-in-time, never-persisted key handling (M80_009) for the minted token's lifecycle (`secureZero` after use). **Channel** → the `agt_r` plane is the existing daemon↔runner wire; the mint route is one more on it. **Pattern** → workload-identity / instance-metadata.
- **CLI** → the "7 Pillars" (handler purity, output-as-a-service, structured-JSON errors). **UI** → M98 vault Integrations group + design-system primitives. **API** → `docs/REST_API_DESIGN_GUIDELINES.md` + nearest `src/agentsfleetd/http/handlers/` handler.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/agentsfleetd/credentials/broker.zig` | CREATE | `mint(workspace, integration, scope)`; integration-registry dispatch; cache till near expiry |
| `src/agentsfleetd/credentials/integration.zig` | CREATE | `Spec` interface + config-driven registry (id → descriptor); `static` integration |
| `src/agentsfleetd/credentials/integration_github.zig` | CREATE | GitHub-specific claims + token exchange: build the App JSON Web Token (JWT), sign it via the shared signer, POST it for an installation token; typed reconnect on revoke |
| `src/agentsfleetd/auth/crypto/rs256_sign.zig` | CREATE | generic RS256 JWT signing primitive (reused by future JWT-bearer integrations — Google/Snowflake/Salesforce); mirror of `jwks_crypto.verifyRs256`, built on `std.crypto`; the App private key is never logged or returned |
| `src/agentsfleetd/http/handlers/admin/platform_keys.zig` (or sibling) · `src/agentsfleetd/state/*` loader | EDIT/CREATE | admin store of the App private key + app id into the `agentsfleet-admin` workspace vault (mirror the `platform_llm_keys` provisioning); broker resolves it via `crypto_store.load` daemon-side — the scalable vault model, NOT env/`ServeConfig` |
| `playbooks/founding/03_priming_infra/001_playbook.md` | EDIT | drop `GITHUB_APP_ID`/`GITHUB_APP_PRIVATE_KEY` from `fly secrets set`; replace with a store-into-admin-vault step (after the store-path lands) |
| `playbooks/founding/02_preflight/02_credentials.sh` | EDIT | add `op://…/github-app/webhook-secret` ref (both envs) — §5 ingress needs the App webhook secret |
| `playbooks/operations/admin_bootstrap/001_playbook.md` (or a new connector runbook) | EDIT/CREATE | operator steps to **register** the GitHub App (permissions, webhook URL, generate key → 1Password) — the upstream human action behind the gated op-refs |
| `src/agentsfleetd/http/handlers/runner/credentials_mint.zig` | CREATE | `POST /v1/runners/me/credentials/mint`; workspace derived from the lease, never the caller |
| `src/runner/engine/credential_request.zig` | CREATE | child→runner local-pipe mint request/response |
| `src/runner/engine/runtime/policy_http_request.zig` · `tool_bridge.zig` | EDIT | mintable-integration path: `${secrets.X.token}` → broker fetch; static unchanged; partial-sub guard preserved |
| `src/agentsfleetd/fleet/secrets_resolve.zig` | EDIT | classify static vs mintable integration; emit a handle (not a token) for mintable; check the integration grant |
| `src/agentsfleetd/http/handlers/connectors/github_connect.zig` · `github_callback.zig` | CREATE | App-install flow; store `{integration:"github", installation_id}` in `fleet:github` (no token); map `installation_id ↔ workspace` |
| `src/agentsfleetd/http/handlers/webhooks/ingress.zig` | CREATE | generic `POST /v1/ingress/{provider}`; dispatch via the verifier/router registry; route by the descriptor's routing-key → workspace → matching fleet(s) |
| `src/agentsfleetd/fleet_runtime/webhook_verify.zig` | EDIT | extend `PROVIDER_REGISTRY` descriptor with `routing_key_path` + the app-level (one-URL) model; seed the `github` entry |
| `ui/packages/app/app/(dashboard)/credentials/components/IntegrationsConnectors.tsx` | CREATE/EDIT | GitHub Connect / connected / **Reconnect**; Slack/Zoho "Planned" |
| `cli/src/commands/connectors.ts` · `services/connectors.ts` | CREATE/EDIT | `agentsfleet connector` status/list; structured-JSON error on disconnected |
| _error registry module_ | EDIT | `UZ-GH-*` (reconnect/mint-failed) + `UZ-CRED-*` (unknown-integration) + `hint()` |
| `docs/architecture/{user_flow,data_flow,capabilities,high_level,README,roadmap,runner_fleet}.md` · `scenarios/github-pr-reviewer.md` | EDIT | §8 docs sweep — absorb C1–C9 (review companion) |
| `docs/v2/reviews/m102-doc-shape-review.md` | CREATE | the adversarial doc-shape review artifact |
| _colocated tests (Zig `test {}` · `*.test.tsx` · `*.spec.ts`)_ | CREATE/EDIT | one test per Dimension below |

---

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** eight Sections. The broker + integration + on-demand channel + tool-boundary path (§1–§4) are the foundation; the App connect surface + ingress (§5), grant/approval placement (§6), CLI (§7), and docs sweep (§8) ride on top. The integration registry is the refactor that prevents the per-connector junk drawer.
- **Alternatives considered:** (a) long-lived PAT per connector — rejected: at-rest broad token; (b) mint per connector inline — rejected: Indy's caveat, doesn't scale; (c) standalone credentials microservice — rejected: a new trust plane the `agt_r` plane already fronts; (d) mint-at-lease only — rejected: fails steer-after-6h-no-trigger; (e) exact-action approval-hash binding now — deferred, reuse the coarse grant.
- **Patch-vs-refactor verdict:** **targeted refactor** — generalize the existing secrets path into a broker with pluggable integrations, plus one local-pipe round-trip and one App ingress. The `oauth_refresh` integration, per-fleet identity, and exact-action approval are named follow-ups.

---

## Sections (implementation slices)

### §1 — Credential broker core + config-driven integration registry
One mint interface dispatching through a registry keyed by integration id. **Implementation default:** the registry is a data table (id → descriptor), so a new connector is a registration, not a `switch` arm (RULE CFG). The broker caches a minted token until near `expires_at`.
- **Dimension 1.1** — `mint(workspace, integration, scope)` returns `{token, expires_at}` via registry dispatch → `test_broker_dispatches_by_id` — ✅ **DONE**
- **Dimension 1.2** — adding an integration is a descriptor; the mint hot path has no per-integration branch → `test_integration_registry_is_data_driven` — ✅ **DONE**
- **Dimension 1.3** — a cached token within validity is reused; near-expiry re-mints → `test_broker_caches_until_expiry` — ✅ **DONE**
- **Dimension 1.4** — an unconfigured integration → typed `unknown_integration`, no mint → `test_broker_unknown_integration_errors` — ✅ **DONE**

### §2 — `github` integration
Sign a GitHub App token with the platform-held private key, exchange it at GitHub for a ≤1h installation access token. **Default:** the App private key + app id resolve daemon-side from the **`agentsfleet-admin` workspace vault** via `crypto_store.load` — the same scalable platform-secret model as `platform_llm_keys` (real bytes in `vault.secrets` under the admin workspace; env holds only the `ENCRYPTION_MASTER_KEY` KEK, never per-integration keys). Never `ServeConfig`/env (Indy, Jun 26: env doesn't scale across a growing integration set). The broker hands the loaded key to the integration as injected `PlatformSecrets`; §2 unit tests inject a fake (no DB, no network). The signer + outbound HTTP boundary + clock are all injected via `MintCtx`, so the integration is pure and unit-testable with a fake key + fake GitHub. **`expires_at` is bounded locally** (`now + 1h`, GitHub's documented installation-token TTL) rather than parsed from GitHub's RFC3339 string — a conservative floor re-mints early, never late, and keeps a date parser off the security path. The `UZ-GH-*` / `UZ-CRED-*` codes register in §3, where the mint endpoint first surfaces these outcomes (registering them now would be a caller-less code, RULE NDC).
- **Dimension 2.1** — valid installation handle → installation token with a near-future `expires_at` → `test_github_mints_installation_token` — ✅ **DONE**
- **Dimension 2.2** — uninstalled/revoked → typed `reconnect_required` (`UZ-GH-*` at the §3 boundary), no token → `test_github_revoked_reconnect` — ✅ **DONE**
- **Dimension 2.3** — the App key never appears in the outbound request, the minted token, or any frame → `test_github_key_never_leaves_broker` — ✅ **DONE**

### §3 — On-demand mint channel (child → runner → daemon)
The child requests a token from its runner over the local pipe; the runner forwards to the broker over the `agt_r` plane. **The workspace is derived from the lease server-side — a child-supplied workspace id is ignored.**
- **Dimension 3.1** — child request → runner forwards → short-lived token returned → `test_child_requests_token_via_runner` — ✅ **DONE** (runner-leg: real child pipe → parent read loop → real control-plane HTTP client → loopback mint stub; passes locally + CI)
- **Dimension 3.2** — a forged workspace id resolves to the lease's workspace only → `test_mint_scoped_to_lease_workspace` — ✅ **DONE** (daemon-leg: real endpoint + runner-bearer + live DB lease + real broker over the `static` integration; DB-gated, runs in CI, compiles + skips clean locally)
- **Dimension 3.3** — a long-idle session with no external trigger mints a fresh token at the tool call → `test_on_demand_mint_no_trigger` — ✅ **DONE** (runner-leg: tool-boundary `${secrets.github.token}` placeholder triggers an on-demand mint with no event; passes locally + CI)

> **§3/§4 end-to-end status (this session):** the on-demand mint path is now wired
> end-to-end in code — child placeholder → `MintResolver` → `credential_request`
> channel → daemon broker mint — and each LINK is unit-proven: the child `mint`
> round-trip + `MintResolver` dedup (`credential_request.zig`), the tool-boundary
> routing + fail-closed (`secret_substitution.zig`, `policy_http_request_test.zig`),
> the daemon classify (`secrets_resolve.zig`), the broker dispatch (`broker_test.zig`).
> Dimensions 3.1–3.3 are now written + DONE. Because the runner graph is
> isolation-gated from the daemon's pg/httpz (a single in-process child→runner→
> daemon→DB test is architecturally precluded — and undesirable), the path is
> proven by TWO legs meeting at the `/v1/runners/me/credentials/mint` wire: the
> **daemon leg** (`http/handlers/runner/credentials_mint_integration_test.zig` —
> real endpoint + runner-bearer + live DB lease + real broker over the `static`
> integration; DB-gated, runs in CI, compiles + skips clean locally) and the
> **runner leg** (`src/runner/credential_mint_e2e_test.zig` — real child pipe →
> parent read loop → real control-plane HTTP client → loopback mint stub;
> cross-platform, passes locally + CI). Both sides agree on the wire via
> `protocol_credentials.zig`; each asserts the wire carries lease_id + integration
> but NEVER a workspace (Invariant 2).

### §4 — Tool-boundary transparent resolve-or-mint
When `PolicyHttpRequestTool` resolves `${secrets.<integration>.token}` for a **mintable** integration, it fetches via the channel instead of a static value; substitution stays at the tool boundary. **Static integrations resolve as today.**
- **Dimension 4.1** [DONE] — mintable `${secrets.github.token}` triggers a broker fetch, substituted only at dispatch → `test_bridge_mints_on_placeholder`
- **Dimension 4.2** [DONE] — the partial-substitution guard holds: residual `${secrets.` aborts the call → `assertNoLeftover catches partial substitution` (unit) + the guard wired at the three `policy_http_request.execute` substitution sites (`S_SUBSTITUTION_LEFT_PLACEHOLDER`)
- **Dimension 4.3** [DONE] — a static integration's credential resolves with no mint (unchanged path) → `test_bridge_static_unchanged`
- **Dimension 4.4** [DONE] — mintable credentials ride the lease as a **typed, out-of-band** `ExecutionPolicy.mintable: []const Mintable` list (name → integration id), NEVER a reserved key inside `secrets_map`. The classifier ([RULE CFG]) routes a vault handle for an on-demand integration to that list and emits only the integration id (no handle/token bytes — VLT); `secrets_map` stays **static-only**, so the redaction set equals the substitution set by construction (no skip logic, no drift) and the wire stays additive/backward-compatible. → `test_policy_mintable_roundtrip` + `test_runner_facing_classify`
- **Dimension 4.5** [DONE] — repeated mintable placeholders within one tool call mint **once** via a per-call resolver cache; the broker still caches across calls and on-demand timing is preserved (no mint at lease/run start) → `test_mint_resolver_dedups_per_call`

  > **Design note (Bun model, locked this session — Indy):** the mint *strategy* is a tagged union (`integration.Mint = { static, custom: fn }`) — Bun's `SideEffects`/`AllowUnresolved` idiom: declarative-data variants + a function-pointer escape hatch, the union owns its `run`/`isOnDemand` dispatch so the broker never branches on id. A declarative `oauth2_refresh` variant (Zoho/Jira refresh-token grant) slots in here as DATA when its first real caller lands (M103) — not built now (untested dead code, RULE NDC). The broader Bun-idiom adoption plan (strategy unions, comptime registries, pure-core + injected effects, typed result unions) is house-style in the dotfiles `dispatch/write_zig.md` "Bun-Inspired Conventions" — it fires at `*.zig` edit time. The per-milestone sequencing (comptime registries → M102_002; the `oauth2_refresh` variant + webhook `Scheme` union → M103) is tracked in those specs.

### §5 — GitHub App connect surface (UI-first) + generic webhook ingress
A live GitHub **Connect** runs the App-install flow, storing `{integration:"github", installation_id}` (no token) and mapping `installation_id ↔ workspace`. The ingress is **generic — `POST /v1/ingress/{provider}`** — backed by an inbound verifier/router registry (RULE CFG, mirrors the outbound broker; extends the existing `webhook_verify.PROVIDER_REGISTRY`). A provider entry is a descriptor: `{scheme, header(s), prefix, timestamp_window?, routing_key_path, lifecycle_hook?}`. M102 ships ONE entry (`github`); Slack/Linear/Jira/Zoho are later descriptors (a new scheme impl or lifecycle hook only when the signature *family* is new — `hmac_sha256_body`, `hmac_sha256_ts_body`, `atlassian_jwt`). The receiver verifies the provider signature and routes by the descriptor's routing-key (`installation_id` for github) → workspace → matching fleet(s).
- **Dimension 5.1** — Connect → install → callback stores the handle (no token) + the `installation_id ↔ workspace` map → `test_github_connect_stores_handle` — ✅ **DONE** (PR #458; `connectors/github/{connect,callback,status,state}.zig`)
- **Dimension 5.2** — App webhook at `/v1/ingress/github` → verify App HMAC → route by `installation_id` to the matching fleet's events stream → `test_ingress_routes_by_installation_id` — ⏭️ **DEFERRED → M102_005 §1** (event receipt is separable from connect; not built in PR #458)
- **Dimension 5.3** — connected shows; uninstalled shows **Reconnect**; Slack/Zoho render "Planned" → `test_github_states_and_planned` — ✅ **DONE** (PR #458; `IntegrationsConnectors.tsx`)
- **Dimension 5.4** — adding a provider is a registry descriptor; the `/v1/ingress/{provider}` route + dispatch carry no per-provider branch (a fake provider entry verifies + routes with no handler edit) → `test_ingress_registry_is_data_driven` — ⏭️ **DEFERRED → M102_005 §1**

### §6 — Grant + approval placement
The standing **integration grant** (`core.integration_grants`) gates whether a fleet may use `github` at all — checked at lease-issue (no mintable handle without an approved grant) and re-checked at mint. The existing **approval gate** stays the per-write gate (poll/continuation), unchanged.
> **Split note (Jul 05, 2026):** the grant **lifecycle** (schema `008_core_integration_grants.sql`, the request/approve/revoke/list routes + `webhooks/grant_approval.zig`) shipped, but the **enforcement** below never wired — `credentials_mint.zig` calls `broker.mint` with no grant read and `secrets_resolve.mintableId` classifies without one. **Invariant 3 is not upheld on main.** Both dimensions carry to M102_005 §2.
- **Dimension 6.1** — mint refused when the fleet has no approved `github` grant → `test_mint_requires_approved_grant` — ⏭️ **DEFERRED → M102_005 §2** (enforcement absent on main)
- **Dimension 6.2** — lease attaches a mintable `github` handle only when the grant is approved → `test_lease_gates_mintable_on_grant` — ⏭️ **DEFERRED → M102_005 §2**

### §7 — CLI connector ops + vault surfacing (fast-follow)
`agentsfleet connector` lists status (connected / needs-reconnect) as human + structured JSON; a disconnected connector returns a structured-JSON error with a non-zero exit (RULE JCL).
- **Dimension 7.1** — `connector list`/`status` reflects github state; `--json` shape is stable → `test_cli_connector_status` — ⏭️ **DEFERRED → M102_005 §3** (connect shipped UI-only; CLI signpost fast-follow, Discovery §5-connect note)
- **Dimension 7.2** — acting through a disconnected connector → structured-JSON error + non-zero exit + reconnect suggestion → `test_cli_connector_disconnected_error` — ⏭️ **DEFERRED → M102_005 §3**

### §8 — Docs sweep (absorb C1–C9)
Update the architecture docs the new model changes, owners before echoes (review companion): `user_flow.md` §8.2–8.5, `data_flow.md` §B/§C, `capabilities.md` §2–3 first; then `README.md` glossary, `high_level.md` §5.1, `scenarios/github-pr-reviewer.md`, `runner_fleet.md` (one `agt_r` route), `roadmap.md`.
- **Dimension 8.1** — the manual-`gh-api-hooks` registration model is replaced by App-connect; no doc still asserts "user registers the webhook" for the App path → `test_docs_no_manual_gh_hook_for_app` (grep-based) — ⏭️ **DEFERRED → M102_005 §4** (owner docs updated early — commits `2bd6bc2f`/`b8a641e9`; the echo sweep + grep tests carry over)
- **Dimension 8.2** — the "platform never holds the user's PAT" claim is reworded to distinguish the platform App key → `test_docs_app_key_vs_user_pat` (grep-based) — ⏭️ **DEFERRED → M102_005 §4**

---

## Interfaces

```
CredentialBroker (daemon-side):
  mint(workspace_id, integration, scope?) -> ok{ token, expires_at }
       | reconnect_required | unknown_integration | mint_failed   (tagged union)
  integration registry:  id -> Spec{ mintFn(ctx) }   # github | static | (oauth_refresh: future)

Runner mint route (existing agt_r plane):
  POST /v1/runners/me/credentials/mint  { lease_id, integration, scope? } -> { token, expires_at } | { error }
  # workspace derived from lease_id server-side; a caller-supplied workspace is ignored

GitHub App connect + App-level webhook:
  Connect -> App install -> callback { installation_id }
          -> vault fleet:github = { "integration":"github", "installation_id":"…" }   # NO token; map installation_id <-> workspace
  App single webhook URL -> POST /v1/ingress/{provider}  (provider="github"; one platform App secret; payload.installation.id)
          -> verifier/router registry (one descriptor per provider) -> verify sig -> routing_key -> workspace -> matching fleet(s) -> XADD fleet:{id}:events

Tool placeholder (UNCHANGED for SKILL authors):
  ${secrets.github.token}  -> mintable integration -> broker mint ; static integration -> stored value
```

Mint result is a tagged union; `${secrets.…}` shape + integration ids are named constants shared verbatim with tests (RULE UFS). No existing endpoint or `ExecutionPolicy` field is repurposed; `secrets_map` gains mintable-handle entries beside static ones. The per-fleet `/v1/webhooks/{fleet_id}/{source}` route remains for non-App custom webhooks.

---

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Unknown integration | mint for an integration never connected | typed `unknown_integration`; no token; CLI structured error |
| Installation revoked | App uninstalled between connect and use | typed `reconnect_required` (`UZ-GH-*`); UI "Reconnect", CLI error — never a silent 401 |
| Mint upstream failure | GitHub 5xx / network on the token exchange | typed `mint_failed` (retryable, ECL); tool call fails loudly, surfaced in Events |
| Forged workspace in mint request | a prompt-injected child supplies another workspace id | ignored; broker binds to the lease's workspace |
| App key exfil attempt | injected fleet reads env / secrets_map for the key | structurally absent from the child; nothing to read |
| Grant absent/revoked | fleet without an approved `github` grant requests a mint | mint refused; no token; lease never attaches the mintable handle |
| Webhook installation unmapped / bad sig | `installation_id` maps to no workspace, or unsigned/tampered payload | reject (`UZ-WH-*`/`UZ-WH-010`); no `XADD`; logged, no fleet woken |
| Stale cached token | token expired mid-run | broker re-mints transparently on next fetch |
| Partial placeholder substitution | a `${secrets.` survives substitution | dispatch aborted (existing leak guard) |
| token leak to logs | logging a mint result or handle | never logged or returned (VLT); only non-secret status/host appears |

---

## Invariants

1. **The App private key never enters the sandbox child** — enforced by the env deny-prefix (`AGENTSFLEET_*`), the broker living daemon-side, and `test_github_key_never_leaves_broker` asserting policy/`secrets_map`/frames carry no key or handle.
2. **A mint is scoped to the lease's workspace** — broker derives workspace from `lease_id`; a caller-supplied workspace is ignored (`test_mint_scoped_to_lease_workspace`).
3. **A mint requires an approved integration grant** — checked at lease-issue + mint; no grant ⇒ no mintable handle, no token (`test_mint_requires_approved_grant`).
4. **Adding a connector adds no branch to the mint hot path** — the registry is data; `test_integration_registry_is_data_driven`.
5. **Minted tokens are short-lived** — every integration returns `expires_at`; the broker never hands out an expired token (`test_broker_caches_until_expiry`).
6. **Mint result is a tagged union** — no fatal-silent path; compile-checked exhaustiveness.
7. **No new trust plane** — the mint route rides the existing `agt_r` plane; the App webhook rides the existing verifier; no new network surface from the sandbox (`test_child_requests_token_via_runner`).
8. **Secrets never logged or returned** — VLT; only host/status/expiry-bool appear in any frame or log.

---

## Metrics & Observability

> Added Jul 05, 2026 at close (template gained this required section after this spec's Jun 26 authoring; filled honestly at CHORE(close) per RULE NLR).

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| mint outcome log line | ops | each `broker.mint` resolves (ok / reconnect_required / mint_failed / unknown_integration) | integration id, workspace id, outcome, latency | no token/handle/App-key bytes (VLT) | `test_github_key_never_leaves_broker` |
| connect callback log line | ops | a GitHub App connect callback stores a handle | workspace id, installation-present bool | no token, no installation_id in product analytics | `test_github_connect_stores_handle` (via M102_005 §1 for ingress receipt) |

Operator/security-plane observability only — no product analytics or funnel change. Metrics review: no analytics/funnel playbook update required (credential-plane work; the mint/connect signals are operator logs, not product events).

---

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts |
|-----------|------|------|---------|
| 1.1 | unit | `test_broker_dispatches_by_id` | `mint(ws,"github",…)` routes to github; returns `{token,expires_at}` |
| 1.2 | unit | `test_integration_registry_is_data_driven` | a fake id becomes mintable with no edit to the dispatch fn |
| 1.3 | unit | `test_broker_caches_until_expiry` | within validity → same token; past threshold → re-mint |
| 1.4 | unit | `test_broker_unknown_integration_errors` | unconfigured → `unknown_integration`; no upstream call |
| 2.1 | integration | `test_github_mints_installation_token` | valid handle → token + future `expires_at` (fake GitHub) |
| 2.2 | unit | `test_github_revoked_reconnect` | 404/installation-gone → `reconnect_required` |
| 2.3 | unit | `test_github_key_never_leaves_broker` | grep produced policy/secrets_map/frames → zero key/handle bytes |
| 3.1 | integration | `test_child_requests_token_via_runner` | child → runner forward → short-lived token |
| 3.2 | integration | `test_mint_scoped_to_lease_workspace` | foreign workspace id → resolved to the lease's workspace only |
| 3.3 | integration | `test_on_demand_mint_no_trigger` | idle session, no event → tool call mints fresh |
| 4.1 | unit | `test_bridge_mints_on_placeholder` | mintable placeholder → broker fetch; value only at dispatch |
| 4.2 | unit | `assertNoLeftover catches partial substitution` + `execute` guard | residual `${secrets.` → call aborted (`S_SUBSTITUTION_LEFT_PLACEHOLDER`) |
| 4.3 | unit | `test_bridge_static_unchanged` | static integration credential → resolved with no mint call |
| 4.4 | unit | `test_policy_mintable_roundtrip` + `test_runner_facing_classify` | mintable rides typed `ExecutionPolicy.mintable`; `secrets_map` static-only; classifier emits id-only, no handle bytes |
| 4.5 | unit | `test_mint_resolver_dedups_per_call` | repeated mintable placeholders in one tool call mint once |
| 5.1 | integration | `test_github_connect_stores_handle` | callback → `fleet:github` has handle, no token; ws map written |
| 5.2 | integration | `test_ingress_routes_by_installation_id` | App webhook → verified → XADD to the matching fleet only |
| 5.3 | unit | `test_github_states_and_planned` | connected/reconnect render; slack/zoho "Planned" + hint |
| 5.4 | integration | `test_ingress_registry_is_data_driven` | a fake provider descriptor verifies + routes with no `/v1/ingress` handler edit |
| 6.1 | integration | `test_mint_requires_approved_grant` | no approved grant → mint refused, no token |
| 6.2 | integration | `test_lease_gates_mintable_on_grant` | approved → mintable handle attached; revoked → static-only |
| 7.1 | e2e (cli) | `test_cli_connector_status` | `connector status --json` reflects live state |
| 7.2 | integration | `test_cli_connector_disconnected_error` | disconnected → structured-JSON error + non-zero exit |
| 8.1 | unit (doc grep) | `test_docs_no_manual_gh_hook_for_app` | no arch doc asserts user-`gh api …/hooks` for the App path |
| 8.2 | unit (doc grep) | `test_docs_app_key_vs_user_pat` | the "never holds the user's PAT" claim distinguishes the App key |

**Regression:** static custom-secret resolution, model routing, the inbound webhook HMAC path, and the sandbox env allowlist unchanged except assertions tracking intentional additions. **Idempotency/replay:** re-minting yields a fresh valid token; a cached token is reused until near expiry; repeated connect is idempotent on `installation_id`; the App ingress dedupes on the delivery id. **Integration coverage:** broker→github→fake-GitHub mint, child→runner→broker round-trip, the revoked-installation reconnect path, and ingress installation_id routing are injected deterministically.

---

## Acceptance Criteria

- [ ] Broker mints + caches + dispatches by data-driven integration; github mints installation tokens — verify: `make test && make test-integration`
- [ ] App key never in the child; mint scoped to lease workspace; mint requires approved grant — verify: `make test` + `make memleak`
- [ ] On-demand mint covers idle/no-trigger; tool boundary resolves mintable vs static — verify: `make test-integration`
- [ ] GitHub App Connect/reconnect UI + ingress installation_id routing — verify: `make test-unit-app && make acceptance-e2e`
- [ ] CLI connector status/errors — verify: `make test-unit-cli && make cli-acceptance`
- [ ] Docs sweep: no arch doc asserts the manual `gh api …/hooks` model for the App path — verify: the §8 doc-grep tests
- [ ] Cross-compile clean — verify: `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux`
- [ ] `make lint` + `make lint-app` clean · `gitleaks detect` clean · no non-md file over 350 lines added

---

## Eval Commands (post-implementation)

```bash
# E1: backend unit + integration + memleak
make test && make test-integration && make memleak 2>&1 | tail -5
# E2: cross-compile both targets
zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux && echo "XC PASS"
# E3: UI + CLI
make test-unit-app && make test-unit-cli && make acceptance-e2e && make cli-acceptance
# E4: lint + gitleaks + App-key-never-in-child sweep (empty = pass)
make lint 2>&1 | grep -E "✓|FAIL"; gitleaks detect 2>&1 | tail -3
grep -rn "private_key\|app_private" src/agentsfleetd/runner src/runner --include='*.zig' | grep -iv "test\|//" | head
```

---

## Dead Code Sweep

**1. Orphaned files** — none deleted; this is additive. New files per Files Changed.

**2. Orphaned references** — grep after the changes; non-zero = stale.

| Removed/renamed symbol | Grep | Expected |
|------------------------|------|----------|
| ad-hoc per-call vault reads bypassing the broker | `grep -rn "vault.loadJson" src/agentsfleetd/runner --include='*.zig' \| head` | 0 (mintable creds route through the broker) |
| `"github"` literal | `grep -rn '"github"' src/ cli/ ui/ \| grep -v const \| head` | only the named-constant defs + imports (RULE UFS) |

---

## Discovery (consult log)

> **Empty at creation.** Populate as work surfaces consults, skill outcomes, and Indy-acked deferrals.

- **Origin (Indy + Orly/CTO, Jun 25–26 2026):** reborn from DEFERRED M99_001 per its "re-spec under the agent-identity-proxy framing" note. Indy's caveat is the reason to exist — *"the agentsfleetd-credentials-proxy: this will become too static that expanding and supporting more connectors will be a pain."* Resolution: config-driven integration registry (RULE CFG), connector = descriptor.
- **Refinements this session:** (1) App-level webhook ingress routed by `installation_id` — one App webhook URL set once by the platform, no per-repo registration (M99 §5 under-specified). (2) grant/approval placement made explicit — grant at lease+mint, approval gate per-write unchanged. (3) Surface order: Indy chose **UI-first**.
- **Provider generalization (Indy, Jun 26):** both planes are data-driven. Inbound = `/v1/ingress/{provider}` + verifier/router registry; per-provider variation is a descriptor (sig scheme, header, routing-key path, optional lifecycle handshake). Outbound = ~3 integration mechanisms (`github` JWT→token; `oauth_refresh` for Slack/Linear/Jira-3LO/Zoho; `static`). Linear/Jira/Zoho/Slack all "follow suit" as descriptors; new code only for a genuinely new signature family (Jira `atlassian_jwt`) or lifecycle handshake.
- **§1 landed (Jun 26):** `credentials/integration.zig` + `credentials/broker.zig` — broker 8/8 + integration 4/4 unit tests pass (leak-checked, `std.testing.allocator`); ZLint 0 errors (pub surface clean); x86_64-linux + aarch64-linux cross-compile clean; test-depth +8 (2145→2153). Registry + cache + `static` integration + DI for tests; the real `github` integration lands in §2. Pre-existing `zig fmt` drift in `state/model_caps_store.zig` + `runner/engine/stream_redactor.zig` flagged — NOT M102 scope, left untouched.
- **Vocabulary lock (Indy, Jun 26):** one term end to end — **`integration`**, never "driver"/"kind". §1's shipped `Driver`/`Kind`/`DRIVER_REGISTRY`/`"kind"` renamed to `Spec`/`Id`/`REGISTRY`/`"integration"` (commit `b8a641e9`, pure rename, tests green); the integration id `github_app` → `github` (the "App" is *how* github mints, not a second id). Spec + arch-owner docs aligned the same session. Slack/Linear/Jira/Zoho become integration entries under one noun.
- **RS256 signer (Indy, Jun 26):** no RS256 *signer* exists in-repo or in Zig std (verify-only — `jwks_crypto.verifyRs256`) and no vendored JWT/RSA dep. GitHub App auth mandates RS256, so §2 **adds** a signer. Indy's steer: it is **not** github-specific — it is a reusable primitive (Google/Snowflake/Salesforce/DocuSign JWT-bearer all sign RS256 the same way), so it lands at `auth/crypto/rs256_sign.zig` beside its mirror `verifyRs256`, built on `std.crypto` (no third-party dep, house style); only github's claims+exchange live in `integration_github.zig`. Files-Changed amended to add the signer file.
- **Platform-key storage = vault, not env (Indy, Jun 26):** the App private key resolves from the **`agentsfleet-admin` workspace vault** (`crypto_store.load`), mirroring `platform_llm_keys` (real bytes in `vault.secrets` under the admin workspace, registered via an admin path; resolver follows `source_workspace_id`). **Reverses** the earlier read that the shipped `03_priming_infra` playbook's `fly secrets set GITHUB_APP_ID/GITHUB_APP_PRIVATE_KEY` (boot-env, like the peppers) was the model — Indy: *"env doesn't scale across a gazillion integration keys; vault is the scalable model."* Only the `ENCRYPTION_MASTER_KEY` KEK stays in env (it decrypts the vault). Sequencing: build the github platform-key store + `crypto_store.load` wiring first (mirror `platform_keys.zig`), THEN rewrite the playbook to store-into-vault and drop the two env lines — the playbook never points at a command that doesn't exist. Gaps this surfaced (now in Files-Changed): the App **webhook-secret** op-ref (preflight gates only app-id + private-key; §5 ingress needs it) and an operator **App-registration** runbook.
- **Doc-shape review:** `docs/v2/reviews/m102-doc-shape-review.md` (C1–C9 + invariants-that-hold); §8 absorbs it. The credential-model owners (`capabilities.md` §2–3, `data_flow.md` example) were updated early (commit `2bd6bc2f`) to record the broker + RS256 model in the `integration` vocabulary; the echoes wait for §8.
- **App-key storage RESOLVED (Indy + Orly, Jun 26):** **option 1 — no new schema.** One **non-secret** config pointer `PLATFORM_ADMIN_WORKSPACE_ID` (a workspace UUID, set once at priming) + per-integration **vault key_names** under that workspace: `(admin_ws, "github-app") → {app_id, private_key_pem}` (AES-GCM in `vault.secrets`). Adding an integration = one new vault key, **zero** env/schema growth — honours "env holds only the KEK + non-secret pointers, never per-integration keys." Load path `vault.loadJson(admin_ws, "github-app") → PlatformSecrets.github`; degrades gracefully (pointer unset / vault miss ⇒ `github=null`, broker still serves `static`, github mints return `reconnect_required`). Rejected: a new `core.platform_integration_refs` table (schema the spec ruled out + an admin route, overkill for one App) and reusing the active `platform_llm_keys` row (couples the App-key home to the LLM default — brittle). **Naming aside (Indy):** the existing `core.platform_llm_keys` is a *misnomer* — it stores a `source_workspace_id` pointer + the active-default config, **no key bytes**; a faithful rename is `core.platform_provider_defaults`, but that is a multi-file migration **outside M102's Files-Changed scope** (left untouched; a follow-up rename can clean it).
- **§3 daemon endpoint landed (Jun 26, commit `0600b831`):** `POST /v1/runners/me/credentials/mint` — `handlers/runner/credentials_mint.zig` resolves the lease's workspace scoped to the presenting runner (Invariant 2; mirrors `service_activity.zig`'s lease-ownership query), loads the `fleet:<integration>` vault handle (`credential_key.allocKeyName`), dispatches to `ctx.broker.mint`, and maps the tagged outcome to the wire. Wire types in a new `protocol_credentials.zig` (FLL split, mirrors `protocol_memory.zig`); error codes `UZ-CRED-001` / `UZ-GH-001` / `UZ-GH-002` registered (first caller — NDC). Broker threaded onto the handler `Context` (optional + defaulted). Pure unit coverage of the outcome→wire mapping + route match; the 3.1–3.3 integration tier (live lease + round-trip) is DB-gated (local Postgres absent).
- **Runner-side mint channel topology RESOLVED (Orly, Jun 26 — codebase-derived, no new sandbox hole):** the child↔parent fd topology (`child_process.forkExec`) is `stdin = .pipe` (parent→child) + `stdout = .pipe` (child→parent), both open for the lease lifetime; `secret_substitution.substitute` runs **in-child, post-bwrap/Landlock**, against the inline `secrets_map`. So the synchronous mint round-trip rides the **existing pipes** — no extra fd: a `credential_request` frame child→parent on **stdout** (new `pipe_proto.FrameType`), the parent's supervisor read loop handles it **inline** (forwards to the daemon mint route via `daemon/control_plane_client.zig` over `agt_r`, the parent holds the token), and writes a `credential_response` frame back parent→child on **stdin**; the child blocks reading it after emitting the request. §4 then threads this channel into `substitute()` so a *mintable* placeholder fetches on demand instead of reading a static value (the partial-substitution leak guard `assertNoLeftover` is preserved). This is the next build step (§3 remainder + §4).
- **§3 runner-side channel landed (Jun 26, Orly):** the child→runner→daemon mint channel rides the EXISTING stdin/stdout pipes — no new fd, no new sandbox hole (the memory channel's pattern, `runner_fleet.md`:300). **Correction to the handoff's premise:** stdin was *closed* right after the lease feed (`child_supervisor.zig`), so "stdin stays open" was the unbuilt feature, not a contradiction. Built: (1) three `pipe_proto.FrameType`s — `lease='L'` (parent→child, the framed lease replacing write-then-EOF so stdin can stay open), `credential_request='C'` (child→parent on stdout), `credential_response='T'` (parent→child on stdin); (2) `engine/credential_request.zig` — the child half (`PipeRequest`/`PipeResponse` shared wire shapes + synchronous `mint` round-trip, deadline-bounded, fail-closed); (3) `control_plane_client_mint.zig` — the `cp.mint` verb (split out of `control_plane_client.zig` for FLL; re-exported), forwards to `POST /v1/runners/me/credentials/mint` with only `lease_id` (no workspace — Invariant 2 server-derived); (4) the parent read loop (`child_supervisor_read.zig`) services a `credential_request` inline via an injected `MintHook` and frames the reply back on the child's stdin — single-threaded child ⇒ no stdout-frame interleave; (5) `child_exec.zig` reads exactly one `lease` frame (no longer read-to-EOF), stdin stays open; (6) `lease_run.zig` wires `MintForwarder` (holds `cp`+`runner_token`+`lease_id`) into `child_supervisor.run`. Unit coverage: frame round-trips, the child `mint` round-trip (ok / rejection / closed-channel / protocol-skew), the parent read-loop service path (mint-hook → framed token; null-hook → ok=false). All runner unit tests green (346 pass / 7 Linux-skip), cross-compile both linux targets clean, `make lint-zig` green (ZLint 0/0, FLL ≤350 after the split, runner-isolation gate: the channel pulls in no pg/httpz). **Dimensions 3.1–3.3 stay open** until §4 threads the channel into `substitute()` so the child actually triggers a request — they are end-to-end integration scenarios (DB-gated locally) that light up with the §4 trigger; the runner-side machinery they rest on is in place + unit-proven.
- **§3 serve wiring landed (Jun 27, Orly):** the broker is now a live daemon singleton. `credentials/serve_broker.zig` builds the production effects the std-only broker takes by injection — `HttpClientExchange` (the `std.http.Client` boundary for the GitHub token exchange, mirrors `github_net.zig`), `buildDeps`/`loadGithubApp` (option-1 platform-key load: `vault.loadJson(admin_ws, "github-app") → {app_id, private_key_pem}`, duped to process lifetime, degrades to null on any miss), and `metricsSink` (logs each mint's outcome/latency, never the token — VLT). The boot wiring lives in `preflight.installCredentialBroker` (heap-owns the broker + its HTTP boundary so `ctx.broker` is stable; degrades closed — init failure ⇒ `ctx.broker` null ⇒ the mint endpoint 503s). `ServeConfig` gains the non-secret pointer `PLATFORM_ADMIN_WORKSPACE_ID` (empty ⇒ broker serves `static` only). **CFG-altitude fix (Indy's catch, Jun 27):** the first cut put a github-specific load *branch in `serve.zig`* — exactly the per-connector junk drawer the broker exists to kill (RULE CFG / Invariant 4). Refactored so `serve.zig` is integration-agnostic (one `preflight.installCredentialBroker(...)` call, zero integration names); WHICH integrations carry a platform key + how to load them lives solely in `serve_broker.buildDeps`. The next platform-keyed integration is added there (ideally registry-driven once `PlatformSecrets` generalizes past the single locked `{github}` field), never in the boot path. `serve.zig` held at exactly the 350-line FLL cap by hoisting all wiring to `preflight`. Build + cross-compile both linux clean; `make lint-zig` green; agentsfleetd unit suite 1377 pass (the 2 known DB-gated failures — webhook UZ-WH-010 + worker_started — untouched); `serve_broker` unit tests green.
- **§3 integration tests landed (Jun 27, Orly):** Dimensions 3.1–3.3 closed. The runner-isolation gate (`build_runner.zig` depends only on nullclaw — zero pg/httpz) makes a single in-process child→runner→daemon→DB test impossible, so the on-demand path is proven by two legs meeting at the mint wire. **(1) Daemon leg** — `http/handlers/runner/credentials_mint_integration_test.zig`: the real `POST /v1/runners/me/credentials/mint` over the live test DB + real runner-bearer + a real `CredentialBroker` over the `static` integration (deterministic — no network / App key / RS256 signer, `nullDeps`). ONE harness/seed spin folds three assertions: connected-but-no-handle → typed `UZ-CRED-001`; the owner mints from ITS OWN workspace handle (the token VALUE, not mere presence, distinguishes it from a sibling — Invariant 2 positive); the owner runner cannot mint on another runner's lease → `UZ-RUN-006` (IDOR negative). DB-gated (`SkipZigTest` without `TEST_DATABASE_URL`); compiles + skips clean locally, runs in CI. **(2) Runner leg** — `src/runner/credential_mint_e2e_test.zig`: real child pipe frames → real parent read loop (`child_supervisor.readResult`) → real control-plane HTTP client (`cp.mint`) → a loopback TCP mint stub (mirrors `control_plane_client_test`'s `RenewBodyStub`). 3.1 drives `credential_request.mint`; 3.3 drives the tool-boundary `${secrets.github.token}` placeholder with **no event** (the on-demand-at-tool-call proof); plus the `cp.mint` verb's fail-closed contract (non-2xx → `rejected`) which had no direct test. Cross-platform (in-process pipes + loopback stub, no fork — fork mechanics stay in `sandbox_integration_test`); each asserts the wire carries lease_id + integration but NEVER a workspace. Runner unit suite **354→357 (+3)** pass / 7 skip; `make lint-zig` green (FLL/ZLint/runner-isolation/pg-drain); both linux cross-compiles + `gitleaks` clean.
- **§3 hardening — OOM proofs + broker bench (Jun 27, Orly; Indy-directed "test and figure out the issues"):** Added failing-allocator (`std.testing.FailingAllocator`) injection on both mint allocation paths — `broker.mint` (cold + warm caller-dup OOM → `mint_failed{transient}`, no leak; `broker_test.zig`) and `credential_request.mint` (request-frame stringify OOM → `OutOfMemory`, no byte reaches the pipe, fail-closed; inline). The error paths are now PROVEN leak-safe (the testing-allocator gate), not assumed. Added two broker micro-benches to `make bench` Tier-1 (`bench_exports.zig` re-exports + `tests/bench/micro.zig`; `cache` dep wired into `bench_app` in `build.zig`): `broker_cache_hit` — the production hot path (a token is minted once then served from cache on every tool call for the lease's life; ~17µs min / 24µs p75, healthy, and a regression guard for the lock-free SHARED-RwLock read) — and `static_mint_dispatch` (the tagged-union dispatch + token-dup floor, ~6–8µs). **Two adversarial-CTO "improvements" were RETRACTED under Indy's scrutiny — do NOT re-attempt:** (1) a `std.time.Timer` to fill the always-0 `latency_ms` metric — violates the broker's injected-clock determinism (latency belongs at the metrics boundary if anywhere, not the core); (2) a sharded-mutex single-flight on cold-miss — re-introduces a lock onto the deliberately lock-free read path (the Bun idiom Indy removed "with great difficulty") AND holds it across the GitHub network call, a worse failure mode than the rare, bounded cold-cache thundering herd it would fix. `broker.zig` is UNCHANGED — the §1–§4 design held. **Pre-existing breakage fixed (Indy-approved):** `make bench` Tier-1 had been unbuildable since M92 (`9a06c0be`) — `benchWebhookSignatureVerify` called a `webhook_verify.verifySignature` the brand cutover deleted (verification was decomposed into `hmac_sig` primitives); the stale bench + its orphaned fixtures were removed (RULE NDC/ORP). Gates: runner 358 pass / 7 skip; daemon broker green; `make lint-zig`, both linux cross-compiles, `gitleaks` clean.
- **§5 connect surface landed (Jun 27, Orly) — UI-first, no token paste; ingress deferred.** The "connect once" loop is built end to end and compiles green; the App-webhook **ingress** (Dims 5.2/5.4, `/v1/ingress/{provider}`) is NOT in this slice — it is event *receipt*, separable from connect, and rides a follow-up. **Decisions (Indy, Jun 27):** connect is **installation-only** (the App install IS the auth; no user-OAuth identify — only `installation_id` is needed); the callback is **Bearer-less** (a github.com redirect), so a **signed `state`** is the sole trust anchor — HMAC-SHA256 over `workspace_id|nonce|exp` with the existing `approval_signing_secret` (domain-separated `ghconnect:v1:`, no new secret) + a **Redis single-use nonce** (`DEL` integer reply) + expiry; the callback writes the EXACT `fleet:github = {integration,installation_id}` handle the broker already reads (`credential_key.allocKeyName`), so the live broker mints from it with zero new mint code. **No token-paste:** the UI catalog's GitHub `requiredSecret: GITHUB_TOKEN` model is removed — GitHub is now `auth: app_connect`. **Files** — UI: `lib/integrations/catalog.ts`, `credentials/components/IntegrationsConnectors.tsx` (replaces the deleted token-paste `IntegrationsComingSoon.tsx`), `lib/api/connectors.ts`, `credentials/connector-actions.ts`, `credentials/page.tsx`; backend: `http/handlers/connectors/github/{connect,callback,status,state}.zig`, `route_table_invoke_connectors.zig` + matchers/routes/dispatch (bearer for connect+status, **none** for callback), `Context.github_app_slug` resolved at boot from the admin `github-app` vault entry's **`app_slug`** field (new field — `serve_broker.loadGithubAppSlug`), error codes `UZ-CONN-001/002`. **Gates:** `zig build` + both linux cross-compiles green; daemon unit suite green (state crypto: 5 tests — round-trip, tampered-mac, foreign-secret, expiry, malformed); UI `tsc` + 6/6 vitest. **F1 (CLI signpost):** not in this slice — connect is UI-only by Indy's call; the CLI `connector status`/signpost is a separate fast-follow. **Operational:** the admin `github-app` vault entry must carry `app_slug` alongside `app_id`/`private_key_pem`, else connect degrades closed (`UZ-CONN-001`).
- **Open to confirm at PLAN:** App registration ownership + admin-vault key storage; whether the App webhook reuses `/v1/webhooks/{fleet_id}` internally or a new `/v1/ingress/github` path.
- **Deferrals:** `oauth_refresh` integration, per-fleet cryptographic identity / Agent Auth wire format, Stripe Agentic Commerce Protocol, exact-action approval-hash binding — Out of Scope, not dropped Dimensions; custom secrets bridge the non-GitHub connectors until then.
- **Tail split → M102_005 (Jul 05, 2026, Orly + Indy):** during post-merge cleanup a live main-tree audit found four dimensions unshipped: §5.2/§5.4 (the `/v1/ingress/{provider}` App-webhook receiver was never built — connect landed, event receipt did not), §6.1/§6.2 (the grant **lifecycle** shipped but the mint/lease **enforcement** did not — `credentials_mint.zig` mints with no grant read; **Invariant 3 is not upheld on main**), §7 (CLI `connector` — connect was UI-only by design), §8 (the C1–C9 docs echo sweep). Indy chose to close M102_001 for the shipped core and carve the tail into a fast-follow. §6 enforcement was NOT in Indy's initial carve framing — it surfaced during the audit as a security gap and was folded into M102_005 §2 (P0). Reactivation: none — M102_005 (`docs/v2/pending/`) owns the tail.
  > Indy (2026-07-05 22:43): "Split + defer the tail — Mark §1–§6 DONE, carve §5-ingress/§7-CLI/§8-docs into a new fast-follow spec (M102_005)" — context: M102_001 tail after PR #458 merge; §6 grant enforcement additionally deferred to M102_005 §2 (found un-shipped during the same audit, Invariant 3 gap).

---

## Skill-Driven Review Chain (mandatory)

| When | Skill | What it does | Required output |
|------|-------|--------------|-----------------|
| After implementation, before CHORE(close) | `/write-unit-test` | Audits diff coverage vs the Test Specification (≥50% negative; every Failure Mode covered) | Clean; iteration count in Discovery |
| After tests pass, before CHORE(close) | `/review` | Adversarial diff review vs this spec, `docs/AUTH.md`, `dispatch/write_zig.md`, `dispatch/write_ts_adhere_bun.md`, Failure Modes, Invariants (esp. key-never-in-child + workspace-scope + grant-gate) | Clean or every finding dispositioned |
| After `gh pr create` | `/review-pr` | Review-comments the open PR | Comments addressed before human review |

---

## Verification Evidence

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Backend unit | `make test` | {paste snippet} | |
| Integration | `make test-integration` | {paste snippet} | |
| Memleak | `make memleak` | {paste snippet} | |
| Cross-compile (Zig) | `zig build -Dtarget=x86_64-linux && -Dtarget=aarch64-linux` | {paste snippet} | |
| UI unit | `make test-unit-app` | {paste snippet} | |
| CLI acceptance | `make cli-acceptance` | {paste snippet} | |
| Lint | `make lint && make lint-app` | {paste snippet} | |
| Gitleaks | `gitleaks detect` | {paste snippet} | |

---

## Out of Scope

- The `oauth_refresh` integration + Slack/Zoho/Jira/Linear connectors (outbound) AND their inbound ingress descriptors / new scheme impls / lifecycle handshakes (Slack `url_verification`, Atlassian install lifecycle) — follow-up. The generic `/v1/ingress/{provider}` + verifier registry ship here with the `github` entry only; each new provider is a descriptor (+ a scheme impl only for a new signature family — Jira's `atlassian_jwt` is the likely first). Custom secrets bridge outbound until an integration lands.
- Per-fleet cryptographic identity / Agent Auth Protocol wire-format alignment — the v3 capability-token layer; the App installation is the identity for now.
- Stripe Agentic Commerce Protocol — a future financial-mutation capability, gated by the same approval machinery.
- Exact-action approval-hash binding ("approve this comment on this line") — reuse the coarse integration grant now; the named Approach-B follow-up.
- A standalone credentials microservice; per-credential fleet-usage analytics; rotation automation beyond mint-on-demand.
- Replacing static custom secrets — they remain a first-class `static` integration.
