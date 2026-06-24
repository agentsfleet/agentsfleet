<!--
SPEC AUTHORING RULES (load-bearing — do not delete):
- No time/effort/hour/day estimates anywhere. No effort columns, complexity ratings,
  percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only sequencing signal.
- If a section below contradicts these rules, the rule wins — delete the section.
- Enforced by SPEC TEMPLATE GATE (dispatch/write_spec.md) and audits/spec-template.sh.
-->

# M99_001: Connectors mint short-lived tokens through one credential broker; adding a connector is data, not daemon code

**Prototype:** v2.0.0
**Milestone:** M99
**Workstream:** 001
**Date:** Jun 23, 2026
**Status:** PENDING
**Priority:** P1 — operator-facing: a fleet can act on GitHub via a one-click Connect, and the platform stops forcing a hand-pasted Personal Access Token (PAT) as a custom secret. Also forestalls a design trap surfaced by Indy: per-connector minters bolted into agentsfleetd that do not scale.
**Categories:** Application Programming Interface (API), Command-Line Interface (CLI), User Interface (UI)
**Batch:** B1 — single workstream; suggested staging is the broker core + first driver + mint channel + bridge (§1–§4, backend) before the connector surfaces (§5–§6). Surfaces may split to M99_002 if the diff gets heavy.
**Branch:** {feat/m99-connectors-credential-broker — added when work begins}
**Depends on:** none hard. Composes with M98_001 (Credentials vault UI + the self-managed credential JSON shape); does not block on it.
**Provenance:** agent-generated (interactive design session with Indy, Jun 23, 2026) — born from decoupling the GitHub connector out of M98_001 after stress-testing the token lifecycle (25h gap before first event, 24h-active fleet, single run > token lifetime, steer-after-6h with no trigger). Re-confirm at PLAN.

**Canonical architecture:** `docs/AUTH.md` (the credential boundary — secrets ride the lease, the App private key is platform-side, the sandbox child holds no control-plane key) + `docs/architecture/runner_fleet.md` (daemon ↔ runner ↔ sandboxed-child model). This spec adds a daemon-side broker + a child→runner→daemon mint request; it introduces no new trust plane.

---

## The caveat this spec exists to answer (Indy, Jun 23, 2026)

> **Indy's issue — "the agentsfleetd-credentials-proxy: This will become too static that expanding and supporting more connectors will be a pain."**

A naive connector adds a bespoke minter to agentsfleetd. By the fifth integration the daemon is a junk drawer of per-vendor token logic. **The whole point of this spec is that adding a connector is a config-driven driver descriptor (data), not a new code branch** (RULE CFG). GitHub is the first driver, not a special case.

---

## Implementing agent — read these first

1. `docs/AUTH.md` §"Fleet Bundle import and credential boundary", §"Runner token", §"Webhook auth" — the credential boundary, the `agt_r` runner control plane (`/v1/runners/me/*`), and the existing GitHub webhook (`POST /v1/webhooks/{fleet_id}/github`, HMAC over vault `fleet:github`). **Auth-flow file — read before touching connect/callback/token-minting.**
2. `src/agentsfleetd/fleet/secrets_resolve.zig` + `src/lib/contract/execution_policy.zig` — `resolveSecretsMap` (vault → `secrets_map`) is the abstraction this spec generalizes from "load static JSON at lease" to "resolve-or-mint on demand."
3. `src/runner/engine/tool_bridge.zig` + `src/runner/engine/runtime/policy_http_request.zig` — where `${secrets.X.field}` is substituted at the tool boundary; the bridge gains a mintable-kind path.
4. `src/runner/child_supervisor.zig` + `src/runner/child_process.zig` — the runner↔child pipe + the fail-closed env allowlist (`ENV_DENY_PREFIX = "AGENTSFLEET_"`). The new child→runner mint request is a local-pipe round-trip alongside these.
5. `dispatch/write_zig.md` + `dispatch/write_ts_adhere_bun.md` — Zig (ZIG/PUB/LIFECYCLE) and TS (FILE SHAPE, DESIGN TOKEN) discipline.

