---
type: explanation
audience: contributor
verified: 2026-11-25
product_version: 0.90.0
executable: false
---

# Run flow: one pull request, end to end

Read this when you want one real fleet's whole path, from its first message to its answer about a pull request (PR).
The example is the [`watch-dog` fixture](../../tests/fixtures/fleets/watch-dog.json).
Each step links to the page that owns its mechanism: [fleets](user_flow.md), [data flow](data_flow.md), [connectors](connectors.md) and [the runner](runner_execution.md).

## What it is

`watch-dog` watches one GitHub repository, `agentsfleet/linkwarden`.
It asks for read access to that repository, subscribes to its pull request events, and says the PR's number when one opens.

| Part | Value in the fixture |
|---|---|
| First message | "You watch one GitHub repository. Ask for read access … then add a GitHub trigger for its pull request events. When a pull request wakes you, say its number." |
| Access it asks for | `github`, scope `agentsfleet/linkwarden`, read |
| Trigger it adds | Kind `app_event`, source `github`, events `pull_request` |
| What it pins | "Say the pull request's number when one wakes you." |
| What a run proves | One card, one approval, one wake per opened PR, and a reply naming the PR's number |

The workspace has already connected GitHub, and the platform's GitHub App is installed on the repository ([GitHub App](connectors.md#github-app-from-install-to-a-routed-event)).

## Why it exists

Each other page tells one slice of this path at one zoom level.
This page follows one fixture through all of them, so a reader can see where each slice begins and ends.
The acceptance suite runs the same fixture against a real GitHub event.

## How it behaves

### 1. The first message shapes the fleet

```text
Person            agentsfleetd                    Runner (model)
  |-- first message -->|                                |
  |                    | fleet + thread + turn 1        |
  |                    |-- run 1 ---------------------->|
  |                    |<-- request_access github ------| scope: linkwarden, read
  |<-- access card ----| grant pending                  |
  |                    |                                | run 1 ends waiting
  |-- Approve -------->| grant approved                 |
  |                    | answer turn (continuation)     |
  |                    |-- run 2 ---------------------->|
  |                    |<-- trigger_add app_event ------| pull_request, linkwarden
  |                    |<-- memory_store core ----------| "say its number"
  |<-- answer ---------| turn settles processed         |
```

