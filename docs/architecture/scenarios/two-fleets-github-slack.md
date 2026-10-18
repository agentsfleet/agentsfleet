# Scenario — Two fleets, end to end: a Pull Request, a Slack thread, and a steer

> Parent: [`README.md`](./README.md) · References: [`../data_flow.md`](../data_flow.md) (the canonical trigger → execute → watch sequence), [`../lease_flow.md`](../lease_flow.md) (one lease in detail), [`../runner_fleet.md`](../runner_fleet.md) §"Datastore topology" (the readiness index and the doorbell), [`../datastore_scaling.md`](../datastore_scaling.md) §"Target design", [`github-pr-reviewer.md`](./github-pr-reviewer.md), [`slack-incident-responder.md`](./slack-incident-responder.md).
>
> One workspace, two fleets, three ways in. Each part is one picture of one step, with the rules it depends on linked rather than copied: when a picture and a linked section disagree, the linked section wins and this page is stale.

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
| **Stream** | A fleet's inbox, `fleet:{id}:events`; its jobs wait there oldest first. |
| **Ready mark** | A note on one of sixteen shelves, `fleet:ready:{p}`, saying "this fleet has work". |
| **Doorbell** | A ring on a shelf's channel, `fleet:ready:{p}:wake`, so the next poll also peeks that shelf instead of waiting for the rotation to reach it. Each replica keeps what it heard as one bit per shelf (`afd_dragonfly::Doorbell`). |
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

Saving with start `none` runs nothing. It writes the fleet and its setup down, asks for its grants and builds its empty inbox; a fleet stays still until something sends it a job. Canonical: [`../data_flow.md`](../data_flow.md) §"A. SAVE" and C1 in [`../api_resources.md`](../api_resources.md).

```
  you: agentsfleet chat --library github-pr-reviewer --revision <id>
       review the setup, then save it                 (and again for fleet B)
   │
   │  POST /v1/workspaces/{ws}/fleets  {draft candidate, review_digest, start: none}
   ▼
  ┌──────────────────────────── agentsfleetd ────────────────────────────┐
  │                                                                      │
  │  1. refuse a stale review: the digest must match the setup now       │
  │  2. Postgres  INSERT fleet.fleets status=installing                  │
  │               + setup revision 1 (description, instructions, config) │
  │               (tenant comes from the workspace row, never the caller)│
  │  3. readiness: model, wallet, credentials by NAME only, connections, │
  │     grants; a missing grant is requested:                            │
  │     Postgres  INSERT connector.integration_grants (fleet, "github")  │
  │               pending  ← a person approves it on the setup check     │
  │  4. Dragonfly XGROUP CREATE MKSTREAM fleet:{id}:events fleet_lease   │
  │               ← the inbox and its "lease" reader group, once every   │
  │               check is ready (bounded at 5 s; failing, it waits)     │
  │  5. Postgres  locked recheck: installing → active                    │
  │               a pending check keeps it installing; the same Save,    │
  │               or PATCH status active, retries the checks             │
  │                                                                      │
  └──────────────────────────────┬───────────────────────────────────────┘
                                 ▼
                    201 Created  (status active or installing)
```
  Postgres                                   Dragonfly
  ────────                                   ─────────
  fleet.fleets  (active)    ✔                 fleet:{A}:events  (empty) + group ✔
  core.fleet_definition_                     fleet:{B}:events  (empty) + group ✔
    revisions (revision 1) ✔
  connector.integration_grants  ✔ approved before the fleet went active
  fleet.admissions    ✗  (no work yet)
  fleet.events        ✗                 fleet:{id}:activity  ✗ (exists from
  runner.leases      ✗                                       its first PUBLISH)
  fleet.sessions      ✗  (the first report writes it)
