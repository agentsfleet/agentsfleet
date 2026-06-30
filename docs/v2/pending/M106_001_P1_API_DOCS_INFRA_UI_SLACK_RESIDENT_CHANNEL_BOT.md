# M106_001: `@agentsfleet` Slack-resident channel bot — reactive on-ramp

**Prototype:** v2.0.0
**Milestone:** M106
**Workstream:** 001
**Date:** Jun 30, 2026
**Status:** PENDING
**Priority:** P1 — first customer-facing surface that lives where users already work (Slack); the acquisition on-ramp to the durable hired teammate.
**Categories:** API, DOCS, INFRA, UI
**Batch:** B1 — standalone; reuses shipped substrate, blocks the Rung-1 follow-on.
**Branch:** {feat/m106-slack-resident — added when work begins}
**Depends on:** None — builds entirely on shipped substrate (M80 runner/memory continuity, M28 webhook-signature middleware, the GitHub OAuth connector pattern). Rung 1 (the hired-teammate follow-on) will depend on this + M103 (template catalogue) + M105 (schedules).
**Provenance:** agent-generated (pre-spec; brainstorm transcript Jun 30, 2026 — office-hours → sequence design → consumption-ladder refinement).

> **Provenance is load-bearing.** LLM-drafted; cross-check every reuse claim against the named files before EXECUTE — the value of this spec is that ~80% is reuse, so a wrong reuse pointer is the main risk.

**Canonical architecture:** `docs/architecture/runner_fleet.md` §Memory continuity + `docs/architecture/data_flow.md` §B (single-ingress trigger model). This milestone ADDS a Slack producer to that model and a per-channel resident fleet; it invents no new runtime primitive.

---

## Implementing agent — read these first

1. `docs/architecture/runner_fleet.md` §Memory continuity — the `GET`/`POST /v1/runners/me/memory/{fleet_id}` hydrate/capture loop the channel memory reuses **verbatim**; `instance_id = fleet_id`, durable in `memory.memory_entries`, `:memory:` SQLite in the child.
2. `src/agentsfleetd/credentials/integration_github.zig` + `src/agentsfleetd/http/handlers/connectors/github/` — the OAuth connector + platform-secret (`crypto_store.load` in the `agentsfleet-admin` vault) pattern the Slack app install mirrors.
3. `src/agentsfleetd/auth/middleware/webhook_hmac.zig` + `webhook_sig.zig` — constant-time signature verify and the `UZ-WH-0xx` error taxonomy the Slack v0 scheme mirrors.
4. `src/agentsfleetd/http/handlers/fleets/messages.zig` — the steer ingress (`XADD fleet:{id}:events`, `actor=steer:<user>`) the mention routing reuses with `actor=slack:<user>`.
5. `src/agentsfleetd/http/handlers/fleets/create.zig` + `create_stream.zig` — the create path the lazy channel-fleet materialization reuses (row + events stream + consumer group + session).

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
4. **Rebuild-vs-iterate** — iterate. A refactor of the memory layer to add a channel-keyed store would trade away the proven `instance_id=fleet_id` determinism for nothing; the resident-fleet-as-namespace reuses it intact. **Verdict: patch (additive), not refactor.**
5. **What we build** — Slack OAuth install + `slack_installations`; one signed events ingress; per-channel resident-fleet lazy materialization + `slack_channel_bindings`; mention→steer routing; in-thread answer; the locked reactive policy; the dashboard Connect-Slack connector; operator playbooks + architecture-doc updates.
6. **What we do NOT build** — hired durable teammates from Slack; source webhooks (Zoho/Statuspage); write actions; approval gating + Slack-user→`approval:resolve` allowlist; interactivity buttons / "Make it permanent"; slash commands; DMs / on-call (`im:write`). All deferred to the Rung-1 follow-on.
7. **Fit** — compounds with memory continuity + the connector/vault + single-ingress models; must not destabilize the lease/report path (the resident fleet leases like any other).
8. **Surface order** — API-first (the ingress is the product); the dashboard connector ships alongside because install is the precondition; CLI/slash deferred.
9. **Dashboard restraint** — only "Slack connected: {team}" + the channel roster; no per-channel controls or "teammates hired" counters until Rung 1.
10. **Confused-user next step** — when the bot lacks system access it says so in-thread and nudges "hire a teammate that can" (text at Rung 0; button at Rung 1). No ticket.