1. The person sends the first message on **New fleet**. One transaction writes the fleet, its thread and turn 1 ([create and recovery](data_flow.md#create-and-recovery)).
2. Run 1 calls `request_access` for `github` with a read scope on the repository. The card names the repository and read.
3. Run 1 ends waiting, and turn 1 settles `waiting` ([access](user_flow.md#access)).
4. The person selects Approve once. The grant becomes `approved`, and the daemon writes a continuation turn.
5. Run 2 resumes the work. It adds the `app_event` row and pins its standing guidance.

The `app_event` row names `repositories: ["agentsfleet/linkwarden"]` and `events: ["pull_request"]` ([wakes on](user_flow.md#wakes-on)).
`trigger_add` would refuse the row if the workspace had not connected GitHub.

### 2. A pull request opens

A person opens a PR on `agentsfleet/linkwarden`, and GitHub posts the signed event to `POST /v1/ingress/github`.

1. The daemon checks the body against `github-app.webhook_secret` before reading any field.
2. It maps `installation.id` to the workspace through `connector.installs`.
3. It reads the body into a 12-field `PullRequestDigest`: action, repo, number, title, url, state, draft, author, head_ref, base_ref, head_sha and received_at.
4. It finds the active fleets whose `app_event` row names the repository and admits the event. `watch-dog` matches.
5. It writes one queued turn per matching fleet, keyed `{fleet}:github:{event_id}`, then answers 202 `{matched, enqueued}`.

The turn has actor `github-app` and no reply destination.
Every `pull_request` action wakes the fleet, except one on a fleet's own repair branch.

### 3. One run, end to end

```text
GitHub                  agentsfleetd                          agentsfleet-runner
  | PR opened              |                                       |
  |-- signed POST -------->| check signature, route to watch-dog   |
  |<-- 202 ----------------| queued turn + wake                    |
  |                        | assign: fence + 1, turn received,     |
  |                        | balance, grants -> policy:            |
  |                        |   GET/HEAD /repos/agentsfleet/        |
  |                        |   linkwarden/* on api.github.com      |
  |                        |-- Down::Run {digest, policy} -------->|
  |                        |                                       | sandbox; clone linkwarden
  |                        |                                       | from the host side
  |                        |<-- memory window ---------------------|
  |                        |                                       | prompt: identity and harness facts
  |                        |                                       |  + guidance for offered tools
  |                        |                                       |  + pinned memory
  |                        |                                       | user turn: digest JSON
  |                        |<-- activity frames -------------------| model answers with
  |                        |                                       | the PR's number
  |                        |<-- report ----------------------------|
  |                        | settle processed, answer saved,       |
  |                        | thread released; no delivery owed     |
```

The assignment reads the fleet row and its approved grant, then builds the run's policy ([assignment](data_flow.md#assignment-and-the-fence)).
The approved read scope becomes a repository binding. The egress rules admit `GET` and `HEAD` under `/repos/agentsfleet/linkwarden/` and nothing else on `api.github.com`.

The runner clones the bound repository into the run's workspace from outside the sandbox, so the token never enters it.
The system prompt begins with the fleet's identity and the fixed harness facts. Each model request adds guidance for the tools that loop is offered, followed by the run's context, including pinned memory. A child offered fewer tools receives only the guidance for its own tools. The saved summary digest covers the fixed prefix, so changing the tool set preserves the summary. [Capabilities](capabilities.md)
The first user turn is the digest JSON, because the digest has no `message` field.

The digest already holds `number`, so the model can answer without calling GitHub.
The answer settles the turn `processed`. A person reads it in the fleet's chat, and the live view shows `turn_complete`.

### 4. What the run may do

| Request | Outcome |
|---|---|
| `GET` or `HEAD` under `/repos/agentsfleet/linkwarden/` | Admitted; the runner asks the daemon to mint a token at the first call |
| Any path outside that repository | Refused before it leaves the runner |
| `POST …/pulls/{n}/reviews`, an issue comment or a PR comment | Refused: `[request_policy_not_allowed]` returns to the model |
| Any write | Refused: a read grant compiles no write rule |

The minted token carries the read permissions [connectors](connectors.md#github-app-from-install-to-a-routed-event) lists.
It carries no `pull_requests` permission, because a read binding asks for none.

Pinned memory and messages are prose. "Post a review" changes what the model tries, never what the policy admits.

A write grant adds bounded write rules: Git objects, one repair-branch ref and one draft PR (`afd_gate`'s egress write rules). It still admits no review or comment ([network egress](runner_execution.md#network-egress)).

### 5. The next push

A push to the PR sends a `synchronize` event. Its body differs, so its digest and turn key differ, and it writes a new turn.

| If the fleet's run is | The new turn |
|---|---|
| Idle | Starts a run of its own |
| Live | Queues, then runs on its own once the live run reports |

Only a person's message folds into a live run. An event such as `synchronize` never does (`afd_fleet`'s `assign_inputs.sql`).

The next run's assignment prefers the last runner. Another eligible runner takes the turn that has waited longest first.
A runner holding the fleet's sandbox checks that fleet first, and other runners skip it while the hold lasts ([sandbox holds](runner_fleet.md#capacity-and-sandbox-holds)).

| Carried to the next run | Not carried |
|---|---|
| Fleet memory: up to 256 KiB, with every `core` entry first ([memory](memory.md)) | The previous run's answer, for an event-triggered run |
| The held sandbox, while its hold lasts and its key matches | A sandbox whose hold ended |

A run started by a chat message also carries the thread's finished turns with their tool calls.
An event-triggered run like this one carries none.

## Limits

- No daemon path answers on GitHub. The App route writes the turn with no reply destination, and only Slack posters exist, so the answer stays in the fleet's chat.
- No egress rule admits a review or a comment, even with a write grant.
- GitHub never redelivers a failed event on its own. An event lost before its turn commits wakes nothing ([connectors](connectors.md#limits)).
- Renaming the repository stops routing until the fleet's row names the new name.
- Recovery after a dead runner resumes from the last saved checkpoint; actions after it can repeat ([checkpoints](checkpoints.md)).
- A sandbox takes the runner host's own size limits; a run carries no size of its own ([the runner](runner_execution.md)).

## Related pages

- [Fleets](user_flow.md): access, triggers and talking to a fleet.
- [Data flow](data_flow.md): intake, assignment, reports and deliveries for every producer.
- [Connectors](connectors.md): the GitHub App and how a delivery finds its fleets.
- [Runner runtime](runner_execution.md): the sandbox, egress rules and repository writes.
- [Durable tasks](durable_tasks.md): work on an existing PR that continues across runs.
