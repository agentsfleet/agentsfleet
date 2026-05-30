# Data Flow — how an event moves through the system

> Parent: [`README.md`](./README.md) · Sibling: [`runner_fleet.md`](./runner_fleet.md) (the structural split this flow runs on).
>
> **Scope:** this file describes the runtime as it runs now — after the M80_002 cutover. `zombied` is the **control plane** (owns Postgres, Redis, the Vault, the HTTP API, and work assignment); the host-resident **`zombie-runner`** daemon is the **execution plane** (leases work over Hypertext Transfer Protocol Secure (HTTPS), runs NullClaw in a forked sandboxed child, reports back). The single-process `zombied worker` loop and the `zombied-executor` sidecar are deleted. See [`runner_fleet.md`](./runner_fleet.md) for the why and the guarantees.

Read this when you need to know where a webhook, a steer, or a cron fire ends up. Many specs reference this file as the canonical picture of the runtime.

## Process and stream ownership at a glance

| Process | Role |
|---|---|
| **`zombied-api`** (`zombied serve`) | The control plane. HTTP routes for the user surface **and** the `/v1/runners` machine surface. Owns Postgres, the Redis pool, and the Vault. Steer, webhook, cron, and continuation handlers all `XADD` directly to `zombie:{id}:events` — single ingress. On `lease` it does a non-blocking `XREADGROUP` to claim the next event, runs the gates + billing + secret resolution, and issues a `fleet.runner_leases` row; on `report` it persists the terminal state and `XACK`s. It is the sole `PUBLISH`er on `zombie:{id}:activity`. Never runs language-model code. |
| **`zombie-runner`** (host-resident daemon) | The execution plane. Boots from an operator-installed `zrn_` token (env `ZOMBIE_RUNNER_TOKEN`, no self-register — Option B), then loops `heartbeat → lease → execute → report → activity` over HTTPS carrying that `zrn_` token. Holds **zero datastore credentials**. Per lease it forks a sandboxed child (Landlock + cgroups + network namespace via bwrap) that runs the NullClaw agent; credential substitution happens at the tool bridge inside that child. Frames stream back to the parent over a stdout pipe and are forwarded to `zombied` over the `activity` verb. |

| Target | Producer | Consumer |
|---|---|---|
| `zombie:{id}:events` | `zombied-api` on steer / webhook / continuation; NullClaw cron-tool fires; `zombied` on chunk-continuation | **`zombied`** — non-blocking `XREADGROUP` on each `lease`, `XACK` on each `report` |
| `zombie:{id}:activity` | `zombied` (sole publisher) — bracket frames directly, mid-run frames fed by the runner's `activity` stream | Server-Sent-Events handler in `zombied-api` on a dedicated Redis connection |
| `core.zombie_events` | `zombied` lease path (INSERT received) → report path (UPDATE terminal) | `zombied-api` `GET /events` endpoints, dashboard, `zombiectl events` |
| `core.zombies` | `zombied-api` only | `zombied` at lease (config resolved fresh per lease) |
| `core.zombie_sessions` | `zombied` lease path (mark busy) + report path (checkpoint) | `zombied` at lease + `zombiectl status` |
| `fleet.runner_leases` / `fleet.runner_affinity` | `zombied` lease path (issue) + report/reclaim (flip / release) | `zombied` assignment + fencing + reclaim |
| `vault.secrets` | `zombied-api` on `credential set` (upsert) | `zombied` resolves just-in-time at `lease`, ships inline in the lease reply |
| `zombie:control` | — (removed at the cutover) | — (removed at the cutover) |

---

## The two agents in play

Two distinct agents are in play. Keeping them straight is essential to understanding the architecture:

```
┌────────────────────────────────┐         ┌──────────────────────────────┐
│  USER'S AGENT (laptop)         │         │  ZOMBIE'S AGENT (host)        │
│                                │         │                              │
│  Claude Code / Amp / Codex /   │         │  NullClaw running inside the │
│  OpenCode driving zombiectl    │         │  zombie-runner's sandboxed   │
│                                │         │  child (Landlock + cgroups + │
│  This is what the human types  │         │  netns via bwrap; durable,   │
│  into. Ephemeral.              │         │  persists across laptop close)│
└────────────────────────────────┘         └──────────────────────────────┘
```

