# Data Flow — how an event moves through the system

> Parent: [`README.md`](./README.md) · Sibling: [`runner_fleet.md`](./runner_fleet.md) (the structural split this flow runs on). · User-facing: [docs.agentsfleet.net/fleets/webhooks](https://docs.agentsfleet.net/fleets/webhooks) (sending an event) and [docs.agentsfleet.net/fleets/running](https://docs.agentsfleet.net/fleets/running) (watching one).
>
> **Scope:** this file describes the runtime as it runs now — after the M80_002 cutover. `agentsfleetd` is the **control plane** (owns Postgres, Dragonfly, the Vault, the HTTP API, and work assignment); the host-resident **`agentsfleet-runner`** daemon is the **execution plane** (leases work over Hypertext Transfer Protocol Secure (HTTPS), runs the agent loop in its supervisor and the lease's tool calls in a sandbox, reports back; [`runner_execution.md`](./runner_execution.md#process-model)). The single-process `agentsfleetd worker` loop and the standalone sandbox sidecar are deleted. See [`runner_fleet.md`](./runner_fleet.md) for the why and the guarantees.

Read this when you need to know where a webhook, a steer, or a cron fire ends up. Many specs reference this file as the canonical picture of the runtime.

## Facts

Every row is extracted from the sections below; the owner column names the section that carries the full story.

| Invariant | Value | Mechanism | Owner section |
|---|---|---|---|
| Event ingress | ONE — six producers | `steer` / `webhook` / `webhook_app` / `schedule_fire` / `gate_continuation` / `repair_verification` (`rustd/crates/afd_admission/src/lib.rs:81-106`) each commit a `core.fleet_admissions` row, then `XADD fleet:{id}:events`; the LEDGER row carries the canonical event id and the stream entry id is its receipt. Slack mentions reach admission through `rustd/crates/afd_api_ingress/src/handler/mention.rs` (M206_002) | §B. TRIGGER |
| Hot-path writes | 12, in the worker's order | `lease` does 1–6, `report` does 7–12; row-equivalent to the deleted worker (cutover Invariant 2) | §Steer flow end-to-end |
| Durable stores | 5 tables, join key `(fleet_id, event_id)` | `fleet_admissions` (one row per acceptance, UNIQUE `(producer, producer_key)`) · `fleet_events` (one row per delivery) · `fleet_obligations` (one row per answer, UNIQUE `(fleet_id, event_id)`) · `fleet_sessions` (one row per fleet, UPSERT) · `billing.usage_ledger` (two rows per FLEET-event — a receive and a stage — UNIQUE `(event_id, charge_type, fleet_id)`, so one event id charged by two fleets holds four rows, not two) | §The five durable stores |
| Replay safety | idempotent | `INSERT … ON CONFLICT DO NOTHING` + the UNIQUE telemetry `event_id` | §C. EXECUTE |
| Stale-writer rejection | `UZ-RUN-005` | `Leases::claim_and_settle` fences, flips, settles and dedups in one statement | §C. EXECUTE |
| Shared Dragonfly handle | one multiplexed connection per daemon | short-lived commands only: `XADD`, non-blocking `XREADGROUP`, `PUBLISH`, `XACK` | §Connection topology |
| Dedicated Dragonfly connections | hub + outbound reader | refcounted `SSUBSCRIBE`; blocking outbound reads use a separate socket | §Connection topology |
| Postgres acquire failures | 2 distinct errors | `PoolTimeout` (capacity) vs `PoolUnavailable` (datastore); one connection per handler (RULE CNX) | §The Postgres pool |
| Config freshness | read per lease | a `PATCH` takes effect on the next lease; no cache, no signal | §Config reload |
| Gate-blocked rows | terminal | never reopened; the resolved gate lands a NEW row via `actor=continuation:<original>` | §"C. EXECUTE" step 3 |
| Install guarantee | stream + group before 201 | `ensure_stream` retries with jittered exponential backoff (200 ms base, 1500 ms cap, 4 attempts, ~1.4 s); exhaustion rolls back the PG row | §A. INSTALL |
| SSE sequence ids | not durable | per-connection counter, resets to 0; `Last-Event-ID` ignored; backfill via the events list | §D. WATCH |
| Client gap recovery | reconnect, or `catching_up` on either stream | bounded `fleet_events` list `since` last delivery − 2 s overlap, merged by event id; a lost server subscription arrives as `catching_up` with `dropped: 0` | §Two streams + one pub/sub channel |
| Cron authority | QStash | signature verified at ingress; replay suppressed atomically; the runner owns no timer | §B. TRIGGER |
| Cancel latency | none: a running lease is never cancelled | kill and pause write only `core.fleets.status`; the run ends on its own, at its fleet's stored budget ceiling, or at `MAX_RUNTIME_MS` | [`runner_fleet.md`](./runner_fleet.md) §"Steer, kill, pause" |
| Lease ownership | at most one active lease per fleet | atomic `runner_affinity` claim + monotonic `fencing_seq` | §One active lease per fleet |
| Provider `api_key` | never in `secrets_map` | rides `ExecutionPolicy.provider` + `.api_key`; injected for the inference call only | §"C. EXECUTE" step 4 |
| Tenant isolation | application-enforced + namespacing | every workspace route passes an ownership check before its handler runs; Dragonfly keys namespaced by unguessable fleet UUID. This repository declares no `ROW LEVEL SECURITY` policy | §Multi-tenancy boundary |

## Traps

Each trap is enforced in its owner section; this list is the index.

- The live tail is the eyeballs surface, not the audit surface; durable history is `core.fleet_events` (§Two streams + one pub/sub channel).
- A connection held across a blocking `SUBSCRIBE` can never return to a pool (§Connection topology).
- Never acquire a second Postgres connection while holding one — that is how a pool deadlocks (§The Postgres pool).
- The Postgres pool has no ordering or fairness guarantee; do not assert one (§The Postgres pool).
- `gate_blocked` rows are NEVER reopened (§"C. EXECUTE" step 3).
- A Pull Request reviewer cannot post a review or a comment: no egress write rule admits one ([`lease_flow.md`](./lease_flow.md) §"4. How AGENT BOB 01 can reply").
- Nothing coalesces per Pull Request: every push is its own admitted event, queued behind the fleet's running lease ([`lease_flow.md`](./lease_flow.md) §"5. The PR is fixed and pushed again").
- Never carry a separate event id in the payload — the stream entry id IS the canonical event id (§B. TRIGGER).
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
- the steer round-trip with the 12 writes — §Steer flow end-to-end
- the Dragonfly connection topology — §Connection topology
- install, trigger envelope, execute, watch — §"End-to-end sequence" — A through D
- the install failure window — §The install failure scenario, visually

## Decisions

| Decision | Reason | Where / artifact |
|---|---|---|
| Two per-delivery tables (`events` + `telemetry`) | different write authorities and retention rules | §The five durable stores |
| `fleet:control` removed | no per-fleet threads left to orchestrate | §Two streams + one pub/sub channel |
| Dedicated Dragonfly tier collapsed | idle cost now tracks lease-poll frequency, not fleet count | §Connection topology; M80_002 |
| A pool acquire answers a typed error, not an absent connection | `PoolTimeout` and `PoolUnavailable` are different operator pages | §The Postgres pool |
| Gap recovery is client-side, not server resume | no channel or frame-shape change; the durable table is the recovery source | §Two streams; M122 |
| QStash owns the clock | the runner owns no schedule timer | §B. TRIGGER |
| Upload-bundle picker path deferred | Indy-acked 2026-06-20 | [`user_flow.md`](./user_flow.md) §"§8.2.3 Fleet Bundle dashboard flow" |
| Watcher reconcile sweep deleted; orphan stays inert | no runner can lease a fleet with no events group; a future reconcile job heals it | §The install failure scenario |
| SSE auth is dual-accept with strict no-fallthrough | a stale cookie must not silently fall through to a valid Bearer | §D. WATCH |
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
| **`agentsfleetd-api`** (`agentsfleetd serve`) | The control plane. HTTP routes for the user surface **and** the `/v1/runners` machine surface. Owns Postgres, the Dragonfly pool, and the Vault. Steer, webhook, cron, and continuation handlers each commit an admission row and then `XADD` to `fleet:{id}:events` — single ingress, and the row is what makes the acceptance durable when the append does not land. On `lease` it does a non-blocking `XREADGROUP` to claim the next event, runs the gates + billing + secret resolution, and issues a `fleet.runner_leases` row; on `report` it persists the terminal state and `XACK`s. It is the sole `PUBLISH`er on `fleet:{id}:activity`. Never runs language-model code. |
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

```
                "what's the deploy status?"
                          ↓
         Coding Fleet → agentsfleet steer <fleet_id> "<msg>"
                          ↓

           ╔════════════════════════════════════════╗
           ║  agentsfleetd-api (HTTP)               ║
           ║  POST /v1/.../fleets/{id}/messages     ║
           ║  ────────────────────────────────────  ║
           ║  INSERT core.fleet_admissions          ║   ← the acceptance;
           ║                                        ║     the XADD is its
           ║                                        ║     receipt.
           ║  XADD fleet:{id}:events *              ║   ← single ingress.
           ║       actor=steer:<user>               ║     Webhook + cron use
           ║       type=chat                        ║     the same XADD.
           ║       workspace_id=<uuid>              ║
           ║       request=<msg-json>               ║
           ║       created_at=<epoch_ms>            ║
           ║  PUBLISH fleet:{id}:activity           ║   ← every screen shows
           ║    {kind:"event_admitted",             ║     the message, waiting
           ║     event_id, actor, message}          ║     (steers only; a
           ║                                        ║     repeat publishes none)
           ║  → 202 { event_id }                    ║
           ╚════════════════════════════════════════╝
                          ↓
        ( the event waits on the stream until a runner asks for work )
                          ↓
           ╔════════════════════════════════════════╗
           ║  agentsfleet-runner (host)             ║
           ║  POST /v1/runners/me/leases            ║   ← single poll; no work
           ║  Authorization: Bearer agt_r           ║     → null + retry_after_ms
           ╚════════════════════════════════════════╝
                          ↓
           ╔════════════════════════════════════════╗
           ║  agentsfleetd (lease handler)          ║   ← the work the worker
           ║  ────────────────────────────────────  ║     used to do, now on
           ║  Leases::select():                     ║     the request thread:
           ║   non-blocking XREADGROUP across       ║
           ║   active Fleets (sticky pref) →        ║   ← narrative log opens
           ║   claim fleet.runner_affinity,         ║     (mutable)
           ║   issue monotonic fencing_token        ║
           ║  1. INSERT core.fleet_events           ║   ← live: pub/sub frame
           ║     (status='received')                ║     (ephemeral, no ACK)
           ║  2. PUBLISH fleet:{id}:activity        ║
           ║     {kind:"event_received"}            ║   See
           ║  3. balance gate, receive debit,       ║   [`capabilities.md`](./capabilities.md)
           ║     approval gate; run metered per     ║   for each gate layer.
           ║     /renew, settled at report          ║
           ║  4. resolve secrets_map from vault     ║
           ║  5. READ core.fleet_sessions           ║   ← read with the fleet
           ║     context_json                       ║     row; handed to no lease
           ║  6. issue fleet.runner_leases row      ║
           ║     (lease_expires_at, fencing)        ║
           ║  → 200 { lease }                       ║   (shape: API reference
           ║                                        ║    › Runner plane)
           ╚════════════════════════════════════════╝
                          ↓
           ╔════════════════════════════════════════╗
           ║  agentsfleet-runner (supervisor)       ║
           ║  ────────────────────────────────────  ║
           ║  supervisor: runs the agent loop over  ║       This is the
           ║  the policy and holds the model key;   ║       Fleet runtime.
           ║  starts the lease's sandbox only when  ║       An LLM whose
           ║  a tool the policy offers needs one.   ║       tools run in a
           ║                                        ║       sandbox; the coding
           ║  Each tool call → the router runs it   ║       fleet never becomes
           ║  where its runtime says. http_request: ║       it, never sees its
           ║  the supervisor's egress guard puts    ║       tokens or context.
           ║  ${secrets.NAME.x} in place at send    ║
           ║  time. shell, file, git: the sandbox.  ║
           ║                                        ║
           ║  Each event → an activity frame:       ║   ← supervisor posts
           ║     - tool_call_started                ║     frames to agentsfleetd
           ║     - fleet_response_chunk             ║     .../activity, which
           ║     - tool_call_completed              ║     PUBLISHes them.
           ║                                        ║
           ║  The loop's answer becomes the report: ║
           ║  → {response_text, tokens, telemetry,  ║
           ║     outcome}                           ║
           ╚════════════════════════════════════════╝
                          ↓
           ╔════════════════════════════════════════╗
           ║  agentsfleetd (report handler)         ║
           ║  POST /v1/runners/me/reports           ║
           ║  ────────────────────────────────────  ║
           ║   claim_and_settle: atomic CAS —       ║   ← fence + flip + dedup
           ║     UPDATE runner_leases               ║     in one statement
           ║     SET status=reported                ║     (stale token → reject
           ║     FROM runner_affinity               ║      UZ-RUN-005)
           ║     WHERE status=active AND            ║
           ║       fencing_token >= fencing_seq     ║
           ║   7. UPDATE core.fleet_events          ║   ← narrative log closes
           ║      status='processed'                ║     (same row)
           ║      response_text=<content>           ║
           ║   8. PUBLISH fleet:{id}:activity       ║   ← live: terminal frame
           ║      {kind:"event_complete"}           ║
           ║   9. INSERT core.fleet_execution_      ║   ← billing/latency
           ║      telemetry (reconcile actuals)     ║     audit (UNIQUE event_id)
           ║  10. UPSERT core.fleet_sessions        ║   ← resume cursor:
           ║      context_json                      ║     advances the
           ║  11. XACK fleet:{id}:events            ║     bookmark
           ║  12. release affinity (token-guard)    ║
           ║  13. INSERT core.fleet_obligations     ║   ← the answer is OWED
           ║      receipt=NULL delivered_at=NULL    ║     (in the transaction)
           ║  14. XADD connector:outbound           ║   ← after the commit
           ║  15. UPDATE …obligations SET receipt   ║   ← the entry id, recorded
           ╚════════════════════════════════════════╝
                          ↓
   Coding Fleet's `agentsfleet steer <fleet_id>` polls GET /events
   (or SSE-tails GET /events/stream which SUBSCRIBEs
    fleet:{id}:activity)
                          ↓
       [claw] <the Fleet.s response, streamed>
                          ↓
                  User reads it.
```

The 12 numbered writes are the deleted worker's `processEvent` effects, in the same order, split across two calls: `lease` does 1–6, `report` does 7–12. The handlers under `rustd/crates/afd_api_runner/src/handler/runner/` mirror the old `event_loop_writepath`. Row equivalence (cutover Invariant 2) keeps history, billing, and the SSE tail byte-identical.

**The report's Postgres writes do not commit independently.** The claim-and-settle, write 7, write 10 and write 12 ride ONE transaction: the money, the run's result, the resume cursor and the freed slot commit together or none of them does. That is not tidiness. Split, the window between the settle and write 7 is a window in which a tenant is charged for a run whose answer is nowhere — and nothing recovers it, because the lease is already `reported` and every retry is refused. Inside one transaction a failure at any statement leaves the lease `active` and the wallet untouched, which makes the runner's retry the recovery path instead of a permanent conflict.

The queue is **not** in that transaction and cannot be: nothing spans Postgres and the datastore. Write 11 (`XACK`) and write 8 (the activity frame) run after the commit. That ordering is the safe direction — an acknowledgement a rollback then un-did takes an entry off the stream with nothing durable to show for it, where an acknowledgement that never lands leaves the entry pending and redelivered against durable terminal state.

**Write 13 is the delivery obligation, and it rides the same transaction for the same reason.** Sending the answer is the last thing a run is for, and until this row existed the queue append was simply the next thing that happened after the commit — a process dying in that gap left a run committed, charged and answered, with the answer existing nowhere: not on the queue, which never got the entry, and not in Postgres, which recorded only that the run finished. The row is that missing record. `receipt` and `delivered_at` start NULL, and the two NULL tests are the whole status vocabulary:

```
   receipt IS NULL              → committed, never queued
                                  (a crash between 13 and 14)

   receipt, delivered_at NULL   → queued, nobody received it
                                  (lost group, lost stream, replaced host)

   delivered_at IS NOT NULL     → a person has it
```

Writes 14 and 15 are the fast path and are allowed to fail. What they leave behind is a row in the first state, and `afd_outbound`'s producer re-appends exactly that set every 30s — plus the second state after 5 minutes, which is the one a queue failure leaves. So losing the datastore costs an answer latency and never the answer, which is the property `datastore_scaling.md` states and Dimension 7.8 must prove.

This is the inbound admission ledger pointed outbound. There, the row is the acceptance and the stream entry is a receipt recorded after the fact; here the row is the obligation and the queue entry is a receipt recorded after the fact. Both exist because a stream entry is not a durable record of intent.

**`delivered_at` is stamped by the poster, never by the `XACK`.** The two record different facts: `Lanes::deliver_and_ack` acknowledges an EXHAUSTED job too — deliberately, since leaving it pending would park one undeliverable answer at the head of a destination's lane forever — so an ack-time stamp would mark undeliverable answers delivered and drop them out of the recovery set for good.

A retry that arrives **after** the commit — the response was lost, not the work — finds the lease `reported` by the same runner. It is answered with the stored outcome and charges nothing, so the runner stops retrying with its result safely landed. The lease id is the report's idempotency key, which is why `POST /v1/runners/me/reports` takes no `Idempotency-Key` header. A holder the fleet has genuinely superseded is a different empty claim and still gets `409`: the lease there is not `reported`, it is somebody else's.

## The five durable stores: who owns what

The flow writes five Postgres tables. Each answers a distinct user question and has its own cardinality, mutability, and retention rule. The cutover moved the writer from the per-Fleet worker thread to the lease/report path; shapes and write order did not change.

Two of the five arrived after that cutover and bracket the others. `core.fleet_admissions` records work this deployment ACCEPTED, committed before a producer is told yes. `core.fleet_obligations` records an answer this deployment OWES, committed with the result that produced it. They are deliberate mirror images, and the property they share is the one the whole datastore design turns on: **PostgreSQL records it before Dragonfly carries it, and the stream entry is a receipt written back afterwards.** A stream entry is not a durable record of intent, so anything whose loss would strand work is a row first and an entry second.

Read the primary keys and the shape falls out:

```
  fleet_admissions   PK id  + UNIQUE (producer, producer_key)   one row per ACCEPTANCE
  fleet_events       PK (fleet_id, event_id)                    one row per EVENT
  fleet_obligations  PK id  + UNIQUE (fleet_id, event_id)       one row per ANSWER
  fleet_sessions     PK fleet_id                                one row per FLEET
```

Three of them grow with traffic. `fleet_sessions` does not — it is a cursor, not a log, which is why "where does this fleet resume" is a primary-key lookup rather than a sort over its history.

One event's life across all four:

```
  a message arrives
      │
      ├─▶ fleet_admissions   INSERT  "accepted"          before the producer hears yes
      │                                                  dedupes the PRODUCER's retry
      ├─▶ fleet_events       INSERT  status='received'   the narrative opens
      │                                                  dedupes REDELIVERY
      │   ┌── a runner executes ──┐
      │   │                       │
      ├─▶ fleet_events       UPDATE  status='processed', response_text
      ├─▶ fleet_obligations  INSERT  "owed"              ← same transaction as the money
      ├─▶ fleet_sessions     UPSERT  context_json        the resume cursor moves
      │
      └─▶ fleet_obligations  UPDATE  delivered_at        a person received it
```

The two ledgers are not symmetric, and the asymmetry is **who can retry**. A producer holds its own `producer_key` and repeats it, so an admission deduplicates on the PRODUCER's identity. A destination has no such key — a chat provider cannot tell us "this is the same message" — so an obligation deduplicates on the event that produced the answer, the only stable identity this side owns.

That decides the direction of error, too. An uncertain admission is REFUSED, because a 4xx is what stops a provider retrying. An uncertain obligation is RE-SENT, because the path is at-least-once and a duplicate message in a thread is visible and recoverable by a person, while an answer never sent is neither.

`fleet_events` is also the only one of the four carrying a `status` TEXT column rather than NULL tests. That is not a lapse from the rule the ledgers follow: an event has a genuine vocabulary — `received`, `processed`, `fleet_error`, `gate_blocked`, `balance_exhausted` and the rest — where a delivery has exactly two facts. The spellings live in `afd_core::event::status` and are never literals in schema (RULE STS). Asking "is it still `received`" is how the redelivery path tells a legitimate re-poll from an event that already ran.

| Table | Cardinality | Mutability | Answers |
|---|---|---|---|
| `core.fleet_admissions` | **One row per acceptance** | INSERT, then UPDATE `receipt` and `delivered_at` | "Did we accept this work, and has a runner taken it?" — committed before the producer is told yes, so a lost queue loses no accepted work. `UNIQUE (producer, producer_key)` is what makes a producer's retry one row rather than two runs. |
| `core.fleet_obligations` | **One row per answer** | INSERT in the report's transaction, then UPDATE `receipt` and `delivered_at` | "Do we still owe somebody this answer?" — committed with the money and the result, so no window exists where a run is charged and its answer exists nowhere. `UNIQUE (fleet_id, event_id)` makes a replayed report owe one delivery, not two. |
| `core.fleet_sessions` | **One row per Fleet** | UPSERT — replaced at each report | "Where did this Fleet leave off?" — the resume bookmark, and only that. The lease path reads it with the fleet row and hands it to no lease; the report path replaces it. The table still declares `execution_id` and `execution_started_at`, and **nothing writes or reads either**: "which fleet is executing right now" is `fleet.runner_leases`, which has the fencing token and the expiry that make the answer trustworthy, where a handle with no expiry could only go stale. The columns are left in place for deployments that still carry them. |
| `core.fleet_events` | **One row per delivery** | INSERT (status=`received`) → UPDATE (status=`processed` \| `fleet_error` \| `gate_blocked`) | "What did this Fleet do for event X? Who triggered it, what did they ask, what did it answer, did the gates pass?" — the user's narrative log. The single source of truth for the Events tab and `agentsfleet events`. |
| `billing.usage_ledger` | **Two rows per fleet-event** under the credit-pool model: one `charge_type='receive'` at the receive debit, one `charge_type='stage'` at the run debit (then UPDATEd with token counts after the report). UNIQUE `(event_id, charge_type, fleet_id)`. | INSERT at each debit, immutable for the `credit_deducted_nanos` column; the run row is reconciled once with actual token counts at report. | "How much did event X cost (split by receive vs run)? How fast was it? What posture was charged?" — billing + latency audit. Joinable to `fleet_events` via `(fleet_id, event_id)` — NOT `event_id` alone, which slot 916 made non-identifying: two fleets may hold the same event id, and joining on it would aggregate one fleet's spend into the other's. |

Why two per-delivery tables (`events` + `telemetry`) instead of one? They have different write authorities and retention rules:

- `fleet_events` holds user-readable strings (`request_json`, `response_text`) — large, mutable mid-lifecycle, deletable on tenant offboarding.
- `billing.usage_ledger` holds numeric audit columns — small, immutable once written, retained for billing reconciliation independent of whether the conversation row is purged.

The durable lease bookkeeping (`fleet.runner_leases`, `fleet.runner_affinity`) is a fourth concern — it is the *ownership* layer (which runner holds this event, at what fencing token, until when), not a user-facing record. It lives in the `fleet` schema and never carries user strings.

### The list read and the detail read are different reads

A page is up to two hundred rows, and the two body columns are unbounded — a trigger payload and a full agent answer. Selecting them per row bought a table that renders about a hundred and sixty characters per cell, so the list stops selecting them and the surfaces that used to quote an answer now state an outcome instead. A row that failed says why; a row that succeeded records that it did, without reproducing the reply. Postgres already keeps wide values in oversized-attribute storage, so the cost was never that the bodies existed — it was that the list **selected** them, which is why this is a read-path change and not a storage one.

Three surfaces changed with it: the events table's prose cell, the fleet header's outcome line, and the fleet thread's transcript. The transcript is the one surface that genuinely wants the bodies, because it renders what was said rather than a summary of it — so it re-reads its turns as details, server-side and in parallel. A turn whose detail read fails keeps its list row and renders its header and outcome rather than taking the page down.

`tool_calls`, the run's bounded tool trace (at most 200 calls and 64 KiB; [Runner Fleet, Live activity](./runner_fleet.md#live-activity-the-sse-tail)), is a third body column under the same rule. The transcript and the single-event read select it, and the list never does. A row from before the column existed holds `NULL`, which reads as "not recorded", never as "no tools ran".

A call's full arguments and output are a fourth body that no page read selects. They live in `core.fleet_tool_call_details` (`schema/925`), a child table of `core.fleet_events`, one row per saved call, cascading on `(fleet_id, event_id)`. The rows are keyed by the lease's fencing token and the runner's call number, and a page never reads them: "show all" reads one call through `GET …/events/{event_id}/tool-calls/{call_id}`. That keeps a thread page at its 512 KiB budget however much a run's tools returned.

The runner lease carries no second copy either. It used to hold its own `request_json`, a duplicate of the payload the event row already stored, written on every lease. Reclaim joins `core.fleet_events` on `(fleet_id, event_id)` to read the body instead; both tables cascade from the same parent, so the join cannot dangle.

**Partitioning is not done here, but its key is.** The ledger's uniqueness is fleet-scoped since slot 916 — `(event_id, charge_type, fleet_id)` — so a partition key may carry the fleet without splitting a row's own arbiter. `billing.usage_ledger` carries the originating event's creation time rather than the write time. A renewal firing hours after its receive row would otherwise land in a different partition, miss the conflict target, and silently duplicate ledger rows. Carrying the column now means a later partitioning decision has a stable key already present and needs no backfill; the machinery itself waits for a measurement that demands it.

## Two streams + one pub/sub channel — and the one that retired

Two Dragonfly surfaces carry a fleet's work: a durable stream for ingress, and an ephemeral pub/sub channel for the live tail. A third, `fleet:control`, was removed at the cutover and the last row records why.

| Dragonfly surface | Type | Cardinality | Purpose | Volume |
|---|---|---|---|---|
| `fleet:{id}:events` | Stream + consumer group `fleet_lease` | One per fleet | Single event ingress — steer / webhook / cron / continuation all `XADD` here, with no `MAXLEN`. `agentsfleetd` is now the consumer: a **non-blocking** `XREADGROUP` on each `lease`, `XACK`ed at `report`, and once acknowledged history is more than 100 entries past 1,000, the `XACK` trims it back to 1,000 without ever crossing the oldest pending or undelivered one. A fleet 10,000 entries behind refuses new admissions (503) instead of losing old ones; a lost group is recreated where the ledgers say delivery stopped. Idempotent on replay via `INSERT … ON CONFLICT DO NOTHING`. | High — every event the fleet handles. |
| `fleet:{id}:activity` | Pub/sub channel (no consumer group, no persistence) | One per fleet | Best-effort live tail — `agentsfleetd` `PUBLISH`es `event_admitted` when a person's message is accepted, before any runner has it, then one frame per `event_received` / `tool_call_started` / `fleet_response_chunk` / `tool_call_progress` / `tool_call_completed` / `event_complete`, and `gate_opened` / `gate_resolved` when a human is asked and answers. The bracket and gate frames originate in `agentsfleetd`; the mid-run frames are forwarded from the runner over the `activity` verb. The SubscriptionHub `SUBSCRIBE`s once per channel-with-viewers on its one shared connection and fans frames out by copy into each SSE stream's bounded queue. No buffer beyond those queues, no ACK, no resume. | High during execution, zero when idle. |
| `fleet:control` | (removed) | — | **Removed at the cutover.** It existed to tell the worker watcher to spawn / cancel / reconfigure per-fleet threads — and there are no per-fleet threads anymore. The producer (`control_stream.publish` from the install / status / config handlers) and the dead `control_stream` module were deleted; the install path keeps only `Fleets::ensure_stream` (load-bearing — the `lease` `XREADGROUP` needs the events group to exist). | gone |

`fleet:{id}:events` is durable (events appended, `XACK`ed entries pruned) and backs the at-least-once delivery guarantee. The pub/sub channel is ephemeral and exists only to power live user interfaces — its loss never affects correctness, only what the user sees in real time. Durable activity history lives in `core.fleet_events`; the pub/sub channel is the eyeballs surface, not the audit surface.

**Client-side gap recovery (M122).** Because the channel has no resume, a dashboard tab that drops its Server-Sent Events (SSE) connection misses every frame published during the reconnect window. The stream registry (`ui/packages/app/lib/streaming/fleet-stream-registry.ts`) closes that gap client-side. On every reconnect open — never the SSR-seeded initial connect — it fetches the bounded `core.fleet_events` list, keyed `since` the last server-delivered event minus a 2-second overlap, and merges by event id. The fetch goes through the same-origin token-minting proxy `/live/v1/workspaces/{ws}/fleets/{id}/events` (mirror of the SSE proxy; the `/live/*` prefix keeps these routes outside the `/backend/:path*` rewrite that shadowed them on Vercel). No server, channel, or frame-shape change — the durable table remains the recovery source of truth.

## Connection topology — the cutover collapsed the dedicated tier

The Rust daemon shares one multiplexed Dragonfly connection for ordinary commands.
A lease request checks readiness and reads available work without `BLOCK`.
An empty response tells the runner when to poll again; the runner holds no Dragonfly connection.

```text
agentsfleetd replica
  HTTP handlers and background writers
    +-- shared ConnectionManager --> XADD / HRANDFIELD / XREADGROUP / PUBLISH / XACK
  SubscriptionHub async pump
    +-- dedicated pub/sub socket --> SUBSCRIBE per watched fleet
         +-- bounded broadcast --> per-fleet or workspace SSE response bodies
  connector:outbound worker
    +-- dedicated command socket --> XREADGROUP BLOCK (up to 5 seconds)
```

Cloning `afd_dragonfly::Dragonfly` shares its socket; it does not open another connection.
The outbound reader owns `afd_dragonfly::Dedicated`, so its blocking read cannot delay request-path commands.
Normal boot opens three Dragonfly connections when both optional background surfaces start.

The hub refcounts subscribers and keeps one wire subscription per watched channel.
Two tasks serve its one connection: a control task owns commands, re-subscribes and redials, and a dispatch task owns the pushes, so a slow subscribe never holds a frame.
Recovery from a lost subscription: [datastore_scaling.md](./datastore_scaling.md) §"Target design".
Each frame is one shared allocation, however many viewers read it.

Source: [`afd_dragonfly::Dragonfly`](../../rustd/crates/afd_dragonfly/src/client.rs),
[`hub control task`](../../rustd/crates/afd_dragonfly/src/hub/pump.rs),
[`hub dispatch task`](../../rustd/crates/afd_dragonfly/src/hub/dispatch.rs),
[`node repair`](../../rustd/crates/afd_dragonfly/src/hub/repair.rs),
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
by name (`rustd/crates/afd_db/src/error.rs` — `is_pool_capacity` against
`is_datastore_unavailable`). An acquire that answered only an absent connection
would erase the distinction at the handler boundary and put both behind one
alert, and the handler that needed to tell them apart would acquire from the
pool directly — reimplementing the acquire/release pairing the pooled-connection
guard makes unskippable. The library reads record the difference as
`agentsfleet_library_pool_result_total{pool_result="timeout"|"error"}`.

**What the pool guarantees, and what it does not.** Releasing an occupied slot
lets at least one queued waiter progress, and every waiter either acquires or
receives the configured timeout — no waiter blocks forever. There is **no
ordering or fairness guarantee**: which waiter wins is scheduling. `afd_db`'s
live-pool integration suite proves the two real guarantees against a live
size-1 pool and deliberately declines to assert the third.

## Config reload — pull-per-lease, no signal

Canonical: [`runner_fleet.md` §Config](./runner_fleet.md) — config resolves fresh from `core.fleets` on every `lease`; a `PATCH` takes effect on the next lease with no cache and no signal. What is specific to this flow: status works the same way — the assignment scan filters `core.fleets.status = 'active'`, so a paused Fleet drops out on the next scan and a resumed one re-enters.

## End-to-end sequence

### A. INSTALL  (`agentsfleet install --library <id>` from an onboarded library entry)

Onboarding (fetch, validate, re-pack, R2 + Postgres): [fleet_bundles.md](./fleet_bundles.md) §"Onboard: fetch, validate, re-pack (agentsfleet builds its own tar)".

```
   user / agentsfleet CLI
    │  POST /v1/workspaces/{ws}/fleets  (one library entry; API reference › Fleets)
    ▼
  agentsfleetd-api (create handler)
    │
    ├─► load normalized SKILL.md/TRIGGER.md + immutable snapshot metadata
    │    from the selected library tier (platform or tenant)
    ├─► if trigger_markdown is absent:
    │      generate default manual/API trigger config
    ├─► check required workspace secrets by key name only; never resolve
    │      raw secret values during install
    ├─► [Postgres] INSERT core.fleets          (tenant taken from the workspace row,
    │                                          never from the caller)
    ├─► [Postgres] record nullable bundle snapshot metadata on the Fleet
    ├─► [Dragonfly] XGROUP CREATE MKSTREAM fleet:{id}:events fleet_lease 0
    │               (Fleets::ensure_stream — the lease XREADGROUP needs this group)
    └─► 201 to user  (invariant: data stream + group exist before 201)

   No worker thread to spawn. The Fleet is installable work the moment its
   events group exists; the first runner to lease it will claim it.

   At rest:
     Postgres: core.fleets row (installing → active); an approved
            core.integration_grants row per declared mintable credential.
            No core.fleet_sessions row: the first report's checkpoint writes it.
            No core.fleet_events. No billing.usage_ledger. No fleet.runner_leases.
     Dragonfly: stream fleet:{id}:events with group fleet_lease (empty).
                Channel fleet:{id}:activity does not yet exist (implicit on first PUBLISH).
```

### B. TRIGGER  (steer / webhook / cron — three callers, ONE ingress)

Workspace connection proofs: [connectors.md](./connectors.md) §"GitHub App: platform setup to fleet execution".

```
   Common envelope (every XADD on fleet:{id}:events carries these
   five fields. The canonical event_id is the ADMISSION ROW's
   `<created_at>-<seq>`, minted in PostgreSQL before the append and
   keeping the `<millis>-<n>` shape every reader was written against.
   The stream entry id is the physical RECEIPT of that append — it is
   what `acknowledge` takes, and it is not an identity: a replayed
   admission earns a second receipt for one event_id):

       actor         steer:<user> | webhook:<source> | github-app | cron:<schedule>
                     | continuation:<original_actor> | slack:<user>
                     | system:repair-verifier
       type          chat | webhook | cron | continuation
       workspace_id  <uuid>
       request       <opaque JSON — the message + metadata>
       created_at    <epoch milliseconds; project bigint convention>
       event_id      <created_at>-<seq>, the admission row's logical id; the
                     append adds it as a sixth field

   STEER     agentsfleet steer <fleet_id> "morning health check"
               → POST /v1/.../fleets/{id}/messages
               → XADD fleet:{id}:events *
                      actor=steer:kishore  type=chat
                      workspace_id=<ws>    request=<msg>
                      created_at=<ms>
               → 202 { event_id }                ← CLI uses event_id
                                                   to filter SSE frames

   GITHUB    App posts pull_request or workflow_run
   APP         → POST /v1/ingress/github
                 verify platform github-app.webhook_secret BEFORE payload read
                 installation.id → core.connector_installs → workspace
                 repository.full_name + event + approved grant
                    → active fleet subscriptions
                 authenticated-body-digest/fleet replay slot
               → XADD fleet:{id}:events * for each exact match
                      actor=github-app      type=webhook
                      workspace_id=<ws>     request=<normalized-json>
                      created_at=<ms>
               → 202

               `repositories` is required and fail-closed, with replay per fleet
               ([connectors.md](./connectors.md) §"Repository and event subscriptions belong to fleets").

   MANUAL     Custom providers and the old GitHub workflow_run path retain
   WEBHOOK      POST /v1/webhooks/{fleet_id}
                 POST /v1/webhooks/{fleet_id}/github
               with a workspace `<source>.webhook_secret`. The fleet identifier
               is already in the URL, so this route does not require
               `repositories` and does not use `core.connector_installs`.

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
               → atomically suppresses replay + XADD fleet:{id}:events *
                      actor=cron:<schedule_id>  type=cron
                      workspace_id=<ws>        request=<schedule-event-json>
                      created_at=<ms>

   CONTINUATION  agentsfleetd re-enqueue (chunk-continuation or
                 user-resumed fulfillment)
               → XADD fleet:{id}:events *
                      actor=continuation:<original_actor>
                      type=continuation
                      workspace_id=<ws>  request=<continuation-msg>
                      created_at=<ms>
                 The new event's row carries
                 resumes_event_id=<immediate_parent_event_id>.
                 Continuation actor is FLAT — never re-nests
                 `continuation:` (a steer that chunks 3 times produces
                 `actor=continuation:steer:kishore` on every continuation,
                 not `continuation:continuation:continuation:...`).

   All six producers land the same envelope on the same stream. The
   reasoning loop never branches on actor. Actor is metadata for the
   SKILL.md prose and the user's history filter.

   > [!NOTE]
   > SLACK — producer `slack_mention` (M206_002,
   > afd_api_ingress/src/handler/mention.rs). One more producer into THIS same
   > ingress — the lease/execute path does not change. Routing lives in
   > scenarios/slack-incident-responder.md §4; memory continuity in
   > [`runner_fleet.md`](./runner_fleet.md) §"Memory continuity". Spec:
   > docs/v2/done/M106_001_P1_API_DOCS_INFRA_UI_SLACK_RESIDENT_CHANNEL_BOT.md

   > [!NOTE]
   > REPAIR VERIFICATION (M157): a sixth producer. After a human merges a
   > repair and GitHub reports production status, a bounded dispatcher matches
   > the workspace, repository, and commit, then lands one
   > actor=system:repair-verifier event on the verifier fleet's stream. Same
   > envelope, same single ingress; the lease/execute path does not change.
   > The responder → repairer → verifier walkthrough, with <img src="https://cdn.simpleicons.org/grafana" width="14" alt="" /> Grafana and
   > <img src="https://cdn.simpleicons.org/elasticsearch" width="14" alt="" /> Elasticsearch as the evidence sources, lives in
   > [`scenarios/production-deploy-repair.md`](./scenarios/production-deploy-repair.md).
```

#### QStash owns the clock

`agentsfleetd` stores the desired schedule, pushes each requested mutation to
QStash synchronously, and receives the fires. The runner owns no schedule
timer ([`runner_execution.md`](./runner_execution.md#tool-catalog)).

A schedule has one of three authors, recorded as its `source`: `trigger` for
the fleet's `TRIGGER.md`, `api` for a person, and `fleet` for the fleet itself,
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
schedule's runs are its history rows under that actor, read through slot 930's
`(fleet_id, actor, created_at, event_id)` index, so a page never walks the
fleet's whole history. A `once` schedule (slot 928) retires inside that one
fire seam: the fire is admitted, then the schedule is claimed `deleting` and
removed from QStash. A `once` fire is keyed by the schedule alone, so a run-now
racing its QStash fire replays it rather than admitting a second run. A QStash
fire dropped because the fleet takes no work also retires a `once` schedule,
since its moment has passed; a retirement whose claim is held answers
`UZ-SCHED-006`, so QStash repeats the fire. A `once` row keeps the instant it
was set for (slot 931, `fire_at`), and a sync after that instant removes one
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

The deleted worker's single in-process `processEvent` loop is now split across two protocol calls. `lease` does the pre-execution control-plane work and hands a self-contained `ExecutionPolicy` to the runner; `report` does the terminal control-plane work after the runner's turn finishes.

```
   agentsfleet-runner (host)
    │  POST /v1/runners/me/leases   (non-blocking poll; Bearer agt_r)
    ▼
   agentsfleetd — lease handler:

     A runner whose isolation verdict is degraded or unreadable is
     answered no-work before any peek.

     Leases::select():
       peek one fleet:ready:{p} partition (sixteen, rotated per poll) for
       at most 64 fleets with work; an empty peek answers at once and
       reads no Postgres. Keep the active fleets whose required_tags are a
       subset of the runner's labels and whose slot no live runner holds;
       order them sticky by last_runner_id, ties at random;
       claim the per-fleet fleet.runner_affinity slot (wins iff free or
       its leased_until has passed) and bump the monotonic fencing_seq.
       A lease past lease_expires_at is RECLAIMED: its event envelope +
       billing are reused, re-fenced with a higher token. Otherwise take
       the group's oldest pending entry (XAUTOCLAIM, min-idle 0), else a
       new one (XREADGROUP >, COUNT 1). Both empty: release the slot, and
       clear the mark only if its token is the one this poll peeked.
       An entry missing any of its six fields (the five envelope fields
       and event_id) is dropped and the slot released.

     1. INSERT core.fleet_events                  ← narrative log opens
          (status='received', actor, request_json)
          ON CONFLICT (fleet_id, event_id) keeps the row (idempotent on replay)
        + stamp core.fleet_admissions.delivered_at on the same connection,
          on both arms (the reconcile pass reads an unstamped row as lost work)
        A redelivery whose row is already terminal is acknowledged and stops here.
     2. PUBLISH fleet:{id}:activity { kind:"event_received", event_id, actor }
     3. Gates + billing (mirror of afd_billing):
          balance gate → budget gate → receive debit → approval gate; the run
          is metered per /renew and settled at report.
          The BUDGET gate is the fleet's own ceiling, resolved from the config
          the session already carries; it sits after the tenant credit pool and
          before any debit, so a refused event is never charged. Both it and the
          balance gate fail OPEN on a datastore fault — a metering outage must
          not halt every fleet on the platform.
          The receive debit fires on FIRST DELIVERY only: the balance debit is
          not replay-guarded (only the telemetry row is), so a PEL re-delivery
          that already paid must not pay twice.
          Blocked → UPDATE core.fleet_events status='gate_blocked',
                                              failure_label=<gate>
                    → PUBLISH fleet:{id}:activity
                        { kind:"event_complete", status:"gate_blocked" }
                    → XACK fleet:{id}:events       ← row-terminal:
                      gate_blocked rows are NEVER reopened. When the gate
                      resolves, a fresh XADD lands with
                      actor=continuation:<original>, producing a NEW row.
          A wait on a person parks instead: no row is written, the slot is
          freed and the ready mark cleared.
     4. resolve secrets_map from the vault (Vault::declared; per-fleet tool
        secrets, workspace-scoped). The provider api_key is resolved separately
        (Providers::resolve, fresh + reclaim) and delivered on the lease via
        ExecutionPolicy.provider + ExecutionPolicy.api_key; it does NOT join
        secrets_map and is never substituted into a tool placeholder. The
        runner's supervisor uses it for the inference call only, and
        agentsfleetd keeps it live only through the synchronous lease write.
     5. issue fleet.runner_leases row              ← durable ownership
          (lease_id, fencing_token, lease_expires_at = now + LEASE_TTL_MS)
          fenced on the claim: if the slot's fencing_seq moved or its
          leased_until passed, nothing is written and the poll answers no-work.
     → 200 { lease } (fields: API reference › Runner plane; instructions/bundle:
       [runner_fleet.md](./runner_fleet.md) §"Running one event")

       Plaintext lifetime:
       [billing_and_provider_keys.md](./billing_and_provider_keys.md) §"8.2 The api_key visibility boundary".

   agentsfleet-runner — supervisor:
       The supervisor runs the lease
       ([runner_execution.md](./runner_execution.md) §"One lease, end to end");
       a lease it cannot admit or sandbox reports startup_posture.

          on tool_call_started    → A frame → POST .../activity
          on fleet_response_chunk → A frame → POST .../activity
          on tool_call_progress   → A frame → POST .../activity
                                   (long-tool heartbeat; absence past ~5s
                                    renders as "stuck" in the UI)
          on tool_call_completed  → A frame → POST .../activity
          │
          └─ terminal: the loop's answer, tokens, telemetry and outcome

   agentsfleet-runner — supervisor:
       build the report and classify any failure (timeout, OOM, crash,
       startup_posture); hold a processed lease's sandbox frozen for the
       fleet's next lease or destroy it, then:
    │  POST /v1/runners/me/reports { lease_id, fencing_token, outcome, ... }
    ▼
   agentsfleetd — report handler:

     Leases::claim_and_settle (afd_fleet/src/lease/settle.rs): atomic CAS —
       UPDATE fleet.runner_leases SET status=reported
       FROM fleet.runner_affinity
       WHERE status='active' AND fencing_token >= fencing_seq
       RETURNING <lease fields>
       (fence + flip + dedup in one statement; a stale/reclaimed holder is
        rejected with UZ-RUN-005 and mutates nothing)

     7. UPDATE core.fleet_events                  ← narrative log closes
          SET status = outcome==ok ? 'processed' : 'fleet_error',
              response_text, completed_at = now()
     8. PUBLISH fleet:{id}:activity { kind:"event_complete", event_id, status }
     9. INSERT/reconcile billing.usage_ledger ← billing/latency,
          (event_id UNIQUE, token_count, ttft_ms, wall_seconds, ...)
    10. UPSERT core.fleet_sessions                ← the bookmark
          SET context_json = { last_event_id, last_response },
              checkpoint_at = now()
    11. XACK fleet:{id}:events                    ← consumer cursor advances
    12. release affinity (WHERE fencing_seq = $token)  ← token-guarded

   Runner dies mid-event → its lease expires at lease_expires_at; the next
   lease's reclaim path re-issues the event to another runner with a higher
   fencing_token. Step 1's ON CONFLICT and the UNIQUE telemetry event_id keep
   the replay safe — exactly one fleet_events row, exactly one telemetry row,
   regardless of how many redelivery attempts occur. A late report from the
   dead runner is fenced out at claim_and_settle (UZ-RUN-005).
```

**Answer round-trip to a connector thread.** A reply destination recorded at admission is the only thing a report owes a delivery to; the outbound worker posts it ([scenarios/slack-incident-responder.md](./scenarios/slack-incident-responder.md) §"6. How the answer comes back"; limits: API reference › Runner plane).

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
                 UNSUBSCRIBE for that channel.

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
               → reads core.fleet_events (cursor-paginated).

   RESUME    no user-facing command reads core.fleet_sessions. The lease
             path reads it with the fleet row and hands it to no lease.
             "Busy or idle" is fleet.runner_leases, which has the fencing
             token and the expiry that make the answer trustworthy.

   Both browser registries backfill durable event rows after reconnect,
   and on any catching_up frame, a lag's or a lost subscription's.
   A publish missed while the HTTP stream stayed open is therefore
   recovered by that backfill rather than lost silently.
   Transient token chunks are not durable history; settled rows are.
   Live tail is best-effort; core.fleet_events remains the durable record.
```

## Multi-tenancy boundary

| Layer | Tenant isolation mechanism |
|---|---|
| PG (`core.fleets`, `core.fleet_events`, etc.) | Enforced in the application, not by the database. `afd_tenant`'s `AUTHORIZE_WORKSPACE` resolves the caller's tenant and the workspace's owner in one statement, and `afd_api` mounts it as a layer in front of every route whose path carries a workspace — so no handler is in a position to forget it. Every control-plane statement filters by `workspace_id` or `fleet_id` explicitly. **No `ROW LEVEL SECURITY` policy is declared anywhere in `schema/`, and no `current_setting('app.workspace_id')` is read.** Re-adding database policies would need a transaction per request, because `sqlx` returns a connection to the pool between requests and a session-level setting would leak one tenant's identifier onto the next. |
| Dragonfly data plane (`fleet:{id}:events`) | Key namespaced by fleet UUID (globally unique); no cross-tenant collision possible. Dragonfly has no per-row authorization of any kind, so the protection is the same shape as the row above it: the key is unguessable and every path to it goes through the ownership check. |
| Runner ↔ control plane | The `agt_r` token authenticates the runner per call, and a lease carries one fleet's event and scoped secrets ([`runner_fleet.md`](./runner_fleet.md) §"Registering a runner"). |
| Runner supervisor and sandbox | Secrets stay in the supervisor, substituted at send time, and a sandbox holds no credential ([`runner_execution.md`](./runner_execution.md) §"Credentials"). |

## One active lease per fleet — the ownership model

Before the cutover, a single worker thread owned all events for a Fleet, and the concern was round-robin across worker replicas breaking per-fleet continuity. That model is gone. Ownership is now a **durable lease**, not a thread:

- `fleet.runner_affinity` holds one slot per fleet. `Leases::select` (`rustd/crates/afd_fleet/src/lease/assign.rs`) claims it atomically — a runner wins iff the slot is free or the prior lease has expired — and bumps a monotonic `fencing_seq`. So **at most one lease is active per fleet at any time**, regardless of how many runners poll concurrently. The claim is the only writer of `fencing_seq`, so every fencing token a lease carries is a value a claim returned.
- A runner that loses the race for a Fleet simply gets no lease for it and tries the next eligible fleet (or backs off).
- Continuity across runs lives in `agentsfleetd`, never in runner-local state, so any runner can pick up the next run. Today only fleet memory reaches the next lease; the session checkpoint is saved to `core.fleet_sessions` and handed to no lease ([`runner_execution.md`](./runner_execution.md) §"Workspace between leases"). Sticky routing (prefer `last_runner_id`, random among the rest, `rustd/crates/afd_fleet/src/lease/sql/lease.rs`) is a hint for warm-sandbox reuse, never ownership.

Failure mode: a dead lease holder blocks its fleet until `lease_expires_at`; reclaim then re-leases with a higher fencing token. Recovery latency = TTL plus poll density (the S0 lazy-reclaim SLA). Tightening it is M80_006.

One PR through one lease: its records and timers in [lease_flow.md §1](./lease_flow.md#1-from-start-a-fleet-to-a-lease), its reply limits in [§4](./lease_flow.md#4-how-agent-bob-01-can-reply), and what bounds the host in [§7](./lease_flow.md#7-where-the-image-lives-and-what-bounds-a-bare-metal-host).

## What the coding fleet never does

- Never sees the fleet's LLM tokens or reasoning state
- Never holds the fleet's secrets in its own context
- Never executes the fleet's tool calls in its own session
- Never persists across the user's laptop being closed

## What the fleet (host) never does

- Never touches the user's laptop directly
- Never reads the user's local filesystem (it sees only what the SKILL.md and TRIGGER.md grant it)
- Never escapes the sandbox — namespaces, Landlock, seccomp and cgroup v2 bound each lease's sandbox, and the agent loop runs outside it ([`runner_execution.md`](./runner_execution.md#sandbox-engines)). **Network egress** follows the runner's network policy (see [`runner_fleet.md` §Egress model](./runner_fleet.md)).
- Never holds a datastore credential — the runner reaches the platform only over the `/v1/runners` protocol

## The install failure scenario, visually

The API server (not a runner) is the side that writes to Dragonfly during install. So a Dragonfly blip during install hits the API → Dragonfly hop. The API has two layers of defence:

1. **Inline retry (API).** `ensure_stream` retries `XGROUP CREATE MKSTREAM fleet:{id}:events` with jittered exponential backoff: 200 ms doubling to a 1500 ms cap, four attempts, about 1.4 s of sleeps before jitter (`rustd/crates/afd_fleet_lifecycle/src/install.rs`). Most blips never escape this loop. (The group is load-bearing — the `lease` `XREADGROUP` needs it.)
2. **PG rollback (API).** If retries exhaust, the handler `DELETE`s the freshly-inserted `core.fleets` row and returns 500 `UZ-AGT-013` so the caller can retry cleanly. No orphan.

**The pre-cutover third layer (watcher reconcile sweep) is gone** with the worker. A rare double fault (group setup exhausts retries AND rollback fails) now leaves an orphaned `core.fleets` row, logged `hint=row_orphaned_manual_recovery` (`rustd/crates/afd_fleet_lifecycle/src/install/rollback.rs`), healed by an operator or a future reconcile job. The orphan is inert: no runner can lease it (no events group), no live tail.

```
   TIME ──►
   t=0  USER → agentsfleet install → API
   t=2  API: INSERT core.fleets (status='installing') → PG ✓
   t=3  API: XGROUP CREATE MKSTREAM ╳ (4 attempts exhausted, ~1.4 s)
   t=4  API: DELETE core.fleets row ╳ (rare second failure)
   t=5  API: 500 → user. Logs: install_stream_retry (each retry),
                              install_rollback_failed

   ── ORPHAN WINDOW (until operator / future reconcile job) ──
      PG row Z = active; Dragonfly stream + group missing. Other fleets
      unaffected. No runner can lease Z (its events group does not exist).
```

A future reconcile job (a control-plane sweep over `core.fleets` for `active` rows whose events group is missing, calling `ensure_stream`) is the planned replacement for the deleted watcher's healing role; it is out of scope here.

