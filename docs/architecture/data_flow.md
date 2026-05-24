# Data Flow — how an event moves through the system

> Parent: [`README.md`](./README.md)
>
> **Scope:** this file describes the single-process model — `zombied` running the `worker` loop plus the `zombied-executor` sidecar. The M80 split moves execution into a host-resident `zombie-runner` that holds no datastore credentials (see [`runner_fleet.md`](./runner_fleet.md)); the two reconcile in M80_003.

Read this when you need to know where a webhook, a steer, or a cron fire ends up. Many specs reference this file as the canonical picture of the runtime.

## Process and stream ownership at a glance

| Process | Role |
|---|---|
| **`zombied-api`** (`zombied serve`) | HTTP routes. Writes to `core.zombies` and the vault, produces messages on `zombie:control`. Steer, webhook, cron, and continuation handlers all `XADD` directly to `zombie:{id}:events` — single ingress, no transient `zombie:{id}:steer` key. Reads `core.zombie_events` for history. |
| **`zombied-worker`** (`zombied worker`) | Hosts one watcher thread that consumes `zombie:control` plus N per-zombie threads, each consuming one `zombie:{id}:events` stream. Owns the per-zombie cancel flags. The worker is the sole publisher on `zombie:{id}:activity`. Never runs language-model code. |
| **`zombied-executor`** (sidecar; `zombied executor`) | Unix-socket remote-procedure-call server speaking `rpc_version: 2` (HELLO handshake on connect; mismatch fast-fails). Hosts the NullClaw agent inside a Landlock + cgroups + bwrap sandbox. Credential substitution lives here; `args_redacted` is rebuilt before any progress frame leaves the remote-procedure-call boundary. |

| Target | Producer | Consumer |
|---|---|---|
| `zombie:control` | `zombied-api` on install / status change / config patch | `zombied-worker` watcher thread |
| `zombie:{id}:events` | `zombied-api` on steer / webhook / continuation; NullClaw cron-tool fires; worker on chunk-continuation | `zombied-worker`'s per-zombie thread |
| `zombie:{id}:activity` | `zombied-worker` (sole publisher) | Server-Sent-Events handler in `zombied-api` on a dedicated Redis connection |
| `core.zombie_events` | `zombied-worker` zombie thread (INSERT received → UPDATE terminal) | `zombied-api` `GET /events` endpoints, dashboard, `zombiectl events` |
| `core.zombies` | `zombied-api` only | `zombied-worker` at claim + watcher tick |
| `core.zombie_sessions` | `zombied-worker` (checkpoint + execution_id) | `zombied-worker` at claim + kill path |
| `vault.secrets` | `zombied-api` on `credential set` (upsert) | `zombied-worker` resolves just-in-time before each `createExecution` |

---

## The two agents in play

Two distinct agents are in play. Keeping them straight is essential to understanding the architecture:

```
┌────────────────────────────────┐         ┌──────────────────────────────┐
│  USER'S AGENT (laptop)         │         │  ZOMBIE'S AGENT (cloud/host) │
│                                │         │                              │
│  Claude Code / Amp / Codex /   │         │  NullClaw running inside     │
│  OpenCode driving zombiectl    │         │  zombied-executor            │
│                                │         │  (sandboxed Landlock+cgroups │
│  This is what the human types  │         │   +bwrap, durable, persists  │
│  into. Ephemeral.              │         │   across user's laptop close)│
└────────────────────────────────┘         └──────────────────────────────┘
```

The user's agent is a workstation tool driving `zombiectl`. The zombie's agent is a long-lived NullClaw instance inside the executor sandbox. The user's agent never becomes the zombie's agent and never sees its tokens — they communicate only through the steer endpoint, the event stream, and the events history.

## Steer flow end-to-end

