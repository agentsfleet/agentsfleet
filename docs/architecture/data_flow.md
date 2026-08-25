# Data Flow — how an event moves through the system

> Parent: [`README.md`](./README.md) · Sibling: [`runner_fleet.md`](./runner_fleet.md) (the structural split this flow runs on).
>
> **Scope:** this file describes the runtime as it runs now — after the M80_002 cutover. `agentsfleetd` is the **control plane** (owns Postgres, Redis, the Vault, the HTTP API, and work assignment); the host-resident **`agentsfleet-runner`** daemon is the **execution plane** (leases work over Hypertext Transfer Protocol Secure (HTTPS), runs NullClaw in a forked sandboxed child, reports back). The single-process `agentsfleetd worker` loop and the standalone sandbox sidecar are deleted. See [`runner_fleet.md`](./runner_fleet.md) for the why and the guarantees.

Read this when you need to know where a webhook, a steer, or a cron fire ends up. Many specs reference this file as the canonical picture of the runtime.

## Facts

Every row is extracted from the sections below; the owner column names the section that carries the full story.

| Invariant | Value | Mechanism | Owner section |
|---|---|---|---|
| Event ingress | ONE — six producers | steer / webhook / cron / continuation / Slack / repair-verifier all `XADD fleet:{id}:events`; the stream entry id IS the canonical event id | §B. TRIGGER |
| Hot-path writes | 12, in the worker's order | `lease` does 1–6, `report` does 7–12; row-equivalent to the deleted worker (cutover Invariant 2) | §Steer flow end-to-end |
| Durable stores | 3 tables, join key `event_id` | `fleet_sessions` (one row per fleet, UPSERT) · `fleet_events` (one row per delivery) · `billing.usage_ledger` (two rows per event, UNIQUE `(event_id, charge_type)`) | §The three durable stores |
| Replay safety | idempotent | `INSERT … ON CONFLICT DO NOTHING` + the UNIQUE telemetry `event_id` | §C. EXECUTE |
| Stale-writer rejection | `UZ-RUN-005` | `claimReport()` fences, flips, and dedups in one atomic statement | §C. EXECUTE |
| Redis pool | `max_idle=8, eager_min=2` | short-lived commands only: `XADD`, non-blocking `XREADGROUP`, `PUBLISH`, `XACK` | §Connection topology |
| Dedicated Redis connections | one — the SubscriptionHub | refcounted `SUBSCRIBE`; N viewers cost one connection per replica | §Connection topology |
| Postgres acquire failures | 2 distinct errors | `PoolTimeout` (capacity) vs `PoolUnavailable` (datastore); `MAX_CONNECTIONS_PER_READ` = 1 | §The Postgres pool |
| Config freshness | read per lease | a `PATCH` takes effect on the next lease; no cache, no signal | §Config reload |
| Gate-blocked rows | terminal | never reopened; the resolved gate lands a NEW row via `actor=continuation:<original>` | §"C. EXECUTE" step 3 |
| Webhook rejections | 3 codes | `UZ-WH-020` (misconfig) · `UZ-WH-010` (bad signature) · `UZ-WH-011` (stale timestamp, 5-minute window) | §B. TRIGGER |
| Install guarantee | stream + group before 201 | `ensureEventStream` retries `[100ms, 500ms, 1500ms]`; exhaustion rolls back the PG row | §A. INSTALL |
| SSE sequence ids | not durable | per-connection counter, resets to 0; `Last-Event-ID` ignored; backfill via the events list | §D. WATCH |
| Client gap recovery | reconnect-only fetch (M122) | bounded `fleet_events` list `since` last delivery − 2 s overlap, merged by event id | §Two streams + one pub/sub channel |
| Cron authority | QStash | signature verified at ingress; replay suppressed atomically; the runner owns no timer | §B. TRIGGER |
| Cancel latency | ≤ one heartbeat interval | revocation rides the heartbeat reply | §KILL |
| Lease ownership | at most one active lease per fleet | atomic `runner_affinity` claim + monotonic `fencing_seq` | §One active lease per fleet |
| Provider `api_key` | never in `secrets_map` | rides `ExecutionPolicy.provider` + `.api_key`; injected for the inference call only | §"C. EXECUTE" step 4 |
| Tenant isolation | RLS + namespacing | Postgres Row-Level Security by `workspace_id`; Redis keys namespaced by unguessable fleet UUID | §Multi-tenancy boundary |

## Traps

Each trap is enforced in its owner section; this list is the index.

- The live tail is the eyeballs surface, not the audit surface; durable history is `core.fleet_events` (§Two streams + one pub/sub channel).
- A connection held across a blocking `SUBSCRIBE` can never return to a pool (§Connection topology).
- Never acquire a second Postgres connection while holding one — that is how a pool deadlocks (§The Postgres pool).
- The Postgres pool has no ordering or fairness guarantee; do not assert one (§The Postgres pool).
- `gate_blocked` rows are NEVER reopened (§"C. EXECUTE" step 3).
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
- the Redis connection topology — §Connection topology
- install, trigger envelope, execute, watch, kill — §"End-to-end sequence" — A through D, plus KILL
- the install failure window — §The install failure scenario, visually

## Decisions

| Decision | Reason | Where / artifact |
|---|---|---|
| Two per-delivery tables (`events` + `telemetry`) | different write authorities and retention rules | §The three durable stores |
| `fleet:control` removed | no per-fleet threads left to orchestrate | §Two streams + one pub/sub channel |
| Dedicated Redis tier collapsed | idle cost now tracks lease-poll frequency, not fleet count | §Connection topology; M80_002 |
| `Hx.db()` returns a named error set, not `?DbScope` | `PoolTimeout` and `PoolUnavailable` are different operator pages | §The Postgres pool |
| Gap recovery is client-side, not server resume | no channel or frame-shape change; the durable table is the recovery source | §Two streams; M122 |
| QStash owns the clock | the runner and its disposable child own no schedule timer | §B. TRIGGER |
| Upload-bundle picker path deferred | Indy-acked 2026-06-20 | §A. INSTALL |
| Watcher reconcile sweep deleted; orphan stays inert | no runner can lease a fleet with no events group; a future reconcile job heals it | §The install failure scenario |
| SSE auth is dual-accept with strict no-fallthrough | a stale cookie must not silently fall through to a valid Bearer | §D. WATCH |
| Outbound answers ride a generic `connector:outbound` stream | the report path stays provider-agnostic (Invariant 9) | §C. EXECUTE; M106 |

---

## Detail

Everything below is the full reference. One event is told three times, at three
zoom levels, and they do not repeat each other — read the one that matches your
question. §"Steer flow end-to-end" draws the path as boxes, so you can see where
a call goes. §"End-to-end sequence" (A INSTALL → D WATCH, plus KILL) states what
each step must guarantee. §"Concrete platform-ops example" shows the actual row
contents at every step, for when you need to know what the data looks like.

Headings are stable — specs cite them by text; insert new sections, never rename existing ones.

## Process and stream ownership at a glance

