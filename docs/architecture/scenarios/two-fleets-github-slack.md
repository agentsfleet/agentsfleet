# Scenario — Two fleets, end to end: a Pull Request, a Slack thread, and a steer

> Parent: [`README.md`](./README.md).

> References: [`../data_flow.md`](../data_flow.md) for trigger → run → watch; [`../lease_flow.md`](../lease_flow.md) for one lease.
> [`../runner_fleet.md`](../runner_fleet.md) §"Datastore topology" and [`../datastore_scaling.md`](../datastore_scaling.md) §"Target design" own queue and datastore details.
> See also [`github-pr-reviewer.md`](./github-pr-reviewer.md) and [`slack-incident-responder.md`](./slack-incident-responder.md).

> One workspace, two fleets, three ways in. Each picture shows one step and links its rules. When a picture disagrees with a linked section, the linked section wins and this page is stale.

**Outcome under test:** a Pull Request (PR) opened on `agentsfleet/linkwarden` wakes fleet A, a mention in a `#ci-dev` thread wakes fleet B, and a person steers fleet A; both fleets run on two runners at once, fleet B's answer lands back in the Slack thread, and fleet A's answer is readable in the dashboard and the Command-Line Interface (CLI) only, because nothing posts to a PR today ([`../lease_flow.md`](../lease_flow.md) §"4. How AGENT BOB 01 can reply").

## 0. The cast and the words

The words this page leans on; the rest are in [`../README.md`](../README.md) §"Glossary".

| Word | What it is |
|---|---|
| **Workspace** | Your team's space; every fleet, secret and connection lives in one. |
| **Fleet** | One saved agent: its reviewed setup, meaning its instructions and the triggers, tools, secrets, network and budget it may use. A `fleet.fleets` row that selects one immutable `fleet.revisions` row. |
| **`agentsfleetd`** | The control plane. Owns Postgres, Dragonfly, the vault and the HTTP API; never runs the model. |
| **`agentsfleet-runner`** | The worker on a host. Asks for work, runs the agent loop, reports back; holds no datastore credential. |
| **Admission** | "We accepted this job", a `fleet.admissions` row committed **before** the sender hears yes. |
| **Queue** | Unsettled `fleet.admissions` rows, oldest first within each fleet. |
| **Doorbell** | Best-effort fleet hints on sixteen `fleet:wake:{shard}` channels. Every idle poll also probes Postgres. |
| **Lease** | A time-limited permission to run one job for one fleet, carrying a **fencing token** that only goes up, so a stale worker cannot overwrite a newer one. |
| **Report** | The worker handing the answer back; settles money, history and the bookmark in one transaction. |
| **Obligation** | "We still owe someone this answer", a `fleet.obligations` row, written only when the job came from somewhere a reply can go. |
| **Activity channel** | The live tail, `fleet:{id}:activity`, a sharded pub/sub channel (`SPUBLISH` / `SSUBSCRIBE`); frames for screens, never the record. |

```
  WORKSPACE "indy-ws"
  ┌───────────────────────────────────────────────────────────────────────┐
  │                                                                       │
  │  FLEET A  "AGENT BOB 01"                FLEET B  "ci-dev-responder"   │
  │  setup: github-pr-reviewer              setup: slack incident resp.   │
  │  triggers:                              triggers:                     │
  │    type: webhook                          type: mention               │
  │    source: github                         source: slack               │
  │    repositories: [agentsfleet/            channels: [<#ci-dev id>]    │
  │                   linkwarden]                                         │
  │    events: [pull_request]                                             │
  │  credential: github (mintable)          credentials: github, grafana  │
  │  network: api.github.com                             (read-only)      │
  │  tools: http_request,                                                 │
  │         memory_store, memory_recall                                   │
  │                                                                       │
  │  Connections (once per workspace):                                    │
  │    GitHub App install  ──► connector.installs (github, inst id)       │
  │    Slack OAuth         ──► connector.installs (slack, team_id)        │
  │                            + vault handle "slack" (bot token)         │
  └───────────────────────────────────────────────────────────────────────┘
```

Slack is connected once per workspace through Open Authorization (OAuth); the GitHub App once per installation.

## 1. Save: "start a fleet"

