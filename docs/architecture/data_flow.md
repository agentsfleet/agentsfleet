---
type: explanation
audience: contributor
verified: 2026-10-21
product_version: 0.62.2
executable: false
---

# Data Flow — how an event moves through the system

> Parent: [`README.md`](./README.md) · Sibling: [`runner_fleet.md`](./runner_fleet.md) (the structural split this flow runs on). · User-facing: [docs.agentsfleet.net/fleets/webhooks](https://docs.agentsfleet.net/fleets/webhooks) (sending an event) and [docs.agentsfleet.net/fleets/running](https://docs.agentsfleet.net/fleets/running) (watching one).

> **Scope:** `agentsfleetd` owns admission, assignment and settlement. `agentsfleet-runner` runs the agent loop and tools, then reports over HTTPS.

> See [runner processes](./runner_execution.md#process-model) for the supervisor and sandbox boundary.

Read this when you need to know where a webhook, a steer, or a cron fire ends up. Many specs reference this file as the canonical picture of the runtime.

## Facts

Every row is extracted from the sections below; the owner column names the section that carries the full story.

| Invariant | Value | Mechanism | Owner section |
|---|---|---|---|
| Event ingress | Six producers, one queue | `Producer` names `steer`, `webhook`, `webhook_app`, `schedule_fire`, `gate_continuation` and `slack_mention`. Each commits `fleet.admissions` and attempts a wake notification. | §B. TRIGGER |
| Queue ownership | Postgres | Admission, claim and settlement use the same ledger row. Dragonfly carries notifications and outbound replies. | §Steer flow end-to-end |
| Durable stores | 5 tables, join key `(fleet_id, event_id)` | `fleet.admissions` (one row per acceptance, UNIQUE `(producer, producer_key)`) · `fleet.events` (one row per delivery) · `fleet.obligations` (one row per answer, UNIQUE `(fleet_id, event_id)`) · `fleet.sessions` (one row per fleet, UPSERT) · `billing.usage_lines` (one row per measured slice of a lease, UNIQUE `(work_type, work_id, seq)`, with the lease's money held in `billing.holds`) | §The five durable stores |
| Replay safety | Stable producer and event identities | Admission deduplicates `(producer, producer_key)`; redelivery retains the event and billing identities. | §C. EXECUTE |
| Stale-writer rejection | `UZ-RUN-005` | Slot, lease and admission fences guard the report transaction. | §C. EXECUTE |
| Shared Dragonfly handle | One shared cluster command owner per daemon | Outbound stream commands, wake notifications and live activity. Inbound queue reads use Postgres. | §Connection topology |
| Dedicated Dragonfly owners | hub + outbound reader | Refcounted `SSUBSCRIBE`; blocking outbound reads use a separate cluster owner | §Connection topology |
| Postgres acquire failures | 2 distinct errors | `PoolTimeout` (capacity) vs `PoolUnavailable` (datastore); one connection per handler (RULE CNX) | §The Postgres pool |
| Config freshness | read per lease | a `PATCH` takes effect on the next lease; no cache, no signal | §Config reload |
| Gate-blocked rows | terminal | never reopened; the resolved gate lands a NEW row via `actor=continuation:<original>` | §"C. EXECUTE" step 3 |
| Activation guarantee | Checked activation and initial admission commit together | Run-now acceptance follows the committed admission; a lost wake cannot strand it. | §A. SAVE |
| Server-Sent Events (SSE) sequence ids | not durable | per-connection counter, resets to 0; `Last-Event-ID` ignored; backfill via the events list | §D. WATCH |
| Client gap recovery | reconnect, or `catching_up` on either stream | bounded `fleet.events` list `since` last delivery − 2 s overlap, merged by event id; a lost server subscription arrives as `catching_up` with `dropped: 0` | §Two streams + one pub/sub channel |
| Cron authority | QStash | signature verified at ingress; replay suppressed atomically; the runner owns no timer | §B. TRIGGER |
| Cancel latency | none: a running lease is never cancelled | kill and pause write only `fleet.fleets.status`; the run ends on its own, at its fleet's stored budget ceiling, or at `MAX_RUNTIME` | [`runner_fleet.md`](./runner_fleet.md) §"Steer, kill, pause" |
| Lease ownership | at most one active lease per fleet | atomic `runner.fleet_slots` claim + monotonic `fencing_token` | §One active lease per fleet |
| Provider `api_key` | never in `secrets_map` | rides `ExecutionPolicy.provider` + `.api_key`; injected for the inference call only | §"C. EXECUTE" step 4 |
| Tenant isolation | application-enforced + namespacing | every workspace route passes an ownership check before its handler runs; Dragonfly keys namespaced by unguessable fleet UUID. This repository declares no `ROW LEVEL SECURITY` policy | §Multi-tenancy boundary |

## Traps

Each trap is enforced in its owner section; this list is the index.

- The live tail is the eyeballs surface, not the audit surface; durable history is `fleet.events` (§Two streams + one pub/sub channel).
- A connection held across a blocking `SSUBSCRIBE` can never return to a pool (§Connection topology).
- Never acquire a second Postgres connection while holding one — that is how a pool deadlocks (§The Postgres pool).
- The Postgres pool has no ordering or fairness guarantee; do not assert one (§The Postgres pool).
- `gate_blocked` rows are NEVER reopened (§"C. EXECUTE" step 3).
- A Pull Request reviewer cannot post a review or a comment: no egress write rule admits one ([`lease_flow.md`](./lease_flow.md) §"4. How AGENT BOB 01 can reply").
- Nothing coalesces per Pull Request: every push is its own admitted event, queued behind the fleet's running lease ([`lease_flow.md`](./lease_flow.md) §"5. The PR is fixed and pushed again").
- Preserve the admission row's event identity across claims and retries (§B. TRIGGER).
- The continuation actor is FLAT — it never re-nests `continuation:` (§B. TRIGGER).
- `repositories` is required for GitHub App traffic; omission means no delivery, never every repository (§B. TRIGGER).
- No Bearer fallback on webhook routes; the `Authorization` header is never consulted there (§B. TRIGGER).
- Clients never derive a cursor from an event id; SSE sequence ids have no cross-connection meaning (§D. WATCH).
- The reasoning loop never branches on actor — actor is metadata (§B. TRIGGER).
- `/v1/webhooks/` and `/v1/ingress/` are customer-data-plane only; Clerk identity events live in the auth plane (§B. TRIGGER).
- The coding fleet never becomes the Fleet runtime and never sees its tokens (§The coding fleet and the Fleet runtime).

## Topology

The diagrams live with their flows — each is the section's proof, so none is duplicated here:

- coding fleet vs Fleet runtime — §The coding fleet and the Fleet runtime
- the admission, claim and settlement boundaries in §Steer flow end-to-end
- the Dragonfly connection topology — §Connection topology
- save, trigger envelope, execute, watch — §"End-to-end sequence" — A through D
- the setup failure window — §The install failure scenario, visually

The same sequence told for two fleets at once, a Pull Request and a Slack thread, one picture per step: [`scenarios/two-fleets-github-slack.md`](./scenarios/two-fleets-github-slack.md).

## Decisions

| Decision | Reason | Where / artifact |
|---|---|---|
| A narrative table and a usage table (`fleet.events` + `billing.usage_lines`) | different write authorities and retention rules | §The five durable stores |
| `fleet:control` removed | no per-fleet threads left to orchestrate | §Two streams + one pub/sub channel |
| Dedicated Dragonfly tier collapsed | idle cost now tracks lease-poll frequency, not fleet count | §Connection topology; M80_002 |
| A pool acquire answers a typed error, not an absent connection | `PoolTimeout` and `PoolUnavailable` are different operator pages | §The Postgres pool |
| Gap recovery is client-side, not server resume | no channel or frame-shape change; the durable table is the recovery source | §Two streams; M122 |
| QStash owns the clock | the runner owns no schedule timer | §B. TRIGGER |
| Private drafts own authoring | Native JSON can initialize a draft; GitHub/template-bundle import and Markdown export remain unsupported | [`user_flow.md`](./user_flow.md) §"§8.1 Authoring the Fleet" |
| Retained setup identity | Retry resumes missing checks under the original creation identity | §The install failure scenario |
| Browser SSE uses the authenticated Next.js proxy | the server resolves the session and forwards a Bearer; the daemon authenticates Authorization ([AUTH](../AUTH.md)) | §D. WATCH |
| Outbound answers ride a generic `connector:outbound` stream | the report path stays provider-agnostic (Invariant 9) | §C. EXECUTE; M106 |

---

## Detail

Everything below is the full reference. One event is told twice, at two
zoom levels, and they do not repeat each other — read the one that matches your
question. §"Steer flow end-to-end" draws the path as boxes, so you can see where
a call goes. §"End-to-end sequence" (A INSTALL → D WATCH) states what
each step must guarantee.

Headings are stable — specs cite them by text; insert new sections, never rename existing ones.

## Process and stream ownership at a glance

| Process | Role |
|---|---|
| **`agentsfleetd`** (`agentsfleetd serve`) | Owns Postgres, Dragonfly, the Vault and HTTP assignment. Producers commit admissions; lease polls claim ledger rows; reports commit results, usage and settlement together. The daemon publishes live activity and delivers outbound replies. Authoring model calls use daemon-held keys. |
| **`agentsfleet-runner`** (host-resident daemon) | The execution plane. Boots from an operator-installed `agt_r` token (env `AGENTSFLEET_RUNNER_TOKEN`, no self-register — Option B), then loops `heartbeat → lease → execute → report → activity` over HTTPS carrying that `agt_r` token. Holds **zero datastore credentials**. A trusted supervisor runs the agent loop and holds the model key. A lease that needs one gets a sandbox (bubblewrap, Landlock, seccomp, cgroups) that runs tool calls and nothing else. The sandbox gets a network namespace of its own unless the runner's network policy is `allow_all`. Credential substitution happens in the supervisor at send time ([`runner_execution.md`](./runner_execution.md#process-model)). The supervisor forwards activity frames to `agentsfleetd` over the `activity` verb. |

Streams and tables: §"Two streams + one pub/sub channel" and §"The five durable stores: who owns what".

---

## The coding fleet and the Fleet runtime

Two distinct things are in play. Keeping them straight is essential to understanding the architecture:

```
┌──────────────────────────────────┐         ┌───────────────────────────────────┐
│  CODING AGENT (laptop)           │         │  FLEET RUNTIME (host)             │
│                                  │         │                                   │
│  Claude Code / Amp / Codex /     │         │  agentsfleet-runner's agent loop, │
│  OpenCode driving agentsfleet    │         │  tool calls in a per-lease        │
│                                  │         │  sandbox (bubblewrap, Landlock,   │
│  This is what the human types    │         │  seccomp, cgroups, netns;         │
│  into. Ephemeral.                │         │  persists across laptop close)    │
└──────────────────────────────────┘         └───────────────────────────────────┘
```

The coding fleet is a workstation tool driving `agentsfleet`. The Fleet runtime — the product object the user creates — runs as the runner's agent loop, with its tool calls in the lease's sandbox. The coding fleet never becomes that runtime and never sees its tokens — they communicate only through the steer endpoint, the event stream, and the events history.

## Steer flow end-to-end

This view shows transaction boundaries. The protocol details live under §"C. EXECUTE".

```text
person or provider
       |
       v
admission transaction -> fleet.admissions (stable event identity)
       | commit
       +-> wake notification (best effort)
       |
       v
lease poll -> indexed oldest unsettled admission per eligible fleet
       |
       v
claim transaction -> slot lock + exact head lock + matching fences
       | commit after decoding
       v
delivery transaction -> event history + delivered_at under current fences
       |
       v
runner lease -> supervisor and sandbox -> terminal report
       |
       v
report transaction -> usage + terminal history + session checkpoint
                  -> admission settled + slot released + outbound obligation
       | commit
       +-> live activity
       +-> connector:outbound -> connector thread
```

A lost notification changes priority only. Every idle lease poll probes Postgres.
An expired slot exposes the same unsettled admission with its original event and billing identities.

Outbound delivery retains a separate stream and durable `fleet.obligations` rows.
An obligation starts with no `receipt`; the outbound append records one, and successful delivery stamps `delivered_at`.
The outbound rebuild pass retries undelivered obligations after stream loss.

The poster stamps `delivered_at` only after successful delivery. Acknowledging an exhausted outbound job must not mark its answer delivered.

## The five durable stores: who owns what

These five tables answer different questions. Admissions record accepted work; events describe each run; obligations record replies still owed.
Sessions hold the resume checkpoint, and usage lines record measured work.

The admissions ledger is the inbound queue. Outbound obligations use Dragonfly for delivery and retain receipts for recovery.

Read the primary keys and the shape falls out:

```
  fleet.admissions   PK id  + UNIQUE (producer, producer_key)   one row per ACCEPTANCE
  fleet.events       PK id  + UNIQUE (fleet_id, event_id)       one row per EVENT
  fleet.obligations  PK id  + UNIQUE (fleet_id, event_id)       one row per ANSWER
  fleet.sessions     PK fleet_id                                one row per FLEET
```

Three of them grow with traffic. `fleet.sessions` does not — it is a cursor, not a log, which is why "where does this fleet resume" is a primary-key lookup rather than a sort over its history.

One event's life across all four:

```
  a message arrives
      │
      ├─▶ fleet.admissions   INSERT  "accepted"          before the producer hears yes
      │                                                  dedupes the PRODUCER's retry
      ├─▶ fleet.events       INSERT  status='received'   the narrative opens
      │                                                  dedupes REDELIVERY
      │   ┌── a runner executes ──┐
      │   │                       │
      ├─▶ fleet.events       UPDATE  status='processed', response_text
      ├─▶ fleet.obligations  INSERT  "owed"              ← same transaction as the money
      ├─▶ fleet.sessions     UPSERT  context_json        the resume cursor moves
      │
      └─▶ fleet.obligations  UPDATE  delivered_at        a person received it
```

The two ledgers are not symmetric, and the asymmetry is **who can retry**. A producer holds its own `producer_key` and repeats it, so an admission deduplicates on the PRODUCER's identity. A destination has no such key — a chat provider cannot tell us "this is the same message" — so an obligation deduplicates on the event that produced the answer, the only stable identity this side owns.

An admission failure remains visible to its producer, which can retry the same identity. Uncertain obligations are sent again because delivery is at-least-once. People can recognize and recover from duplicate replies; a missing answer leaves no visible record.

`fleet.events` is also the only one of the four carrying a `status` TEXT column rather than NULL tests. That is not a lapse from the rule the ledgers follow: an event has a genuine vocabulary — `received`, `processed`, `fleet_error`, `gate_blocked`, `balance_exhausted` and the rest — where a delivery has exactly two facts. The spellings live in `afd_core::event::status` and are never literals in schema (RULE STS). Asking "is it still `received`" is how the redelivery path tells a legitimate re-poll from an event that already ran.

| Table | Cardinality | Mutability | Answers |
|---|---|---|---|
| `fleet.admissions` | **One row per acceptance** | INSERT; update `lease_fence`, `parked_at`, `delivered_at` and `settled_at` | Accepted work, its current ownership and completion. `UNIQUE (producer, producer_key)` deduplicates producer retries. |
| `fleet.obligations` | **One row per answer** | INSERT in the report's transaction, then UPDATE `receipt` and `delivered_at` | "Do we still owe somebody this answer?" — committed with the money and the result, so no window exists where a run is charged and its answer exists nowhere. `UNIQUE (fleet_id, event_id)` makes a replayed report owe one delivery, not two. |
| `fleet.sessions` | **One row per Fleet** | UPSERT — replaced at each report | "Where did this Fleet leave off?" — the resume bookmark, and only that. The lease path reads it with the fleet row and hands it to no lease; the report path replaces it. "Which fleet is executing right now" is `runner.leases`, which has the fencing token and the expiry that make the answer trustworthy. |
| `fleet.events` | **One row per delivery** | INSERT (status=`received`) → UPDATE (status=`processed` \| `fleet_error` \| `gate_blocked`) | "What did this Fleet do for event X? Who triggered it, what did they ask, what did it answer, did the gates pass?" — the user's narrative log. The single source of truth for the Events tab and `agentsfleet events`. |
| `billing.usage_lines` | **One row per measured slice** of a lease: each renewal and the report log the runtime and tokens since the last slice, priced. UNIQUE `(work_type, work_id, seq)`, where the work is the lease. The lease's money is held in `billing.holds` and taken from `billing.wallets` by the biller after commit (billing §3, §11). | INSERT in the lease's own transaction; `settled_at` is stamped once the biller has charged the line, and nothing else changes. | "What did event X's run use, and what did it cost?" — billing audit and the source of fleet budgets. Joinable to `fleet.events` via `(fleet_id, event_id)` — NOT `event_id` alone, which is not unique across fleets. |

Why a narrative table and a usage table (`fleet.events` + `billing.usage_lines`) instead of one? They have different write authorities and retention rules:

- `fleet.events` holds user-readable strings (`request_json`, `response_text`) — large, mutable mid-lifecycle, deletable on tenant offboarding.
- `billing.usage_lines` holds numeric audit columns — small, priced once and never repriced, retained for billing reconciliation independent of whether the conversation row is purged; it has no foreign keys, so a line outlives the fleet, workspace and price row it names.

Durable lease bookkeeping (`runner.leases`, `runner.fleet_slots`) is a fourth concern. It records ownership: which runner holds an event, its fencing token, and its expiry. This bookkeeping lives in the `runner` schema and never carries user strings.

### The list read and the detail read are different reads

A page is up to two hundred rows, and the two body columns are unbounded — a trigger payload and a full agent answer. Selecting them per row bought a table that renders about a hundred and sixty characters per cell, so the list stops selecting them and the surfaces that used to quote an answer now state an outcome instead. A row that failed says why; a row that succeeded records that it did, without reproducing the reply. Postgres already keeps wide values in oversized-attribute storage, so the cost was never that the bodies existed — it was that the list **selected** them, which is why this is a read-path change and not a storage one.

The events table and fleet header use summaries. The chat transcript reads bodies through `GET /messages`; individual event detail remains a separate read. See [watch recovery](#d-watch--user-side-how-the-live-tail-surfaces) for reconnect behavior.

`tool_calls`, the run's bounded tool trace (at most 200 calls and 64 KiB; [Runner Fleet, Live activity](./runner_fleet.md#live-activity-the-sse-tail)), is a third body column under the same rule. The transcript and the single-event read select it, and the list never does. A row from before the column existed holds `NULL`, which reads as "not recorded", never as "no tools ran".

A call's full arguments and output are a fourth body that no page read selects. They live in `fleet.tool_calls` (`schema/1710_fleet_tool_calls.sql`), a child table of `fleet.events`, one row per saved call, cascading on `(fleet_id, event_id)`. The rows are keyed by the lease's fencing token and the runner's call number, and a page never reads them: "show all" reads one call through `GET …/events/{event_id}/tool-calls/{call_id}`. That keeps a thread page at its 512 KiB budget however much a run's tools returned.

Runner leases keep no duplicate request body. Claim and reclaim read the payload from the admission row and join fleet events to identify prior leases. Settled leases can outlive deleted fleets or events; their identifiers then remain opaque history.

**Partitioning is not done here.** A usage line's arbiter is `(work_type, work_id, seq)` and it is written once, so a later partition key by `created_at` never splits a line from its own conflict target; the machinery waits for a measurement that demands it.

## Two streams + one pub/sub channel — and the one that retired

The admissions queue lives in Postgres. Dragonfly carries outbound delivery, live activity and wake notifications.

| Surface | Kind | Owner | Purpose |
|---|---|---|---|
| `fleet.admissions` | Postgres ledger | Admission and lease/report paths | Durable inbound queue, ordered by admission `sequence` within a fleet. |
| `connector:outbound` | Stream and consumer group | Outbound worker | Delivers replies backed by `fleet.obligations`. |
| `fleet:{id}:activity` | Sharded publish/subscribe channel | Activity hub | Live frames; reconnect recovers durable history through the events list. |
| `fleet:wake:{shard}` | Sixteen sharded publish/subscribe channels | Doorbell listeners | Prioritizes fleets on subsequent polls. The local hint buffer holds at most 64 distinct fleets. |

Activity and wake notifications are ephemeral. Their loss cannot remove accepted work from the ledger.

**Client-side gap recovery (M122).** Because the channel has no resume, a dashboard tab that drops its SSE connection misses every frame published during the reconnect window. The stream registry (`ui/packages/app/lib/streaming/fleet-stream-registry.ts`) closes that gap client-side. On every reconnect open — never the SSR-seeded initial connect — it fetches the bounded `fleet.events` list, keyed `since` the last server-delivered event minus a 2-second overlap, and merges by event id. The fetch goes through the same-origin token-minting proxy `/live/v1/workspaces/{ws}/fleets/{id}/events` (mirror of the SSE proxy; the `/live/*` prefix keeps these routes outside the `/backend/:path*` rewrite that shadowed them on Vercel). No server, channel, or frame-shape change — the durable table remains the recovery source of truth.

## Connection topology — the cutover collapsed the dedicated tier

The Rust daemon shares one cluster command owner for ordinary Dragonfly commands.
A lease request probes the Postgres ledger and never blocks on a Dragonfly stream.
An empty response tells the runner when to poll again; the runner holds no Dragonfly connection.

```text
agentsfleetd replica
  HTTP handlers and background writers
    +-- shared cluster command owner --> outbound XADD / XACK and activity / wake SPUBLISH
  SubscriptionHub async pump
    +-- dedicated pub/sub owner --> SSUBSCRIBE per watched fleet
         +-- bounded broadcast --> per-fleet or workspace SSE response bodies
    +-- the same owner --> SSUBSCRIBE fleet:wake:{shard}, sixteen --> bounded fleet hints
  connector:outbound worker
    +-- dedicated blocking owner --> XREADGROUP BLOCK (up to 5 seconds)
```

Cloning `afd_dragonfly::Dragonfly` shares its command owner. The outbound reader owns `afd_dragonfly::Dedicated`, keeping blocking reads separate from request-path commands.

Normal boot has three logical owners when both optional background surfaces start. Cluster topology determines physical sockets; see [datastore scaling](datastore_scaling.md#target-design).

The hub refcounts subscribers and keeps one wire subscription per watched channel.
The control task owns commands, subscriptions and reconnects. A separate dispatch task receives pushes, so a slow subscription command cannot hold a frame.

A connection that ends inside 5 s waits a climbing step before redial.
This keeps a dead or flapping primary from being redialled at the first step forever.
A connection that held 5 s is redialled at once.

Recovery from a lost subscription: [datastore_scaling.md](./datastore_scaling.md) §"Target design".
Each frame is one shared allocation, however many viewers read it.

Source: [`afd_dragonfly::Dragonfly`](../../rustd/crates/afd_dragonfly/src/client/mod.rs),
[`hub control task`](../../rustd/crates/afd_dragonfly/src/hub/pump/mod.rs),
[`hub dispatch task`](../../rustd/crates/afd_dragonfly/src/hub/dispatch.rs),
[`node repair`](../../rustd/crates/afd_dragonfly/src/hub/repair/mod.rs),
[`runtime boot`](../../rustd/crates/agentsfleetd/src/serve/runtime.rs), and
[`outbound worker boot`](../../rustd/crates/agentsfleetd/src/outbound.rs).

## The Postgres pool: a saturated pool and a dead datastore are different pages

Every request-path Postgres read takes exactly one pooled connection and holds
it for the life of the handler. A read that acquires a second while holding the
first is how a pool deadlocks under load — two requests each holding one and
waiting for another — so a handler never holds two pool connections at once
(RULE CNX, `.orly/docs/greptile-learnings/RULES.md`).

An acquire can fail two ways, and they are **different operator problems**:

| Failure | What it means | The fix |
|---|---|---|
| `PoolTimeout` | every connection is leased and the acquire budget elapsed | capacity — pool size, or the slow query holding a slot |
| `PoolUnavailable` | the pool could not produce a connection at all | the datastore — reachability, credentials, TLS |

The pool acquire answers those as a typed error, and the two are asked apart
by name (`rustd/crates/afd_db/src/error/mod.rs` — `is_pool_capacity` against
`is_datastore_unavailable`). An acquire that answered only an absent connection
would erase the distinction at the handler boundary and put both behind one
alert, and the handler that needed to tell them apart would acquire from the
pool directly — reimplementing the acquire/release pairing the pooled-connection
guard makes unskippable.

**What the pool guarantees, and what it does not.** Releasing an occupied slot
lets at least one queued waiter progress, and every waiter either acquires or
receives the configured timeout — no waiter blocks forever. There is **no
ordering or fairness guarantee**: which waiter wins is scheduling. `afd_db`'s
live-pool integration suite proves the two real guarantees against a live
size-1 pool and deliberately declines to assert the third.

## Config reload — pull-per-lease, no signal

Canonical: [`runner_fleet.md` §Config](./runner_fleet.md) — config resolves fresh from `fleet.fleets` on every `lease`; a `PATCH` takes effect on the next lease with no cache and no signal. What is specific to this flow: status works the same way — the assignment scan filters `fleet.fleets.status = 'active'`, so a paused Fleet drops out on the next scan and a resumed one re-enters.

## End-to-end sequence

### Fleet setup and runtime sequence

```text
person -> private draft/messages -> validated Definition -> reviewed candidate
                                      |                         |
                                explicit Library publish   Save fleet
                                      |                         |
                                pinned Library revision -> retained installing fleet
                                                                |
                                             checks + actual grants + guarded activation
                                                                |
                                      no initial request | Blocked | Accepted stable event
                                                                |
                                            schedules sync after activation
                                                                |
                                          B. TRIGGER -> C. EXECUTE -> D. WATCH
```

Messages are durable recovery; progress streams are hints. Private draft access requires its creator and current membership. Runtime history remains fleet-scoped.

An unchanged Library install uses a pinned revision directly. Improve applies a reviewed setup revision without publishing it. An accepted initial admission survives reload with the same event identity.

### A. SAVE  (a reviewed private candidate or a pinned Library revision)

One creation sequence serves both sources; §"Fleet creation and setup recovery" below carries it. The retained creation identity makes retries resume the same setup.

### Fleet creation and setup recovery

#### Review and first Save

A draft has no queued/running message when Save begins. GET draft returns normalized effective setup, immutable requirements, live readiness, and a digest. The person chooses Save or Save and run now; trigger content is identical. Library installation of an unchanged revision bypasses a draft and reads its review through Inspect. Changing its name/model/setup opens a draft so every effective input has been reviewed.

Stop raises the draft's accepted-message watermark before Save. It cancels queued and running messages through that sequence; retained attempts keep their accounting and reservations. Discovery and locked admission check only attempts above the watermark.

A fresh Send refuses uncancelled unknown provider usage after checking exact replay. Stop allows a higher-sequence message to run subject to ordinary capacity and balance checks.

Save preserves the watermark while the draft enters `saving`; activation closes it. A setup Kill that restores `editing` also preserves the watermark, so old proposals and answers remain fenced. Old reports may still settle their original attempt.

Already accepted candidate edits remain available for the person's review; Stop does not undo them. The [Stop implementation](../../rustd/crates/afd_authoring/src/message/cancel.rs), [publication guard](../../rustd/crates/afd_authoring/src/lease/guard/mod.rs), and [creation regression](../../rustd/crates/afd_authoring/tests/draft/creation/stopped.rs) define these boundaries.

POST fleets includes a UUIDv7 `Idempotency-Key`, immutable source, reviewed name, digest and start choice. Authorization precedes recording the normalized body and fleet identity. Changed-body reuse returns 409.

Within the [authoring replay window](api_resources.md#shared-wire-rules), the same key and body resume that fleet’s setup checks. Progress may advance; accepted inputs and identity remain unchanged. After expiry, inspect the linked fleet before another Save.

The draft enters `saving` with the linked fleet identity. A previously published candidate's Library receipt remains a source pin; do not publish it again or change an existing revision. Save itself creates no Library entry. Publishing before Save and Save without publication both work.

#### Bounded setup checks

1. Confirm the source revision, schema version, review digest, and policy marker.
2. Resolve the workspace owner's runtime model and wallet; recheck current membership and fleet scope.
3. Verify declared connections and credentials by name without putting secret values in the candidate or response.
4. Create or locate the required approval requests. A pending/rejected grant produces a blocked check, not ready access. Repeated setup does not duplicate approval requests.
5. Complete required setup resources under the retained identity. External calls run before database locks are held.
6. Atomically compare installing state, setup revision, policy marker, and approved access. Check affected rows. Only this guarded update may activate the fleet.
7. Commit the requested initial admission with activation, or record its refused outcome. Acceptance follows this commit. Attempt the wake notification after commit, then register schedules independently.

Each external setup call has a declared timeout. A request has a 15-second setup budget and returns `201 installing` if the fleet exists but required checks remain. Five-second external-call limits fit this budget; after it is spent, retain pending checks for explicit retry. These are proposed limits to measure in implementation, not observed timings.

Malformed source or mismatched review fails before creating a runnable fleet. A database outage may leave the client uncertain whether the accepted identity committed: same-key retry resolves that uncertainty. A platform outage cannot become a guessed success. A stored partial result exposes checks and next actions through GET fleet.

#### Initial run and triggers

`none` creates no initial admission. `run_now` commits one admission with activation and returns its stable event identity. A lost wake leaves `start.state` accepted.

A refused initial admission may leave the fleet active with `start.state` blocked and its error. Retrying setup preserves the initial event identity. A person can explicitly send another fleet message later.

Cron is registered after activation, removing the proposed pre-activation callback queue. GitHub and Slack can observe activation immediately; no claim is made that the initial run completes before other work arrives. Existing affinity and admission ordering apply. A schedule push failure leaves an active fleet with `sync:"failed"` and Retry sync. A push still unfinished when Save stops waiting (5 s) stays `sync:"syncing"`; no sweep re-drives it, and its 30 s fence lease refuses a `:sync` retry until it expires. GET fleet and CLI expose both independently from required activation checks.

#### Explicit retry and Kill

Same-key POST or `PATCH fleet {status:"active"}` with the current resource `If-Match` resumes missing setup checks. PATCH remains status/name only. A name edit during installing must invalidate the pending review and refuse activation until a newly reviewed revision is applied; it cannot silently change accepted creation inputs. Concurrent setup requests share the same guarded stages and initial admission identity.

Kill uses existing fleet:write authority. The transaction changes fleet state and setup cancellation generation before a late activation can commit. The linked draft returns to `editing`. Pending approval requests are cancelled under the fleet lifecycle guard. Setup resources remain owned by the killed fleet until ordinary fleet:admin purge; no new cleanup or creation worker is assumed. Killed state prevents any new lease or grant use. If activation already won and advanced the resource tag, the stale setup Kill receives412; the person must refetch before deciding to Kill the active fleet. Another Save has a fresh key and fleet ID. The cancelled key keeps referring to the killed fleet.

Fleet DELETE still requires fleet:admin and killed state. Deleting private draft text is not permission to delete the shared fleet. Draft removal cancels private work and hides its prefix; shared installing checks remain recoverable through the fleet by authorized members.

#### Applying an improvement

Improve copies an immutable fleet setup revision into a creator-private draft. POST fleet revisions carries `If-Match: setup_etag`, one header replay key, candidate ID, and review digest. It atomically checks target/source revisions and updates the setup pointer, policy marker, and affected inherited schedules. A stale base or digest refuses the whole update. The response is the new setup tag and schedule states; publication remains separate.

Already leased runs retain captured setup except live revocation and renewed budget ceilings. New leases use the reviewed new revision. A policy marker drift blocks new admission until a human reviews and applies a revision. A name/status resource change does not falsely make setup content change; inherited schedule tags include instance setup and recipe/override revision.

In the browser, **Edit setup** opens that private draft beside the current fleet conversation.
Multiple messages and manual instruction edits update the same candidate.
Explicit **Save changes** applies the reviewed revision and returns focus to the fleet conversation.

A stale setup tag retains the proposed edits for review.
Initial **Save fleet** opens the same fleet Chat workspace.
Both purposes share the [provider reader and browser chat implementation](shared_chat_verification.md).

#### CLI partial success

The CLI displays `Fleet created: installing`, each missing check and its action, the fleet ID, and a copyable resume command that reads the fresh tag then PATCHes active. It exits nonzero, preserving the creation replay key for explicit retry. An active fleet with a blocked initial run, or any schedule not `synced` (`failed`, `syncing`, or a status the CLI does not know), prints the partial outcome and actionable retry and exits nonzero, rather than claiming all requested work succeeded.

### B. TRIGGER  (steer / webhook / cron — three callers, ONE ingress)

Workspace connection proofs: [connectors.md](./connectors.md) §"GitHub App: platform setup to fleet execution".

The diagram's chunk-continuation entry records earlier behavior. Current [conversation compaction](context_lifecycle.md) continues the same run without adding an admission.
Approval resolution still creates a continuation admission.

```
   Common admission fields, stored in fleet.admissions.
   The row's UUIDv7 id is the canonical event identity across redelivery.

       actor         steer:<user> | webhook:<source> | github-app | cron:<schedule>
                     | continuation:<original_actor> | slack:<user>
       type          chat | webhook | cron | continuation
       workspace_id  <uuid>
       request       <opaque JSON — the message + metadata>
       created_at    <RFC 3339 in UTC; when the admission row was stored>
       id            <uuid>, the admission row's event identity

   STEER     agentsfleet steer <fleet_id> "morning health check"
               → POST /v1/.../fleets/{id}/messages
               → INSERT fleet.admissions
                      actor=steer:kishore  type=chat
                      workspace_id=<ws>    request=<msg>
                      created_at=<rfc3339>
               → 202 { event_id }                ← CLI uses event_id
                                                   to filter SSE frames

   GITHUB    App posts pull_request or workflow_run
   APP         → POST /v1/ingress/github
                 verify platform github-app.webhook_secret BEFORE payload read
                 installation.id → connector.installs → workspace
                 repository.full_name + event + approved grant
                    → active fleet subscriptions
                 authenticated-body-digest/fleet replay slot
               → INSERT fleet.admissions for each exact match
                      actor=github-app      type=webhook
                      workspace_id=<ws>     request=<normalized-json>
                      created_at=<rfc3339>
               → 202

               `repositories` is required and fail-closed, with replay per fleet
               ([connectors.md](./connectors.md) §"Repository and event subscriptions belong to fleets").

   MANUAL     Custom providers and the old GitHub workflow_run path retain
   WEBHOOK      POST /v1/webhooks/{fleet_id}/{source}   (one per webhook trigger)
                 POST /v1/webhooks/{fleet_id}/github
               with a workspace `<source>.webhook_secret`. The fleet identifier
               is already in the URL, so this route does not require
               `repositories` and does not use `connector.installs`.

               The internal Clerk endpoint that bootstraps our own tenants
               on `user.created` is NOT this surface. Its path is in the
               auth family — `POST /v1/auth/identity-events/clerk` — but
               the ingress plane serves it, because the caller is a vendor
               presenting a signature rather than a bearer token. The
               `/v1/webhooks/` and `/v1/ingress/` namespaces are
               customer-data-plane only.

   CRON      QStash calls POST /v1/ingress/qstash/schedules
               → agentsfleetd verifies the signature with its boot-loaded
                 current or next signing key
               → checks the stored schedule generation and Fleet state
               → INSERT fleet.admissions with the scheduler replay key
                      actor=cron:<schedule_id>  type=cron
                      workspace_id=<ws>        request=<schedule-event-json>
                      created_at=<rfc3339>

   CONTINUATION  agentsfleetd re-enqueue (chunk-continuation or
                 user-resumed fulfillment)
               → INSERT fleet.admissions
                      actor=continuation:<original_actor>
                      type=continuation
                      workspace_id=<ws>  request=<continuation-msg>
                      created_at=<rfc3339>
                 The new event's row carries
                 resumes_event_id=<immediate_parent_event_id>.
                 Continuation actor is FLAT — never re-nests
                 `continuation:` (a steer that chunks 3 times produces
                 `actor=continuation:steer:kishore` on every continuation,
                 not `continuation:continuation:continuation:...`).

   All six producers commit the same admission shape to one ledger. The
   reasoning loop never branches on actor. Actor is metadata for the
   fleet's instructions and the user's history filter.

   > [!NOTE]
   > SLACK — producer `slack_mention` (M206_002,
   > afd_api_ingress/src/handler/mention/mod.rs). One more producer into THIS same
   > ingress — the lease/execute path does not change. Routing lives in
   > scenarios/slack-incident-responder.md §4; memory continuity in
   > [`runner_fleet.md`](./runner_fleet.md) §"Memory continuity". Spec:
   > docs/v2/done/M106_001_P1_API_DOCS_INFRA_UI_SLACK_RESIDENT_CHANNEL_BOT.md

```

#### QStash owns the clock

`agentsfleetd` stores the desired schedule, pushes each requested mutation to
QStash synchronously, and receives the fires. The runner owns no schedule
timer ([`runner_execution.md`](./runner_execution.md#tool-catalog)).

A schedule has one of three authors, recorded as its `source`: `trigger` for
the cron trigger in the fleet's reviewed setup, `api` for a person, and `fleet` for the fleet itself,
through its lease's schedules verb. The fleet is the lease's, so a body cannot
name another; a fleet holds at most 16 schedules it made, and it can change or
delete only those. A create, change or delete proves the lease again on the
write's own transaction (`afd_fleet::lease::write_fence`), holding the lease row
shared until the write commits, so a reclaim, renew or settle waits for it and
an old holder resumed after a reclaim writes nothing: `UZ-RUN-005`, `current_state`
`superseded`. A run-now admits through the same `schedule_fire` producer a
QStash fire does, keyed `run:<event_id>` by the leased event so a reclaimed
lease replays the run, and both record `actor=cron:<schedule_id>`. A run-now's
refusals are listed in API reference › Runner plane (Run a schedule now). A
schedule's runs are its history rows under that actor, read through slot 1700's
`(fleet_id, actor, created_at, event_id)` index, so a page never walks the
fleet's whole history. A `once` schedule (slot 1750) retires inside that one
fire seam: the fire is admitted, then the schedule is claimed `deleting` and
removed from QStash. A `once` fire is keyed by the schedule alone, so a run-now
racing its QStash fire replays it rather than admitting a second run. A QStash
fire dropped because the fleet takes no work also retires a `once` schedule,
since its moment has passed; a retirement whose claim is held answers
`UZ-SCHED-006`, so QStash repeats the fire. A `once` row keeps the instant it
was set for (slot 1750, `fire_at`), and a sync after that instant removes one
QStash never registered rather than registering an expression whose next match
is a year away; one QStash already holds stays, so a delayed callback finds it.
A pause that removes a one-off upstream puts its key back to its own id, so a
resume after its moment retires it too.

#### The webhook auth taxonomy

Rejections carry `UZ-WH-020` (misconfigured), `UZ-WH-010` (bad signature) and `UZ-WH-011` (stale timestamp); meanings and fixes are in [error codes](https://docs.agentsfleet.net/api-reference/error-codes#UZ-WH-020).

There is no Bearer fallback. The `Authorization` header is never
consulted on `/v1/webhooks/…` routes. See
[`../AUTH.md`](../AUTH.md) §"Manual fleet-webhook auth" for the full surface.

### C. EXECUTE  (lease → runner → report)

The lease call prepares work for the runner. The report call commits its terminal outcome.

1. Refuse placement when the runner's isolation verdict is degraded or unreadable.
2. Probe eligible active fleets and each fleet's oldest unsettled admission through `idx_admissions_unsettled`.
3. Prefer held fleets, recent hints and previous placement; randomize the remaining candidates. Try at most 64 candidates per poll.
4. Lock a fleet slot and its exact oldest admission with `SKIP LOCKED`. A locked or parked head cannot be skipped within that fleet.
5. Advance the slot and admission fences together. Decode before committing; malformed stored data rolls back both fences.
6. Open `fleet.events` and stamp admission delivery in one transaction under those fences. Terminal refusals settle the admission without issuing a lease.
7. Resolve the payer, balance, budget, approvals and installed policy. A wait for a person parks the head and releases the unissued slot.
8. Resolve declared tool secrets separately from the model provider key. Build `ExecutionPolicy` and issue `runner.leases` under the claimed fence.
9. Run the lease in the supervisor and sandbox. Forward activity over HTTP; report the outcome and measured usage when the turn ends.
10. Commit the report under the host, slot, lease and admission guards. One transaction records usage, terminal history, the checkpoint, admission settlement, slot release and any outbound obligation.
11. After commit, publish terminal activity and attempt outbound delivery.

A repeated committed report acknowledges the original result without charging again.
A superseded report answers `UZ-RUN-005` and changes no durable state.
A failed report transaction leaves the admission unsettled and its other writes rolled back.

A runner that disappears leaves an unsettled admission. Once its slot expires, another claimant can reissue that event under a higher fence.

The admission retains its original time and billing identities. The previous holder cannot settle through the newer fence.

Approval resolution or expiry unparks eligible heads and attempts a wake notification.
Every subsequent idle poll probes Postgres, so notification loss cannot leave that work stranded.

See [the queue design](./datastore_scaling.md#the-admissions-ledger-is-the-queue) for budgets, polling costs and recovery limits.
See [provider key boundaries](./billing_and_provider_keys.md#82-the-api_key-visibility-boundary) for plaintext ownership.
The [runner sequence](./runner_execution.md#one-lease-end-to-end) owns sandbox startup, agent runs and teardown.

A recorded reply destination creates an outbound obligation with the report.
The outbound worker posts that answer to its connector thread; [the Slack flow](./scenarios/slack-incident-responder.md#6-how-the-answer-comes-back) shows the destination.

### D. WATCH  (user-side: how the live tail surfaces)

```
   CLI       agentsfleet steer <fleet_id> "<message>"   (batch mode)
               → opens GET /v1/.../fleets/{id}/events/stream (SSE) BEFORE
                 posting the message, and waits (bounded, 2 s) for response
                 headers. The hub queues SSUBSCRIBE before returning the stream.
                 Headers do not acknowledge Dragonfly subscription readiness;
                 an early frame can still race the subscription.
               → the hub shares one pub/sub connection across viewers;
                 the response owns a bounded broadcast receiver.
               → frames arriving before the 202 names the event wait in a
                 bounded client-side buffer (drop-oldest) and replay in
                 order once the id is known; a tail that misses the ready
                 bound is closed unheard and the durable events list alone
                 decides the outcome (a late tail must never pass a
                 truncated reply off as complete).
               → on disconnect: release the receiver; the last reader queues
                 SUNSUBSCRIBE for that channel.

   UI        Fleet Console /fleets/{id}
               → browser EventSource opens the same-origin
                 /live/v1/workspaces/{ws}/fleets/{id}/events/stream proxy.
               → one registry entry owns the connection and retry timers.
                 Connection attempts expire after 30 seconds. Open connections
                 expire after 45 seconds without a heartbeat or application frame.
                 Named heartbeat events arrive every 15 idle seconds, without ids
                 or database queries. They update timing, not React snapshots.
               → actual arrivals spanning 30 seconds establish stability and
                 reset retry history. HTTP open alone never confirms recovery.
                 Heartbeats prove HTTP transport, not Dragonfly publisher health.
               → after a stable connection closes, the displayed status retains
                 last-known health for up to 25 seconds while reconnection runs.
                 Browser recovery signals preserve that deadline. Sustained
                 failures show a notice; automatic retries continue alongside
                 the optional Retry now action. Backfill remains best effort.
               → deploy the named-heartbeat server before the watchdog client.
                 Old clients ignore named keepalives; new clients need them to
                 distinguish quiet streams from silent network failures.
               → the core chat data starts with two reads: fleet detail
                 (status, pending_approvals) and GET /messages?limit=20 (the
                 thread, bodies included). The summary strip is a view
                 over the stream from there: event_complete carries the
                 terminal row plus fleet_status and pending_approvals,
                 gate_opened / gate_resolved carry the count, and the
                 reconnect backfill's durable rows fold in the same way —
                 no read is issued to move a figure. Only a fleet_status
                 that differs from the server-rendered one re-runs the
                 server tree, once, for the header's lifecycle controls.

   UI        Fleets Wall /fleets
               → opens ONE same-origin
                 /live/v1/workspaces/{id}/events/stream SSE connection
                 shared by the workspace's fleet tiles.
               → agentsfleetd authorizes the workspace and fans in only its
                 readable fleet:{id}:activity channels through bounded per-channel
                 broadcast receivers.
               → frame kinds and control frames: API reference › Workspaces
                 (Stream live activity for a whole workspace).
               → if the hub lost a channel's subscription and restored it
                 (a node's socket, a slot move, a redial), agentsfleetd sends
                 catching_up { dropped:0 } once it is back, on this stream
                 and on the per-fleet one.

   Browser authentication
     EventSource + session cookie --> Next.js /live/* Route Handler
       --> Clerk auth().getToken()
       --> fetch API /v1/.../events/stream with Authorization: Bearer JWT
       <-- upstream streaming body <-- agentsfleetd

   The Node.js proxy forwards request cancellation to the upstream fetch.
   It returns upstream error statuses and rejects an absent session with 401.
   CLI clients call the API directly with their Bearer credential.
   No stream token belongs in the URL.

   Sequence ids are per connection and `Last-Event-ID` is ignored (API
   reference › Fleet events); clients backfill from the events list with the
   server's `next_cursor`, never one derived from an event id.

   HISTORY   agentsfleet events {id} [--actor=…] [--since=2h]
             Dashboard /fleets/{id}/events
               → reads fleet.events (cursor-paginated).

   RESUME    no user-facing command reads fleet.sessions. The lease
             path reads it with the fleet row and hands it to no lease.
             "Busy or idle" is runner.leases, which has the fencing
             token and the expiry that make the answer trustworthy.

   Both browser registries backfill durable event rows after reconnect,
   and on any catching_up frame, a lag's or a lost subscription's.
   A publish missed while the HTTP stream stayed open is therefore
   recovered by that backfill rather than lost silently.
   Transient token chunks are not durable history; settled rows are.
   Live tail is best-effort; fleet.events remains the durable record.
```

## Multi-tenancy boundary

| Layer | Tenant isolation mechanism |
|---|---|
| Postgres (`fleet.fleets`, `fleet.events`, etc.) | Enforced in the application, not by the database. `afd_tenant`'s `AUTHORIZE_WORKSPACE` resolves the caller's tenant and the workspace's owner in one statement, and `afd_api` mounts it as a layer in front of every route whose path carries a workspace — so no handler is in a position to forget it. Every control-plane statement filters by `workspace_id` or `fleet_id` explicitly. **No `ROW LEVEL SECURITY` policy is declared anywhere in `schema/`, and no `current_setting('app.workspace_id')` is read.** Re-adding database policies would need a transaction per request, because `sqlx` returns a connection to the pool between requests and a session-level setting would leak one tenant's identifier onto the next. |
| Dragonfly live activity (`fleet:{id}:activity`) | Key namespaced by fleet UUID (globally unique); no cross-tenant collision possible. Dragonfly has no per-row authorization of any kind, so the protection is the same shape as the row above it: the key is unguessable and every path to it goes through the ownership check. |
| Runner ↔ control plane | The `agt_r` token authenticates the runner per call, and a lease carries one fleet's event and scoped secrets ([`runner_fleet.md`](./runner_fleet.md) §"Registering a runner"). |
| Runner supervisor and sandbox | Secrets stay in the supervisor, substituted at send time, and a sandbox holds no credential ([`runner_execution.md`](./runner_execution.md) §"Credentials"). |

## One active lease per fleet — the ownership model

Before the cutover, a single worker thread owned all events for a Fleet, and the concern was round-robin across worker replicas breaking per-fleet continuity. That model is gone. Ownership is now a **durable lease**, not a thread:

- `runner.fleet_slots` holds one slot per fleet. `Leases::select` (`rustd/crates/afd_fleet/src/lease/assign/mod.rs`) claims it atomically — a runner wins iff the slot is free or the prior lease has expired — and bumps a monotonic `fencing_token`. So **at most one lease is active per fleet at any time**, regardless of how many runners poll concurrently. The claim is the only writer of `fencing_token`, so every fencing token a lease carries is a value a claim returned.
- A runner that loses the race for a Fleet simply gets no lease for it and tries the next eligible fleet (or backs off).
- Durable fleet memory and bounded chat history can reach another runner’s next lease. Warm sandbox reuse is optional; session checkpoints are saved but not replayed into leases. [Workspace continuity](runner_execution.md#workspace-between-leases) owns these limits; sticky placement remains a hint, never ownership.

Failure mode: a dead lease holder blocks its fleet until `lease_expires_at`; reclaim then re-leases with a higher fencing token. Recovery latency and its bound: [`runner_fleet.md`](./runner_fleet.md) §"Failure recovery model".

One PR through one lease: its records and timers in [lease_flow.md §1](./lease_flow.md#1-from-start-a-fleet-to-a-lease), its reply limits in [§4](./lease_flow.md#4-how-agent-bob-01-can-reply), and what bounds the host in [§7](./lease_flow.md#7-where-the-image-lives-and-what-bounds-a-bare-metal-host).

## What the coding fleet never does

- Never sees the fleet's LLM tokens or reasoning state
- Never holds the fleet's secrets in its own context
- Never executes the fleet's tool calls in its own session
- Never persists across the user's laptop being closed

## What the fleet (host) never does

- Never touches the user's laptop directly
- Never reads the user's local filesystem (it sees only what the fleet's reviewed setup grants it)
- Never escapes the sandbox — namespaces, Landlock, seccomp and cgroup v2 bound each lease's sandbox, and the agent loop runs outside it ([`runner_execution.md`](./runner_execution.md#sandbox-engines)). **Network egress** follows the runner's network policy (see [`runner_fleet.md` §Egress model](./runner_fleet.md)).
- Never holds a datastore credential — the runner reaches the platform only over the `/v1/runners` protocol

## The install failure scenario, visually

```text
record key + normalized body + retained installing fleet -> commit
checks or grants incomplete -> installing + missing checks
same key + same body -> resume same fleet and setup revision
Kill wins activation guard -> killed fleet; linked draft returns to editing
activation + initial admission -> commit together
read back the same initial event -> Accepted
read-back unavailable -> Pending; retry confirms the same event
register schedules independently
lost wake -> the next idle poll still reads the ledger
```

Do not delete the retained fleet on setup failure, claim activation after a zero-row update, or submit another initial event on retry. Undelivered, unsettled steer admissions participate in ordinary message history alongside delivered events in one statement snapshot, with stable event identity and pagination (`afd_events/src/history/statement/select_thread_page.sql`). No creation worker is introduced.