The user's agent is a workstation tool driving `zombiectl`. The zombie's agent is a NullClaw instance inside the runner's sandboxed child. The user's agent never becomes the zombie's agent and never sees its tokens — they communicate only through the steer endpoint, the event stream, and the events history.

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
           ║       workspace_id=<uuid>          ║
           ║       request=<msg-json>           ║
           ║       created_at=<epoch_ms>        ║
           ║  → 202 { event_id }                ║
           ╚═══════════════════════════════════╝
                          ↓
        ( the event waits on the stream until a runner asks for work )
                          ↓
           ╔═══════════════════════════════════╗
           ║  zombie-runner (host)             ║
           ║  POST /v1/runners/me/leases        ║   ← long-poll; no work
           ║  Authorization: Bearer zrn_        ║     → null + retry_after_ms
           ╚═══════════════════════════════════╝
                          ↓
           ╔═══════════════════════════════════╗
           ║  zombied (lease handler)          ║   ← the work the worker
           ║  ───────────────────────────────  ║     used to do, now on
           ║  assign.select():                  ║     the request thread:
           ║   non-blocking XREADGROUP across   ║
           ║   active zombies (sticky pref) →   ║   ← narrative log opens
           ║   claim fleet.runner_affinity,     ║     (mutable)
           ║   issue monotonic fencing_token    ║
           ║  1. INSERT core.zombie_events      ║   ← live: pub/sub frame
           ║     (status='received')            ║     (ephemeral, no ACK)
           ║  2. PUBLISH zombie:{id}:activity   ║
           ║     {kind:"event_received"}        ║   See
           ║  3. balance gate, receive debit,   ║   [`capabilities.md`](./capabilities.md)
           ║     approval gate, stage debit     ║   for each gate layer.
           ║  4. resolve secrets_map from vault ║
           ║  5. UPSERT core.zombie_sessions    ║   ← resume cursor:
           ║     SET execution_id (busy)        ║     marks zombie busy
           ║  6. issue fleet.runner_leases row  ║
           ║     (lease_expires_at, fencing)    ║
           ║  → 200 { event, ExecutionPolicy,   ║
           ║         secrets_map, lease_id,     ║
           ║         fencing_token }            ║
           ╚═══════════════════════════════════╝
                          ↓
           ╔═══════════════════════════════════╗
           ║  zombie-runner (parent + child)   ║
           ║  ───────────────────────────────  ║
           ║  parent: establish cgroup, fork,   ║       This is the
           ║  exec self as `__execute` under    ║       "ZOMBIE'S AGENT".
           ║  bwrap, feed the lease via stdin   ║       An LLM in a
           ║                                    ║       sandbox; the user's
           ║  sandboxed child:                  ║       agent never becomes
           ║   apply mandatory Landlock,        ║       it, never sees its
           ║   run NullClaw over the policy.    ║       tokens or context.
           ║   Each tool call → tool bridge     ║
           ║   substitutes ${secrets.NAME.x}    ║
           ║   inside the sandbox, then the     ║
           ║   HTTPS request fires.             ║
           ║                                    ║
           ║   Each progress frame → stdout pipe ║   ← parent forwards
           ║   (A=activity, R=result, framed):  ║     each A frame to
           ║     - tool_call_started            ║     zombied .../activity,
           ║     - agent_response_chunk         ║     which PUBLISHes it.
           ║     - tool_call_completed          ║
           ║                                    ║
           ║   Child returns ExecutionResult.   ║
           ║  → {content, tokens, ttft_ms,      ║
           ║     wall_ms, outcome}              ║
           ╚═══════════════════════════════════╝
                          ↓
           ╔═══════════════════════════════════╗
           ║  zombied (report handler)         ║
           ║  POST /v1/runners/me/reports       ║
           ║  ───────────────────────────────  ║
           ║   claimReport(): atomic CAS —      ║   ← fence + flip + dedup
           ║     UPDATE runner_leases           ║     in one statement
           ║     SET status=reported            ║     (stale token → reject
           ║     FROM runner_affinity           ║      UZ-RUN-005)
           ║     WHERE status=active AND        ║
           ║       fencing_token >= fencing_seq ║
           ║   7. UPDATE core.zombie_events     ║   ← narrative log closes
           ║      status='processed'            ║     (same row)
           ║      response_text=<content>       ║
           ║   8. PUBLISH zombie:{id}:activity  ║   ← live: terminal frame
           ║      {kind:"event_complete"}       ║
           ║   9. INSERT zombie_execution_      ║   ← billing/latency
           ║      telemetry (reconcile actuals) ║     audit (UNIQUE event_id)
           ║  10. UPSERT core.zombie_sessions   ║   ← resume cursor:
           ║      context_json, execution_id=NULL║     clears handle,
           ║  11. XACK zombie:{id}:events       ║     advances bookmark
           ║  12. release affinity (token-guard)║
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

