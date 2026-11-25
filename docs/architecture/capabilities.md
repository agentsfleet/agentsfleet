---
type: explanation
audience: contributor
verified: 2026-11-25
product_version: 0.90.0
executable: false
---

# Capabilities: what a fleet is told, and what its run enforces

> Read this when you need to know what stops a fleet, as opposed to what its model was asked to do. Parent: [`README.md`](./README.md) · User-facing: [docs.agentsfleet.net/fleets/tools](https://docs.agentsfleet.net/fleets/tools).

## What it is

A fleet's capabilities sit in two layers.
The soft layer is what the language model reads: the fixed prompt, its pinned memory and the conversation.
The hard layer is the run's policy, which the runner's supervisor enforces outside the model's reach.

| Part | What it carries | Who enforces it |
|---|---|---|
| Fixed prompt | `agentsfleet`'s own text, with the fleet's name filled in | The model only; it can drift |
| Pinned memory | Every `core` entry the fleet stored ([memory](./memory.md#what-reaches-the-model)) | The model only; a pinned entry grants nothing |
| Conversation | The thread's turns and the run's own tool results | The model only |
| The run's policy | Tools, approved credentials, the GitHub repository binding and the allowed network | The runner's supervisor, in code |

## Why it exists

A model can misread its prompt, or a hostile web page can talk it into something.
The soft layer is where a fleet learns what to do.
The hard layer contains the damage when it does the wrong thing, so only a person's approval adds a credential.

## How it behaves

### The fixed prompt

The prompt builder (`afr_agent`'s `prompt` module) writes this text, then the pinned memory under one heading:

```text
You are {name}, a fleet in agentsfleet: a long-lived agent a team talks to in one chat.
Remember standing guidance with memory_store under the category core, only when the person's newest message asks you to.
It is shown to you at the start of every run. Store anything else under another category.
Schedule yourself with the cron tools, add a webhook, GitHub or Slack trigger with trigger_add, and start work that must
keep going with task_start, each only when the person's newest message asks.
You hold only the secrets and integrations a person approved for you. When you need one, call request_access with its name
(and, for GitHub, the repositories and read or write); the person approves it once.
```

A fleet has no written job description beyond this text and its conversation.

### The run's policy

`afd_gate` builds each run's `ExecutionPolicy` from the fleet row and its approved grants.
It travels in the run payload, and the runner's supervisor refuses any tool call outside it.
The fleet's two network toggles set its policy inside the runner's posture: `allow_all`, `deny_all_egress` or `allow_list_egress`.

Every run is offered every published tool and the seven sub-agent tools ([tool catalog](./runner_execution.md#tool-catalog)).
`notify`, `git` and `message` fail until a person approves their credential.
`task_start` and `task_input` appear only when the runner is task-capable and the run carries a decision profile ([durable tasks](./durable_tasks.md#how-a-task-starts)).

Placeholders such as `${secrets.NAME.host}` resolve at send time in `af_egress`.
They resolve in two places only: the `Authorization` header's value and the URL's whole host.

### Tools that change the fleet

Six tool calls change the fleet itself: `memory_store` under `core`, `cron_add`, `trigger_add`, `request_access`, `task_start` and `task_input`.
Each tool's description tells the model to call it only when the person's newest message asks.
A refused call returns to the model as `[kind] sentence`, and the model may correct it in the same run.

### A trigger's `kind` and a turn's `event_type`

These are two fields on two tables, and some values overlap.

| Field | Where it lives | Values | What it tags |
|---|---|---|---|
| `kind` | `fleet.triggers.kind`, one row per trigger | `webhook`, `app_event`, `mention` | How the fleet gets woken, as added with `trigger_add` |
| `event_type` | `thread.turns.kind`, named `event_type` on the wire (`TurnKind` in `af_wire`) | `chat`, `webhook`, `cron`, `continuation` | What one turn is |

A fleet needs no trigger to receive a `chat` turn, and no trigger row holds a schedule; a schedule's fire arrives as `cron`.
A person's answer to a question or an access card writes a `continuation` turn, whatever woke the original one.

### Guarantees around every tool call

| Guarantee | Owner page |
|---|---|
| One run per fleet at a time, fenced, with status, model and grants read fresh per run | [`data_flow.md`](./data_flow.md) |
| Every message, webhook and schedule fire becomes a queued `thread.turns` row with its actor | [`data_flow.md`](./data_flow.md) |
| Webhooks are signature-checked, replay-refused and routed only inside the workspace | [`connectors.md`](./connectors.md) |
| Vault rows are encrypted per row; a mintable integration stores a handle, never a token | [`connectors.md`](./connectors.md) |
| The daemon mints short-lived GitHub tokens; the App private key never leaves it | [`connectors.md`](./connectors.md) |
| A run asks a person with `ask_user` or `request_access`; the answer writes a continuation turn | [`connectors.md`](./user_flow.md#access) |
| A provider key goes vault to policy to supervisor to HTTPS, never to a user-facing surface | [`models.md`](./models.md#where-the-provider-key-lives) |
| A turn is refused `balance_exhausted` when the payer's balance is empty or no wallet row exists | [`billing.md`](./billing.md#the-funds-check) |
| Long conversations compact and the same run continues | [`context_lifecycle.md`](./context_lifecycle.md) |

## Limits

The platform never does any of these:

- Logs raw secret bytes, echoes a secret into the fleet's context, or saves one in `thread.turns`.
- Lets the GitHub App private key leave the daemon; the run envelope, the `secrets_map` and the runner never carry it.
- Issues a run a credential no person approved for its fleet.
- Sends a stored secret to any host but its own `host` field.
- Lets a private fleet's HTTP tools reach a host outside its approved secrets' hosts and, once approved, GitHub.
- Lets a fleet's network exceed the runner's assigned posture.
- Starts a run for a payer whose balance is empty or who has no wallet row.

The soft layer has its own limits:

- A pinned entry stays in every later prompt until a person removes it, including one a hostile page talked the fleet into ([memory](memory.md#what-reaches-the-model)).
- No fleet has a budget of its own; the payer's balance and a person's Stop bound a runaway run.
- A fleet's schedules go through a runner verb; the daemon and its hosted scheduler keep the clock ([schedules](data_flow.md#schedules)).

## Related

- [`runner_execution.md`](./runner_execution.md#tool-catalog): the tool catalog.
- [`memory.md`](./memory.md): categories, pinning and the load window.
- [`context_lifecycle.md`](./context_lifecycle.md): compaction near the model's window.
- [`connectors.md`](./connectors.md): grants, triggers and the credential broker.
- [`runner_fleet.md`](./runner_execution.md#network-egress): the egress model.
