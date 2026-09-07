# Scaling and tuneup — how the runtime grows after the cutover

> Parent: [`README.md`](./README.md) · Companions: [`data_flow.md`](./data_flow.md) §"Connection topology", [`runner_fleet.md`](./runner_fleet.md) §"Scaling".
>
> [!IMPORTANT]
> **Scope:** this file sizes the runtime as it runs now — after the M80_002 cutover. The cutover **deleted the per-fleet dedicated Redis connection** (the worker's blocking `XREADGROUP` loop), which was the pre-cutover binding constraint. The binding constraint moved; the math below reflects the new shape.

Read this when you need to size a deployment, pick env-var values, or decide whether the next bottleneck is `agentsfleetd` API replicas, Postgres, the Upstash plan, or runner fan-out.

---

## Facts

Every row is extracted from the sections below; the owner column names the section that carries the full story.

| Invariant | Value | Mechanism | Owner section |
|---|---|---|---|
| Redis connection budget | 3·R at normal boot | one shared command socket, one hub socket, one outbound reader socket per replica | §Connection budget after the cutover |
| Idle Upstash bill | `N_runners / poll_s` requests/sec | each idle poll = one bounded `HRANDFIELD` + one indexed auth read; ~72,000/hour at 20 runners, 1 s | §Per-request volume |
| Idle-cost knob | `NO_WORK_RETRY_AFTER_MS` = 1000 | trades idle bill against idle pickup latency; single-sourced in `rustd/crates/afd_core/src/timing.rs` | §Tuneup knobs |
| Per-poll fan-out ceiling | `MAX_READY_CANDIDATES_PER_POLL` = 64, compile-time | randomized readiness slice; per-poll cost independent of population, even when the index is wrong | §The per-poll bound |
| Auth read per request | one indexed single-row read | M143_001 removed the per-process memo, so cordon/drain/revoke bite fleet-wide the moment they commit | §Per-request volume |
| Fleet count in the idle term | absent | fleet count appears only in the readiness *recovery* bound | §The per-poll bound |
| SSE ceiling | `SSE_MAX_STREAMS` = 64 per replica | async response bodies own semaphore permits; 503 at the cap; hub connection shared | §Tuneup knobs, §2 |
| Redis timeout | `REDIS_REQUEST_TIMEOUT_MS` = 5000 — do not raise | above 5 s is failure, not slowness | §Tuneup knobs |
| Redis connect timeout | `REDIS_CONNECT_TIMEOUT_MS` = 5000 | bounds establishment, not just commands; a dead endpoint refuses inside the budget instead of holding boot open | §Tuneup knobs |
| Lease TTL | `LEASE_TTL_MS` = 30000 | reclaim latency floor; renewal decouples run length from it | §Tuneup knobs |
| Per-host concurrency | assigned `worker_count` = 1 (dashboard, per runner) | a capacity knob that widens the failure domain to N in-flight runs on host loss | §Tuneup knobs, §Runner host loss |
| Admission ceiling | `DEFAULT_MAX_IN_FLIGHT` = 256, api-class only | compiled default; operational and stream routes use separate handling | §Tuneup knobs |
| The binding constraint | `agentsfleetd` replicas + Postgres writes | both horizontally scalable; the hot path is shardable per fleet | §Where the next ceiling actually lives |
| Recurring-read indexes | one slot per table, plan-asserted | idle Postgres cost tracks work, not accumulated rows; liveness batch = 6 buffer hits at 20,000 runners | §Which recurring Postgres reads are index-served |
| Idle retry wait | up to `NO_WORK_RETRY_AFTER_MS` before another attempt | assignment and datastore work add latency; this is not a delivery bound | §Event-delivery latency |
| Outbound-answer consumer | blocking read, up to 5 seconds | owns one dedicated Redis socket; does not block shared commands | §Tuneup knobs |
| Failover reconnects | three connection owners per healthy replica | command managers reconnect; the hub retries and restores subscriptions; retry attempts are not bounded by socket count | §Upstash failover |

## Traps

The nine sizing anti-patterns ARE the trap list — read §Anti-patterns (do NOT do these) before any sizing conversation. Three more that live elsewhere:

- The pre-M141 idle figure was wrong by exactly the fleet count — never size idle cost by fleet population (§Per-request volume).
- Metric ratios need their denominator: without `agentsfleet_lease_polls_total`, a traffic increase and a fan-out regression look identical (§The per-poll bound).
- Fleet-memory hydration still sorts, and correctly so — an index only removes a sort where the plan can exit early (§Which recurring Postgres reads are index-served).

## Topology

No standalone diagram; the sizing procedure block in §"Sizing procedure" is the operational artifact, with inputs, formulas, and emit targets.

## Decisions

| Decision | Reason | Where / artifact |
|---|---|---|
| Auth memo removed — read the runner row every request | revocation must be deterministic fleet-wide, not per-machine | §Per-request volume; M143_001, [`../AUTH.md`](../AUTH.md) §"Runner token" |
| Readiness recorded at ingress; lease consults the index before Postgres | idle cost follows the pollers, not the population | §Per-request volume; M141 |
| Bare `LIMIT` on the candidate scan rejected | an ordered scan silently starves every fleet past the bound; only a randomized slice + ceiling is fair | §Anti-patterns |
| Async SSE bodies share one subscription pump | waiting for frames does not reserve an operating-system thread per viewer | §2; `afd_sse` |
| Multiplex ordinary Redis commands | cloned handles share one socket; blocking consumers own separate sockets | §Connection budget after the cutover |

---

## Detail

Everything below is the full reference. Headings are stable — specs cite them by text; insert new sections, never rename existing ones.

## TL;DR — what the cutover changed

**The old wall is gone.** Before the cutover, every fleet held one dedicated `XREADGROUP … BLOCK 5000` Redis connection, so the fleet was capped by the Upstash max-concurrent-connections ceiling at roughly one connection per fleet. **That tier no longer exists.** `agentsfleetd` now claims work with a **non-blocking** `XREADGROUP` inside the asynchronous `lease` handler over a shared command connection. Runners hold **zero** Redis connections.

**The new binding constraint** is `agentsfleetd` API replicas + Postgres write throughput on the lease/report hot path — both horizontally scalable. Redis sees shared short-lived commands plus dedicated pub/sub and outbound-reader connections. The outbound worker blocks on its own socket for up to five seconds. Runners scale out with no Redis coordination at all.

**The idle Upstash bill** is no longer driven by N blocking `XREADGROUP` loops. It is driven by **runner lease-poll cadence**: each idle runner polls `lease` every `NO_WORK_RETRY_AFTER_MS` (1 s) and each empty poll checks the readiness index without scanning fleet streams. The knob is the poll backoff, not `XREADGROUP BLOCK`.

| What | Before (deleted) | Now |
|---|---|---|
| Per-fleet Redis connections | 1 dedicated blocking conn per fleet | 0 — `lease` uses a shared non-blocking read |
| Binding constraint | Upstash max-connections cap (~1/fleet) | `agentsfleetd` API replicas + Postgres write throughput |
| Idle request driver | `(fleets + workers) × (3600 / BLOCK_s)` | `runners × (3600 / poll_s)` |
| Idle-cost knob | `XREADGROUP BLOCK` | `NO_WORK_RETRY_AFTER_MS` (runner poll backoff) |
| Redis dedicated connections | per-fleet XREADGROUP + watcher + SSE | one SubscriptionHub connection and one outbound-reader connection per replica; neither scales with fleet or viewer count |

---

## The infra reality first

v2 ships hosted on Fly.io; the canonical Redis is **Upstash Redis**, accessed over Transport Layer Security (TLS) from every Fly machine. Three Upstash-specific properties still shape decisions, though the cutover changed which one binds:

1. **Plan-bound max-connections cap.** Each Upstash database has a hard concurrent-connection ceiling. New dials past the cap are refused. **After the cutover this no longer scales with fleet count** — and since the SubscriptionHub, not with viewer count either: only with the shared command, hub, and outbound-reader connections per replica. It is rarely the first wall now.
2. **Per-request pricing on Pay-as-you-go.** Every command is billable: `XADD`, the readiness `HRANDFIELD` (one per idle lease poll), `PUBLISH`, `XACK`, SSE acknowledgements. The idle bill is the lease-poll loop — see §"Per-request volume".
3. **TLS dial cost + regional round-trip-time (RTT).** Pool warm-up matters because each dial pays a TLS handshake. Regional vs Global database choice sets the floor on every round-trip.

---

## Event-delivery latency

The cutover added one hop (the runner lease request) and removed another (the in-process worker dispatch). End-to-end latency for a steer:

| Step | Typical cost (regional Upstash) |
|---|---|
| API handler receives steer / webhook | ~ms |
| API `XADD fleet:{id}:events` round-trip | ~3–10 ms (Upstash regional RTT) |
| Runner's next `lease` poll picks it up | up to the retry interval before another attempt, plus request and assignment time |
| `lease` handler: non-blocking `XREADGROUP` + gates + secret resolve + issue lease | ~ms + PG round-trips |
| Runner forks the sandboxed child, runs NullClaw | dominated by the fleet's own runtime |

An idle runner waits up to its retry interval before the next lease attempt.
That interval is not an end-to-end latency bound: assignment, available workers, and datastore work add delay.
Reducing `NO_WORK_RETRY_AFTER_MS` trades more readiness checks for shorter idle pickup waits.

**Live-tail cadence.** The runner batches activity frames per flush window — one POST per batch — but ships the first frame of a run and the first response chunk eagerly on arrival (at most two eager POSTs per run, one-shot latches in the runner's activity forwarder). The first visible token therefore pays a round-trip, not the staleness window, while the chatty middle of a run keeps the batch economics.

---

## Connection budget after the cutover

At normal boot, each `agentsfleetd` replica opens these Redis connections:

| Owner | Connection | Count per replica |
|---|---|---|
| `afd_redis::Redis` | shared multiplexed command connection | 1 |
| `SubscriptionHub` | dedicated pub/sub connection for all viewers | 1 |
| `connector:outbound` | dedicated command connection for blocking reads | 1 |

```text
R replicas * (1 shared + 1 hub + 1 outbound) = 3 * R Redis connections
```

This is a normal-operation count, not a bound on reconnect attempts or transient sockets.
A failed optional hub or outbound startup leaves fewer connections and reduced service.
Viewer count adds response buffers and fan-out work, while runners open no Redis connections.

Source: [`runtime boot`](../../rustd/crates/agentsfleetd/src/serve/runtime.rs),
[`Redis handle`](../../rustd/crates/afd_redis/src/client.rs), and
[`outbound worker boot`](../../rustd/crates/agentsfleetd/src/outbound.rs).

### Per-request volume (the Upstash bill)

Idle cost is the runner lease-poll loop, fully idle:

| Source | Requests per hour |
|---|---|
| Runner lease polls (`R_runners × 3600 / poll_seconds`), each doing **one** bounded `HRANDFIELD` read of the readiness index and **one** indexed `fleet.runners` auth read (M143_001 removed the memo — see below) | `R_runners × 3600` at the 1 s default |
| (No watcher loop, no per-fleet BLOCK loops — both deleted) | 0 |

"Zero Postgres round-trips" is about the CANDIDATE SCAN, not authentication. The
`agt_r` verdict (`sha256(token)` → runner row) is the one Postgres read left on an
idle poll after the candidate scan moved to the readiness index, and M143_001
removed the per-process memo that used to amortize it — so **every runner request
pays one indexed single-row read**, not one per heartbeat interval.

That was a deliberate trade, and the reasoning is in
[`../AUTH.md`](../AUTH.md) §"Runner token" — admin-state transitions have no delivery
channel other than auth rejection, so a per-process memo made revocation
deterministic only on the machine that served the operator's write. Reading the row
every time makes a cordon, drain, revoke, or delete effective fleet-wide the moment
it commits.

Sizing consequence: at the 1 s default the auth read tracks the lease-poll rate
above (`R_runners × 3600` per hour), against a `fleet.runners` table whose pages
stay resident. At the ~100 runners the sizing assumes, that is a few hundred index
probes a second. (`fleet.runners` is created by `schema/600_runners.sql` and
carries no separate index slot: the M154 rebuild retired the shared
`033_hot_path_indexes` and moved each index into the slot owning its table.) Revisit when runner count or poll rate makes
that measurable — AUTH.md records the replacement design (a short-lived signed
credential verified locally) and the condition it must meet.>>>>>>> origin/main

For a 20-runner fleet at the 1 s default: ~72,000 idle `lease` requests/hour. Doubling `NO_WORK_RETRY_AFTER_MS` to 2 s halves it; the trade is idle pickup latency, not event-delivery latency for a busy fleet. Active traffic (XADD ingress, PUBLISH activity ~5/event, XACK on report) sits on top, scaling with event throughput as before.

**The load-bearing shift:** the idle bill scales with **runner count**, not `(fleets + workers)` and not `runners × fleets`. A deployment with many idle fleets but few runners is cheap at idle; the cost follows the pollers, not the population.

> **This figure was wrong before M141, and wrong in the direction that hurts.** The table above previously claimed one bounded scan per idle poll, and the sizing procedure and Upstash estimate below were derived from that number. In reality `assign.listCandidates` carried no `LIMIT` and no workspace scope, so it returned **every active fleet platform-wide**, and because an idle fleet's affinity slot is uncontended the polling runner *won* each one — paying a Postgres claim, a prior-lease probe and a release (3 round-trips) plus a group-create and two stream reads (3 Redis commands) **per fleet, per poll**. Idle cost was therefore proportional to `runners × active_fleets`, and the published sizing understated it by exactly the fleet count. Operators who sized a deployment against the old figure saw Postgres connection pressure and lease latency climb as they added fleets, with no user traffic to explain it. M141 makes the table above true: readiness is recorded at the single ingress producer, and the lease consults it before opening a database connection.

### The per-poll bound, and why cost no longer tracks fleet count

An idle poll reads the readiness index and stops. A **busy** poll takes a randomized, server-bounded slice of that index and restricts the candidate query to it, capped at `MAX_READY_CANDIDATES_PER_POLL` (`rustd/crates/afd_fleet/src/lease/assign.rs`, on the same axis `NO_WORK_RETRY_AFTER_MS` trades).

That ceiling is what makes per-poll cost independent of the population, and it holds **even when the index is wrong**. A stale or over-marked index costs extra candidate checks up to the ceiling and never more, so a hint failure degrades discovery fairness rather than cost. The index is a hint; the streams stay the system of record. (The index mechanism — the `fleet:ready` hash, its token semantics, the sweeper — is canonical in [`runner_fleet.md` §Redis topology](./runner_fleet.md).)

Two consequences worth carrying into a sizing conversation:

- **Fleet count no longer appears in the idle term at all.** It appears only in the *recovery* term — see the readiness recovery bound in [`runner_fleet.md`](./runner_fleet.md) §"Failure recovery model", which scales with `active_fleets / sweep batch`.
- **Randomized sampling interacts with label placement.** The slice is drawn at random; the label gate (`required_tags <@ labels`) filters it in Postgres afterwards. A runner whose labels match only a small share of ready fleets may therefore need several polls to draw one it can serve. It is a latency effect, never a loss, and it self-corrects across polls; the ceiling is sized generously for exactly this reason. Label-aware placement is M85_001's concern, not a knob here.

Watch `agentsfleet_lease_poll_candidates_scanned_total / agentsfleet_lease_polls_total` for mean fan-out per poll, and `agentsfleet_lease_poll_db_roundtrips_total / agentsfleet_lease_polls_total` for mean database cost per poll. The denominator is not optional: without it a traffic increase and a fan-out regression look identical.

#### Which recurring Postgres reads are index-served

The Redis figures above are only half the idle bill. The other half is Postgres, and it used to scale with *accumulated rows* rather than with work — an account that had been running a year cost more at idle than a fresh one, for no reason a user asked for. Each table's own index slot closes that: every recurring control-plane read below is served by an index, and each index is asserted against the query **plan**, not merely created.

| Recurring read | Was | Now |
|---|---|---|
| Liveness sweep — due runners | Sequential scan + top-N sort of `fleet.runners`, every cycle | `idx_runners_updated_at_id`; no sort node. Measured at 20 000 runners: the 200-row batch costs **6 shared buffer hits**. |
| Liveness sweep — affinity expiry | Full scan of `fleet.runner_affinity`, once **per due runner per cycle** | `idx_runner_affinity_last_runner_id_leased_until`; also covers the unindexed `ON DELETE SET NULL` foreign key |
| Reclaim — prior active lease | Scan of `fleet.runner_leases` on an unindexed `ON DELETE CASCADE` foreign key | `idx_runner_leases_fleet_id_status_fencing_token`; filter, ordering and `LIMIT 1` in one seek |
| Workspace event keyset page | Index scan plus a post-filter on the tiebreak column | `idx_fleet_events_workspace_id_created_at_event_id`; a single seek |
| Fleet list page | Unserved — the existing index is partial on `status='active'` and the list is not status-filtered | `idx_fleets_workspace_id_created_at_id` |
| Runner and api-key list sorts | Sort node per request | One index per sort column; `tenant_id`/no leading filter means one btree serves both directions |

**What this bounds:** idle Postgres cost now tracks the *work* an account is doing, not the number of rows it has accumulated. It does not bound cost per unit of work — that is still governed by traffic.

**What it does not cover.** Fleet-memory hydration (`fleet_memory.listAll`) still sorts, and correctly so: it fetches a fleet's entire memory set with no `LIMIT`, and for an unbounded fetch a bitmap scan plus sort is genuinely cheaper than an ordered index scan with random heap access. An index only removes a sort where the plan can exit early.

**One anti-pattern this replaced,** worth stating because it reads as an optimisation: the operator runner list resolved each row's lease-liveness in a CTE spanning the whole runner table before paginating. PostgreSQL answers that by hashing the *entire* `runner_leases` table once per request — 6 472 buffer hits against a 200 000-row lease table. Resolving liveness over the surviving page instead costs 79. When per-row work sits above a `LIMIT`, the cost is not "one lookup per row"; it is usually one whole-table build.

---

## Tuneup knobs and when to turn them

| Knob | Default | What it scales with | Turn it when |
|---|---|---|---|
| `REDIS_REQUEST_TIMEOUT_MS` | 5000 | Upstash tail-latency tolerance | Upstash p99 round-trip exceeds 4 s under healthy traffic. **Do not raise it** — >5 s is failure, not slowness. |
| `REDIS_CONNECT_TIMEOUT_MS` | 5000 | Time to establish a connection, which `REDIS_REQUEST_TIMEOUT_MS` never covered — that knob bounds commands on a connection that already exists. | A dead or black-holing endpoint used to hold a boot preflight open past every deadline it declared; the budget now refuses it as `Unreachable`. Raise only where a TLS handshake to a distant region measurably exceeds 4 s. |
| `NO_WORK_RETRY_AFTER_MS` | 1000 | Idle lease-poll request volume (Upstash bill) **and** idle pickup latency. **Not busy-fleet delivery latency.** | Idle request bill is the dominant cost line on PAYG. Raise to 2000–5000 to cut the idle bill proportionally; idle pickup latency rises by the same factor. Single-sourced in `rustd/crates/afd_core/src/timing.rs`. |
| `MAX_READY_CANDIDATES_PER_POLL` | 64 | Per-poll fan-out ceiling: the most fleets one lease poll will examine, and the width of the randomized readiness slice. **Not** an idle-cost knob — an idle poll examines zero regardless. | Compile-time, not env-driven. Lower it only if `agentsfleet_lease_poll_candidates_scanned_total / agentsfleet_lease_polls_total` shows busy polls doing more per-fleet work than the hot path can absorb. Raise it if labelled runners are visibly slow to find their eligible fleets (a narrow slice plus a selective label gate — see §"Per-request volume"). In `rustd/crates/afd_fleet/src/lease/assign.rs`, on the same axis `NO_WORK_RETRY_AFTER_MS` trades: per-poll cost against discovery latency. |
| `LEASE_TTL_MS` | 30000 | Reclaim latency floor **and** the max single-fleet runtime before reclaim (the renewal gap) | Raise to cover the longest expected fleet runtime until M80_006 lands per-lease renewal (see `runner_fleet.md` Failure Recovery Model). Lower only with a tighter recovery requirement and short fleets. |
| `SSE_MAX_STREAMS` | 64 | Concurrent asynchronous SSE bodies per replica; shared by fleet and workspace tails. Zero is rejected at boot. | Raise only after measuring stream refusals, memory, CPU, and proxy capacity. Watch `agentsfleet_sse_in_flight_streams` and `agentsfleet_sse_backpressure_rejections_total`. |
| `DEFAULT_MAX_IN_FLIGHT` | 256 | Compiled API admission ceiling; excess API requests receive 429 with Retry-After. SSE has its own ceiling. | `serve.rs` passes this constant directly; there is no environment override in the Rust boot path. Measure admission refusals and datastore capacity before changing it. |
| `agentsfleetd` API replica count | deployment-driven | HTTP QPS (user surface + `/v1/runners`) + lease/report throughput + SSE fan-in | Lease/report p99 climbs, or per-replica viewer count keeps hitting the `SSE_MAX_STREAMS` ceiling. |
| Runner count | operator-driven | Compute throughput; idle lease-poll request volume | Add hosts to add execution capacity — no Redis or coordination cost. Each idle runner adds one poll loop to the Upstash bill (tune via `NO_WORK_RETRY_AFTER_MS`). |
| assigned `worker_count` (per-runner, dashboard) | 1 | Concurrent leased fleets **per host** — the runner worker-pool size (M88_002), assigned on the runner row (M148) and delivered with the heartbeat. N workers each run the lease→execute→report unit; the per-fleet `affinity.claim` keeps two workers off the same fleet. | A host has spare cores/memory while one long fleet run monopolises it (per-host throughput is fixed at 1 at the default). Raise N to run more fleets per host instead of enrolling more hosts. **Tradeoff:** N is a capacity knob, not a throughput guarantee (CPU/RAM/disk/network are not isolated across workers), and it **widens the failure domain** — one host loss drops N in-flight runs, not 1 (all re-leased by the M84_002 sweeper, but interrupted). `worker_count=1` is maximum isolation. |

The lease path never passes `BLOCK` to Redis.
The outbound reader uses `BLOCK_INTERVAL = 5000` ms on its dedicated socket, with cancellation raced against the read.
Its worker also checks pending deliveries; include those commands when measuring idle request volume.
Source: [`outbound worker`](../../rustd/crates/afd_outbound/src/worker.rs).

---

## Where the next ceiling actually lives

Once Redis connection count and request volume fit the plan, the next bottleneck is one of:

### 1. `agentsfleetd` API replicas + Postgres write throughput (the usual answer now)

The lease/report hot path does the durable writes the worker used to do — `INSERT fleet_events`, the two billing debits, `UPDATE` terminal, `INSERT telemetry`, checkpoint `UPSERT`, plus the `fleet.runner_leases`/`runner_affinity` bookkeeping. At fleet scale this is the binding axis. Both `agentsfleetd` replicas and Postgres (with a connection pooler) scale horizontally; the hot path is shardable per fleet.

Symptom: lease/report p99 climbs; Postgres connection saturation or write-lock contention on the `fleet` tables. Fix: more `agentsfleetd` replicas + Postgres sizing in the deployment runbook.

### 2. Pub/sub fan-out on activity (unchanged in shape)

Each replica's async hub receives one delivery per published frame on each subscribed channel.
A bounded broadcast queue holds 256 messages per channel and serves each subscriber independently.
A lagging reader receives `catching_up`; a closed hub ends a per-fleet tail.

Each SSE response owns a semaphore permit until its body is dropped.
The default `SSE_MAX_STREAMS` ceiling is 64 per replica; new streams receive 503 when full.
Waiting bodies do not reserve dedicated operating-system threads.

Measure response-buffer memory, serialization work, network throughput, and proxy connection limits under the expected viewer count.
HTTP/2 can share a socket across streams, but each HTTP hop negotiates independently.
See [Data Flow, D. WATCH](./data_flow.md#d-watch--user-side-how-the-live-tail-surfaces) for the browser proxy path and protocol limits.

Source: [`stream ceiling`](../../rustd/crates/afd_sse/src/ceiling.rs),
[`tail`](../../rustd/crates/afd_sse/src/tail.rs), and
[`hub`](../../rustd/crates/afd_redis/src/hub.rs).

### 3. Upstash plan ceiling (now rarely first)

Max concurrent connections, requests/sec, or daily request quota — whichever the plan tier defines first. After the cutover (and the SubscriptionHub) the normal connection count is `3·R`, far below the pre-cutover `~fleets`. The request axis is the runner poll loop. Check current plan limits before sizing; the binding axis is usually #1 now, not this.

---

## Measured ceilings

Numbers, not estimates. Each row is what a lane in `rustd/crates/afd_bench`
measured on one developer machine against a freshly reset compose Postgres and
Redis — `make bench-<lane> PROFILE=rig` — and the committed result sits beside
it in `bench/baselines/`. Absolute rates move with hardware; the shapes below
do not. Every row was re-measured after the pre-landing review found the first
lease numbers tail-dominated, and a baseline that moves takes its row here with
it in the same commit.

| Path | What it costs | The number that decides |
|------|---------------|-------------------------|
| Idle lease poll | 1.00 Redis command, 0 Postgres round trips (61 562 polls, index depth 0) | Idle cost scales with runners, not fleets. A million idle fleets add nothing to it. |
| Contended lease | 79.8 leases/s; 36.6 Postgres round trips per issued lease; 6.1% of polls find nothing; p95 175 ms (200 ready, 8 runners, pool 20, window ended at exhaustion) | The candidate loop tries up to 64 fleets in turn, so a lease costs tens of round trips under contention. This is the refactor's target. |
| Steer ingress | 2.0003 Redis commands per steer, 0.0007 Postgres transactions; 14 435/s at p95 0.74 ms (8 submitters, 50 fleets) | Ingress never reaches Postgres. The readiness index fills to the population and holds until a runner drains it. |
| Delivery, healthy | 50.0 jobs/s per worker with one 250 ms destination in sixteen | Ten times the five-per-second estimate the refactor argument was made from — but see the next row. |
| Delivery, head-of-line | the OTHER fifteen destinations' p95 3 719 ms against the slow one's 3 731 ms | With 6% of jobs slow, the healthy 94% wait exactly as long. One stream, one worker, one queue position at a time. |
| Delivery, retry | 96.7% of the window in the ladder with two refusing destinations in sixteen | Eight jobs that never resolve cost every job behind them the whole ladder. |
| Cardinality | 4.6 KB of Redis per idle fleet, flat from 10 to 10 000 (4 616 / 4 474 / 4 659 / 4 647 B); peek 0.27–0.36 ms, stream read 0.19–0.23 ms, candidate query 1.06 ms at 10 000 | Linear. A million idle fleets is roughly 4.6 GB of Redis and no slower a hot path. |

Two of those rows change what the section below assumes. The idle row says the
per-poll bound holds all the way up: cost tracks runner count and never fleet
count, which is what makes the "idle deployment" line in the sizing procedure a
measurement rather than an argument. The head-of-line row says the delivery
worker's ceiling is not its rate but its ORDERING — a single slow vendor sets
the latency for every vendor — and that is a shape a replica count cannot fix.

What is deliberately not here: a production number, because nothing is
deployed; behaviour under real customer traffic, because there is none yet; and
any threshold, because the lanes print a delta against their baseline and never
fail a build on it. The rig measures one process against local datastores,
which is a floor and not a capacity plan.

## Sizing procedure (fleet- and playbook-readable)

Structured for an LLM fleet or a `agentsfleet`-driven scaling playbook. Each step has explicit inputs, a formula, a decision rule, and an emit target.

### Inputs

| Symbol | Meaning | Source |
|---|---|---|
| `Z` | Target fleet count (active + idle) | Product / fleet plan |
| `N` | Runner host count | Operator / capacity plan |
| `R` | `agentsfleetd` API replica count | Deployment plan |
| `S` | Peak concurrent SSE tails | Product / dashboard usage |
| `P_conn` | Upstash plan max-concurrent-connections cap | Upstash plan docs (current) |
| `P_rps` | Upstash plan requests/sec cap (or ∞ on PAYG) | Upstash plan docs |
| `poll_s` | Runner idle poll interval = `NO_WORK_RETRY_AFTER_MS / 1000` | Config |

### Procedure

```
Step 1: Redis connection budget (no per-fleet, no per-viewer term)
  redis_conns = R * 3       (shared commands + hub + outbound reader)
  ASSERT redis_conns + measured_reconnect_headroom <= P_conn
    if violated → increase datastore connection capacity
  NOTE: Z and S do not appear in the steady-state Redis socket count.
        S still sizes SSE_MAX_STREAMS, response memory, and HTTP capacity.

Step 2: Idle Upstash request rate (the lease-poll loop)
  idle_rps = N / poll_s
  ASSERT idle_rps <= P_rps        (only meaningful on capped plans)
    if violated → raise NO_WORK_RETRY_AFTER_MS (2000, 5000) and re-evaluate

Step 3: Hot-path throughput (the real wall)
  lease_report_qps = peak events/sec across the fleet
  size R + Postgres so lease/report p99 stays within budget under lease_report_qps
  (PG connection pooler assumed; sizing in the deployment runbook)

Step 4: Emit configuration
  REDIS_REQUEST_TIMEOUT_MS = 5000    (do not raise)
  REDIS_CONNECT_TIMEOUT_MS = 5000    (raise only for a measured cross-region handshake)
  NO_WORK_RETRY_AFTER_MS   = <step 2 result>
  LEASE_TTL_MS             = <≥ max expected fleet runtime until M80_006>
  agentsfleetd_replicas         = <step 3 result>
  runner_hosts             = N
```

### Anti-patterns (do NOT do these)

1. **Size Redis connections by fleet or viewer count.** There is no per-fleet and no per-viewer connection. Normal boot opens `3·R` connections: shared commands, hub, and outbound reader.
2. **Tune `XREADGROUP BLOCK`.** It no longer exists on the hot path. Use `NO_WORK_RETRY_AFTER_MS` for the idle-cost/latency trade.
3. **Add runners to fix lease/report latency.** Runners add compute, not control-plane throughput. Scale `agentsfleetd` replicas + Postgres for hot-path latency.
4. **Raise `REDIS_REQUEST_TIMEOUT_MS` above 5000.** Upstash regional p99 is single-digit-ms; >5 s is failure, not slowness.
5. **Put `SUBSCRIBE` on the shared command socket.** The hub owns a separate connection and fans out locally; request-path commands keep their multiplexed socket.
6. **Treat SSE streams as dedicated threads.** Rust serves async bodies under `SSE_MAX_STREAMS`; API admission and viewer capacity are separate budgets.
7. **Include fleet count in the idle term.** It is not there any more. An idle poll costs one Redis read and no database work regardless of how many fleets exist; fleet count appears only in the readiness *recovery* bound ([`runner_fleet.md`](./runner_fleet.md) §"Failure recovery model"). Sizing an idle deployment by fleet population is the pre-M141 mistake, and it is the reason the idle figure in §"Per-request volume" used to be wrong.
8. **Reach for a bare `LIMIT` on the candidate scan.** It was considered and rejected: a bare limit caps discovery throughput without removing the per-poll Postgres cost, and it silently starves every fleet past the bound because the scan is ordered, not sampled. The ceiling only works *because* the readiness slice above it is randomized.
9. **Sum `agentsfleet_fleet_ready_depth` across replicas.** Every replica samples the same shared hash, so the fleet-wide value is any single instance's series. Summing multiplies it by replica count.

---

## Failure and rebalance behavior

### Runner host loss

A runner that dies holds no datastore connection to leak and no Redis consumer to reclaim. Its in-flight lease expires at `lease_expires_at`; the next runner's `lease` reclaim path re-issues the event with a higher fencing token (see `runner_fleet.md` Failure Recovery Model). Recovery latency is `LEASE_TTL_MS` + poll density — the S0 lazy-reclaim SLA. There is **no connection storm** on runner loss — the survivors just keep polling.

**Failure domain scales with the assigned `worker_count`.** A host running a pool of N concurrent leases (M88_002) drops **N** in-flight runs on loss, not one. Each lease expires and re-leases independently (no batch coupling), so no work is lost — but N runs restart instead of one. This is the cost of the per-host utilization win; `worker_count=1` keeps the failure domain at one run. Operators size N against this tradeoff.

### Runner host add

A new runner registers and starts polling `lease`. No rebalance of in-flight work, no Redis connection migration, no coordination. Sticky routing prefers the runner that ran the previous run but never blocks on it.

### Upstash failover (provider-side primary swap)

The shared and outbound command managers reconnect after connection loss.
The hub retries with backoff and reissues subscriptions for channels that still have readers.
Frames published while Redis pub/sub is disconnected are lost; an open HTTP stream can conceal that gap.

Reconnect attempts are not bounded by the number of sockets.
Measure provider-specific failover and command-error behavior on the intended deployment before claiming recovery guarantees.

---

## What is explicitly out of scope here

- **Adaptive Redis pooling.** The Rust command path uses a multiplexed connection, so the retired pool knobs do not apply.
- **Datastore cutover.** Redis remains the default. [Datastore scaling](./datastore_scaling.md) defines the required Dragonfly Cloud Swarm target and deployment proof.
- **Postgres scaling.** Pgbouncer + plan sizing covered in the deployment runbook, not here — though after the cutover it is the **primary** scaling axis, so the runbook carries more weight than it did.
- **Placement / scheduler.** Label-aware assignment (`required_tags ⊆ runner.labels`) is M85_001; capacity-aware placement and autoscale-by-queue-depth stay out of scope (the non-goals fence in runner_fleet.md).
- **Multi-region topology.** Follow [datastore scaling](./datastore_scaling.md); provider-specific latency and limits require measurements on the selected deployment.