The 12 numbered writes are the same durable effects the deleted worker's `processEvent` produced, in the same order — split across two protocol calls (`lease` does 1–6, `report` does 7–12) instead of one in-process loop. The control-plane handlers under `src/zombied/fleet/` are faithful mirrors of the old `event_loop_writepath`; the row-equivalence guarantee (cutover Invariant 2) is what keeps history, billing, and the SSE tail byte-identical to the pre-cutover path.

## The three durable stores: who owns what

The flow above writes to three Postgres tables. They are **not** redundant — each answers a distinct user question, has a different cardinality, mutability, and retention contract. The cutover did not change their shape or their write order; it moved the writer from the per-zombie worker thread to `zombied`'s lease/report path.

| Table | Cardinality | Mutability | Answers |
|---|---|---|---|
| `core.zombie_sessions` | **One row per zombie** | UPSERT — mutated on every event boundary | "Where is this zombie *right now*? Is it idle or executing? What was its last successful response?" — the resume bookmark + active-execution handle. `execution_id` is set at `lease` (busy) and cleared at `report` (idle). Read at `lease` and by `zombiectl status`. |
| `core.zombie_events` | **One row per delivery** | INSERT (status=`received`) → UPDATE (status=`processed` \| `agent_error` \| `gate_blocked`) | "What did this zombie do for event X? Who triggered it, what did they ask, what did it answer, did the gates pass?" — the user's narrative log. The single source of truth for the Events tab and `zombiectl events`. |
| `zombie_execution_telemetry` | **Two rows per event** under the credit-pool model: one `charge_type='receive'` at the receive debit, one `charge_type='stage'` at the stage debit (then UPDATEd with token counts after the report). UNIQUE `(event_id, charge_type)`. | INSERT at each debit, immutable for the `credit_deducted_nanos` column; the stage row is reconciled once with actual token counts at report. | "How much did event X cost (split by receive vs stage)? How fast was it? What posture was charged?" — billing + latency audit. Joinable to `zombie_events` via `event_id`. |

Why two per-delivery tables (`events` + `telemetry`) instead of one? They have different write authorities and retention contracts:

- `zombie_events` holds user-readable strings (`request_json`, `response_text`) — large, mutable mid-lifecycle, deletable on tenant offboarding.
- `zombie_execution_telemetry` holds numeric audit columns — small, immutable once written, retained for billing reconciliation independent of whether the conversation row is purged.

The durable lease bookkeeping (`fleet.runner_leases`, `fleet.runner_affinity`) is a fourth concern — it is the *ownership* layer (which runner holds this event, at what fencing token, until when), not a user-facing record. It lives in the `fleet` schema and never carries user strings.

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

**Step 1 — INSERT `zombie_events`** (status=`received`, at `lease`):

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

The lease reply ships to the runner. NullClaw runs inside the runner's sandboxed child: fetches GH run logs via `${secrets.github.api_token}`, fetches Fly app logs, fetches Upstash Redis stats, posts a remediation message to Slack. The child returns `ExecutionResult{content, tokens=1840, wall_ms=8210, ttft_ms=320, outcome=ok}` over the stdout pipe; the runner POSTs it to `report`.

**Step 7 — UPDATE `zombie_events`** (close the same row, at `report`):

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

**Step 9 — INSERT `zombie_execution_telemetry`** (immutable audit row, joinable on `event_id`):

