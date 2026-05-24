# Scaling and tuneup — how the runtime grows on Upstash

> Parent: [`README.md`](./README.md) · Companion: [`data_flow.md`](./data_flow.md) §"Connection topology — pool vs. dedicated"
>
> **Scope:** the connection math assumes per-zombie worker threads holding dedicated Redis connections. The M80 split ([`runner_fleet.md`](./runner_fleet.md)) moves datastore connections off the host into `zombied`, which changes these numbers; re-derive in M80_003.

Read this when you need to size a deployment, pick env-var values, or decide whether the next bottleneck is the Upstash plan, worker-box fan-out, or process-local pool sizing.

---

## TL;DR

**Event delivery latency:** ~10–30 ms per zombie, fleet-count-independent. **The `BLOCK 5000` in XREADGROUP is NOT delivery latency** — it's the polling ceiling for an idle stream. Messages return immediately on arrival. See §"Event-delivery latency" below.

**Binding constraint:** Upstash plan's max-concurrent-connections cap. Each zombie holds one dedicated connection; pool sizing does not move this number.

**Connection math (100 zombies, 10 worker boxes, 2 API boxes):**

```
worker tier   ≈ 130 – 190     (1 watcher + ~10 zombies + 2-8 pool, per box × 10)
api tier      ≈  16           (2 boxes × pool)
sse tier      ≈  N_tabs       (1 connection per open dashboard tail)
─────────────────────────────
                ≈ 150 + N_tabs
```

**Dominant idle cost on Upstash PAYG:** the `XREADGROUP BLOCK 5000` loop on idle streams iterates every 5 seconds, costing **~79k requests/hour at idle for this fleet, before any event traffic**. Doubling `BLOCK` halves this; event delivery latency is unaffected.

**Defaults to keep unless instrumentation says otherwise:**

| Env var | Default | Move when |
|---|---|---|
| `REDIS_POOL_MAX_IDLE` | 8 | `Pool.acquire` p99 > ~5ms |
| `REDIS_POOL_EAGER_MIN` | 2 | Cold-boot install p99 dial-bound |
| `REDIS_REQUEST_TIMEOUT_MS` | 5000 | Upstash p99 RTT > 4s (= failure, not slowness) |
| `XREADGROUP BLOCK` (ms) | 5000 | Idle Upstash bill is the dominant cost line; raise to 10000–30000 |

Full reasoning, math, and growth paths below.

---

## The infra reality first

v2 ships hosted on Fly.io; the canonical Redis is **Upstash Redis**, accessed over TLS from every Fly machine. Three Upstash-specific properties shape every other decision in this file:

1. **Plan-bound max-connections cap.** Each Upstash database has a hard concurrent-connection ceiling tied to its plan tier. New dials past the cap are refused — there is no overflow queue. **This is the binding fleet-wide constraint, not pool sizing.**
2. **Per-request pricing on Pay-as-you-go.** Every command is a billable request: XADD, XREADGROUP (one per BLOCK iteration, even when it returns empty), PUBLISH, SUBSCRIBE acknowledgements, XACK. The dedicated-XREADGROUP loop has a measurable idle bill — see §"Per-request volume" below.
3. **TLS dial cost + regional RTT.** Pool warm-up matters more on Upstash than on plaintext local Redis because each dial pays a TLS handshake. Regional vs Global database choice sets the floor on every round-trip.

The tuneup must respect all three. Pool sizing helps with #2 (request volume) and process-local contention; it does **not** help with #1 (connection cap) — per-zombie XREADGROUP connections are dedicated by design and add directly to the fleet total.

---

## Event-delivery latency — what `BLOCK 5000` does NOT do

The load-bearing semantic that's easy to misread: `XREADGROUP ... BLOCK 5000` **does not delay event delivery by 5 seconds**. The block is a polling ceiling for an empty stream, not a floor on message latency. When a message arrives, the call returns immediately.

End-to-end event latency for a 100-zombie fleet:

| Step | Typical cost (regional Upstash) |
|---|---|
| API handler receives steer / webhook | ~ms |
| API `XADD zombie:{id}:events` round-trip | ~3–10 ms (Upstash regional RTT) |
| Redis dispatches to worker's blocked XREADGROUP | sub-ms |
| `XREADGROUP` returns to worker | ~3–10 ms (RTT) |
| Worker dispatches to executor over Unix-socket RPC | ~ms |
| **Total: XADD → worker pickup** | **~10–30 ms typical, well under 100ms** |

**Independent of zombie count.** 100 zombies receiving events concurrently each see their own ~10–30ms — XREADGROUP delivery is per-stream and per-connection, not fleet-shared. The 100ms budget is comfortable as long as Upstash regional RTT stays single-digit-ms; use regional Upstash databases co-located with the Fly worker region (Upstash Global has higher RTT and would push event delivery toward the 100ms ceiling).

**What `BLOCK 5000` actually bounds:**

- **Idle Upstash bill** — one billed request per BLOCK timeout on each of the 110 long-lived XREADGROUPs in this fleet. See §"Per-request volume" below.
- **Shutdown / drain detection floor on idle zombies** — the worker checks its in-process drain flag *between* XREADGROUP iterations. On an idle stream, that check happens up to `BLOCK_ms` after the flag is set. (Active streams check between every event return, so this floor only bites zombies receiving zero traffic at shutdown time.)

Neither is event-delivery latency. Lengthening `BLOCK` to 30000 ms **does not slow event delivery**; it slows graceful shutdown for idle zombies and cuts the idle request bill by 6×.

---

## Worked example: 100 zombies, 10 worker boxes, one Upstash database

A concrete fleet to anchor the math: 100 long-running zombies, 10 `zombied-worker` Fly machines, 2 `zombied-api` Fly machines, one Upstash Redis database in the same Fly region. Zombies balance across worker boxes via the `zombie_workers` consumer group's XREADGROUP claim — expected ~10 zombies/box on average, with skew bounded by claim recency.

### Per-worker-box Upstash connection budget

Each worker box hosts one watcher thread + N per-zombie threads + one request-path pool. Per the topology diagram in [`data_flow.md`](./data_flow.md) §"Connection topology", these are deliberately split: long-lived blocking reads hold **dedicated** Upstash connections, short-lived commands borrow from the **pool**.

| Surface | Count (this example) | Connection type | Counted against Upstash plan? |
|---|---|---|---|
| Watcher → `zombie:control` XREADGROUP BLOCK 5000 | 1 | Dedicated | Yes — 1 |
| Per-zombie → `zombie:{id}:events` XREADGROUP BLOCK 5000 | ~10 (zombies on this box) | Dedicated | Yes — ~10 |
| Pool idle (XADD / PUBLISH / XACK) | 2 → 8 | Pooled, bursty | Yes — counts the **idle** count, not in-flight |
| **Per box: steady total** | **~13 idle → ~19 burst** | | |

Across 10 worker boxes: **~130 idle → ~190 burst** Upstash connections from the worker tier alone.

### Per-API-box Upstash connection budget

| Surface | Count per API box | Counted against Upstash plan? |
|---|---|---|
| Pool idle (XADD `zombie:{id}:events` from steer / webhook / cron / continuation handlers) | 2 → 8 | Yes |
| SSE subscribers (one per open `GET /events/stream` connection) | N tabs | Yes — 1 per open SSE tail |

Across 2 API boxes: **~16 pool + N_tabs SSE connections**. The SSE count is customer-driven, not fleet-driven — a single dashboard with 100 tabs open contributes 100 Upstash connections to the total.

### Fleet-wide Upstash connection total

```
worker tier:  10 boxes × (1 watcher + ~10 zombies + 2-8 pool) ≈ 130 – 190
api tier:      2 boxes × 8 pool                                ≈   16
sse tier:     1 conn per open dashboard tab                    ≈ N_tabs
                                                              ────────────
                                                              ≈ 150 + N_tabs
```