```
                "what's the deploy status?"
                          ↓
         User's Agent → zombiectl steer <zombie_id> "<msg>"
                          ↓

           ╔═══════════════════════════════════╗
           ║  zombied-api (HTTP)               ║
           ║  POST /v1/.../zombies/{id}/messages║
           ║  ───────────────────────────────  ║
           ║  XADD zombie:{id}:events *         ║   ← single ingress.
           ║       actor=steer:<user>           ║     Webhook + cron use
           ║       type=chat                    ║     the same XADD.
           ║       workspace_id=<uuid>          ║     No SET/GETDEL key.
           ║       request=<msg-json>           ║
           ║       created_at=<epoch_ms>        ║
           ║  → 202 { event_id }                ║
           ╚═══════════════════════════════════╝
                          ↓
           ╔═══════════════════════════════════╗
           ║  zombied-worker (zombie thread)   ║
           ║  ───────────────────────────────  ║
           ║  XREADGROUP unblocks ──────────────╫───┐
           ║                                    ║   │
           ║  processEvent():                   ║   │
           ║   1. INSERT core.zombie_events     ║   │   ← narrative log
           ║      (status='received',           ║   │     opens (mutable)
           ║       actor, request_json)         ║   │
           ║   2. PUBLISH zombie:{id}:activity  ║   │   ← live: pub/sub
           ║      {kind:"event_received"}       ║   │     channel (ephemeral,
           ║                                    ║   │     no buffer, no ACK)
           ║   3. balance gate, approval gate   ║   │   See
           ║   4. resolve creds from vault      ║   │   [`capabilities.md`](./capabilities.md)
           ║   5. UPSERT core.zombie_sessions   ║   │   each layer.
           ║      SET execution_id, started_at  ║   │   ← resume cursor:
           ║      (one row per zombie, mutable) ║   │     marks zombie busy
           ║   6. executor.createExecution      ║   │
           ║         (workspace_path,           ║   │
           ║          {network_policy, tools,   ║   │
           ║           secrets_map, context})   ║   │
           ║   7. executor.startStage           ║   │
           ║         (execution_id, message)    ║   │
           ╚═══════════════════════════════════╝   │
                          ↓                         │
           ╔═══════════════════════════════════╗   │
           ║  zombied-executor (RPC over Unix) ║   │
           ║  ───────────────────────────────  ║   │
           ║  handleStartStage(...)             ║   │
           ║  → runner.execute(NullClaw Agent)  ║   │
           ║                                    ║   │
           ║   NullClaw reasons over msg.       ║   │
           ║   Calls tools per its SKILL.md.    ║───┘
           ║   Each tool call → tool bridge     ║
           ║   substitutes ${secrets.NAME.x}    ║       This is the
           ║   at sandbox boundary, then        ║       "ZOMBIE'S AGENT".
           ║   HTTPS request fires.             ║       It's an LLM in a
           ║                                    ║       sandbox; user's
           ║   For each progress event,         ║       agent never
           ║   the worker (NOT the executor)    ║       becomes it,
           ║   PUBLISHes zombie:{id}:activity:  ║       never sees its
           ║     - tool_call_started            ║       tokens or context.
           ║     - agent_response_chunk         ║
           ║     - tool_call_completed          ║
           ║                                    ║
           ║   Agent returns StageResult.       ║
           ║  → {content, tokens, ttft_ms,      ║
           ║     wall_ms, exit_ok}              ║
           ╚═══════════════════════════════════╝
                          ↓
           ╔═══════════════════════════════════╗
           ║  zombied-worker (zombie thread)   ║
           ║  ───────────────────────────────  ║
           ║   8. UPDATE core.zombie_events     ║   ← narrative log
           ║      status='processed'            ║     closes (same row)
           ║      response_text=<content>       ║
           ║      completed_at=now()            ║
           ║   9. INSERT zombie_execution_      ║   ← billing/latency
           ║      telemetry                     ║     audit (immutable,
           ║      (event_id UNIQUE, tokens,     ║     UNIQUE event_id)
           ║       ttft_ms, wall_seconds,       ║
           ║       plan_tier, credit_cents)     ║
           ║  10. UPSERT core.zombie_sessions   ║   ← resume cursor:
           ║      SET context_json={last_       ║     clears execution
           ║         event_id, last_response},  ║     handle, advances
           ║      execution_id=NULL,            ║     bookmark
           ║      checkpoint_at=now()           ║
           ║  11. PUBLISH zombie:{id}:activity  ║   ← live: terminal
           ║      {kind:"event_complete",       ║     SSE frame
           ║       status:"processed"}          ║
           ║  12. XACK zombie:{id}:events       ║
           ╚═══════════════════════════════════╝
                          ↓
   User's Agent's `zombiectl steer <zombie_id>` polls GET /events
   (or SSE-tails GET /events/stream which SUBSCRIBEs
    zombie:{id}:activity)
                          ↓
       [claw] <the zombie's response, streamed>
                          ↓
                  User reads it.
```

## The three durable stores: who owns what

The flow above writes to three Postgres tables. They are **not** redundant — each answers a distinct user question, has a different cardinality, mutability, and retention contract. Use the right one for the right question.

| Table | Cardinality | Mutability | Answers |
|---|---|---|---|
| `core.zombie_sessions` | **One row per zombie** | UPSERT — mutated on every event boundary | "Where is this zombie *right now*? Is it idle or executing? What was its last successful response?" — the worker's resume bookmark + active-execution handle. Read at claim, written at start + end of each event. |
| `core.zombie_events` | **One row per delivery** | INSERT (status=`received`) → UPDATE (status=`processed` \| `agent_error` \| `gate_blocked`) | "What did this zombie do for event X? Who triggered it, what did they ask, what did it answer, did the gates pass?" — the user's narrative log. The single source of truth for the Events tab and `zombiectl events`. |
| `zombie_execution_telemetry` | **Two rows per event** under M48: one `charge_type='receive'` written at the receive deduct, one `charge_type='stage'` written at the stage deduct (then UPDATEd with token counts after `StageResult`). UNIQUE `(event_id, charge_type)`. | INSERT at each debit, immutable for the `credit_deducted_nanos` column; stage row UPDATEd once with token counts. | "How much did event X cost (split by receive vs stage)? How fast was it? What posture was charged?" — billing + latency audit. Joinable to `zombie_events` via `event_id`. Aggregated for p95 latency, token-spend rollups, credit deductions. |

Why two per-delivery tables (`events` + `telemetry`) instead of one? They have different write authorities and retention contracts:

- `zombie_events` holds user-readable strings (`request_json`, `response_text`) — large, mutable mid-lifecycle, deletable on tenant offboarding.
- `zombie_execution_telemetry` holds numeric audit columns — small, immutable once written, retained for billing reconciliation independent of whether the conversation row is purged.

## Concrete platform-ops example

A GitHub Actions deploy fails on `usezombie/usezombie@c0a151bd`. The webhook lands as `event_id=1729874000000-0`, `actor=webhook:github`. Here is exactly what each row holds at each stage.

**Before the event** — `zombie_sessions` shows the zombie idle since the previous event:

```
core.zombie_sessions  (one row, the zombie itself)
─────────────────────────────────────────────────
zombie_id            f4e3c2b1-...
context_json         {"last_event_id": "1729873200000-0",
                      "last_response":  "All apps healthy at 07:30Z."}
checkpoint_at        1729873208000
execution_id         NULL          ← idle
execution_started_at NULL
```

**Step 1 — INSERT `zombie_events`** (status=`received`):

```
core.zombie_events  (new row, narrative-log opens)
──────────────────────────────────────────────────
zombie_id      f4e3c2b1-...
event_id       1729874000000-0
workspace_id   8d2e1c9f-...
actor          webhook:github
event_type     webhook
status         received
request_json   {
  "message":  "GH Actions workflow_run failure on
               usezombie/usezombie deploy.yml run 9876",
  "metadata": {"run_id": 9876, "head_sha": "c0a151bd",
               "conclusion": "failure", "ref": "main",
               "repo": "usezombie/usezombie", "attempt": 1}
}
response_text  NULL
created_at     2026-04-25T08:00:00Z
completed_at   NULL
```