```
zombie_execution_telemetry  (stage row reconciled with actuals)
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

**Step 10 — UPSERT `zombie_sessions`** (advance bookmark, clear execution handle):

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

## Two streams + one pub/sub channel — and the one that retired

Before the cutover there were three Redis surfaces. The split kept two and retired one.

| Redis surface | Type | Cardinality | Purpose | Volume |
|---|---|---|---|---|
| `zombie:{id}:events` | Stream + consumer group `zombie_workers` | One per zombie | Single event ingress — steer / webhook / cron / continuation all `XADD` here. `zombied` is now the consumer: a **non-blocking** `XREADGROUP` on each `lease`, `XACK`ed at `report`. Idempotent on replay via `INSERT … ON CONFLICT DO NOTHING`. | High — every event the zombie handles. |
| `zombie:{id}:activity` | Pub/sub channel (no consumer group, no persistence) | One per zombie | Best-effort live tail — `zombied` `PUBLISH`es one frame per `event_received` / `tool_call_started` / `agent_response_chunk` / `tool_call_progress` / `tool_call_completed` / `event_complete`. The bracket frames originate in `zombied`; the mid-run frames are forwarded from the runner over the `activity` verb. SSE handler `SUBSCRIBE`s and forwards. No buffer, no ACK, no resume. | High during execution, zero when idle. |
| `zombie:control` | (removed) | — | **Removed at the cutover.** It existed to tell the worker watcher to spawn / cancel / reconfigure per-zombie threads — and there are no per-zombie threads anymore. The producer (`control_stream.publish` from the install / status / config handlers) and the dead `control_stream` module were deleted; the install path keeps only `redis_zombie.ensureZombieConsumerGroup` (load-bearing — the `lease` `XREADGROUP` needs the events group to exist). | gone |

`zombie:{id}:events` is durable (events appended, `XACK`ed entries pruned) and backs the at-least-once delivery contract. The pub/sub channel is ephemeral and exists only to power live user UIs — its loss never affects correctness, only what the user sees in real time. Durable activity history lives in `core.zombie_events`; the pub/sub channel is the eyeballs surface, not the audit surface.

## Connection topology — the cutover collapsed the dedicated tier

Before the cutover, the worker held **one dedicated blocking Redis connection per zombie** (`XREADGROUP … BLOCK 5000`) plus a watcher connection — that dedicated tier was the binding fleet constraint. The cutover **deleted that tier**. `zombied` now claims work with a **non-blocking** `XREADGROUP` on the request thread that serves a `lease` call — a short-lived pooled command, not a held connection. The runner's "blocking" is an HTTP long-poll against `zombied`, not a Redis `BLOCK`, and the runner holds no Redis at all.

```
                        REDIS CONNECTION TOPOLOGY (post-cutover)
                        ═════════════════════════════════════════

  ┌─────────────────────────────────────────────────────────────────────────────────┐
  │                        POOL  (max_idle=8, eager_min=2)                           │
  │              ──── short-lived request-path commands only ────                    │
  │                                                                                  │
  │   acquire → command → release    (microseconds to milliseconds per cycle)        │
  │                                                                                  │
  └──▲───────────────────────▲───────────────────────▲───────────────────▲───────────┘
     │ XADD                   │ XREADGROUP (no BLOCK)  │ PUBLISH            │ XACK
     │ zombie:{id}:events     │ zombie:{id}:events     │ zombie:{id}:       │ zombie:{id}:
     │ (steer/webhook/cron/   │ (on each lease)        │ activity           │ events
     │  continuation)         │                        │ (brackets +        │ (on report)
     │                        │                        │  forwarded frames) │
  ┌──┴─────────┐         ┌────┴────────┐          ┌─────┴───────┐      ┌─────┴──────┐
  │ HTTP user  │         │ lease       │          │ lease +     │      │ report     │
  │ handlers   │         │ handler     │          │ report +    │      │ handler    │
  │ (zombied-  │         │ (zombied-   │          │ activity    │      │ (zombied-  │
  │  api)      │         │  api)       │          │ (zombied-api)│      │  api)      │
  └────────────┘         └─────────────┘          └─────────────┘      └────────────┘


  ┌──────────────────────────────────────────────────────────────────────────────────┐
  │              DEDICATED CONNECTIONS  (NOT in the pool) — SSE only now              │
  │                    ──── long-lived blocking SUBSCRIBE ────                         │
  │                                                                                   │
  │   ┌──────────────────────┐       ┌──────────────────────┐                         │
  │   │ SUBSCRIBE            │       │ SUBSCRIBE            │  ...                    │
  │   │ zombie:Z1:activity   │       │ zombie:Z2:activity   │                         │
  │   │  (held while SSE     │       │  (held while SSE     │                         │
  │   │   client is open)    │       │   client is open)    │                         │
  │   └──────────────────────┘       └──────────────────────┘                         │
  │                                                                                   │
  │   The per-zombie XREADGROUP-BLOCK connection that used to live here is GONE.      │
  │   A dead runner is reclaimed by lease expiry + fencing_token, not consumer idle.  │
  └───────────────────────────────────────────────────────────────────────────────────┘