| Process | Role |
|---|---|
| **`agentsfleetd-api`** (`agentsfleetd serve`) | The control plane. HTTP routes for the user surface **and** the `/v1/runners` machine surface. Owns Postgres, the Redis pool, and the Vault. Steer, webhook, cron, and continuation handlers all `XADD` directly to `fleet:{id}:events` — single ingress. On `lease` it does a non-blocking `XREADGROUP` to claim the next event, runs the gates + billing + secret resolution, and issues a `fleet.runner_leases` row; on `report` it persists the terminal state and `XACK`s. It is the sole `PUBLISH`er on `fleet:{id}:activity`. Never runs language-model code. |
| **`agentsfleet-runner`** (host-resident daemon) | The execution plane. Boots from an operator-installed `agt_r` token (env `AGENTSFLEET_RUNNER_TOKEN`, no self-register — Option B), then loops `heartbeat → lease → execute → report → activity` over HTTPS carrying that `agt_r` token. Holds **zero datastore credentials**. Per lease it forks a sandboxed child (Landlock + cgroups + network namespace via bwrap) that runs the NullClaw fleet; credential substitution happens at the tool bridge inside that child. Frames stream back to the parent over a stdout pipe and are forwarded to `agentsfleetd` over the `activity` verb. |

| Target | Producer | Consumer |
|---|---|---|
| `fleet:{id}:events` | `agentsfleetd-api` on steer / webhook / signed QStash fire / continuation | **`agentsfleetd`** — non-blocking `XREADGROUP` on each `lease`, `XACK` on each `report` |
| `fleet:{id}:activity` | `agentsfleetd` (sole publisher) — bracket frames directly, mid-run frames fed by the runner's `activity` stream | SSE streams in `agentsfleetd-api`, fanned out from the SubscriptionHub's one shared pub/sub connection (refcounted SUBSCRIBE per channel) |
| `core.fleet_events` | `agentsfleetd` lease path (INSERT received) → report path (UPDATE terminal) | `agentsfleetd-api` `GET /events` endpoints, dashboard, `agentsfleet events` |
| `core.fleets` | `agentsfleetd-api` only | Canonical Fleet runtime table; `agentsfleetd` reads it at lease so config resolves fresh per lease |
| `core.fleet_sessions` | `agentsfleetd` lease path (mark busy) + report path (checkpoint) | `agentsfleetd` at lease + `agentsfleet status` |
| `fleet.runner_leases` / `fleet.runner_affinity` | `agentsfleetd` lease path (issue) + report/reclaim (flip / release) | `agentsfleetd` assignment + fencing + reclaim |
| `vault.secrets` | `agentsfleetd-api` on `secret create` (upsert) | `agentsfleetd` resolves just-in-time at `lease`, ships inline in the lease reply |
| `fleet:control` | — (removed at the cutover) | — (removed at the cutover) |

---

## The coding fleet and the Fleet runtime

Two distinct things are in play. Keeping them straight is essential to understanding the architecture:

```
┌──────────────────────────────────┐         ┌───────────────────────────────────┐
│  CODING AGENT (laptop)           │         │  FLEET RUNTIME (host)             │
│                                  │         │                                   │
│  Claude Code / Amp / Codex /     │         │  NullClaw running inside the      │
│  OpenCode driving agentsfleet    │         │  agentsfleet-runner's sandboxed   │
│                                  │         │  child (Landlock + cgroups +      │
│  This is what the human types    │         │  netns via bwrap; durable,        │
│  into. Ephemeral.                │         │  persists across laptop close)    │
└──────────────────────────────────┘         └───────────────────────────────────┘
```

The coding fleet is a workstation tool driving `agentsfleet`. The Fleet runtime — the product object the user creates — runs a NullClaw fleet loop inside the runner.s sandboxed child. The coding fleet never becomes that runtime and never sees its tokens — they communicate only through the steer endpoint, the event stream, and the events history.

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
           ║  XADD fleet:{id}:events *              ║   ← single ingress.
           ║       actor=steer:<user>               ║     Webhook + cron use
           ║       type=chat                        ║     the same XADD.
           ║       workspace_id=<uuid>              ║
           ║       request=<msg-json>               ║
           ║       created_at=<epoch_ms>            ║
           ║  → 202 { event_id }                    ║
           ╚════════════════════════════════════════╝
                          ↓
        ( the event waits on the stream until a runner asks for work )
                          ↓
           ╔════════════════════════════════════════╗
           ║  agentsfleet-runner (host)             ║
           ║  POST /v1/runners/me/leases            ║   ← long-poll; no work
           ║  Authorization: Bearer agt_r           ║     → null + retry_after_ms
           ╚════════════════════════════════════════╝
                          ↓
           ╔════════════════════════════════════════╗
           ║  agentsfleetd (lease handler)          ║   ← the work the worker
           ║  ────────────────────────────────────  ║     used to do, now on
           ║  assign.select():                      ║     the request thread:
           ║   non-blocking XREADGROUP across       ║
           ║   active Fleets (sticky pref) →        ║   ← narrative log opens
           ║   claim fleet.runner_affinity,         ║     (mutable)
           ║   issue monotonic fencing_token        ║
           ║  1. INSERT core.fleet_events           ║   ← live: pub/sub frame
           ║     (status='received')                ║     (ephemeral, no ACK)
           ║  2. PUBLISH fleet:{id}:activity        ║
           ║     {kind:"event_received"}            ║   See
           ║  3. balance gate, receive debit,       ║   [`capabilities.md`](./capabilities.md)
           ║     approval gate, run debit           ║   for each gate layer.
           ║  4. resolve secrets_map from vault     ║
           ║  5. UPSERT core.fleet_sessions         ║   ← resume cursor:
           ║     SET execution_id (busy)            ║     marks Fleet busy
           ║  6. issue fleet.runner_leases row      ║
           ║     (lease_expires_at, fencing)        ║
           ║  → 200 { event, ExecutionPolicy,       ║
           ║         secrets_map, instructions,     ║
           ║         lease_id, fencing_token }      ║
           ╚════════════════════════════════════════╝
                          ↓
           ╔════════════════════════════════════════╗
           ║  agentsfleet-runner (parent + child)   ║
           ║  ────────────────────────────────────  ║
           ║  parent: establish cgroup, fork,       ║       This is the
           ║  exec self as `__execute` under        ║       Fleet runtime.
           ║  bwrap, feed the lease via stdin       ║       An LLM in a
           ║                                        ║       sandbox; the coding
           ║  sandboxed child:                      ║       fleet never becomes
           ║   apply mandatory Landlock,            ║       it, never sees its
           ║   run NullClaw over the policy.        ║       tokens or context.
           ║   Each tool call → tool bridge         ║
           ║   substitutes ${secrets.NAME.x}        ║
           ║   inside the sandbox, then the         ║
           ║   HTTPS request fires.                 ║
           ║                                        ║
           ║   Each progress frame → stdout pipe    ║   ← parent forwards
           ║   (A=activity, R=result, framed):      ║     each A frame to
           ║     - tool_call_started                ║     agentsfleetd .../activity,
           ║     - fleet_response_chunk             ║     which PUBLISHes it.
           ║     - tool_call_completed              ║
           ║                                        ║
           ║   Child returns ExecutionResult.       ║
           ║  → {content, tokens, ttft_ms,          ║
           ║     wall_ms, outcome}                  ║
           ╚════════════════════════════════════════╝
                          ↓
           ╔════════════════════════════════════════╗
           ║  agentsfleetd (report handler)         ║
           ║  POST /v1/runners/me/reports           ║
           ║  ────────────────────────────────────  ║
           ║   claimReport(): atomic CAS —          ║   ← fence + flip + dedup
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
           ║      context_json, execution_id=NULL   ║     clears handle,
           ║  11. XACK fleet:{id}:events            ║     advances bookmark
           ║  12. release affinity (token-guard)    ║
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

The 12 numbered writes are the deleted worker's `processEvent` effects, in the same order, split across two calls: `lease` does 1–6, `report` does 7–12. The handlers under `src/agentsfleetd/fleet/` mirror the old `event_loop_writepath`. Row equivalence (cutover Invariant 2) keeps history, billing, and the SSE tail byte-identical.