**Step 5 — UPSERT `zombie_sessions`** (mark busy, do *not* touch `zombie_events`):

```
core.zombie_sessions  (same row, mutated)
─────────────────────────────────────────
execution_id         exec-7af3c2b1-...   ← now busy
execution_started_at 1729874001000
(other fields unchanged from "before")
```

NullClaw runs in the executor: fetches GH run logs via `${secrets.github.api_token}`, fetches Fly app logs, fetches Upstash Redis stats, posts a remediation message to Slack. Returns `StageResult{content, tokens=1840, wall_ms=8210, ttft_ms=320, exit_ok=true}`.

**Step 7 — UPDATE `zombie_events`** (close the same row):

```
core.zombie_events  (same row, narrative-log closes)
────────────────────────────────────────────────────
status         processed
response_text  "Deploy failed: Fly.io OOM kill on machine i-01abc,
                app over 4GB resident. Last successful migration at
                c0a151bc. Posted to #platform-ops with rollback-to-
                c0a151bc remediation."
completed_at   2026-04-25T08:00:08Z
```

**Step 8 — INSERT `zombie_execution_telemetry`** (immutable audit row, joinable on `event_id`):

```
zombie_execution_telemetry  (new row, write-once)
─────────────────────────────────────────────────
id                       tel-1729874000000-0
zombie_id                f4e3c2b1-...
workspace_id             8d2e1c9f-...
event_id                 1729874000000-0   ← UNIQUE; joins to zombie_events
token_count              1840
time_to_first_token_ms   320
wall_seconds             8
epoch_wall_time_ms       1729874000000
plan_tier                free
credit_deducted_cents    4
recorded_at              1729874008210
```

**Step 9 — UPSERT `zombie_sessions`** (advance bookmark, clear execution handle):

```
core.zombie_sessions  (same row, mutated)
─────────────────────────────────────────
context_json         {"last_event_id": "1729874000000-0",
                      "last_response":  "Deploy failed: Fly.io OOM kill..."}
checkpoint_at        1729874008210
execution_id         NULL          ← idle again
execution_started_at NULL
```

## Reading the three tables

- `zombiectl status {id}` reads **`zombie_sessions`** — answers "is the zombie executing right now, and where did it leave off?"
- `zombiectl events {id} [--actor=…]` reads **`zombie_events`** — answers "what has this zombie done, what was asked, what did it reply, did any gate block it?"
- Billing rollups + p95 dashboards read **`zombie_execution_telemetry`** — answers "how many tokens this month, what's the latency tail?"

If only **one** table existed, every user query would either pay full-table-scan cost (one row per delivery for "is it busy now?") or lose immutability guarantees on billing audit (mutable narrative columns alongside immutable spend columns). Three tables, three contracts, one join key (`event_id`).


## Two streams + one pub/sub channel — three surfaces, three jobs

| Redis surface | Type | Cardinality | Purpose | Volume |
|---|---|---|---|---|
| `zombie:control` | Stream + consumer group `zombie_workers` | One, fleet-wide | Lifecycle signals (`created` / `status_changed` / `config_changed` / `drain_request`) — tells the watcher to spawn / cancel / reconfig per-zombie threads. | Low — only on install / kill / patch. |
| `zombie:{id}:events` | Stream + consumer group `zombie_workers` | One per zombie | Single event ingress — steer / webhook / cron / continuation all `XADD` here. At-least-once delivery via `XREADGROUP`, `XACK`ed at end of `processEvent`. Idempotent on replay via `INSERT ON CONFLICT`. | High — every event the zombie handles. |
| `zombie:{id}:activity` | Pub/sub channel (no consumer group, no persistence) | One per zombie | Best-effort live tail — worker `PUBLISH`es one frame per `event_received` / `tool_call_started` / `agent_response_chunk` / `tool_call_progress` (~2s heartbeat during long tool calls) / `tool_call_completed` / `event_complete`. SSE handler `SUBSCRIBE`s and forwards. No buffer, no ACK, no resume; if a frame drops, fall back to `GET /events` for the durable record. | High during execution, zero when idle. Subscribers get messages only while connected. |

The two streams are durable (events appended, `XACK`ed entries pruned) and back the at-least-once delivery contract. The pub/sub channel is ephemeral and exists only to power live user UIs — its loss never affects correctness, only what the user sees in real time. Durable activity history lives in `core.zombie_events`; the pub/sub channel is the eyeballs surface, not the audit surface.

## Connection topology — pool vs. dedicated

The three Redis surfaces above ride on **two distinct connection patterns** inside the process. Mixing them — specifically, pooling a long-lived blocking read — would silently exhaust the request-path pool the first time a customer crossed the pool's `max_idle` in concurrent zombies. The split is load-bearing.