```

## 2. Trigger one: a Pull Request wakes fleet A

Canonical: [`../lease_flow.md`](../lease_flow.md) §"Ingress: how a PR on `agentsfleet/linkwarden` finds the fleet" and §"Admission: the row is the acceptance, the stream entry is a receipt"; [`../connectors.md`](../connectors.md) §"Repository and event subscriptions belong to fleets".

```
  GitHub  ── "pull_request opened" (signed body) ──►  POST /v1/ingress/github
                                                          │
  ┌───────────────────────────── agentsfleetd ────────────▼──────────────────┐
  │                                                                          │
  │  a. check the Hash-based Message Authentication Code (HMAC) against      │
  │     the vault's "github-app".webhook_secret BEFORE reading the body.     │
  │     Bad → UZ-WH-010.  No Bearer fallback, ever.                          │
  │                                                                          │
  │  b. installation.id ──► connector.installs ──► workspace "indy-ws"       │
  │                                                                          │
  │  c. shrink GitHub's payload into a 12-field digest:                      │
  │     action, repo, number, title, url, state, draft, author,              │
  │     head_ref, base_ref, head_sha, received_at                            │
  │                                                                          │
  │  d. who listens?  every fleet in the workspace that is                   │
  │       active  ∩  holds an approved github grant                          │
  │               ∩  TRIGGER repositories ∋ agentsfleet/linkwarden           │
  │               ∩  TRIGGER events ∋ pull_request                           │
  │     ──► fleet A   (fleet B listens to Slack, so it does not match)       │
  │     (no `repositories` list = listens to NOTHING; fail-closed)           │
  │                                                                          │
  │  e. per matching fleet, ADMIT:                                           │
  │       INSERT fleet.admissions                                            │
  │         producer = webhook_app                                           │
  │         key      = "{fleetA}:{sha256(body)}"   ← one body, one job       │
  │         reply    = NONE   ← nowhere to answer back to (part 9)           │
  │       XADD fleet:{A}:events *  actor=github-app  type=webhook  ...       │
  │       UPDATE admission SET receipt = <stream entry id>                   │
  │       HSET fleet:ready:{pA}           ← the note on A's shelf            │
  │       SPUBLISH fleet:ready:{pA}:wake  ← ring A's shelf                   │
  │                                                                          │
  └──────────────────────────────────────────────────────────────┬───────────┘
                                                                 ▼
                                      202 { matched: 1, enqueued: 1 }  (to GitHub)
```

GitHub gives this path ten seconds and never retries on its own, which is why the row comes first and everything after it is a receipt.

## 3. Trigger two: a Slack mention wakes fleet B

Under GitHub's "run 123 failed" announcement in `#ci-dev`, someone types `@agentsfleet-dev why did this fail?`. Canonical: [`slack-incident-responder.md`](./slack-incident-responder.md) §"4. Routing a mention" and §"5. What the fleet is told".

```
  Slack ── signed app_mention ──► POST /v1/connectors/slack/events
                                     │
  ┌──────────────────────────────────▼───────────────────────────────────────┐
  │ 1. FILTER   app_mention only; drop bots, edits, and the bot's own posts  │
  │                                                                          │
  │ 2. ROUTE    team_id ──► connector.installs ──► workspace "indy-ws"       │
  │             who subscribes to THIS channel?                              │
  │                                                                          │
  │       ┌─────────────┬──────────────────┬─────────────────────────┐       │
  │       │ nobody      │ exactly one      │ several                 │       │
  │       ▼             ▼                  ▼                         │       │
  │   the channel's   that fleet       "@agentsfleet-dev <name> …"   │       │
  │   resident        ──► fleet B       → the named one              │       │
  │   (no tools)                        no name → ONE notice listing │       │
  │                                     the names, no model runs     │       │
  │                                                                          │
  │ 3. READ THE THREAD  conversations.replies: the parent (GitHub's          │
  │    announcement, carrying the run link) + the latest replies,            │
  │    ≤ 20 messages, each capped, 1.5 s deadline, packed into ONE           │
  │    `message` under a heading that marks it UNTRUSTED data.               │
  │    A failed read → the mention alone, and the message says so.           │
  │                                                                          │
  │ 4. ADMIT                                                                 │
  │      INSERT fleet.admissions                                             │
  │        producer = slack_mention                                          │
  │        key      = "{team_id}:{slack event_id}"  ← a Slack retry is the   │
  │                                                   same job               │
  │        reply    = slack {team_id, channel_id, thread_ts}  ★ remembered   │
  │      XADD fleet:{B}:events *  actor=slack:<user>  ...                    │
  │      receipt, HSET fleet:ready:{pB}, SPUBLISH fleet:ready:{pB}:wake      │
  └──────────────────────────────────────────────────────────────────────────┘
```

The ★ is the difference from part 2: the admission remembers where the answer must go, and that address travels with the job to part 9.

## 4. Trigger three: a person steers a fleet

A steer is you typing to a fleet. It enters through the same door as every other trigger. Canonical: [`../data_flow.md`](../data_flow.md) §"Steer flow end-to-end" and §"B. TRIGGER".

```
  you: agentsfleet steer <fleetA> "also check the migration file"
   │
   │  (the CLI first opens the live tail, GET …/events/stream, and waits ≤2 s,
   │   so early frames are not missed)
   │  POST /v1/workspaces/{ws}/fleets/{A}/messages
   ▼
  ┌──────────────────────────── agentsfleetd ────────────────────────────┐
  │  INSERT fleet.admissions      producer=steer                         │
  │  XADD fleet:{A}:events *           actor=steer:kishore  type=chat    │
  │  receipt, ready mark + doorbell ring                                 │
  │  SPUBLISH fleet:{A}:activity       {kind:"event_admitted", message}  │
  │                                    ← every open screen shows the     │
  │                                      message as waiting at once      │
  └──────────────────────────────┬───────────────────────────────────────┘
                                 ▼
          202 { status, event_id, replayed }   ← the CLI filters frames by event_id
```

