# Scenario — Slack-resident channel bot (the Rung-0 on-ramp)

> Parent: [`README.md`](./README.md) · User-facing: [docs.agentsfleet.net/fleets/connectors](https://docs.agentsfleet.net/fleets/connectors).

> References: [`../runner_fleet.md`](../runner_fleet.md) §"Memory continuity" (the hydrate/capture loop), [`../data_flow.md`](../data_flow.md) (single-ingress trigger/execute loop), [`../user_flow.md`](../user_flow.md) §"§8.8 Slack as a resident surface (Rung 0)".
>
> **Shipped (M206_002 §5).** First specced in [`../../v2/done/M106_001_P1_API_DOCS_INFRA_UI_SLACK_RESIDENT_CHANNEL_BOT.md`](../../v2/done/M106_001_P1_API_DOCS_INFRA_UI_SLACK_RESIDENT_CHANNEL_BOT.md). `agentsfleetd` routes a mention to a fleet whose `TRIGGER.md` names the channel, and a mention no such fleet takes to the channel's resident. The resident is `slack-channel-<team>-<channel>`, installed through `Fleets::install` on the first mention with a policy built in code (`rustd/crates/afd_ingress/src/slack/resident.rs`), and bound once in `core.connector_channels` (`rustd/crates/afd_api_ingress/src/handler/mention/resident.rs`). Concurrent first mentions converge on the fleet name. [`slack-incident-responder.md`](./slack-incident-responder.md) §"4. Routing a mention" carries the routing.

**Outcome under test:** a fact a user tells `@agentsfleet` in one Slack thread is recalled by the bot in a *different thread of the same channel* — because the memory namespace is the per-channel resident fleet, not the thread. The bot stays read-only and never acts unattended.

Legend: ✅ shipped · ♻️ reused substrate · 🟡 shipped, gap open (§5).

```mermaid
sequenceDiagram
  autonumber
  participant Lead as Support lead (Slack)
  participant Slack as Slack
  participant API as agentsfleetd
  participant PG as Postgres
  participant Runner as agentsfleet-runner

  Note over Lead,Runner: one-time — admin connects Slack (Open Authorization (OAuth)) → vault handle + core.connector_installs ✅
  Lead->>Slack: @agentsfleet what's our prod called? (thread A)
  Slack->>API: POST /v1/connectors/slack/events (v0 Hash-based Message Authentication Code (HMAC)) ✅
  API->>PG: resolve team→workspace; materialize channel fleet via Fleets::install (default skill.md) ✅
  API->>API: admit producer slack_mention, key <team_id>:<event_id>, actor=slack:<user> → XADD fleet:{channel_fleet_id}:events ✅
  Runner->>API: lease → run; GET /me/memory/{channel_fleet_id} (empty) ♻️
  Runner->>API: report "don't know yet — tell me?"
  API->>Slack: chat.postMessage thread_ts=A (outbound poster) ✅
  Lead->>Slack: it's "aurora" (thread A)
  Runner->>API: POST /me/memory/{channel_fleet_id} {prod: aurora} ♻️
  Note over Lead,Runner: …days later, a different thread…
  Lead->>Slack: @agentsfleet is aurora healthy? (thread B)
  API->>API: SAME channel_fleet_id (core.connector_channels)
  Runner->>API: lease → GET /me/memory/{channel_fleet_id} → {prod: aurora} ✅
  Runner->>API: report (uses "aurora")
  API->>Slack: chat.postMessage thread_ts=B (outbound poster) ✅
```

---

## 1. Install — one Open Authorization (OAuth), multi-tenant

A workspace admin clicks **Connect Slack** in the dashboard. `agentsfleetd` runs the Open Authorization (OAuth) code-exchange and persists the install as a `(workspace_id,'slack')` **vault handle** carrying the bot token + metadata (`bot_user_id`, `scopes`) — the same path the GitHub connector takes (`rustd/crates/afd_api_tenant/src/handler/connector/callback.rs`), with no Slack-specific table. A `core.connector_installs (provider='slack', external_account_id=team_id → workspace_id)` row makes the inbound `team_id → workspace` lookup resolvable. The platform app credentials (`client_id`/`client_secret`/`signing_secret`) were registered once via [`../../../playbooks/operations/slack_app_registration/001_playbook.md`](../../../playbooks/operations/slack_app_registration/001_playbook.md). One app serves every tenant; `team_id` is the routing key. ✅

## 2. The channel is the memory namespace

The first `@mention` in `#support` materializes a **durable per-channel resident fleet** by calling the existing install path (`Fleets::install`, `rustd/crates/afd_fleet_lifecycle/src/install.rs`) with the default skill `resident.md` (`rustd/crates/afd_ingress/src/slack/resident.md`). The result is a normal `core.fleets` row with a **code-set config**: one `api` trigger, `tools: []`, no hosts, and the default daily budget (`default_trigger` in `rustd/crates/afd_fleet_lifecycle/src/install/authored.rs:139-144`), never read from the skill prose. It is bound once in `core.connector_channels (provider='slack', external_account_id=team_id, external_channel_id=channel_id → fleet_id, kind='resident')` (`schema/560_connector_channels.sql`). Every later mention in *any thread* of `#support` that no subscribed fleet takes routes to this same fleet. No new fleet-creation path exists: `Fleets::install` is the only `core.fleets` insert path.

| | Thread | Channel-resident fleet |
|---|---|---|
| Lifetime | one conversation | durable, per `(team_id, channel_id)` |
| Carries | transient input (mention + recent thread msgs) + `thread_ts` delivery target | the channel's **memory namespace** (keyed by the resident `fleet_id`) |

## 3. Mention → steer → answer (one reasoning loop)

A mention is a `slack:<user>` event admitted under producer `slack_mention`, keyed `<team_id>:<event_id>` (`rustd/crates/afd_ingress/src/slack/admit.rs:59`), on `fleet:{channel_fleet_id}:events`, the **same** single ingress as webhook, cron and steer ([`../data_flow.md`](../data_flow.md) §"B. TRIGGER"). On lease, the runner hydrates the channel's memory (`GET /v1/runners/me/memory/{channel_fleet_id}`); the runner's agent loop ([`../runner_execution.md`](../runner_execution.md#process-model)) answers from that memory plus the live thread context (it holds no tools and no credentials); the daemon's outbound poster posts the answer with `chat.postMessage thread_ts=<originating>` (`afd_outbound/src/slack.rs`). Before the report, new facts are captured (`POST …/memory/{channel_fleet_id}`). The hydrate/capture loop is **reused unchanged** from [`../runner_fleet.md`](../runner_fleet.md) §"Memory continuity".

## 4. The cross-thread payoff

Thread A stored `prod=aurora`. Thread B — a different thread, possibly days later — hydrates the **same** `channel_fleet_id` namespace and recalls `aurora`. Memory persists because the resident fleet owns the namespace and each run borrows it; nothing is stored in the thread. The run holds its memory in the runner's supervisor and pushes it, fenced, before the report ([`../runner_execution.md`](../runner_execution.md#crates)); the channel store is durable in Postgres. The capture half has an open gap for the resident (§5).

## 5. Surface in `agentsfleetd`

| Step | Status |
|---|---|
| Memory hydrate/capture loop (keyed by `fleet_id`) | ✅ lease-scoped (`Plane::hydrate`, `Plane::capture` in `rustd/crates/afd_fleet/src/lease/memory.rs`) |
| The resident capturing a fact | 🟡 read from source, not run: the resident's policy is `tools: []` (`rustd/crates/afd_fleet_lifecycle/src/install/authored.rs:141`) and a run is offered only the tools its lease names (`Catalog::select`, `rustd/crates/afr_tools/src/catalog/mod.rs:245`), so the model has no `memory_store` to call, though `resident.md` tells it to capture. A live two-thread run settles it. |
| Single ingress / lease / execute / report | ✅ one `slack_mention` admission per Slack event (`rustd/crates/afd_api_ingress/src/handler/mention.rs`) |
| Slack OAuth install — vault handle + generic `core.connector_installs` | ✅ |
| Signed events ingress + `(team,channel)→fleet` routing (`core.connector_channels`) | ✅ (`rustd/crates/afd_ingress/src/slack/route.rs`) |
| Per-channel resident fleet: default skill (`resident.md`), code-set config | ✅ (`rustd/crates/afd_ingress/src/slack/resident.rs`) |
| In-thread answer (`chat.postMessage thread_ts`) | ✅ owed only to the recorded destination (`rustd/crates/afd_fleet/src/lease/obligation.rs`, `rustd/crates/afd_outbound/src/slack.rs`) |

## 6. What this scenario proves

- **Memory scope = channel = audience boundary.** The resident fleet (keyed by its `fleet_id`), not the thread, is the namespace — so memory crosses threads and never bleeds across channels.
- **One reasoning loop.** A Slack mention is one more producer into the same ingress; the lease/execute/report path never branches on actor type.
- **Reactive runs on the same runtime.** Read-only, mention-only, never unattended: the on-ramp to the durable hired agent (Rung 1).

## 7. What is NOT in this scenario (Rung 1)

Hired durable agents, source webhooks (Zoho Desk, Statuspage), write actions, approval gating, interactivity buttons, slash commands, and direct messages belong to the follow-on milestone. The resident's limit (no system access) is what points a channel at that durable agent.
