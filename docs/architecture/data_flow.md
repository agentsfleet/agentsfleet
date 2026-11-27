---
type: explanation
audience: contributor
verified: 2026-11-25
product_version: 0.90.0
executable: false
---

# Data flow

Read this when you need to know where a message, webhook, schedule fire or Slack mention goes, from intake to the answer.
This is the daemon's view. [Fleets](user_flow.md) is the person's view, and [run flow](run_flow.md) walks one example.

## What it is

Every input to a fleet becomes one row in `thread.turns`, the turns ledger.
`agentsfleetd` assigns each row to a runner as a run, then records the run's report, its usage and any reply owed in one transaction.

| Question | Answer |
|---|---|
| Where does every input go? | One `thread.turns` row, written by one of seven producers |
| How is a retried input kept from running twice? | Each row is unique on `(producer, producer_key)`, and a repeat returns the first turn |
| How many runs can a fleet have at once? | One, enforced by a fencing token on the fleet's `thread.threads` row |
| What carries the wake-ups? | Core NATS, the message bus; a lost signal costs latency, never work |
| What happens to a person's message sent while a run is live? | It folds into that run, which reads it at its next model call |
| What happens when a run asks a person something? | The turn settles `waiting`, and the person's answer writes a new turn |
| How does an answer reach Slack? | The report writes a `thread.deliveries` row, and the outbound worker posts it |
| Can the live view lose frames? | Yes; the client backfills from the turns list |

[Thread model](thread_model.md) owns the tables and their columns.

## Why it exists

Postgres holds every durable fact, and the bus holds none ([datastore scaling](datastore_scaling.md)).
The turns ledger is therefore the queue: a producer hears "accepted" only after its row commits.
Any reader can rebuild from Postgres after a lost signal.

Every producer writes the same row shape, so the assignment and run path never branch on who asked.

## How it behaves

### The path in one picture

```text
Producer      agentsfleetd                 Postgres               Runner      Viewer
   |-- input -->|                              |                     |           |
   |            |-- queued turn -------------->|                     |           |
   |<- accepted-|   commit                     |                     |           |
   |            |-- wake on NATS (best effort) |                     |           |
   |            |-- assign: fence + open turn->|                     |           |
   |            |-- run row ------------------>|                     |           |
   |            |-- Down::Run ---------------------------------------->|           |
   |            |<-- live activity ------------------------------------|           |
   |            |-- frames on NATS ------------------------------------------------>|
   |            |<-- report -------------------------------------------|           |
   |            |-- usage, answer, settle,     |                     |           |
   |            |   release, delivery -------->|  one transaction    |           |
   |            |-- turn_complete ------------------------------------------------->|
   |            |-- delivery ping -> outbound worker -> Slack thread                |
```

No timer re-reads the ledger.
An assignment pass starts on a thread wake, a full recheck after a bus gap or reconnect, or a runner connection that gains free slots.
A wake the bus drops without a gap waits for the next of those events; the turn stays in Postgres.

### Create and recovery

`POST /v1/workspaces/{workspace_id}/fleets {message, operation_id, model_ref?, name?}` needs the `fleet:write` scope.
`afd_setup`'s create writes four rows in one transaction, in this order:

1. The replay receipt, keyed by `operation_id` under `<workspace>:fleet:create:<person>`. It comes first, so two callers with one key create one fleet.
2. The `fleet.fleets` row, `active`, with its name, its resolved `model_ref` and both network toggles off.
3. The `thread.threads` row, whose ID is the fleet's ID.
4. Turn 1, `queued`, with actor `steer:<person>` and no reply destination.

After the commit, the daemon publishes the thread's wake and the workspace's fleet notice.
The answer is 201 with the fleet and `accepted: {turn_id, disposition}`.