---

## PR Intent & comprehension handshake

- **PR title (eventual):** `feat(m99): credential broker + GitHub App connector (on-demand token mint)`
- **Intent (one sentence):** a workspace clicks "Connect GitHub," and from then on its fleets mint a short-lived, workspace-scoped GitHub token **on demand at the moment a tool needs it** — through one config-driven broker whose next connector is a descriptor, never a new daemon branch.
- **Handshake (agent fills at PLAN, before EXECUTE):** restate intent; list `ASSUMPTIONS I'M MAKING:`. Confirm: (a) the runner control plane (`agt_r`, `/v1/runners/me/*`) is the right host for a `…/credentials/mint` endpoint; (b) the child→runner local-pipe request shape (no new network surface from the sandbox); (c) GitHub App registration + where the App private key is stored platform-side (admin vault); (d) the broker may cache a minted token until near expiry. A `[?]` blocks EXECUTE.

---

## Product Clarity

1. **Successful user moment** — a workspace connects GitHub once at 11:00; the next day at 12:00 a teammate opens a Pull Request; the fleet wakes, mints a fresh token at that instant, pulls the PR, posts a review — and 6 hours later the operator steers it ("now check PR #500"), which mints again on the spot. The operator never saw a token, a webhook URL, or a paste field.
2. **Preserved user behaviour** — every existing flow is unchanged: static custom secrets (`GITHUB_TOKEN` as a custom secret) still resolve; the inbound webhook HMAC path is untouched; named/own-key model routing is untouched; a fleet with no connector behaves exactly as today.
3. **Optimal-way check** — on-demand mint through a broker is the most direct path: the token is born at use, so idle time and trigger source stop mattering. The gap to "perfect" (a generic OAuth-refresh driver for Zoho/Slack) is deferred — the abstraction lands now, the second driver later.
4. **Rebuild-vs-iterate** — iterate on the existing secrets path. `secrets_resolve` + the runner control plane + the tool bridge all exist; this generalizes them. A new credential service would trade determinism + the proven sandbox boundary for nothing.
5. **What we build** — a daemon-side `CredentialBroker` with a config-driven driver registry; the `github_app` driver; a child→runner→daemon on-demand mint request; transparent tool-bridge resolve-or-mint; the GitHub Connect/reconnect surface; webhook reuse + auto-registration; CLI/vault surfacing.
6. **What we do NOT build** — the `oauth_refresh` driver (Zoho/Slack stay "Planned," bridged by custom secrets); a standalone credentials microservice; non-GitHub connectors; per-credential fleet-usage analytics; credential rotation automation beyond mint-on-demand.
7. **Fit with existing features** — compounds with the M98 Credentials vault and the runner lease envelope; must not destabilize the sandbox env-allowlist boundary, the inbound webhook verifier, or static-secret resolution.
8. **Surface order** — CLI-first per repo default for the connector status/ops; the broker + mint channel are backend-first (the wire is the shared dependency). The UI Connect flow follows the auth contract.
9. **Dashboard restraint** — only GitHub shows a live "Connect"; Zoho/Slack render "Planned" with the custom-secret bridge hint; no token, webhook URL, or secret is ever displayed; a connector shows real connected/needs-reconnect state, never a fabricated one.
10. **Confused-user next step** — a user whose fleet can't reach GitHub sees a typed "Reconnect GitHub" state (from CLI structured JSON and the UI), not a silent 401; a user wanting Zoho today reads the inline hint to store a custom secret and self-serves.

