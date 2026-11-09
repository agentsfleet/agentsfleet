# Scaling and tuneup — how the runtime grows after the cutover

> Parent: [`README.md`](./README.md) · Companions: [`data_flow.md`](./data_flow.md) §"Connection topology", [`runner_fleet.md`](./runner_fleet.md) §"Scaling".

> [!IMPORTANT]
> **Scope:** size the ledger queue, pushed runner sessions, database work and the shared NATS connection. Historical stream measurements are marked as comparisons.

Read this when you need to size a deployment or pick env-var values. It also tells you whether the next bottleneck is `agentsfleetd` API replicas, Postgres, the datastore machine, or runner fan-out.

---

## Facts

Every row is extracted from the sections below; the owner column names the section that carries the full story.

| Invariant | Value | Mechanism | Owner section |
|---|---|---|---|
| Bus connections | R at boot | one NATS connection per replica, shared by publishers, the hub and the outbound worker | §Connection budget after the cutover |
| Idle discovery volume | One query per replica every five seconds | Daemon recovery probe; runners send no lease requests | §Idle request volume |
| Recovery interval | `OFFER_FALLBACK_TICK` = 5 seconds | Recovers missed disposable wake notifications | §Idle request volume |
| Candidate result ceiling | `MAX_CANDIDATES_PER_POLL` = 64 | bounds returned fleet identifiers and claim attempts; discovery still examines eligible active fleets | §Candidate and claim bounds |
| Auth read per request | one indexed single-row read | M143_001 removed the per-process memo, so cordon/drain/revoke bite fleet-wide the moment they commit | §Idle request volume |
| Fleet count in discovery work | eligible active fleets when unsettled admissions exist | an empty ledger skips fleet scans; otherwise one oldest-unsettled lookup per eligible fleet | §Candidate and claim bounds |
| Server-Sent Events (SSE) ceiling | `SSE_MAX_STREAMS` = 256 per replica, bound by file descriptors | async response bodies own semaphore permits; 503 at the cap; hub connection shared | §Tuneup knobs, §2 |
| Bus publish deadline | `publish_deadline_ms` = 250 (`afd_bus::nats::Tuning`) | a publish or flush that misses it fails and is owed a gap marker; an outage costs a caller this, not a hang | §Tuneup knobs |
| Bus connect timeout | `connect_timeout_ms` = 5000 (`afd_bus::nats::Tuning`) | a seed that does not answer inside it refuses boot as `UZ-STARTUP-004` | §Tuneup knobs |
| Lease TTL | `LEASE_TTL` = 35000 | reclaim latency floor; renewal decouples run length from it | §Tuneup knobs |
| Per-host concurrency | assigned `worker_count` = 1 (dashboard, per runner) | a capacity knob that widens the failure domain to N in-flight runs on host loss | §Tuneup knobs, §Runner host loss |
| Admission ceiling | `DEFAULT_MAX_IN_FLIGHT` = 256, api-class only | compiled default; operational and stream routes use separate handling | §Tuneup knobs |
| The binding constraint | `agentsfleetd` replicas + Postgres writes | both horizontally scalable; the hot path is shardable per fleet | §Where the next ceiling actually lives |
| Recurring-read indexes | one slot per table, plan-asserted | idle Postgres cost tracks work, not accumulated rows; runner sorts stay unindexed on purpose, because runners number about a hundred | §Which recurring Postgres reads are index-served |
| Missed-wake recovery | Next five-second probe, then normal claim work | Healthy notifications trigger offers immediately; no end-to-end delay guarantee | §Event-delivery latency |
| Outbound-answer worker | claims on a ping, a finished lane, or every 30 s | one lane-head claim statement per wake; no connection of its own | §Tuneup knobs |
| Bus node loss | one client reconnect per replica | the client reconnects to another seed; every local subscription receives one `Gap` | §Datastore failover |

## Traps

The ten sizing anti-patterns are the trap list. Read §Anti-patterns (do NOT do these) before any sizing conversation. Three more live elsewhere:

- Statement count alone hides server work: measure discovery latency at the intended active fleet count (§Idle request volume).
- Metric ratios need their denominator: without `agentsfleet_lease_polls_total`, a claim increase and a fan-out regression look identical (§Candidate and claim bounds).
- Fleet-memory load still sorts, and correctly so. An index only removes a sort where the plan can exit early (§Which recurring Postgres reads are index-served).

## Topology

No standalone diagram; the sizing procedure block in §"Sizing procedure" is the operational record, with inputs, formulas, and emit targets.

## Decisions

| Decision | Reason | Where |
|---|---|---|
| Auth memo removed; the daemon reads the runner row every request | revocation must be deterministic fleet-wide, not per-machine | §Idle request volume; M143_001, [`../AUTH.md`](../AUTH.md) §"Runner token" |
| Commit the admission before notification | lost notifications cannot lose accepted work | §Idle request volume |
| Randomize candidates after notification and affinity priority | spread competing polls across eligible fleets; no strict fairness bound | §Candidate and claim bounds |
| Async SSE bodies share one native subscription per subject | waiting for frames does not reserve an operating-system thread per viewer | §2; `afd_sse` |
| One NATS connection per process | publishers and subscribers share it; the hub fans out locally | §Connection budget after the cutover; M244 |