## The three durable stores: who owns what

The flow writes three Postgres tables. Each answers a distinct user question and has its own cardinality, mutability, and retention rule. The cutover moved the writer from the per-Fleet worker thread to the lease/report path; shapes and write order did not change.

| Table | Cardinality | Mutability | Answers |
|---|---|---|---|
| `core.fleet_sessions` | **One row per Fleet** | UPSERT — mutated on every event boundary | "Where is this Fleet *right now*? Is it idle or executing? What was its last successful response?" — the resume bookmark + active-execution handle. `execution_id` is set at `lease` (busy) and cleared at `report` (idle). Read at `lease` and by `agentsfleet status`. |
| `core.fleet_events` | **One row per delivery** | INSERT (status=`received`) → UPDATE (status=`processed` \| `fleet_error` \| `gate_blocked`) | "What did this Fleet do for event X? Who triggered it, what did they ask, what did it answer, did the gates pass?" — the user's narrative log. The single source of truth for the Events tab and `agentsfleet events`. |
| `billing.usage_ledger` | **Two rows per event** under the credit-pool model: one `charge_type='receive'` at the receive debit, one `charge_type='stage'` at the run debit (then UPDATEd with token counts after the report). UNIQUE `(event_id, charge_type)`. | INSERT at each debit, immutable for the `credit_deducted_nanos` column; the run row is reconciled once with actual token counts at report. | "How much did event X cost (split by receive vs run)? How fast was it? What posture was charged?" — billing + latency audit. Joinable to `fleet_events` via `event_id`. |

Why two per-delivery tables (`events` + `telemetry`) instead of one? They have different write authorities and retention rules:

- `fleet_events` holds user-readable strings (`request_json`, `response_text`) — large, mutable mid-lifecycle, deletable on tenant offboarding.
- `billing.usage_ledger` holds numeric audit columns — small, immutable once written, retained for billing reconciliation independent of whether the conversation row is purged.

The durable lease bookkeeping (`fleet.runner_leases`, `fleet.runner_affinity`) is a fourth concern — it is the *ownership* layer (which runner holds this event, at what fencing token, until when), not a user-facing record. It lives in the `fleet` schema and never carries user strings.

## Concrete platform-ops example

A GitHub Actions deploy fails on `agentsfleet/agentsfleet@c0a151bd`. The webhook lands as `event_id=1729874000000-0`, `actor=webhook:github`. Here is exactly what each row holds at each step.

**Before the event** — `fleet_sessions` shows the fleet idle since the previous event:

```
core.fleet_sessions  (one row, the fleet itself)
─────────────────────────────────────────────────
fleet_id            f4e3c2b1-...
context_json         {"last_event_id": "1729873200000-0",
                      "last_response":  "All apps healthy at 07:30Z."}
checkpoint_at        1729873208000
execution_id         NULL          ← idle
execution_started_at NULL
```

**Step 1 — INSERT `fleet_events`** (status=`received`, at `lease`):

```
core.fleet_events  (new row, narrative-log opens)
──────────────────────────────────────────────────
fleet_id      f4e3c2b1-...
event_id       1729874000000-0
workspace_id   8d2e1c9f-...
actor          webhook:github
event_type     webhook
status         received
request_json   {
  "message":  "GH Actions workflow_run failure on
               agentsfleet/agentsfleet deploy.yml run 9876",
  "metadata": {"run_id": 9876, "head_sha": "c0a151bd",
               "conclusion": "failure", "ref": "main",
               "repo": "agentsfleet/agentsfleet", "attempt": 1}
}
response_text  NULL
created_at     2026-04-25T08:00:00Z
completed_at   NULL
```

**Step 5 — UPSERT `fleet_sessions`** (mark busy, do *not* touch `fleet_events`):

```
core.fleet_sessions  (same row, mutated)
─────────────────────────────────────────
execution_id         exec-7af3c2b1-...   ← now busy
execution_started_at 1729874001000
(other fields unchanged from "before")
```

The lease reply ships to the runner. NullClaw runs inside the runner's sandboxed child: fetches GH run logs via `${secrets.github.token}`, fetches Fly app logs, fetches Upstash Redis stats, posts a remediation message to Slack. GitHub is a **mintable integration**, so that placeholder does not resolve to a stored value. At the tool bridge the child asks its runner, which forwards to the daemon-side credential broker over the `agt_r` plane (`POST /v1/runners/me/credentials/mint`). The broker signs a GitHub App JWT (RS256, platform key, daemon-side) and exchanges it for a short-lived installation token, returned just for that call. The App private key never leaves the daemon. (Fly/Upstash/Slack remain static custom secrets until the `oauth_refresh` integration lands.) The child returns `ExecutionResult{content, tokens=1840, wall_ms=8210, ttft_ms=320, outcome=ok}` over the stdout pipe; the runner POSTs it to `report`.

**Step 7 — UPDATE `fleet_events`** (close the same row, at `report`):

```
core.fleet_events  (same row, narrative-log closes)
────────────────────────────────────────────────────
status         processed
response_text  "Deploy failed: Fly.io OOM kill on machine i-01abc,
                app over 4GB resident. Last successful migration at
                c0a151bc. Posted to #platform-ops with rollback-to-
                c0a151bc remediation."
completed_at   2026-04-25T08:00:08Z
```

**Step 9 — INSERT `billing.usage_ledger`** (immutable audit row, joinable on `event_id`):

```
billing.usage_ledger  (run row reconciled with actuals)
─────────────────────────────────────────────────
id                       tel-1729874000000-0
fleet_id                f4e3c2b1-...
workspace_id             8d2e1c9f-...
event_id                 1729874000000-0   ← UNIQUE; joins to fleet_events
token_count              1840
time_to_first_token_ms   320
wall_seconds             8
epoch_wall_time_ms       1729874000000
plan_tier                free
credit_deducted_nanos    4
recorded_at              1729874008210
```

**Step 10 — UPSERT `fleet_sessions`** (advance bookmark, clear execution handle):

```
core.fleet_sessions  (same row, mutated)
─────────────────────────────────────────
context_json         {"last_event_id": "1729874000000-0",
                      "last_response":  "Deploy failed: Fly.io OOM kill..."}
checkpoint_at        1729874008210
execution_id         NULL          ← idle again
execution_started_at NULL
```

## Reading the three tables

- `agentsfleet status {id}` reads **`fleet_sessions`** — answers "is the fleet executing right now, and where did it leave off?"
- `agentsfleet events {id} [--actor=…]` reads **`core.fleet_events`** — answers "what has this fleet done, did any gate block it?" The **list** read stops there. It carries no request body and no agent answer.
- Expanding one row reads **the same table by event id** — answers "what was asked, what did it reply?" One event, on its own request.
- Billing rollups + p95 dashboards read **`billing.usage_ledger`** — answers "how many tokens this month, what's the latency tail?"

One table would force either a full scan to answer "is it busy now?" or mutable narrative columns beside immutable spend columns. Three tables, three jobs, one join key: `event_id`.

### The list read and the detail read are different reads

A page is up to two hundred rows, and the two body columns are unbounded — a trigger payload and a full agent answer. Selecting them per row bought a table that renders about a hundred and sixty characters per cell, so the list stops selecting them and the surfaces that used to quote an answer now state an outcome instead. A row that failed says why; a row that succeeded records that it did, without reproducing the reply. Postgres already keeps wide values in oversized-attribute storage, so the cost was never that the bodies existed — it was that the list **selected** them, which is why this is a read-path change and not a storage one.

