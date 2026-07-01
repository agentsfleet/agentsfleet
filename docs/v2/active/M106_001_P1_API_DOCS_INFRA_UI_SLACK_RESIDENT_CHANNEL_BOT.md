# M106_001: `@agentsfleet` Slack-resident channel bot — reactive on-ramp

**Prototype:** v2.0.0
**Milestone:** M106
**Workstream:** 001
**Date:** Jun 30, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — first customer-facing surface that lives where users already work (Slack); the acquisition on-ramp to the durable hired teammate.
**Categories:** API, DOCS, INFRA, UI
**Batch:** B1 — standalone; reuses shipped substrate, blocks the Rung-1 follow-on.
**Branch:** feat/m106-slack-resident
**Test Baseline:** unit=2190 integration=210 (`make _lint_zig_test_depth` @ origin/main 8c41ae18)
**Depends on:** None — builds entirely on shipped substrate (M80 runner/memory continuity, M28 webhook-signature middleware, the GitHub OAuth connector pattern). Rung 1 (the hired-teammate follow-on) will depend on this + M103 (template catalogue) + M105 (schedules).
**Provenance:** agent-generated (pre-spec; brainstorm transcript Jun 30, 2026 — office-hours → sequence design → consumption-ladder refinement → code-grounded reuse correction).

> **Provenance is load-bearing.** LLM-drafted, then corrected against the named files. The value of this spec is that ~80% is reuse, so a wrong reuse pointer is the main risk. Every reuse pointer below was checked against the code on Jun 30, 2026 (the actor, fleet-creation, and storage claims were corrected after that check — see Discovery).

**Canonical architecture:** `docs/architecture/runner_fleet.md` §Memory continuity + `docs/architecture/data_flow.md` §B (single-ingress trigger model). This milestone ADDS a Slack producer to that model and a per-channel resident fleet created through the existing fleet-create path; it invents no new runtime primitive.

---

## Implementing agent — read these first

1. `docs/architecture/runner_fleet.md` §Memory continuity + `docs/architecture/memory.md` — the `GET`/`POST /v1/runners/me/memory/{fleet_id}` hydrate/capture loop the channel memory reuses **verbatim**; the durable scope column is `fleet_id` (the legacy `instance_id` name is retired — `schema/013`), durable in `memory.memory_entries`, `:memory:` SQLite in the child.
2. `src/agentsfleetd/credentials/integration_github.zig` + `src/agentsfleetd/http/handlers/connectors/github/` — the OAuth connector + platform-secret (`crypto_store.load`) pattern the Slack app install mirrors. **Note:** GitHub persists its install as a **vault JSON handle keyed `(workspace_id, "github")`** (`callback.zig:86`) with **zero entity tables**; status is a handle-existence check (`status.zig:40`); the broker mints from the handle at runtime. Slack mirrors this for the token + metadata and adds only a generic inbound-routing index (item 6).
3. `src/agentsfleetd/auth/middleware/webhook_hmac.zig` + `webhook_sig.zig` — constant-time signature verify and the `UZ-WH-0xx` error taxonomy the Slack v0 scheme mirrors.
4. `src/agentsfleetd/http/handlers/webhooks/fleet.zig` — the **signature-authenticated producer**: per-fleet HMAC only, **no OIDC principal** (`:3`), a **free-form** `actor="webhook:{src}"` (`:104`), and a direct `XADD` of an `EventEnvelope` (`:114`). The Slack mention reuses *this* producer shape with `actor=slack:<user>`. It does **NOT** reuse `fleets/messages.zig`, whose steer ingress is gated on `authorizeWorkspace(hx.principal, …)` (`messages.zig:67`) and derives the actor from the principal (`:177`) — authority the signature-only Slack plane does not have.
5. `src/agentsfleetd/http/handlers/fleets/create.zig` (`innerCreateFleet` `:64` → its request-independent core `insertFleetOnConn` `:260`) + `create_stream.zig` — the **only** path that inserts `core.fleets` (runners never create fleets). `innerCreateFleet` is HTTP-coupled (parses `*httpz.Request`, needs a principal); the principal-less channel-fleet materialization therefore **calls `insertFleetOnConn` directly** (it never inserts `core.fleets` itself), seeded with the default channel-bot `skill.md` as `source_markdown` + a code-built reactive config, under the install-delegated workspace authority.
6. `schema/010_core_integration_grants.sql` + `schema/020_tenant_providers.sql` — convention reference for the two **generic, provider-keyed** routing tables this milestone adds (`connector_installs`, `connector_channels`). The inbound `team_id → workspace_id` reverse lookup is the one piece the vault (keyed by `workspace_id`) cannot serve, because Slack events arrive addressed only by `team_id`.

---

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(m106): Slack-resident @agentsfleet channel bot (rung-0 on-ramp)
- **Intent (one sentence):** a workspace admin connects Slack once in the dashboard, and from then on `@agentsfleet` answers in any channel it's invited to — learning that channel over time — as the read-only on-ramp that later converts to a durable hired teammate.
- **Handshake (agent fills at PLAN, before EXECUTE):** restate intent + `ASSUMPTIONS I'M MAKING: …`; a mismatch with the Intent above → STOP and reconcile.

---

## Product Clarity (answer in order, at authoring)

1. **Successful user moment** — a support lead types `@agentsfleet what's our prod called?` in `#support` thread A, tells it "aurora", and *days later in a different thread* asks `@agentsfleet is aurora healthy?` — and it answers using "aurora" it learned in thread A. The bot lives in the channel and remembers the channel.
2. **Preserved user behaviour** — every existing trigger (webhook/cron/steer), the dashboard chat, the runner lease/report path, and the memory-continuity loop keep working unchanged. The Slack mention is one more producer into the single ingress, not a new runtime.
3. **Optimal-way check** — the unconstrained-optimal is "channel-scoped memory keyed by channel." The direct shape is exactly that: one resident fleet per `(team_id, channel_id)` carrying the channel's memory namespace. No gap.
4. **Rebuild-vs-iterate** — iterate. A refactor of the memory layer to add a channel-keyed store would trade away the proven `fleet_id`-scoped determinism for nothing; the resident-fleet-as-namespace reuses it intact. **Verdict: patch (additive), not refactor.**
5. **What we build** — Slack OAuth install (vault handle + generic `connector_installs` row); one signed events ingress; per-channel resident-fleet materialization *via the existing create API + a default skill.md* + generic `connector_channels` binding; mention→producer-XADD routing; in-thread answer; the locked reactive policy; the dashboard Connect-Slack connector; operator playbooks + architecture-doc updates.
6. **What we do NOT build** — hired durable teammates from Slack; source webhooks (Zoho/Statuspage); write actions; approval gating + Slack-user→`approval:resolve` allowlist; interactivity buttons / "Make it permanent"; slash commands; DMs / on-call (`im:write`). All deferred to the Rung-1 follow-on.
7. **Fit** — compounds with memory continuity + the connector/vault + single-ingress models; must not destabilize the lease/report path (the resident fleet leases like any other).
8. **Surface order** — API-first (the ingress is the product); the dashboard connector ships alongside because install is the precondition; CLI/slash deferred.
9. **Dashboard restraint** — only "Slack connected: {team}" + the channel roster; no per-channel controls or "teammates hired" counters until Rung 1.
10. **Confused-user next step** — when the bot lacks system access it says so in-thread and nudges "hire a teammate that can" (text at Rung 0; button at Rung 1). No ticket.

