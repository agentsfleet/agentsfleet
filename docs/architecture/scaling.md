# Scaling and tuneup — how the runtime grows after the cutover

> Parent: [`README.md`](./README.md) · Companions: [`data_flow.md`](./data_flow.md) §"Connection topology", [`runner_fleet.md`](./runner_fleet.md) §"Scaling".
>
> **Scope:** this file sizes the runtime as it runs now — after the M80_002 cutover. The cutover **deleted the per-agent dedicated Redis connection** (the worker's blocking `XREADGROUP` loop), which was the pre-cutover binding constraint. The binding constraint moved; the math below reflects the new shape.

Read this when you need to size a deployment, pick env-var values, or decide whether the next bottleneck is `agentsfleetd` API replicas, Postgres, the Upstash plan, or runner fan-out.

---

## TL;DR — what the cutover changed

**The old wall is gone.** Before the cutover, every agent held one dedicated `XREADGROUP … BLOCK 5000` Redis connection, so the fleet was capped by the Upstash max-concurrent-connections ceiling at roughly one connection per agent. **That tier no longer exists.** `agentsfleetd` now claims work with a **non-blocking** `XREADGROUP` on the request thread that serves a `lease` call — a short-lived pooled command. Runners hold **zero** Redis connections.

**The new binding constraint** is `agentsfleetd` API replicas + Postgres write throughput on the lease/report hot path — both horizontally scalable. Redis sees only pooled short-lived commands (`XADD`, non-blocking `XREADGROUP`, `PUBLISH`, `XACK`) plus the SSE `SUBSCRIBE` tier; runners scale out with no Redis coordination at all.

**The idle Upstash bill** is no longer driven by N blocking `XREADGROUP` loops. It is driven by **runner lease-poll cadence**: each idle runner polls `lease` every `NO_WORK_RETRY_AFTER_MS` (1 s) and each poll does one bounded non-blocking `XREADGROUP` scan. The knob is the poll backoff, not `XREADGROUP BLOCK`.

| What | Before (deleted) | Now |
|---|---|---|
| Per-agent Redis connections | 1 dedicated blocking conn per agent | 0 — `lease` uses a pooled non-blocking read |
| Binding constraint | Upstash max-connections cap (~1/agent) | `agentsfleetd` API replicas + Postgres write throughput |
| Idle request driver | `(agents + workers) × (3600 / BLOCK_s)` | `runners × (3600 / poll_s)` |
| Idle-cost knob | `XREADGROUP BLOCK` | `NO_WORK_RETRY_AFTER_MS` (runner poll backoff) |
| Redis dedicated connections | per-agent XREADGROUP + watcher + SSE | **one SubscriptionHub conn per replica** (viewers share it; refcounted SUBSCRIBE per channel) |

---

## The infra reality first

v2 ships hosted on Fly.io; the canonical Redis is **Upstash Redis**, accessed over Transport Layer Security (TLS) from every Fly machine. Three Upstash-specific properties still shape decisions, though the cutover changed which one binds:

1. **Plan-bound max-connections cap.** Each Upstash database has a hard concurrent-connection ceiling. New dials past the cap are refused. **After the cutover this no longer scales with agent count** — and since the SubscriptionHub, not with viewer count either: only with `agentsfleetd` API-pool connections + one hub connection per replica. It is rarely the first wall now.
2. **Per-request pricing on Pay-as-you-go.** Every command is billable: `XADD`, the non-blocking `XREADGROUP` (one per idle lease poll), `PUBLISH`, `XACK`, SSE acknowledgements. The idle bill is the lease-poll loop — see §"Per-request volume".
3. **TLS dial cost + regional round-trip-time (RTT).** Pool warm-up matters because each dial pays a TLS handshake. Regional vs Global database choice sets the floor on every round-trip.

---

## Event-delivery latency

The cutover added one hop (the runner long-poll) and removed another (the in-process worker dispatch). End-to-end latency for a steer:

| Step | Typical cost (regional Upstash) |
|---|---|
| API handler receives steer / webhook | ~ms |
| API `XADD agent:{id}:events` round-trip | ~3–10 ms (Upstash regional RTT) |
| Runner's next `lease` poll picks it up | **0 – `NO_WORK_RETRY_AFTER_MS` (≤ 1 s)** if the runner is idle-polling; immediate if a runner is already mid-poll |
| `lease` handler: non-blocking `XREADGROUP` + gates + secret resolve + issue lease | ~ms + PG round-trips |
| Runner forks the sandboxed child, runs NullClaw | dominated by the agent's own runtime |

**The lease-poll interval is the floor on pickup latency for an idle fleet**, not a per-message delay — a runner already long-polling returns as soon as work is assignable. Tightening `NO_WORK_RETRY_AFTER_MS` lowers idle pickup latency at the cost of more idle `XREADGROUP` requests; it is the direct trade the old `XREADGROUP BLOCK` knob used to make, now on the runner side.

---

## Connection budget after the cutover

`agentsfleetd` holds the only Redis connections now. Per API replica:

| Surface | Connection type | Counted against Upstash plan? |
|---|---|---|
| Pool (XADD ingress, non-blocking `XREADGROUP` on lease, PUBLISH activity, XACK on report) | Pooled, bursty (`max_idle=8`) | Yes — the **idle** pool count |
| SubscriptionHub (all SSE viewers on the replica share it) | ONE dedicated `SUBSCRIBE` conn, long-lived | Yes — 1 per replica, regardless of viewer count |

```
agentsfleetd tier:  R replicas × REDIS_POOL_MAX_IDLE          ≈ 8·R
sse tier:      1 SubscriptionHub conn per replica         ≈ R
                                                          ──────────
                                                          ≈ 9·R
```

**There is no per-agent term — and no per-viewer term.** Adding agents adds Postgres writes and lease throughput, not Redis connections; a dashboard with 100 open tabs costs the hub one connection and one wire SUBSCRIBE per distinct agent watched. Viewer count still drives per-replica **threads + memory** (the `SSE_MAX_STREAMS` knob), just not Upstash connections. Runners contribute **zero** Upstash connections.

### Per-request volume (the Upstash bill)

Idle cost is now the runner lease-poll loop, fully idle:

| Source | Requests per hour |
|---|---|
| Runner lease polls (`R_runners × 3600 / poll_seconds`), each doing one bounded non-blocking `XREADGROUP` scan | `R_runners × 3600` at the 1 s default |
| (No watcher loop, no per-agent BLOCK loops — both deleted) | 0 |

For a 20-runner fleet at the 1 s default: ~72,000 idle `lease`-scan requests/hour. Doubling `NO_WORK_RETRY_AFTER_MS` to 2 s halves it; the trade is idle pickup latency, not event-delivery latency for a busy fleet. Active traffic (XADD ingress, PUBLISH activity ~5/event, XACK on report) sits on top, scaling with event throughput as before.

**The load-bearing shift:** the idle bill now scales with **runner count**, not `(agents + workers)`. A fleet with many idle agents but few runners is cheap at idle; the cost follows the pollers, not the population.

---

## Tuneup knobs and when to turn them

| Knob | Default | What it scales with | Turn it when |
|---|---|---|---|
| `REDIS_POOL_MAX_IDLE` | 8 | Concurrent in-flight short-lived commands per `agentsfleetd` replica — **not** agent count | p99 of `Pool.acquire` wait exceeds ~5 ms under load. The lease/report/ingress/activity commands all complete in single-digit ms over Upstash TLS; above 16 is unusual. |
| `REDIS_POOL_EAGER_MIN` | 2 | Cold-boot dial cost (Upstash TLS handshake) | Cold-boot `agentsfleetd` latency p99 is dominated by dial time. |
| `REDIS_REQUEST_TIMEOUT_MS` | 5000 | Upstash tail-latency tolerance | Upstash p99 round-trip exceeds 4 s under healthy traffic. **Do not raise it** — >5 s is failure, not slowness. |
| `NO_WORK_RETRY_AFTER_MS` | 1000 | Idle lease-poll request volume (Upstash bill) **and** idle pickup latency. **Not busy-fleet delivery latency.** | Idle request bill is the dominant cost line on PAYG. Raise to 2000–5000 to cut the idle bill proportionally; idle pickup latency rises by the same factor. Single-sourced in `src/lib/common/constants.zig`. |
| `LEASE_TTL_MS` | 30000 | Reclaim latency floor **and** the max single-agent runtime before reclaim (the renewal gap) | Raise to cover the longest expected agent runtime until M80_006 lands per-lease renewal (see `runner_fleet.md` Failure Recovery Model). Lower only with a tighter recovery requirement and short agents. |
| `API_HTTP_THREADS` | 1 | Concurrent short-lived handlers **per worker** — the httpz handler-pool size. **No handler parks a pool thread anymore**: SSE streams run on dedicated detached threads (`startEventStream`), capped by `SSE_MAX_STREAMS`, not by this knob; the lease is a non-blocking single poll. | Sustained 429 shed (`agent_api_backpressure_rejections_total`) with idle CPU. Total request concurrency = `API_HTTP_WORKERS × API_HTTP_THREADS`. |
| `API_HTTP_WORKERS` | 1 | Accept/event-loop threads (epoll/kqueue), each multiplexing up to `API_MAX_CLIENTS` idle connections as fds **and owning its own `API_HTTP_THREADS` handler pool**. | Scale toward core count on a multi-core VM. The accept layer is rarely the wall. |
| `SSE_MAX_STREAMS` | 64 | Concurrent SSE tails per replica — each is a dedicated detached thread (~0.25 MiB stack; budget math at the const in `runtime_loader.zig`); all tails share the SubscriptionHub's one Redis connection. 0 is rejected at boot. | 503s on `GET /events/stream` (`agent_sse_backpressure_rejections_total`) while the box has memory headroom; watch `agent_sse_in_flight_streams`. |
| `API_MAX_IN_FLIGHT_REQUESTS` | 256 | **api-class** admission ceiling → 429 + Retry-After. Route classes at dispatch: ops (`/healthz`, `/readyz`, `/metrics`) are NEVER shed — an overload must not blind the operators; the SSE tail (stream class) answers to `SSE_MAX_STREAMS` instead; everything else is api. | Incident lever: set below `API_HTTP_WORKERS × API_HTTP_THREADS` and redeploy to shed during an overload (dormant at the default — dispatch concurrency is physically below the ceiling). Watch `agent_api_backpressure_rejections_total`. |
| `agentsfleetd` API replica count | deployment-driven | HTTP QPS (user surface + `/v1/runners`) + lease/report throughput + SSE fan-in | Lease/report p99 climbs, or per-replica viewer count keeps hitting the `SSE_MAX_STREAMS` ceiling. |
| Runner count | operator-driven | Compute throughput; idle lease-poll request volume | Add hosts to add execution capacity — no Redis or coordination cost. Each idle runner adds one poll loop to the Upstash bill (tune via `NO_WORK_RETRY_AFTER_MS`). |
| `RUNNER_WORKER_COUNT` | 1 | Concurrent leased agents **per host** — the runner worker-pool size (M88_002). N workers each run the lease→execute→report unit; the per-agent `affinity.claim` keeps two workers off the same agent. | A host has spare cores/memory while one long agent run monopolises it (per-host throughput is fixed at 1 at the default). Raise N to run more agents per host instead of enrolling more hosts. **Tradeoff:** N is a capacity knob, not a throughput guarantee (CPU/RAM/disk/network are not isolated across workers), and it **widens the failure domain** — one host loss drops N in-flight runs, not 1 (all re-leased by the M84_002 sweeper, but interrupted). `worker_count=1` is maximum isolation. |

**The `XREADGROUP BLOCK` knob is gone.** The lease path uses a non-blocking read; there is no per-stream blocking loop to tune. Its role (idle-cost vs latency trade) moved to `NO_WORK_RETRY_AFTER_MS` on the runner side.

---

## Where the next ceiling actually lives

Once Redis connection count and request volume fit the plan, the next bottleneck is one of:

### 1. `agentsfleetd` API replicas + Postgres write throughput (the usual answer now)

The lease/report hot path does the durable writes the worker used to do — `INSERT agent_events`, the two billing debits, `UPDATE` terminal, `INSERT telemetry`, checkpoint `UPSERT`, plus the `fleet.runner_leases`/`runner_affinity` bookkeeping. At fleet scale this is the binding axis. Both `agentsfleetd` replicas and Postgres (with a connection pooler) scale horizontally; the hot path is shardable per agent.

Symptom: lease/report p99 climbs; Postgres connection saturation or write-lock contention on the `fleet` tables. Fix: more `agentsfleetd` replicas + Postgres sizing in the deployment runbook.

### 2. Pub/sub fan-out on activity (unchanged in shape)

`agent:{id}:activity` PUBLISH is cheap server-side (`agentsfleetd` is the sole publisher); every SSE viewer on a replica shares the SubscriptionHub's ONE `SUBSCRIBE` connection, with one wire subscription per distinct agent watched. A dashboard with 1000 simultaneous viewers costs Upstash one connection per replica and one delivery per frame per watched channel — the per-viewer fan-out happens in-process (bounded queues, drop-oldest for stalled tabs). Plan API-tier sizing around peak concurrent SSE *threads* (`SSE_MAX_STREAMS`), not connections.

**Each open SSE stream costs one dedicated detached thread** (`startEventStream` — never a handler-pool thread; a parked stream cannot black-hole pool batches). The per-replica SSE ceiling is the `SSE_MAX_STREAMS` env knob (default 64; ~0.25 MiB + 1 client fd per stream — the Redis side is the hub's shared connection, no per-viewer term); at the cap new tails get 503 and the handler pool keeps serving everything else. An event-loop SSE substrate (a stream costs an fd, not a thread) stays a much later, gated lever — it only earns its keep above the thread/memory ceiling, and it must make the SSE *subscriber socket itself* evented, not merely offload the blocking read. See `docs/v2/done/M88_001_*` for the gated design (its premise is being re-anchored: the measured pain was pool poisoning + per-stream Redis connections, not raw httpz throughput).

Symptom: `agent_sse_backpressure_rejections_total` climbing (503s) while memory has headroom; Upstash connection count climbing with viewer count. Fix, in order: raise `SSE_MAX_STREAMS`, then a larger VM, then more API replicas.

### 3. Upstash plan ceiling (now rarely first)

Max concurrent connections, requests/sec, or daily request quota — whichever the plan tier defines first. After the cutover (and the SubscriptionHub) the connection axis is `9·R`, far below the pre-cutover `~agents`. The request axis is the runner poll loop. Check current plan limits before sizing; the binding axis is usually #1 now, not this.

---

## Sizing procedure (agent- and playbook-readable)

Structured for an LLM agent or a `agentsfleet`-driven scaling playbook. Each step has explicit inputs, a formula, a decision rule, and an emit target.

### Inputs

| Symbol | Meaning | Source |
|---|---|---|
| `Z` | Target agent count (active + idle) | Product / fleet plan |
| `N` | Runner host count | Operator / capacity plan |
| `R` | `agentsfleetd` API replica count | Deployment plan |
| `S` | Peak concurrent SSE tails | Product / dashboard usage |
| `P_conn` | Upstash plan max-concurrent-connections cap | Upstash plan docs (current) |
| `P_rps` | Upstash plan requests/sec cap (or ∞ on PAYG) | Upstash plan docs |
| `poll_s` | Runner idle poll interval = `NO_WORK_RETRY_AFTER_MS / 1000` | Config |

### Procedure

```
Step 1: Redis connection budget (no per-agent, no per-viewer term)
  redis_conns = R * (REDIS_POOL_MAX_IDLE + 1)        (+1 = the SubscriptionHub conn)
  ASSERT redis_conns + failover_burst <= P_conn
    failover_burst ≈ R * REDIS_POOL_MAX_IDLE   (pool re-dials on failover; the hub
                                                redials once and replays SUBSCRIBEs)
    if violated → add Upstash capacity
  NOTE: Z and S do not appear — agents add PG writes, viewers add hub fan-out,
        neither adds Redis connections. S sizes SSE_MAX_STREAMS (threads/memory).

Step 2: Idle Upstash request rate (the lease-poll loop)
  idle_rps = N / poll_s
  ASSERT idle_rps <= P_rps        (only meaningful on capped plans)
    if violated → raise NO_WORK_RETRY_AFTER_MS (2000, 5000) and re-evaluate

Step 3: Hot-path throughput (the real wall)
  lease_report_qps = peak events/sec across the fleet
  size R + Postgres so lease/report p99 stays within budget under lease_report_qps
  (PG connection pooler assumed; sizing in the deployment runbook)

Step 4: Emit configuration
  REDIS_POOL_MAX_IDLE      = 8       (override only with measured Pool.acquire p99 > 5ms)
  REDIS_POOL_EAGER_MIN     = 2
  REDIS_REQUEST_TIMEOUT_MS = 5000    (do not raise)
  NO_WORK_RETRY_AFTER_MS   = <step 2 result>
  LEASE_TTL_MS             = <≥ max expected agent runtime until M80_006>
  agentsfleetd_replicas         = <step 3 result>
  runner_hosts             = N
```

### Anti-patterns (do NOT do these)

1. **Size Redis connections by agent or viewer count.** There is no per-agent and no per-viewer connection. The connection budget is `9·R` (pool + one hub conn per replica).
2. **Tune `XREADGROUP BLOCK`.** It no longer exists on the hot path. Use `NO_WORK_RETRY_AFTER_MS` for the idle-cost/latency trade.
3. **Add runners to fix lease/report latency.** Runners add compute, not control-plane throughput. Scale `agentsfleetd` replicas + Postgres for hot-path latency.
4. **Raise `REDIS_REQUEST_TIMEOUT_MS` above 5000.** Upstash regional p99 is single-digit-ms; >5 s is failure, not slowness.
5. **Put `SUBSCRIBE` on the request pool.** A subscribed connection can serve nothing else — it lives outside the pool, on the SubscriptionHub, which holds exactly one and fans out in-process.
6. **Size `API_HTTP_THREADS` to peak concurrent SSE tails.** Streams no longer touch the handler pool (dedicated detached threads, `SSE_MAX_STREAMS` cap); size the pool to request concurrency and the SSE knob to viewer concurrency — they are independent axes.

---

## Failure and rebalance behavior

### Runner host loss

A runner that dies holds no datastore connection to leak and no Redis consumer to reclaim. Its in-flight lease expires at `lease_expires_at`; the next runner's `lease` reclaim path re-issues the event with a higher fencing token (see `runner_fleet.md` Failure Recovery Model). Recovery latency is `LEASE_TTL_MS` + poll density — the S0 lazy-reclaim SLA. There is **no connection storm** on runner loss — the survivors just keep polling.

**Failure domain scales with `RUNNER_WORKER_COUNT`.** A host running a pool of N concurrent leases (M88_002) drops **N** in-flight runs on loss, not one — each of the N leases expires and re-leases independently (no batch coupling), so no work is dropped, but N runs restart instead of one. This is the cost of the per-host utilization win; `worker_count=1` keeps the failure domain at one run. Operators size N against this tradeoff.

### Runner host add

A new runner registers and starts polling `lease`. No rebalance of in-flight work, no Redis connection migration, no coordination. Sticky routing prefers the runner that ran the previous run but never blocks on it.

### Upstash failover (provider-side primary swap)

Only `agentsfleetd`'s pool + each replica's hub connection re-dial. `READONLY`-after-failover is resumable (connection recycled, retry against the new primary); transport errors close + re-dial (paying a TLS handshake). The storm is bounded by `9·R` re-dials — viewers notice nothing while their replica's hub redials and replays its SUBSCRIBEs (heartbeats continue from the stream threads).

---

## What is explicitly out of scope here

- **Adaptive pool sizing.** Fixed `max_idle` cap is sufficient; revisit only if post-landing bench shows pool contention.
- **Switching off Upstash.** Self-hosted Redis on Fly machines is a v3 consideration; the connection/request shape here still applies.
- **Postgres scaling.** Pgbouncer + plan sizing covered in the deployment runbook, not here — though after the cutover it is the **primary** scaling axis, so the runbook carries more weight than it did.
- **Placement / scheduler.** Label-aware assignment (`required_tags ⊆ runner.labels`) is M85_001; capacity-aware placement and autoscale-by-queue-depth stay out of scope (the non-goals fence in runner_fleet.md).
- **Multi-region Redis.** Single regional Upstash database assumed; co-locate with the Fly `agentsfleetd` region.