---

## Detail

Everything below is the full reference. Headings are stable because specs cite them by text; insert new sections, never rename existing ones.

## What binds this deployment

The per-fleet dedicated datastore connection is gone. An offer pass reads Postgres and never waits on the bus.

The binding constraint is now `agentsfleetd` API replicas plus Postgres write throughput on the lease/report hot path, both horizontally scalable.
Idle database cost follows the **replica count**, one recovery probe per replica every 5 s, rather than fleet population.

Every current number is in the Facts table at the top of this page.

## The infra reality first

v2 ships hosted on Fly.io. Postgres is the only datastore, and the message bus is **self-hosted core NATS** with JetStream off.

`nats-prod` runs three `shared-cpu-2x` 1 GB machines and `nats-dev` two `shared-cpu-1x` 512 MB machines. Both sit in `iad` with no volume (`deploy/fly/nats-*/fly.toml`, `NATS_MACHINES` in the deploy workflows).

The daemon reaches them over Fly's private network (6PN). `datastore_scaling.md` is canonical for the target and wins over this page where the two disagree.

**Three properties of the hosted service are gone.** They were three of the five things this page used to size against. A reader who sized a deployment last quarter sized it against them, so they are recorded here:

- **The plan-bound max-connections cap is gone.** There is no plan. The ceiling is now the machine's file-descriptor limit and the NATS server's own accept path. Both sit orders of magnitude above anything the connection budget below asks for.
- **Per-request pricing is gone**. Message volume consumes CPU on the NATS machines, and the fleet scheduler's idle query consumes Postgres capacity. The section "Idle request volume" still matters, but it now sizes a **machine**, not an invoice.
- **The Transport Layer Security (TLS) dial cost is gone in-region**. The 6PN network is WireGuard-encrypted already, so the seed is plaintext and a dial pays no handshake. That is worth roughly 230 ms per connection against an RSA-2048 leaf.

   The local rig runs plaintext too.

**A fixed machine replaces them, with a different failure shape.** A hosted plan degrades by refusing a dial or sending a bill. A machine degrades by running out of memory or CPU. Core NATS stores no messages, so a node's memory does not grow with accepted work.

Postgres is **PlanetScale**, and two of its properties bind the same way:

1. **Per-cluster `max_connections`.** A conservative default that varies with cluster size (Clusters → Parameters); the dev cluster reports 25 (`deploy/fly/agentsfleetd-dev/fly.toml`). Every replica's `DATABASE_POOL_SIZE` (default 20) plus the migrator during a release must fit inside it.

   A connection asked for past the ceiling is refused with `53300`, which sqlx retries silently until the acquire budget runs out. The daemon can therefore only report it as a **stall** ("the pool held N of M and could not open another"). It retries the acquire once, then answers `UZ-INTERNAL-001`; it is not an outage and must not be paged as one.

   The request path is therefore meant to run through PlanetScale's PgBouncer (port `6432`, transaction pooling). It multiplexes replicas' connections onto one bounded server pool. The deployment preflight refuses an `api-connection-string` on any other port, and the migrator stays on `5432` for its session advisory lock.

   PgBouncer's own `default_pool_size` (20) counts against that same ceiling once, beside PlanetScale's admin and exporter backends and the migrator at release. Set it low enough in the console that all four fit.
2. **Region**: production's database sits in `us-east` beside Fly `iad`. Development's sits in `aws-us-east-2` (Ohio), a round trip away on every handshake and every query. A region cannot be changed in place; `playbooks/operations/database_region_move/` moves it.

---

## Event-delivery latency

A committed steer wakes the gateway, which pushes an offer to an eligible connected runner:

| Step | Work on the serving path |
|---|---|
| API receives a steer or webhook | Validate identity, access and backlog budget |
| API accepts the event | Commit `fleet.admissions`, then publish a best-effort `af.wake.<shard>` notification |
| Gateway handles a wake or available capacity | Claim an eligible ledger head and its fleet slot, then push the lease over the native session |
| Runner runs the lease | Prepare its sandbox and run the fleet instructions |

Healthy notifications start an offer pass immediately.
Available capacity, gates and datastore work determine when the runner can start.
A missed wake is recovered by the next per-replica five-second database probe.

**Live-tail cadence.** The runner attempts immediate posting for a lease's first frame and first response chunk.
Later frames batch toward 64 KiB; partial batches flush every 250 ms.
The first response does not wait for that timer when the posting queue has room.

Four batches may be queued before backpressure delays posting.
A single oversized frame rides alone.
These limits do not bound browser paint time.

The constants live in `afr_supervisor`'s activity module: `Early`, `MAX_BATCH_BYTES`, `FLUSH_EVERY` and `MAX_BATCHES_HELD`.

---

## Connection budget after the cutover

Each daemon replica holds one NATS connection, opened by `afd_bus::nats::connect` (`rustd/crates/daemon/main/src/serve/runtime/mod.rs`).
Publishers, the hub's subscriptions, the wake listeners and the outbound worker all share it.