Three surfaces changed with it: the events table's prose cell, the fleet header's outcome line, and the fleet thread's transcript. The transcript is the one surface that genuinely wants the bodies, because it renders what was said rather than a summary of it — so it re-reads its turns as details, server-side and in parallel. A turn whose detail read fails keeps its list row and renders its header and outcome rather than taking the page down.

The runner lease carries no second copy either. It used to hold its own `request_json`, a duplicate of the payload the event row already stored, written on every lease. Reclaim joins `core.fleet_events` on `(fleet_id, event_id)` to read the body instead; both tables cascade from the same parent, so the join cannot dangle.

**Partitioning is not done here, but its key is.** `billing.usage_ledger` carries the originating event's creation time rather than the write time. A renewal firing hours after its receive row would otherwise land in a different partition, miss the conflict target, and silently duplicate ledger rows. Carrying the column now means a later partitioning decision has a stable key already present and needs no backfill; the machinery itself waits for a measurement that demands it.

## Two streams + one pub/sub channel — and the one that retired

Two Redis surfaces carry a fleet's work: a durable stream for ingress, and an ephemeral pub/sub channel for the live tail. A third, `fleet:control`, was removed at the cutover and the last row records why.

| Redis surface | Type | Cardinality | Purpose | Volume |
|---|---|---|---|---|
| `fleet:{id}:events` | Stream + consumer group `fleet_lease` | One per fleet | Single event ingress — steer / webhook / cron / continuation all `XADD` here. `agentsfleetd` is now the consumer: a **non-blocking** `XREADGROUP` on each `lease`, `XACK`ed at `report`. Idempotent on replay via `INSERT … ON CONFLICT DO NOTHING`. | High — every event the fleet handles. |
| `fleet:{id}:activity` | Pub/sub channel (no consumer group, no persistence) | One per fleet | Best-effort live tail — `agentsfleetd` `PUBLISH`es one frame per `event_received` / `tool_call_started` / `fleet_response_chunk` / `tool_call_progress` / `tool_call_completed` / `event_complete`. The bracket frames originate in `agentsfleetd`; the mid-run frames are forwarded from the runner over the `activity` verb. The SubscriptionHub `SUBSCRIBE`s once per channel-with-viewers on its one shared connection and fans frames out by copy into each SSE stream's bounded queue. No buffer beyond those queues, no ACK, no resume. | High during execution, zero when idle. |
| `fleet:control` | (removed) | — | **Removed at the cutover.** It existed to tell the worker watcher to spawn / cancel / reconfigure per-fleet threads — and there are no per-fleet threads anymore. The producer (`control_stream.publish` from the install / status / config handlers) and the dead `control_stream` module were deleted; the install path keeps only `redis_agent.ensureFleetConsumerGroup` (load-bearing — the `lease` `XREADGROUP` needs the events group to exist). | gone |

`fleet:{id}:events` is durable (events appended, `XACK`ed entries pruned) and backs the at-least-once delivery guarantee. The pub/sub channel is ephemeral and exists only to power live user interfaces — its loss never affects correctness, only what the user sees in real time. Durable activity history lives in `core.fleet_events`; the pub/sub channel is the eyeballs surface, not the audit surface.

**Client-side gap recovery (M122).** Because the channel has no resume, a dashboard tab that drops its Server-Sent Events (SSE) connection misses every frame published during the reconnect window. The stream registry (`ui/packages/app/lib/streaming/fleet-stream-registry.ts`) closes that gap client-side. On every reconnect open — never the SSR-seeded initial connect — it fetches the bounded `core.fleet_events` list, keyed `since` the last server-delivered event minus a 2-second overlap, and merges by event id. The fetch goes through the same-origin token-minting proxy `/live/v1/workspaces/{ws}/fleets/{id}/events` (mirror of the SSE proxy; the `/live/*` prefix keeps these routes outside the `/backend/:path*` rewrite that shadowed them on Vercel). No server, channel, or frame-shape change — the durable table remains the recovery source of truth.

## Connection topology — the cutover collapsed the dedicated tier

Before the cutover, the worker held **one dedicated blocking Redis connection per fleet** (`XREADGROUP … BLOCK 5000`) plus a watcher connection — that dedicated tier was the binding fleet constraint. The cutover **deleted that tier**. `agentsfleetd` now claims work with a **non-blocking** `XREADGROUP` on the request thread that serves a `lease` call — a short-lived pooled command, not a held connection. The runner's "blocking" is an HTTP long-poll against `agentsfleetd`, not a Redis `BLOCK`, and the runner holds no Redis at all.

```
                        REDIS CONNECTION TOPOLOGY (post-cutover)
                        ════════════════════════════════════════

  ┌─────────────────────────────────────────────────────────────────────────────┐
  │                      POOL  (max_idle=8, eager_min=2)                        │
  │            ──── short-lived request-path commands only ────                 │
  │                                                                             │
  │   acquire → command → release   (microseconds to milliseconds)              │
  │                                                                             │
  └──▲──────────────────▲──────────────────▲──────────────────▲─────────────────┘
     │ XADD             │ XREADGROUP       │ PUBLISH          │ XACK
     │ fleet:{id}:      │ (no BLOCK)       │ fleet:{id}:      │ fleet:{id}:
     │ events           │ fleet:{id}:      │ activity         │ events
     │ (steer/webhook/  │ events           │ (brackets +      │ (on report)
     │   cron/continue) │ (on each lease)  │  forwarded)      │
  ┌──┴─────────────┐ ┌──┴─────────────┐ ┌──┴─────────────┐ ┌──┴─────────────┐
  │ HTTP user      │ │ lease          │ │ lease + report │ │ report         │
  │ handlers       │ │ handler        │ │ + activity     │ │ handler        │
  │ (agentsfleetd) │ │ (agentsfleetd) │ │ (agentsfleetd) │ │ (agentsfleetd) │
  └────────────────┘ └────────────────┘ └────────────────┘ └────────────────┘

  ┌──────────────────────────────────────────────────────────────────────────┐
  │   DEDICATED CONNECTION  (NOT in the pool) — one SubscriptionHub conn     │
  │                ──── long-lived blocking SUBSCRIBE ────                   │
  │                                                                          │
  │   SubscriptionHub reader thread                                          │
  │     SUBSCRIBE fleet:{a}:activity      one wire SUBSCRIBE per channel     │
  │     SUBSCRIBE fleet:{b}:activity  …   that has viewers, refcounted:      │
  │     → fan-out by copy into each SSE   first viewer subscribes,           │
  │       stream's bounded queue; never   last one unsubscribes.             │
  │       blocks on a slow viewer         N viewers cost one connection      │
  │       (drop-oldest + counter)         per replica, not one each.         │
  └──────────────────────────────────────────────────────────────────────────┘
```

**The rule that survives.** A connection held across a Redis call that blocks the server (`SUBSCRIBE`) cannot return to a pool — its lifetime is tied to the consumer, not the request. The pool is reserved for commands that complete in milliseconds: `XADD`, the non-blocking `XREADGROUP`, `PUBLISH`, `XACK`. The SubscriptionHub's reader is the only remaining dedicated-connection consumer; when its connection dies it redials with stop-checked pacing and replays SUBSCRIBE from the refcount map, while streams heartbeat through the gap (`agentsfleet_sse_hub_reconnects_total` counts recoveries).