Saving with start `none` creates the fleet and its reviewed setup without admitting work.
Setup checks the model, wallet, named credentials, connections and grants before guarded activation.
See [setup recovery](../data_flow.md#fleet-creation-and-setup-recovery).

```text
reviewed setup -> retained installing fleet -> setup checks and approvals
                                                     |
                                                     v
                                            guarded active update
                                                     |
                                      no initial admission for start=none
```

Fleet A and fleet B each own a setup revision and approved grants.
Their first trigger creates an admission; their first report creates a session checkpoint.

## 2. Trigger one: a Pull Request wakes fleet A

The GitHub App route verifies the signed request, resolves its installation and finds active fleets with matching subscriptions and approved grants.
It admits one normalized event for each match. See [repository routing](../connectors.md#repository-and-event-subscriptions-belong-to-fleets).

```text
GitHub pull_request -> verified App delivery -> matching fleet A
                                                    |
                                                    v
                                   commit fleet.admissions
                                   producer=webhook_app
                                   stable delivery key and event identity
                                                    |
                                      attempt fleet:wake:{shard}
                                                    |
                                      202 {matched: 1, enqueued: 1}
```

The admission is the queued job. A lost notification leaves it available to the next idle poll.

## 3. Trigger two: a Slack mention wakes fleet B

The Slack route verifies the provider signature and maps the team and channel to fleet B.
Its admission records the thread as a reply destination. Provider retries reuse the original event identity.

```text
signed mention -> workspace and fleet B -> commit fleet.admissions
                                         producer=slack_mention
                                         reply=Slack channel and thread
                                                  |
                                    attempt fleet:wake:{shard}
```

That destination is what makes fleet B's report owe an outbound reply.
See [the Slack flow](./slack-incident-responder.md).

## 4. Trigger three: a person steers a fleet

A person sends a fleet message after the Pull Request event arrives.
Admission records it under producer `steer`; the request's operation identity deduplicates retries.

```text
person -> POST fleet messages -> commit a second admission for fleet A
                                          |
                              attempt fleet:wake:{shard}
                                          |
                                   202 {event_id}
```

All six producers use the same ledger shape. Actor and event type describe the job; the admission row owns its stable identity.

## 5. The waiting room: inboxes, shelves and doorbells

Fleet A has two unsettled admissions and fleet B has one.
Postgres orders each fleet's queue by admission `sequence`.

```text
Postgres fleet.admissions
  fleet A: [PR opened] [steer: check migration]
  fleet B: [Slack mention]

Dragonfly fleet:wake:{shard}
  sixteen channels -> each daemon's buffer of at most 64 distinct fleet hints
```

Hints prioritize candidate selection. Every idle poll probes the ledger even if all notifications are lost.
[The queue design](../datastore_scaling.md#the-admissions-ledger-is-the-queue) owns the limits and polling costs.

## 6. Pickup: two runners, two leases

Each runner polls for work. Eligible fleets must be active, match runner labels and have an available slot.

```text
runner R1 -> poll -> fleet A's oldest admission (PR opened)
runner R2 -> poll -> fleet B's oldest admission (Slack mention)

for each claim:
  lock slot and exact oldest head with SKIP LOCKED
  advance slot and admission fences; decode before commit
  open history and stamp delivery under current fences
  check gates, billing and installed policy
  issue runner.leases under that fence
```

Only one active lease can hold a fleet. The steer waits behind A's Pull Request job until settlement releases its slot.

A parked or locked head cannot be skipped within that fleet. See [claim ordering](../lease_flow.md#the-poll-claims-an-admission-and-its-fleet-slot).

## 7. Execute: inside the runner

Canonical: [`../runner_execution.md`](../runner_execution.md) §"One lease, end to end"; the PR reviewer's egress in [`../lease_flow.md`](../lease_flow.md) §"3. `github-pr-reviewer` end to end".

```
  ┌──────────────────────── agentsfleet-runner (R1, lease A) ────────────────────────┐
  │                                                                                  │
  │  SUPERVISOR (trusted, holds the model key)                                       │
  │   prompt = "## Installed instructions" + the lease's instructions                │
  │            (+ "## Trusted repair context" for a write binding)                   │
  │   user turn = the 12-field PR digest  (a steer: your message, plus the           │
  │               thread's last eight finished turns)                                │
  │   memory: hydrate the fleet's memory window (≤256 KiB)                           │
  │                                                                                  │
  │   model turn ─► tool call ─► ROUTER                                              │
  │                    │                                                             │
  │        ┌───────────┴──────────────┐                                              │
  │        ▼                          ▼                                              │
  │   http_request / memory      shell / file / git                                  │
  │   (in the supervisor)        (in a SANDBOX: bubblewrap, Landlock, seccomp,       │
  │        │                      cgroups, its own network namespace)                │
  │        │                      fleet A has no sandbox tool → no sandbox at all    │
  │        ▼                                                                         │
  │   EGRESS GUARD  is this method, host and path allowed by the policy?             │
  │        │  GET /repos/agentsfleet/linkwarden/pulls/{n}  ✔ admitted                │
  │        │  needs ${secrets.github.token} → the daemon MINTS one:                  │
  │        │      POST /v1/runners/me/credentials/mint {lease_id,"github"}           │
  │        │      ◄── an installation token narrowed to linkwarden                   │
  │        │  POST /repos/…/pulls/{n}/reviews        ✗ no rule matches               │
  │        │      → the model reads "[request_policy_not_allowed] …" and goes on     │
  │        ▼                                                                         │
  │   each step → an activity frame → POST …/activity → the daemon SPUBLISHes it:    │
  │      tool_call_started · fleet_response_chunk · tool_call_progress ·             │
  │      tool_call_completed                                                         │
  │                                                                                  │
  │   loop ends → { response_text, tokens, telemetry, outcome }                      │
  └──────────────────────────────────────────────────────────────────────────────────┘
```

Fleet B runs the same way on R2, with read-only rules (GitHub job logs, Grafana Loki) and up to eight interim lines through `POST …/leases/{lease_id}/messages`.

## 8. Report: one transaction settles it

A terminal report commits all durable results together under the current slot, lease and admission fences.

```text
runner -> report -> one Postgres transaction
                     usage and reported lease
                     terminal event history
                     session checkpoint
                     admission settled_at
                     slot released
                     outbound obligation when a reply is owed
                            | commit
                            +-> publish terminal activity
                            +-> append connector:outbound and record its receipt
```

A stale holder receives `UZ-RUN-005` without durable changes. A repeat committed report charges nothing again.

Fleet A owes no connector delivery; fleet B's obligation names the Slack thread.
[Report settlement](../data_flow.md#c-execute--lease--runner--report) owns the failure and retry rules.

## 9. The answer comes back

Canonical: [`slack-incident-responder.md`](./slack-incident-responder.md) §"6. How the answer comes back"; [`../data_flow.md`](../data_flow.md) §"Steer flow end-to-end" (the obligation's states).

```
  FLEET B (Slack)                                 FLEET A (GitHub)
  ───────────────                                 ────────────────
  fleet.obligations row                      no obligation row
    receipt NULL, delivered_at NULL                 (admitted with Reply::None)
        │                                                │
        │ XADD connector:outbound                        │
        ▼                                                ▼
  ┌─────────────── outbound worker ───────────────┐  the answer lives in:
  │ its own Dragonfly socket, XREADGROUP BLOCK    │   fleet.events.response_text,
  │ SlackPoster:                                  │   the live tail, the dashboard
  │   the bot token from vault "slack"            │   and `agentsfleet events`
  │   escape & < >  (literal text only)           │
  │   chat.postMessage(channel_id, thread_ts)     │  ON GITHUB: nothing.
  │   ─────────────────────────► #ci-dev thread   │   egress refused the review POST
  │   stamp delivered_at  (by the POSTER,         │   (part 7), and no GitHub poster
  │                        never by the XACK)     │   exists (Indy parked it)
  └───────────────────────────────────────────────┘
```

```
   receipt IS NULL             → committed, never queued    (a crash before the outbound append)
   receipt set, delivered NULL → queued, nobody received it (a lost stream or host)
   delivered_at set            → a person has it

   afd_outbound re-queues the first state once it is 30 s old and the second after 5 min;
   a destination that refuses for good, or twelve failed cycles, stamps it abandoned.
```

A steer records no reply destination either: its answer reaches you on the live tail the CLI opened in part 4, and later through `agentsfleet events`.

## 10. Watching it live

Canonical: [`../data_flow.md`](../data_flow.md) §"D. WATCH".

```
  fleet page (ChatView)   ── EventSource ──► /live/v1/…/fleets/{id}/events/stream
  fleet wall (FleetWall)  ── EventSource ──► /live/v1/workspaces/{ws}/events/stream
                                                │
   Next.js /live/* proxy (Clerk session → Bearer) ──► agentsfleetd ── SubscriptionHub ──┐
                                                       ▲                                │
  CLI steer ── Server-Sent Events (SSE), Bearer ───────┘                                │
                                                                                        │
          one shared Dragonfly pub/sub socket: SSUBSCRIBE fleet:{id}:activity           │
          frames copied into each viewer's bounded queue ◄──────────────────────────────┘

   frames: event_admitted → event_received → tool_call_started →
           fleet_response_chunk … → tool_call_completed → event_complete
           (+ gate_opened / gate_resolved when a person is asked)

   a dropped tab:   on reconnect, read fleet.events since (last − 2 s), merge by id
   a lost channel:  catching_up {dropped:0}, then the same backfill
   the live tail is for watching; fleet.events is the record
```

A browser's `EventSource` cannot send an `Authorization` header, so the dashboard reads through its own `/live/*` routes, which turn your Clerk sign-in session into the Bearer token `agentsfleetd` accepts (`ui/packages/app/app/live/v1/`). The CLI holds its own token and calls `agentsfleetd` directly.

## 11. Both fleets on one timeline

```text
 time -------------------------------------------------------------------->

 GitHub  PR opened -> commit A#1 to fleet.admissions
 Slack   mention   -> commit B#1 (reply = thread)
 You     steer A   -> commit A#2 (no reply); event_admitted frame
                       |
                       +-> each admission attempts a wake notification

 R1  poll -> Postgres -> claim A#1 + A's slot -> run -> report A#1
 R2  poll -> Postgres -> claim B#1 + B's slot -> run -> report B#1
             |                                           |
             |       both fleets can run concurrently    +-> thread reply
             v
 A#2 waits while A's slot is held
     -> slot freed -> next poll claims A#2 -> run -> report A#2

 You  <--- live activity frames; A#2's answer reaches the CLI stream
```

**A runner that dies mid-job:** its lease expires 30 s after its last renew, and the next poll **reclaims** the job: the same history row, a new lease under a higher fencing token. A late report from the dead runner is refused with `UZ-RUN-005`, so history stays one row and each slice one `billing.usage_lines` row ([`../data_flow.md`](../data_flow.md) §"C. EXECUTE").