```
                        REDIS CONNECTION TOPOLOGY
                        ═════════════════════════

  ┌─────────────────────────────────────────────────────────────────────────────────┐
  │                        POOL  (max_idle=8, eager_min=2)                           │
  │                  ──── short-lived request-path commands ────                     │
  │                                                                                  │
  │   acquire → command → release    (microseconds to milliseconds per cycle)        │
  │                                                                                  │
  └────▲─────────────────────────────▲────────────────────────────▲──────────────────┘
       │                             │                            │
       │ XADD                        │ XADD                       │ PUBLISH
       │ zombie:{id}:events          │ zombie:control             │ zombie:{id}:activity
       │ (from steer / webhook       │ (from install path)        │ (from worker after
       │  / cron / continuation)     │                            │  each event step)
       │                             │                            │
   ┌───┴────────┐               ┌────┴────────┐              ┌────┴──────────┐
   │ HTTP       │               │ Watcher     │              │ Per-zombie    │
   │ handlers   │               │ thread      │              │ worker thread │
   │ in         │               │ in          │              │ in            │
   │ zombied-api│               │ zombied-    │              │ zombied-      │
   │            │               │   worker    │              │   worker      │
   └────────────┘               └─────────────┘              └───────────────┘


  ┌──────────────────────────────────────────────────────────────────────────────────┐
  │                   DEDICATED CONNECTIONS  (NOT in the pool)                        │
  │                    ──── long-lived blocking reads ────                            │
  │                                                                                   │
  │   Watcher: 1 dedicated conn       Per-zombie worker: 1 conn per zombie thread     │
  │   ─────────────────────────       ─────────────────────────────────────────       │
  │                                                                                   │
  │   ┌──────────────────┐            ┌─────────────────┐  ┌─────────────────┐        │
  │   │ XREADGROUP       │            │ XREADGROUP      │  │ XREADGROUP      │        │
  │   │ zombie:control > │            │ zombie:Z1:events│  │ zombie:Z2:events│  ...   │
  │   │ BLOCK 5000ms     │            │ BLOCK 5000ms    │  │ BLOCK 5000ms    │        │
  │   │   (loops)        │            │   (loops)       │  │   (loops)       │        │
  │   └──────────────────┘            └─────────────────┘  └─────────────────┘        │
  │                                                                                   │
  │                                                                                   │
  │   SSE subscribers: 1 conn per live tail                                           │
  │   ─────────────────────────────────────                                           │
  │                                                                                   │
  │   ┌──────────────────────┐       ┌──────────────────────┐                         │
  │   │ SUBSCRIBE            │       │ SUBSCRIBE            │                         │
  │   │ zombie:Z1:activity   │       │ zombie:Z2:activity   │  ...                    │
  │   │  (held while SSE     │       │  (held while SSE     │                         │
  │   │   client is open)    │       │   client is open)    │                         │
  │   └──────────────────────┘       └──────────────────────┘                         │
  │                                                                                   │
  └───────────────────────────────────────────────────────────────────────────────────┘
```

**The rule.** A connection held across a Redis call that blocks the server (XREADGROUP with BLOCK, SUBSCRIBE) cannot return to a pool — its lifetime is tied to the consumer, not the request. The pool is therefore reserved for commands that complete in milliseconds: XADD, PUBLISH, XACK, command-style operations on `core` data structures.

**Why this matters at scale.** Suppose `max_idle = 8` and every per-zombie worker pulled a pooled connection for its `XREADGROUP BLOCK 5000` loop. The 9th zombie installs successfully (worker spawns) but cannot acquire a connection to begin reading its events — pool exhausted, worker stuck. usezombie supports ~unbounded zombies per worker process; the pool topology must not be the bottleneck. Same argument applies to SSE subscribers: a customer with 100 dashboard tabs open should not be able to exhaust the request-path pool just by watching.