**What this changed at scale.** The pre-cutover idle cost was dominated by N blocking `XREADGROUP BLOCK 5000` loops iterating every five seconds; the fleet's Upstash bill scaled with `(fleets + workers)`, not throughput. After the cutover there are no idle blocking loops — the idle cost is driven by runner **lease poll frequency** (each idle `lease` does one non-blocking `XREADGROUP`), tunable by the runner's `retry_after_ms` backoff rather than a Redis `BLOCK` constant. [`scaling.md`](./scaling.md) re-derives the math.

## The Postgres pool: a saturated pool and a dead datastore are different pages

Every request-path Postgres read takes exactly one pooled connection and holds
it for the life of the handler. A read that acquires a second while holding the
first is how a pool deadlocks under load — two requests each holding one and
waiting for another — so `library_read_counters.MAX_CONNECTIONS_PER_READ` is 1
and the library reads assert it.

An acquire can fail two ways, and they are **different operator problems**:

| Failure | What it means | The fix |
|---|---|---|
| `PoolTimeout` | every connection is leased and the acquire budget elapsed | capacity — pool size, or the slow query holding a slot |
| `PoolUnavailable` | the pool could not produce a connection at all | the datastore — reachability, credentials, TLS |

`Hx.db()` returns those as a named error set. It previously returned
`?DbScope`, which erased the distinction at the handler boundary and put both
behind one alert; the handler that needed to tell them apart worked around it by
acquiring from the pool directly, which meant reimplementing the
acquire/release pairing `DbScope` exists to make unskippable. Both are now
gone. The library reads record the difference as
`agentsfleet_library_pool_result_total{pool_result="timeout"|"error"}`.

**What the pool guarantees, and what it does not.** Releasing an occupied slot
lets at least one queued waiter progress, and every waiter either acquires or
receives the configured timeout — no waiter blocks forever. There is **no
ordering or fairness guarantee**: the vendored `pg.zig` fork wakes waiters from
a 2 ms poll loop rather than a queue (`Io.Condition` has no timed wait), so
which waiter wins is scheduling. `db/pool_bounded_progress_integration_test.zig`
proves the two real guarantees against a live size-1 pool and deliberately
declines to assert the third.

## Config reload — pull-per-lease, no signal

Canonical: [`runner_fleet.md` §Config](./runner_fleet.md) — config resolves fresh from `core.fleets` on every `lease`; a `PATCH` takes effect on the next lease with no cache and no signal. What is specific to this flow: status works the same way — the assignment scan filters `core.fleets.status = 'active'`, so a paused Fleet drops out on the next scan and a resumed one re-enters.

## End-to-end sequence

### A. INSTALL  (`agentsfleet install --library <id>` from an onboarded library entry)

Fleet library onboarding is the source-prep step before Fleet creation, not a second runtime creation path:

```
   dashboard source picker
    │
    ├─► Start from Fleet library
    ├─► Import public GitHub repository/path
    ├─► Manual SKILL.md paste / local library onboarding
    └─► Upload bundle archive            (DEFERRED 2026-06-20, Indy-acked — not in the shipping picker)
            │
            ▼
   agentsfleetd-api
    │  GET  /v1/fleets/bundles                (first-party catalog metadata)
    │  GET  /v1/workspaces/{ws}/fleet-libraries
    │       (platform ∪ workspace tenant library gallery)
    │  POST /v1/workspaces/{ws}/fleet-libraries
    │       body: { source_kind, source_ref }   (upload_ref DEFERRED 2026-06-20)
    │
    ├─► validate archive/path names, size caps, required SKILL.md,
    │    frontmatter, secret-shaped content, and path traversal
    ├─► if TRIGGER.md is missing, keep import valid; install will create
    │    a default manual/API trigger with no tools, secrets, or network
    ├─► [Postgres] store searchable bundle metadata, parsed requirements,
    │    source kind, validation status, and content hash
    ├─► [object storage / R2] store the immutable canonical tar (agentsfleet re-packs the validated files, not GitHub's raw archive), content-hash-addressed
    │    (`fleet-bundles/sha256/{hash}.tar`) — the snapshot the runner untars into the sandbox
    │    for support files; the parsed SKILL.md/TRIGGER.md live in Postgres (above) and ride every
    │    lease. R2 is the SOLE support-file content store (M103); `support_files_json` holds a
    │    path/size/hash manifest only. See [`fleet_bundles.md`](./fleet_bundles.md) for the
    │    two-tier Fleet library/fleet split.
    └─► 201 { id, visibility, requirements, content_hash }   (onboarding; install is a separate POST /fleets)
```

The user-facing copy says Fleet Bundle for source packages and Fleet for the
installed runtime. Runner remains the infrastructure vocabulary.

```
   user / agentsfleet CLI
    │  POST /v1/workspaces/{ws}/fleets
    │  body, platform library entry:
    │       { platform_library_id, name? }
    │  OR body, tenant library entry:
    │       { tenant_library_id, name? }
    ▼
  agentsfleetd-api (create handler)
    │
    ├─► load normalized SKILL.md/TRIGGER.md + immutable snapshot metadata
    │    from the selected library tier (platform or tenant)
    ├─► if trigger_markdown is absent:
    │      generate default manual/API trigger config
    ├─► check required workspace secrets by key name only; never resolve
    │      raw secret values during install
    ├─► [Postgres] INSERT core.fleets          (Row-Level Security (RLS): tenant boundary)
    ├─► [Postgres] INSERT core.fleet_sessions  (checkpoint row:
    │                                         execution_id=NULL,
    │                                         context_json={}, checkpoint_at=now)
    ├─► [Postgres] record nullable bundle snapshot metadata on the Fleet
    ├─► [Redis] XGROUP CREATE MKSTREAM fleet:{id}:events fleet_lease 0
    │           (ensureFleetConsumerGroup — the lease XREADGROUP needs this group)
    └─► 201 to user  (invariant: data stream + group exist before 201)

   No worker thread to spawn. The Fleet is installable work the moment its
   events group exists; the first runner to lease it will claim it.

   At rest:
     Postgres: core.fleets row, core.fleet_sessions idle checkpoint row.
            No core.fleet_events. No billing.usage_ledger. No fleet.runner_leases.
     Redis: stream fleet:{id}:events with group fleet_lease (empty).
            Channel fleet:{id}:activity does not yet exist (implicit on first PUBLISH).
```

### B. TRIGGER  (steer / webhook / cron — three callers, ONE ingress)

Before the GitHub App can produce events, a workspace connection is established
with two independent proofs:

```
   USER       signs up → creates/selects workspace W
                → POST /workspaces/W/connectors/github/connect
   API        signs single-use state bound to W
                → GitHub App installation page
   GITHUB     user chooses account + permitted repositories
                → callback { installation_id, code, state }
   API        state proves W; code exchange + user-installation probe proves
              the returning GitHub user can access installation_id
                → conditional transaction:
                   workspace vault handle + connector_installs route
                   existing other-workspace owner → 403, no mutation
```

The browser-provided installation identifier is therefore a claim to verify,
not authority by itself.