**This number must fit inside the Upstash plan's max-connections cap, with headroom for failover storms (§"Redis failover" below).** If the plan caps at 256 concurrent connections, 150 baseline leaves 106 for SSE tails + failover burst — workable. If the plan caps at 100, the example fleet already exceeds it before the first dashboard tab opens; the Upstash plan must scale up before the fleet does.

### Per-request volume (the Upstash bill)

Per-request pricing is where the dedicated-XREADGROUP design has a non-obvious cost. Each blocked XREADGROUP that hits its `BLOCK 5000` timeout on an *idle* stream counts as one billable request, even when no messages arrived. Worked numbers for this fleet, fully idle:

| Source | Requests per hour |
|---|---|
| Watcher XREADGROUP `BLOCK 5000` on `zombie:control` (10 boxes × 12/min) | 7,200 |
| Per-zombie XREADGROUP `BLOCK 5000` on `zombie:{id}:events` (100 zombies × 12/min) | 72,000 |
| **Idle subtotal** | **~79,200/hr ≈ 1.9M/day** |

Then active traffic on top:

| Source | Per zombie per active hour | At 100 zombies, 50% active |
|---|---|---|
| XADD events (steer / webhook / continuation) | ~varies; ~10-30/hr typical | ~1,500/hr |
| PUBLISH activity frames (per stage step, ~5/event) | ~50-150/hr | ~5,000/hr |
| XACK (one per event) | ~10-30/hr | ~1,500/hr |
| **Active subtotal** | | **~8,000/hr** |

**Idle dominates.** The 100-zombie fleet costs ~79k Upstash requests per hour even when nothing is happening, simply because 110 BLOCK 5000 loops iterate every 5 seconds. This is the load-bearing observation for Upstash sizing: **the bill scales with `(zombies + workers) × (3600 / BLOCK_seconds)`**, not with event throughput. Doubling BLOCK to 10000 halves the idle bill — but doubles event latency floor under cancellation (worker takes up to 10s to notice a `drain_request` instead of 5s).

This is a real tuneup knob; see §"Tuneup knobs" below.

---

## Tuneup knobs and when to turn them

| Knob | Default | What it scales with | Turn it when |
|---|---|---|---|
| `REDIS_POOL_MAX_IDLE` | 8 | Concurrent in-flight short-lived commands per process — **not** zombie count | p99 of `Pool.acquire` wait exceeds ~5ms under load; pool burst-create rate stays elevated steady-state. Above 16 is unusual: request-path commands complete in single-digit ms even over Upstash TLS. |
| `REDIS_POOL_EAGER_MIN` | 2 | Cold-boot dial cost (Upstash TLS handshake ~tens of ms per dial) | Cold-boot install latency p99 is dominated by dial time. Two pre-warmed connections cover the typical boot window; raising it only helps if boot is followed by an immediate burst of short-lived commands. |
| `REDIS_REQUEST_TIMEOUT_MS` | 5000 | Upstash tail latency tolerance | Upstash p99 round-trip exceeds 4s under healthy traffic, or `error.ReadFailed` fires spuriously on the request path. Lower it to surface a frozen-proxy / dead-peer-with-keepalive faster. **Do not raise it** — Upstash regional p99 is single-digit-ms; >5s is failure, not slowness. |
| `XREADGROUP BLOCK` duration | 5000 ms | Idle request volume (Upstash bill) **and** shutdown/drain detection floor on idle zombies. **Not event-delivery latency** — XREADGROUP returns immediately when a message arrives. | Idle request bill is the dominant cost line on PAYG. Doubling BLOCK to 10000 halves the idle bill but doubles drain-detection floor for idle zombies. Below 2000 wastes requests; above 30000 makes SIGTERM drain visibly slow on idle streams. Event delivery latency is unaffected at any value. |
| `XREADGROUP COUNT` | 16 (watcher) / 1 (per-zombie) | Burst absorption | Per-zombie COUNT > 1 only makes sense if event bursts are common and ordering across a burst doesn't matter; today's design assumes one-event-at-a-time per zombie for ordering. Leave at 1. |
| Worker replica count | deployment-driven | Zombies-per-box budget; Upstash connection burst on box loss | A single box would carry > ~50 zombies. Each per-zombie thread is a dedicated Upstash connection; at ~50 zombies/box recovery from box loss puts ~50 simultaneous TLS dials onto Upstash within the consumer-group reclaim window. |
| Per-box zombie soft cap | not enforced | Upstash plan headroom | Total fleet connections approach the Upstash plan cap. Refuse new claims from a saturated box; let the consumer group rebalance new `zombie:control` reads onto a less-loaded replica. |
| API replica count | deployment-driven | HTTP QPS + SSE fan-in (each SSE tail = 1 Upstash conn) | Steer / webhook latency p99 climbs, **or** SSE connection count on a single API replica × open tabs is bumping the plan cap. SSE connections are dedicated and long-lived — these are the Upstash-side cost driver for the API tier. |