| Call | Answer | Rows written |
|---|---|---|
| A first call | 201, `replayed: false` | The receipt, fleet, thread and turn 1 |
| Same `operation_id` and text | 200, same fleet and turn, `replayed: true` | None |
| Same `operation_id`, other text | 409 `UZ-SETUP-005` | None |
| Same `operation_id` after the fleet is deleted | 410 `UZ-SETUP-013` | None |
| No usable model | 409 `UZ-SETUP-008` | None |
| A name the workspace already holds | 409 `UZ-AGT-006` | None |

The create checks no wallet. The first run's assignment does, and an empty wallet refuses the turn `balance_exhausted` while the fleet stays `active`.

A daemon that dies after the commit leaves a queued turn, and the next daemon's boot recheck finds it.
[API resources](api_resources.md) owns the name rule and the wire shapes.

Archiving is `PATCH` with `{status: "archived"}` and the current `If-Match` tag, under `fleet:write`; a stale tag answers 412.
An archived fleet takes no new run, its pending cards are cancelled and its history stays readable.
`DELETE` needs `fleet:admin`, works in any status, releases live work and then purges the fleet.

### Intake

Seven producers write turns. Each commits its row, then attempts a wake on `af.thread.<thread_id>.wake`.

| Producer | Route | Actor | Dedup key | Reply destination |
|---|---|---|---|---|
| `steer` (person) | `POST …/fleets/{fleet_id}/messages` | `steer:<user>` | `<fleet_id>:<operation_id>` | None |
| `steer` (fleet) | `fleet_steer` run verb | `fleet:<sender_fleet_id>` | Target, sender turn and a digest of the text | Copied from the sender's turn |
| `webhook` | `POST /v1/webhooks/{fleet_id}/{source}` | `webhook:<source>` | `{fleet}:{source}:{event_id}` | None |
| `webhook_app` | `POST /v1/ingress/github` | `github-app` | `{fleet}:github:{event_id}` | None |
| `schedule_fire` | `POST /v1/ingress/qstash/schedules` | `cron:<schedule_id>` | `{fleet}:{schedule}:{message_id}`; a `once` schedule uses a fixed word in place of the message ID | None |
| `question_answer` | Approve, Deny or an answer on a card | `continuation:<waiting_turn_id>` | The card's action | Inherited from the waiting turn |
| `durable_task_continuation` | A task's start, input, completion or wake | `durable_task:<task_id>` | Task, generation and wake | Recorded, inherited or none |
| `slack_mention` | `POST /v1/connectors/slack/events` | `slack:<team_id>:<user>` | `<team_id>:<event_id>` | The Slack thread |

The `event_id` of a webhook is the signed body's SHA-256, or the Svix delivery ID for `svix-v1`.
A person's message answers 202 with `{turn_id, disposition}`: `started`, `folded` (with `folded_into_turn_id`) or `queued`.
A fleet's steer computes no disposition and never folds.

Each turn's row ID is its identity across every reassignment and retry.
The actor is metadata for the fleet's instructions and the history filter. The run loop never branches on it.