```
   Common envelope (every XADD on fleet:{id}:events carries these
   five fields; the stream entry id IS the canonical event_id —
   never carry a separate id in the payload):

       actor         steer:<user> | webhook:<source> | cron:<schedule>
                     | continuation:<original_actor> | slack:<user>
                     | system:repair-verifier
       type          chat | webhook | cron | continuation
       workspace_id  <uuid>
       request       <opaque JSON — the message + metadata>
       created_at    <epoch milliseconds; project bigint convention>

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
                      actor=webhook:github  type=webhook
                      workspace_id=<ws>     request=<normalized-json>
                      created_at=<ms>
               → 202

               A GitHub App trigger declares both events and repositories:

                 triggers:
                   - type: webhook
                     source: github
                     events: [pull_request]
                     repositories: [acme/payments]

               `repositories` is required for App traffic. Omission means no
               App delivery; it never means every repository in the workspace.
               Multiple fleets may intentionally match. Each gets its own
               replay slot so a failed fan-out leg can retry without duplicating
               successful fleets. The unsigned delivery header remains
               diagnostic and cannot select a new replay identity.

   MANUAL     Custom providers and the old GitHub workflow_run path retain
   WEBHOOK      POST /v1/webhooks/{fleet_id}
                 POST /v1/webhooks/{fleet_id}/github
               with a workspace `<source>.webhook_secret`. The fleet identifier
               is already in the URL, so this route does not require
               `repositories` and does not use `core.connector_installs`.

               The internal Clerk endpoint that bootstraps our own tenants
               on `user.created` is NOT this surface — it lives in the auth
               plane at `POST /v1/auth/identity-events/clerk`. The
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
   > SLACK (M106): a fifth producer — the Slack-resident
   > bot lands an actor=slack:<user> event on fleet:{channel_fleet_id}:events
   > via the webhook-producer XADD shape (signature-authed, no principal —
   > webhooks/fleet.zig) after POST /v1/connectors/slack/events resolves
   > team_id → workspace (core.connector_installs) and (team_id, channel_id) →
   > channel-resident fleet (core.connector_channels). On first mention the
   > fleet is materialized through the existing fleet-create path
   > (innerCreateFleet, seeded with a default channel-bot skill.md) — no new
   > creation actor. One more producer into THIS same ingress — the
   > lease/execute path does not change. The resident fleet owns the channel's
   > memory namespace (keyed by the resident fleet_id), so memory persists
   > thread→thread through the existing hydrate/capture loop
   > ([`runner_fleet.md`](./runner_fleet.md) §"Memory continuity"). Reactive
   > only — read-only tools, no source triggers,
   > no cron, code-set at creation (not from the skill.md prose). Spec:
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
QStash synchronously, and receives the fires. Neither the runner nor its
disposable NullClaw child owns a schedule timer.

#### The webhook auth taxonomy

`webhook_sig` classifies every inbound rejection into one of three codes, each
with a different user action (user-facing registry:
[error-codes#UZ-WH-020](https://docs.agentsfleet.net/api-reference/error-codes#UZ-WH-020)):

- `UZ-WH-020 webhook_credential_not_configured` (error code name unchanged — M112_001
  deferred renaming this constant) — the matching `triggers[].source` is unknown
  to the provider registry, OR the workspace has no `fleet:<source>` vault secret
  (vault row missing OR `webhook_secret` field absent). User-recoverable misconfig
  — fix with `agentsfleet secret create <source> --data @-` and pipe JSON on stdin.
- `UZ-WH-010 invalid_signature` — provider + secret both configured but
  the request is unsigned, mis-signed, or the body was tampered with.
  Either an attack or a real drift between what the provider has
  registered vs the workspace vault — investigate.
- `UZ-WH-011 stale_timestamp` — Slack-style schemes only, request
  timestamp outside the 5-minute drift window. Clock skew or replay.

There is no Bearer fallback. The `Authorization` header is never
consulted on `/v1/webhooks/…` routes. See
[`../AUTH.md`](../AUTH.md) §"Manual fleet-webhook auth" for the full surface.

### C. EXECUTE  (lease → runner → report)

The deleted worker's single in-process `processEvent` loop is now split across two protocol calls. `lease` does the pre-execution control-plane work and hands a self-contained `ExecutionPolicy` to the runner; `report` does the terminal control-plane work after the runner's sandboxed child finishes.

```
   agentsfleet-runner (host)
    │  POST /v1/runners/me/leases   (long-poll; Bearer agt_r)
    ▼
   agentsfleetd — lease handler:

     assign.select():
       non-blocking XREADGROUP fleet:{id}:events across all ACTIVE
       fleets, sticky-ordered by last_runner_id; claim the per-fleet
       fleet.runner_affinity slot (wins iff free or prior lease expired)
       and bump the monotonic fencing_seq. A lease past lease_expires_at
       is RECLAIMED: its event envelope + billing are reused, re-fenced
       with a higher token.

     1. INSERT core.fleet_events                  ← narrative log opens
          (status='received', actor, request_json)
          ON CONFLICT (fleet_id, event_id) DO NOTHING   (idempotent on replay)
     2. PUBLISH fleet:{id}:activity { kind:"event_received", event_id, actor }
     3. Gates + billing (mirror of metering.zig):
          balance gate → budget gate → receive debit → approval gate → run debit.
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
     4. resolveSecretsMap from vault (per-fleet tool secrets,
        workspace-scoped). The provider api_key is resolved separately
        (resolveActiveProvider, fresh + reclaim) and delivered on the lease via
        ExecutionPolicy.provider + ExecutionPolicy.api_key; it does NOT join
        secrets_map and is never substituted into a tool placeholder. The
        runner injects it into the NullClaw child for the inference call only,
        and agentsfleetd keeps it live only through the synchronous lease write.
     5. UPSERT core.fleet_sessions                ← marks busy
          SET execution_id, execution_started_at = now()
     6. issue fleet.runner_leases row              ← durable ownership
          (lease_id, fencing_token, lease_expires_at = now + LEASE_TTL_MS)
     → 200 { event, ExecutionPolicy(config + secrets_map + network_policy
              + tool_allowlist + provider + api_key), instructions, lease_id,
              fencing_token, checkpoint?, bundle_manifest? }
       (`instructions` = the installed fleet's SKILL.md body, extracted server-side
        by FleetSession, so the runner gives NullClaw the installed behaviour and
        not a generic chat — soft reasoning input, never a secret. M84_008.)
       (`bundle_manifest` appears only for fleets installed from a Fleet Bundle. It
        names the immutable snapshot and support-file paths the runner must
        materialize; it never contains resolved secret values.)

       Plaintext lifetime boundary: vault decrypt buffers and canonical secret
       JSON are erased before release; secret store, replace, and credential-mint
       request bodies are erased by the dispatcher after middleware and handler
       completion; every dispatch-arena page is erased by its backing allocator;
       and lease, mint, runner-registration, or API-key creation JSON response
       buffers are erased after a synchronous socket write. A failed sensitive
       write closes that connection. This does
       not claim erasure while bytes are actively in use, or cover authorization
       headers in httpz's connection read buffer.

   agentsfleet-runner — parent (child_supervisor.zig):
       establish cgroup → fork → exec self as `agentsfleet-runner __execute`
       under bwrap (unshare-all + ro-system + rw-workspace + die-with-parent)
       → if bundle_manifest exists, fetch/materialize support files into the
         lease workspace before the child starts
       → feed the lease over child stdin (VLT: secrets only via stdin)
       → read framed frames off child stdout under the lease deadline (poll)

   agentsfleet-runner — sandboxed child (child_exec.zig):
       apply mandatory Landlock (fail-closed on the required tier) →
       build NullClaw config + tool set from the policy → run the fleet turn.
       Bundle files such as SOUL.md, provider playbooks, scripts, examples, or
       assets are ordinary workspace files inside the sandbox. SKILL.md can tell
       the fleet to read them, but capability still comes only from ExecutionPolicy
       and workspace secret grants.
       (fail-closed: an empty installed playbook OR a config-build allocation
        failure reports startup_posture and never invokes the model — the
        provider/key pair is assembled atomically, so a half-built config
        never reaches the engine.)

          args_redacted is built INSIDE the child before any frame leaves:
          any byte range from a secrets_map[NAME][FIELD] substitution is
          replaced with the ${secrets.NAME.FIELD} placeholder. Resolved
          secret bytes never appear on the pipe and never reach activity.

          on tool_call_started   → A frame → parent → POST .../activity
          on fleet_response_chunk → A frame → parent → POST .../activity
          on tool_call_progress  → A frame → parent → POST .../activity
                                   (long-tool heartbeat; absence past ~5s
                                    renders as "stuck" in the UI)
          on tool_call_completed → A frame → parent → POST .../activity
          │
          └─ terminal: R frame ExecutionResult{ content, tokens, ttft_ms,
                                                wall_ms, outcome }

   agentsfleet-runner — parent:
       collect the ExecutionResult, classify timeout/OOM/crash/startup_posture,
       scope.destroy() (idempotent), then:
    │  POST /v1/runners/me/reports { lease_id, fencing_token, outcome, ... }
    ▼
   agentsfleetd — report handler:

     claimReport(): atomic CAS —
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
    10. UPSERT core.fleet_sessions                ← idle bookmark
          SET context_json = { last_event_id, last_response },
              execution_id = NULL, checkpoint_at = now()
    11. XACK fleet:{id}:events                    ← consumer cursor advances
    12. release affinity (WHERE fencing_seq = $token)  ← token-guarded

   Runner dies mid-event → its lease expires at lease_expires_at; the next
   lease's reclaim path re-issues the event to another runner with a higher
   fencing_token. Step 1's ON CONFLICT and the UNIQUE telemetry event_id keep
   the replay safe — exactly one fleet_events row, exactly one telemetry row,
   regardless of how many redelivery attempts occur. A late report from the
   dead runner is fenced out at claimReport (UZ-RUN-005).