**Knobs that do NOT scale with zombie count.** `REDIS_POOL_MAX_IDLE` and `REDIS_POOL_EAGER_MIN` size the **request-path** pool. Adding zombies adds dedicated XREADGROUP connections, not pool pressure. Bumping `MAX_IDLE` to 100 because you have 100 zombies is the wrong instinct — measure pool acquire wait time first; if it's healthy, leave it.

**Knobs that DO scale with zombie count.** The `XREADGROUP BLOCK` duration is the only knob that meaningfully moves Upstash request-volume cost at idle. Treat it as a deliberate cost/latency trade-off, not a default.

---

## Where the next ceiling actually lives on Upstash

Once connection count and request volume both fit comfortably inside the plan, the next bottleneck is one of three:

### 1. Upstash plan ceiling (the usual answer)

The binding constraint is one of: max concurrent connections, requests per second, daily request quota, or bandwidth — whichever your plan tier defines first. **Check the current Upstash plan limits before sizing.** Plans differ on which axis is tight: prepaid plans set request quotas explicitly; PAYG meters everything and charges per unit.

Symptom: dials get refused, command returns transient errors, or invoice spikes month-over-month with no event-volume change. Fix: upgrade plan, or drop idle request volume by tuning `XREADGROUP BLOCK`.

### 2. Pub/sub fan-out on activity

`zombie:{id}:activity` PUBLISH is cheap server-side (worker is the sole publisher); SSE subscribers each hold one dedicated Upstash SUBSCRIBE connection. A dashboard with 1000 simultaneous viewers = 1000 Upstash connections **before** the underlying zombie has produced a byte. Plan API-tier sizing around peak concurrent SSE, not peak zombie count.

Symptom: API-tier fd exhaustion or socket-accept queue depth climbing while Upstash is otherwise idle; Upstash connection count climbs with viewer count. Fix: more API replicas, or move SSE off the API tier to a dedicated process pool.

### 3. Postgres connection count (orthogonal)

Every worker thread that resolves credentials via `vault.secrets` or writes `core.zombie_events` opens a Postgres connection too. Pgbouncer / a connection pooler is assumed in production; sizing lives in the deployment runbook, not here.

---

## Growth paths that respect Upstash's shape

Scaling zombies linearly (100 → 1000) without touching the connection topology:

| Strategy | Per-box footprint | Fleet-total Upstash connections | Idle request volume | Trade-off |
|---|---|---|---|---|
| **More boxes, same fan-out per box** (10 × 10 → 100 × 10) | Unchanged: ~13–19 conns/box | 10× (~1500–1900) | 10× (~792k/hr) | Capacity-planning, not config. Upstash plan must absorb both axes. |
| **Bigger boxes, more zombies per box** (10 × 100) | ~108 conns/box | Fleet total ~same (~1180) | Fleet total ~same | Single-box loss reclaims 100 zombies onto survivors — 100 simultaneous TLS dials. Watch reclaim p99. |
| **Longer XREADGROUP BLOCK** (5000 → 30000) | Unchanged | Unchanged | 6× lower (~13k/hr) | Cancellation / drain detection floor rises proportionally. |
| **Coalesce XREADGROUP across zombies on one box** | Single XREADGROUP serving N streams via multiple `>` keys | Fleet-total halved | Halved | Not implemented today — each per-zombie thread owns its XREADGROUP for clean cancellation and ownership transfer on `drain_request`. See `src/queue/AUDIT.md` for the design alternative; out of scope for M69_004. |

For an Upstash-bound fleet, the realistic order of operations is: (1) lengthen BLOCK to reduce idle bill, (2) cap zombies-per-box and add boxes for resilience, (3) only then upgrade the Upstash plan, (4) revisit coalesced XREADGROUP if connection-count is the binding constraint after all of the above.

---

## Sizing procedure (agent- and playbook-readable)

This section is intentionally structured for an LLM agent or a `zombiectl`-driven scaling playbook to execute step-by-step. Each step has explicit inputs, a formula, a decision rule, and an emit target. Prose elsewhere in this file is the *why*; this section is the *what to do*.

### Inputs

| Symbol | Meaning | Source |
|---|---|---|
| `Z` | Target zombie count (active + idle) | Product / fleet plan |
| `W` | Worker box count (`zombied-worker` Fly machines) | Deployment plan |
| `A` | API box count (`zombied-api` Fly machines) | Deployment plan |
| `S` | Peak concurrent SSE tails (dashboard viewers) | Product / dashboard usage |
| `P_conn` | Upstash plan max-concurrent-connections cap | Upstash plan docs (current) |
| `P_rps` | Upstash plan requests/sec cap (or ∞ on PAYG) | Upstash plan docs |
| `C_req` | Upstash per-request cost on PAYG (USD) | Upstash pricing (current) |
| `B_idle_usd` | Acceptable monthly idle bill budget | Finance / product |

### Procedure

```
Step 1: Per-box zombie load
  zombies_per_box = ceil(Z / W)
  ASSERT zombies_per_box <= 50
    if violated → action: increase W until zombies_per_box <= 50
    reason: failover dial burst > 50 connections risks plan-cap overflow

Step 2: Steady-state Upstash connection budget
  worker_conns = W * (1 + zombies_per_box + REDIS_POOL_MAX_IDLE)
                  └─ 1 watcher + N zombie XREADGROUPs + pool idle slots
  api_conns    = A * REDIS_POOL_MAX_IDLE
  sse_conns    = S
  total_steady = worker_conns + api_conns + sse_conns

Step 3: Failover headroom
  failover_burst = W * zombies_per_box
                   └─ every per-zombie thread re-dials on transport error
  required_cap = total_steady + failover_burst
  ASSERT required_cap <= P_conn
    if violated → action: upgrade Upstash plan to a tier with P_conn >= required_cap
                        OR reduce zombies_per_box (raise W)

Step 4: Idle Upstash request rate
  idle_streams = W + Z         (1 watcher per box + 1 per zombie)
  idle_rps     = idle_streams / (XREADGROUP_BLOCK_MS / 1000)
  ASSERT idle_rps <= P_rps        (only meaningful on capped plans)
    if violated → action: raise XREADGROUP_BLOCK_MS to {10000, 30000}
                          and re-evaluate

Step 5: Monthly idle bill projection (PAYG only)
  monthly_idle_requests = idle_rps * 60 * 60 * 24 * 30
  monthly_idle_cost     = monthly_idle_requests * C_req
  ASSERT monthly_idle_cost <= B_idle_usd
    if violated → action: raise XREADGROUP_BLOCK_MS to halve cost per doubling
                          NOTE: drain-detection floor for idle zombies rises proportionally;
                                event-delivery latency unchanged

Step 6: Emit configuration
  REDIS_POOL_MAX_IDLE        = 8            (override only with measured Pool.acquire p99 > 5ms)
  REDIS_POOL_EAGER_MIN       = 2            (override only with cold-boot install p99 dial-bound)
  REDIS_REQUEST_TIMEOUT_MS   = 5000         (do not raise; lower only to surface dead-peer faster)
  XREADGROUP_BLOCK_MS        = <step 4/5 result>
  per_box_zombie_soft_cap    = zombies_per_box
  upstash_plan_tier          = <step 3 result>
```

