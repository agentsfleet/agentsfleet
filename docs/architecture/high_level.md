---
type: explanation
audience: contributor
verified: 2026-11-25
product_version: 0.90.0
executable: false
---

# How `agentsfleet` fits together

> Parent: [`README.md`](./README.md) · User-facing: [docs.agentsfleet.net/concepts](https://docs.agentsfleet.net/concepts) states the same model for a user.

## What it is

`agentsfleet` runs AI agents, called fleets, that wake on events and keep working after a person leaves.
A fleet is one chat plus its memory, its approved access and its wake-ups.
A person's first message, in the browser or the terminal, creates the fleet, its thread and its first turn.

`agentsfleetd` is the control plane: the API, the run assignment and the stores.
`agentsfleet-runner` runs the agent loop, its model calls and its sandboxes on a runner host; the daemon makes no model call.
The browser app and the `agentsfleet` command line talk only to `agentsfleetd`.

| Question | Answer |
|---|---|
| What holds durable facts? | Postgres, every one of them |
| What carries live signals? | Core NATS, three nodes in one region, storing nothing |
| What wakes a fleet? | A person's message, a schedule, a webhook, a GitHub App event or a Slack mention |
| How many runs can a fleet have at once? | One |
| What can a fleet touch? | The published tools, plus access a person approved |

The [product thesis](../product/thesis.md) says who it is for and what it deliberately does not do.

## Why it exists

Operational work outlives the prompt that started it.
It needs durable state, evidence from real systems and a person's approval before risky steps.
So the runtime keeps a small typed core for safety and durability, and leaves behaviour to the conversation.

## How it behaves

### The pieces

```text
   browser, command line, webhooks
             |  HTTPS
             v
 +-----------------------+   signals, at most once   +---------------------------+
 | agentsfleetd replicas |<------------------------->| core NATS                 |
 | API, runs, reports    |                           | 3 nodes in prod, 1 region |
 |                       |                           +---------------------------+
 |                       |
 |                       |   facts, in transactions  +---------------------------+
 |                       |<------------------------->| Postgres                  |
 +-----------------------+                           | every durable fact        |
             ^                                       +---------------------------+
             |  one push stream per runner host
             v
 +-----------------------+
 | agentsfleet-runner    |
 | agent loop, sandboxes |
 +-----------------------+
```

Threads, turns, runs, items, outbound deliveries and usage are all Postgres rows.
Core NATS carries seven kinds of signal: thread wakes, thread items, runner commands, delivery wakes, member changes, fleet-list changes and bus gaps.
Runners never reach either store; each runner host holds one stream to a replica, and work is pushed down it.

### The design rules

- **Every wake takes one path.** Each wake becomes a turn in `thread.turns`, stamped with its actor, such as `steer:<user>`, `cron:<schedule>` or `github-app`.
- **One run per fleet.** Assignment hands out a fleet's thread to one runner at a time, guarded by a fencing token.
- **Behaviour comes from words.** The prompt is fixed text, the conversation and the fleet's pinned memory.
- **Capability comes from grants.** `agentsfleetd` builds each run's policy from the fleet row and its approved grants.
- **Secrets stay outside the model.** The runner swaps `${secrets.NAME.FIELD}` for the value at send time, outside the sandbox.
- **Approvals are rows.** A question or an access request raises a card; the turn waits, and an answer resumes it.
- **Context fits the model.** Near the model's window, compaction summarizes older history and keeps the prompt, pinned memory and tools.

### Where each topic lives

| Topic | Owner |
|---|---|
| What a fleet is, from a person's side | [User flow](user_flow.md) |
| One event from intake to report | [Data flow](data_flow.md) |
| Postgres and NATS roles | [Datastore scaling](datastore_scaling.md) |
| Turns, runs and actors | [Thread model](thread_model.md) |
| Triggers, GitHub and Slack | [Connectors](connectors.md) |
| Tools, questions and access cards | [Capabilities](capabilities.md) |
| Pinned and semantic memory | [Memory](memory.md) |
| Compaction | [Context lifecycle](context_lifecycle.md) |
| Model calls and credentials on the runner | [Runner runtime](runner_execution.md) |
| Money and model keys | [Billing](billing.md) and [models](models.md) |
| Browser app | [Web app](web_app.md) |
| Wire shapes | [API resources](api_resources.md) |

## Limits

- A lost signal never loses work, because the turn stays queued in Postgres.
- A detected bus gap starts a recovery pass; a silently lost wake waits for the next wake or a freed runner, because no timer polls.
- A browser viewer that misses frames gets `catching_up` and reloads from `thread.turns`.
- A message or a pinned memory entry grants nothing; only an approved grant adds access.
- A fleet acts only with credentials a person approved; a GitHub grant bounds its repositories and its write reach.
- Memory recall is a case-insensitive substring match, with no index or embeddings; the model judges relevance.
- A dead run holder blocks its fleet until the run expires; then assignment runs again.

## Related

- [Product thesis](../product/thesis.md): who `agentsfleet` is for and why.
- [Fixture fleets](../../tests/fixtures/fleets/README.md): eight example fleets, one per product path.
- [Data flow](data_flow.md): follows one turn through the stores.