```

**The issue-time run debit is a daemon divergence during cutover (M177).** The
sequence above is the INTENDED billing shape and is what `agentsfleetd-rs`
implements. The Zig daemon does not: `fleet/service_billing.zig` ends its gate
pass with `// No issue-time stage debit: run fee + tokens meter on /renew +
settle at report`, and `fleet_runtime/metering.zig` exports `debitReceive` as
its only debit. So during cutover the two daemons charge differently at lease —
the Rust one debits a floor-token run estimate that the Zig one defers to
`/renew`. This is a deliberate, declared divergence rather than a port defect
(Indy, M177 §2): the Rust daemon is written to the documented behaviour, the
Zig daemon is not being changed, and M181 §4 carries the divergence into the
cutover register. Anything reconciling ledger rows across the two daemons has
to know which one wrote them.

**Slack-resident answer round-trip (M106).** For the Slack producer in §"B. TRIGGER" two connector-specific hops bracket this generic trace without altering it. *At ingress:* `connectors/slack/thread.zig` does a best-effort re-read of the recent thread (Slack `conversations.replies`, bounded to the last-N messages) so the leased `request_json` carries same-thread context. It **never throws**: a failed or absent re-fetch degrades to an empty thread, and the answer still runs from the mention alone. *On the way out:* the answer is not posted from the report handler directly. Step 7's report path calls `enqueueOutboundAnswer` (`fleet/service_report.zig`) — if the reporting fleet has a `core.connector_channels` binding it enqueues a `provider`-tagged job onto the generic `connector:outbound` stream (`queue/connector_outbound.zig`); a non-connector fleet, empty answer, or any failure is a logged no-op that never fails the finalized report. The boot-started `outbound/worker.zig` consumer (the one blocking Redis consumer sized in [`scaling.md`](./scaling.md)) then reads the job, routes it by `provider`, and posts the answer back in-thread with bounded retry + pending-first redelivery. The core report path stays provider-agnostic (Invariant 9) — the worker is the only place a connector poster is imported.

### D. WATCH  (user-side: how the live tail surfaces)

```
   CLI       agentsfleet steer <fleet_id> "<message>"   (batch mode)
               → opens GET /v1/.../fleets/{id}/events/stream (SSE) BEFORE
                 posting the message, and waits (bounded, 2 s) for response
                 headers — the server SUBSCRIBEs before it writes SSE
                 headers, so headers-received means the subscription is
                 live and the POST cannot race the event's first frame.
               → server SUBSCRIBE fleet:{id}:activity on a dedicated
                 Redis connection held outside the request-handler pool
                 (SUBSCRIBE blocks the conn).
               → frames arriving before the 202 names the event wait in a
                 bounded client-side buffer (drop-oldest) and replay in
                 order once the id is known; a tail that misses the ready
                 bound is closed unheard and the durable events list alone
                 decides the outcome (a late tail must never pass a
                 truncated reply off as complete).
               → forward each PUBLISH as an SSE frame, one per line:
                   id:<seq>\nevent:<kind>\ndata:<json>\n\n
               → on disconnect: UNSUBSCRIBE, close.

   UI        Fleet Console /fleets/{id}
               → same per-fleet GET /events/stream SSE consumer.
               → on page load also fetches GET /events?limit=20 for
                 recent history context.

   UI        Fleets Wall /fleets
               → opens ONE GET /v1/workspaces/{id}/events/stream SSE
                 connection for every visible live fleet.
               → agentsfleetd authorizes the workspace and fans in only its
                 readable fleet:{id}:activity channels through one bounded
                 shared-consumer ring.
               → first frame is hello { fleet_ids:[...] }; this is the live
                 set the wall trusts for quiet-versus-last-known status.
               → activity data gains fleet_id; the wall routes it to one tile.
               → if the bounded ring drops old frames, agentsfleetd sends
                 catching_up { dropped:N }; the wall shows recovery state.
               → hello and catching_up use id:0 without advancing the
                 per-connection activity sequence.

   SSE auth (dual-accept, strict no-fallthrough). The endpoint accepts
   EITHER a session cookie (browser EventSource path; cookie sent
   automatically) OR Authorization: Bearer <api_key> (CLI path; Node
   fetch can set custom headers). Resolution order:
     if request has Cookie header → validate cookie → 401 on failure
                                     (do NOT also try Authorization).
     elif request has Authorization → validate Bearer → 401 on failure.
     else → 401.
   A stale or leaked cookie does not silently fall through to a valid
   Bearer; the request is 401'd. No query-param tokens (avoids leaking
   long-lived API keys via URL / referrer / access logs).

   Reconnect / sequence id. The id:<seq> line on each SSE frame is a
   per-connection in-memory monotonic counter that resets to 0 on each
   new SUBSCRIBE. The server IGNORES the Last-Event-ID request header —
   sequence ids are not durable and have no cross-connection meaning.
   Clients backfill after reconnect through the matching events list. The
   first request uses a server-time `since` floor; later pages use only the
   server-issued `next_cursor`. Clients never derive a cursor from an event id.
   The new SSE resumes its activity sequence from 0.

   HISTORY   agentsfleet events {id} [--actor=…] [--since=2h]
             Dashboard /fleets/{id}/events
               → reads core.fleet_events (cursor-paginated).

   STATUS    agentsfleet status {id}
               → reads core.fleet_sessions
                 ("busy or idle, last response").

   If a live frame drops (slow consumer, network blip), the client pulls
   the gap from the matching GET /events list. Live tail is best-effort;
   the durable record is core.fleet_events.