### Output (canonical shape for a playbook to emit)

```yaml
fleet_sizing:
  inputs:
    target_zombies: <Z>
    worker_boxes: <W>
    api_boxes: <A>
    peak_sse_tails: <S>
  derived:
    zombies_per_box: <ceil(Z/W)>
    total_steady_connections: <total_steady>
    failover_burst_connections: <failover_burst>
    required_plan_capacity: <required_cap>
    idle_requests_per_second: <idle_rps>
    monthly_idle_cost_usd: <monthly_idle_cost>
  config:
    REDIS_POOL_MAX_IDLE: 8
    REDIS_POOL_EAGER_MIN: 2
    REDIS_REQUEST_TIMEOUT_MS: 5000
    XREADGROUP_BLOCK_MS: <derived>
    per_box_zombie_soft_cap: <derived>
  upstash:
    plan_tier_required: <derived>
    rationale: <which assert failed at which step, or "all asserts passed">
```

### Anti-patterns (do NOT do these)

A playbook or sizing agent should **never** emit any of the following — they are common-instinct moves that produce wrong configurations on this architecture:

1. **Raise `REDIS_POOL_MAX_IDLE` because zombie count is high.** The pool serves request-path commands; per-zombie connections are dedicated and bypass it. Bump only on measured `Pool.acquire` p99 wait time.
2. **Lower `XREADGROUP BLOCK` to "make event delivery faster".** BLOCK is the idle-iteration ceiling, not delivery latency. Lowering it raises the Upstash bill and does nothing for event latency.
3. **Raise `REDIS_REQUEST_TIMEOUT_MS` above 5000 to "tolerate slow Upstash".** Upstash regional p99 is single-digit-ms; >5s is failure, not slowness. Raising the timeout hides a real symptom.
4. **Coalesce multiple zombies onto one XREADGROUP to save connections.** Not supported in the current design — each per-zombie thread owns its XREADGROUP for cancellation and ownership-transfer semantics. Out of scope until a future spec implements it.
5. **Pool the watcher's or SSE's connection.** Architectural invariant from [`data_flow.md`](./data_flow.md) §"Connection topology" — blocking reads must hold dedicated connections.
6. **Skip the failover-headroom check in Step 3.** Steady-state fit is necessary but not sufficient. A plan that fits `total_steady` but not `total_steady + failover_burst` will silently drop reconnect attempts during failover.

### Self-check (after emitting config)

A playbook should verify its own output by re-running these asserts against the emitted config:

```
assert (config.REDIS_POOL_MAX_IDLE >= 2) and (config.REDIS_POOL_MAX_IDLE <= 32)
assert config.REDIS_POOL_EAGER_MIN <= config.REDIS_POOL_MAX_IDLE
assert config.REDIS_REQUEST_TIMEOUT_MS in range(1000, 5001)
assert config.XREADGROUP_BLOCK_MS in range(2000, 30001)
assert derived.required_plan_capacity <= upstash.plan_capacity
assert derived.monthly_idle_cost_usd <= inputs.budget_usd or upstash.plan_tier == "fixed"
```

Any failure halts the playbook with the assertion identity and the violating value, **not** with an auto-corrected config — sizing decisions cross the budget/architecture boundary and require operator sign-off.

---

## Failure and rebalance behavior on Upstash

These are properties of the consumer-group + sticky-thread + Upstash-TLS design that the operator needs to know when sizing.

### Worker box loss (one of 10 boxes dies)