```

**The rule that survives.** A connection held across a Redis call that blocks the server (`SUBSCRIBE`) cannot return to a pool — its lifetime is tied to the consumer, not the request. The pool is reserved for commands that complete in milliseconds: `XADD`, the non-blocking `XREADGROUP`, `PUBLISH`, `XACK`. The SSE subscriber is the only remaining dedicated-connection consumer.

**What this changed at scale.** The pre-cutover idle cost was dominated by N blocking `XREADGROUP BLOCK 5000` loops iterating every five seconds; the fleet's Upstash bill scaled with `(zombies + workers)`, not throughput. After the cutover there are no idle blocking loops — the idle cost is driven by runner **lease poll frequency** (each idle `lease` does one non-blocking `XREADGROUP`), tunable by the runner's `retry_after_ms` backoff rather than a Redis `BLOCK` constant. [`scaling.md`](./scaling.md) re-derives the math.

## Config reload — pull-per-lease, no signal

`zombied` resolves a zombie's config fresh from `core.zombies` on every `lease`, so a `PATCH /v1/workspaces/{ws}/zombies/{id}` takes effect on the **next lease** with no signaling. There is no in-memory config cache to invalidate and no `zombie_config_changed` consumer to wait on — the worker's watcher-reload path and the `system:config_updated` synthetic-event acknowledgement that depended on it were deleted with the worker.

A config change never alters a language-model turn already in flight (one lease = one stage, and the stage already has its resolved policy); the next stage picks up the new config. The PATCH handler writes `core.zombies` and returns — there is no signal to emit, since the control stream was removed at the cutover. A status change (`paused` / `stopped` / `killed` / back to `active`) is read the same way: the lease assignment scan filters on `core.zombies.status = 'active'`, so a paused zombie drops out of the candidate set on the next scan and a resumed one re-enters — no notification needed.

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
    │           (ensureZombieConsumerGroup — the lease XREADGROUP needs this group)
    └─► 201 to user  (invariant: data stream + group exist before 201)

   No worker thread to spawn. The zombie is installable work the moment its
   events group exists; the first runner to lease it will claim it.

   At rest:
     PG:    core.zombies row, core.zombie_sessions idle row.
            No core.zombie_events. No zombie_execution_telemetry. No fleet.runner_leases.
     Redis: stream zombie:{id}:events with group zombie_workers (empty).
            Channel zombie:{id}:activity does not yet exist (implicit on first PUBLISH).
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
               `/svix/{zombie_id}` for Clerk). The signature middleware
               dispatches on the path, not on runtime config.

               A zombie's TRIGGER.md declares `triggers: [...]` as an array
               (length 1–8, unique on `(type, source)` tuple). Each webhook
               entry carries `events: [...]` — the provider-specific
               subscription list the install-skill forwards to the provider
               (e.g. `gh api repos/.../hooks --field 'events[]=workflow_run'`).
               The receiver itself doesn't read `events[]`; the normaliser
               filters server-side on the event's own semantics
               (`conclusion=failure` for `workflow_run`). Declaration shape
               and wire-side filtering are separate concerns.

               The internal Clerk endpoint that bootstraps our own tenants
               on `user.created` is NOT this surface — it lives in the auth
               plane at `POST /v1/auth/identity-events/clerk`. The
               `/v1/webhooks/` namespace is customer-data-plane only.

   CRON      NullClaw cron-tool fires on schedule (in the sandboxed child)
               → the runner reports a cron-scheduling intent; zombied
               → XADD zombie:{id}:events *
                      actor=cron:0_*/30_*_*_*  type=cron
                      workspace_id=<ws>        request=<msg>
                      created_at=<ms>

   CONTINUATION  zombied re-enqueue (chunk-continuation or
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

### C. EXECUTE  (lease → runner → report)

The deleted worker's single in-process `processEvent` loop is now split across two protocol calls. `lease` does the pre-execution control-plane work and hands a self-contained `ExecutionPolicy` to the runner; `report` does the terminal control-plane work after the runner's sandboxed child finishes.

```
   zombie-runner (host)
    │  POST /v1/runners/me/leases   (long-poll; Bearer zrn_)
    ▼
   zombied — lease handler:

     assign.select():
       non-blocking XREADGROUP zombie:{id}:events across all ACTIVE
       zombies, sticky-ordered by last_runner_id; claim the per-zombie
       fleet.runner_affinity slot (wins iff free or prior lease expired)
       and bump the monotonic fencing_seq. A lease past lease_expires_at
       is RECLAIMED: its event envelope + billing are reused, re-fenced
       with a higher token.

     1. INSERT core.zombie_events                  ← narrative log opens
          (status='received', actor, request_json)
          ON CONFLICT (zombie_id, event_id) DO NOTHING   (idempotent on replay)
     2. PUBLISH zombie:{id}:activity { kind:"event_received", event_id, actor }
     3. Gates + billing (mirror of metering.zig):
          balance gate → receive debit → approval gate → stage debit.
          Blocked → UPDATE core.zombie_events status='gate_blocked',
                                              failure_label=<gate>
                    → PUBLISH zombie:{id}:activity
                        { kind:"event_complete", status:"gate_blocked" }
                    → XACK zombie:{id}:events       ← row-terminal:
                      gate_blocked rows are NEVER reopened. When the gate
                      resolves, a fresh XADD lands with
                      actor=continuation:<original>, producing a NEW row.
     4. resolveSecretsMap from vault (per-zombie tool credentials,
        workspace-scoped). The provider api_key is resolved separately
        (resolveActiveProvider, fresh + reclaim) and delivered on the lease via
        ExecutionPolicy.provider + ExecutionPolicy.api_key; it does NOT join
        secrets_map and is never substituted into a tool placeholder. The
        runner injects it into the NullClaw child for the inference call only,
        and zombied secureZeros it after the lease serializes.
     5. UPSERT core.zombie_sessions                ← marks busy
          SET execution_id, execution_started_at = now()
     6. issue fleet.runner_leases row              ← durable ownership
          (lease_id, fencing_token, lease_expires_at = now + LEASE_TTL_MS)
     → 200 { event, ExecutionPolicy(config + secrets_map + network_policy
              + tool_allowlist + provider + api_key), lease_id,
              fencing_token, checkpoint? }

   zombie-runner — parent (child_supervisor.zig):
       establish cgroup → fork → exec self as `zombie-runner __execute`
       under bwrap (unshare-all + ro-system + rw-workspace + die-with-parent)
       → feed the lease over child stdin (VLT: secrets only via stdin)
       → read framed frames off child stdout under the lease deadline (poll)

   zombie-runner — sandboxed child (child_exec.zig):
       apply mandatory Landlock (fail-closed on the required tier) →
       build NullClaw config + tool set from the policy → run the agent turn.

          args_redacted is built INSIDE the child before any frame leaves:
          any byte range from a secrets_map[NAME][FIELD] substitution is
          replaced with the ${secrets.NAME.FIELD} placeholder. Resolved
          secret bytes never appear on the pipe and never reach activity.

          on tool_call_started   → A frame → parent → POST .../activity
          on agent_response_chunk → A frame → parent → POST .../activity
          on tool_call_progress  → A frame → parent → POST .../activity
                                   (long-tool heartbeat; absence past ~5s
                                    renders as "stuck" in the UI)
          on tool_call_completed → A frame → parent → POST .../activity
          │
          └─ terminal: R frame ExecutionResult{ content, tokens, ttft_ms,
                                                wall_ms, outcome }

   zombie-runner — parent:
       collect the ExecutionResult, classify timeout/OOM/crash/startup_posture,
       scope.destroy() (idempotent), then:
    │  POST /v1/runners/me/reports { lease_id, fencing_token, outcome, ... }
    ▼
   zombied — report handler:

     claimReport(): atomic CAS —
       UPDATE fleet.runner_leases SET status=reported
       FROM fleet.runner_affinity
       WHERE status='active' AND fencing_token >= fencing_seq
       RETURNING <lease fields>
       (fence + flip + dedup in one statement; a stale/reclaimed holder is
        rejected with UZ-RUN-005 and mutates nothing)

     7. UPDATE core.zombie_events                  ← narrative log closes
          SET status = outcome==ok ? 'processed' : 'agent_error',
              response_text, completed_at = now()
     8. PUBLISH zombie:{id}:activity { kind:"event_complete", event_id, status }
     9. INSERT/reconcile zombie_execution_telemetry ← billing/latency,
          (event_id UNIQUE, token_count, ttft_ms, wall_seconds, ...)
    10. UPSERT core.zombie_sessions                ← idle bookmark
          SET context_json = { last_event_id, last_response },
              execution_id = NULL, checkpoint_at = now()
    11. XACK zombie:{id}:events                    ← consumer cursor advances
    12. release affinity (WHERE fencing_seq = $token)  ← token-guarded

   Runner dies mid-event → its lease expires at lease_expires_at; the next
   lease's reclaim path re-issues the event to another runner with a higher
   fencing_token. Step 1's ON CONFLICT and the UNIQUE telemetry event_id keep
   the replay safe — exactly one zombie_events row, exactly one telemetry row,
   regardless of how many redelivery attempts occur. A late report from the
   dead runner is fenced out at claimReport (UZ-RUN-005).
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
  zombied
    ├─► UPDATE core.zombies SET status='killed' (PG)
    ├─► mark the in-flight fleet.runner_leases row revoked
    └─► 202 to user

  zombie-runner  (next heartbeat)
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
| PG (`core.zombies`, `core.zombie_events`, etc.) | Row-Level Security by `workspace_id`. The API enforces via `app.workspace_id` session var; the control-plane lease/report path uses the service role with explicit WHERE filtering. |
| Redis data plane (`zombie:{id}:events`) | Key namespaced by zombie UUID (globally unique); no cross-tenant collision possible. No RLS in Redis — protected by `zombie_id` being unguessable + API gatekeeping. |
| Runner ↔ control plane | The `zrn_` token authenticates the runner per call; `me` resolves from the token. The lease carries exactly one zombie's event + scoped secrets; a runner never sees another tenant's data plane. Enrollment is gated on the `platform_admin` claim (M80_005) — only usezombie's operator may add a host to the shared fleet. Trust-gated placement (don't put other-tenant work on a weak sandbox tier) is operator-assigned, deferred to M80_007. |
| Sandboxed child | Per-execution: secrets resolved at the lease, delivered via the child's stdin only, substituted at the tool bridge inside the sandbox, never flowing as raw strings into agent context. |