**Where this is enforced.** `src/queue/redis_pool.zig` (introduced by M69_004) exposes only `acquire`/`release` for short-lived commands. The blocking-XREADGROUP consumers — `src/queue/redis_zombie.zig` (per-zombie events loop) and `src/cmd/worker_watcher_poll.zig` (watcher's control loop) — each construct a dedicated `Connection` via `Connection.init(...)` and own it for the lifetime of the thread. SSE subscribers use a separate `Subscriber` type with its own connection (the unified `src/queue/redis_subscriber.zig`).

**Sizing the fleet.** This topology determines connection budget and Upstash-billing math at scale. See [`scaling.md`](./scaling.md) for the worked 100-zombie / 10-worker example, the tuneup-knob table (`REDIS_POOL_MAX_IDLE`, `XREADGROUP BLOCK` duration, etc.), Upstash plan-ceiling considerations, and failover-storm behavior.

## Synthetic system events

Most events on `zombie:{id}:events` come from outside the zombie: a steer the user typed, a webhook the platform received, a cron tick. The runtime also produces a smaller class of events on its **own** behalf — synthetic `system:*` rows that record state changes the worker has just *applied*, not events it was asked to handle. `config_updated` (ack of a `PATCH /v1/.../zombies/{id}` reload) is the first kind; future kinds (`balance_exhausted`, `manual_pause`, `kill_received`) follow the same shape.

These rows sit in the same durable + ephemeral surfaces as any other event, with three differences:

- The durable record lands in `core.zombie_events` with `actor='system:<change>'`, `event_type='system'`, `status='processed'`, `event_id='<kind>-<natural-key>'`. The deterministic event_id collides on the `(zombie_id, event_id)` primary key under retry, so re-emission on the same key is a no-op INSERT.
- The live frame lands on `zombie:{id}:activity` with `kind: '<change>'`, **gated on the INSERT actually writing a row** — never publish without a durable record.
- There is *no* entry on `zombie:{id}:events` and *no* XACK cycle. Synthetic system events are not events the worker reasons over; they record state transitions, not work to do. They never enter `zombie_execution_telemetry`.

### Single-publisher invariant

Only the worker that owns a zombie's event loop may publish to `zombie:{id}:activity` (declared at the top of `src/zombie/activity_publisher.zig`). HTTP handlers therefore **cannot** synthesise these acks themselves — the user-visible signal must originate worker-side, *after* the state change has been applied, not at the moment the API accepted the PATCH.

Why this matters: PATCH `/v1/.../zombies/{id}` succeeds the moment the row is written and the `config_changed` signal is XADDed to `zombie:control`. The worker may not pick that signal up for milliseconds; the per-zombie thread may be mid-stage with `reload_pending` flipped but not yet applied; `reloadZombieConfig` may fail (bad triggers, vault permission denied). An API-side ack would lie. The worker-emitted ack ties the activity-channel frame to operational reality — if the user sees "Configuration updated to revision N", the new triggers are armed, the new credential names are resolved, and the zombie's next event will use them.

### `config_updated` flow end-to-end

```
   user / dashboard / CLI
    │  PATCH /v1/workspaces/{ws}/zombies/{id}  body: { config_json }
    ▼
  zombied-api (PATCH handler)
    │
    ├─► [PG]    UPDATE core.zombies SET config_json=$1, updated_at=now()
    ├─► [Redis] XADD zombie:control * type=config_changed zombie_id=Z
    └─► 200 OK    (revision = the new updated_at, an i64 epoch_ms)

  zombied-worker watcher  ── XREADGROUP zombie:control ─►  type=config_changed
    │
    └─► per-zombie session reload_pending.store(true, .release)
        XACK zombie:control

  zombied-worker per-zombie thread  (next loop iteration)
    │  reload_pending.swap(false, .acq_rel) == true
    ├─► reloadZombieConfig(alloc, session, pool)
    │     │
    │     ├─► [PG] SELECT config_json FROM core.zombies WHERE id=Z
    │     └─► session.config := parsed
    │
    └─► system_events.emitConfigUpdated(alloc, session, cfg)
          │
          ├─► [PG] SELECT updated_at FROM core.zombies WHERE id=Z   → revision
          ├─► [PG] INSERT core.zombie_events
          │         (zombie_id=Z, event_id='cfg-{revision}',
          │          actor='system:config_update', event_type='system',
          │          status='processed', request_json={revision},
          │          response_text=<summary>)
          │         ON CONFLICT (zombie_id, event_id) DO NOTHING RETURNING 1
          │
          └─► (only if RETURNING produced a row)
              [Redis] PUBLISH zombie:Z:activity {kind:'config_updated',
                                                  revision, summary}
```

The `RETURNING 1`-gated PUBLISH is the load-bearing detail. Reload can re-fire — `reload_pending` is a coalescing atomic, but multiple `config_changed` signals can land before the per-zombie thread services any of them, and the watcher's reconcile sweep emits another `config_changed` on restart. The deterministic `event_id` makes those retries idempotent at the durable layer; the gated PUBLISH keeps them idempotent at the live-tail layer too. Subscribers don't see ghost "Configuration updated" toasts.

### Why this isn't a fourth trigger type

Webhook, cron, steer, and continuation are *triggers*: external or user-initiated signals that produce work the zombie reasons over. They land on `zombie:{id}:events`, get XREADGROUPed, become an executor stage, and end with an XACK. They consume tokens; they pay rent in `zombie_execution_telemetry`.

A `system:*` event is the opposite — it is the worker telling the user "something I had to apply just got applied." It belongs in `core.zombie_events` for audit (`zombiectl events {id} --actor=system` shows the operational timeline alongside the work timeline) and on the activity channel for live UIs — but it is not work, so it never enters the worker's reasoning path. The `type: api` catch-all reserved in [`user_flow.md` §8.3](./user_flow.md#83-triggering-the-zombie) is the same shape as webhook/cron/steer (work to reason over). The `type: system` ack is a distinct kind: the worker's own diagnostic surface, not an ingress.

## Why a single zombie:control instead of per-tenant control

Considered alternatives:
- Per-tenant control (`zombie:control:{workspace_id}`): would force every worker to XREADGROUP on N streams. Discovery problem (which tenants exist?). High-cardinality BLOCK polling with no traffic.
- Per-zombie control: collapses control plane into data plane — no longer a control plane.
- Single `zombie:control` ✓: one XREADGROUP per worker, exactly-once delivery via consumer group, payload carries `workspace_id` + `zombie_id`. Multi-tenancy is encoded in the message body, not the stream key. Tenant boundary is enforced at the PG layer (RLS on `core.zombies`) — Redis stays fleet-wide.

## End-to-end sequence

### A. INSTALL  (`zombiectl install --from <path>`)

```
   user / install-skill
    │  POST /v1/workspaces/{ws}/zombies
    │  body: { name, config_json, source_markdown }
    ▼
  zombied-api (innerCreateZombie)
    │
    ├─► [PG]    INSERT core.zombies          (RLS: tenant boundary)
    ├─► [PG]    INSERT core.zombie_sessions  (idle row: execution_id=NULL,
    │                                         context_json={}, checkpoint_at=now)
    ├─► [Redis] XGROUP CREATE MKSTREAM zombie:{id}:events zombie_workers 0
    ├─► [Redis] XADD zombie:control * type=zombie_created
    │                                  zombie_id={id} workspace_id={ws}
    └─► 201 to user  (invariant 1: data stream + group exist before 201)

  zombied-worker:watcher  (any replica, exactly-once via zombie_workers)
    │  XREADGROUP zombie_workers <consumer> COUNT 16 BLOCK 5000
    │             STREAMS zombie:control >
    │
    ├─► SELECT core.zombies + core.zombie_sessions  (config + resume cursor)
    ├─► spawn per-zombie thread on this worker
    │    └─► thread XREADGROUPs zombie:{id}:events
    │        with consumer name worker-{pid}:zombie-{id}
    └─► XACK zombie:control

   ≤1s end-to-end from 201 to thread-ready. No worker restart.

   At rest:
     PG:    core.zombies row, core.zombie_sessions idle row.
            No core.zombie_events. No zombie_execution_telemetry.
     Redis: stream zombie:{id}:events with group zombie_workers (empty).
            Channel zombie:{id}:activity does not yet exist (implicit on
            first PUBLISH).
     Worker: one thread per replica blocked on XREADGROUP.
```

### B. TRIGGER  (steer / webhook / cron — three callers, ONE ingress)

```
   Common envelope (every XADD on zombie:{id}:events carries these
   five fields; the stream entry id IS the canonical event_id —
   never carry a separate id in the payload):

       actor         steer:<user> | webhook:<source> | cron:<schedule>
                     | continuation:<original_actor>
       type          chat | webhook | cron | continuation
       workspace_id  <uuid>
       request       <opaque JSON — the message + metadata>
       created_at    <epoch milliseconds; project bigint convention>

   STEER     zombiectl steer <zombie_id> "morning health check"
               → POST /v1/.../zombies/{id}/messages
               → XADD zombie:{id}:events *
                      actor=steer:kishore  type=chat
                      workspace_id=<ws>    request=<msg>
                      created_at=<ms>
               → 202 { event_id }                ← CLI uses event_id
                                                   to filter SSE frames

   WEBHOOK   GH Actions posts workflow_run failure
               → POST /v1/webhooks/{zombie_id}/github   (HMAC-SHA256
                 verified against workspace credential
                 `github`.webhook_secret)
               → XADD zombie:{id}:events *
                      actor=webhook:github  type=webhook
                      workspace_id=<ws>     request=<normalized-json>
                      created_at=<ms>
               → 202

               Receiver path carries a static `/{source}` suffix per provider
               (`/github`, `/linear`, `/jira`, `/grafana`, `/agentmail`,
               `/svix/{zombie_id}` for Clerk). M28's signature middleware
               dispatches on the path, not on runtime config.

               A zombie's TRIGGER.md declares `triggers: [...]` as an array
               (length 1–8, unique on `(type, source)` tuple). Each webhook
               entry carries `events: [...]` — the provider-specific
               subscription list the install-skill forwards to the provider
               (e.g. `gh api repos/.../hooks --field 'events[]=workflow_run'`).
               The receiver itself doesn't read `events[]`; M43's normaliser
               filters server-side on the event's own semantics
               (`conclusion=failure` for `workflow_run`). Declaration shape
               and wire-side filtering are separate concerns.

               The internal Clerk endpoint that bootstraps our own tenants
               on `user.created` is NOT this surface — it lives in the auth
               plane at `POST /v1/auth/identity-events/clerk` (M68). The
               `/v1/webhooks/` namespace is customer-data-plane only.

   CRON      NullClaw cron-tool fires on schedule (in-executor)
               → XADD zombie:{id}:events *
                      actor=cron:0_*/30_*_*_*  type=cron
                      workspace_id=<ws>        request=<msg>
                      created_at=<ms>

   CONTINUATION  worker re-enqueue (chunk-continuation or
                 user-resumed fulfillment)
               → XADD zombie:{id}:events *
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

   All four producers land the same envelope on the same stream. The
   reasoning loop never branches on actor — actor is metadata for the
   SKILL.md prose and the user's history filter.
```

**Webhook auth taxonomy.** The `webhook_sig` middleware classifies every
inbound rejection into one of three error codes, each with a distinct
user action:

- `UZ-WH-020 webhook_credential_not_configured` — the matching
  `triggers[].source` is unknown to the provider registry, OR the
  workspace has no `zombie:<source>` vault credential (vault row missing
  OR `webhook_secret` field absent). User-recoverable misconfig — fix
  with `zombiectl credential set <source> --data @-` and pipe JSON on stdin.
- `UZ-WH-010 invalid_signature` — provider + secret both configured but
  the request is unsigned, mis-signed, or the body was tampered with.
  Either an attack or a real drift between what the provider has
  registered vs the workspace vault — investigate.
- `UZ-WH-011 stale_timestamp` — Slack-style schemes only, request
  timestamp outside the 5-minute drift window. Clock skew or replay.

There is no Bearer fallback. The `Authorization` header is never
consulted on `/v1/webhooks/…` routes. See `docs/AUTH.md §Webhook auth
(separate surface)` for the full surface.

### C. EXECUTE  (worker → executor → tables → activity → XACK)

```
   per-zombie thread (XREADGROUP-blocked on zombie:{id}:events)
    │  unblocks with new entry
    ▼
   processEvent(envelope):

     1. INSERT core.zombie_events                  ← narrative log opens
          (zombie_id, event_id, workspace_id, actor, event_type,
           status='received', request_json, created_at)
          ON CONFLICT (zombie_id, event_id) DO NOTHING   (idempotent
                                                          on XAUTOCLAIM)

     2. PUBLISH zombie:{id}:activity               ← live: ephemeral
          { kind:"event_received", event_id, actor }     pub/sub, no buffer

     3. Gates:  balance, approval.
          Blocked → UPDATE core.zombie_events SET status='gate_blocked',
                                                  failure_label=<gate>,
                                                  updated_at=now_ms
                    → PUBLISH zombie:{id}:activity
                        { kind:"event_complete", event_id,
                          status:"gate_blocked" }
                    → XACK zombie:{id}:events       ← row-terminal:
                      gate_blocked rows are NEVER reopened. When the
                      gate resolves, a fresh
                      XADD lands with actor=continuation:<original>,
                      type=continuation, resumes_event_id=<blocked>,
                      producing a NEW zombie_events row whose
                      lifecycle is independent. The original blocked
                      row stays as the historical record.

     4. resolveSecretsMap from vault (per-zombie tool credentials —
        github, fly, slack, etc. — keyed by name; workspace-scoped).
        Provider posture (platform or self-managed) is resolved separately
        through tenant_provider.resolveActiveProvider(tenant_id), which
        either synthesises the platform default (no row) or reads the
        tenant_providers row and follows credential_ref into the vault
        for the self-managed api_key. The api_key crosses this boundary in
        process memory only — it does NOT join secrets_map and is never
        substituted into a tool placeholder. See
        billing_and_provider_keys.md §8.2 for the full visibility boundary.

     5. UPSERT core.zombie_sessions                ← worker marks busy
          SET execution_id, execution_started_at = now()

     6. executor.createExecution(workspace_path, {
          network_policy, tools, secrets_map, context, model, provider_api_key })
            (RPC over Unix socket to zombied-executor)
          → returns execution_id

     7. executor.startStage(execution_id, message)
          │
          │  Executor RPC speaks rpc_version: 2 (HELLO handshake on
          │  socket connect; mismatch → executor.rpc_version_mismatch
          │  fast-fail, no v1 compat shim pre-v2.0.0).
          │
          │  Reply for StartStage is multiplexed over the same Unix
          │  socket: zero-or-more JSON-RPC Progress notifications
          │  followed by exactly ONE terminal result frame, all
          │  sharing the StartStage request id. The worker dispatches
          │  each progress frame to its on_progress handler before
          │  the next read; the handler PUBLISHes to the activity
          │  channel.
          │
          │  args_redacted is built INSIDE the executor before the
          │  frame leaves the RPC boundary: any byte range that came
          │  from a secrets_map[NAME][FIELD] substitution is replaced
          │  with ${secrets.NAME.FIELD} placeholder. Resolved secret
          │  bytes never appear on this RPC channel and therefore
          │  never reach the activity pub/sub.
          ▼
          on tool_call_started   → PUBLISH zombie:{id}:activity
                                     { kind:"tool_call_started",
                                       name, args_redacted }
          on agent_response_chunk → PUBLISH zombie:{id}:activity
                                     { kind:"chunk", text }
          on tool_call_progress  → PUBLISH zombie:{id}:activity
                                     { kind:"tool_call_progress",
                                       name, elapsed_ms }
                                   (~2s heartbeat for any tool call
                                    still in flight; absence past
                                    ~5s renders as "stuck" in the UI)
          on tool_call_completed → PUBLISH zombie:{id}:activity
                                     { kind:"tool_call_completed",
                                       name, ms }
          │
          └─ terminal: StageResult{ content, tokens, ttft_ms,
                                    wall_ms, exit_ok }

     8. UPDATE core.zombie_events                  ← narrative log closes
          SET status = exit_ok ? 'processed' : 'agent_error',
              response_text, completed_at = now()

     9. INSERT zombie_execution_telemetry          ← billing/latency,
          (event_id UNIQUE, token_count,             immutable, write-once
           time_to_first_token_ms, wall_seconds,
           plan_tier, credit_deducted_cents)

    10. UPSERT core.zombie_sessions                ← idle bookmark
          SET context_json = { last_event_id, last_response },
              execution_id = NULL, checkpoint_at = now()

    11. PUBLISH zombie:{id}:activity               ← live: terminal frame
          { kind:"event_complete", event_id, status }

    12. XACK zombie:{id}:events                    ← consumer group
                                                     cursor advances

   Crash mid-event → worker restarts. If an XAUTOCLAIM sweep is present,
   the pending entry is handed to a
   new consumer name (the same worker process post-restart, with a
   new pid → new consumer name worker-{newpid}:zombie-{id}) inside
   zombie_workers. Step 1's ON CONFLICT (zombie_id, event_id) DO NOTHING
   and the UNIQUE event_id on zombie_execution_telemetry guarantee
   the replay is safe — exactly one zombie_events row, exactly one
   telemetry row — regardless of how many redelivery attempts occur.
   The write path is replay-safe even when reclaim is added later.
```

### D. WATCH  (user-side: how the live tail surfaces)

```
   CLI       zombiectl steer <zombie_id> "<message>"   (batch mode)
               → opens GET /v1/.../zombies/{id}/events/stream (SSE)
               → server SUBSCRIBE zombie:{id}:activity on a dedicated
                 Redis connection held outside the request-handler pool
                 (SUBSCRIBE blocks the conn).
               → forward each PUBLISH as an SSE frame, one per line:
                   id:<seq>\nevent:<kind>\ndata:<json>\n\n
               → on disconnect: UNSUBSCRIBE, close.

   UI        Dashboard /zombies/{id}/live
               → same GET /events/stream SSE consumer.
               → on page load also fetches GET /events?limit=20 for
                 recent history context.

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
   Clients backfill via GET /events?cursor=<last_seen_event_id>&limit=20
   after reconnect; the new SSE then resumes from sequence 0.

   HISTORY   zombiectl events {id} [--actor=…] [--since=2h]
             Dashboard /zombies/{id}/events
               → reads core.zombie_events (cursor-paginated).

   STATUS    zombiectl status {id}
               → reads core.zombie_sessions
                 ("busy or idle, last response").

   If a live frame drops (slow consumer, network blip), the user pulls
   the gap from GET /events. Live tail is best-effort by design; the
   durable record is core.zombie_events.
```

### KILL

```
   user
    │  POST /v1/.../zombies/{id}/kill
    ▼
  zombied-api
    ├─► UPDATE core.zombies SET status='killed' (PG)
    ├─► XADD zombie:control * type=zombie_status_changed
    │                              zombie_id={id} status=killed
    └─► 202 to user

  zombied-worker:watcher
    ├─► XREADGROUP picks up the control message
    ├─► cancel_flag_map[zombie_id].store(true, .release)
    ├─► executor_client.cancelExecution(execution_id)  [if mid-tool-call]
    └─► XACK

  per-zombie thread (top of loop)
    ├─► cancel_flag.load(.acquire) → true
    ├─► WorkerState.endEvent() if mid-event
    └─► break, thread exits

   ≤200ms end-to-end from 202 to thread-exit
```

## Multi-tenancy boundary

| Layer | Tenant isolation mechanism |
|---|---|
| PG (`core.zombies`, `core.zombie_events`, etc.) | Row-Level Security by `workspace_id`. API enforces via `app.workspace_id` session var; worker uses service role with explicit WHERE filtering. |
| Redis data plane (`zombie:{id}:events`) | Key namespaced by zombie UUID (globally unique); no cross-tenant collision possible. No RLS in Redis — protected by `zombie_id` being unguessable + API gatekeeping. |
| Redis control plane (`zombie:control`) | Fleet-wide, not tenant-scoped. Workers are tenant-blind by design (one fleet serves all tenants). Message payload carries `workspace_id` for logging + downstream PG lookups; routing uses `zombie_id`. |
| Worker process | Per-zombie thread maintains its own consumer name `worker-{pid}:zombie-{id}` on the data stream. Different zombies' events never cross threads. |
| Executor | Per-execution session — secrets resolved at `createExecution` boundary, never flow as raw strings into agent context. |

## Why one worker = all events for that zombie

Concern: if multiple workers are members of `zombie_workers`, won't `zombie:{id}:events` round-robin events across workers and break per-zombie state continuity?

No — consumer groups distribute messages across consumers that are actively reading the stream. Only the worker that won the control message spawns the per-zombie thread; only that thread reads `zombie:{id}:events`. Other workers never XREADGROUP that stream → no round-robin → all events flow to the right thread.

Failure mode: if the worker hosting zombie X crashes, no other worker is reading `zombie:{id}:events` until reclaim logic or failover takes over. Recovery needs a heartbeat or XAUTOCLAIM sweep. Multi-replica high availability remains a later concern, so this file treats reclaim as an architectural requirement even where rollout details may still evolve.

## What the user's agent never does

- Never sees the zombie's LLM tokens or reasoning state
- Never holds the zombie's credentials in its own context
- Never executes the zombie's tool calls in its own session
- Never persists across the user's laptop being closed

## What the zombie's agent never does

- Never touches the user's laptop directly
- Never reads the user's local filesystem (it sees only what the SKILL.md and TRIGGER.md grant it)
- Never escapes the sandbox — Landlock + cgroups + bwrap enforce egress, fs, and process limits

## The install failure scenario, visually

The API server (not the worker) is the side that writes to Redis during install. The worker only reads from `zombie:control`. So a Redis blip during install hits the API → Redis hop, not worker → Redis. The API has two layers of defence, and the watcher's reconcile sweep is the third.

**Defence-in-depth, in order of how the system tries to keep `core.zombies` and Redis consistent:**

1. **Inline retry (API).** `publishInstallSignals` retries `XGROUP CREATE` + `XADD zombie:control` on a fixed backoff `[100ms, 500ms, 1500ms]` — four attempts, ~2.1s total wall budget. Most blips never escape this loop.
2. **PG rollback (API).** If retries exhaust, the handler `DELETE`s the freshly-inserted `core.zombies` row and returns 500 with `hint=rolling_back_pg_row` so the caller can retry cleanly. No orphan.
3. **Reconcile sweep (worker watcher).** If both publish AND rollback fail (rare double-failure, logged with `hint=row_orphaned_reconcile_will_heal`), the watcher's reconcile loop runs every ~30s (6 ticks × 5s), walks `core.zombies` for `status='active'` rows, and calls `spawnZombieThread` for each. Idempotent: `ensureZombieEventsGroup` treats `BUSYGROUP` as success, and `spawnZombieThread` no-ops if the zombie is already mapped.

Worker process restart is the same machinery — boot calls `listActiveZombieIds(pool)` and runs the same `spawnZombieThread` per id — but the periodic sweep means orphans don't have to wait for a restart.

**The rare double-failure path** (publish exhausts retries AND rollback also fails) is what motivates layer 3:

```
   TIME ──►
   t=0  USER → zombiectl install → API
   t=2  API: INSERT core.zombies (status='active') → PG ✓
   t=3  API: XGROUP CREATE + XADD zombie:control ╳ (4 retries exhausted, ~2.1s)
   t=4  API: DELETE core.zombies row ╳ (rare second failure)
   t=5  API: 500 → user. Logs: zombie.create_publish_failed,
                              zombie.create_rollback_failed,
                              hint=row_orphaned_reconcile_will_heal

   ── ORPHAN WINDOW (≤ ~30s) ──
      PG row Z = active; Redis stream + group + control signal all missing.
      Other zombies unaffected. Webhooks arriving for Z create the stream
      without a group; events accumulate (bounded by Redis retention).

   ── RECONCILE TICK (~30s cadence; same code path as worker boot) ──
   t=N  watcher.listActiveZombieIds(pool) → [Z, …]
        watcher.spawnZombieThread(Z):
          ensureZombieEventsGroup (BUSYGROUP-as-success — idempotent)
          install ZombieRuntime + Thread.spawn
        Zombie thread XREADGROUPs zombie:Z:events from id 0,
        processes any backlog webhooks in order. Z is now healthy.
```

**Variant where only `XADD zombie:control` fails after `XGROUP CREATE` succeeded** — same picture; reconcile finds Z in PG, `ensureZombieEventsGroup` no-ops via BUSYGROUP-as-success, thread spawns.

The 85-line full-fidelity walkthrough that used to live here was moved out — agents implementing recovery should grep `publishInstallSignals` / `spawnZombieThread` / `listActiveZombieIds` in `src/` for the live behavior; this section is the architectural narrative, not a runbook.

---

## Notable invariants this flow proves

- **No race on stream / group creation.** `innerCreateZombie` does INSERT + `XGROUP CREATE` + `XADD zombie:control` synchronously before returning 201. Any webhook arriving within microseconds of the 201 finds the stream already there.
- **All triggers funnel into one reasoning loop.** Webhook, cron, steer, and continuation are different *producers* into `zombie:{id}:events`; the worker's per-zombie thread doesn't branch on actor type.
- **Credentials never enter agent context.** Substitution happens at the tool bridge, inside the executor, after sandbox entry. The agent sees `${secrets.fly.api_token}`; HTTPS request headers get real bytes; responses never echo the token.
- **Kill is immediate for in-flight runs.** A control-stream `XADD` triggers `cancelExecution` within milliseconds — not on the 5-second `XREADGROUP` cycle.
- **Stuck zombies self-heal.** A per-zombie thread that exits early (Redis connect, Postgres claim, missing executor) flips `runtime.exited` via the watcher-owned wrapper. The next spawn (driven by another `zombie_created` retry, or by the watcher's ~30-second reconcile sweep) reaps the entry and re-spawns.
- **Long-running stages don't crash the model.** The three context-lifecycle layers (see [`capabilities.md`](./capabilities.md) §4) keep context bounded. If a single incident exceeds budget, the zombie chunks and continues in a new stage from a `memory_recall` snapshot.
