---
type: explanation
audience: contributor
verified: 2026-11-25
product_version: 0.90.0
executable: false
---

# Slack mentions

Read this when you need to know which fleet a Slack mention reaches, what it is told and how its answer gets back.
The [`chatty-parrot` fixture](../../tests/fixtures/fleets/chatty-parrot.json) is the worked example.

## What it is

A person mentions the bot in a Slack channel. The daemon hands the mention to one fleet that subscribed to that channel, then posts the fleet's answer in the same thread.

| Question | Answer |
|---|---|
| How does a fleet subscribe? | It adds a `mention` trigger row for one channel with `trigger_add`, when a person asks ([triggers](user_flow.md#wakes-on)) |
| Which fleet gets a mention? | The fleet whose name leads the message, or the channel's only eligible subscriber |
| What if no fleet subscribes? | The route answers `channel_not_configured`; no fleet is created and no model runs |
| Who posts the answer? | The daemon, with the workspace's bot token; no fleet holds a Slack credential |
| What scopes does the app ask for? | `app_mentions:read`, `chat:write` and `channels:history` |
| How is a retry kept from running twice? | The turn is keyed `<team_id>:<event_id>`, which Slack signs and repeats |

## Why it exists

A channel's fleets are long-lived subscribers, so each one keeps its memory across mentions.
A fleet created per mention would start every time with empty memory ([memory](memory.md)).
One bot serves every fleet, so the first word after the bot names the fleet a person wants.

## How it behaves

### Three things a channel needs

| | Slack connector | Notice owner | Subscription |
|---|---|---|---|
| What it is | The workspace's Slack credential and team routing | A fleet that carries fixed notices for the channel | A fleet's `mention` trigger row |
| Where it lives | Vault handle and `connector.installs` | The existing `connector.channels` row | `fleet.triggers`: kind `mention`, source `slack`, one channel ID |
| Who creates it | A workspace administrator, with **Connect** | The first notice, from an existing subscriber | The fleet, with `trigger_add`; a person removes it under **Wakes on** |
| What it proves | The daemon can use the workspace's connector | Notice ownership only, never model or turn authority | This fleet is a candidate for the channel's mentions |

A row names the channel by its ID, so renaming the channel changes nothing.

### How a mention is routed

```text
Person           Slack                 agentsfleetd                         Postgres
  |-- @bot ... -->|                         |                                   |
  |               |-- signed app_mention -->|                                   |
  |               |                         | check signature, 5-minute window  |
  |               |                         | drop edits, own posts and replays |
  |               |                         |-- team_id -> connector.installs ->|
  |               |                         |<------------- workspace ----------|
  |               |                         |-- channel's mention rows -------->|
  |               |                         | match the leading name            |
  |               |                         | re-read the thread (1.5 s)        |
  |               |                         |-- one slack_mention turn -------->|
  |               |<-------- 200 -----------|                                   |
```

Before any route, the daemon drops a message Slack marks with a `subtype`: an edit, a join, or a classic bot or webhook post.
It drops a mention with no user, and any post by its own bot user, so a reply never wakes a fleet.

A post with Slack's `bot_id` and a real user wakes a fleet like a person's. Alert apps and a person's own token through an app both post this way.

The daemon reads only the subscription rows in the workspace the Slack install belongs to.
A leading fleet name is matched without regard to case, and a `:` or `,` after it ends the name.

| Channel state | Outcome |
|---|---|
| No subscribers | `Unconfigured`: answers `channel_not_configured`, with no fleet and no model call |
| One subscriber's name matches, and it can run | `Addressed`: that fleet gets the turn |
| The named fleet is archived | `Archived` notice; no model call |
| Two names match, differing only in case | `Ambiguous` notice; no fleet is picked |
| No name, one eligible subscriber | `Sole`: that fleet gets the turn |
| No name, several eligible subscribers | `Choose` notice, asking which fleet should answer |
| No name, and no subscriber may take an unaddressed mention | `AddressIt` notice, naming the fleets a person can address |

A fleet whose approved GitHub grant can write is addressed-only. It gets a mention only when the message names it, even as the channel's only subscriber.
The [route](../../rustd/crates/daemon/afd_ingress/src/slack/route.rs) owns name matching, and the [mention handler](../../rustd/crates/daemon/afd_api_ingress/src/handler/mention/mod.rs) owns the turn.

A notice is fixed text posted through the outbound ledger, the same path an answer takes.

The channel's `connector.channels` row names a notice owner. With none, the first notice binds an existing active subscriber and creates no fleet.
The binding must match the current workspace and Slack team, and an archived fleet never carries a notice.

### What the fleet is told

Slack's event carries the mention and nothing else.
Before writing the turn, the daemon re-reads the thread with `conversations.replies`, under a 1.5 s deadline. It releases its pooled Postgres connection before the call.

The turn's `message` is the mention text, then the whole thread under a fixed heading that marks it as untrusted data.
Attachment and block text are flattened in, so a link in the thread's first message survives.
A failed or forbidden re-read sends the mention alone, and the message says so.

The turn's actor is `slack:<team_id>:<user>`.
The fleet's own grants, memory and network toggles decide what it can do. A mention grants nothing.

### How the answer comes back

The mention's turn records a reply destination: provider `slack` and the address `{team_id, channel_id, thread_ts}`.
The `thread_ts` is the mention's thread, or the mention itself when it started one.
An answer to a question or access card the run raised inherits the destination from the turn it resumes.

When the run reports, the report transaction writes one `thread.deliveries` row for that destination.
The outbound worker posts it with `chat.postMessage`, under a 5 s deadline ([outbound deliveries](data_flow.md#outbound-deliveries)).
A failed run with no answer, or a turn refused before it ran, owes the thread one fixed notice instead.

A run can post interim lines before its answer through `POST /v1/runners/me/runs/{run_id}/messages`.
Each line is named by its call number in the run, so a retried call posts the same line once.
The daemon posts every text as literal text, sending `&`, `<` and `>` as Slack entities.

`chat.postMessage` has no idempotency key, so every post carries a marker in Slack's message metadata. An interim line's marker is `agentsfleet_interim` with `{run_id, line}`.

A poster unsure whether an earlier attempt landed reads the thread first, under a 3 s deadline.
It skips the post only on its own bot's marker for that answer, and posts anyway when it cannot read the thread.

## Limits

- One Slack event writes at most one turn or one notice, across Slack's retries. A post is at least once: a lost reply from Slack can still show the same answer twice when the thread cannot be read.
- A private channel needs `groups:history`, which the app does not ask for. A thread re-read there fails, and the fleet gets the mention alone.
- A connected Slack card proves a saved credential. It does not prove a live token, an event subscription or a channel binding.
- A destination that refuses a delivery for good, or one out of delivery cycles, is abandoned and never offered again.
- No count or length cap applies to interim lines. Slack refuses what it cannot take, and the run reads that line as undelivered.
- A channel has no mention per fleet, and a mention cannot reach a fleet in another workspace.
- Any app in the channel can wake a fleet by mentioning it. Our own replies stay dropped.
- Alert text carries outside content that reaches the model. Approved grants, the repair-branch write rule and egress policy still bound what the run can do.
- A noisy or looping bot starts a paid run on each mention, bounded by the workspace wallet.

## Related pages

- [Connectors](connectors.md): how a workspace connects Slack and where the signing secret lives.
- [Fleets](user_flow.md): how a fleet adds its `mention` row and reaches other fleets by name.
- [Data flow](data_flow.md): intake, the run and outbound deliveries.
- [Memory](memory.md): what a subscribed fleet keeps across threads.
- [Slack incident drill](../../playbooks/operations/slack_incident_drill/001_playbook.md): the channels, apps and fleets used in the live drill.