---

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — universal. Specific IDs this diff trips: **CTM** + **CTC** (constant-time, non-short-circuiting Slack signature compare), **VLT** (bot token in the vault handle, never in an entity table), **STS** + **NSQ** (no static strings / schema-qualified named constants in the two migrations), **UFS** (Slack scope strings, ingress path constants, `slack:bot` key-name, `slack` provider constant, `slack:` actor prefix, `UZ-SLK-*` codes — all named constants), **CFG** (Slack added as a config-driven connector descriptor against the *generic* `connector_installs`/`connector_channels` tables, not a per-integration table), **PRI** (Slack mention text is untrusted user input flowing into fleet reasoning; the reactive guarantee is code-set, never prompt-set), **TGU** (ingress result as a tagged union), **OBS** (every ingress rejection + materialization emits a log/event), **MIG** + **ORP** (migration-index assertions; orphan sweep on the new symbols), **NDC** + **NLR** (no dead code; flip the placeholder Slack catalogue card, don't leave both).
- **`docs/SCHEMA_CONVENTIONS.md`** — the two new migrations (`029`, `030`) + `schema/embed.zig` + migration-array update.
- **`dispatch/write_zig.md`** — all ingress/handler code is `*.zig` (pg-drain lifecycle, tagged-union results, multi-step `errdefer`, cross-compile both linux targets).
- **`docs/REST_API_DESIGN_GUIDELINES.md`** — the `/v1/connectors/slack/*` routes (single platform connector namespace; GitHub already lives at `/v1/connectors/github/*`).
- **`dispatch/write_ts_adhere_bun.md`** — the dashboard Connect-Slack connector (Next.js/TS).
- **`docs/AUTH.md`** — read before touching the ingress: the signature-only auth surface (no Bearer fallback) mirrors the webhook plane, and the **install-delegated authority** (the admin's one-time OAuth install is the standing consent under which the events worker creates a fleet) is the only principal in the inbound flow.

---

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes — ingress/handlers are Zig | cross-compile `x86_64-linux` + `aarch64-linux`; tagged-union ingress result; `errdefer` on the materialization path; `conn.query().drain()` before `deinit()`. |
| PUB / Struct-Shape | yes — new ingress + connector pub surface | shape verdict per new pub fn; keep the ingress handler surface minimal (one entry per verb). |
| File & Function Length (≤350/≤50/≤70) | yes | split the ingress into signature-verify / resolve / route helpers; channel-fleet resolve-or-create in its own file. |
| UFS | yes | Slack scopes, paths, `slack:bot`, `slack` provider, `slack:` actor prefix, `UZ-SLK-*` as named constants shared verbatim cross-runtime. |
| UI Substitution / DESIGN TOKEN | yes — dashboard connector | design-system primitives + `theme.css` tokens; no raw HTML, no arbitrary `*-[…]`. |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | yes | logfmt scopes per RULE OBS; `UZ-SLK-0xx` registered in the error registry mirroring `UZ-WH-0xx`; SCHEMA guard on `029`/`030` + `embed.zig`. |

---

## Credential Manifest

Every secret the end-to-end flow needs, with who writes it and when. Platform secrets are a **one-time human step** (the registration playbook); the per-install token is written **by the agent at runtime** on Connect-Slack. No secret is ever passed by argument/env — the vault is the interface. Implementation must **fail loud** listing any platform secret missing before serving the ingress.

| Secret | Scope | Vault location | Written by | When | Consumed by |
|--------|-------|----------------|------------|------|-------------|
| `slack-app` `client_id` | platform (one app, all tenants) | `op://<env-vault>/slack-app/client_id` | operator (human) | once, registration | OAuth authorize-URL build (dashboard) |
| `slack-app` `client_secret` | platform | `op://<env-vault>/slack-app/client_secret` | operator (human) | once, registration | `oauth.zig` code-exchange |
| `slack-app` `signing_secret` | platform | `op://<env-vault>/slack-app/signing_secret` | operator (human) | once, registration | `slack_sig.zig` ingress verify |
| bot token `xoxb` + `bot_user_id` + `scopes` | per-install (per workspace) | `(workspace_id, 'slack')` vault handle | `oauth.zig` (agent, runtime) | each Connect-Slack | `post.zig` `chat.postMessage`; thread re-read |
| GitHub App private key | platform (sibling connector) | `op://<env-vault>/github-app/private_key` | operator (human) | once, registration | `integration_github.zig` (documented by the GitHub playbook; not M106 runtime) |

**Sequencing (human → agent → activation):** (1) operator registers the Slack app + vaults the three platform secrets via `slack_app_registration/001_playbook.md`; (2) admin clicks **Connect Slack** in the dashboard → `oauth.zig` writes the per-install handle; (3) `@agentsfleet` answers on mention. Steps 2–3 are agent-side and idempotent; step 1 is the only human prerequisite.

---

## Overview

**Goal (testable):** a signed Slack `app_mention` POSTed to `/v1/connectors/slack/events` resolves `team_id → workspace_id` (via `connector_installs`) and `(team_id, channel_id) → fleet_id` (via `connector_channels`); on a binding miss it materializes a per-channel resident fleet by **calling the existing fleet-create path** with the default channel-bot `skill.md`; it lands a `slack:<user>` event on `fleet:{channel_fleet_id}:events` via the webhook-producer XADD shape; and the fleet's answer — hydrated from and captured back to that channel's `memory.memory_entries` namespace — is posted to the originating Slack thread. A second mention in a *different thread of the same channel* recalls memory written by the first.

**Problem:** agentsfleet's only human front doors are the CLI, the dashboard chat, and per-fleet webhooks. Support and ops people live in Slack, never open the dashboard, and never author markdown — so the durable runtime never reaches them. There is no surface where the product is useful with zero setup.

**Solution summary:** ship a first-party multi-tenant `@agentsfleet` Slack app. Install is one OAuth click in the dashboard (`team_id → workspace`, persisted as a vault handle + a generic `connector_installs` row). Each channel the bot is mentioned in materializes a **durable, reactive resident fleet** — a normal `core.fleets` row created through the existing `innerCreateFleet` path, seeded with a default channel-bot `skill.md` and a code-set reactive config — that owns that channel's memory namespace. A mention is an event XADDed via the webhook-producer shape; the answer rides the existing lease→execute→report→memory loop and posts back in-thread. The bot is the acquisition on-ramp; it is reactive (answers, never acts), which is the boundary to the Rung-1 hired teammate — agency, not memory, is the paywall.

---

## Prior-Art / Reference Implementations

- **API/ingress + producer** → `webhook_hmac.zig`/`webhook_sig.zig` (signature verify + `UZ-WH-0xx` taxonomy) and `webhooks/fleet.zig` (no-principal `XADD` + free-form actor + dedup) — the Slack ingress is a sibling producer. Divergence: Slack uses one app-level signing secret + `(team,channel)→fleet` resolution, vs the per-fleet URL model.
- **OAuth connector + storage** → `integration_github.zig` + `connectors/github/callback.zig:86` (vault JSON handle keyed by `workspace_id`, **zero tables**) — mirror for the Slack token + install metadata. The generic `connector_installs` row is the **only** addition, for the inbound `team_id → workspace_id` lookup the vault can't serve.
- **Fleet creation** → `fleets/create.zig:275` (`innerCreateFleet`) — the sole `core.fleets` insert path; reused under install-delegated authority with the default skill.md.
- **Schema / Memory** → nearest migrations `021`–`028` + `docs/SCHEMA_CONVENTIONS.md`; `runner_fleet.md` §Memory continuity reused unchanged (the only new thing is the routing key).
- **UI** → the existing GitHub connector card in `ui/.../integrations/catalog.ts` (flip Slack from `vault_secret` placeholder to OAuth) + design-system primitives.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `schema/030_core_connector_installs.sql` | CREATE | **generic** `(provider, external_account_id) → workspace_id` inbound-routing index; UNIQUE `(provider, external_account_id)`. Slack: `external_account_id = team_id`. Token + metadata live in the vault handle, NOT here. (Renumbered 029→030 on the M103 merge — M103 took v29.) |
| `schema/031_core_connector_channels.sql` | CREATE | **generic** `(provider, external_account_id, external_channel_id) → fleet_id` binding; UNIQUE `(provider, external_account_id, external_channel_id)`. Slack: `(slack, team_id, channel_id)`. (Renumbered 030→031 on the M103 merge.) |
| `schema/embed.zig` + `src/agentsfleetd/cmd/common.zig` | EDIT | `@embedFile` `029`/`030` in `embed.zig`; register both in the `canonicalMigrations()` array in `src/cmd/common.zig` (RULE MIG). |
| `src/agentsfleetd/types/id_format.zig` | EDIT | add UUIDv7 generators `generateConnectorInstallId()` + `generateConnectorChannelId()` (SCHEMA_CONVENTIONS uid format). |
| `src/agentsfleetd/errors/error_registry.zig` + `error_entries.zig` | EDIT | register `UZ-SLK-010/011/020/021/022/030` (comptime-validated), mirroring `UZ-WH-0xx`. |
| `src/agentsfleetd/http/handlers/connectors/slack/callback.zig` | CREATE | OAuth callback: state-verify, code-exchange, vault the bot token + install metadata as a `(workspace_id,"slack")` handle (mirrors `github/callback.zig`), insert the `connector_installs` reverse-lookup row. |
| `src/agentsfleetd/http/handlers/connectors/slack/events.zig` | CREATE | signed `app_mention` ingress: verify → handshake → 3 s ack → resolve install/channel → XADD via the webhook-producer shape (`actor=slack:<user>`, no principal). |
| `src/agentsfleetd/http/handlers/connectors/slack/channel_fleet.zig` | CREATE | resolve `(slack, team, channel)` → fleet via `connector_channels`; on miss **call the shared `innerCreateFleet` path** with the default channel-bot skill.md + reactive config, then upsert the binding. Concurrent first-mentions converge via UNIQUE + ON CONFLICT. **Never inserts `core.fleets` directly.** |
| `src/agentsfleetd/http/handlers/connectors/slack/channel_bot_skill.md` | CREATE | the default channel-bot `skill.md`, embedded via `@embedFile`, seeded as `source_markdown` for every resident channel fleet. |
| `src/agentsfleetd/http/handlers/connectors/slack/post.zig` | CREATE | post the fleet's answer back in-thread via `chat.postMessage`. |
| `src/agentsfleetd/auth/middleware/slack_sig.zig` | CREATE | Slack v0 signature middleware (reuses the constant-time compare). |
| `src/agentsfleetd/http/handlers/connectors/state.zig` | CREATE | shared connector OAuth install-state (signed single-use, HMAC + Redis nonce), parameterized by a per-connector `Config` (domain/nonce prefix). Extracted from GitHub's `state.zig` per Indy ("do C"). |
| `src/agentsfleetd/http/handlers/connectors/github/state.zig` | EDIT | collapse to a thin wrapper binding GitHub's `Config` (`ghconnect:v1:`) to the shared module — behavior-preserving; `callback`/`connect` unchanged. |
| `src/agentsfleetd/http/handlers/connectors/oauth2.zig` | CREATE | shared OAuth-2.0 connector mechanism (`Spec`-parameterized authorize/exchange/state + on-demand `loadAppCreds` from admin vault `<provider>-app`). Slack is its first user; Zoho/Jira/Linear reuse it as a `Spec`. |
| `src/agentsfleetd/http/handlers/connectors/slack/spec.zig` | CREATE | Slack connector descriptor (`Spec`: authorize/token endpoints, scopes, `slackconnect:v1:` state domain). |
| `src/agentsfleetd/http/handlers/connectors/slack/connect.zig` | CREATE | authed connect (`POST …/connectors/slack/connect`): mint state + build authorize URL via the shared mechanism. |
| `src/agentsfleetd/http/{routes,router,route_matchers,route_table,route_table_invoke_connectors,route_scopes}.zig` | EDIT | register `/v1/connectors/slack/callback` (public, state-authed) + `/v1/workspaces/{ws}/connectors/slack/connect` (authed, `connector:write`); events ingress added in §2. Mirrors the GitHub connector wiring. |
| `src/agentsfleetd/cmd/serve.zig` + `src/agentsfleetd/http/handlers/common.zig` | EDIT | `Context.platform_admin_workspace_id` (generic admin-workspace vault namespace for `<provider>-app` connector secrets) + boot wiring. |
| `src/lib/common/constants.zig` | EDIT | Slack scopes, paths, `slack` provider, `slack:bot`, `slack:` actor prefix, thread re-read bound (UFS). |
| `ui/packages/app/lib/integrations/catalog.ts` | EDIT | flip Slack card to OAuth connector. |
| `ui/packages/app/app/(dashboard)/integrations/components/IntegrationsConnectors.tsx` (+ extract a `SlackConnectorRow` file if length-capped) + `connector-actions.ts` | EDIT | `SlackConnectorRow` mirroring `GithubConnectorRow` + `startSlackConnectAction`; connected-state "Slack connected: {team}". |
| `playbooks/operations/slack_app_registration/001_playbook.md` | CREATE | operator runbook: register the Slack app, set URLs/scopes (incl. `channels:history` for thread re-read), vault the platform secrets. |
| `playbooks/operations/github_app_registration/001_playbook.md` | CREATE | operator runbook: register the GitHub App, vault the App private key (documents the existing pattern). |
| `docs/architecture/{high_level,user_flow,data_flow,direction,roadmap}.md` | EDIT | introduce the Slack-resident surface (forward-marked). |
| `docs/architecture/scenarios/slack-channel-resident.md` | CREATE | end-to-end scenario: `#support` thread→thread memory. |

---

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** six value slices — install, ingress, resident fleet, memory round-trip, dashboard, docs — each independently testable. The resident fleet is the keystone, and it is *not* a new creation mechanism: it is the existing `innerCreateFleet` called with a default skill.md under install-delegated authority. Everything else is plumbing around the existing memory loop.
- **Alternatives considered:** (a) *per-thread fleet* — rejected: forgets across threads, kills "learns the channel." (b) *per-workspace fleet* — rejected: bleeds `#support` memory into `#random` (and the store has no workspace key — memory is `fleet_id`-scoped). (c) *a new channel-keyed memory store* — rejected: reinvents `memory.memory_entries`. (d) *a server-side autonomous fleet creator (`resident.zig`)* — **rejected as originally specced**: no such actor exists; fleets are created only through `innerCreateFleet`. (e) *per-integration tables (`slack_installations`/`slack_channel_bindings`)* — **rejected**: GitHub proves the connector pattern is a vault handle + zero tables; the only genuine gap is inbound routing, served by ONE generic pair of tables, not a table per integration.
- **Patch-vs-refactor verdict:** **patch (additive)** — one new producer + one routing layer + reused create path + reused memory loop. The Rung-1 hired-teammate surface is the named follow-up, not silently mud-patched in here.

---

## Sections (implementation slices)

### §1 — Slack OAuth install + vault handle + `connector_installs`

Dashboard Connect-Slack runs the OAuth code-exchange. Persist the token + install metadata as a `(workspace_id, "slack")` **vault handle** (mirrors `github/callback.zig`), and insert a generic `connector_installs(provider='slack', external_account_id=team_id → workspace_id)` row so inbound events can resolve `team_id → workspace_id`. **Implementation default:** sign the OAuth `state` with the platform key (carry `workspace_id` + a Cross-Site Request Forgery nonce) so the callback can't be forged — mirrors the GitHub connector's `state.verifyConsume`.

- **Dimension 1.1** — ✅ DONE — OAuth callback exchanges `code`, inserts one `connector_installs` row, vaults `xoxb` + metadata under the `(workspace_id,"slack")` handle → Test `test_slack_oauth_persists_install_and_vaults_token` (implemented as `oauth_callback_integration_test.zig`; green against live PG+Redis)
- **Dimension 1.2** — ✅ DONE — forged/expired `state` is rejected, no row + no handle written → Test in `oauth_callback_integration_test.zig` ("rejects a forged state (Dim 1.2)"): a minted state with one tampered byte → **generic** `ERR_CONNECTOR_STATE_INVALID` (UZ-CONN-002, 400), zero `connector_installs` rows, zero `vault.secrets` handle. Asserts the generic code, NOT `UZ-SLK-021` (green against live PG+Redis).

### §2 — Signed events ingress

`POST /v1/connectors/slack/events`: verify Slack v0 signature (constant-time), echo `url_verification` challenge, then **inline** resolve `team_id → workspace` (`connector_installs`) + `(team, channel) → fleet` (`connector_channels`, materializing on a miss) and XADD the `slack:<user>` event — all within the 3 s ack budget. **There is no deferred-task substrate**: like `webhooks/fleet.zig`, the handler does its work inline (fast lookups + at most one `insertFleetOnConn` INSERT + one XADD) and returns. The **answer** is the only asynchronous part — the runner leases the fleet and replies later via `chat.postMessage`; Slack never waits on it.

- **Dimension 2.1** — ✅ DONE — valid signed `app_mention` resolves + XADDs inline and returns ≤3 s (one stream entry written before the response) → Test in `events_integration_test.zig` ("signed app_mention acks + enqueues; second mention reuses the fleet") — folds Dim 3.1's reuse assertion (green against live PG+Redis).
- **Dimension 2.2** — ✅ DONE — bad signature → `UZ-SLK-010`; stale timestamp (>300 s) → `UZ-SLK-011`; unknown team → `UZ-SLK-020` (200-ack no-op). **Tier reconciled:** `_sig_invalid`/`_sig_stale` are pure unit tests on `slack_sig.verifyAt` (+ a bad-signature *end-to-end* integration test proving the route rejects 401); `_team_unmapped` is an integration test (needs the DB miss) asserting a 200-ack + zero bindings.
- **Dimension 2.3** — ✅ DONE — `url_verification` returns the `challenge` verbatim → pure-unit parser test (`event_parse.parseSlackEvent`) + an integration handshake test through the signed endpoint.

### §3 — Per-channel resident fleet (materialized via the create API)

First mention with no `connector_channels` binding calls the **shared insert helper** (`insertFleetOnConn`, the request-independent core that `innerCreateFleet` wraps) under the **install-delegated workspace authority** (the admin's one-time OAuth install is the standing consent — there is no human in the inbound flow), seeded with the **default channel-bot `skill.md`** as `source_markdown` and a **code-constructed reactive config** (one `api` trigger, read-only `tools: []`, a modest code-set budget — built in code, *not parsed from the skill.md prose*), then upserts `connector_channels(slack, team_id, channel_id → fleet_id)`. Concurrent first-mentions converge on one fleet. The events worker never inserts `core.fleets` directly.

> **§3 materialization core landed with §2** (Jul 01, 2026). The §2 signed-events e2e cannot XADD without a resident fleet to target, so `connectors/slack/channel_fleet.zig` (resolve-or-create via the shared `insertFleetOnConn`), the embedded `channel_bot_skill.md`, the code-built reactive config, and the Invariant-2 assertion all shipped in this step. `create.zig`'s `insertFleetOnConn`/`deleteFleetRow`/`isUniqueViolation` were made `pub` for the reuse (Invariant 7 — no new fleet-insert site). **Divergence from the spec's concurrency note:** convergence is on the **per-workspace fleet-name unique constraint** (`slack-channel-<channel>`), not the `connector_channels` ON CONFLICT alone — a same-channel first-mention collides on the fleet name (23505) and the loser converges via `resolveExistingByName`; the binding ON CONFLICT DO NOTHING is the belt-and-suspenders. Materialized fleets are born `installing` (the `insertFleetOnConn` contract); **activation + the answer round-trip is §4** (not wired yet).

- **Dimension 3.1** — ✅ DONE — first mention creates exactly one resident fleet + binding; subsequent mentions reuse the same `fleet_id` → the reuse assertion is folded into the §2.1 e2e (`events_integration_test.zig`: mention #2 resolves to the same `fleet_id`, still one binding, two stream entries).
- **Dimension 3.2** — ✅ DONE — two `std.atomic`-gate-released concurrent first-mentions to the same channel converge on **exactly one** `core.fleets` row + one `connector_channels` binding (name-unique 23505 → `resolveExistingByName`; binding ON CONFLICT DO NOTHING) → Test in `events_integration_test.zig` ("two concurrent first-mentions converge on exactly one fleet + binding (Dim 3.2)"): a barrier releases both `std.Thread` firings together to bias toward the 23505 convergence path (the invariant holds under any interleaving); asserts both mentions 200-ack + one fleet + one binding (green ×3 against live PG+Redis, non-flaky).
- **Dimension 3.3** — ✅ DONE — the resident fleet's config admits no write tool / trigger / cron regardless of skill.md or mention input → Test in `channel_fleet.zig` (`assertReactiveReadonly`): the code-built reactive trigger parses to exactly one `api` trigger + zero tools, and the guard rejects an injected `webhook` trigger or a `git` tool.

**Default channel-bot `skill.md` (embedded via `@embedFile`).** Seeded as `source_markdown` (prose + frontmatter `name`) for every resident fleet. The `{channel_id}` placeholder is substituted by the materialization helper so `SkillMetadata.name == FleetConfig.name` holds. The reactive config (one `api` trigger, `tools: []`, code-set budget) is **constructed in code and asserted** (Invariant 2) — the skill.md frontmatter is *not* the source of capability; exact frontmatter finalized in §3 against the `insertFleetOnConn`/`ParsedTrigger` contract:

```markdown
---
name: slack-channel-{channel_id}
description: Reactive read-only assistant resident in one Slack channel. Answers @mentions from what it has learned about this channel; never acts unattended.
version: 1.0.0
when_to_use: A member @mentions the bot in this channel with a question.
---
<!-- Reactive config (one `api` trigger, tools: [], budget) is built in code by the materialization helper and asserted (Invariant 2); this skill.md carries prose + name only. -->


You are @agentsfleet, a reactive assistant living in one Slack channel.

- Answer the mention using this channel's memory plus the recent thread messages provided as input.
- You are read-only: you hold no system-access tools and never act unattended. If a request needs an action you cannot take, say so plainly and suggest hiring a teammate that can.
- Capture durable facts about this channel to memory so you recall them in later threads. When the latest in-thread statement contradicts older memory, treat the fresh statement as authoritative and update memory.
- Keep replies short and Slack-native; reply in the thread you were mentioned in.
```

### §4 — Channel memory + answer round-trip (the flagged mechanism)

A mention is an event on the channel fleet; the run hydrates and captures the channel's memory via the **existing** `/v1/runners/me/memory/{channel_fleet_id}` loop; the answer posts back in-thread using **`thread_ts = event.thread_ts orelse event.ts`** — a top-level mention (no `thread_ts`) anchors a new thread on its own `event.ts`, so the reply is always threaded, never a detached channel message. **The thread is a delivery surface, not a memory boundary — the resident fleet owns the durable namespace, so memory crosses threads because they share `channel_fleet_id`.** On **every** mention the worker live-fetches the recent thread (bounded last-N, a named const) into `recent_thread_msgs[]`: the bot is mention-only and blind to intervening non-mention messages, so same-thread continuity *requires* this re-read. Thread context is transient — passed as input, never written to `memory.memory_entries`. **Conflict rule:** when an in-thread statement contradicts durable memory, the freshest in-thread value wins for the current answer AND the run re-captures the correction to memory (so a later cross-thread recall isn't stale).

- **Dimension 4.1** — mention routes to a `slack:<user>` event on `fleet:{channel_fleet_id}:events`; the answer posts to the originating `thread_ts` → Test `test_mention_steers_channel_fleet_and_replies_in_thread`
- **Dimension 4.2** — a fact stored during thread A's run is recalled in a thread-B run of the same channel (cross-thread persistence) → Test `test_channel_memory_persists_across_threads`
- **Dimension 4.3** — recent thread messages are passed as transient input but never written to `memory.memory_entries` → Test `test_thread_context_is_transient_not_stored`
- **Dimension 4.4** — a same-thread statement that contradicts durable memory → the answer uses the fresh value AND memory is updated → Test `test_thread_correction_overrides_and_recaptures`

### §5 — Dashboard Connect-Slack connector

Flip the Slack catalogue card from paste-token to an OAuth connector; render connected state. Requires `connector:write`.

- **Dimension 5.1** — Connect-Slack launches the OAuth flow and renders "Slack connected: {team}" on return → Test `test_dashboard_slack_connect_flow`

### §6 — Operator playbooks + architecture-doc updates

Write the two registration playbooks and update the architecture docs (forward-marked). The full step-by-step lives in the playbook files; the spec points at them (anti-pseudocode).

- **Dimension 6.1** — `slack_app_registration` playbook covers app create, scopes (incl. `channels:history` for thread re-read), the three Request URLs, OAuth redirect, and vaulting `client_id`/`client_secret`/`signing_secret` as platform secrets → Test `test_playbook_slack_registration_present` (doc-presence + required-anchor check)
- **Dimension 6.2** — `github_app_registration` playbook documents the existing GitHub App registration + private-key vaulting → Test `test_playbook_github_registration_present`
- **Dimension 6.3** — `high_level`/`user_flow`/`data_flow`/`direction`/`roadmap` updated + the new scenario added, all marking the surface forward-looking (not "runs now") → Test `test_arch_docs_reference_slack_resident`

---

## Metrics & Observability

Per RULE OBS, every ingress rejection + materialization emits a structured log/event; message text and secrets are never logged (RULE PRI/VLT). Log scopes: `connector_slack` (OAuth connect/callback), the Slack ingress + materialization (§2/§3), and the outbound post (§4).

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| `slack.connected` | ops | callback vaults the bot token + inserts the install | `workspace_id`, `team_id` | no `bot_token` / `code` | `test_slack_oauth_persists_install_and_vaults_token` |
| `slack.oauth_exchange_failed` | ops | token exchange returns non-OK (`UZ-SLK-022`) | `workspace_id` | no `code` / `client_secret` | callback error path (unit: `parseSlackToken` rejects `ok:false`) |
| `slack.ingress_rejected` | ops | signature / stale / unmapped rejection (`UZ-SLK-010/011/020`) | `reason` | no request body | `test_slack_sig_invalid` / `test_slack_sig_stale` / `test_slack_team_unmapped` |
| `slack.mention_enqueued` | ops | a valid mention is XADDed to the channel fleet | `channel_fleet_id` | no mention text | `test_slack_events_acks_fast_and_enqueues` |
| `slack.channel_fleet_materialized` | ops | first mention creates a resident fleet | `team_id`, `channel_id`, `fleet_id` | no mention text | `test_resident_fleet_materialized_once` |
| `slack.connect_initiated` | product | admin clicks Connect Slack in the dashboard | `workspace_id` | none | `test_dashboard_slack_connect_flow` |

**Metrics review:** the dashboard Connect-Slack action is the one net-new product funnel step; no analytics/funnel playbook update is required beyond the connect event — Rung 0 is reactive/read-only, so there are no per-message product events until Rung 1.

---

## Interfaces

```
GET /v1/connectors/slack/callback?code=&state=    (signature: none; state-signed; browser redirect)
  → 302 to dashboard "Slack connected" on success      (Slack redirects the browser here via GET, mirroring github/callback.zig)
  → UZ-SLK-021 invalid_state | UZ-SLK-022 oauth_exchange_failed

POST /v1/connectors/slack/events                         (auth: Slack v0 signature ONLY)
  headers: X-Slack-Signature: v0=<hmac>, X-Slack-Request-Timestamp
  body (url_verification): { type, challenge }   → 200 { challenge }
  body (event_callback):   { team_id, event:{ type:"app_mention", channel, user, text, ts, thread_ts? } }
                           → 200 (empty, ≤3s); work proceeds async
  → UZ-SLK-010 invalid_signature | UZ-SLK-011 stale_timestamp | UZ-SLK-020 team_not_installed

core.connector_installs:  provider, external_account_id (=team_id) → workspace_id (FK), installed_by, scopes[], created_at
                          UNIQUE (provider, external_account_id)
                          — token + metadata NOT here; vault handle (workspace_id,'slack') (RULE VLT)
core.connector_channels:  provider, external_account_id (=team_id), external_channel_id (=channel_id) → fleet_id (FK), kind='resident', created_at
                          UNIQUE (provider, external_account_id, external_channel_id)

producer (reused, webhooks/fleet.zig shape — signature-authed, NO principal):
  XADD fleet:{channel_fleet_id}:events  actor=slack:<user_id>  type=chat
  request={ text, reply_thread_ts (= event.thread_ts orelse event.ts), channel_id, recent_thread_msgs[] }
fleet creation (reused): insertFleetOnConn(conn, workspace_id, source_markdown=DEFAULT_CHANNEL_BOT_SKILL, trigger_markdown=<code-built reactive config>, …)  — innerCreateFleet's request-independent core
memory (reused, unchanged): GET/POST /v1/runners/me/memory/{channel_fleet_id}   (scope column fleet_id = the channel's resident fleet)
```

---

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Invalid signature | wrong/missing `X-Slack-Signature` | `UZ-SLK-010`, 401, no enqueue; logged. |
| Stale timestamp / replay | `\|now−ts\|` > 300 s | `UZ-SLK-011`, 401; replay window closed. |
| Team not installed | `team_id` absent from `connector_installs` | `UZ-SLK-020`, 200-ack + no-op (Slack must not see an error loop); logged. |
| Slow handler | the inline path (lookups + ≤1 INSERT + XADD) must stay < 3 s | bounded by design — the model run is the runner's async job, not the handler's, so Slack never sees it; if a future inline step risks the budget it must move behind the XADD, not block the ack. |
| Duplicate delivery | Slack at-least-once retry (same `event.ts`) | dedup on `(channel_fleet_id, event.ts)` via `SET NX`; one event enqueued. |
| Concurrent first-mention | two mentions, no binding yet | UNIQUE `(provider, team_id, channel_id)` on `connector_channels` + ON CONFLICT → one resident fleet. |
| Forged OAuth callback | tampered `state` | `UZ-SLK-021`, no install row, no vault handle. |
| Prompt injection | mention text or skill.md tries to escalate (RULE PRI) | text/skill.md are advisory reasoning input only; tools/triggers/secrets stay code-enforced by the reactive config + tool bridge — no prose can grant capability. |
| Outbound post fails | bot lacks `chat:write`, or `chat.postMessage` 429 | logged `UZ-SLK-030`; honor `Retry-After` + bounded retry; never crashes. |

---

## Invariants

1. **Memory scope = channel = audience boundary** — the memory scope column `fleet_id` is the resident channel fleet (`channel_fleet_id`), server-derived from the `connector_channels` binding; that table has UNIQUE `(provider, team_id, channel_id)`. One channel ⇒ one namespace; never per-thread, per-user, or per-workspace. Enforced by the UNIQUE constraint + server-side derivation (no client-supplied scope).
2. **Resident fleet is reactive** — concretely, the created `FleetConfig` (`fleet_runtime/config_types.zig`) carries **exactly one `api` trigger** (parameterless — the fleet is woken only when an event is XADDed to its stream, never by a `webhook`/`cron` autonomous trigger; an empty `triggers` slice is *rejected* by the config parser, so the reactive shape is the lone `api` trigger, not the *absence* of a trigger) and `tools` ⊆ a read-only allow-list. The reactive policy is **constructed in code**, not parsed from skill.md prose; the materialization helper **asserts post-build** that no `webhook`/`cron` trigger and no write-tool slipped in (a prompt can be injection-overridden — RULE PRI). No code path grants a resident a `webhook`/`cron` trigger or a write-tool.
3. **Bot token never in an entity table** — `connector_installs` stores only `(provider, team_id, workspace_id, scopes)`; the token + metadata live in the `(workspace_id,'slack')` vault handle (RULE VLT). Enforced by schema (no token column) + `crypto_store`/`vault` resolution.
4. **Signature is constant-time + time-bounded** — non-short-circuiting compare (RULE CTC) over `v0:{ts}:{body}` + 300 s window. Enforced by reuse of `webhook_hmac` constant-time path.
5. **Signature is the only auth on `/v1/connectors/slack/events`** — `Authorization` is never consulted (mirrors the webhook plane); the only inbound authority is the install-delegated workspace resolved from `connector_installs`. Enforced by the middleware wiring (no Bearer branch on the route).
6. **One resident fleet per channel under concurrency** — UNIQUE + ON CONFLICT DO NOTHING on the `connector_channels` insert. Enforced by Postgres.
7. **Fleets are only ever inserted by the shared insert helper (`insertFleetOnConn`, which `innerCreateFleet` wraps)** — the Slack materialization reuses `insertFleetOnConn` directly (it is request-independent; `innerCreateFleet` is coupled to an `httpz.Request` + principal and is not callable from the principal-less events worker). The worker has no `INSERT INTO core.fleets` of its own. Enforced by code review + ORP sweep (zero new fleet-insert sites).
8. **No static strings / unscoped SQL in `029`/`030`** — RULE STS/NSQ. Enforced by the SCHEMA guard.

---

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | integration | `test_slack_oauth_persists_install_and_vaults_token` | valid `code` → one `connector_installs` row, token only in the `(ws,'slack')` vault handle. |
| 1.2 | unit | `test_slack_oauth_rejects_forged_state` | tampered `state` → `UZ-SLK-021`, zero rows, no handle. |
| 2.1 | e2e | `test_slack_events_acks_fast_and_enqueues` | signed `app_mention` → 200 ≤3 s + one stream entry. |
| 2.2 | unit | `test_slack_sig_invalid` / `test_slack_sig_stale` / `test_slack_team_unmapped` | each → its `UZ-SLK-0xx`, no enqueue (unmapped: 200-ack no-op). |
| 2.3 | unit | `test_slack_url_verification_handshake` | `{type:url_verification,challenge:X}` → `{challenge:X}`. |
| 3.1 | integration | `test_resident_fleet_materialized_once` | mention #1 creates fleet (via `innerCreateFleet`) + binding; #2 reuses same `fleet_id`. |
| 3.2 | integration | `test_resident_fleet_concurrent_first_mention` | two parallel first-mentions → exactly one fleet. |
| 3.3 | unit | `test_resident_policy_is_reactive_readonly` | resident config build → no write tool/trigger/cron, regardless of skill.md content. |
| 4.1 | e2e | `test_mention_steers_channel_fleet_and_replies_in_thread` | mention in thread A → event on channel fleet; reply posted to `thread_ts=A`. |
| 4.2 | integration | `test_channel_memory_persists_across_threads` | store key in thread-A run → recalled in thread-B run of same channel. |
| 4.3 | integration | `test_thread_context_is_transient_not_stored` | recent thread msgs passed as input; `memory.memory_entries` unchanged by them. |
| 4.4 | integration | `test_thread_correction_overrides_and_recaptures` | thread says "aurora-2" vs stored "aurora" → answer uses "aurora-2" AND memory updated. |
| 5.1 | e2e | `test_dashboard_slack_connect_flow` | Connect-Slack → OAuth → "Slack connected: {team}". |
| 6.1 | unit | `test_playbook_slack_registration_present` | playbook exists + has scope/URL/secret-vaulting anchors. |
| 6.2 | unit | `test_playbook_github_registration_present` | playbook exists + has private-key-vaulting anchor. |
| 6.3 | unit | `test_arch_docs_reference_slack_resident` | each named arch doc references the surface + marks it forward-looking. |

**Regression:** existing webhook/cron/steer ingress, memory continuity, and lease/report tests must stay green (the Slack producer is additive). **Idempotency/replay:** 2.2 (stale) + the dedup test (`(channel_fleet_id, event.ts)`) cover Slack at-least-once.

---

## Acceptance Criteria

- [ ] Signed `app_mention` → in-thread answer end-to-end — verify: `make test-integration` (slack e2e scenario)
- [ ] Cross-thread memory recall holds — verify: `make test-integration` (`test_channel_memory_persists_across_threads`)
- [ ] Same-thread correction overrides + re-captures — verify: `make test-integration` (`test_thread_correction_overrides_and_recaptures`)
- [ ] `make lint` clean · `make test` passes
- [ ] `make test-integration` passes (HTTP/schema/Redis touched)
- [ ] Cross-compile clean: `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux`
- [ ] `make check-pg-drain` clean (new queries) · `gitleaks detect` clean · no non-`.md` file over 350 lines
- [ ] `bash audits/spec-template.sh --staged` clean · SCHEMA guard clean on `029`/`030`/`embed.zig`
- [ ] Two playbooks present + five arch docs + scenario updated — verify: `git diff --name-only origin/main | grep -E 'playbooks/operations/(slack|github)_app_registration|docs/architecture'`

---

## Eval Commands (post-implementation)

```bash
# distinctive check (rest is in Acceptance Criteria):
make test-integration 2>&1 | grep test_channel_memory_persists_across_threads
```

---

## Dead Code Sweep

**1. Orphaned files.** N/A — no files deleted (the Slack catalogue card is edited in place, not removed; RULE NLR).

**2. Orphaned references.** After flipping the Slack `vault_secret` placeholder to the OAuth connector, `grep -rn SLACK_BOT_TOKEN ui/ src/` must show 0 stale paste-token uses. After materialization is wired, `grep -rn "INSERT INTO core.fleets" src/agentsfleetd/http/handlers/connectors/slack` must show 0 (the worker calls `innerCreateFleet`, never inserts).

---

## Discovery (consult log)

> Append as work surfaces consults, skill outcomes, and Indy-acked deferral quotes.

- **Code-grounded corrections (Jun 30, 2026, design review with Indy)** — three reuse claims in the first draft were wrong against the code and were corrected before any implementation:
  - *Actor:* the steer reuse pointed at `fleets/messages.zig`, which is principal-gated (`:67`, `:177`); the signature-only Slack plane has no principal. Corrected to the `webhooks/fleet.zig` producer shape (no principal, free-form actor `:104`).
  - *Fleet creation:* the first draft invented `integrations/slack/resident.zig` as an autonomous server-side fleet creator. No such actor exists — `innerCreateFleet` (`create.zig:275`) is the only fleet-insert path. Corrected to **call the existing create API with a default skill.md**, under install-delegated authority (Indy: "Option 1 is a no brainer, just spin a fleet via API, using a default skill.md").
  - *Storage:* the first draft added Slack-specific `slack_installations` + `slack_channel_bindings` tables. GitHub proves the connector pattern is a vault handle + zero tables (`github/callback.zig:86`). Corrected to a vault handle + **two generic** provider-keyed tables (`connector_installs`, `connector_channels`) — no per-integration table sprawl (Indy chose "Vault handle + 1 generic routing table").
- **Agent-chosen defaults (open to Indy veto)** — §4 same-thread behavior: live thread re-read (bounded last-N) is *required* for mention-only coherence; the conflict rule (freshest in-thread wins + re-capture to memory) is the chosen default.
- **Completeness pass (`/review`, Jun 30, 2026)** — four handoff-readiness gaps closed so a fresh agent doesn't stall: (1) §2 corrected — the ingress works **inline** (no deferred-task substrate exists; `webhooks/fleet.zig` is the model), only the answer is async; (2) the default channel-bot `skill.md` content is now specified verbatim in §3; (3) the reactive config is pinned to `FleetConfig` fields (then believed `triggers == &.{}`, read-only `tools`) in Invariant 2 — **superseded Jul 01** (empty `triggers` is unparseable; reactive = one `api` trigger — see the EXECUTE-start reconciliations below); (4) a `## Credential Manifest` enumerates every secret + vault location + human-vs-agent sequencing.
- **EXECUTE-start reconciliations (Jul 01, 2026 — code-grounded via 4 parallel verification agents + Indy)** — four spec pointers aligned to verified code before any implementation:
  - *Route/handler namespace:* adopted the single platform standard `/v1/connectors/slack/*` + `http/handlers/connectors/slack/`. GitHub already ships `/v1/connectors/github/*`, and its callback URL is registered in the live GitHub App — so the alternative (renaming GitHub to `integrations/`) is a breaking change outside M106 scope that collides with active M102. Indy: *"I think i want to follow 1 single convention … so ensure the standard is followed."* → standardize on `connectors/`.
  - *Reactive config:* Invariant 2 corrected — an empty `triggers` slice is *rejected* by the config parser, so "reactive" is one parameterless `api` trigger (woken by event XADD) + `tools: []` + a code-set budget, all built in code and asserted; not the *absence* of a trigger.
  - *Fleet insert:* `innerCreateFleet` is `httpz.Request`+principal-coupled and uncallable from the events worker; materialization reuses its request-independent core `insertFleetOnConn` directly (Invariant 7 reworded; single insert site preserved, grep-confirmed).
  - *Scope additions:* migration array also lives in `src/cmd/common.zig`; UUIDv7 gens in `src/types/id_format.zig`; `UZ-SLK-*` in `errors/error_registry.zig`+`error_entries.zig`; per-install vault key is `fleet:slack` via `credential_key.allocKeyName` (mirrors `fleet:github`).
- **Connector-state extraction (Jul 01, 2026 — Indy: "I wan you to do C" + "connectors/state.zig").** The signed single-use OAuth install-state (HMAC + Redis nonce) is now a shared, `Config`-parameterized module at `http/handlers/connectors/state.zig`; GitHub's `state.zig` (landed by M102) is collapsed to a thin wrapper binding its `ghconnect:v1:` domain, and Slack binds `slackconnect:v1:` — a per-connector domain prefix keeps one connector's state from cross-verifying as another's (new isolation test). **No M102 collision:** M102's connect surface (incl. `state.zig`) landed Jun 27 and is stable; M102's open work is the webhook ingress (`/v1/ingress/{provider}`), different files. This edits GitHub's shipped connector — outside M106's original Files-Changed — under Indy's explicit "do C".
- **§1 OAuth flow implemented (Jul 01, 2026).** Built on a shared, `Spec`-parameterized OAuth-2.0 mechanism (`connectors/oauth2.zig`: authorize URL + code exchange + state + on-demand `loadAppCreds` from admin vault `<provider>-app`), so Slack is the first of the OAuth-2.0 family — Zoho/Jira/Linear reuse it as a `Spec` + a vaulted secret (GitHub stays its App-installation flow, a different protocol). `connect.zig`/`callback.zig` are thin drivers wired through the 6-file routing (routes/router/route_matchers/route_table/route_table_invoke_connectors/route_scopes) mirroring GitHub. Admin-vault model kept per Indy (env is for bootstrap-into-platform secrets like Clerk; connector data-secrets live in the vault) via one generic `Context.platform_admin_workspace_id`. **Error taxonomy reconciled:** state-invalid reuses the generic `ERR_CONNECTOR_STATE_INVALID` (consistent with GitHub); only the Slack-specific exchange failure is new (`UZ-SLK-022`) — so Dim 1.2 asserts the generic code, not `UZ-SLK-021`. **Tests:** state crypto, oauth2 URL building, and Slack token-response parsing (`parseSlackToken`) are unit-tested + green. **Remaining in §1 (not a deferral — the next step):** the e2e integration test (Dim 1.1 — signed callback → `connector_installs` row + `fleet:slack` vault handle) needs `Spec.token_endpoint` made injectable + a loopback fake-Slack + the DB harness (no connector-integration-test precedent exists — M102's GitHub connector shipped without one).
- **§1 Dim 1.1 e2e test landed (Jul 01, 2026) — two §1 production bugs caught + fixed.** `slack/oauth_callback_integration_test.zig` drives the real `/v1/connectors/slack/callback` through `TestHarness` (live Postgres+Redis), with the code-exchange pointed at a loopback fake-Slack (`std.http.Server` over `test_port.listenLoopback`); it asserts exactly one `connector_installs` row + the `fleet:slack` vault handle carrying the bot token (proving RULE VLT — the token is not in the table). **Test-infra seams (additive, null in prod):** `Context.connector_oauth_token_endpoint_override` (test/dev override of `Spec.token_endpoint`, applied in `completeInstall`); a `redirectBehavior()` knob on the harness fluent `Request` (`.unhandled` returns the 302 as-is instead of the client chasing `Location` to a dead `app_url`). **Bug A — `oauth2.exchange` returned an empty body:** it read `resp_body.toOwnedSlice(alloc)` off the seed `ArrayList`, which goes stale once the Allocating writer grows its buffer via drain → every real code-exchange would have failed `UZ-SLK-022`. Fixed to read `aw.toOwnedSlice()` (matches the proven `test_http_message.zig` pattern); also closed an error-path buffer leak. **Bug B — redirect use-after-free:** `redirectToDashboard` built the `Location` URL on `hx.alloc` (the dispatcher's per-request arena, freed before httpz writes headers) → segfault on every successful install redirect. Fixed to `hx.res.arena` (the response-lifetime arena, per `server.zig::headerUint`). **The same use-after-free was fixed in `github/callback.zig`** (M102's shipped connector — identical `Location`-on-`hx.alloc` pattern, `:91`, never caught for want of a GitHub connector integration test). Indy acked the cross-scope edit: `> Indy (2026-07-01): "Fix it in this PR" — context: the identical redirect use-after-free in github/callback.zig.`
- **§2 signed events ingress + §3 materialization core implemented (Jul 01, 2026).** `POST /v1/connectors/slack/events` verifies the Slack v0 signature, echoes `url_verification`, resolves `team_id→workspace` + `(team,channel)→fleet` (materializing on miss), dedups on `(channel_fleet_id, event.ts)`, and XADDs a `slack:<user>` `chat` event via the webhook-producer shape. Green against live PG+Redis (Dim 1.2, 2.1/3.1, 2.2, 2.3). Decisions taken during EXECUTE:
  - *Signing-secret source:* resolved **per-request from the admin-vault `slack-app` entry's `signing_secret`** (`events.zig:loadSigningSecret`, mirroring `oauth2.loadAppCreds`) — honors the admin-vault decision (#3); no boot/env wiring, no new `Context` field. The events flow already holds a conn for the install/channel lookups, so it is one extra indexed vault read, not a hot-path cost.
  - *`slack_sig.zig` location:* the spec listed `auth/middleware/slack_sig.zig`, but that layer deliberately does **not** import `fleet_runtime/webhook_verify` (its portability boundary, per `webhook_sig.zig`'s own comment) and the Slack signature is verified **in-handler** (route `middlewares = none`, like `grant_approval_webhook`) because the secret is per-request-vaulted, not a boot secret. So the pure verifier lives at `connectors/slack/slack_sig.zig`, reusing `webhook_verify.SLACK` (header names / `v0` / 300 s) + the `hmac_sig` primitives (RULE UFS/NDC — no duplicate verify algorithm authored).
  - *`UZ-SLK-020` (team not installed):* a benign **200-ack no-op** (Slack must never see an error loop), so its registry entry carries `.ok` — the code is a structured telemetry/log reason + the `{"ignored":"UZ-SLK-020"}` body value, never an `hx.fail` wire status (mirrors the retired `UZ-WH-003` paused-webhook pattern).
  - *Constants:* `PROVIDER_SLACK` / `CONNECTOR_CHANNEL_KIND_RESIDENT` / `SLACK_ACTOR_PREFIX` added to `src/lib/common/constants.zig` (honoring the `029`/`030` schema comments' pointer); `spec.PROVIDER` de-duplicated to alias `PROVIDER_SLACK`. The Slack signature header/version/drift constants **already existed** in `error_registry.zig` (`SLACK_SIG_*`, added in §1) and are reused.
  - *`pg` drain gotcha (bit once):* two `SELECT count(*)` queries on the same conn need the first result **drained before** the second, or `error.ConnectionBusy` — a `SELECT count(*)` reads one row without hitting the `next()==null` that drains, and `defer q.deinit()` fires too late. Fix: scope each query in a block (or a helper that returns), so the auto-drain `deinit` fires between queries.
- **`/review` outcome (Jul 01, 2026 — 3 adversarial finder passes: correctness, Zig memory/lifetime/drain, conventions/reuse).** Memory/drain pass: **zero** bugs (double-free/errdefer/drain/borrowed-slice/stack-escape/dedup-release/cross-allocator all verified sound). Applied fixes: **(a)** resident fleet name now `slack-channel-<team>-<channel>` (was channel-only) — the binding key is `(team,channel)`, so the per-workspace-unique fleet name (the convergence key) must include the team, else two Slack teams mapped to one workspace could collide on a shared channel id and bleed memory; **(b)** a missing `X-Slack-Request-Timestamp` now returns `UZ-SLK-010` (unverifiable) not `UZ-SLK-011` (stale); **(c)** a signed-but-unparseable body 200-acks (not 4xx) — matching the "Slack never sees an error loop" invariant; **(d)** reuse cleanups — `oauth2.loadAppVaultJson` shared by `loadAppCreds` + the ingress `loadSigningSecret` (one `-app` key site), `std.ascii.allocLowerString` for the slug, the test signs via `slack_sig.CONFIG`.
- **Deferred review findings (out-of-scope / pre-existing — NOT M106 regressions).**
  - *`setNx` conflates a Redis server-error reply with "key exists" → a new mention could 200-ack as "duplicate" and be silently dropped when Redis is write-degraded (OOM-noeviction / read-only replica).* This is a **pre-existing** characteristic of `queue/redis_client.zig:setNx` (out of M106's Files-Changed), and the shipped **webhook** producer (`webhooks/fleet.zig`) has the identical behavior — the Slack ingress faithfully mirrors that named prior-art. Fixing it (make `setNx` surface `.err` as a Zig error so the caller 500s + Slack retries instead of silently acking) is a queue-layer change touching the webhook plane and wants its own focused PR + tests. **Surfaced to Indy** for prioritization; not bundled here.
  - *A concurrent first-mention whose race-winner's `ensureEventStream` fails (Redis blip) rolls back its own fleet row, so the race-loser's `resolveExistingByName` can find nothing → one `UZ-…` 500.* Extremely rare (concurrency **and** a Redis blip on the winner) and **self-healing** — Slack retries the 500 and the next attempt re-materializes cleanly. Accepted as-is over adding retry-loop complexity in §2.
- **§3 Dim 3.2 concurrent-convergence test landed (Jul 01, 2026).** Closes §3. `events_integration_test.zig` fires two barrier-gated `std.Thread` mentions at the same channel through the real ingress; a `std.atomic.Value(bool)` gate (Zig 0.16 dropped `ResetEvent.timedWait`; the atomic-bool gate is the house barrier idiom per `patch_concurrent_integration_test.zig`) releases both together to bias toward the fleet-name-unique (23505) convergence path. Asserts both mentions 200-ack + exactly one `core.fleets` row + one `connector_channels` binding. **The invariant holds under any interleaving** (a non-overlapping race just reads the binding), so the test is correct regardless of scheduling — green ×3 in a row, non-flaky. No production code changed (convergence shipped with §2/§3-core); this is the missing automated proof.
- **Skill chain** — `/write-unit-test`, `/review` (done — above), `/review-pr`, `kishore-babysit-prs` outcomes (filled during EXECUTE/CHORE(close)).
- **Deferrals** — Rung 1 (hired teammates, source webhooks, writes, approvals, buttons, slash, DMs) is **scoped out by design**, not deferred work; the follow-on milestone owns it. Any *other* "deferred to follow-up" needs an Indy-acked verbatim quote here.

---

## Skill-Driven Review Chain (mandatory)

| When | Skill | What it does | Required output |
|------|-------|--------------|-----------------|
| After implementation, before CHORE(close) | `/write-unit-test` | Audits diff coverage vs the Test Specification. | Clean; iteration count + coverage in Discovery. |
| After tests pass, before CHORE(close) | `/review` | Adversarial diff review vs spec, `docs/architecture/`, REST guide, `dispatch/write_zig.md`, Failure Modes, Invariants. | Clean OR every finding dispositioned. |
| After `gh pr create` | `/review-pr` | Review-comments the open PR. | Comments addressed before merge. |

---

## Verification Evidence

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Unit tests | `make test` | {paste} | |
| Integration + e2e | `make test-integration` | {paste} | |
| Lint | `make lint` | {paste} | |
| Cross-compile (Zig) | `zig build -Dtarget=x86_64-linux` | {paste} | |
| Gitleaks | `gitleaks detect` | {paste} | |

---

## Out of Scope

- **Rung 1 — hired durable teammates from Slack** (source webhooks, write actions, approval gating + Slack-user→`approval:resolve` allowlist, interactivity buttons / "Make it permanent", slash commands, DMs/on-call). The follow-on milestone; depends on this + M103 (templates) + M105 (schedules).
- **Reading whole-channel history** (`message.channels` firehose) — the bot learns from interaction, not surveillance; thread re-read on mention (`channels:history`, bounded) is the only history read. Out of scope by privacy + scope discipline.
- **CLI surface for the resident bot** — Slack-only at Rung 0.