```

### KILL

```
   user
    │  POST /v1/.../fleets/{id}/kill
    ▼
  agentsfleetd
    ├─► UPDATE core.fleets SET status='killed' (PG)
    ├─► mark the in-flight fleet.runner_leases row revoked
    └─► 202 to user

  agentsfleet-runner  (next heartbeat)
    ├─► POST /v1/runners/me/heartbeats  → reply carries the revoked lease id
    ├─► kill the sandboxed child (cgroup tree-kill)
    └─► POST /v1/runners/me/reports { outcome: cancelled }
            → claimReport finalizes 'cancelled'; a late report from the
              killed child is fenced out by fencing_token.

   Cancel latency is bounded by the heartbeat interval. A dedicated
   low-latency cancel channel can come later; heartbeat-carried
   revocation is the S0 mechanism.
```

## Multi-tenancy boundary

| Layer | Tenant isolation mechanism |
|---|---|
| PG (`core.fleets`, `core.fleet_events`, etc.) | Row-Level Security by `workspace_id`. The API enforces via `app.workspace_id` session var; the control-plane lease/report path uses the service role with explicit WHERE filtering. |
| Redis data plane (`fleet:{id}:events`) | Key namespaced by fleet UUID (globally unique); no cross-tenant collision possible. No RLS in Redis — protected by `fleet_id` being unguessable + API gatekeeping. |
| Runner ↔ control plane | The `agt_r` token authenticates the runner per call; `me` resolves from the token. The lease carries exactly one fleet's event + scoped secrets; a runner never sees another tenant's data plane. Enrollment is gated on the `platform_admin` claim (M80_005) — only agentsfleet's platform admin may add a host to the shared fleet, via the dashboard "Add runner" (M84_001). Trust-gated placement (don't put other-tenant work on a weak sandbox tier) is operator-assigned, deferred to a later milestone (M85_001 shipped label-matching placement only, not trust tiers; M80_007 shipped as the observability spec). |
| Sandboxed child | Per-execution: secrets resolved at the lease, delivered via the child's stdin only, substituted at the tool bridge inside the sandbox, never flowing as raw strings into fleet context. |

## One active lease per fleet — the ownership model

Before the cutover, a single worker thread owned all events for a Fleet, and the concern was round-robin across worker replicas breaking per-fleet continuity. That model is gone. Ownership is now a **durable lease**, not a thread:

- `fleet.runner_affinity` holds one slot per fleet. `assign.select` claims it atomically — a runner wins iff the slot is free or the prior lease has expired — and bumps a monotonic `fencing_seq`. So **at most one lease is active per fleet at any time**, regardless of how many runners poll concurrently.
- A runner that loses the race for a Fleet simply gets no lease for it and tries the next eligible fleet (or backs off).
- Continuity across runs is the checkpoint in `agentsfleetd`, not runner-local state — so any runner can pick up the next run. Sticky routing (prefer `last_runner_id`) is a hint for warm-sandbox reuse, never ownership.

Failure mode: a dead lease holder blocks its fleet until `lease_expires_at`; reclaim then re-leases with a higher fencing token. Recovery latency = TTL plus poll density (the S0 lazy-reclaim SLA). Tightening it is M80_006.

## What the coding fleet never does

- Never sees the fleet's LLM tokens or reasoning state
- Never holds the fleet's secrets in its own context
- Never executes the fleet's tool calls in its own session
- Never persists across the user's laptop being closed

## What the fleet (host) never does

- Never touches the user's laptop directly
- Never reads the user's local filesystem (it sees only what the SKILL.md and TRIGGER.md grant it)
- Never escapes the sandbox — Landlock (filesystem) + cgroups (process/memory kill domain) bound the runner's child. **Network egress** is fully blocked on the `deny_all` policy (empty net namespace via `--unshare-all`) and, on the network-enabled policy, constrained to an operator-declared host allowlist by the **runner egress model** (own net namespace + host-side nftables IP-allowlist (resolve-at-setup, resolver-less) — see [`runner_fleet.md` §Egress model](./runner_fleet.md)). Note the network-enabled policy historically shared the host net namespace (`--share-net`, allowlist log-only) with no kernel egress restriction; that is the gap the egress model closes.
- Never holds a datastore credential — the runner reaches the platform only over the `/v1/runners` protocol

## The install failure scenario, visually

The API server (not a runner) is the side that writes to Redis during install. So a Redis blip during install hits the API → Redis hop. The API has two layers of defence:

1. **Inline retry (API).** `ensureEventStream` retries `XGROUP CREATE MKSTREAM fleet:{id}:events` on a fixed backoff `[100ms, 500ms, 1500ms]` — four attempts, ~2.1s total wall budget. Most blips never escape this loop. (The group is load-bearing — the `lease` `XREADGROUP` needs it.)
2. **PG rollback (API).** If retries exhaust, the handler `DELETE`s the freshly-inserted `core.fleets` row and returns 500 with `hint=rolling_back_pg_row` so the caller can retry cleanly. No orphan.

**The pre-cutover third layer (watcher reconcile sweep) is gone** with the worker. A rare double fault (group setup exhausts retries AND rollback fails) now leaves an orphaned `core.fleets` row, logged `hint=row_orphaned_manual_recovery`, healed by an operator or a future reconcile job. The orphan is inert: no runner can lease it (no events group), no live tail.

```
   TIME ──►
   t=0  USER → agentsfleet install → API
   t=2  API: INSERT core.fleets (status='active') → PG ✓
   t=3  API: XGROUP CREATE MKSTREAM ╳ (4 retries exhausted, ~2.1s)
   t=4  API: DELETE core.fleets row ╳ (rare second failure)
   t=5  API: 500 → user. Logs: fleet.create_stream_setup_failed,
                              fleet.create_rollback_failed

   ── ORPHAN WINDOW (until operator / future reconcile job) ──
      PG row Z = active; Redis stream + group missing. Other fleets
      unaffected. No runner can lease Z (its events group does not exist).
```

A future reconcile job (a control-plane sweep over `core.fleets` for `active` rows whose events group is missing, calling `redis_agent.ensureFleetConsumerGroup`) is the planned replacement for the deleted watcher's healing role; it is out of scope here.

---

## Notable invariants this flow proves

- **No race on stream / group creation.** `innerCreateFleet` does INSERT + `XGROUP CREATE` synchronously before returning 201. Any event arriving within microseconds of the 201 finds the stream already there, ready to be leased.
- **All triggers funnel into one ingress.** Webhook, cron, steer, continuation, the Slack bot, and the repair verifier are different *producers* into `fleet:{id}:events`; the lease path doesn't branch on actor type.
- **Secrets never enter fleet context.** Substitution happens at the tool bridge, inside the runner's sandboxed child, after sandbox entry. The fleet sees `${secrets.fly.api_token}`; HTTPS request headers get real bytes; responses never echo the token; the bytes never cross the activity pipe.
- **Exactly one active lease per fleet.** The atomic affinity claim + monotonic fencing token guarantee a single in-flight lease per fleet no matter how many runners poll.
- **Reclaim is lease-layer, not Redis-consumer.** A dead runner is reclaimed via `lease_expires_at` + `fencing_token`, never `XAUTOCLAIM` — Redis cannot observe an off-platform processor's death.
- **Late writers are fenced.** A reclaimed or killed runner's `report` is rejected by the `fencing_token` CAS, so it cannot mutate state. Negative-tested.
- **Long-running runs don't crash the model.** The three context-lifecycle layers (see [`capabilities.md`](./capabilities.md) §4) keep context bounded. If a single incident exceeds budget, the fleet chunks and continues in a new run from a `memory_recall` snapshot — possibly on a different runner.
