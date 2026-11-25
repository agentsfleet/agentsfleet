---
type: explanation
audience: contributor
verified: 2026-11-25
product_version: 0.90.0
executable: false
---

# Fleets

Read this when you need to know what a fleet is made of, how it starts, how it gets access and what wakes it.
This is the person's view. [Data flow](data_flow.md) owns the mechanism behind each step, and [run flow](run_flow.md) walks one example end to end.

The [fixture fleets](../../tests/fixtures/fleets/README.md) are the examples.

## What it is

A fleet is a long-lived AI agent in a workspace. A person starts it with one message, and it keeps working after the chat closes.

| Question | Answer |
|---|---|
| How does a fleet start? | A person's first message creates it, in the browser or with `agentsfleet fleet create` |
| What does it hold at birth? | A name, a model, a thread and nothing else: no credential, no trigger, no schedule |
| How does it get access? | It asks with `request_access`; a signed-in person approves the card once |
| What wakes it? | Messages, plus the triggers and schedules it adds when a person asks |
| Who can see it? | Every member of its workspace, from the moment it exists |
| What bounds its spending? | The payer's balance ([billing](billing.md)) |

### What a fleet is made of

| Part | Where it lives | Who sets it |
|---|---|---|
| Name | `fleet.fleets.name`, unique in the workspace | The create body, else `fleet-` and the last 12 hex digits of its ID; a person can rename it |
| Model | `fleet.fleets.model_ref` | The create body, else the workspace's selected model; a person changes it ([models](models.md)) |
| Thread | `thread.threads`, whose ID is the fleet's | Written with the fleet; every message, delivery and schedule fire is a turn on it ([thread model](thread_model.md)) |
| Pinned memory | `memory.entries`, category `core` | The fleet, with `memory_store`, when a person asks ([memory](memory.md)) |
| Grants | `connector.integration_grants` | The fleet asks; a person approves (see [Access](#access)) |
| Triggers | `fleet.triggers` | The fleet, with `trigger_add`; a person can remove them (see [Wakes on](#wakes-on)) |
| Schedules | `fleet.schedules` | The fleet, with `cron_add`, or a person ([schedules](data_flow.md#schedules)) |
| Network | `fleet.fleets.http_egress` and `package_domains` | Off at birth; only a signed-in person turns on "Allow all" or "Packages" ([network egress](runner_execution.md#network-egress)) |

## Why it exists

Nothing about a fleet's job is written down before it works.
The fleet learns what it needs while it works, and asks for each credential at that moment.
A person decides each request once, on a card that names the credential and its scope.

Messages and pinned memory are prose, and they grant nothing.
A run holds only the published tool catalog, the credentials a person approved for its fleet, and the network its toggles allow ([capabilities](capabilities.md)).

## How it behaves

### Starting a fleet

In the terminal, install the command-line interface (CLI), sign in and send the first message:

```bash
curl -fsSL https://agentsfleet.dev | bash
agentsfleet login
agentsfleet fleet create "<MESSAGE>" --name <NAME>
```

`<MESSAGE>` is the fleet's first job, and `<NAME>` is optional. With `--json`, the create prints `{fleet_id, turn_id}`.
`agentsfleet fleet steer @<NAME>` with no message opens an interactive chat, one turn per line.

In the browser, the person types the same message on **New fleet** and selects Send.
The suggestion buttons there load editable text from `ui/packages/app/public/fleet-starters.json`.

Both paths call `POST /v1/workspaces/{workspace_id}/fleets`:

1. The daemon writes the fleet, its thread and its first turn in one transaction.
2. The first turn runs like any other, with no credential yet.
3. The fleet reads the message and calls tools to shape itself, each only when the person's newest message asks.
4. Later messages land on the same thread, so the chat that created the fleet stays its chat.

A retry with the same `operation_id` answers the same fleet. A refused create writes nothing ([create and recovery](data_flow.md#create-and-recovery)).
`agentsfleet doctor --json` checks that the server is reachable and a workspace is selected; it checks no model or balance.

### Standing guidance

"Always link the evidence" is standing guidance.
The fleet pins it with `memory_store` under category `core`, and every later run's prompt carries it ([memory](memory.md)).
A refused tool call returns to the model, which may correct it ([capabilities](capabilities.md#tools-that-change-the-fleet)).

### Access

A fleet asks for a credential when it needs one, with `request_access {name, scope?}`.
A workspace secret, GitHub and every other connected provider go through the same card.

```text
Run                     agentsfleetd                         Person
 |-- request_access ------>|                                    |
 |   {name, scope?}        | is it a vault secret or a          |
 |                         | connected provider? a GitHub       |
 |                         | scope? a readable host?            |
 |<-- [kind] sentence -----| refused: no row, no card           |
 |<-- granted -------------| already covered: no card           |
 |                         | else: grant pending + one card --->|
 |   run ends waiting      |                                    |
 |                         |<------------- Approve or Deny -----|
 |                         | grant approved or revoked,         |
 |                         | answer turn written                |
 |<== new run resumes, holding the credential or knowing no ====|
```

The daemon checks a request before it writes anything:

- The name must be a workspace vault secret or a connected provider.
- A `github` request must name a scope: the repositories, and read or write.
- A secret's `host` must be one the egress allowlist can read, because the secret is sent only there.

A refused request returns `[kind] sentence`, and the run continues with no row and no card.
A second request while one is pending raises no second card.

Otherwise the grant goes `pending`, with the asked-for scope kept as `requested_scope`, and one approval card appears in the chat.
The run ends waiting, and its turn settles `waiting`, so the fleet's later messages are not held behind it.

Only a signed-in person answers the card; an API key gets 403.
Approve moves `requested_scope` into `scope`. Deny sets the grant `revoked`, and a later request raises a new card.

Either answer writes a new turn that resumes the waiting work ([waiting on a person](data_flow.md#waiting-on-a-person)).

`agentsfleet fleet grants revoke`, or **Remove** under **Can access**, revokes a grant.
A live run keeps what it was issued, and the next run and the next token mint do not.
Each request, approval, denial and revoke emits the product event `fleet_access_changed` ([product analytics](product_analytics.md)).

One grant row exists per fleet and service, unique on `(fleet_id, service)`.

A run's policy reads only approved grants. Their names fill the run's secrets map, and a GitHub scope builds the repository binding.
Each approved secret's host joins the run's network allowlist.

### Wakes on

A fleet wakes on messages, on its triggers and on its schedules.
A trigger is a `fleet.triggers` row the fleet adds with `trigger_add` when a person asks. A fleet with none wakes only on messages.

| Kind | `source` | Fields | One per fleet | The turn it writes |
|---|---|---|---|---|
| `webhook` | The sender's lowercase name, never `approval` or `github` | `events`, `verifier_scheme`, `verifier_header`, `secret_name` | Per source | Actor `webhook:<source>`, from `POST /v1/webhooks/{fleet_id}/{source}` |
| `app_event` | `github` | `repositories`, `events` | One | Actor `github-app` ([GitHub App](connectors.md#github-app-from-install-to-a-routed-event)) |
| `mention` | `slack` | `channel`, a Slack channel ID | One | Actor `slack:<team_id>:<user>` ([Slack mentions](slack_mentions.md)) |

A webhook row's signature follows one of the [signing recipes](connectors.md#webhook-signing-recipes). The fleet page shows its URL to register with the sender.
The [`night-owl` fixture](../../tests/fixtures/fleets/night-owl.json) wakes on a schedule and a signed webhook.

A run adds, lists and removes rows with `trigger_add`, `trigger_list` and `trigger_remove`.
Each posts a fenced run verb on `/v1/runners/me/runs/{run_id}/triggers`, so a runner that lost its run writes nothing.

An add is refused, with `[kind] sentence` back to the model, when:

- the workspace has not connected the provider, for an `app_event` or `mention` row;
- `secret_name` names no secret in the workspace vault;
- a webhook row's source is `approval` or `github`;
- the fleet already holds a row of that kind and source.

A person lists rows with `agentsfleet fleet triggers list` and removes one with `agentsfleet fleet triggers delete`.
The fleet page's **Wakes on** list calls the same routes. Each add or remove writes the operator log `trigger_changed`.

Each delivery route reads `fleet.triggers` with a `workspace_id` predicate, so a delivery never reaches another workspace's fleet.
A delivery needs no grant. A grant governs only what the fleet may call.

### Talking to it

A person sends a message with `agentsfleet fleet steer` or the chat composer. Each message is a turn, and the answer names a disposition:

| Disposition | When |
|---|---|
| `folded` | The fleet's run is live; the run takes the message at its next model call |
| `started` | No run is live, so the message starts one |
| `queued` | Work ahead of it holds the fleet's one run |

The dashboard names every send with an `operation_id`, so **Try again** never sends twice ([web app](web_app.md)).

A person reaches any fleet by name, written `@<NAME>`. The CLI and dashboard look the name up with `GET …/fleets?name=`, then send an ordinary message.
In Slack, the first word after the bot names the fleet ([Slack mentions](slack_mentions.md)).

A fleet hands work to another fleet in its workspace with `fleet_steer`, and hears back the same way.
The [`queen-bee` and `busy-bee`](../../tests/fixtures/fleets/README.md) fixtures are the pair for this path.

```text
person, in queen-bee's chat: "Ask busy-bee for today's word and tell me what it said."

queen-bee   fleet_activity()                          finds busy-bee's ID
            fleet_steer(<busy-bee ID>, "what is today's word?")
            answers at once: "asked busy-bee"         it does not wait
busy-bee    runs under its own grants; its message begins
            [from fleet queen-bee, fleet_id <ID>; ...]
            fleet_steer(<queen-bee ID>, "today's word is ...")
queen-bee   wakes for a second run: "busy-bee said ..."
```

A fleet's steer lands as actor `fleet:<fleet_id>` and never folds into a live run.
`fleet_activity()` lists every fleet with its status, so a fleet can read a digest without steering.

### What the fleet page shows

Beside the chat, **Wakes on** lists triggers and schedules, **Can access** lists approved grants and **Remembers** lists pinned memory. Each entry has a remove control.

The model picker and the "Allow all" and "Packages" toggles each send one `PATCH` to the fleet. [Web app](web_app.md) owns the page.

## Limits

- A name grants nothing. An unknown `@<NAME>` is refused and never falls back to the open chat's fleet.
- A fleet starts with no credential. A missing one is a card the fleet raises, never a refused create.
- Nothing caps how many triggers a fleet adds, beyond one per kind and source and what the workspace has connected.
- A GitHub row that names no repository receives nothing.
- Any fleet may steer any active fleet in its workspace, as often as the payer's balance allows. Nothing lists targets, limits the chain or caps the count.
- A steer carries text, never access. A fleet that reads untrusted text can be told to steer another fleet. Each target's own grants and the payer's balance bound the damage.
- Fleet steering has no wait. The receiver steers back, or the sender reads `fleet_activity`.
- Connecting Slack creates no default fleet. A channel with no `mention` row answers `channel_not_configured`.

## Related pages

- [Data flow](data_flow.md): what each message, delivery and fire does inside the daemon.
- [Run flow](run_flow.md): the `watch-dog` fixture, from first message to a pull request's answer.
- [Connectors](connectors.md): how a workspace connects GitHub and Slack.
- [Capabilities](capabilities.md): what a run is told and what it holds.
- [Durable tasks](durable_tasks.md): work that continues across runs until it is done.
- [Web app](web_app.md): the fleet page and the chat.
