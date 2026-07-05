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

# M102_005: Agent-identity-proxy tail — the App-webhook ingress, the grant-gate that was never wired, the connector CLI, and the docs sweep M102_001 shipped without

**Prototype:** v2.0.0
**Milestone:** M102
**Workstream:** 005
**Date:** Jul 05, 2026
**Status:** PENDING
**Priority:** P1 — one dimension is a **live security gap** (a mint currently needs no approved grant — M102_001 Invariant 3 is not enforced on main); the rest is operator-facing (App-webhook receipt, `agentsfleet connector` status) + doc-truth.
**Categories:** API, CLI, DOCS
**Batch:** B1 — §2 (grant-gate) is the priority and independent; §1 (ingress), §3 (CLI), §4 (docs) each touch disjoint surfaces and may land in any order after §2.
**Depends on:** M102_001 (MERGED, PR #458 — shipped §1–§4 mint broker + §5-connect; this workstream carries its explicitly-deferred tail: §5.2/§5.4 ingress, §6 grant enforcement, §7 CLI, §8 docs). M102_004 was attempted and abandoned (PRs #462/#464 closed, no file landed) — 005 avoids reusing a burned number.
**Provenance:** LLM-drafted (Claude, Jul 05, 2026) — carved from M102_001's Discovery log + a live main-tree audit this session that found §6 grant enforcement absent (mint calls `broker.mint` with no grant check; `secrets_resolve.mintableId` classifies without a grant read).
**Canonical architecture:** `docs/AUTH.md` (credential boundary + the `agt_r` plane + webhook verifier) + `docs/architecture/data_flow.md` §B/§C + `docs/architecture/runner_fleet.md` (the `agt_r` plane). Introduces no new trust plane — the ingress rides the existing verifier, the grant-gate rides the existing mint route.

---

## Overview

**Goal (testable):** `POST /v1/ingress/{provider}` verifies the provider signature and routes a GitHub App webhook to the matching fleet by `installation_id` through a data-driven registry (a fake provider descriptor routes with zero handler edits); `CredentialBroker.mint` and the lease path both **refuse to hand out a token for a fleet with no approved `github` integration grant** (restoring M102_001 Invariant 3); `agentsfleet connector` reports connected/needs-reconnect state as human + stable `--json` and returns a structured-JSON error with a non-zero exit when acting through a disconnected connector; and no architecture doc still asserts the manual `gh api …/hooks` webhook-registration model for the App path.

**Problem:** M102_001 merged its backend mint wire (§1–§4) and the UI-first connect surface (§5-connect) but explicitly deferred four dimensions — and one of them is load-bearing for security. On main today: (a) the App's single webhook has nowhere to land (`/v1/ingress/{provider}` was never built — the App can be installed but its events are dropped); (b) **any connected fleet with a valid lease mints a GitHub token without an approved grant** — the grant lifecycle (request/approve/revoke) exists and is operable, but nothing reads it at mint or lease, so the designed defense-in-depth is inert; (c) there is no `agentsfleet connector` command, so a disconnected connector surfaces only as a silent tool failure with no CLI signpost; (d) architecture docs still describe the pre-App manual-webhook model, contradicting shipped reality.

**Solution summary:** build the generic `/v1/ingress/{provider}` receiver on top of the existing `webhook_verify.PROVIDER_REGISTRY` (which already carries the `github` verify descriptor), adding a `routing_key_path` so a verified payload routes by `installation_id → workspace → matching fleet(s)`; wire an approved-grant read into both the mint handler and the lease classifier so a missing/revoked grant refuses the mint with a typed outcome; add the `agentsfleet connector` command mirroring the existing `grant` CLI and the 7-Pillars structure; and run the §8 docs sweep absorbing the C1–C9 contradiction map already written in `docs/v2/reviews/m102-doc-shape-review.md`.

## PR Intent & comprehension handshake

- **PR title (eventual):** `feat(m102): agent-identity-proxy tail — App-webhook ingress, grant-gated mint, connector CLI, docs sweep`
- **Intent (one sentence):** finish the agent-identity proxy — an installed GitHub App's events actually reach the right fleet, a mint is refused without an approved grant, the operator can see connector state from the CLI, and the docs stop lying about the webhook model.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `docs/v2/done/M102_001_P1_API_CLI_UI_AGENT_IDENTITY_PROXY_GITHUB_APP.md` — the parent; its §5/§6/§7/§8 dimension text, Interfaces, Failure Modes, and **Invariant 3** are the blueprint. This workstream implements exactly its deferred tail. **Do not edit it.**
2. `docs/AUTH.md` §"credential boundary", §"Runner token", §"Webhook auth" — the boundary + the `agt_r` plane + the existing verifier. **Auth-flow file — read before any ingress/mint/grant code.**
3. `src/agentsfleetd/fleet_runtime/webhook_verify.zig` — `PROVIDER_REGISTRY` (SLACK/GITHUB/LINEAR verify descriptors + the compile-time dup guard); the ingress extends the descriptor with `routing_key_path` and seeds github's `installation.id`.
4. `src/agentsfleetd/http/handlers/connectors/github/{connect,callback,status}.zig` — the connect side already shipped; the callback writes the `installation_id ↔ workspace` map the ingress reads. Mirror their handler shape + `route_table_invoke_connectors.zig` registration for the bearer-less ingress route.
5. `src/agentsfleetd/http/handlers/runner/credentials_mint.zig` + `src/agentsfleetd/fleet/secrets_resolve.zig` (`mintableId`) — the two sites that must gain the approved-grant read; `src/agentsfleetd/http/handlers/integration_grants/handler.zig` + `webhooks/grant_approval.zig` (`GrantStatus`) is the grant row + status source of truth.
6. `cli/src/commands/grant.ts` — the existing integration-grant CLI; `connector` mirrors its command→handler→errors split, structured-JSON errors, and `--json` rendering (7 Pillars).
7. `docs/v2/reviews/m102-doc-shape-review.md` — the C1–C9 contradiction map + "Docs to update" list the §4 sweep absorbs verbatim.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/agentsfleetd/http/handlers/webhooks/ingress.zig` | CREATE | generic `POST /v1/ingress/{provider}`; verify via the registry, route by the descriptor's routing-key → workspace → fleet(s) → `XADD fleet:{id}:events` |
| `src/agentsfleetd/fleet_runtime/webhook_verify.zig` | EDIT | extend each descriptor with `routing_key_path`; seed github's `installation.id`; keep the compile-time dup guard |
| `src/agentsfleetd/http/routes.zig` · `route_table_invoke*.zig` · matchers | EDIT | register the bearer-less `/v1/ingress/{provider}` route + dispatch (mirror `slack_events` / the connectors table) |
| `src/agentsfleetd/http/handlers/runner/credentials_mint.zig` | EDIT | read the approved-grant state before `broker.mint`; typed refusal on absent/revoked grant, no token |
| `src/agentsfleetd/fleet/secrets_resolve.zig` | EDIT | gate the mintable classification on an approved grant at lease-issue — no grant ⇒ no mintable handle attached |
| `src/agentsfleetd/state/integration_grant_lookup.zig` (or sibling of the grants handler) | CREATE/EDIT | one `isApproved(workspace/fleet, integration)` DB read shared by the mint handler + the lease path |
| _error registry module_ | EDIT | `UZ-GRANT-*` (grant-required refusal) + `UZ-WH-*` (ingress unmapped/bad-sig) + `hint()`; no secret in any frame (VLT) |
| `cli/src/commands/connector.ts` · `cli/src/services/connectors.ts` | CREATE | `agentsfleet connector list`/`status`; structured-JSON error + non-zero exit on disconnected |
| `cli/src/commands/index.ts` (or the command registry) | EDIT | register the `connector` command |
| `docs/architecture/{user_flow,data_flow,capabilities,high_level,README,roadmap,runner_fleet}.md` · `scenarios/gh-pr-reviewer.md` | EDIT | §4 docs sweep — absorb C1–C9; drop the manual-`gh api …/hooks` model for the App path |
| _colocated tests (Zig `test {}` · `*.test.ts` · doc-grep tests)_ | CREATE/EDIT | one test per Dimension below |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — **RULE CFG** (the ingress provider registry is data — a new provider is a descriptor, never a `/v1/ingress` branch — the core of §1), **VLT** (App key / minted token / grant payloads never logged or returned), **PRI/NTP** (treat inbound webhook + CLI inputs as hostile), **ECL** (grant-required / reconnect-required / ingress-rejected are typed classes), **EMS** (standard error structure), **TGU** (tagged-union mint + ingress result), **JCL** (CLI JSON contract — §3), **UFS** (`/v1/ingress` route, provider ids, `installation.id` routing path, grant-status literals → named constants shared verbatim with tests), **NDC/NLR/ORP**.
- **`docs/AUTH.md`** — auth-flow: the ingress rides the existing verifier; the grant-gate is checked at BOTH lease-issue and mint (mirror the boundary, add no new plane).
- **`dispatch/write_zig.md`** — tagged-union results, multi-step `errdefer`, pg-drain (the grant read + ingress routing both query pg), file ≤350 / fn ≤50, cross-compile both linux targets.
- **`dispatch/write_ts_adhere_bun.md`** — §3 CLI: handler purity (no `console.log`/`process.exit` in handlers), output-as-a-service, structured-JSON errors.
- **`docs/REST_API_DESIGN_GUIDELINES.md`** — the `/v1/ingress/{provider}` route lives under `src/agentsfleetd/http/handlers/**`.
- No schema Data Definition Language — `core.integration_grants` already exists (`schema/008_core_integration_grants.sql`); this workstream only READS it. `docs/SCHEMA_CONVENTIONS.md` does not apply.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes — ingress handler, grant-gate read, verifier descriptor | tagged-union results; `errdefer`; `conn.query().drain()`; cross-compile both linux targets |
| PUB / Struct-Shape | yes — ingress result, `isApproved` signature, extended `VerifyConfig` | shape verdict per new/changed pub surface; tagged-union ingress result |
| File & Function Length (≤350/≤50/≤70) | yes — ingress handler, mint handler edit | keep the ingress handler thin (registry does the per-provider work); extract the grant read to its own file |
| UFS (repeated/semantic literals) | yes — route path, provider ids, routing-key path, grant-status literals | named constants in one module per side; tests import them |
| UI Substitution / DESIGN TOKEN | no — CLI + backend + docs only | — |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | ERROR REGISTRY yes (`UZ-GRANT-*` / `UZ-WH-*`); LOGGING yes (VLT on ingress + mint frames); SCHEMA no (grant table exists) | register codes with `hint()`; log only non-secret status/host/installation-presence |

## Prior-Art / Reference Implementations

- **Ingress (§1)** — mirror the existing `slack_events` bearer-less ingress route registration + `webhook_verify.PROVIDER_REGISTRY` (SLACK/GITHUB/LINEAR verify descriptors already present; §1 adds the `routing_key_path` field + the generic route, and proves data-drivenness). The connect-side callback that writes the `installation_id ↔ workspace` map already shipped (`connectors/github/callback.zig`).
- **Grant-gate (§2)** — mirror `service_activity.zig`'s lease-ownership query for the fleet/workspace resolution and `integration_grants/handler.zig` (`GrantStatus.approved`) for the grant-status read; the mint handler already resolves the lease's workspace (Invariant 2) — the grant read slots in beside it.
- **Connector CLI (§3)** — mirror `cli/src/commands/grant.ts` (the existing integration-grant command) + the "7 Pillars" (command→handler→errors split; handler purity; output-as-a-service; structured-JSON errors with a reconnect suggestion; auto-JSON when piped).
- **Docs sweep (§4)** — the review companion `docs/v2/reviews/m102-doc-shape-review.md` is the source of truth for what to change (its "Docs to update" list).

## Sections (implementation slices)

### §1 — Generic App-webhook ingress (`/v1/ingress/{provider}`, data-driven)

The App's single webhook URL posts to `POST /v1/ingress/{provider}` (bearer-less — the provider signature is the trust anchor). The receiver looks the provider up in the verify/router registry, verifies the signature, then routes by the descriptor's `routing_key_path` (`installation.id` for github) → `installation_id ↔ workspace` map → matching fleet(s). **Implementation default:** extend the existing `webhook_verify.PROVIDER_REGISTRY` descriptor rather than a parallel table — one registry, one dup guard — because RULE CFG demands a new provider be a descriptor, not a route branch. The existing provider-specific `slack_events` route is left in place (migrating it onto the generic ingress is Out of Scope — named follow-up); §1 ships the github descriptor and proves a fake descriptor routes with no handler edit.

- **Dimension 1.1** — App webhook at `/v1/ingress/github` → verify App HMAC → route by `installation_id` to the matching fleet's events stream (`XADD`); an unmapped `installation_id` or bad signature rejects with no `XADD` → Test `test_ingress_routes_by_installation_id`
- **Dimension 1.2** — adding a provider is a registry descriptor; the `/v1/ingress/{provider}` route + dispatch carry no per-provider branch (a fake provider descriptor verifies + routes with no handler edit) → Test `test_ingress_registry_is_data_driven`

### §2 — Integration-grant enforcement at mint + lease (restores Invariant 3)

M102_001 designed a grant that gates whether a fleet may use `github` at all — but nothing reads it. §2 wires the approved-grant check into BOTH enforcement points: the lease path attaches a mintable `github` handle only when the grant is approved, and the mint handler re-checks at mint time (defense in depth — a grant revoked mid-lease refuses the next mint). **Implementation default:** one shared `isApproved(scope, integration)` read (a single indexed query on `core.integration_grants` for `status = approved`) called from both sites, because duplicating the predicate invites drift between lease and mint.

- **Dimension 2.1** — mint refused when the fleet has no approved `github` grant → typed `grant_required` (`UZ-GRANT-*`), no token, no upstream call → Test `test_mint_requires_approved_grant`
- **Dimension 2.2** — lease attaches a mintable `github` handle only when the grant is approved; no/revoked grant ⇒ the credential resolves static-only, no mintable id emitted → Test `test_lease_gates_mintable_on_grant`
- **Dimension 2.3** — a grant revoked between lease-issue and a later tool call → the next mint is refused (mint-time re-check, not just lease-time) → Test `test_mint_rechecks_revoked_grant`

### §3 — `agentsfleet connector` CLI ops

`agentsfleet connector list`/`status` reports connector state (connected / needs-reconnect / planned) as human text and a stable `--json` shape; acting through a disconnected connector returns a structured-JSON error with a non-zero exit and a reconnect suggestion (RULE JCL). **Implementation default:** read state from the existing `connectors/github/status.zig` endpoint the connect surface already exposes — the CLI is a renderer, not a new state source.

- **Dimension 3.1** — `connector list`/`status` reflects live github state; the `--json` shape is stable and documented → Test `test_cli_connector_status`
- **Dimension 3.2** — acting through a disconnected connector → structured-JSON error + non-zero exit + reconnect suggestion (never a silent 401) → Test `test_cli_connector_disconnected_error`

### §4 — Docs sweep (absorb C1–C9)

Reconcile the architecture docs with shipped reality per the review companion: the manual `gh api …/hooks` webhook-registration model is replaced by App-connect; the "platform never holds the user's PAT" claim is reworded to distinguish the platform App key. **Implementation default:** owners before echoes (`data_flow.md`/`capabilities.md` first, then the glossary/scenario echoes) — the review's "Docs to update" list is the exact worklist.

- **Dimension 4.1** — no architecture doc still asserts the user-`gh api …/hooks` manual registration for the App path → Test `test_docs_no_manual_gh_hook_for_app` (grep-based)
- **Dimension 4.2** — the "never holds the user's PAT" claim is reworded to distinguish the platform App key from a user PAT → Test `test_docs_app_key_vs_user_pat` (grep-based)

## Interfaces

```
Generic App-webhook ingress (bearer-less; provider signature is the trust anchor):
  POST /v1/ingress/{provider}   # provider="github"; one platform App secret; payload.installation.id
      -> verify/router registry (one descriptor per provider; extends webhook_verify.PROVIDER_REGISTRY)
      -> verify provider signature -> routing_key_path -> installation_id -> workspace -> matching fleet(s)
      -> XADD fleet:{id}:events
      | rejected{ bad_sig | unmapped_installation }   (tagged union; no XADD; UZ-WH-*)
  # the per-provider slack_events route (/v1/connectors/slack/events) is unchanged; its migration onto
  #   the generic ingress is Out of Scope.

Grant-gated mint (edits the EXISTING route + lease path — no new endpoint):
  isApproved(scope, integration) -> bool            # one indexed read on core.integration_grants (status=approved)
  POST /v1/runners/me/credentials/mint  -> { token, expires_at } | grant_required(UZ-GRANT-*) | reconnect_required | ...
  lease-issue: a mintable handle is attached to ExecutionPolicy.mintable ONLY when isApproved(...) is true

Connector CLI (structured-JSON on --json / when piped; non-zero exit on disconnected):
  agentsfleet connector list            -> [{ provider, state: connected|needs_reconnect|planned }]
  agentsfleet connector status github   -> { provider, state, hint? }
  # disconnected action -> { error: { code, message, suggestion:"agentsfleet connector ..." } } + exit != 0
```

Mint + ingress results are tagged unions; the route path, provider ids, `installation.id` routing path, and grant-status literals are named constants shared verbatim with tests (RULE UFS). No existing endpoint shape is repurposed; the mint route gains a refusal outcome, the lease path gains a gate — both additive.

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Ingress bad signature | tampered/unsigned App webhook payload | typed `rejected{bad_sig}` (`UZ-WH-*`); no `XADD`; logged, no fleet woken |
| Ingress unmapped installation | `installation_id` maps to no workspace (uninstalled/never-connected) | typed `rejected{unmapped_installation}`; no `XADD`; no leak of which ids exist |
| Ingress new provider added | a descriptor added for a provider with a known signature family | routes with zero `/v1/ingress` handler edits (`test_ingress_registry_is_data_driven`) |
| Mint without approved grant | connected fleet + valid lease but no approved `github` grant | typed `grant_required` (`UZ-GRANT-*`); no token; no upstream GitHub call |
| Grant revoked mid-lease | grant approved at lease-issue, revoked before a later tool call | the mint-time re-check refuses; the next mint returns `grant_required` |
| CLI acting through a disconnected connector | `connector status` / a tool run against needs-reconnect state | structured-JSON error + non-zero exit + reconnect suggestion |
| CLI daemon unreachable | control-plane down during `connector list` | structured-JSON transport error + non-zero exit; no partial/fabricated state |
| Token leak to logs | logging an ingress payload, a mint result, or a grant row | never logged or returned (VLT); only non-secret provider/state/installation-presence appears |

## Invariants

1. **A mint requires an approved integration grant** — enforced by code at BOTH the lease classifier (`secrets_resolve`) and the mint handler (`credentials_mint`) via the shared `isApproved` read; `test_mint_requires_approved_grant` + `test_lease_gates_mintable_on_grant` + `test_mint_rechecks_revoked_grant` assert no token without an approved grant. (Restores M102_001 Invariant 3, currently un-enforced on main.)
2. **Adding an ingress provider adds no branch to the `/v1/ingress` dispatch** — the provider registry is data; `test_ingress_registry_is_data_driven` proves a fake descriptor routes with no handler edit; the compile-time dup guard in `PROVIDER_REGISTRY` holds.
3. **The ingress verifies the signature before any routing or side-effect** — verification precedes the routing-key read and the `XADD`; a `bad_sig` payload produces zero `XADD` (`test_ingress_routes_by_installation_id` negative leg).
4. **Ingress + mint + grant frames never carry a secret** — VLT; only provider/state/installation-presence/expiry-bool appear in any log or response.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| `credential_mint_denied` (operator log) | ops | a mint is refused for a missing/revoked grant | integration id, fleet id, refusal reason | no token/handle/grant payload bytes (VLT) | `test_mint_requires_approved_grant` |
| `ingress_rejected` (operator log) | ops | an inbound App webhook fails verify or maps to no workspace | provider, reason (bad_sig / unmapped), delivery id | no payload body, no signature bytes | `test_ingress_routes_by_installation_id` |

No product analytics or funnel change — this is operator/security-plane observability only; Discovery records "Metrics review: no analytics/funnel playbook update required" with that reason.

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | integration | `test_ingress_routes_by_installation_id` | signed github App webhook → verified → `XADD` to the mapped fleet only; bad sig / unmapped id → no `XADD` |
| 1.2 | integration | `test_ingress_registry_is_data_driven` | a fake provider descriptor verifies + routes with no `/v1/ingress` handler edit |
| 2.1 | integration | `test_mint_requires_approved_grant` | no approved grant → `grant_required`, no token, no upstream call |
| 2.2 | integration | `test_lease_gates_mintable_on_grant` | approved → mintable handle attached; no/revoked → static-only, no mintable id |
| 2.3 | integration | `test_mint_rechecks_revoked_grant` | grant revoked after lease-issue → next mint refused (mint-time re-check) |
| 3.1 | e2e (cli) | `test_cli_connector_status` | `connector status --json` reflects live github state; shape stable |
| 3.2 | integration | `test_cli_connector_disconnected_error` | disconnected → structured-JSON error + non-zero exit + reconnect suggestion |
| 4.1 | unit (doc grep) | `test_docs_no_manual_gh_hook_for_app` | no arch doc asserts user-`gh api …/hooks` for the App path |
| 4.2 | unit (doc grep) | `test_docs_app_key_vs_user_pat` | the "never holds the user's PAT" claim distinguishes the platform App key |

**Regression:** the shipped §1–§4 mint path, the §5-connect callback, static custom-secret resolution, the existing `slack_events` ingress, and model routing stay unchanged except assertions tracking the additive grant-gate + ingress route. **Idempotency/replay:** repeated App deliveries dedupe on the delivery id (no double `XADD`); a re-approved grant re-enables minting; `connector status` is read-only and idempotent. **Integration coverage:** the ingress verify→route path (signed + tampered + unmapped), the grant-gate at both sites (approved / absent / revoked-mid-lease), and the CLI disconnected path are injected deterministically.

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | App webhook routes by `installation_id`; bad sig / unmapped → no `XADD` (§1) | `make test-integration 2>&1 \| grep -E "ingress_routes_by_installation_id"` | pass line present | P1 | |
| R2 | Adding a provider needs no `/v1/ingress` handler edit (§1) | `make test-integration 2>&1 \| grep -E "ingress_registry_is_data_driven"` | pass line present | P1 | |
| R3 | No mint without an approved grant, at lease AND mint (§2 — Invariant 3) | `make test-integration 2>&1 \| grep -E "requires_approved_grant\|gates_mintable_on_grant\|rechecks_revoked_grant"` | 3 pass lines | P0 | |
| R4 | `connector` CLI: live state + structured-JSON error on disconnected (§3) | `make test-unit-cli && make cli-acceptance` | exit 0 | P1 | |
| R5 | No doc asserts the manual `gh api …/hooks` App model (§4) | `make test 2>&1 \| grep -E "docs_no_manual_gh_hook_for_app\|docs_app_key_vs_user_pat"` | 2 pass lines | P1 | |
| R6 | Diff stays inside Files Changed | `git diff --name-only origin/main` | 0 paths missing from the Files Changed table | P0 | |
| S1 | Unit tests pass | `make test` | exit 0 | P0 | |
| S2 | Lint clean | `make lint` | exit 0 | P0 | |
| S3 | Integration passes | `make test-integration` | exit 0 | P0 | |
| S5 | No leaks (allocator wiring touched) | `make memleak` | exit 0 | P0 | |
| S6 | Cross-compile (Zig touched) | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` | exit 0 | P0 | |
| S7 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S8 | No oversize source file | `git diff --name-only origin/main \| grep -v '\.md$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | |
| S9 | Orphan sweep | Dead Code Sweep greps | 0 matches | P0 | |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line (`342 passed`); long evidence goes to PR Session Notes with a pointer here. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE; a P1 ❌ ships only with an Indy-acked deferral quote in Discovery.

## Dead Code Sweep

**1. Orphaned files — none deleted; this workstream is additive.**

**2. Orphaned references — grep after the changes; non-zero = stale.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| duplicated grant predicate | `grep -rn "status.*approved\|isApproved" src/agentsfleetd --include='*.zig' \| grep -v "integration_grant_lookup\|test" \| head` | 0 (the approved-grant read lives in one shared module) |
| provider id literal | `grep -rn '"github"' src/agentsfleetd/http/handlers/webhooks --include='*.zig' \| grep -v const \| head` | only named-constant defs + imports (RULE UFS) |

## Out of Scope

- Migrating the existing provider-specific `slack_events` ingress (`/v1/connectors/slack/events`) onto the generic `/v1/ingress/{provider}` — a follow-up; §1 ships the generic route with the github descriptor and leaves slack's route untouched.
- The `oauth_refresh` integration + Slack/Zoho/Jira/Linear outbound connectors and their inbound ingress descriptors / new scheme impls / lifecycle handshakes — inherited from M102_001's Out of Scope; each new provider is a descriptor, a scheme impl only for a new signature family.
- Per-fleet cryptographic identity / Agent Auth wire-format alignment; Stripe Agentic Commerce Protocol; exact-action approval-hash binding — all v3 / named follow-ups per M102_001.
- Any change to the shipped §1–§4 mint broker or the §5-connect surface beyond the additive grant-gate — those dimensions are DONE in M102_001 and stay unchanged.

---

## Product Clarity (authoring record)

1. **Successful user moment** — an operator connects GitHub, approves the fleet's `github` grant, and a teammate's Pull Request wakes the fleet: the App webhook lands (§1), the fleet mints because the grant is approved (§2), and the operator can confirm "connected" from `agentsfleet connector status` (§3) — no silent drops, no ungated tokens.
2. **Preserved user behaviour** — the shipped connect flow, the §1–§4 on-demand mint, static custom secrets, the existing `slack_events` ingress, and model routing all keep working; a fleet that already had an approved grant is unaffected.
3. **Optimal-way check** — the most direct finish: the ingress rides the existing verifier, the grant-gate rides the existing mint route + lease path, the CLI is a renderer over the existing status endpoint. The gap to unconstrained-optimal (migrating slack onto the generic ingress, a fully registry-driven `PlatformSecrets`) is deferred — the abstraction lands, convergence later.
4. **Rebuild-vs-iterate** — iterate. Every surface exists; this wires the four deferred dimensions onto them. Verdict: targeted completion, not a refactor.
5. **What we build** — the generic `/v1/ingress/{provider}` receiver + github descriptor; the approved-grant read at lease + mint; the `agentsfleet connector` command; the C1–C9 docs sweep.
6. **What we do NOT build** — slack-route migration; new outbound connectors; per-fleet identity; any schema change (the grant table exists).
7. **Fit with existing features** — compounds with the shipped mint broker, the connect surface, and the approval inbox; must not destabilize the existing webhook verifier, the mint hot path, or static-secret resolution.
8. **Surface order** — backend-first within the workstream (§2 grant-gate is the security priority and unblocks a truthful §3 CLI signpost); §1 ingress and §4 docs are independent.
9. **Dashboard restraint** — no new UI; the CLI reports only real connector state read from the live endpoint, never a fabricated "connected".
10. **Confused-user next step** — a fleet that can't reach GitHub sees a typed `grant_required` / `reconnect_required` (CLI structured error + suggestion), never a silent 401; the reconnect/grant path is the self-serve move.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** four Sections mapping 1:1 to M102_001's four deferred dimensions (§5.2/§5.4 → §1; §6 → §2; §7 → §3; §8 → §4). §2 is separated and P0 because it is a security-invariant restoration, not cosmetic.
- **Alternatives considered:** (a) reopen M102_001 and land the tail there — rejected: the spec is closed/merged in `done/`, and reopening a merged workstream muddies the shipped record; a clean fast-follow workstream is the project's pattern. (b) fold the grant-gate silently into an unrelated PR — rejected: Invariant 3 is load-bearing and deserves its own reviewed Section with negative tests. (c) migrate slack onto the generic ingress now — deferred: it widens the blast radius past the tail's scope.
- **Patch-vs-refactor verdict:** this is a **patch** (targeted completion) — it wires four deferred dimensions onto surfaces that already exist and introduces no new trust plane. The slack-ingress convergence and a registry-driven `PlatformSecrets` are the named follow-ups if the connector set grows.

## Discovery (consult log)

- **Consults** — Architecture / Legacy-Design / gate-flag triage: the question asked + Indy's decision.
- **Metrics review** — events added, extra events found during `/review`, analytics/funnel playbook update or the explicit no-change reason.
- **Skill-chain outcomes** — `/write-unit-test`, `/review`, `kishore-babysit-prs` results (order per `AGENTS.md` CHORE(close); iteration counts, findings dispositioned).
- **Deferrals** — every "deferred to follow-up" needs an **Indy-acked verbatim quote** here, format `> Indy (YYYY-MM-DD HH:MM): "<quote>" — context: <which item, why>`. An agent-unilateral deferral is **incomplete scope, not deferral**, and blocks CHORE(close) until the item lands or the quote is captured.
- **Origin (Indy + Orly, Jul 05, 2026):** carved from M102_001 on Indy's "Split + defer the tail" decision during post-merge cleanup. Live main-tree audit this session confirmed the split map: §1–§4 + §5-connect DONE; §5.2/§5.4 ingress, §6 grant enforcement, §7 CLI, §8 docs absent. The §6 finding is the reason §2 is P0 — `credentials_mint.zig` calls `broker.mint` with no grant read and `secrets_resolve.mintableId` classifies without one, so M102_001 Invariant 3 is currently un-enforced on main.