## One active lease per zombie — the ownership model

Before the cutover, a single worker thread owned all events for a zombie, and the concern was round-robin across worker replicas breaking per-zombie continuity. That model is gone. Ownership is now a **durable lease**, not a thread:

- `fleet.runner_affinity` holds one slot per zombie. `assign.select` claims it atomically — a runner wins iff the slot is free or the prior lease has expired — and bumps a monotonic `fencing_seq`. So **at most one lease is active per zombie at any time**, regardless of how many runners poll concurrently.
- A runner that loses the race for a zombie simply gets no lease for it and tries the next eligible zombie (or backs off).
- Continuity across stages is the checkpoint in `zombied`, not runner-local state — so any runner can pick up the next stage. Sticky routing (prefer `last_runner_id`) is a hint for warm-sandbox reuse, never ownership.

Failure mode: if the runner holding a lease dies, no other runner can claim that zombie until `lease_expires_at`; the reclaim sweep then re-leases it with a higher fencing token. Recovery latency is bounded by the TTL (Time To Live) plus poll density — the S0 lazy-reclaim SLA. Tightening it (heartbeat-driven reassignment, sub-10 s recovery) is M80_006.

## What the user's agent never does

- Never sees the zombie's LLM tokens or reasoning state
- Never holds the zombie's credentials in its own context
- Never executes the zombie's tool calls in its own session
- Never persists across the user's laptop being closed