- The `zombie_workers` consumer group's pending-entry list (PEL) holds in-flight `zombie:control` messages claimed by the dead box.
- Surviving boxes' XREADGROUP picks them up after the group's `min-idle-time` window (XPENDING-driven reclaim, not automatic on disconnect).
- Per-zombie threads on the lost box are gone. The surviving box that claims the next `status_changed` / `drain_request` / continuation event for those zombies re-spawns the thread; the thread's first action is to dial Upstash for its dedicated XREADGROUP.
- No replay of in-flight executions: durability is at-least-once via `core.zombie_events` + `INSERT ON CONFLICT (zombie_id, event_id) DO NOTHING`.

**Operator implication on Upstash.** Recovery from a box loss puts ~10 zombies' worth of cold-start TLS dials onto Upstash over the reclaim window. With 50 zombies/box this becomes a 50-connection burst with 50 TLS handshakes. Pre-warming via `REDIS_POOL_EAGER_MIN` does not help — those reconnects are dedicated, not pooled. The Upstash plan must have headroom above the steady-state baseline equal to the largest box's per-zombie connection count, or the reclaim attempts will fail mid-recovery.

### Worker box add (scale out from 10 → 15 boxes)

- The new box's watcher joins the consumer group; new `zombie:control` messages start landing on it.
- **Existing zombies stay sticky to their current box.** Their per-zombie thread is already mid-`XREADGROUP BLOCK 5000` on `zombie:{id}:events`; the new box's watcher only sees `zombie:control`, not the per-zombie streams.
- New installs flow to the least-recently-busy replica via consumer-group fairness.
- This is sticky by design: mid-flight handoff would require draining + re-claiming an in-flight `XREADGROUP`, which the protocol doesn't cleanly support.

**Operator implication on Upstash.** Scaling out does not rebalance warm zombies and does not move Upstash connections from one box to another. To force rebalance, drain a busy box (`SIGTERM` triggers the shutdown sequence in [`data_flow.md`](./data_flow.md) §C — per-zombie workers close their XREADGROUP, surviving boxes claim continuations and re-dial Upstash for new XREADGROUP connections).

### Upstash failover (provider-side primary swap)

- The pool's `isResumable(err)` predicate (M69_004 §4) classifies errors at compile time. `READONLY` after failover is resumable: connection recycled, retry succeeds against the new primary.
- Transport errors (`BrokenPipe`, `ConnectionResetByPeer`) are not resumable: connection closed, fresh one dialed (paying a TLS handshake).
- Per-zombie XREADGROUP loops surface transport errors to their thread, which re-dials on the next iteration of its loop. There is no centralized reconnect handler — the loop is the handler.

**Operator implication on Upstash.** During failover, expect a brief connection-storm as every per-zombie thread + every API pool acquire re-dials. With 100 zombies + 10 boxes, that's ~150 simultaneous TLS handshakes inside ~1 second. The Upstash plan needs to absorb that burst; the request-path retry budget (`isResumable` resumable errors retry twice, transport closes + redials once) bounds the storm but doesn't eliminate it.

---

## What is explicitly out of scope here

- **Adaptive pool sizing.** Fixed `max_idle` cap is sufficient under M69_004. Revisit only if post-landing bench shows pool-of-8 still leaves contention on a sustained workload.
- **Coalesced per-box XREADGROUP.** See growth-paths table above. Designed but not implemented; revisit only if dedicated-connection count becomes the binding fleet constraint on Upstash.
- **Switching off Upstash.** Self-hosted Redis on Fly machines is a v3 consideration; the rules here would still apply (replace "Upstash plan cap" with "`maxclients`" and "per-request pricing" with "RAM + CPU"), but the load-bearing math is unchanged.
- **Postgres scaling.** Pgbouncer + plan sizing covered in the deployment runbook, not here.
- **Multi-region Redis.** Single regional Upstash database assumed; co-locate with Fly worker region. Cross-region failover and Upstash Global database posture is a separate architectural decision (RTT vs consistency trade-off).