Webhook routes check a signature and never read the `Authorization` header.
A rejected delivery answers `UZ-WH-020` (misconfigured), `UZ-WH-010` (bad signature) or `UZ-WH-011` (stale timestamp) ([error codes](https://docs.agentsfleet.net/api-reference/error-codes#UZ-WH-020)).
[Connectors](connectors.md#github-app-from-install-to-a-routed-event) owns how an App delivery finds its fleets.

`/v1/webhooks/` and `/v1/ingress/` carry customer events only.
The Clerk sign-up events that create tenants arrive at `POST /v1/auth/identity-events/clerk` ([`../AUTH.md`](../AUTH.md)).

### Schedules

QStash, Upstash's hosted scheduler, owns the clock. The runner owns no timer.
`agentsfleetd` stores each `fleet.schedules` row, pushes every change to QStash before it answers, and receives the fires.

- **Two authors.** A person writes a schedule through the schedule routes (`source` `api`). The fleet writes its own with `cron_add` (`source` `fleet`).
- **A fleet edits only its own.** A fleet holds at most 32 schedules, and at most 16 of its own making (`afd_cron`'s model).
- **A fire.** The daemon checks the signature with the current or next key from the admin workspace's `qstash` bag, then writes one `schedule_fire` turn.
- **Run now.** A person's run-now goes through the same producer, keyed `run:<turn_id>`, so a reassigned run replays it rather than firing twice.
- **A `once` schedule.** It retires after its fire is admitted. A retirement whose claim is held answers `UZ-SCHED-006`, so QStash repeats the fire.
- **A fleet delete.** One transaction holds the fleet row `FOR UPDATE` from the `If-Match` check to the purge.
  A concurrent change waits, so it cannot stale the tag after the schedules are gone.

  On that transaction each schedule is marked `deleting` and removed from QStash (`afd_cron`'s `release_fleet_on`), within the scheduler's call budget.
  Its row goes once QStash agrees, and only then does the fleet row go.
  A schedule QStash keeps, one another syncer holds, or one the budget cut off stays `deleting`.

  The delete then commits the removals that landed, keeps the fleet, and answers `409` `UZ-SCHED-012` with `current_state` `deleting`; repeating it finishes the job.
  The purge refuses a fleet that still holds a schedule (`delete_locked`), so no schedule can cascade away registered.
  A stale `If-Match` is refused before any schedule is touched.

The QStash signature covers the destination URL, the body and the message ID.
It does not cover the `Upstash-Schedule-Id` header, so the daemon checks the named schedule and its fleet against the database.
Every schedule write proves the run again, so a runner that lost its run writes nothing (`UZ-RUN-005`).

### Assignment and the fence

1. Skip a runner whose isolation check is degraded or unreadable.
2. Read each eligible active fleet's oldest unsettled turn through `idx_turns_thread_id_sequence_unsettled`.
3. Prefer fleets with a parked sandbox, recent wakes and the last runner, then the turn that has waited longest. Try at most 64 candidates.
4. Lock the fleet's `thread.threads` row and its oldest turn with `SKIP LOCKED`. A locked oldest turn blocks the turns behind it.
5. In one statement, raise the thread's `fencing_token`, stamp it on the turn as `run_fence`, and open the turn (`received`). The same statement expires a lapsed prior run.
6. Resolve the payer's balance. An empty wallet settles the turn `balance_exhausted` with no run.
7. Build the run's policy from the fleet row and its approved grants, then insert the `thread.runs` row under the new fence.
8. Push `Down::Run` on the runner's stream ([runner protocol](runner_protocol.md)).

Only the assignment writes `fencing_token`, so at most one run is active per fleet.
A runner that disappears leaves the turn unsettled. Once its run expires, the next assignment runs the turn again under a higher fence, with the same turn and billing identities.

The policy is read fresh on every run, so a model change, a toggle or a grant takes effect on the next run.

### Reports and folded messages

The runner reports when the run ends. One transaction, guarded by the host, thread, run and turn fences, records:

- the usage lines no renewal recorded ([billing](billing.md));
- the answer, the turn's terminal status and the thread's `resume_context`;
- the thread's release and any outbound delivery the turn owes.

A repeated report acknowledges the first result and charges nothing again.
A report from a superseded run answers `UZ-RUN-005` and changes nothing. A failed report transaction leaves the turn unsettled.

A message written while the run is live answers `folded`.
At its next model call, the run claims later unsettled messages a person sent in the same conversation, the turns that share its reply destination.

The claim stops at the first task-bound turn or one no person sent, such as a webhook, a schedule fire or a card's answer. That turn runs on its own.

When the run succeeds, each folded message a model call read gets the run's answer on its own turn. A claimed message no call read stays unsettled and runs next.

### Waiting on a person

A run that asks a question, or calls `request_access` for an unapproved credential, ends waiting.
Its turn settles `waiting`, and a card carries the turn, so the fleet's later turns are not held behind it.

The person's answer, Approve or Deny writes a new `question_answer` turn, with actor `continuation:<waiting_turn_id>` and `resumes_turn_id` set.
The waiting row is never reopened, and the continuation actor never nests.

Nothing expires a card. One past its deadline reads `timed_out` and stays answerable ([access](user_flow.md#access)).

A daemon gate that refuses a turn before it runs settles it `gate_blocked`, which is also never reopened.

### Outbound deliveries

The report writes a `thread.deliveries` row only when the turn carries a reply destination.
The row is unique on `(fleet_id, turn_id)`, so a replayed report owes one delivery.

The outbound worker takes rows per lane, one `(provider, workspace_id)` lane at a time.
It claims at start, on an `af.delivery.wake` ping, when one of its deliveries finishes and when another holder's claim lapses. It runs no timed pass.

Delivery is at least once. The poster stamps `delivered_at` only after the provider accepts the post.
A delivery whose attempts all fail, or that its destination refuses for good, is abandoned and never offered again.

[Slack mentions](slack_mentions.md#how-the-answer-comes-back) shows the Slack destination.

### Watching a fleet

A viewer reads a fleet's live frames over Server-Sent Events (SSE) at `GET …/fleets/{fleet_id}/turns/stream`.
The frames come from the NATS subject `af.thread.<thread_id>.items`, which keeps nothing.

```text
Browser            Next.js /live/*            agentsfleetd            NATS
   |-- EventSource -->|                            |                    |
   |  (session cookie)|-- Bearer token ----------->|                    |
   |                  |                            |-- subscribe ------>|
   |<------------------------- hello, catching_up {dropped: 0} --------|
   |<------------------------- frames ------------------------------<--|
   |<------------------------- keep-alive every 15 s ------------------|
   |-- on reconnect or catching_up: GET …/turns?since=<last - 2 s> ---->|
```

The opening `catching_up` covers frames published before the subscription reached every NATS node.
A bus reconnect, a slow consumer or a gap marker sends `catching_up` again. The client then backfills from the turns list and merges by turn ID.

SSE sequence IDs count per connection and reset to 0, and `Last-Event-ID` is ignored.
Clients page the turns list with the server's `next_cursor`, never a cursor built from a turn ID.

The CLI opens the stream before it posts a message and waits up to 2 s for headers.
Frames that arrive before the answer names the turn wait in a small buffer, then replay in order.
[Web app](web_app.md) owns the browser's retry timers and stream registry.

### Tenant isolation

| Layer | How one workspace is kept from another |
|---|---|
| Postgres | `afd_http` checks workspace ownership before every workspace route, and every statement filters by `workspace_id` or `fleet_id`; no row-level security policy exists |
| NATS | Subjects carry the fleet's random thread ID, and every path to a subject passes the ownership check |
| Runner | The runner token authenticates each call, and a run carries one fleet's turn and approved secrets |

## Limits

- No timer re-reads Postgres. Work behind a lost wake waits for the next wake, bus gap recheck or free runner slot.
- A fleet runs one turn at a time. A locked or long-running oldest turn holds every turn behind it.
- A dead runner's turn waits until its run lease expires ([runner fleet](runner_fleet.md)).
- Deliveries are at least once, so a person can see an answer twice. An abandoned delivery is never retried.
- The live view is best effort. Settled turn rows are the record, and token chunks are not saved.
- A schedule fire's schedule header is not signed; the daemon trusts it only after the database check.

## Related pages

- [Fleets](user_flow.md): the person's view of creating, shaping and talking to a fleet.
- [Run flow](run_flow.md): one pull request through this path.
- [Thread model](thread_model.md): the turns, runs, items and deliveries tables.
- [Datastore scaling](datastore_scaling.md): why Postgres holds facts, and the NATS subjects.
- [Concurrency](concurrency.md): the Postgres pool and statement bounds.
- [Runner protocol](runner_protocol.md): the runner's stream, renewal and reconnect.
- [Durable tasks](durable_tasks.md): tasks that continue across runs through this ledger.