---

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — RULE CFG (the driver registry + credential kinds are data, never per-integration branches — the spec's core), RULE VLT (App private key / refresh tokens / minted tokens never logged or returned), RULE PRI/NTP (treat connector inputs as hostile; narrow at the parse boundary), RULE ECL (reconnect/mint-failed/unknown-integration are typed classes, not fatal-silent), RULE EMS (standard error structure), RULE TGU (tagged-union mint result), RULE JCL (CLI JSON contract for connector status/errors), RULE UFS (kind ids `"github_app"`/`"static"`, the mint route, the `${secrets.…}` token shape → named constants shared verbatim with tests), RULE NDC/NLR/ORP.
- **`docs/AUTH.md`** — auth-flow: the connector is a token-minting surface; mirror the boundary (App key platform-side, sandbox holds no control-plane key), reuse the `agt_r` plane and the webhook verifier.
- **`dispatch/write_zig.md`** — tagged-union results, multi-step `errdefer`, pg-drain on any query, file ≤350 / fn ≤50, cross-compile both linux targets.
- **`dispatch/write_ts_adhere_bun.md`** — TS FILE SHAPE per new component, primitive substitution, DESIGN TOKEN gate (UI).
- No schema DDL anticipated → `docs/SCHEMA_CONVENTIONS.md` applies only if a connector-state column proves necessary (default: connector handle rides vault JSON, no migration).

---

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes — broker, driver, mint endpoint, channel | tagged-union results; `errdefer`; cross-compile `x86_64-linux` + `aarch64-linux` |
| PUB / Struct-Shape | yes — `CredentialBroker`, `Driver`, mint request/response | shape verdict per new pub surface; tagged-union mint result |
| File & Function Length (≤350/≤50/≤70) | yes — broker + drivers | one file per driver; broker dispatch table extracted |
| UFS (repeated/semantic literals) | yes — kind ids, mint route, token-placeholder shape, error strings | named constants in one module per side; tests import them |
| UI Substitution / DESIGN TOKEN | yes — Connect/reconnect rows | design-system primitives; theme tokens only |
| ERROR REGISTRY (`UZ-XXX-NNN`) | yes — reconnect / mint-failed / unknown-integration | register `UZ-GH-*` / `UZ-CRED-*` codes + `hint()` entries |
| LOGGING / OBS | yes — connect, mint, reconnect-required are observable | log/event per the LOGGING standard; no secret in any frame |
| SCHEMA | no — connector handle + webhook_secret ride vault JSON | — |

---

## Overview

**Goal (testable):** a `CredentialBroker.mint(workspace_id, integration, scope)` returns a short-lived `{token, expires_at}` by dispatching to a config-driven driver registry (`github_app` first); a sandboxed fleet child obtains that token **on demand** by asking its parent runner, which forwards over the `agt_r` control plane; the App private key never leaves the daemon; `${secrets.github.token}` in a SKILL.md transparently triggers the mint at the tool boundary; and adding a connector adds a driver descriptor with **zero** new branches in the mint dispatch.

**Problem:** GitHub is "Coming soon," so fleets that act on GitHub need a hand-pasted PAT stored as a custom secret — long-lived, broad, and at-rest in the vault. Worse, the obvious fix (mint a token per connector inside agentsfleetd) does not scale: each new integration would bolt bespoke minting logic into the daemon. There is also no clean moment to refresh a token for a long-idle interactive session steered hours later with no external trigger.

**Solution summary:** a daemon-side broker with a driver registry (data, not branches) mints short-lived workspace-scoped tokens; the sandbox child fetches them on demand through the runner so idle time and trigger source stop mattering; master keys stay platform-side; the existing webhook path and `secrets_map` resolution are reused, not reinvented. GitHub ships as the first driver + connector; the next connector is a descriptor.

---

## Prior-Art / Reference Implementations

- **Backend** → generalize `src/agentsfleetd/fleet/secrets_resolve.zig` (name→vault-JSON) into resolve-or-mint; mirror `resolveActiveProvider`'s just-in-time, daemon-side, never-persisted key handling (M80_009) for the minted token's lifecycle (`secureZero` after use).
- **Channel** → the `agt_r` runner control plane (`/v1/runners/me/*`, `runnerBearer`) is the existing authenticated daemon↔runner wire; the mint endpoint is one more route on it, not a new plane.
- **Pattern** → workload-identity / instance-metadata: the workload holds no key and requests a short-lived, scoped token from a trusted local broker at the moment of use.
- **CLI** → the "7 Pillars" — extend the existing connector/credential commands; structured-JSON errors with `suggestion`/`retry`.
- **UI** → M98 Credentials vault Integrations group + design-system primitives.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/agentsfleetd/credentials/broker.zig` | CREATE | `mint(workspace, integration, scope)`; driver-registry dispatch; minted-token cache till near expiry |
| `src/agentsfleetd/credentials/driver.zig` | CREATE | `Driver` interface + config-driven registry (kind → descriptor); `static` kind |
| `src/agentsfleetd/credentials/driver_github_app.zig` | CREATE | sign App JWT (platform key) → installation access token; typed reconnect on revoke |
| `src/agentsfleetd/http/handlers/runner/credentials_mint.zig` | CREATE | `POST /v1/runners/me/credentials/mint`; workspace derived from the lease, not the caller |
| `src/runner/engine/credential_request.zig` | CREATE | child→runner local-pipe mint request/response |
| `src/runner/engine/tool_bridge.zig` · `runtime/policy_http_request.zig` | EDIT | mintable-kind path: `${secrets.X.token}` → broker fetch; static kinds unchanged; partial-sub guard preserved |
| `src/agentsfleetd/fleet/secrets_resolve.zig` | EDIT | classify a credential as static vs mintable kind; emit a handle, not a token, for mintable kinds |
| `src/agentsfleetd/http/handlers/connectors/github_connect.zig` · `github_callback.zig` | CREATE | App-install auth flow; store `{kind:"github_app", installation_id}` in `fleet:github` (no token); read `docs/AUTH.md` |
| `src/agentsfleetd/fleet_runtime/webhook_register.zig` | CREATE | auto-register the repo/App webhook via the installation token; secret into the same `fleet:github` row |
| `ui/packages/app/app/(dashboard)/credentials/components/IntegrationsConnectors.tsx` | CREATE/EDIT | GitHub Connect / connected / **Reconnect** states; Zoho/Slack "Planned" |
| `cli/src/commands/connectors.ts` · `services/connectors.ts` | CREATE/EDIT | `agentsfleet connector` status/list; structured-JSON error on disconnected |
| _error registry module_ | EDIT | `UZ-GH-*` (reconnect/mint-failed) + `UZ-CRED-*` (unknown-integration) + `hint()` |
| _colocated tests (Zig `test {}` · `*.test.tsx` · `*.spec.ts`)_ | CREATE/EDIT | one test per Dimension below |

---

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** one workstream, six Sections. The broker + first driver + on-demand channel + bridge (§1–§4) are the foundation that answers the caveat; the GitHub connector surfaces (§5–§6) ride on top. The driver registry is the refactor that prevents the per-connector junk drawer.
- **Alternatives considered:** (a) store a long-lived PAT per connector — rejected: at-rest broad token, no scalability story; (b) mint per connector inline in agentsfleetd — rejected: Indy's caveat, doesn't scale; (c) a standalone credentials microservice — rejected: a new trust plane + deploy surface for a problem the existing `agt_r` plane already fronts; (d) mint-at-lease only — rejected: fails the steer-after-6h-no-trigger case.
- **Patch-vs-refactor verdict:** **targeted refactor** — generalize the existing secrets path into a broker with pluggable drivers, plus one new local-pipe round-trip. The `oauth_refresh` driver and a typed multi-connector catalog are named follow-ups.

---

## Sections (implementation slices)

### §1 — Credential Broker core + config-driven driver registry

The daemon-side broker exposes one mint interface and dispatches through a registry keyed by credential-kind. **Implementation default:** the registry is a data table (kind → descriptor), so a new connector is a registration, not a `switch` arm (RULE CFG). The broker caches a minted token until near its `expires_at` and re-mints when stale.

- **Dimension 1.1** — `mint(workspace, integration, scope)` returns `{token, expires_at}` via registry dispatch → Test `test_broker_dispatches_by_kind`
- **Dimension 1.2** — adding a driver is a descriptor; the mint hot path has no per-integration branch → Test `test_driver_registry_is_data_driven`
- **Dimension 1.3** — a cached token within validity is reused; a near-expiry token re-mints → Test `test_broker_caches_until_expiry`
- **Dimension 1.4** — an unconfigured/unknown integration → typed `unknown_integration`, no mint → Test `test_broker_unknown_integration_errors`

### §2 — `github_app` driver

The first real driver: sign a GitHub App token (JSON Web Token, JWT) with the platform-held App private key, exchange it for a ≤1h installation access token, return `{token, expires_at}`. **Implementation default:** the App private key + app id resolve from the platform/admin vault only.

- **Dimension 2.1** — valid installation handle → installation token with a near-future `expires_at` → Test `test_github_app_mints_installation_token`
- **Dimension 2.2** — installation uninstalled/revoked → typed `reconnect_required` (`UZ-GH-*`), no token → Test `test_github_app_revoked_reconnect`
- **Dimension 2.3** — the App private key never appears in any `ExecutionPolicy`, `secrets_map`, log, or frame → Test `test_github_app_key_never_leaves_broker`

### §3 — On-demand mint channel (child → runner → daemon)

The sandboxed child requests a token from its parent runner over the local pipe; the runner forwards to the broker over the `agt_r` control plane. **The workspace is derived from the lease server-side — a child-supplied workspace id is ignored.** This is what makes idle time and trigger source irrelevant.

- **Dimension 3.1** — child mint request → runner forwards → returns a short-lived token → Test `test_child_requests_token_via_runner`
- **Dimension 3.2** — the mint is authorized to the lease's workspace; a forged workspace id in the request is ignored → Test `test_mint_scoped_to_lease_workspace`
- **Dimension 3.3** — a long-idle session with no external trigger mints a fresh token at the tool call (steer-after-6h) → Test `test_on_demand_mint_no_trigger`

### §4 — Tool-bridge transparent resolve-or-mint

When the tool bridge resolves `${secrets.<integration>.token}` for a **mintable** kind, it fetches via the channel instead of reading a static value; substitution stays at the tool boundary. **Static kinds (a stored PAT) resolve exactly as today.**

- **Dimension 4.1** — `${secrets.github.token}` on a mintable kind triggers a broker fetch, substituted only at dispatch → Test `test_bridge_mints_on_placeholder`
- **Dimension 4.2** — the partial-substitution guard holds: any residual `${secrets.` aborts the call → Test `test_bridge_refuses_partial_sub`
- **Dimension 4.3** — a static-kind credential resolves with no mint (unchanged path) → Test `test_bridge_static_unchanged`

### §5 — GitHub connector surface (Connect / callback / reconnect) + webhook

Credentials → Integrations gains a live GitHub **Connect** running the App-install auth flow, storing `{kind:"github_app", installation_id}` (no token). On connect, the repo/App webhook is auto-registered via the installation token; its secret lands in the **same** `fleet:github` row the inbound HMAC verifier already reads.

- **Dimension 5.1** — Connect → App install → callback stores the handle (no token) → Test `test_github_connect_stores_handle`
- **Dimension 5.2** — connected state shows; uninstalled/revoked shows **Reconnect**; Zoho/Slack render "Planned" → Test `test_github_states_and_planned`
- **Dimension 5.3** — one `fleet:github` row serves both readers (minted-token consumer + `webhook_secret` verifier); inbound HMAC path unchanged → Test `test_one_row_two_readers`

### §6 — CLI + vault surfacing

`agentsfleet connector` lists status (connected / needs-reconnect) as human + structured JSON; a disconnected connector returns a structured-JSON error with a non-zero exit (RULE JCL).

- **Dimension 6.1** — `connector list`/`status` reflects github state; `--json` shape is stable → Test `test_cli_connector_status`
- **Dimension 6.2** — acting through a disconnected connector → structured-JSON error + non-zero exit + reconnect suggestion → Test `test_cli_connector_disconnected_error`

---

## Interfaces

```
CredentialBroker (daemon-side):
  mint(workspace_id, integration, scope?) -> ok{ token, expires_at }
                                           | reconnect_required | unknown_integration | mint_failed   (tagged union)
  driver registry:  kind -> Driver{ mint(handle, platform_secrets) }   # kinds: github_app | static | (oauth_refresh: future)

Runner mint endpoint (existing agt_r control plane):
  POST /v1/runners/me/credentials/mint   { lease_id, integration, scope? } -> { token, expires_at } | { error }
  # workspace is derived from lease_id server-side; a caller-supplied workspace is ignored

GitHub connect (UI auth flow):
  Connect -> GitHub App install -> callback { installation_id }
          -> vault fleet:github = { "kind":"github_app", "installation_id":"…", "webhook_secret":"…" }   # NO token stored

Tool placeholder (UNCHANGED for SKILL authors):
  ${secrets.github.token}  -> bridge: mintable kind -> broker mint ; static kind -> stored value
```

Mint result is a tagged union. `${secrets.…}` shape and kind ids are named constants shared verbatim with tests (RULE UFS). No existing endpoint or `ExecutionPolicy` field is repurposed; `secrets_map` gains mintable-handle entries alongside static ones.

---

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Unknown / unconfigured integration | mint for an integration the workspace never connected | typed `unknown_integration`; no token; CLI structured error |
| Installation revoked in the gap | App uninstalled between connect and use | typed `reconnect_required` (`UZ-GH-*`); UI "Reconnect", CLI error — never a silent 401 |
| Mint upstream failure | GitHub 5xx / network on the token exchange | typed `mint_failed` (retryable class, ECL); tool call fails loudly, surfaced in Events |
| Forged workspace in mint request | a prompt-injected child supplies another workspace id | ignored; broker binds to the lease's workspace; request authorized only for that workspace |
| App private key exfil attempt | injected fleet reads env / secrets_map for the key | structurally absent from the child (env deny-prefix + key never in the lease); nothing to read |
| Stale cached token | token expired mid-run | broker re-mints transparently on next fetch; caller sees a valid token |
| Static-secret regression | mintable path accidentally changes static resolution | static kinds resolve byte-for-byte as before (guarded by test) |
| Partial placeholder substitution | a `${secrets.` survives substitution | dispatch aborted (existing leak guard) |
| Webhook secret missing post-connect | auto-registration stored no secret | inbound `UZ-WH-020` (existing); connector surfaces "reconnect/repair" |
| api_key / token leak to logs | logging a mint result or handle | never logged or returned (VLT); only non-secret status/host appears |

---

## Invariants

1. **The App private key (and any master/refresh secret) never enters the sandbox child** — enforced by the env deny-prefix (`AGENTSFLEET_*`), the broker living daemon-side, and `test_github_app_key_never_leaves_broker` asserting `secrets_map`/policy/frames carry no key or `github_app` handle.
2. **A mint is scoped to the lease's workspace** — the broker derives workspace from `lease_id` server-side; a caller-supplied workspace is ignored (`test_mint_scoped_to_lease_workspace`).
3. **Adding a connector adds no branch to the mint hot path** — the registry is data; enforced by `test_driver_registry_is_data_driven` (dispatch is table-driven; no per-integration arm).
4. **Minted tokens are short-lived** — every driver returns `expires_at`; the broker never hands out an expired token, re-minting instead (`test_broker_caches_until_expiry`).
5. **Mint result is a tagged union** — no fatal-silent path; every outcome is `ok | reconnect_required | unknown_integration | mint_failed` (compile-checked exhaustiveness).
6. **Secrets never logged or returned** — VLT; only host/status/expiry-bool appear in any frame or log (log audit + `test`).

---

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_broker_dispatches_by_kind` | `mint(ws,"github",…)` routes to the github_app driver; returns `{token,expires_at}` |
| 1.2 | unit | `test_driver_registry_is_data_driven` | registering a fake kind makes it mintable with no edit to the dispatch function |
| 1.3 | unit | `test_broker_caches_until_expiry` | within validity → same token, no re-mint; past threshold → re-mint |
| 1.4 | unit | `test_broker_unknown_integration_errors` | unconfigured integration → `unknown_integration`; no upstream call |
| 2.1 | integration | `test_github_app_mints_installation_token` | valid installation handle → token + future `expires_at` (fake GitHub) |
| 2.2 | unit | `test_github_app_revoked_reconnect` | 404/installation-gone → `reconnect_required` (`UZ-GH-*`) |
| 2.3 | unit | `test_github_app_key_never_leaves_broker` | grep the produced policy/secrets_map/frames → zero key/handle bytes |
| 3.1 | integration | `test_child_requests_token_via_runner` | child request → runner forwards → short-lived token returned |
| 3.2 | integration | `test_mint_scoped_to_lease_workspace` | request with a foreign workspace id → resolved to the lease's workspace only |
| 3.3 | integration | `test_on_demand_mint_no_trigger` | idle session, no webhook/API event → tool call mints a fresh token |
| 4.1 | unit | `test_bridge_mints_on_placeholder` | mintable `${secrets.github.token}` → broker fetch; value only at dispatch |
| 4.2 | unit | `test_bridge_refuses_partial_sub` | residual `${secrets.` → call aborted |
| 4.3 | unit | `test_bridge_static_unchanged` | static-kind credential → resolved with no mint call |
| 5.1 | integration | `test_github_connect_stores_handle` | callback `{installation_id}` → `fleet:github` has handle, no token |
| 5.2 | unit | `test_github_states_and_planned` | connected/reconnect render for github; zoho/slack "Planned" + bridge hint |
| 5.3 | integration | `test_one_row_two_readers` | same `fleet:github` row → mint reads handle, webhook verifier reads `webhook_secret` |
| 6.1 | e2e (cli-acceptance) | `test_cli_connector_status` | `connector status --json` reflects connected/needs-reconnect vs live API |
| 6.2 | integration | `test_cli_connector_disconnected_error` | disconnected → structured-JSON error + non-zero exit + reconnect suggestion |

**Regression:** static custom-secret resolution, named/own-key model routing, the inbound webhook HMAC path, and the sandbox env allowlist are all unchanged except assertions tracking intentional additions. **Idempotency/replay:** re-minting yields a fresh valid token; a cached token is reused until near expiry; repeated connect is idempotent on the `installation_id`. **Integration coverage:** broker→github_app→fake-GitHub mint, child→runner→broker round-trip, and the revoked-installation reconnect path are injected deterministically.

---

## Acceptance Criteria

- [ ] Broker mints + caches + dispatches by data-driven driver; github_app driver mints installation tokens — verify: `make test && make test-integration`
- [ ] App key never in the child; mint scoped to lease workspace — verify: `make test` (the two invariant tests) + `make memleak`
- [ ] On-demand mint covers idle/no-trigger; tool bridge resolves mintable vs static — verify: `make test-integration`
- [ ] GitHub Connect/reconnect UI + one-row-two-readers + webhook auto-register — verify: `make test-unit-app && make acceptance-e2e`
- [ ] CLI connector status/errors — verify: `make test-unit-cli && make cli-acceptance`
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
# E4: lint + gitleaks
make lint 2>&1 | grep -E "✓|FAIL"; gitleaks detect 2>&1 | tail -3
# E5: 350-line gate (exempts .md)
git diff --name-only origin/main | grep -v '\.md$' | xargs wc -l 2>/dev/null | awk '$1 > 350 {print "OVER: "$2": "$1}'
# E6: invariant sweeps (empty = pass) — App key never in child; kind ids are constants
grep -rn "private_key\|app_private" src/agentsfleetd/runner src/runner --include='*.zig' | grep -iv "test\|//" | head
grep -rn '"github_app"' src/ cli/ ui/ | grep -v const | head
```

---

## Dead Code Sweep

**1. Orphaned files** — none deleted; this is additive. New files per Files Changed.

**2. Orphaned references** — grep after the changes; non-zero = stale.

| Removed/renamed symbol | Grep | Expected |
|------------------------|------|----------|
| ad-hoc per-call vault reads bypassing the broker | `grep -rn "vault.loadJson" src/agentsfleetd/runner --include='*.zig' \| head` | 0 (mintable creds route through the broker) |
| `"github_app"` literal | `grep -rn '"github_app"' src/ cli/ ui/ \| grep -v const \| head` | only the named-constant defs + imports (RULE UFS) |

---

## Discovery (consult log)

> **Empty at creation.** Populate as work surfaces consults, skill outcomes, and any Indy-acked deferrals.

- **Origin (Indy, Jun 23, 2026):** decoupled the GitHub connector out of M98_001 after a long stress-test of the token lifecycle. Indy's caveat is the spec's reason to exist — *"the agentsfleetd-credentials-proxy: This will become too static that expanding and supporting more connectors will be a pain."* Resolution direction: config-driven driver registry (RULE CFG), connector = descriptor.
- **Design decisions (Indy, Jun 23, 2026):** mint daemon-side only (App key in the child = cross-tenant breach via one prompt-injected PR); on-demand mint at the tool call (covers 25h-gap, 24h-active, single-run > token lifetime, and steer-after-6h-no-trigger); reuse the `agt_r` plane + the existing webhook verifier; one `fleet:github` row serves both readers.
- **Open to confirm at PLAN:** GitHub App registration ownership + where the App private key is stored platform-side; the child→runner mint request shape (local-pipe framing); whether `oauth_refresh` lands as a stub interface now or wholly deferred.
- **Deferrals** — the `oauth_refresh` driver (Zoho/Slack connectors) is **Out of Scope** here, not a dropped Dimension; custom secrets bridge them until then.

---

## Skill-Driven Review Chain (mandatory)

| When | Skill | What it does | Required output |
|------|-------|--------------|-----------------|
| After implementation, before CHORE(close) | `/write-unit-test` | Audits diff coverage vs the Test Specification (≥50% negative; every Failure Mode covered; integration + cli-acceptance present) | Clean; iteration count in Discovery |
| After tests pass, before CHORE(close) | `/review` | Adversarial diff review vs this spec, `docs/AUTH.md`, `dispatch/write_zig.md`, `dispatch/write_ts_adhere_bun.md`, Failure Modes, Invariants (esp. the key-never-in-child + workspace-scope guards) | Clean or every finding dispositioned |
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
| Key-never-in-child sweep | Eval E6 | {paste snippet} | |

---

## Out of Scope

- The `oauth_refresh` driver + Zoho/Slack connectors — follow-up; custom secrets bridge them until a driver lands. (The abstraction is built here so they are descriptors, not new code.)
- A typed multi-connector catalog UI beyond GitHub-live / others-Planned.
- Credential rotation automation beyond mint-on-demand + cache-till-expiry.
- Per-credential fleet-usage analytics.
- Replacing static custom secrets — they remain a first-class kind (`static` driver).