## What the zombie's agent never does

- Never touches the user's laptop directly
- Never reads the user's local filesystem (it sees only what the SKILL.md and TRIGGER.md grant it)
- Never escapes the sandbox — Landlock + cgroups + a network namespace (via bwrap) enforce egress, filesystem, and process limits inside the runner's child
- Never holds a datastore credential — the runner reaches the platform only over the `/v1/runners` protocol

## The install failure scenario, visually

The API server (not a runner) is the side that writes to Redis during install. So a Redis blip during install hits the API → Redis hop. The API has two layers of defence:

1. **Inline retry (API).** `ensureEventStream` retries `XGROUP CREATE MKSTREAM zombie:{id}:events` on a fixed backoff `[100ms, 500ms, 1500ms]` — four attempts, ~2.1s total wall budget. Most blips never escape this loop. (The group is load-bearing — the `lease` `XREADGROUP` needs it.)
2. **PG rollback (API).** If retries exhaust, the handler `DELETE`s the freshly-inserted `core.zombies` row and returns 500 with `hint=rolling_back_pg_row` so the caller can retry cleanly. No orphan.

**The watcher reconcile sweep — the pre-cutover third layer — is gone.** It lived in the deleted worker. So the rare **double-fault** (group-setup exhausts retries AND rollback also fails) now leaves an orphaned `core.zombies` row that is **not** auto-healed; recovery is operator-driven (logged `hint=row_orphaned_manual_recovery`) or awaits a future control-plane reconcile job. The orphan is inert: the zombie has no runner leasing it and no live tail; it surfaces in `core.zombies` as `status='active'` with no events group.