Fleet and viewer counts add no connections: the hub keeps one native subscription per subject with local readers. [Data flow](data_flow.md#connection-topology-the-cutover-collapsed-the-dedicated-tier) owns the connection topology.

`afd_bus::nats` owns the client and the hub; the daemon's `serve` runtime opens it and shuts it down.

### Idle request volume

Runners keep one native session per host and send no idle lease requests. Each host announces free capacity as credits ([push scheduler](push_scheduler.md)).
The gateway reacts to wake notifications and increased capacity.

A separate daemon recovery timer queries Postgres once every five seconds per replica (`OFFER_FALLBACK_TICK`), even when no work is waiting.
The probe is one statement, `select_waiting_admission.sql` (`afd_gateway/src/scheduler/recovery/mod.rs`).
For `R` replicas, that is `R / 5` discovery statements per second.

This backstop repairs missed disposable notifications; it remains database polling.
A probe that finds pending work starts an offer pass. Wake traffic never resets the probe's timer.

A wake notification starts an offer pass at once. Admission and gate-answer commits publish on `af.wake.<shard>` (`afd_admission/src/wake.rs`, `afd_approval/src/inbox/resolve/mod.rs`).

The pass walks the ready streams that hold credits, those holding a woken fleet first. It claims from Postgres for each stream until a claim comes back empty or the pass spends its budget (`afd_gateway/src/scheduler/mod.rs`).

The session authenticates at opening and rechecks current authority at heartbeat and grant boundaries.
See the [runner guide](runner_protocol.md) for connection ownership, cancellation and timing.

Prod runs three replicas, so an idle prod makes 36 probe statements a minute however many runner hosts connect.

Claim cost still depends on eligible fleet population and indexed admission heads.
An empty ledger skips fleet scans.

When unsettled admissions exist, server work grows with eligible active fleet population, including when all heads are parked.
Measure that cost separately from the number of statements.

### Candidate and claim bounds

Discovery returns at most `MAX_CANDIDATES_PER_POLL` (64) fleet identifiers, declared in `afd_fleet`'s lease assignment module.
The query first checks for any unsettled admission. It then checks placement labels and the oldest unsettled admission of each eligible fleet before applying that limit.

The partial `(fleet_id, sequence)` admission index serves each head lookup; the limit bounds returned candidates and subsequent claim attempts.
It does not bound the number of active fleets examined inside Postgres.

An isolated plan probe with 20,001 active fleets and no admissions performed zero fleet scans after the empty-ledger check.
The single local samples took 19.702 ms before and 0.079 ms after. These are query-plan observations, not throughput or chat-latency measurements.

The real-database test in `afd_fleet`'s lease-plan suite asserts that every fleet scan has zero actual loops.

The process prioritizes recent notification hints, held fleets and previous holders, then randomizes the rest.
An absent or stale notification cannot strand work, because the 5 s recovery probe still queries Postgres.
A parked oldest admission blocks later admissions while standing access awaits an answer or an event approval stays within its deadline.

The partial `(fleet_id, event_id, expires_at)` gate index serves both probes over unresolved rows.
The approval-history plan test grows resolved, expired and unrelated gates from 2,000 to 16,000 rows. It runs both statements through `EXPLAIN (ANALYZE, BUFFERS)` with custom and generic prepared plans.

It checks fleet and event index bounds, deadline bounds for event approvals, and early stopping when many gates match. Total discovery still grows with the eligible fleet population.

The claim locks the slot and exact head, stamps their fence, and commits before returning it.
An uncontended successful claim takes discovery, begin, the claim statement and commit; contended attempts add work.

The historical `agentsfleet_lease_polls_total` counter now counts gateway claim attempts.
Divide the candidate counter by those attempts to measure claim fan-out.

Statement cost comes from `pg_stat_statements` in `make bench-lease`, which counts what every crate sends.
Use query latency and query plans alongside these ratios: one slow discovery statement still counts as one statement.

#### Which recurring Postgres reads are index-served

Bus traffic is only part of the idle bill. The larger part is Postgres, and it used to scale with *accumulated rows* rather than with work. An account that had been running a year cost more at idle than a fresh one, for no reason a user asked for.

Each table's own index slot closes that. Every recurring control-plane read below is served by an index, except the runner sorts left unindexed on purpose. Each index is asserted against the query **plan**, not merely created.

| Recurring read | Was | Now |
|---|---|---|
| Liveness sweep, due runners | Sequential scan + top-N sort of `runner.runners`, every cycle | Still a sort, on purpose: `afd_runner`'s `SELECT_DUE_RUNNERS` orders by `updated_at, id` under its batch `LIMIT`, and slot 1800 leaves runner sorts unindexed because runners number about a hundred. M142_001's `idx_runners_updated_at_id` measured 6 shared buffer hits at 20 000 runners; the M154 rebuild dropped it. |
| Liveness sweep, affinity expiry | Full scan of `runner.fleet_slots`, once **per due runner per cycle** | `idx_fleet_slots_last_runner_id_lease_expires_at`; also covers the unindexed `ON DELETE SET NULL` foreign key |
| Reclaim, prior active lease | Scan of `runner.leases` on an unindexed `ON DELETE CASCADE` foreign key | `idx_leases_fleet_id_status_fencing_token`; filter, ordering and `LIMIT 1` in one seek |
| Workspace event keyset page | Index scan plus a post-filter on the tiebreak column; under a cached generic plan the cursor and `since` fell behind their `IS NULL` guards into filters | One text per scope, cursor and actor filter; `idx_events_workspace_id_created_at_event_id` with the workspace, cursor and `since` in its index condition, walked in order |
| Fleet event page and chat thread | A cached generic plan multiplied the workspace and fleet selectivities, expected one row, and read the fleet's whole history into a bitmap scan plus sort | `idx_events_fleet_id_created_at_event_id` walked in order, because `stx_events_workspace_id_fleet_id` (slot 1700) tells the planner the fleet implies the workspace. An actor-filtered page still sorts its matches |
| Fleet list page | Unserved: the existing index is partial on `status='active'` and the list is not status-filtered | `idx_fleets_workspace_id_created_at_id` |
| Runner and api-key list sorts | Sort node per request | Still a sort node: the M154 rebuild left runner list sorts unindexed (slot 1800), and `identity.api_keys` keeps only `idx_api_keys_tenant_id_live` (slot 1150) |

**Queue discovery:** the unsettled partial index avoids completed admission history. Each probe still examines eligible active fleets; size Postgres against both fleet count and poll rate.

**What it does not cover.** Fleet-memory load (`afd_memory`'s `SELECT_ALL_FOR_FLEET`) still sorts, and correctly so. It fetches a fleet's entire memory set with no `LIMIT`.

For an unbounded fetch, a bitmap scan plus sort beats an ordered index scan with random heap access. An index only removes a sort where the plan can exit early.

**One anti-pattern this replaced,** worth stating because it reads as an optimisation. The operator runner list resolved each row's lease-liveness in a common table expression (CTE) spanning the whole runner table before paginating. PostgreSQL answers that by hashing the *entire* `runner.leases` table once per request: 6 472 buffer hits against a 200 000-row lease table.

Resolving liveness over the surviving page instead costs 79. When per-row work sits above a `LIMIT`, the cost is usually one whole-table build, not one lookup per row.

---

## Tuneup knobs and when to turn them

| Knob | Default | What it scales with | Turn it when |
|---|---|---|---|
| `publish_deadline_ms` (`afd_bus::nats::Tuning`) | 250 | What a bus outage costs each publisher: a publish or flush that misses it fails and is owed a gap marker. | A compiled default today: no environment variable or file sets it yet (`Config::with_tuning` exists for one). Raising it buys no delivery, because the marker recovers the frame. |
| `connect_timeout_ms` (`afd_bus::nats::Tuning`) | 5000 | Time for a seed to answer at boot; a seed that misses it refuses boot as `UZ-STARTUP-004`. | A compiled default, as above. Raise only where a seed is measurably slower to answer than 4 s. |
| `OFFER_FALLBACK_TICK` | 5 seconds | One recovery query per daemon replica | Compiled recovery interval; measure missed-wake latency and database cost before changing it. |
| `MAX_CANDIDATES_PER_POLL` | 64 | Maximum candidates returned by Postgres discovery and attempted by one claim. The statement still probes eligible active fleets before applying this limit. | Compile-time in `afd_fleet`'s lease assignment module. Measure discovery latency and candidate attempts before changing it; increasing it also increases contention work per claim. |
| `LEASE_TTL` | 35000 | Reclaim latency floor **and** the increment each renewal adds. Fleet renewal runs every 5 s, up to `MAX_RUNTIME` (`afr_supervisor`'s renew module and `af_core`'s timing module). | Compile-time. Run length does not size it, because renewal carries long runs. Lower only with a tighter recovery requirement. |
| `SSE_MAX_STREAMS` | 256 | Concurrent asynchronous SSE bodies per replica; shared by fleet, workspace and draft-progress tails. Zero is rejected at boot. Each stream holds a socket. The daemon neither raises `RLIMIT_NOFILE` nor checks the knob against it. About 300 descriptors go to admitted requests, Postgres and NATS first, so a stock 1,024 soft limit leaves room for 256. | Raise only on a host whose descriptor limit is raised to match, after measuring stream refusals, memory, CPU, and proxy capacity. The tail itself held 4,096 streams inside every bound (below). Watch `agentsfleet_sse_in_flight_streams` and `agentsfleet_sse_backpressure_rejections_total`. |
| `DEFAULT_MAX_IN_FLIGHT` | 256 | Compiled API admission ceiling; excess API requests receive 429 with Retry-After. SSE has its own ceiling. | The daemon's `serve` boot passes this constant (declared in `afd_http`'s admission module) directly; there is no environment override in the Rust boot path. Measure admission refusals and datastore capacity before changing it. |
| `agentsfleetd` API replica count | deployment-driven | HTTP requests per second (user surface + `/v1/runners`) + lease/report throughput + SSE fan-in | Lease/report p99 climbs, or per-replica viewer count keeps hitting the `SSE_MAX_STREAMS` ceiling. |
| Runner count | operator-driven | Compute throughput and native session count | Add hosts for available compute; size daemon memory and stream permits for their sessions. |
| assigned `worker_count` (per-runner, dashboard) | 1 | Concurrent leased fleets **per host**: the runner worker-pool size (M88_002), assigned on the runner row (M148) and delivered with the heartbeat. N workers each run the lease→run→report unit; the per-fleet `affinity.claim` keeps two workers off the same fleet. | A host has spare cores/memory while one long fleet run monopolises it (per-host throughput is fixed at 1 at the default). Raise N to run more fleets per host instead of enrolling more hosts. **Tradeoff:** N is a capacity knob, not a throughput guarantee (CPU/RAM/disk/network are not isolated across workers), and it **widens the failure domain**: one host loss drops N in-flight runs, not 1 (all re-leased by the M84_002 sweeper, but interrupted). `worker_count=1` is maximum isolation. At the ceiling (`MAX_WORKERS` = 64 in `af_core`'s limits module) and the 2-core sandbox default, any host under 128 cores is oversubscribed. |

The lease path never waits on the bus.
The outbound worker, owned by `afd_outbound`, sleeps until a ping, a finished lane or its 30 s `CLAIM_PASS`.
An idle replica therefore runs one claim statement every 30 s; include it when measuring idle request volume.

---

## Where the next ceiling actually lives

Once bus traffic and Postgres request volume fit their machines, the next bottleneck is one of:

### 1. `agentsfleetd` API replicas + Postgres write throughput (the usual answer now)

The lease/report hot path does the durable writes the worker used to do. Those are `INSERT fleet.events`, the usage logged on each `/renew`, the terminal `UPDATE`, `INSERT telemetry` and the checkpoint `UPSERT`. The `runner.leases`/`runner.fleet_slots` bookkeeping rides the same path, and the biller settles the usage afterwards.

At fleet scale this is the binding axis. Both `agentsfleetd` replicas and Postgres (with a connection pooler) scale horizontally; the hot path is shardable per fleet.

Symptom: lease/report p99 climbs; Postgres connection saturation or write-lock contention on the `fleet` tables. Fix: more `agentsfleetd` replicas + Postgres sizing in the deployment runbook.

### 2. Pub/sub fan-out on activity (unchanged in shape)

Each replica's hub receives one delivery per published frame on each subject it subscribes.
A bounded broadcast queue holds 256 messages per subject and serves each local reader independently.
A lagging reader receives `catching_up`; a closed bus ends a per-fleet tail, and the viewer reconnects to a live replica.

Each SSE response owns a semaphore permit until its body is dropped.
The default `SSE_MAX_STREAMS` ceiling is 256 per replica; new streams receive 503 when full.
Waiting bodies do not reserve dedicated operating-system threads.

Measure response-buffer memory, serialization work, network throughput, and proxy connection limits under the expected viewer count.
HTTP/2 can share a socket across streams, but each HTTP hop negotiates independently.

See [Data Flow, D. WATCH](./data_flow.md#d-watch--user-side-how-the-live-tail-surfaces) for the browser proxy path and protocol limits.
`afd_sse` owns the stream ceiling and the tail; `afd_bus::nats` owns the hub.

### 3. Datastore machine capacity (now rarely first)

Size the NATS machines for connections, subscriptions and message rate. Each daemon replica holds one connection; see [Connection budget](#connection-budget-after-the-cutover).

NATS stores nothing, so its memory tracks live subscriptions and messages in flight rather than accepted work. Measure it alongside database work and poll traffic.

---

## Measured ceilings

The stream queue rows below are historical comparisons. Current ledger measurements follow the table.

Numbers, not estimates. Each row is what a lane in `afd_bench` measured on one developer machine against freshly reset compose datastores (`make bench-<lane> PROFILE=rig`). Rows measured before 2026-11-09 predate core NATS.

The committed result sits beside each row in `bench/baselines/`.

Absolute rates move with hardware; the shapes below do not. A baseline that moves takes its row here with it in the same commit.

The steer and delivery rows were re-measured at `774f99cbd`.
The lease rows quote `lease.rig.json` as committed at `c82badcb5`, and the M241 baselines below replace that file.
The live-tail rows come from `e2d250172`, the revision the tail lane was built on.

Latency columns moved with host load between runs on this shared machine; the counts beside them did not.

The saturation row comes from paired runs on Oct 21, 2026. The two sets differ only in the claim code.

Statement and commit counts are Postgres's own: `pg_stat_statements` calls (transaction control left out) and `pg_stat_database.xact_commit` less session startups. Both are read after every pooled connection has been made to flush. The compose Postgres preloads `pg_stat_statements` for this; `afd_bench` documents why each correction is there.

| Path | What it costs | The number that decides |
|------|---------------|-------------------------|
| Stream comparison: contended lease | 1 928 leases/s; 2.4 Postgres round trips per issued lease by the poll's own tally, which M241_001 found undercounts; 27.8% of polls find nothing; p95 7.8 ms (200 ready, 8 runners, pool 20, window ended at exhaustion) | A lease costs about two round trips under contention, down from 11.3 at `e2d250172`. |
| Stream comparison: lease through report (drain) | 50.0 Postgres statements and 26.8 commits per lease issued, counting the polls that missed and the report. Throughput was 172 leases/s, with lease p95 56.5 ms and report p95 21.5 ms. All 200 events processed once, 2 ledger rows each (200 fleets, 8 runners, the drain half of `make bench-lease`), measured before M225. An interim M225_001 build, since replaced by the wake ring, measured lease p95 48.1 ms, report p95 16.9 ms and 176.9 leases/s. The wake-ring build is not yet re-measured. | What one event costs the scheduler and the ledger end to end through `Plane::lease` and `Plane::report`, counted by Postgres rather than by the lane. |
| Stream comparison: steer ingress | 1.003 Postgres statements per steer and 2 191 accepted a second at p95 9.52 ms (8 submitters, 50 fleets), the median of three runs at `43d828970`; the base `6ce61641d` measured 4.004 statements and 1 260 a second. One fleet with 16 senders takes 933 a second, against 342 on the base ([push scheduler](push_scheduler.md#m242-one-statement-intake)) | Intake is one autocommit statement that locks the fleet row only for its own duration, so a busy fleet admits about 2.7 times as many messages a second. |
| Historical serial delivery, healthy | 49.6 jobs/s per worker with one 250 ms destination in sixteen | Ten times the five-per-second estimate the refactor argument was made from, but see the next row. |
| Historical serial delivery, head-of-line | the OTHER fifteen destinations' p95 3 752 ms against the slow one's 3 758 ms | With 6% of jobs slow, the healthy 94% wait exactly as long. One stream, one worker, one queue position at a time. |
| Historical serial delivery, retry | 95.9% of the window in the ladder with two refusing destinations in sixteen (not re-measured at `774f99cbd`: the default run refuses nothing, and that baseline now carries a retry occupancy of 0) | Eight jobs that never resolve cost every job behind them the whole ladder. |
| Setup chat claim under saturation | Before → after the claim stopped waiting on a held draft, medians of three runs each. Sampled open-transaction age p95 959 → 17 ms; chat lease p95 3 006 → 707 ms; any-lease p95 3 207 → 745 ms. Pool wait p95 28 → 13 ms; 41.7 → 62.7 leases/s. All six runs doubled no lease, overran no host and recovered the pool. Shape: 40 hosts × 4 workers, a 20-connection pool, 5 chat tenants × 8 drafts and 200 fleets. One writer holds a tenant's drafts 1 s of every 1.25 s (`make bench-lease BENCH_SHAPE=saturation`). The baseline `lease-saturation.rig.json` is the before set's reset-rig run. | One busy draft used to hold its claim's host row for the whole write, so every tenant behind it waited. Skipped instead, the claim's transaction lasts only its own work. The chat tail left is the held tenant's drafts waiting for the writer's gaps. |
| Live tail fan-out | 3.05–3.06 allocations per delivered frame with 1 024 viewers on one fleet, flat from 200 B to 64 KiB (3.12–3.14 at 256, 3.45–3.52 at 64, 31–36 at one); 8.3 / 10.2 / 20.0 µs of runtime busy time per delivered frame and publish-to-receive p95 3.1 / 4.1 / 7.4 ms at 1 024 viewers for 200 B / 4 KiB / 64 KiB; every frame delivered, no lag notice (`make bench-tail`) | About three allocations per viewer per frame: the hub's per-reader copy of the message and the SSE frame built from it. Fan-out cost grows with viewers × frames, and with payload size through the copy. |
| Live tail streams | 13.5 KB of heap per open, idle per-fleet stream, flat from 64 to 4 096 streams (13 800 / 13 494 / 13 521 / 13 529 B), so 4 096 streams hold about 53 MiB; one frame sent to every stream arrives at p95 0.18 / 0.14 / 0.14 / 0.13 ms, none lost (`make bench-tail`, after the hub shared each frame; 21.7 KB per stream before) | Flat per stream: at the `SSE_MAX_STREAMS` default of 256 that is 3.3 MiB. Memory and latency bind nothing up to 4 096 streams; file descriptors bind first. |

An idle poll is one Postgres discovery probe on the ledger queue.
Its database work depends on eligible active fleets as well as poll rate.

Current outbound delivery uses independent destination lanes with eight concurrent sends and 32 queued jobs per lane (`afd_outbound`). A slow destination delays its lane first. When its queue fills, dispatch can also delay unrelated destinations; the serial delivery rows above do not measure this implementation.

Three things are deliberately not here:

- a production number, because nothing is deployed;
- behaviour under real customer traffic, because there is none yet;
- any threshold, because the lanes print a delta against their baseline and never fail a build on it.

The rig measures one process against local datastores, which is a floor and not a capacity plan.

### Push scheduler baselines

M241_001 replaced `lease.rig.json` and added `lease-burst.rig.json` on Nov 06, 2026.
Each lane ran three times on a reset rig, and the committed file is the median run.
[Push scheduler](push_scheduler.md#baselines) lists the statements per chat by path.

| Lane | Median run | Lowest | Highest |
| --- | --- | --- | --- |
| Drain, leases per second (200 fleets, 8 runners, a 10 s chat per lease) | 82.51 | 80.43 | 84.08 |
| Drain, Postgres statements per chat | 115.025 | 114.875 | 115.025 |
| Drain, commits per chat | 77.185 | 76.97 | 77.185 |
| Burst, pickup p95 (50 fleets, 10 messages a second each, 60 s, 8 runners) | 491.5 ms | 489.0 ms | 491.8 ms |

The burst row counts only the messages a run picked up: 25,212 of 30,000 still waited at the close.
The drain now sends a whole chat per lease: two renewals and forty activity posts.
The older rows above sent only the lease and its report.

PR #803 also added six durable-task reads per lease.
So statements per lease rose from 50.0 to 115.0, and leases per second fell from 172 to 82.5.

### Core NATS baselines (M244)

M244 re-measured every lane on core NATS at `2bea47c3d`, on Nov 10, 2026, with `make bench-<lane> PROFILE=rig`.
Outbound and cardinality each ran three times, and the committed file is the median run.
The lease shapes and steer ran once.

| Lane | Number | Before M244 | On core NATS |
| --- | --- | --- | --- |
| Outbound, 200 jobs over 16 destinations | delivered per second | 49.6 | 34.7 |
| Outbound | p95 / p99 | 3,752 / 4,024 ms | 5,386 / 5,730 ms |
| Cardinality, 10,000 fleets | candidate query | 0.54 ms with no ledger rows | 25.45 ms with 10,000 unsettled admissions |
| Steer | Postgres statements per steer | 1.003 | 2.002 |
| Steer | accepted per second, p95 | 2,191, 9.52 ms | 2,868, 4.33 ms |
| Lease drain | statements per chat, lease p95 | 115.0, 44.1 ms | 114.3, 57.9 ms |
| Lease saturation | lease p95 | 3,207 ms | 550 ms |
| Lease burst | pickup p95 | 491.5 ms | 494.6 ms |

Outbound delivers about 30% slower than the stream reader it replaced.
The likely cause is unmeasured. The worker claims only lane heads from `fleet.obligations`, so each busy-lane delivery may wait one claim round trip.

Counting `agentsfleet_outbound_claims_total{via="scan"}` against deliveries in one run would settle it.

The cardinality row measures a different workload.
The new seed admits one event per fleet, so the candidate query scans every unsettled admission.

Steer now costs two Postgres statements where it cost one, and the second statement is not yet identified.

### Admissions ledger measurements

The `0.65.0` production ledger path was measured with `BENCH_FLEETS=<n> BENCH_RUNNERS=8 make bench-lease PROFILE=rig KEEP_TEST_STATE=1`.
The canonical coverage run reset the owned rig before measurement; benchmarks reused those migrated services and swept their own fixtures.
Each row is the median of three runs on the shared development machine, with pool size 20.

All 15 runs completed: 1,500 drain events, zero failures, zero unsettled admissions and 9,240 of 9,240 fixture records swept.
These are production claim and report transactions, including admission settlement.

| Fleets / events | Drain statements per event | Drain commits per event | Completed events / second | Claim round trips per lease | Idle discovery statements per poll |
| --- | --- | --- | --- | --- | --- |
| 20 | 63.650 | 38.800 | 90.23 | 11.550 | 1.000 |
| 40 | 57.025 | 29.600 | 108.18 | 7.475 | 1.000 |
| 80 | 53.450 | 24.363 | 130.26 | 6.263 | 1.000 |
| 160 | 52.813 | 22.838 | 136.55 | 5.019 | 1.000 |
| 200 | 52.725 | 22.535 | 142.37 | 4.860 | 1.000 |

The 20-to-200 event increase used 8.28 times as many drain statements for ten times the completed work.
This measures client statement volume; it does not establish a bound on database rows examined or server CPU.
The empty-ledger precheck skips fleet scans; nonempty discovery still examines eligible fleets.

At 200 events the median lease and report p95 were 43.679 ms and 33.695 ms.
The raw claim `rate_per_second` counts polls, including misses; the table uses the drain's explicit completed-event rate.

The measured source was `ab66bb2ec992c87f0158470672a33bab30a58f38`, with integrated base `124c2bce303c95533745312c1ec725092bb604cb`.
Documentation edits were pending; runtime sources matched that commit throughout all measurements.

The later failed-park repair adds fenced cleanup when parking fails; successful claim and report statements stay unchanged.
No later timing run was made for that repair.

The fresh comparison checkout at `0fa9e55356571d57df69cd3a13fb3b90930b0e49` exited successfully but completed none of its 200 drain events, recorded 17 failures and left all 200 events pending.
That incomplete result cannot support zero-cost ratios or a completed-throughput comparison.

The new benchmark refuses incomplete drains instead of publishing those ratios.
No fresh before/after throughput improvement is established.

Earlier stream and ledger prototypes used a temporary selector and settled admissions after the report transaction.
Their rates and costs describe those prototypes; the table above measures the production implementation.

## Sizing procedure (fleet- and playbook-readable)

Structured for a large language model (LLM) fleet or a `agentsfleet`-driven scaling playbook. Each step has explicit inputs, a formula, a decision rule, and an emit target.

### Inputs

| Symbol | Meaning | Source |
|---|---|---|
| `Z` | Target fleet count (active + idle) | Product / fleet plan |
| `N` | Runner host count | Operator / capacity plan |
| `W_idle` | Total enabled idle workers across those hosts | Assigned worker counts minus busy or disabled workers |
| `C_bus` | Bus connections per replica: 1 | `afd_bus::nats::connect` |
| `R` | `agentsfleetd` API replica count | Deployment plan |
| `S` | Peak concurrent SSE tails | Product / dashboard usage |
| `P_conn` | NATS concurrent-connection ceiling | The NATS server's connection limit and the machine's file-descriptor limit; no longer a plan tier |
| `probe_s` | Per-replica work-recovery interval: 5 seconds | `OFFER_FALLBACK_TICK` |

### Procedure

```
Step 1: Bus connection budget (no per-fleet, no per-viewer term)
  bus_conns = R * C_bus     (one NATS connection per replica)
  ASSERT bus_conns + measured_reconnect_headroom <= P_conn
    if violated → raise the NATS machines' descriptor limit or connection capacity
  NOTE: Adding agentsfleetd replicas increases R and connection demand.
  NOTE: Z and S do not appear in the steady-state bus connection count.
        S still sizes SSE_MAX_STREAMS, response memory, and HTTP capacity.

Step 2: Idle recovery work
  recovery_statements_per_second = R / probe_s
  Runners send no lease requests while idle.
  Size Postgres using recovery and claim query latency at the target fleet count Z.

Step 3: Hot-path throughput (the real wall)
  lease_report_qps = peak events/sec across the fleet
  size R + Postgres so lease/report p99 stays within budget under lease_report_qps
  (PG connection pooler assumed; sizing in the deployment runbook)

Step 4: Emit configuration
  OFFER_FALLBACK_TICK   = 5 s     (compiled daemon recovery interval)
  LEASE_TTL             = 35000   (compile-time; renewal carries long runs)
  agentsfleetd_replicas         = <step 3 result>
  runner_hosts             = N
```

### Anti-patterns (do NOT do these)

1. **Size bus connections by fleet or viewer count.** Each replica holds one NATS connection, with one native subscription per watched subject.
2. **Tune runner polling.** Runners no longer poll for leases. Account for the daemon recovery probe separately.
3. **Shorten the claim pass for faster replies.** A report pings the workers when it commits; the 30 s pass covers only a lost ping.
4. **Add runners to fix lease/report latency.** Runners add compute, not control-plane throughput. Scale `agentsfleetd` replicas + Postgres for hot-path latency.
5. **Raise `publish_deadline_ms` to ride out an outage.** The deadline is what an outage costs each publisher. A longer wait buys no delivery, because the gap marker recovers the frame.
6. **Open a native subscription per viewer.** The hub keeps one per subject and fans out locally. A viewer costs a broadcast receiver, not a subscription.
7. **Treat SSE streams as dedicated threads.** Rust serves async bodies under `SSE_MAX_STREAMS`; API admission and viewer capacity are separate budgets.
8. **Ignore active fleet count in the Postgres budget.** One claim can still perform many indexed head lookups. Measure at the intended fleet population.
9. **Replace randomized candidate ordering with a fixed prefix.** A bounded result with a fixed order can repeatedly exclude eligible fleets. Notification and affinity priority do not provide a strict fairness guarantee.
10. **Sum admission backlog gauges across replicas.** Each census reads the same deployment-wide ledger. Use the maximum; summing duplicates the same work.

---

## Failure and rebalance behavior

### Runner host loss

A runner that dies holds no datastore or bus connection to leak and no subscription to reclaim. Its in-flight lease expires at `lease_expires_at`.

The next runner's `lease` reclaim path re-issues the event with a higher fencing token ([`runner_fleet.md`](./runner_fleet.md) §"Failure recovery model"). A disconnected lease expires before another runner can claim it; the recovery probe finds work after missed notifications.

Surviving hosts keep their existing sessions; the gateway pushes newly available work.

**Failure domain scales with the assigned `worker_count`**. A host running a pool of N concurrent leases (M88_002) drops **N** in-flight runs on loss, not one. Each lease expires and re-leases independently (no batch coupling), so no work is lost, but N runs restart instead of one.

This is the cost of running more fleets per host; `worker_count=1` keeps the failure domain at one run. Operators size N against this tradeoff.

### Runner host add

A newly enrolled runner opens its native session and advertises capacity.
Existing leases retain their owners; the gateway can offer waiting work to the new host.

### Datastore failover (primary swap)

Reconnect, the gap every subscription receives, and the `catching_up` viewers are told about: [`datastore_scaling.md`](./datastore_scaling.md) §"How it behaves".
Measure provider-specific failover and command-error behavior on the intended deployment before claiming recovery guarantees.

---

## What is explicitly out of scope here

- **Adaptive connection pooling.** The Rust command path uses a multiplexed connection, so the retired pool knobs do not apply.
- **The datastore's own target design.** [Datastore scaling](./datastore_scaling.md) is canonical for it and wins over this page. Postgres holds every fact, and the bus is core NATS in one region: three nodes in production, two in development. This page sizes the runtime that runs on top.
- **Postgres scaling.** Pgbouncer + plan sizing are covered in the deployment runbook, not here. After the cutover it is the **primary** scaling axis, so the runbook carries more weight than it did.
- **Placement.** Label checks and shared host-capacity admission are implemented ([runner protocol](runner_fleet.md#authoring-leases)). Autoscaling and fairness stay outside this sizing page.
- **Multi-region topology.** Follow [datastore scaling](./datastore_scaling.md); provider-specific latency and limits require measurements on the selected deployment.