Six producers (`afd_admission::Producer`: steer, webhook, webhook_app, schedule_fire, gate_continuation, slack_mention) write one envelope into one stream:

```
  ┌──────────────────────── the envelope, every job ───────────────────────┐
  │ actor         steer:<user> | github-app | slack:<user> | cron:<id> |   │
  │               continuation:<original>                                  │
  │ type          chat | webhook | cron | continuation                     │
  │ workspace_id  <uuid>                                                   │
  │ request       <the message or the digest JSON>                         │
  │ created_at    <RFC 3339>                                               │
  │ event_id      <admission id>       ← minted by the ADMISSION ROW       │
  └────────────────────────────────────────────────────────────────────────┘
       The agent never branches on actor; it is metadata for the instructions and history.
```

## 5. The waiting room: inboxes, shelves and doorbells

Fleet A now holds two jobs (the PR and the steer) and fleet B one. A fleet's shelf is its id's Cyclic Redundancy Check (CRC16) modulo sixteen. Canonical: [`../runner_fleet.md`](../runner_fleet.md) §"Datastore topology"; the scale limits of this picture are in [`../datastore_scaling.md`](../datastore_scaling.md) §"Target design" ("Time order, the doorbell's bits, and scale").

```
                      DRAGONFLY  (four-node cluster)
  ┌────────────────────────────────────────────────────────────────────────┐
  │                                                                        │
  │  INBOXES (one per fleet, durable, oldest first)                        │
  │    fleet:{A}:events   [ PR opened ] [ steer "check migration" ]        │
  │    fleet:{B}:events   [ slack mention ]                                │
  │                                                                        │
  │  SHELVES (readiness index: 16 hashes; a fleet's shelf = CRC16(id) mod  │
  │  16; one field per fleet, no order between fields)                     │
  │    fleet:ready:{3}  { A: token }        ← say fleet A hashes to 3      │
  │    fleet:ready:{9}  { B: token }        ← and fleet B to 9             │
  │                                                                        │
  │  DOORBELLS (16 sharded pub/sub channels, one per shelf)                │
  │    fleet:ready:{3}:wake  ── ring ──┐                                   │
  │    fleet:ready:{9}:wake  ── ring ──┤                                   │
  └────────────────────────────────────┼───────────────────────────────────┘
                                       │  every agentsfleetd replica listens
                                       ▼  to all sixteen on its hub connection
        ┌───────────────── each agentsfleetd replica ─────────────────┐
        │  Doorbell::listen: sixteen tasks, one per channel           │
        │  a message on channel p → fetch_or(1 << p)                  │
        │  a gap or a lag         → counted kind="gap", sets nothing  │
        │                                                             │
        │  AtomicU16:  bit  0 1 2 3 4 5 6 7 8 9 A B C D E F           │
        │                   . . . 1 . . . . . 1 . . . . . .           │
        └─────────────────────────────────────────────────────────────┘
```

The inbox holds the jobs. The note says where to look. The ring says look there too, on the next poll; the rotation's own shelf still comes first. A lost ring costs the fleet the rotation's speed, never its work.

## 6. Pickup: two runners, two leases

Canonical: [`../lease_flow.md`](../lease_flow.md) §"The poll: ready index → claim → event → row"; [`../data_flow.md`](../data_flow.md) §"C. EXECUTE".

