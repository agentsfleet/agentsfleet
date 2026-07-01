# Scenario — Slack-resident channel bot (the Rung-0 on-ramp)

> Parent: [`README.md`](./README.md) · References: [`../runner_fleet.md`](../runner_fleet.md) (§Memory continuity — the hydrate/capture loop), [`../data_flow.md`](../data_flow.md) (single-ingress trigger/execute loop), [`../user_flow.md`](../user_flow.md) §8.8 (Slack as a resident surface).
>
> **Forward-looking (M106 — pending).** Specced in [`../../v2/pending/M106_001_P1_API_DOCS_INFRA_UI_SLACK_RESIDENT_CHANNEL_BOT.md`](../../v2/pending/M106_001_P1_API_DOCS_INFRA_UI_SLACK_RESIDENT_CHANNEL_BOT.md). This narrates the *target* surface, not the runtime as it ships today.

**Outcome under test:** a fact a user tells `@agentsfleet` in one Slack thread is recalled by the bot in a *different thread of the same channel* — because the memory namespace is the per-channel resident fleet, not the thread — with the bot read-only and never acting unattended.

Legend: ✅ built today (reused) · 🔨 to-build (M106).

```mermaid
sequenceDiagram
  autonumber
  participant Lead as Support lead (Slack)
  participant Slack as Slack
  participant API as agentsfleetd-api
  participant PG as Postgres
  participant Runner as agentsfleet-runner

  Note over Lead,Runner: one-time — admin connects Slack (OAuth) → vault handle + core.connector_installs 🔨
  Lead->>Slack: @agentsfleet what's our prod called? (thread A)
  Slack->>API: POST /v1/connectors/slack/events (v0 HMAC) 🔨
  API->>PG: resolve team→workspace; materialize channel fleet via innerCreateFleet (default skill.md) 🔨
  API->>API: XADD fleet:{channel_fleet_id}:events actor=slack:<user> (webhook-producer shape) ✅ reused
  Runner->>API: lease → run; GET /me/memory/{channel_fleet_id} (empty) ✅
  Runner->>Slack: chat.postMessage thread_ts=A "don't know yet — tell me?" 🔨
  Lead->>Slack: it's "aurora" (thread A)
  Runner->>API: POST /me/memory/{channel_fleet_id} {prod: aurora} ✅
  Note over Lead,Runner: …days later, a different thread…
  Lead->>Slack: @agentsfleet is aurora healthy? (thread B)
  API->>API: SAME channel_fleet_id (core.connector_channels)
  Runner->>API: lease → GET /me/memory/{channel_fleet_id} → {prod: aurora} ✅
  Runner->>Slack: chat.postMessage thread_ts=B (uses "aurora") 🔨
```

---

## 1. Install — one OAuth, multi-tenant

A workspace admin clicks **Connect Slack** in the dashboard. `agentsfleetd` runs the OAuth (Open Authorization) code-exchange, persists the install as a `(workspace_id,'slack')` **vault handle** carrying the bot token + metadata (`bot_user_id`, `scopes`) — mirroring the GitHub connector (`github/callback.zig`, zero entity tables) — plus a generic `core.connector_installs (provider='slack', external_account_id=team_id → workspace_id)` row that makes the inbound `team_id → workspace` lookup resolvable. The platform app credentials (`client_id`/`client_secret`/`signing_secret`) were registered once via [`../../../playbooks/operations/slack_app_registration/001_playbook.md`](../../../playbooks/operations/slack_app_registration/001_playbook.md). One app serves every tenant; `team_id` is the routing key. 🔨

## 2. The channel is the memory namespace

The first `@mention` in `#support` materializes a **durable per-channel resident fleet** by calling the existing fleet-create path (`innerCreateFleet`) with a default channel-bot `skill.md` — a normal `core.fleets` row with a **code-set reactive config** (read-only tools, no `triggers[]`, no cron; set by the materialization helper, never from the skill.md prose) — and binds it in the generic `core.connector_channels (provider='slack', team_id, channel_id → fleet_id, kind='resident')`. Every later mention in *any thread* of `#support` routes to this same fleet. No new fleet-creation actor exists: `innerCreateFleet` is the sole `core.fleets` insert path, invoked here under the install-delegated workspace authority.

| | Thread | Channel-resident fleet |
|---|---|---|
| Lifetime | one conversation | durable, per `(team_id, channel_id)` |
| Carries | transient input (mention + recent thread msgs) + `thread_ts` delivery target | the channel's **memory namespace** (keyed by the resident `fleet_id`) |

## 3. Mention → steer → answer (one reasoning loop)

A mention is a `slack:<user>` event XADDed via the webhook-producer shape (signature-authed, no principal — `webhooks/fleet.zig`) on `fleet:{channel_fleet_id}:events` — the **same** single ingress as webhook / cron / steer ([`../data_flow.md`](../data_flow.md) §B). On lease, the runner hydrates the channel's memory (`GET /v1/runners/me/memory/{channel_fleet_id}`); NullClaw answers from that memory plus the live thread context (read-only — it holds no write credentials at Rung 0); the answer posts back via `chat.postMessage thread_ts=<originating>`. On report, new facts are captured (`POST …/memory/{channel_fleet_id}`). The hydrate/capture loop is **reused unchanged** from [`../runner_fleet.md`](../runner_fleet.md) §Memory continuity.

## 4. The cross-thread payoff

Thread A stored `prod=aurora`. Thread B — a different thread, possibly days later — hydrates the **same** `channel_fleet_id` namespace and recalls `aurora`. Memory persisted **not because anything was stored in the thread**, but because the resident fleet owns the namespace and the ephemeral run borrows it. The compute is ephemeral (`:memory:` SQLite, gone on child exit); the channel store is durable in Postgres.

## 5. Built vs to-build

| Step | Status |
|---|---|
| Memory hydrate/capture loop (keyed by `fleet_id`) | ✅ reused |
| Single ingress / lease / execute / report | ✅ reused |
| Slack OAuth install — vault handle + generic `core.connector_installs` | 🔨 M106 |
| Signed events ingress + `(team,channel)→fleet` routing (`core.connector_channels`) | 🔨 M106 |
| Per-channel resident fleet — via `innerCreateFleet` + default skill.md, code-set reactive config | 🔨 M106 |
| In-thread answer (`chat.postMessage thread_ts`) | 🔨 M106 |

## 6. What this scenario proves

- **Memory scope = channel = audience boundary.** The resident fleet (keyed by its `fleet_id`), not the thread, is the namespace — so memory crosses threads and never bleeds across channels.
- **One reasoning loop.** A Slack mention is one more producer into the same ingress; the lease/execute/report path never branches on actor type.
- **Reactive is not a second runtime.** Read-only, mention-only, never unattended — the on-ramp to the durable hired teammate (Rung 1), not "a chat UI over tools."

## 7. What is NOT in this scenario (Rung 1)

Hired durable teammates, source webhooks (Zoho Desk / Statuspage), write actions, approval gating, interactivity buttons, slash commands, and direct messages — the follow-on milestone. The reactive bot's limit (no system access) is the conversion lever to that durable teammate.