```
   TIME ──►
   t=0  USER → zombiectl install → API
   t=2  API: INSERT core.zombies (status='active') → PG ✓
   t=3  API: XGROUP CREATE MKSTREAM ╳ (4 retries exhausted, ~2.1s)
   t=4  API: DELETE core.zombies row ╳ (rare second failure)
   t=5  API: 500 → user. Logs: zombie.create_stream_setup_failed,
                              zombie.create_rollback_failed

   ── ORPHAN WINDOW (until operator / future reconcile job) ──
      PG row Z = active; Redis stream + group missing. Other zombies
      unaffected. No runner can lease Z (its events group does not exist).
```

A future reconcile job (a control-plane sweep over `core.zombies` for `active` rows whose events group is missing, calling `redis_zombie.ensureZombieConsumerGroup`) is the planned replacement for the deleted watcher's healing role; it is out of scope here.

---

## Notable invariants this flow proves

- **No race on stream / group creation.** `innerCreateZombie` does INSERT + `XGROUP CREATE` synchronously before returning 201. Any event arriving within microseconds of the 201 finds the stream already there, ready to be leased.
- **All triggers funnel into one ingress.** Webhook, cron, steer, and continuation are different *producers* into `zombie:{id}:events`; the lease path doesn't branch on actor type.
- **Credentials never enter agent context.** Substitution happens at the tool bridge, inside the runner's sandboxed child, after sandbox entry. The agent sees `${secrets.fly.api_token}`; HTTPS request headers get real bytes; responses never echo the token; the bytes never cross the activity pipe.
- **Exactly one active lease per zombie.** The atomic affinity claim + monotonic fencing token guarantee a single in-flight lease per zombie no matter how many runners poll.
- **Reclaim is lease-layer, not Redis-consumer.** A dead runner is reclaimed via `lease_expires_at` + `fencing_token`, never `XAUTOCLAIM` — Redis cannot observe an off-platform processor's death.
- **Late writers are fenced.** A reclaimed or killed runner's `report` is rejected by the `fencing_token` CAS, so it cannot mutate state. Negative-tested.
- **Long-running stages don't crash the model.** The three context-lifecycle layers (see [`capabilities.md`](./capabilities.md) §4) keep context bounded. If a single incident exceeds budget, the zombie chunks and continues in a new stage from a `memory_recall` snapshot — possibly on a different runner.