---

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — universal. Specific IDs this diff trips: **CTM** + **CTC** (constant-time, non-short-circuiting Slack signature compare), **VLT** (bot token in vault, never in `slack_installations`), **STS** + **NSQ** (no static strings / schema-qualified named constants in the two migrations), **UFS** (Slack scope strings, ingress path constants, `slack:bot` key-name, `slack:` actor prefix, `UZ-SLK-*` codes — all named constants), **CFG** (Slack added as a config-driven connector descriptor, not a new branch), **PRI** (Slack mention text is untrusted user input flowing into fleet reasoning), **TGU** (ingress result as a tagged union), **OBS** (every ingress rejection + materialization emits a log/event), **MIG** + **ORP** (migration-index assertions; orphan sweep on the new symbols), **NDC** + **NLR** (no dead code; flip the placeholder Slack catalogue card, don't leave both).
- **`docs/SCHEMA_CONVENTIONS.md`** — the two new migrations (`029`, `030`) + `schema/embed.zig` + migration-array update.
- **`dispatch/write_zig.md`** — all ingress/handler code is `*.zig` (pg-drain lifecycle, tagged-union results, multi-step `errdefer`, cross-compile both linux targets).
- **`docs/REST_API_DESIGN_GUIDELINES.md`** — the `/v1/integrations/slack/*` routes.
- **`dispatch/write_ts_adhere_bun.md`** — the dashboard Connect-Slack connector (Next.js/TS).
- **`docs/AUTH.md`** — read before touching the ingress: the signature-only auth surface (no Bearer fallback) mirrors the webhook plane.

---

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes — ingress/handlers are Zig | cross-compile `x86_64-linux` + `aarch64-linux`; tagged-union ingress result; `errdefer` on the materialization path; `conn.query().drain()` before `deinit()`. |
| PUB / Struct-Shape | yes — new ingress + connector pub surface | shape verdict per new pub fn; keep the ingress handler surface minimal (one entry per verb). |
| File & Function Length (≤350/≤50/≤70) | yes | split the ingress into signature-verify / resolve / route helpers; materialization in its own file. |
| UFS | yes | Slack scopes, paths, `slack:bot`, `slack:` actor prefix, `UZ-SLK-*` as named constants shared verbatim cross-runtime. |
| UI Substitution / DESIGN TOKEN | yes — dashboard connector | design-system primitives + `theme.css` tokens; no raw HTML, no arbitrary `*-[…]`. |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | yes | logfmt scopes per RULE OBS; `UZ-SLK-0xx` registered in the error registry mirroring `UZ-WH-0xx`; SCHEMA guard on `029`/`030` + `embed.zig`. |

---

## Overview

**Goal (testable):** a signed Slack `app_mention` POSTed to `/v1/integrations/slack/events` resolves `(team_id, channel_id)` to a lazily-materialized per-channel resident fleet, lands a `slack:<user>` steer on `fleet:{channel_fleet_id}:events`, and the fleet's answer — hydrated from and captured back to that channel's `memory.memory_entries` namespace — is posted to the originating Slack thread; a second mention in a *different thread of the same channel* recalls memory written by the first.

**Problem:** agentsfleet's only human front doors are the CLI, the dashboard chat, and per-fleet webhooks. Support and ops people live in Slack, never open the dashboard, and never author markdown — so the durable runtime never reaches them. There is no surface where the product is useful with zero setup.

**Solution summary:** ship a first-party multi-tenant `@agentsfleet` Slack app. Install is one OAuth click in the dashboard (`team_id → workspace`). Each channel the bot is mentioned in lazily materializes a **durable, reactive resident fleet** — a normal `core.fleets` row with a locked read-only/no-trigger policy — that owns that channel's memory namespace. A mention is a steer; the answer rides the existing lease→execute→report→memory loop and posts back in-thread. The bot is the acquisition on-ramp; it is reactive (answers, never acts), which is the boundary to the Rung-1 hired teammate — agency, not memory, is the paywall.

---

## Prior-Art / Reference Implementations

- **API/ingress** → `webhook_hmac.zig`/`webhook_sig.zig` (signature verify + `UZ-WH-0xx` taxonomy) and `webhooks/fleet.zig` (`XADD` + dedup) — the Slack ingress is a sibling producer. Divergence: Slack uses one app-level signing secret + `(team,channel)→fleet` resolution, vs the per-fleet URL model.
- **OAuth connector** → `integration_github.zig` + `connectors/github/` (appConnect, `crypto_store.load` platform secret) — mirror for the Slack OAuth exchange + install row.
- **Schema / Memory** → nearest migrations `021`–`028` + `docs/SCHEMA_CONVENTIONS.md`; `runner_fleet.md` §Memory continuity reused unchanged (the only new thing is the routing key).
- **UI** → the existing GitHub connector card in `ui/.../integrations/catalog.ts` (flip Slack from `vault_secret` placeholder to OAuth) + design-system primitives.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `schema/029_core_slack_installations.sql` | CREATE | `team_id → workspace_id` install binding. |
| `schema/030_core_slack_channel_bindings.sql` | CREATE | `(team_id, channel_id) → fleet_id` resident binding. |
| `schema/embed.zig` | EDIT | register `029`/`030` in the migration array (RULE MIG). |
| `src/agentsfleetd/http/handlers/integrations/slack/oauth.zig` | CREATE | OAuth callback: state-verify, code-exchange, persist install, vault the bot token. |
| `src/agentsfleetd/http/handlers/integrations/slack/events.zig` | CREATE | signed `app_mention` ingress: verify → handshake → 3s ack → resolve → route. |
| `src/agentsfleetd/integrations/slack/resident.zig` | CREATE | lazy per-channel fleet materialization + locked reactive policy + binding upsert. |
| `src/agentsfleetd/integrations/slack/post.zig` | CREATE | post the fleet's answer back in-thread via `chat.postMessage`. |
| `src/agentsfleetd/auth/middleware/slack_sig.zig` | CREATE | Slack v0 signature middleware (reuses the constant-time compare). |
| `src/agentsfleetd/http/routes.zig` | EDIT | register `/v1/integrations/slack/{oauth/callback,events}`. |
| `src/lib/common/constants.zig` | EDIT | Slack scopes, paths, `slack:bot`, `slack:` actor prefix (UFS). |
| `ui/packages/app/lib/integrations/catalog.ts` | EDIT | flip Slack card to OAuth connector. |
| `ui/packages/app/.../integrations/SlackConnect.tsx` | CREATE | dashboard Connect-Slack action + connected-state view. |
| `playbooks/operations/slack_app_registration/001_playbook.md` | CREATE | operator runbook: register the Slack app, set URLs/scopes, vault the platform secrets. |
| `playbooks/operations/github_app_registration/001_playbook.md` | CREATE | operator runbook: register the GitHub App, vault the App private key (documents the existing pattern). |
| `docs/architecture/{high_level,user_flow,data_flow,direction,roadmap}.md` | EDIT | introduce the Slack-resident surface (forward-marked). |
| `docs/architecture/scenarios/slack-channel-resident.md` | CREATE | end-to-end scenario: `#support` thread→thread memory. |

---

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** six value slices — install, ingress, resident fleet, memory round-trip, dashboard, docs — each independently testable. The resident fleet is the keystone; everything else is plumbing around the existing memory loop.
- **Alternatives considered:** (a) *per-thread fleet* — rejected: forgets across threads, kills "learns the channel." (b) *per-workspace fleet* — rejected: bleeds `#support` memory into `#random`. (c) *a new channel-keyed memory store* — rejected: reinvents `memory.memory_entries`, trades away `instance_id=fleet_id` determinism (`direction.md`).
- **Patch-vs-refactor verdict:** **patch (additive)** — one new producer + one routing layer + reused memory loop. The Rung-1 hired-teammate surface is the named follow-up, not silently mud-patched in here.

---

## Sections (implementation slices)

### §1 — Slack OAuth install + `slack_installations`

Dashboard Connect-Slack runs the OAuth code-exchange; persist the `team_id → workspace_id` binding and vault the bot token. **Implementation default:** sign the OAuth `state` with the platform key (carry `workspace_id` + a Cross-Site Request Forgery nonce) so the callback can't be forged — mirrors the GitHub connector.

- **Dimension 1.1** — OAuth callback exchanges `code`, persists `slack_installations`, vaults `xoxb` under `slack:bot` → Test `test_slack_oauth_persists_install_and_vaults_token`
- **Dimension 1.2** — forged/expired `state` is rejected, no row written → Test `test_slack_oauth_rejects_forged_state`

### §2 — Signed events ingress

`POST /v1/integrations/slack/events`: verify Slack v0 signature (constant-time), echo `url_verification` challenge, **ack within 3 s** then process async, resolve `team_id → workspace` + `actor=slack:<user>`.

- **Dimension 2.1** — valid signed `app_mention` acks ≤3 s and enqueues async work → Test `test_slack_events_acks_fast_and_enqueues`
- **Dimension 2.2** — bad signature → `UZ-SLK-010`; stale timestamp (>300 s) → `UZ-SLK-011`; unknown team → `UZ-SLK-020` → Tests `test_slack_sig_invalid`, `test_slack_sig_stale`, `test_slack_team_unmapped`
- **Dimension 2.3** — `url_verification` returns the `challenge` verbatim → Test `test_slack_url_verification_handshake`

### §3 — Per-channel resident fleet (lazy materialization)

First mention in a channel materializes a durable `core.fleets` row with a **locked reactive `ExecutionPolicy`** (read-only tools, no triggers, no cron) and upserts `slack_channel_bindings(team_id, channel_id → fleet_id, kind='resident')`. Concurrent first-mentions converge on one fleet.

- **Dimension 3.1** — first mention creates exactly one resident fleet + binding; subsequent mentions reuse it → Test `test_resident_fleet_materialized_once`
- **Dimension 3.2** — two concurrent first-mentions yield one fleet (UNIQUE + ON CONFLICT) → Test `test_resident_fleet_concurrent_first_mention`
- **Dimension 3.3** — the resident policy admits no write tool / trigger / cron regardless of input → Test `test_resident_policy_is_reactive_readonly`

### §4 — Channel memory + answer round-trip (the flagged mechanism)

A mention is a steer on the channel fleet; the run hydrates and captures the channel's memory via the **existing** `/v1/runners/me/memory/{channel_fleet_id}` loop; the answer posts back in-thread (`thread_ts`). **The thread is a delivery surface, not a memory boundary — the resident fleet owns the durable namespace, so memory crosses threads because they share `channel_fleet_id`.** The thread carries only transient input (mention + recent thread messages, fetched live, never stored) and `thread_ts`.

- **Dimension 4.1** — mention routes to a `slack:<user>` steer on `fleet:{channel_fleet_id}:events`; the answer posts to the originating `thread_ts` → Test `test_mention_steers_channel_fleet_and_replies_in_thread`
- **Dimension 4.2** — a fact stored during thread A's run is recalled in a thread-B run of the same channel (cross-thread persistence) → Test `test_channel_memory_persists_across_threads`
- **Dimension 4.3** — recent thread messages are passed as transient input but never written to `memory.memory_entries` → Test `test_thread_context_is_transient_not_stored`

### §5 — Dashboard Connect-Slack connector

Flip the Slack catalogue card from paste-token to an OAuth connector; render connected state. Requires `connector:write`.

- **Dimension 5.1** — Connect-Slack launches the OAuth flow and renders "Slack connected: {team}" on return → Test `test_dashboard_slack_connect_flow`

### §6 — Operator playbooks + architecture-doc updates

Write the two registration playbooks and update the architecture docs (forward-marked). The full step-by-step lives in the playbook files; the spec points at them (anti-pseudocode).

- **Dimension 6.1** — `slack_app_registration` playbook covers app create, scopes, the three Request URLs, OAuth redirect, and vaulting `client_id`/`client_secret`/`signing_secret` as platform secrets → Test `test_playbook_slack_registration_present` (doc-presence + required-anchor check)
- **Dimension 6.2** — `github_app_registration` playbook documents the existing GitHub App registration + private-key vaulting → Test `test_playbook_github_registration_present`
- **Dimension 6.3** — `high_level`/`user_flow`/`data_flow`/`direction`/`roadmap` updated + the new scenario added, all marking the surface forward-looking (not "runs now") → Test `test_arch_docs_reference_slack_resident`

---

## Interfaces

```
POST /v1/integrations/slack/oauth/callback?code=&state=   (signature: none; state-signed)
  → 302 to dashboard "Slack connected" on success
  → UZ-SLK-021 invalid_state | UZ-SLK-022 oauth_exchange_failed

POST /v1/integrations/slack/events                         (auth: Slack v0 signature ONLY)
  headers: X-Slack-Signature: v0=<hmac>, X-Slack-Request-Timestamp
  body (url_verification): { type, challenge }   → 200 { challenge }
  body (event_callback):   { team_id, event:{ type:"app_mention", channel, user, text, ts, thread_ts? } }
                           → 200 (empty, ≤3s); work proceeds async
  → UZ-SLK-010 invalid_signature | UZ-SLK-011 stale_timestamp | UZ-SLK-020 team_not_installed

core.slack_installations:    team_id (PK) → workspace_id (FK), bot_user_id, installed_by, scopes[], created_at
                             — bot token NOT here; vault key_name `slack:bot` (RULE VLT)
core.slack_channel_bindings: (team_id, channel_id) UNIQUE → fleet_id (FK), kind='resident', created_at

steer envelope (reused):     XADD fleet:{channel_fleet_id}:events  actor=slack:<user_id>  type=chat
                             request={ text, thread_ts, channel_id, recent_thread_msgs[] }
memory (reused, unchanged):  GET/POST /v1/runners/me/memory/{channel_fleet_id}   (instance_id = channel_fleet_id)
```

---

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Invalid signature | wrong/missing `X-Slack-Signature` | `UZ-SLK-010`, 401, no enqueue; logged. |
| Stale timestamp / replay | `\|now−ts\|` > 300 s | `UZ-SLK-011`, 401; replay window closed. |
| Team not installed | `team_id` absent from `slack_installations` | `UZ-SLK-020`, 200-ack + no-op (Slack must not see an error loop); logged. |
| Slow handler | work > 3 s inline | ack first, process async; Slack never retries on timeout. |
| Duplicate delivery | Slack at-least-once retry (same `event.ts`) | dedup on `(channel_fleet_id, event.ts)` via `SET NX`; one steer enqueued. |
| Concurrent first-mention | two mentions, no binding yet | UNIQUE `(team_id, channel_id)` + ON CONFLICT → one resident fleet. |
| Forged OAuth callback | tampered `state` | `UZ-SLK-021`, no install row. |
| Prompt injection | mention text tries to escalate (RULE PRI) | text is advisory reasoning input only; tools/triggers/secrets stay code-enforced by the reactive policy + tool bridge — text cannot grant capability. |
| Outbound post fails | bot lacks `chat:write`, or `chat.postMessage` 429 | logged `UZ-SLK-030`; honor `Retry-After` + bounded retry; never crashes. |

---

## Invariants

1. **Memory scope = channel = audience boundary** — `instance_id` is the resident `channel_fleet_id`, server-derived from the binding; `slack_channel_bindings` has UNIQUE `(team_id, channel_id)`. One channel ⇒ one namespace; never per-thread, per-user, or per-workspace. Enforced by the UNIQUE constraint + server-side derivation (no client-supplied scope).
2. **Resident fleet is reactive** — its `ExecutionPolicy` for `kind='resident'` admits no write tool, no `triggers[]`, no cron. Enforced by a single locked-policy constructor (comptime/explicit) with no code path that sets agency for residents.
3. **Bot token never in an entity table** — `slack_installations` stores only the `slack:bot` key-name; the secret lives in the vault (RULE VLT). Enforced by schema (no token column) + `crypto_store` resolution.
4. **Signature is constant-time + time-bounded** — non-short-circuiting compare (RULE CTC) over `v0:{ts}:{body}` + 300 s window. Enforced by reuse of `webhook_hmac` constant-time path.
5. **Signature is the only auth on `/v1/integrations/slack/events`** — `Authorization` is never consulted (mirrors the webhook plane). Enforced by the middleware wiring (no Bearer branch on the route).
6. **One resident fleet per channel under concurrency** — UNIQUE + ON CONFLICT DO NOTHING on the binding insert. Enforced by Postgres.
7. **No static strings / unscoped SQL in `029`/`030`** — RULE STS/NSQ. Enforced by the SCHEMA guard.

---

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | integration | `test_slack_oauth_persists_install_and_vaults_token` | valid `code` → one `slack_installations` row, token only in vault under `slack:bot`. |
| 1.2 | unit | `test_slack_oauth_rejects_forged_state` | tampered `state` → `UZ-SLK-021`, zero rows. |
| 2.1 | e2e | `test_slack_events_acks_fast_and_enqueues` | signed `app_mention` → 200 ≤3 s + one stream entry. |
| 2.2 | unit | `test_slack_sig_invalid` / `test_slack_sig_stale` / `test_slack_team_unmapped` | each → its `UZ-SLK-0xx`, no enqueue (unmapped: 200-ack no-op). |
| 2.3 | unit | `test_slack_url_verification_handshake` | `{type:url_verification,challenge:X}` → `{challenge:X}`. |
| 3.1 | integration | `test_resident_fleet_materialized_once` | mention #1 creates fleet+binding; #2 reuses same `fleet_id`. |
| 3.2 | integration | `test_resident_fleet_concurrent_first_mention` | two parallel first-mentions → exactly one fleet. |
| 3.3 | unit | `test_resident_policy_is_reactive_readonly` | resident policy build → no write tool/trigger/cron present. |
| 4.1 | e2e | `test_mention_steers_channel_fleet_and_replies_in_thread` | mention in thread A → steer on channel fleet; reply posted to `thread_ts=A`. |
| 4.2 | integration | `test_channel_memory_persists_across_threads` | store key in thread-A run → recalled in thread-B run of same channel. |
| 4.3 | integration | `test_thread_context_is_transient_not_stored` | recent thread msgs passed as input; `memory.memory_entries` unchanged by them. |
| 5.1 | e2e | `test_dashboard_slack_connect_flow` | Connect-Slack → OAuth → "Slack connected: {team}". |
| 6.1 | unit | `test_playbook_slack_registration_present` | playbook exists + has scope/URL/secret-vaulting anchors. |
| 6.2 | unit | `test_playbook_github_registration_present` | playbook exists + has private-key-vaulting anchor. |
| 6.3 | unit | `test_arch_docs_reference_slack_resident` | each named arch doc references the surface + marks it forward-looking. |

**Regression:** existing webhook/cron/steer ingress, memory continuity, and lease/report tests must stay green (the Slack producer is additive). **Idempotency/replay:** 2.2 (stale) + the dedup test (`(channel_fleet_id, event.ts)`) cover Slack at-least-once.

---

## Acceptance Criteria

- [ ] Signed `app_mention` → in-thread answer end-to-end — verify: `make test-integration` (slack e2e scenario)
- [ ] Cross-thread memory recall holds — verify: `make test-integration` (`test_channel_memory_persists_across_threads`)
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

**2. Orphaned references.** After flipping the Slack `vault_secret` placeholder to the OAuth connector, `grep -rn SLACK_BOT_TOKEN ui/ src/` must show 0 stale paste-token uses.

---

## Discovery (consult log)

> **Empty at creation.** Append as work surfaces consults, skill outcomes, and Indy-acked deferral quotes.

- **Consults** — Architecture / Legacy-Design / gate-flag triage: question + Indy's decision.
- **Skill chain** — `/write-unit-test`, `/review`, `/review-pr`, `kishore-babysit-prs` outcomes.
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
- **Reading whole-channel history** (`message.channels` firehose) — the bot learns from interaction, not surveillance; out of scope by privacy + scope discipline.
- **CLI surface for the resident bot** — Slack-only at Rung 0.