```
  runner R1 ── POST /v1/runners/me/leases (Bearer agt_r) ──►┐
  runner R2 ── POST /v1/runners/me/leases ─────────────────►┤
                                                            ▼
  ┌──────────────────── agentsfleetd: Leases::select ──────────────────────┐
  │                                                                        │
  │ R1's poll:  rotation → shelf 2 (empty)  ∥  doorbell → first rung ≥2 = 3│
  │             take bit 3 ── peek shelf 3 (≤100 ms) → [A]                 │
  │                                                                        │
  │ R2's poll:  rotation → shelf 3          ∥  doorbell → first rung ≥3 = 9│
  │             take bit 9 ── peek shelf 9 → [B]                           │
  │             (consecutive polls take DIFFERENT rung shelves)            │
  │                                                                        │
  │ then, in Postgres, for each:                                           │
  │  a. (checked before the peeks above) a fleet whose held sandbox the    │
  │     runner names goes first, any shelf; neither runner holds one here  │
  │  b. SELECT_READY_CANDIDATES ≤64: active, slot free, runner has the     │
  │     labels; rotation's shelf first, then the last runner, then random  │
  │  c. CLAIM runner.fleet_slots    ← ONE active lease per fleet           │
  │       fencing_token + 1, lease_expires_at = now + 30 s                 │
  │  d. the fleet's oldest job: pending first (XAUTOCLAIM), else new       │
  │     (XREADGROUP >). A gets "PR opened"; the steer waits behind it      │
  │  1. INSERT fleet.events  status=received  + stamp delivered_at         │
  │  2. SPUBLISH fleet:{id}:activity {event_received}                      │
  │  3. GATES: payer → balance → budget → receive debit → approval         │
  │     refused → status=gate_blocked, terminal, never reopened            │
  │  4. vault: secrets_map; the provider key apart (never in secrets_map); │
  │     ExecutionPolicy: tools, network.allow, per-host rules              │
  │  5. INSERT runner.leases  status=active, fencing_token, expiry         │
  │                                                                        │
  │ settle the rung shelf: rung again or left clear, by the rule in        │
  │ runner_fleet.md, its Datastore topology table                          │
  └────────────────────────┬─────────────────────────────┬─────────────────┘
                           ▼                             ▼
             R1 ◄── 200 { lease A }          R2 ◄── 200 { lease B }
                 lease_id, fencing_token, event{digest}, policy,
                 instructions = the setup's instructions_md
```

**One active lease per fleet.** The steer cannot run beside the PR job: the affinity slot is held. It waits in A's inbox until A's report frees the slot, and the next poll takes it.

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

Canonical: [`../data_flow.md`](../data_flow.md) §"Steer flow end-to-end" (writes 7–15 and why they share a transaction).

```
  runner ── POST /v1/runners/me/reports { lease_id, fencing_token, outcome, … } ──►
  ┌─────────────────────────────── agentsfleetd ───────────────────────────────┐
  │                                                                            │
  │  ╔═════════════════ ONE Postgres transaction ═════════════════╗            │
  │  ║  claim_and_settle: lease active AND token ≥ fencing_token? ║            │
  │  ║     no  → UZ-RUN-005 (a stale runner), nothing changes     ║            │
  │  ║     yes → lease status = reported                          ║            │
  │  ║  7.  fleet.events  status=processed, response_text         ║ ← history  │
  │  ║  9.  billing.fleet_charges  reconcile actual tokens        ║ ← money    │
  │  ║  10. fleet.sessions  UPSERT context_json                   ║ ← bookmark │
  │  ║  12. release the affinity slot (token-guarded)             ║ ← next job │
  │  ║  13. fleet.obligations INSERT (receipt and delivered_at    ║ ← ONLY when│
  │  ║      NULL)  "we owe this answer"                           ║   a reply  │
  │  ╚════════════════════════════════════════════════════════════╝   was      │
  │                                                                 remembered │
  │  after the commit (no transaction spans Postgres and Dragonfly):           │
  │   8.  SPUBLISH fleet:{id}:activity {event_complete}                        │
  │   11. XACK fleet:{id}:events          ← the job leaves the inbox           │
  │   14. XADD connector:outbound          (only with an obligation)           │
  │   15. obligation.receipt = <entry id>                                      │
  └────────────────────────────────────────────────────────────────────────────┘
```

Fleet A skips write 13: its admission said `reply = NONE`. Fleet B's writes the obligation, addressed to the thread.

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
   receipt IS NULL             → committed, never queued    (a crash between 13 and 14)
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

```
 time ─────────────────────────────────────────────────────────────────────────────►

 GitHub   PR opened
            │ admit A#1, mark shelf 3, ring 3
 Slack            mention in #ci-dev
                    │ admit B#1 (reply = thread), mark shelf 9, ring 9
 You                    steer A "check migration"
                          │ admit A#2 (no reply), event_admitted frame
 R1  poll ─► takes ring 3 ─► lease A#1 ──── run (GET diff ✔, review POST ✗) ──► report A#1
 R2      poll ─► takes ring 9 ─► lease B#1 ── run (logs, Loki) ── interim line ─► report B#1
                                                                                │
 outbound                                                       post into thread ◄┘
 R?  poll … poll (A's slot held, A#2 waits) ……………… slot freed ─► lease A#2 ─► run ─► report
 You  ◄─── the live tail shows every frame; A#2's answer arrives on the CLI's stream
```

**A runner that dies mid-job:** its lease expires 30 s after its last renew, and the next poll **reclaims** the job: the same history row, a new lease under a higher fencing token. A late report from the dead runner is refused with `UZ-RUN-005`, so history stays one row and billing one `billing.fleet_charges` row ([`../data_flow.md`](../data_flow.md) §"C. EXECUTE").
