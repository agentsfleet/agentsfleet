---
type: explanation
audience: contributor
verified: 2026-11-09
product_version: 0.75.0
executable: false
---

# Data Flow: how an event moves through the system

> Parent: [`README.md`](./README.md) · Sibling: [`runner_fleet.md`](./runner_fleet.md) (the structural split this flow runs on). · User-facing: [docs.agentsfleet.net/fleets/webhooks](https://docs.agentsfleet.net/fleets/webhooks) (sending an event) and [docs.agentsfleet.net/fleets/running](https://docs.agentsfleet.net/fleets/running) (watching one).

> **Scope:** `agentsfleetd` owns intake, assignment and settlement. `agentsfleet-runner` runs the agent loop and tools, then reports over HTTPS.

> See [runner processes](./runner_execution.md#process-model) for the supervisor and sandbox boundary.

Read this when you need to know where a webhook, a steer, or a cron fire ends up. Many specs reference this file as the canonical picture of the runtime.

## Facts

Every row is extracted from the sections below; the owner column names the section that carries the full story.

| Invariant | Value | Mechanism | Owner section |
|---|---|---|---|
| Event ingress | Seven producers, one queue | `Producer` names `steer`, `webhook`, `webhook_app`, `schedule_fire`, `gate_continuation`, `task_continuation` and `slack_mention`. Each commits a `thread.turns` row and attempts a wake notification. | §B. TRIGGER |
| Message disposition | `started`, `folded` or `queued` | a message accepted while its fleet's run is live folds into that run; the run takes it at its next model call, and the message's own turn gets the run's reply | §"C. EXECUTE" |
| Queue ownership | Postgres | Intake, assignment and settlement use the same `thread.turns` row. Outbound replies are `thread.deliveries` rows; core NATS carries only notifications. | §Steer flow end-to-end |
| Durable stores | 4 tables, join key `turn_id` | `thread.turns` (one row per input, UNIQUE `(producer, producer_key)` and `(thread_id, sequence)`) · `thread.deliveries` (one row per answer, UNIQUE `(fleet_id, turn_id)`) · `thread.threads` (one row per fleet or draft, holding `resume_context`) · `billing.usage_lines` (one row per measured slice of a run, UNIQUE `(work_type, work_id, kind, seq)`, charged by the billing relay) | §The five durable stores |
| Replay safety | Stable producer and turn identities | Intake deduplicates `(producer, producer_key)`; a reassigned turn keeps its turn and billing identities. | §"C. EXECUTE" |
| Stale-writer rejection | `UZ-RUN-005` | Thread, run and turn fences guard the report transaction. | §"C. EXECUTE" |
| Bus connection | One NATS connection per daemon process | Publishes and subscriptions share it; one native subscription per subject with local readers. The inbound queue and outbound delivery claims use Postgres. | §Connection topology |
| Postgres acquire failures | 2 distinct errors | `PoolTimeout` (capacity) vs `PoolUnavailable` (datastore); one connection per handler (RULE CNX) | §The Postgres pool |
| Postgres statement bounds | the API login role's | 2 s lock wait (`55P03`) and 5 s statement (`57014`), declared in `afd_db::login` and set once by `ALTER ROLE`; no statement sets its own | §The Postgres pool |
| Config freshness | named by the assignment | a `PATCH` takes effect on the next run: the assignment returns the head revision id and the parsed definition is held under that id; no signal | §Config reload |
| Gate-blocked rows | terminal | never reopened; the resolved gate lands a NEW row via `actor=continuation:<original>` | §"C. EXECUTE" step 3 |
| Activation guarantee | Checked activation and the initial turn commit together | Run-now acceptance follows the committed turn; a lost wake cannot strand it. | §A. SAVE |
| Server-Sent Events (SSE) sequence ids | not durable | per-connection counter, resets to 0; `Last-Event-ID` ignored; backfill via the turns list | §D. WATCH |
| Client gap recovery | reconnect, or `catching_up` on either stream | bounded `thread.turns` list `since` last delivery minus 2 s overlap, merged by turn id; a lost server subscription arrives as `catching_up` with `dropped: 0` | §Two streams + one pub/sub channel |
| Cron authority | QStash | signature verified at ingress; replay suppressed atomically; the runner owns no timer | §B. TRIGGER |
| Cancel latency | the runner's next Cancel message | archive and delete send Cancel to each live run's runner; a gate trip writes only `fleet.fleets.status`, and that run ends on its own, at its fleet's stored budget ceiling, or at `MAX_RUNTIME` | [`runner_fleet.md`](./runner_fleet.md) §"Steer, archive, delete" |
| Run ownership | at most one active run per fleet | atomic `thread.threads` assignment + monotonic `fencing_token` | §One active run per fleet |
| Provider `api_key` | never in `secrets_map` | rides `ExecutionPolicy.provider` + `.api_key`; injected for the inference call only | §"C. EXECUTE" step 8 |
| Tenant isolation | application-enforced + namespacing | every workspace route passes an ownership check before its handler runs; NATS subjects carry the unguessable thread UUID. This repository declares no `ROW LEVEL SECURITY` policy | §Multi-tenancy boundary |

## Traps

Each trap is enforced in its owner section; this list is the index.

- The live tail is the eyeballs surface, not the audit surface; durable history is `thread.turns` (§Two streams + one pub/sub channel).
- A publish never waits out a broker outage. From a disconnect until its gap markers are paid, every publish fails at once (§Connection topology).
- Never acquire a second Postgres connection while holding one; that is how a pool deadlocks (§The Postgres pool).
- The Postgres pool has no ordering or fairness guarantee; do not assert one (§The Postgres pool).
- No product statement sets its own lock or statement bound; the login role's bounds cover every daemon statement (§The Postgres pool).
- `gate_blocked` rows are NEVER reopened (§"C. EXECUTE" step 3).
- A Pull Request reviewer cannot post a review or a comment: no egress write rule admits one ([`run_flow.md`](./run_flow.md) §"4. How AGENT BOB 01 can reply").
- Every push is its own turn. One that arrives while the fleet's run is live folds into that run; the rest queue behind it (§"C. EXECUTE").
- Preserve the turn's identity across assignments and retries (§B. TRIGGER).
- The continuation actor is FLAT: it never re-nests `continuation:` (§B. TRIGGER).
- `repositories` is required for GitHub App traffic; omission means no delivery, never every repository (§B. TRIGGER).
- No Bearer fallback on webhook routes; the `Authorization` header is never consulted there (§B. TRIGGER).
- Clients never derive a cursor from a turn id; SSE sequence ids have no cross-connection meaning (§D. WATCH).
- The reasoning loop never branches on actor; actor is metadata (§B. TRIGGER).
- `/v1/webhooks/` and `/v1/ingress/` are customer-data-plane only; Clerk identity events live in the auth plane (§B. TRIGGER).
- The coding fleet never becomes the Fleet runtime and never sees its tokens (§The coding fleet and the Fleet runtime).

## Topology

Two stores back the runtime. Postgres holds every durable fact: threads, turns, runs, items, deliveries and usage.

Core NATS carries disposable signals on five subjects: thread wake, thread items, runner command, delivery wake and bus gap. Production runs three nodes in one region (`iad`), with no JetStream and no key-value store.

A lost signal costs latency, never work, because every reader can rebuild from Postgres. [`high_level.md`](./high_level.md#how-the-runtime-fits-together) draws the picture.

The diagrams live with their flows. Each is the section's proof, so none is duplicated here:

- coding fleet vs Fleet runtime: §The coding fleet and the Fleet runtime
- the intake, assignment and settlement boundaries in §Steer flow end-to-end
- the NATS connection topology: §Connection topology
- save, trigger envelope, run, watch: §"End-to-end sequence", A through D
- the setup failure window: §The install failure scenario, visually

## Decisions

| Decision | Reason | Where |
|---|---|---|
| A narrative table and a usage table (`thread.turns` + `billing.usage_lines`) | different write authorities and retention rules | §The five durable stores |
| The per-fleet control channel removed | no per-fleet threads left to run | §Two streams + one pub/sub channel |
| One bus connection per process | idle cost tracks the replica count, not fleet count | §Connection topology; M80_002, M244 |
| A pool acquire answers a typed error, not an absent connection | `PoolTimeout` and `PoolUnavailable` are different operator pages | §The Postgres pool |
| Statement bounds live on the login role | one `ALTER ROLE` covers every statement; no product statement carries its own override | §The Postgres pool; M242_001 |
| Gap recovery is client-side, not server resume | no channel or frame-shape change; the durable table is the recovery source | §Two streams; M122 |
| QStash owns the clock | the runner owns no schedule timer | §B. TRIGGER |
| A message accepted during a live run folds into it | one Postgres statement writes its turn; the running run takes it at its next model call, so no second run starts | §"C. EXECUTE"; M242_002 |
| Private drafts own setup | Native JSON can initialize a draft; GitHub/template-bundle import and Markdown export stay unsupported | [`user_flow.md`](./user_flow.md) §"§8.1 Authoring the Fleet" |
| Retained setup identity | Retry resumes missing checks under the original creation identity | §The install failure scenario |
| Browser SSE uses the authenticated Next.js proxy | the server resolves the session and forwards a Bearer; the daemon authenticates Authorization ([AUTH](../AUTH.md)) | §D. WATCH |
| Outbound answers are generic `thread.deliveries` rows, each taken by one worker per lane | the report path stays provider-agnostic (Invariant 9) | §"C. EXECUTE"; M106, M244 |

---

## Detail

Everything below is the full reference. One event is told twice, at two zoom levels, and they do not repeat each other; read the one that matches your question.

Section "Steer flow end-to-end" draws the path as boxes, so you can see where a call goes. Section "End-to-end sequence" (A INSTALL → D WATCH) states what each step must guarantee.

Headings are stable because specs cite them by text. Insert new sections; never rename existing ones.

## Process and stream ownership at a glance

| Process | Role |
|---|---|
| **`agentsfleetd`** (`agentsfleetd serve`) | Owns Postgres, its NATS connection, the Vault and HTTP assignment. Producers commit turns; assignment passes give a turn's run to a runner; reports commit results, usage and settlement together. The daemon publishes live activity and delivers outbound replies. Setup model calls use daemon-held keys. |
| **`agentsfleet-runner`** (host-resident daemon) | The plane that runs the work. Boots from an operator-installed `agt_r` token (env `AGENTSFLEET_RUNNER_TOKEN`, no self-register), then loops `heartbeat → run → run → report → activity` over HTTPS carrying that token. Holds **zero datastore credentials**. A trusted supervisor runs the agent loop and holds the model key. A run that needs one gets a sandbox (bubblewrap, Landlock, seccomp, cgroups) that runs tool calls and nothing else; the sandbox gets a network namespace of its own unless the runner's network policy is `allow_all`. Credential substitution happens in the supervisor at send time ([`runner_execution.md`](./runner_execution.md#process-model)). The supervisor forwards activity frames to `agentsfleetd` over the `activity` verb. |

Streams and tables: §"Two streams + one pub/sub channel" and §"The five durable stores: who owns what".

---

## The coding fleet and the Fleet runtime

Two distinct things are in play, and the architecture only makes sense once they are kept apart:

```
+----------------------------------+         +-----------------------------------+
|  CODING AGENT (laptop)           |         |  FLEET RUNTIME (host)             |
|                                  |         |                                   |
|  Claude Code / Amp / Codex /     |         |  agentsfleet-runner's agent loop, |
|  OpenCode driving agentsfleet    |         |  tool calls in a per-run        |
|                                  |         |  sandbox (bubblewrap, Landlock,   |
|  This is what the human types    |         |  seccomp, cgroups, netns;         |
|  into. Ephemeral.                |         |  survives laptop close)           |
+----------------------------------+         +-----------------------------------+
```

The coding fleet is a workstation tool driving `agentsfleet`. The Fleet runtime (the product object the user creates) runs as the runner's agent loop, with its tool calls in the run's sandbox.

The coding fleet never becomes that runtime and never sees its tokens. They communicate only through the steer endpoint, the turn stream, and the turn history.

## Steer flow end-to-end

This view shows transaction boundaries. The protocol details live under §"C. EXECUTE".

```text
person or provider
       |
       v
intake statement -> thread.turns, status queued (stable turn identity)
       | one top-level statement; commit
       +-> af.thread.<thread_id>.wake (best effort)
       +-> disposition: started | folded into the live run | queued
       |
       v
assignment pass -> indexed oldest unsettled turn per eligible fleet
       |
       v
assign statement -> thread row lock + exact head lock + fence raised
                    + turn stamped run_fence and opened (received)
       | commit after decoding
       v
issue transaction -> thread.runs row under the current fence
       |
       v
Down::Run down the runner stream -> supervisor and sandbox -> terminal report
       |            (a folded message is taken at the next model call)
       v
report transaction -> usage + turn answer + thread resume_context
                  -> turn settled + thread released + outbound delivery
                  -> folded messages' turns get the run's reply
       | commit
       +-> live activity
       +-> af.delivery.wake ping -> outbound worker takes the row -> connector thread
```

A lost notification changes priority only. Each replica's recovery probe reads Postgres every 5 s (`ASSIGN_FALLBACK_TICK`).
An expired run exposes the same unsettled turn with its original turn and billing identities.

The outbound worker takes durable `thread.deliveries` rows from Postgres.
The report writes each row with a fresh `receipt` token and a lapsed `claim_expires_at`, so any replica may take it at once.
A lost `af.delivery.wake` ping costs latency only, because every replica also passes every 30 s (`CLAIM_PASS`).

The poster stamps `delivered_at` only after successful delivery. Abandoning an exhausted job must not mark its answer delivered.

## The five durable stores: who owns what

The heading keeps its old count because other pages cite it by text. Four tables answer different questions now.

Turns record each input and its answer, deliveries record replies still owed, threads hold the resume checkpoint, and usage lines record measured work. One `thread.turns` row replaces the old admission and event pair: intake writes it, and its first assignment opens it.

The unsettled turns are the inbound queue. The outbound worker takes deliveries per lane from their own rows, and `receipt` names the current holder.

Read the primary keys and the shape falls out:

```
  thread.turns       PK id  + UNIQUE (producer, producer_key)   one row per INPUT
                            + UNIQUE (thread_id, sequence)
  thread.deliveries  PK id  + UNIQUE (fleet_id, turn_id)       one row per ANSWER
  thread.threads     PK id  (= fleet_id or draft_id)            one row per THREAD
```

Two of them grow with traffic. `thread.threads` does not, because it is a cursor and not a log. That is why "where does this fleet resume" is a primary-key lookup rather than a sort over its history.

One turn's life across all three:

```
  a message arrives
      │
      ├─▶ thread.turns       INSERT  status='queued'     before the producer hears yes
      │                                                  dedupes the PRODUCER's retry
      ├─▶ thread.turns       UPDATE  status='received'   the assignment opens it
      │                              run_fence, opened_at
      │   ┌── a runner runs the run ──┐
      │   │                           │
      ├─▶ thread.turns       UPDATE  status='processed', answer_text, settled_at
      ├─▶ thread.deliveries  INSERT  "owed"              ← same transaction as the money
      ├─▶ thread.threads     UPDATE  resume_context      the resume cursor moves
      │
      └─▶ thread.deliveries  UPDATE  delivered_at        a person received it
```

The two ledgers are not symmetric, and the asymmetry is **who can retry**. A producer holds its own `producer_key` and repeats it, so a turn deduplicates on the PRODUCER's identity.

A destination has no such key, because a chat provider cannot tell us "this is the same message". So a delivery deduplicates on the turn that produced the answer, the only stable identity this side owns.

An intake failure stays visible to its producer, which can retry the same identity. Uncertain deliveries are sent again because delivery is at-least-once. People can recognize and recover from duplicate replies; a missing answer leaves no visible record.

`thread.turns` is also the only one of the three carrying a `status` TEXT column rather than NULL tests. A turn has a real vocabulary (`queued`, `received`, `processed`, `fleet_error`, `gate_blocked`, `cancelled`, `recorded`), where a delivery has exactly two facts.

The spellings live in `af_core::turn::status` and are never literals in schema (RULE STS). Asking "is it still `received`" is how a reassignment tells a legitimate retry from a turn that already ran.

| Table | Cardinality | Mutability | Answers |
|---|---|---|---|
| `thread.turns` | **One row per input** | INSERT (status=`queued`); the assignment stamps `run_fence` and `opened_at` (status=`received`); the input claim stamps `input_claim_run_id` and `input_claim_fence`; then `parked_at` and the report's `settled_at` with status=`processed` \| `fleet_error` \| `gate_blocked` \| `cancelled` | "What did this Fleet do for turn X? Who triggered it, what did they ask, what did it answer, did the gates pass?" Accepted work, its current ownership and completion, and the user's narrative log. `UNIQUE (producer, producer_key)` deduplicates producer retries. The single source of truth for the turns list and `agentsfleet fleet turns`. |
| `thread.deliveries` | **One row per answer** | INSERT in the report's transaction, then UPDATE `receipt` and `delivered_at` | "Do we still owe somebody this answer?" Committed with the money and the result, so no window exists where a run is charged and its answer exists nowhere. `UNIQUE (fleet_id, turn_id)` makes a replayed report owe one delivery, not two. |
| `thread.threads` | **One row per fleet or draft** | INSERT with its owner; the assignment raises `fencing_token`; the report replaces `resume_context` | "Where did this Fleet leave off?" The fence, the warm hold and the resume bookmark. The run path reads `resume_context` with the fleet row and hands it to no run. "Which fleet is running right now" is `thread.runs`, which has the fencing token and the expiry that make the answer trustworthy. |
| `billing.usage_lines` | **One row per measured slice** of a run: each renewal and the report log the runtime and tokens since the last slice, priced. UNIQUE `(work_type, work_id, kind, seq)`, where the work is the run and `kind` says whether the line is a model call, a runtime slice or a decision call. No money is set aside: the billing relay takes it from `billing.wallets` within seconds of the commit (billing §3, §11). | INSERT in the run's own transaction; `settled_at` is stamped once the biller has charged the line, and nothing else changes. | "What did turn X's run use, and what did it cost?" Billing audit and the source of fleet budgets. Joinable to `thread.turns` on `turn_id`, the turn's primary key. |

Why a narrative table and a usage table (`thread.turns` + `billing.usage_lines`) instead of one? They have different write authorities and retention rules:

- `thread.turns` holds user-readable strings (`request_json`, `answer_text`): large, mutable mid-lifecycle, deletable on tenant offboarding.
- `billing.usage_lines` holds numeric audit columns: small, priced once and never repriced. Lines are kept for billing reconciliation whether or not the conversation row is purged. The table has no foreign keys, so a line outlives the fleet, workspace and price row it names.

Durable run bookkeeping (`thread.runs`, plus the fence on `thread.threads`) is a fourth concern. It records ownership: which runner holds a turn, its fencing token, and its expiry. These rows never carry user strings.

### The list read and the detail read are different reads

A page is up to two hundred rows, and the two body columns are unbounded: a trigger payload and a full agent answer. Selecting them per row bought a table that renders about a hundred and sixty characters per cell. So the list stops selecting them, and the surfaces that used to quote an answer now state an outcome instead.

A row that failed says why; a row that succeeded records that it did, without reproducing the reply. Postgres already keeps wide values in oversized-attribute storage, so the cost was never that the bodies existed. The cost was that the list **selected** them, which is why this is a read-path change and not a storage one.

The turns table and fleet header use summaries. The chat transcript reads bodies through `GET /messages`; one turn's detail stays a separate read. See [watch recovery](#d-watch--user-side-how-the-live-tail-surfaces) for reconnect behavior.

`tool_calls` is a third body column under the same rule. It is the run's bounded tool trace, at most 200 calls and 64 KiB ([Runner Fleet, Live activity](./runner_fleet.md#live-activity-the-sse-tail)). The transcript and the single-turn read select it, and the list never does.

A row from before the column existed holds `NULL`, which reads as "not recorded", never as "no tools ran".

A call's full arguments and output are a fourth body that no page read selects. They live in `thread.items`, a child table of `thread.turns`, one row per saved call, cascading on `turn_id`. The rows are unique on the turn, the run's fencing token and the runner's call number.

A page never reads them: "show all" reads one call through `GET …/turns/{turn_id}/items/{item_id}`. That keeps a thread page at its 512 KiB budget however much a run's tools returned.

Runner runs keep no duplicate request body. Assignment and reassignment read the payload from the turn row and find prior runs through `thread.runs.turn_id`. Settled runs can outlive deleted fleets or turns; their identifiers then stay opaque history.

**Partitioning is not done here.** A usage line's arbiter is `(work_type, work_id, kind, seq)`, written once.

A later partition key by `created_at` therefore never splits a line from its own conflict target. The machinery waits for a measurement that demands it.

## Two streams + one pub/sub channel (and the one that retired)

The turns queue and the outbound deliveries live in Postgres. Core NATS carries live items, wake notifications, runner commands, the delivery ping and the bus gap, each at most once.
Subject spellings come from `afd_bus::nats::spelling` (`rustd/crates/daemon/afd_bus/src/nats/mod.rs`).

| Surface | Kind | Owner | Purpose |
|---|---|---|---|
| `thread.turns` | Postgres ledger | Intake, assignment and report paths | Durable inbound queue, ordered by turn `sequence` within a thread. |
| `thread.deliveries` | Postgres rows, taken per lane | Outbound worker | Delivers replies; at most one row per `(provider, workspace_id)` lane is held at a time. |
| `af.thread.<thread_id>.items` | Core NATS subject | Each process's NATS hub | A fleet's live frames, where a gap or reconnect recovers durable history through the turns list, or a draft's setup progress, where a lost subscription sends `draft.resync`. |
| `af.thread.<thread_id>.wake` | Core NATS subject, read through one `af.thread.*.wake` wildcard per replica | The gateway's assignment scheduler | Starts an assignment pass and puts its thread first. The local queue holds at most 64 distinct threads by default. |
| `af.runner.<runner_id>.cmd` | Core NATS subject | The gateway holding the runner's connection | `Recheck` or `Cancel` for one runner host; Postgres stays the authority (`afd_gateway/src/connection/commands.rs`). |
| `af.delivery.wake` | Core NATS subject | Outbound worker | Pings the worker after a report commits; a lost ping waits for the 30 s pass. |

A development namespace prefixes every subject with `dev.<namespace>.` (`afd_bus/src/nats/config.rs`).

Item and wake notifications are ephemeral. Their loss cannot remove an accepted turn from the ledger.

**Client-side gap recovery (M122).** The subject has no resume.

So a dashboard tab that drops its SSE connection misses every frame published during the reconnect window. The app's stream registry (`ui/packages/app`, `lib/streaming`) closes that gap client-side.

The registry fetches the bounded turns list on every reconnect open, never on the SSR-seeded initial connect. The fetch is keyed `since` the last server-delivered turn minus a 2-second overlap, and the registry merges by turn id.

The fetch goes through the same-origin token-minting proxy `/live/v1/workspaces/{ws}/fleets/{id}/turns`, the mirror of the SSE proxy. The `/live/*` prefix keeps these routes outside the `/backend/:path*` rewrite that shadowed them on Vercel. No server, channel, or frame-shape change: the durable table stays the recovery source of truth.

A fleet tail sends one `catching_up` with `dropped: 0` right after `hello` (`afd_sse/src/live.rs`).
NATS interest reaches other nodes after the subscribe returns, so frames published in that window exist only in Postgres.
The same backfill recovers them.

## Connection topology: the cutover collapsed the dedicated tier

Each `agentsfleetd` process holds one NATS connection, opened by `afd_bus::nats::connect` at boot.
An assignment pass probes the Postgres ledger and never waits on the bus.
The runner holds no bus connection; work reaches it down its one stream.

```text
agentsfleetd replica
  one async-nats client (seeds from NATS_URL)
    +-- publish --> af.thread.<thread_id>.items, af.thread.<thread_id>.wake,
    |               af.runner.<runner_id>.cmd, af.delivery.wake
    +-- hub: one native subscription per subject with local readers
    |     +-- pump --> bounded broadcast --> per-fleet or workspace SSE response bodies
    |     +-- af.thread.*.wake, one wildcard --> bounded wake queue
    |     +-- af.delivery.wake --> outbound worker --> take from thread.deliveries
    +-- gap watcher --> reconnect, slow consumer, Af-Gap markers, af.bus.gap
```

The hub counts readers per subject and keeps one native subscription for each.
Each subject's pump forwards frames and never takes the channel map's lock to do so.
Each frame is one shared allocation, however many viewers read it.

The `async-nats` client reconnects to another seed on its own.
After a reconnect, every local subscription receives one `Gap`.
A slow-consumer event gaps every subscription too, because the client keeps the subscription id private.

A publish that fails, and every subject published in the 2 s before a disconnect, is owed a marker.
On reconnect each owed subject gets one empty message with header `Af-Gap: 1`, which subscribers read as `Gap`.
Past 4,096 owed subjects, one marker on `af.bus.gap` gaps every subscription in every process (`afd_bus/src/nats/debt.rs`).

Boot refuses, as `UZ-STARTUP-004`, a seed it cannot reach within five seconds.
It also refuses a server with JetStream on, no cluster, no headers, or a `max_payload` under the largest frame (`afd_bus/src/nats/preflight.rs`).
After boot, `/readyz` reports the bus in its `queue` field and does not gate on it (`main/src/probes.rs`).

## The Postgres pool: a saturated pool and a dead datastore are different pages

Every request-path Postgres read takes exactly one pooled connection and holds it for the life of the handler. A read acquiring a second while holding the first is how a pool deadlocks under load. Two requests each hold one and wait for another.

A handler therefore never holds two pool connections at once (CNX in `.orly/standards/domains/sql.md`).

An acquire can fail two ways, and they are **different operator problems**:

| Failure | What it means | The fix |
|---|---|---|
| `PoolTimeout` | every connection is held and the acquire budget elapsed | capacity: pool size, or the slow query holding a slot |
| `PoolUnavailable` | the pool could not produce a connection at all | the datastore: reachability, credentials, TLS |

The pool acquire answers those as a typed error. `afd_db`'s error module asks the two apart by name (`is_pool_capacity` against `is_datastore_unavailable`). An acquire that answered only an absent connection would erase the distinction at the handler boundary and put both behind one alert.

The handler that needed to tell them apart would then acquire from the pool directly. That reimplements the acquire/release pairing the pooled-connection guard makes unskippable.

**What the pool guarantees, and what it does not.** Releasing an occupied slot lets at least one queued waiter progress. Every waiter either acquires or receives the configured timeout, so no waiter blocks forever.

There is **no ordering or fairness guarantee**: which waiter wins is scheduling. `afd_db`'s live-pool integration suite proves the two real guarantees against a live size-1 pool. It deliberately declines to assert the third.

**Statement bounds belong to the login role.** `afd_db::login` declares a 2 s `lock_timeout` and a 5 s `statement_timeout`. The API login role carries them through `ALTER ROLE` at setup time, so every daemon statement runs under them.

No product statement sets its own override. A statement that hits either bound answers `55P03` or `57014`, and `afd_db::error::is_bound_reached` reads both as "try later".

## Config reload: pull-per-run, no signal

Canonical: [`runner_fleet.md` §Config](./runner_fleet.md). The assignment reads the head revision id from `fleet.fleets` on every run, and a replica holds the parsed definition under that id. A `PATCH` takes effect on the next run with no signal.

What is specific to this flow: status works the same way. The assignment scan filters `fleet.fleets.status = 'active'`, so an archived Fleet drops out on the next scan and never re-enters.

## End-to-end sequence

### Fleet setup and runtime sequence

```text
person -> private draft/messages -> validated Definition -> reviewed candidate
                                      |                         |
                                explicit Plugin publish   chat Save
                                      |                         |
                                pinned Plugin revision -> retained installing fleet
                                                                |
                                             checks + actual grants + guarded activation
                                                                |
                                      no initial request | Blocked | Accepted stable turn
                                                                |
                                            schedules sync after activation
                                                                |
                                          B. TRIGGER -> C. EXECUTE -> D. WATCH
```

Messages are durable recovery; progress streams are hints. Private draft access requires its creator and current membership. Runtime history stays fleet-scoped.

An unchanged Plugin install uses a pinned revision directly. Improve applies a reviewed setup revision without publishing it. An accepted initial turn survives reload with the same turn identity.

### A. SAVE  (a reviewed private candidate or a pinned Plugin revision)

One creation sequence serves both sources; §"Fleet creation and setup recovery" below carries it. The retained creation identity makes retries resume the same setup.

### Fleet creation and setup recovery

#### Review and first Save

A draft has no queued/running message when Save begins. GET draft returns normalized effective setup, immutable requirements, live readiness, and a digest. The person chooses Save or Save and run now; trigger content is identical.

Plugin installation of an unchanged revision bypasses a draft and reads its review through Inspect. Changing its name/model/setup opens a draft so every effective input has been reviewed.

Stop raises the draft's accepted-message watermark before Save. It cancels queued and running messages through that sequence; retained attempts keep their accounting. Discovery and the locked assignment check only attempts above the watermark.

A fresh Send refuses uncancelled unknown provider usage after checking exact replay. Stop allows a higher-sequence message to run subject to ordinary capacity and balance checks.

Save keeps the watermark while the draft enters `saving`; activation closes it. An archive during setup that restores `editing` also keeps the watermark, so old proposals and answers stay fenced. Old reports may still settle their original attempt.

Already accepted candidate edits stay available for the person's review; Stop does not undo them. `afd_setup` owns the Stop implementation, the publication guard, and the creation regression tests that define these boundaries.

POST fleets includes a UUIDv7 `Idempotency-Key`, immutable source, reviewed name, digest and start choice. Authorization precedes recording the normalized body and fleet identity. Changed-body reuse returns 409.

Within the [setup replay window](api_resources.md#shared-wire-rules), the same key and body resume that fleet’s setup checks. Progress may advance; accepted inputs and identity stay unchanged. After expiry, inspect the linked fleet before another Save.

The draft enters `saving` with the linked fleet identity. A previously published candidate's Plugin receipt stays a source pin; do not publish it again or change an existing revision. Save itself creates no Plugin entry, so publishing before Save and Save without publication both work.

#### Bounded setup checks

1. Confirm the source revision, schema version, review digest, and policy marker.
2. Resolve the workspace owner's runtime model and wallet; recheck current membership and fleet scope.
3. Verify declared connections and credentials by name without putting secret values in the candidate or response.
4. Create or locate the required approval requests. A pending/rejected grant produces a blocked check, not ready access. Repeated setup does not duplicate approval requests.
5. Complete required setup resources under the retained identity. External calls run before database locks are held.
6. Atomically compare installing state, setup revision, policy marker, and approved access. Check affected rows. Only this guarded update may activate the fleet.
7. Commit the requested initial turn with activation, or record its refused outcome. Acceptance follows this commit. Attempt the wake notification after commit, then register schedules independently.

Each external setup call has a declared timeout. A request has a 15-second setup budget and returns `201 installing` if the fleet exists but required checks remain. Five-second external-call limits fit this budget; after it is spent, the daemon keeps pending checks for explicit retry.

Malformed source or mismatched review fails before creating a runnable fleet. A database outage may leave the client uncertain whether the accepted identity committed: same-key retry resolves that uncertainty. A platform outage cannot become a guessed success.

A stored partial result exposes checks and next actions through GET fleet.

#### Initial run and triggers

`none` creates no initial turn. `run_now` commits one turn with activation and returns its stable turn identity. A lost wake leaves `start.state` accepted.

A refused initial turn may leave the fleet active with `start.state` blocked and its error. Retrying setup keeps the initial turn identity. A person can explicitly send another fleet message later.

Cron is registered after activation, so no pre-activation callback queue exists. GitHub and Slack can observe activation immediately; no claim is made that the initial run completes before other work arrives. Existing affinity and turn ordering apply.

A schedule push failure leaves an active fleet with `sync:"failed"` and Retry sync. A push still unfinished when Save stops waiting (5 s) stays `sync:"syncing"`. No sweep re-drives it, and its 30 s fence run refuses a `:sync` retry until it expires.

GET fleet and CLI expose both independently from required activation checks.

#### Retry, archive and delete

Same-key POST or `PATCH fleet {status:"active"}` with the current resource `If-Match` resumes missing setup checks. Approving a setup grant through the dashboard's approval route runs the same step (`afd_api_tenant`'s approval resolve). It also syncs the fleet's trigger schedules, so the fleet activates with no further request.

The signed approval webhook (`POST /v1/webhooks/{fleet_id}/approval`) resolves the gate but carries no person to run setup as. After an approval there, the next Save or `PATCH` `active` activates the fleet.

PATCH stays status/name only. Concurrent setup requests share the same guarded stages and initial turn identity.

A name edit during installing invalidates the pending review and refuses activation until a newly reviewed revision is applied. It cannot silently change accepted creation inputs.

Archive uses existing fleet:write authority. The transaction changes fleet state and setup cancellation generation before a late activation can commit. The linked draft returns to `editing`.

Pending approval requests are cancelled under the fleet lifecycle guard. An archived fleet takes no new run or grant use, and its history stays readable.

If activation already won and advanced the resource tag, a stale archive receives 412. The person must refetch before deciding to archive the active fleet. Another Save has a fresh key and fleet ID, and the cancelled key keeps referring to the archived fleet.

Fleet DELETE requires fleet:admin and works in any status: it releases live work as archive does, then purges. Deleting private draft text is not permission to delete the shared fleet. Draft removal cancels private work and hides its prefix; shared installing checks stay recoverable through the fleet by authorized members.

#### Applying an improvement

Improve copies an immutable fleet setup revision into a creator-private draft. POST fleet revisions carries `If-Match: setup_etag`, one header replay key, candidate ID, and review digest. It atomically checks target/source revisions and updates the setup pointer, policy marker, and affected inherited schedules.

A stale base or digest refuses the whole update. The response is the new setup tag and schedule states; publication stays separate.

Already held runs keep captured setup except live revocation and renewed budget ceilings. New runs use the reviewed new revision. A policy marker drift blocks new turns until a human reviews and applies a revision.

A name/status resource change does not falsely make setup content change; inherited schedule tags include instance setup and recipe/override revision.

In the browser, **Edit setup** opens that private draft beside the current fleet conversation.
Multiple messages and manual instruction edits update the same candidate.
The instruction editor’s **Save changes** applies the reviewed revision and returns focus to the fleet conversation.

A stale setup tag keeps the proposed edits for review.
An initial chat **Save** opens the same fleet Chat workspace.
Both purposes share the [provider reader and browser chat implementation](shared_chat_verification.md).

#### CLI partial success

The CLI displays `Fleet created: installing`, each missing check and its action, and the fleet ID. Approving a missing grant retries setup on its own; after fixing any other check, `agentsfleet fleet activate <FLEET_ID>` retries it. It exits nonzero, keeping the creation replay key for explicit retry.

An active fleet with a blocked initial run prints the partial outcome and an actionable retry. So does any schedule not `synced` (`failed`, `syncing`, or a status the CLI does not know). The CLI exits nonzero rather than claiming all requested work succeeded.

### B. TRIGGER  (steer / webhook / cron — three callers, ONE ingress)

The heading keeps its old words because other pages cite it by text. Seven producers enter here, not three.

Workspace connection proofs: [connectors.md](./connectors.md) §"GitHub App: platform setup to fleet runs".

[Conversation compaction](context_lifecycle.md) continues a run without a new turn. Only an approved gate writes a continuation turn.

```
   Common turn fields, stored in thread.turns.
   The row's UUIDv7 id is the canonical turn identity across reassignment.

       actor         steer:<user> | steer:api | webhook:<source> | github-app
                     | cron:<schedule_id> | continuation:<turn_id> | slack:<user>
                     | fleet:<fleet_id>  (scenarios/chief-of-staff.md)
       kind          chat | webhook | cron | continuation
       workspace_id  <uuid>
       request       <opaque JSON — the message + metadata>
       created_at    <RFC 3339 in UTC; when the turn row was stored>
       id            <uuid>, the turn's identity

   STEER     agentsfleet fleet steer <fleet_id> "morning health check"
               → POST /v1/.../fleets/{id}/messages
               → one write (afd_turns's admit_one.sql), then one read of run facts:
                 INSERT thread.turns status=queued
                      actor=steer:kishore  kind=chat
                      workspace_id=<ws>    request=<msg>
                      created_at=<rfc3339>
               → 202 { turn_id, disposition }   ← CLI uses turn_id
                                                   to filter SSE frames
                 disposition: started | folded (+ folded_into_turn_id) | queued.
                 A folded message is taken by the live run at its next
                 model call and its turn gets the run's reply.

   FLEET      (scenarios/chief-of-staff.md): a fleet's run calls
   STEER        fleet_steer → POST /v1/runners/me/runs/{run_id}/steers
               → the same INSERT: actor=fleet:<sender_fleet_id>  kind=chat
                 key <target_fleet_id>:<sender_turn_id>:<text digest>
                 reply copied from the sender's turn; target is any fleet in
                 the sender's workspace; the write proves the run

   GITHUB    App posts pull_request or workflow_run
   APP         → POST /v1/ingress/github
                 verify platform github-app.webhook_secret BEFORE payload read
                 installation.id → connector.installs → workspace
                 repository.full_name + event + approved grant
                    → active fleet subscriptions
                 authenticated-body-digest/fleet replay slot
               → INSERT thread.turns for each exact match
                      actor=github-app      kind=webhook
                      workspace_id=<ws>     request=<normalized-json>
                      created_at=<rfc3339>
               → 202

               `repositories` is required and fail-closed, with replay per fleet
               ([connectors.md](./connectors.md) §"Repository and event subscriptions belong to fleets").

   MANUAL     Custom providers and the old GitHub workflow_run path keep
   WEBHOOK      POST /v1/webhooks/{fleet_id}/{source}   (one per webhook trigger)
                 POST /v1/webhooks/{fleet_id}/github
               with a workspace credential's `webhook_secret` field. The named
               stored trigger selects its registry or declared recipe. Svix uses
               this same source URL and claims its signed delivery identifier;
               other recipes claim the authenticated body digest. The fleet
               identifier is already in the URL, so this route does not require
               `repositories` and does not use `connector.installs`.

               The internal Clerk endpoint that bootstraps our own tenants
               on `user.created` is NOT this surface. Its path is in the
               auth family (`POST /v1/auth/identity-events/clerk`), but
               the ingress plane serves it, because the caller is a vendor
               presenting a signature rather than a bearer token. The
               `/v1/webhooks/` and `/v1/ingress/` namespaces are
               customer-data-plane only.

   CRON      QStash calls POST /v1/ingress/qstash/schedules
               → agentsfleetd verifies the signature with the current or
                 next signing key it reads from the admin workspace's
                 `qstash` bag
               → resolves the schedule header and checks current schedule/Fleet state
               → INSERT thread.turns with the scheduler replay key
                      actor=cron:<schedule_id>  kind=cron
                      workspace_id=<ws>        request=<schedule-event-json>
                      created_at=<rfc3339>

   CONTINUATION  an approved gate resumes the run it parked
               → INSERT thread.turns keyed by the gate's action
                      actor=continuation:<parked_turn_id>  kind=continuation
                      workspace_id=<ws>  request=<continuation-msg>
                      reply copied from the parked turn (Reply::Inherit)
                 The new row carries resumes_turn_id=<parked_turn_id>;
                 the parked row is never reopened (afd_approval's inbox resolve).

   TASK          a durable task's matched completion or wake
   CONTINUATION  → INSERT thread.turns keyed by task, generation and wake
                 (producer task_continuation; durable_tasks.md)

   All seven producers commit the same turn shape to one ledger. The
   reasoning loop never branches on actor. Actor is metadata for the
   fleet's instructions and the user's history filter.

   > [!NOTE]
   > SLACK — producer `slack_mention` (M206_002, afd_api_ingress's mention
   > handler). One more producer into THIS same ingress; the assign/run path
   > does not change. Routing lives in scenarios/slack-incident-responder.md §4;
   > memory continuity in [`runner_fleet.md`](./runner_fleet.md) §"Memory
   > continuity". Spec: docs/v2/done/M106_001_P1_API_DOCS_INFRA_UI_SLACK_RESIDENT_CHANNEL_BOT.md

```

#### QStash owns the clock

The callback signature covers its destination URL, exact body and message identity.
Ingress selects the schedule through `Upstash-Schedule-Id`, outside that signature; cryptographic target and revision binding stay unresolved.
Schedule activity snapshots describe database state at acceptance.

Schedule history uses `afd_api_wire::activity::FleetActivity` and the existing `afd_turns::activity` receipt writer.
`afd_cron::store::history` records create/edit/pause/removal intent and fenced sync outcomes on the schedule transaction.
Setup recipes append receipts to the author's transaction; confirmed deletion keeps the held description and message.

The transaction commits before its connection is released and the existing catch-up notice is published.
These receipts admit no work and create no Durable Task.
The history scope uses `FOR NO KEY UPDATE`, allowing a session's foreign-key check to progress during child-row waits.

An accepted fire stores its message and resolved schedule description on the original cron turn.
`afd_turns::History::thread_page` includes queued `ScheduleFire` and `Steer` turns, then reads the same identity after running.

The app renders the message and recorded outcome directly; native run details become available once the turn opens.
Historical receipt snapshots stay separate from current registration and retry controls.

The [durable repair scenario](scenarios/durable-repair.md) uses this existing scheduling path for Grafana observations. CI continuation needs a durable task wait and matching completion/fallback wake through the same intake path. A cron does not keep PR ownership or task limits.

`agentsfleetd` stores the desired schedule, pushes each mutation to QStash synchronously, and receives the fires. The runner owns no schedule timer ([`runner_execution.md`](./runner_execution.md#tool-catalog)).

- **Three authors.** A schedule's `source` is `trigger`, `api` or `fleet`. `trigger` is the cron trigger in the reviewed setup, `api` a person, and `fleet` the fleet itself through its run's schedules verb.
- **A fleet edits only its own.** The fleet is the run's, so a body cannot name another. A fleet holds at most 16 schedules it made and changes or deletes only those.
- **Every write proves the run again** on its own transaction (`afd_fleet::run::write_fence`), holding the run row shared until commit. A reassignment, renew or settle waits for it. An old holder resumed after a reassignment writes nothing: `UZ-RUN-005`, `current_state` `superseded`.
- **Run-now** admits through the same `schedule_fire` producer a QStash fire does, keyed `run:<turn_id>` by the held turn, so a reassigned run replays the fire. Both record `actor=cron:<schedule_id>`. Its refusals are listed in API reference › Runner plane (Run a schedule now).
- **A schedule's runs** are its history rows under that actor, read through the `idx_turns_thread_id_actor_created_at_id` index. A page never walks the fleet's whole history.
- **A `once` schedule** (slot 1750) retires inside the fire seam: the fire is admitted, then the schedule is claimed `deleting` and removed from QStash. Its fire is keyed by the schedule alone, so a run-now racing the QStash fire replays it rather than admitting a second run.
- **A dropped `once` fire** (the fleet takes no work) retires the schedule too, since its moment has passed. A retirement whose claim is held answers `UZ-SCHED-006`, so QStash repeats the fire.
- **`fire_at`** (slot 1750) keeps the instant a `once` row was set for. A sync after that instant removes a `once` QStash never registered, rather than registering an expression whose next match is a year away. One QStash already holds stays, so a delayed callback finds it.
- **A pause** that removes a one-off upstream puts its key back to its own id, so a resume after its moment retires it too.

#### The webhook auth taxonomy

Rejections carry `UZ-WH-020` (misconfigured), `UZ-WH-010` (bad signature) and `UZ-WH-011` (stale timestamp); meanings and fixes are in [error codes](https://docs.agentsfleet.net/api-reference/error-codes#UZ-WH-020).

There is no Bearer fallback. The `Authorization` header is never consulted on `/v1/webhooks/…` routes. See [`../AUTH.md`](../AUTH.md) §"Manual fleet-webhook auth" for the full surface.

### C. EXECUTE  (run → runner → report)

The gateway assigns a turn's run and pushes a native `Down::Run`.
The native report commits its terminal outcome.

1. Refuse placement when the runner's isolation verdict is degraded or unreadable.
2. Probe eligible active fleets and each thread's oldest unsettled turn through `idx_turns_thread_id_sequence_unsettled`.
3. Prefer held fleets, recent hints and previous placement; randomize the remaining candidates. Try at most 64 candidates per assignment.
4. Lock the fleet's `thread.threads` row and its exact oldest turn with `SKIP LOCKED`. A locked or parked head cannot be skipped within that fleet.
5. Raise the thread's fence and stamp it on the turn as `run_fence` in one statement. Decode before committing; malformed stored data rolls back the fence.
6. The same statement opens the turn (`opened_at`, status `received`). Terminal refusals settle the turn without issuing a run.
7. Resolve the payer, balance, budget, approvals and installed policy. A wait for a person parks the head and releases the unissued thread.
8. Resolve declared tool secrets separately from the model provider key. Build `ExecutionPolicy` and insert the `thread.runs` row under the raised fence.
9. Run the run in the supervisor and sandbox. Forward live activity over the runner's connection; report the outcome and measured usage when the run ends.
10. Commit the report under the host, thread, run and turn guards. One transaction records usage, the answer, `resume_context`, turn settlement, thread release and any outbound delivery.
11. After commit, publish terminal activity and attempt outbound delivery.

**Folded messages.** A message written while the fleet's run is live answers `folded` with `folded_into_turn_id`.

At its next model call the running run takes a person's later unsettled messages in its own conversation. The input claim stops at the first parked, task-bound or non-person turn. It stamps `input_claim_run_id` and `input_claim_fence` on each turn it takes.

A durable-task run follows the same rule through its own input claim.

When the run reports succeeded, each folded message a model call read gets the run's reply on its own turn. That turn settles in the report's transaction. A taken message no completed call read stays unsettled, and the fleet's next run takes it in order.

A repeated committed report acknowledges the original result without charging again.
A superseded report answers `UZ-RUN-005` and changes no durable state.
A failed report transaction leaves the turn unsettled and its other writes rolled back.

A runner that disappears leaves an unsettled turn. Once its run expires, another assignment can run that turn again under a higher fence.

The turn keeps its original time and billing identities. The previous holder cannot settle through the newer fence.

Approval resolution or expiry unparks eligible heads and attempts a wake notification.
The 5 s recovery probe reads Postgres too, so notification loss cannot leave that work stranded.

See [the queue design](./datastore_scaling.md#the-admissions-ledger-is-the-queue) for budgets, probe costs and recovery limits.
See [provider key boundaries](./billing_and_provider_keys.md#82-the-api_key-visibility-boundary) for plaintext ownership.
The [runner sequence](./runner_execution.md#one-run-end-to-end) owns sandbox startup, agent runs and teardown.

A recorded reply destination creates an outbound delivery with the report.
The outbound worker posts that answer to its connector thread; [the Slack flow](./scenarios/slack-incident-responder.md#6-how-the-answer-comes-back) shows the destination.

### D. WATCH  (user-side: how the live tail surfaces)

```
   CLI       agentsfleet fleet steer <fleet_id> "<message>"   (batch mode)
               → opens GET /v1/.../fleets/{id}/turns/stream (SSE) BEFORE
                 posting the message, and waits (bounded, 2 s) for response
                 headers. The hub flushes the subscription to the socket before
                 the stream returns. NATS interest reaches other nodes later,
                 so an early frame can still race the subscription; the
                 opening catching_up { dropped:0 } covers it.
               → the hub shares one NATS connection across viewers;
                 the response owns a bounded broadcast receiver.
               → frames arriving before the 202 names the turn wait in a
                 bounded client-side buffer (drop-oldest) and replay in
                 order once the id is known; a tail that misses the ready
                 bound is closed unheard and the durable turns list alone
                 decides the outcome (a late tail must never pass a
                 truncated reply off as complete).
               → on disconnect: release the receiver; the last reader's drop
                 aborts that subject's pump, which unsubscribes.

   UI        Fleet Console /fleets/{id}
               → browser EventSource opens the same-origin
                 /live/v1/workspaces/{ws}/fleets/{id}/turns/stream proxy.
               → one registry entry owns the connection and retry timers.
                 Connection attempts expire after 30 seconds. Open connections
                 expire after 45 seconds without a heartbeat or application frame.
                 Named heartbeat events arrive every 15 idle seconds, without ids
                 or database queries. They update timing, not React snapshots.
               → actual arrivals spanning 30 seconds establish stability and
                 reset retry history. HTTP open alone never confirms recovery.
                 Heartbeats prove HTTP transport, not NATS publisher health.
               → after a stable connection closes, the displayed status keeps
                 last-known health for up to 25 seconds while reconnection runs.
                 Browser recovery signals preserve that deadline. Sustained
                 failures show a notice; automatic retries continue alongside
                 the optional Retry now action. Backfill stays best effort.
               → deploy the named-heartbeat server before the watchdog client.
                 Old clients ignore named keepalives; new clients need them to
                 distinguish quiet streams from silent network failures.
               → the core chat data starts with two reads: fleet detail
                 (status, pending_approvals) and GET /messages?limit=20 (the
                 thread, bodies included). The summary strip is a view
                 over the stream from there: turn_complete carries the
                 terminal row plus fleet_status and pending_approvals,
                 gate_opened / gate_resolved carry the count, and the
                 reconnect backfill's durable rows fold in the same way;
                 no read is issued to move a figure. Only a fleet_status
                 that differs from the server-rendered one re-runs the
                 server tree, once, for the header's lifecycle controls.

   UI        Fleets Wall /fleets
               → opens ONE same-origin
                 /live/v1/workspaces/{id}/turns/stream SSE connection
                 shared by the workspace's fleet tiles.
               → agentsfleetd authorizes the workspace and fans in only its
                 readable af.thread.<thread_id>.items subjects through bounded
                 per-subject broadcast receivers.
               → frame kinds and control frames: API reference › Workspaces
                 (Stream live activity for a whole workspace).
               → if the NATS client reconnected, reported a slow consumer,
                 or a publisher's Af-Gap marker arrived, agentsfleetd sends
                 catching_up { dropped:0 }, on this stream and on the
                 per-fleet one.

   Browser authentication
     EventSource + session cookie --> Next.js /live/* Route Handler
       --> Clerk auth().getToken()
       --> fetch API /v1/.../turns/stream with Authorization: Bearer JWT
       <-- upstream streaming body <-- agentsfleetd

   The Node.js proxy forwards request cancellation to the upstream fetch.
   It returns upstream error statuses and rejects an absent session with 401.
   CLI clients call the API directly with their Bearer credential.
   No stream token belongs in the URL.

   Sequence ids are per connection and `Last-Event-ID` is ignored (API
   reference › Fleet turns); clients backfill from the turns list with the
   server's `next_cursor`, never one derived from a turn id.

   HISTORY   agentsfleet fleet turns {id} [--actor=…] [--since=2h]
             API GET …/fleets/{id}/turns
               → reads thread.turns (cursor-paginated).

   RESUME    no user-facing command reads thread.threads.resume_context.
             The run path reads it with the fleet row and hands it to no run.
             "Busy or idle" is thread.runs, which has the fencing
             token and the expiry that make the answer trustworthy.

   Both browser registries backfill durable turn rows after reconnect,
   and on any catching_up frame, a lag's or a lost subscription's.
   A publish missed while the HTTP stream stayed open is therefore
   recovered by that backfill rather than lost silently.
   Transient token chunks are not durable history; settled rows are.
   Live tail is best-effort; thread.turns stays the durable record.
```

## Multi-tenancy boundary

| Layer | Tenant isolation mechanism |
|---|---|
| Postgres (`fleet.fleets`, `thread.turns`, etc.) | Enforced in the application, not by the database. `afd_tenant`'s `AUTHORIZE_WORKSPACE` resolves the caller's tenant and the workspace's owner in one statement, and `afd_http`'s ownership layer runs it in front of every route whose path carries a workspace, so no handler can forget it. Every control-plane statement filters by `workspace_id` or `fleet_id` explicitly. **No `ROW LEVEL SECURITY` policy is declared anywhere in `schema/`, and no `current_setting('app.workspace_id')` is read.** Re-adding database policies would need a transaction per request, because `sqlx` returns a connection to the pool between requests and a session-level setting would leak one tenant's identifier onto the next. |
| NATS live activity (`af.thread.<thread_id>.items`) | Subject named by thread UUID (globally unique); no cross-tenant collision possible. NATS has no per-subject authorization here. The protection has the row above's shape. The subject is unguessable, and every path to it passes the ownership check. |
| Runner ↔ control plane | The `agt_r` token authenticates the runner per call, and a run carries one fleet's turn and scoped secrets ([`runner_fleet.md`](./runner_fleet.md) §"Registering a runner"). |
| Runner supervisor and sandbox | Secrets stay in the supervisor, substituted at send time, and a sandbox holds no credential ([`runner_execution.md`](./runner_execution.md) §"Credentials"). |

## One active run per fleet: the ownership model

Before the cutover, a single worker thread owned all work for a Fleet, and the concern was round-robin across worker replicas breaking per-fleet continuity. That model is gone. Ownership is now a **durable run**, not a thread:

- `thread.threads` holds one row per fleet. `Runs::select` in `afd_fleet` assigns it atomically and raises a monotonic `fencing_token`.

  A runner wins only when the thread's prior run has expired. So **at most one run is active per fleet at any time**, however many assignment passes run concurrently.
- The assignment is the only writer of `fencing_token`, so every fencing token a run carries is a value an assignment returned.
- A runner that loses the race for a Fleet simply gets no run for it. It tries the next eligible fleet or backs off.
- Durable fleet memory and bounded chat history can reach another runner’s next run. Warm sandbox reuse is optional; `resume_context` checkpoints are saved but not replayed into runs. [Workspace continuity](runner_execution.md#workspace-between-runs) owns these limits; sticky placement stays a hint, never ownership.

Failure mode: a dead run holder blocks its fleet until `run_expires_at`; reassignment then runs again with a higher fencing token. Recovery latency and its bound: [`runner_fleet.md`](./runner_fleet.md) §"Failure recovery model".

One PR through one run: its records and timers in [run_flow.md §1](./run_flow.md#1-from-start-a-fleet-to-a-run), its reply limits in [§4](./run_flow.md#4-how-agent-bob-01-can-reply), and what bounds the host in [§7](./run_flow.md#7-where-the-image-lives-and-what-bounds-a-bare-metal-host).

## What the coding fleet never does

- Never sees the fleet's LLM tokens or reasoning state
- Never holds the fleet's secrets in its own context
- Never runs the fleet's tool calls in its own session
- Never survives the user's laptop being closed

## What the fleet (host) never does

- Never touches the user's laptop directly
- Never reads the user's local filesystem (it sees only what the fleet's reviewed setup grants it)
- Never escapes the sandbox: namespaces, Landlock, seccomp and cgroup v2 bound each run's sandbox, and the agent loop runs outside it ([`runner_execution.md`](./runner_execution.md#sandbox-engines)). **Network egress** follows the runner's network policy (see [`runner_fleet.md` §Egress model](./runner_fleet.md)).
- Never holds a datastore credential: the runner reaches the platform only over the `/v1/runners` protocol

## The install failure scenario, visually

```text
record key + normalized body + retained installing fleet -> commit
checks or grants incomplete -> installing + missing checks
same key + same body -> resume same fleet and setup revision
Archive wins activation guard -> archived fleet; linked draft returns to editing
activation + initial turn -> commit together
read back the same initial turn -> Accepted
read-back unavailable -> Pending; retry confirms the same turn
register schedules independently
lost wake -> the next 5 s recovery probe still reads the ledger
```

Do not delete the retained fleet on setup failure. Do not report activation after a zero-row update or submit another initial turn on retry.

Queued steer turns take part in ordinary message history alongside opened turns in one statement snapshot. That history carries stable turn identity and pagination (`afd_turns`'s `select_thread_page.sql`). No creation worker is introduced.

## Proposed Durable Task admission

M236 added [Durable Tasks](durable_tasks.md) beneath fleets while keeping this turns ledger and one active run per fleet.
The daemon saves a message before acknowledging it; the runner then uses [runner-assigned decision routing](goal_evaluation.md#intent-routing) under its run.
Native code creates work or associates input only after current authority and target checks.

A waiting Durable Task (DT) has a settled turn and a separate wait record.
Another queued objective can use the fleet's thread; a matched completion creates one continuation turn.
Neither model classification nor an R2 checkpoint replaces queue or ownership authority.
