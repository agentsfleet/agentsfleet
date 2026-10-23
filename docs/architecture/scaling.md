# Scaling and tuneup — how the runtime grows after the cutover

> Parent: [`README.md`](./README.md) · Companions: [`data_flow.md`](./data_flow.md) §"Connection topology", [`runner_fleet.md`](./runner_fleet.md) §"Scaling".
>
> [!IMPORTANT]
> **Scope:** size the ledger queue, runner poll rate, database work and shared Dragonfly clients. Historical stream measurements are marked as comparisons.

Read this when you need to size a deployment, pick env-var values, or decide whether the next bottleneck is `agentsfleetd` API replicas, Postgres, the datastore machine, or runner fan-out.

---

## Facts

Every row is extracted from the sections below; the owner column names the section that carries the full story.

| Invariant | Value | Mechanism | Owner section |
|---|---|---|---|
| Dragonfly connection budget | 3·R at normal boot | one shared command socket, one hub socket, one outbound reader socket per replica | §Connection budget after the cutover |
| Idle request volume | `N_runners / poll_s` requests/sec | one Postgres fleet-discovery statement per poll; authentication and accepted-kind placement reads are additional | §Idle request volume |
| Idle-cost knob | `NO_WORK_RETRY_AFTER` = 1000 | trades idle bill against idle pickup latency; single-sourced in `rustd/crates/afd_core/src/timing.rs` | §Tuneup knobs |
| Candidate result ceiling | `MAX_CANDIDATES_PER_POLL` = 64 | bounds returned fleet identifiers and claim attempts; discovery still examines eligible active fleets | §Candidate and claim bounds |
| Auth read per request | one indexed single-row read | M143_001 removed the per-process memo, so cordon/drain/revoke bite fleet-wide the moment they commit | §Idle request volume |
| Fleet count in discovery work | eligible active fleets when unsettled admissions exist | an empty ledger skips fleet scans; otherwise one oldest-unsettled lookup per eligible fleet | §Candidate and claim bounds |
| Server-Sent Events (SSE) ceiling | `SSE_MAX_STREAMS` = 256 per replica, bound by file descriptors | async response bodies own semaphore permits; 503 at the cap; hub connection shared | §Tuneup knobs, §2 |
| Dragonfly timeout | `DRAGONFLY_REQUEST_TIMEOUT_MS` = 5000 — do not raise | above 5 s is failure, not slowness | §Tuneup knobs |
| Dragonfly connect timeout | `DRAGONFLY_CONNECT_TIMEOUT_MS` = 5000 | bounds establishment, not just commands; a dead endpoint refuses inside the budget instead of holding boot open | §Tuneup knobs |
| Lease TTL | `LEASE_TTL` = 30000 | reclaim latency floor; renewal decouples run length from it | §Tuneup knobs |
| Per-host concurrency | assigned `worker_count` = 1 (dashboard, per runner) | a capacity knob that widens the failure domain to N in-flight runs on host loss | §Tuneup knobs, §Runner host loss |
| Admission ceiling | `DEFAULT_MAX_IN_FLIGHT` = 256, api-class only | compiled default; operational and stream routes use separate handling | §Tuneup knobs |
| The binding constraint | `agentsfleetd` replicas + Postgres writes | both horizontally scalable; the hot path is shardable per fleet | §Where the next ceiling actually lives |
| Recurring-read indexes | one slot per table, plan-asserted | idle Postgres cost tracks work, not accumulated rows; runner sorts stay unindexed on purpose, because runners number about a hundred | §Which recurring Postgres reads are index-served |
| Idle retry wait | up to `NO_WORK_RETRY_AFTER` before another attempt | assignment and datastore work add latency; this is not a delivery bound | §Event-delivery latency |
| Outbound-answer consumer | blocking read, up to 5 seconds | owns one dedicated Dragonfly socket; does not block shared commands | §Tuneup knobs |
| Failover reconnects | three connection owners per healthy replica | command managers reconnect; the hub retries and restores subscriptions; retry attempts are not bounded by socket count | §Datastore failover |

## Traps

The nine sizing anti-patterns ARE the trap list — read §Anti-patterns (do NOT do these) before any sizing conversation. Three more that live elsewhere:

- Statement count alone hides server work: measure discovery latency at the intended active fleet count (§Idle request volume).
- Metric ratios need their denominator: without `agentsfleet_lease_polls_total`, a traffic increase and a fan-out regression look identical (§Candidate and claim bounds).
- Fleet-memory hydration still sorts, and correctly so — an index only removes a sort where the plan can exit early (§Which recurring Postgres reads are index-served).

## Topology

No standalone diagram; the sizing procedure block in §"Sizing procedure" is the operational artifact, with inputs, formulas, and emit targets.

## Decisions

| Decision | Reason | Where / artifact |
|---|---|---|
| Auth memo removed — read the runner row every request | revocation must be deterministic fleet-wide, not per-machine | §Idle request volume; M143_001, [`../AUTH.md`](../AUTH.md) §"Runner token" |
| Commit the admission before notification | lost notifications cannot lose accepted work | §Idle request volume |
| Randomize candidates after notification and affinity priority | spread competing polls across eligible fleets; no strict fairness bound | §Candidate and claim bounds |
| Async SSE bodies share one subscription pump | waiting for frames does not reserve an operating-system thread per viewer | §2; `afd_sse` |
| Multiplex ordinary Dragonfly commands | cloned handles share one socket; blocking consumers own separate sockets | §Connection budget after the cutover |

---

## Detail

Everything below is the full reference. Headings are stable — specs cite them by text; insert new sections, never rename existing ones.

## What binds this deployment

The per-fleet dedicated datastore connection is gone; `lease` reads
non-blocking on a shared socket. The binding constraint is now `agentsfleetd`
API replicas plus Postgres write throughput on the lease/report hot path, both
horizontally scalable, and idle request volume follows **runner poll cadence**
rather than fleet population. Every current number is in the Facts table at the top of this page.


## The infra reality first

v2 ships hosted on Fly.io, and the canonical datastore is **self-hosted Dragonfly**: four processes — two primaries and two replicas — in one Fly machine, in the same region as the daemon, reached over Fly's private network (6PN). M196 moved development onto that shape; `datastore_scaling.md` is canonical for the target and wins over this page where the two disagree.

**Three properties of the hosted service are gone, and they were three of the five things this page used to size against.** They are recorded here because a reader who sized a deployment last quarter sized it against them:

- **The plan-bound max-connections cap is gone.** There is no plan. The ceiling is now the machine's file-descriptor limit and Dragonfly's own accept path, which are orders of magnitude above anything the connection budget below asks for. It is no longer a wall at any fleet count.
- **Per-request pricing is gone.** Command volume consumes CPU on the datastore machine. The fleet scheduler's idle query consumes Postgres capacity. §"Idle request volume" still matters, but it now sizes a **machine**, not an invoice.
- **The Transport Layer Security (TLS) dial cost is gone in-region.** 6PN is WireGuard-encrypted already, so the seed is plaintext and a dial pays no handshake. That is worth roughly 230 ms per connection against an RSA-2048 leaf, which is why the local rig keeps a TLS node for one suite and runs everything else plaintext.

**What replaces them is a fixed machine, and a fixed machine has a different failure shape.** A hosted plan degrades by refusing a dial or sending a bill; a machine degrades by running out of memory or CPU, and a Dragonfly node that hits `maxmemory` while retaining rather than evicting refuses writes. The dev cluster is `performance-2x` (2 vCPU, 4GB) against a 2.25GB configured ceiling.

Postgres is **PlanetScale**, and two of its properties bind the same way:

1. **Per-cluster `max_connections`.** A conservative default that varies with cluster size (Clusters → Parameters); the dev cluster reports 25 (`deploy/fly/agentsfleetd-dev/fly.toml`). Every replica's `DATABASE_POOL_SIZE` (default 20) plus the migrator during a release must fit inside it. A connection asked for past the ceiling is refused with `53300`, which sqlx retries silently until the acquire budget runs out — so the daemon can only report it as a **stall** ("the pool held N of M and could not open another", `afd_db::pool::acquire`), retries the acquire once, and then answers `UZ-INTERNAL-001`. It is not an outage and must not be paged as one. The request path is therefore meant to run through PlanetScale's PgBouncer (port `6432`, transaction pooling), which multiplexes replicas' connections onto one bounded server pool; the deployment preflight refuses an `api-connection-string` on any other port, and the migrator stays on `5432` for its session advisory lock. PgBouncer's own `default_pool_size` (20) counts against that same ceiling once, beside PlanetScale's admin and exporter backends and the migrator at release — set it low enough in the console that all four fit.
2. **Region.** Production's database sits in `us-east` beside Fly `iad`; development's sits in `aws-us-east-2` (Ohio), a round trip away on every handshake and every query. A region cannot be changed in place — `playbooks/operations/database_region_move/` moves it.

---

## Event-delivery latency

The cutover added one hop (the runner lease request) and removed another (the in-process worker dispatch). End-to-end latency for a steer:

| Step | Work on the serving path |
|---|---|
| API receives a steer or webhook | Validate identity, access and backlog budget |
| API accepts the event | Commit `fleet.admissions`, then send a best-effort sharded notification |
| Runner polls for a lease | Discover eligible ledger heads, claim one head and its fleet slot, then apply gates and issue the lease |
| Runner executes | Prepare its sandbox and run the fleet instructions |

An idle runner waits up to its retry interval before the next lease attempt.
That interval is not an end-to-end latency bound: assignment, available workers, and datastore work add delay.
Reducing `NO_WORK_RETRY_AFTER` trades more Postgres probes for shorter idle pickup waits.

**Live-tail cadence.** The runner attempts immediate posting for a lease's first frame and first response chunk.
Later frames batch toward 64 KiB; partial batches flush every 250 ms.
The first response does not wait for that timer when the posting queue has room.

Four batches may be queued before backpressure delays posting.
A single oversized frame rides alone.
These limits do not bound browser paint time.

The source is `rustd/crates/afr_supervisor/src/activity/mod.rs`: `Early`, `MAX_BATCH_BYTES`, `FLUSH_EVERY` and `MAX_BATCHES_HELD`.

---

## Connection budget after the cutover

At normal boot, each `agentsfleetd` replica opens three Dragonfly connections: shared commands, the hub, and the outbound reader. `R` replicas × 3 = 3·R ([`data_flow.md`](./data_flow.md) §"Connection topology — the cutover collapsed the dedicated tier").

Source: [`runtime boot`](../../rustd/crates/agentsfleetd/src/serve/runtime.rs),
[`Dragonfly handle`](../../rustd/crates/afd_dragonfly/src/client/mod.rs), and
[`outbound worker boot`](../../rustd/crates/agentsfleetd/src/outbound.rs).

### Idle request volume

Runner polls occur at `runner_count / poll_seconds` requests per second.
The fleet scheduler makes one Postgres discovery statement per idle poll and no Dragonfly command.
This is the scheduler probe; authentication and accepted-kind placement reads are additional work on the HTTP request.

Runner authentication reads `runner.runners` on every request.
Reading the row makes cordon, drain, revoke and delete effective across replicas after commit
([runner token](../AUTH.md#runner-token)).

For 20 runners polling once per second, the fleet scheduler makes 72,000 discovery statements per hour.

An empty ledger skips fleet scans. When unsettled admissions exist, server work grows with eligible active fleet population, including when all heads are parked.
Measure that cost separately from the number of statements.

### Candidate and claim bounds

Discovery returns at most `MAX_CANDIDATES_PER_POLL` (64) fleet identifiers
([assignment](../../rustd/crates/afd_fleet/src/lease/assign/mod.rs)).
The query first checks for any unsettled admission, then checks placement labels and the oldest unsettled admission of each eligible fleet before applying that limit.

The partial `(fleet_id, sequence)` admission index serves each head lookup; the limit bounds returned candidates and subsequent claim attempts.
It does not bound the number of active fleets examined inside Postgres.

An isolated plan probe with 20,001 active fleets and no admissions performed zero fleet scans after the empty-ledger check.
The single local samples took 19.702 ms before and 0.079 ms after; these are query-plan observations, not throughput or chat-latency measurements.
The real-database test is `afd_fleet/tests/integration_lease_plan/idle.rs`; its assertion requires every fleet scan to have zero actual loops.

The process prioritizes recent notification hints, held fleets and previous holders, then randomizes the rest.
An absent or stale notification cannot strand work because every poll still queries Postgres.
A parked oldest admission blocks later admissions while standing access awaits an answer or an event approval remains within its deadline.
The partial `(fleet_id, event_id, expires_at)` gate index serves both probes over unresolved rows.
The approval-history plan test grows resolved, expired and unrelated gates from 2,000 to 16,000 rows and runs both statements through `EXPLAIN (ANALYZE, BUFFERS)` with custom and generic prepared plans.
It checks fleet and event index bounds, deadline bounds for event approvals, and early stopping when many gates match. Total discovery still grows with the eligible fleet population.

The claim locks the slot and exact head, stamps their fence, and commits before returning it.
An uncontended successful claim takes discovery, begin, the claim statement and commit; contended attempts add work.

Watch candidate, database-round-trip and Dragonfly-command counters divided by `agentsfleet_lease_polls_total`.
Use query latency and query plans alongside these ratios: one slow discovery statement still counts as one statement.

#### Which recurring Postgres reads are index-served

The Dragonfly figures above are only half the idle bill. The other half is Postgres, and it used to scale with *accumulated rows* rather than with work — an account that had been running a year cost more at idle than a fresh one, for no reason a user asked for. Each table's own index slot closes that: every recurring control-plane read below is served by an index, except the runner sorts left unindexed on purpose, and each index is asserted against the query **plan**, not merely created.

| Recurring read | Was | Now |
|---|---|---|
| Liveness sweep — due runners | Sequential scan + top-N sort of `runner.runners`, every cycle | Still a sort, on purpose: `SELECT_DUE_RUNNERS` (`rustd/crates/afd_runner/src/sql/sweep/mod.rs`) orders by `updated_at, id` under its batch `LIMIT`, and `schema/1800_runner_runners.sql` leaves runner sorts unindexed because runners number about a hundred. M142_001's `idx_runners_updated_at_id` measured 6 shared buffer hits at 20 000 runners; the M154 rebuild dropped it. |
| Liveness sweep — affinity expiry | Full scan of `runner.fleet_slots`, once **per due runner per cycle** | `idx_fleet_slots_last_runner_id_lease_expires_at`; also covers the unindexed `ON DELETE SET NULL` foreign key |
| Reclaim — prior active lease | Scan of `runner.leases` on an unindexed `ON DELETE CASCADE` foreign key | `idx_leases_fleet_id_status_fencing_token`; filter, ordering and `LIMIT 1` in one seek |
| Workspace event keyset page | Index scan plus a post-filter on the tiebreak column; under a cached generic plan the cursor and `since` fell behind their `IS NULL` guards into filters | One text per scope, cursor and actor filter; `idx_events_workspace_id_created_at_event_id` with the workspace, cursor and `since` in its index condition, walked in order |
| Fleet event page and chat thread | A cached generic plan multiplied the workspace and fleet selectivities, expected one row, and read the fleet's whole history into a bitmap scan plus sort | `idx_events_fleet_id_created_at_event_id` walked in order, because `stx_events_workspace_id_fleet_id` (slot 1700) tells the planner the fleet implies the workspace. An actor-filtered page still sorts its matches |
| Fleet list page | Unserved — the existing index is partial on `status='active'` and the list is not status-filtered | `idx_fleets_workspace_id_created_at_id` |
| Runner and api-key list sorts | Sort node per request | Still a sort node: the M154 rebuild left runner list sorts unindexed (`schema/1800_runner_runners.sql`), and `identity.api_keys` keeps only `idx_api_keys_tenant_id_live` (`schema/1150_identity_api_keys.sql`) |

**Queue discovery:** the unsettled partial index avoids completed admission history. Each probe still examines eligible active fleets; size Postgres against both fleet count and poll rate.

**What it does not cover.** Fleet-memory hydration (`SELECT_ALL_FOR_FLEET`, `rustd/crates/afd_memory/src/sql/mod.rs`) still sorts, and correctly so: it fetches a fleet's entire memory set with no `LIMIT`, and for an unbounded fetch a bitmap scan plus sort is genuinely cheaper than an ordered index scan with random heap access. An index only removes a sort where the plan can exit early.

**One anti-pattern this replaced,** worth stating because it reads as an optimisation: the operator runner list resolved each row's lease-liveness in a common table expression (CTE) spanning the whole runner table before paginating. PostgreSQL answers that by hashing the *entire* `runner.leases` table once per request — 6 472 buffer hits against a 200 000-row lease table. Resolving liveness over the surviving page instead costs 79. When per-row work sits above a `LIMIT`, the cost is not "one lookup per row"; it is usually one whole-table build.

---

## Tuneup knobs and when to turn them

| Knob | Default | What it scales with | Turn it when |
|---|---|---|---|
| `DRAGONFLY_REQUEST_TIMEOUT_MS` | 5000 | Datastore tail-latency tolerance | No healthy datastore's p99 round-trip approaches 4 s, and in-region 6PN is far below that. **Do not raise it** — >5 s is failure, not slowness. |
| `DRAGONFLY_CONNECT_TIMEOUT_MS` | 5000 | Time to establish a connection, which `DRAGONFLY_REQUEST_TIMEOUT_MS` never covered — that knob bounds commands on a connection that already exists. | A dead or black-holing endpoint used to hold a boot preflight open past every deadline it declared; the budget now refuses it as `Unreachable`. Raise only where a TLS handshake to a distant region measurably exceeds 4 s. |
| `NO_WORK_RETRY_AFTER` | 1000 | Idle lease-poll request volume **and** idle pickup latency. **Not busy-fleet delivery latency.** | On self-hosted Dragonfly the idle loop costs CPU on a machine already paid for, not a per-request charge; it sizes the machine rather than an invoice. Raise to 2000–5000 to cut idle load proportionally; idle pickup latency rises by the same factor. Single-sourced in `rustd/crates/afd_core/src/timing.rs`. |
| `MAX_CANDIDATES_PER_POLL` | 64 | Maximum candidates returned by Postgres discovery and attempted by one poll. The statement still probes eligible active fleets before applying this limit. | Compile-time in `afd_fleet/src/lease/assign/mod.rs`. Measure discovery latency and candidate attempts before changing it; increasing it also increases contention work per poll. |
| `LEASE_TTL` | 30000 | Reclaim latency floor **and** the increment each renewal adds. The runner checks every 5 s and renews a held lease within 10 s of its expiry, up to `MAX_RUNTIME` (`rustd/crates/afr_supervisor/src/renew/mod.rs`, `rustd/crates/afd_core/src/timing.rs`). | Compile-time. Run length does not size it, because renewal carries long runs. Lower only with a tighter recovery requirement. |
| `SSE_MAX_STREAMS` | 256 | Concurrent asynchronous SSE bodies per replica; shared by fleet, workspace and draft-progress tails. Zero is rejected at boot. Each stream holds a socket, and the daemon neither raises `RLIMIT_NOFILE` nor checks the knob against it: about 300 descriptors go to admitted requests, Postgres and Dragonfly first, so 256 is what a stock 1,024 soft limit leaves room for. | Raise only on a host whose descriptor limit is raised to match, after measuring stream refusals, memory, CPU, and proxy capacity. The tail itself held 4,096 streams inside every bound (below). Watch `agentsfleet_sse_in_flight_streams` and `agentsfleet_sse_backpressure_rejections_total`. |
| `DEFAULT_MAX_IN_FLIGHT` | 256 | Compiled API admission ceiling; excess API requests receive 429 with Retry-After. SSE has its own ceiling. | `rustd/crates/agentsfleetd/src/serve/mod.rs` passes this constant (declared in `afd_http`'s `admission/mod.rs`) directly; there is no environment override in the Rust boot path. Measure admission refusals and datastore capacity before changing it. |
| `agentsfleetd` API replica count | deployment-driven | HTTP requests per second (user surface + `/v1/runners`) + lease/report throughput + SSE fan-in | Lease/report p99 climbs, or per-replica viewer count keeps hitting the `SSE_MAX_STREAMS` ceiling. |
| Runner count | operator-driven | Compute throughput; idle lease-poll request volume | Add hosts to add execution capacity — no datastore or coordination cost. Each idle runner adds one poll loop to the datastore's load (tune via `NO_WORK_RETRY_AFTER`). |
| assigned `worker_count` (per-runner, dashboard) | 1 | Concurrent leased fleets **per host** — the runner worker-pool size (M88_002), assigned on the runner row (M148) and delivered with the heartbeat. N workers each run the lease→execute→report unit; the per-fleet `affinity.claim` keeps two workers off the same fleet. | A host has spare cores/memory while one long fleet run monopolises it (per-host throughput is fixed at 1 at the default). Raise N to run more fleets per host instead of enrolling more hosts. **Tradeoff:** N is a capacity knob, not a throughput guarantee (CPU/RAM/disk/network are not isolated across workers), and it **widens the failure domain** — one host loss drops N in-flight runs, not 1 (all re-leased by the M84_002 sweeper, but interrupted). `worker_count=1` is maximum isolation. At the ceiling (`MAX_WORKERS` = 64, `afd_core/src/limits.rs`) and the 2-core sandbox default, any host under 128 cores is oversubscribed. |

The lease path never passes `BLOCK` to Dragonfly.
The outbound reader uses `BLOCK_INTERVAL = 5000` ms on its dedicated socket, with cancellation raced against the read.
Its worker also checks pending deliveries; include those commands when measuring idle request volume.
Source: [`outbound worker`](../../rustd/crates/afd_outbound/src/worker/mod.rs).

---

## Where the next ceiling actually lives

Once Dragonfly connection count and request volume fit the machine, the next bottleneck is one of:

### 1. `agentsfleetd` API replicas + Postgres write throughput (the usual answer now)

The lease/report hot path does the durable writes the worker used to do — `INSERT fleet.events`, the run's usage logged on each `/renew` and settled through the biller, `UPDATE` terminal, `INSERT telemetry`, checkpoint `UPSERT`, plus the `runner.leases`/`runner.fleet_slots` bookkeeping. At fleet scale this is the binding axis. Both `agentsfleetd` replicas and Postgres (with a connection pooler) scale horizontally; the hot path is shardable per fleet.

Symptom: lease/report p99 climbs; Postgres connection saturation or write-lock contention on the `fleet` tables. Fix: more `agentsfleetd` replicas + Postgres sizing in the deployment runbook.

### 2. Pub/sub fan-out on activity (unchanged in shape)

Each replica's async hub receives one delivery per published frame on each subscribed channel.
A bounded broadcast queue holds 256 messages per channel and serves each subscriber independently.
A lagging reader receives `catching_up`; a closed hub ends a per-fleet tail.

Each SSE response owns a semaphore permit until its body is dropped.
The default `SSE_MAX_STREAMS` ceiling is 256 per replica; new streams receive 503 when full.
Waiting bodies do not reserve dedicated operating-system threads.

Measure response-buffer memory, serialization work, network throughput, and proxy connection limits under the expected viewer count.
HTTP/2 can share a socket across streams, but each HTTP hop negotiates independently.
See [Data Flow, D. WATCH](./data_flow.md#d-watch--user-side-how-the-live-tail-surfaces) for the browser proxy path and protocol limits.

Source: [`stream ceiling`](../../rustd/crates/afd_sse/src/ceiling.rs),
[`tail`](../../rustd/crates/afd_sse/src/tail/mod.rs), and
[`hub`](../../rustd/crates/afd_dragonfly/src/hub/mod.rs).

### 3. Datastore machine capacity (now rarely first)

Memory and CPU on the machine hosting the cluster, not a plan tier. After the cutover (and the SubscriptionHub) the normal connection count is `3·R`, far below the pre-cutover `~fleets`, and no vendor plan caps it. What binds instead is `maxmemory`: Dragonfly is configured to retain rather than evict, so a node at its ceiling refuses writes rather than silently dropping the oldest ones. That is the safer failure and still an outage. The request axis is the runner poll loop. The binding axis is usually #1, not this.

---

## Measured ceilings

The stream queue rows below are historical comparisons. Current ledger measurements follow the table.

Numbers, not estimates. Each row is what a lane in `rustd/crates/afd_bench`
measured on one developer machine against a freshly reset compose Postgres and
Dragonfly — `make bench-<lane> PROFILE=rig` — and the committed result sits beside
it in `bench/baselines/`. Absolute rates move with hardware; the shapes below
do not. A baseline that moves takes its row here with it in the same commit.
The steer, delivery and cardinality rows were re-measured at `774f99cbd`; the
lease rows come from the result file stamped `c82badcb5`, and the live-tail rows
from `e2d250172`, the revision the tail lane was built on. Latency columns moved with
host load between runs on this shared machine; the counts beside them did not.

The saturation row comes from paired runs on Oct 21, 2026. The two sets differ
only in the claim code.

Statement and commit counts are Postgres's own: `pg_stat_statements` calls
(transaction control left out) and `pg_stat_database.xact_commit` less session
startups, after every pooled connection has been made to flush. The compose
Postgres preloads `pg_stat_statements` for this; `afd_bench`'s `statements/mod.rs`
says why each correction is there.

| Path | What it costs | The number that decides |
|------|---------------|-------------------------|
| Stream comparison: idle lease poll | 1.00 Dragonfly command, 0 Postgres statements and 0 commits per poll (75 382 polls after a 200-fleet drain, readiness index empty, nothing force-cleared), measured before M225; the readiness doorbell adds a second peek only to a poll it rang, so an idle poll stays at 1.00 (`agentsfleet_lease_poll_dragonfly_commands_total ÷ agentsfleet_lease_polls_total`) | Idle cost scales with runners, not fleets. The lease clears a drained fleet's readiness mark, so once work runs out the index is empty and an idle poll never reaches Postgres. |
| Stream comparison: contended lease | 1 928 leases/s; 2.4 Postgres round trips per issued lease; 27.8% of polls find nothing; p95 7.8 ms (200 ready, 8 runners, pool 20, window ended at exhaustion) | A lease costs about two round trips under contention, down from 11.3 at `e2d250172`. |
| Stream comparison: lease through report (drain) | 50.0 Postgres statements and 26.8 commits per lease issued, the polls that missed and the report included; 14.1 Dragonfly commands per lease; 172 leases/s; lease p95 56.5 ms, report p95 21.5 ms; all 200 events processed once, 2 ledger rows each (200 fleets, 8 runners, the drain half of `make bench-lease`), measured before M225; an interim M225_001 build (a readiness directory, since replaced by the doorbell) measured lease p95 48.1 ms, report p95 16.9 ms and 176.9 leases/s, and the doorbell build is not yet re-measured | What one event costs the scheduler and the ledger end to end through `Plane::lease` and `Plane::report`, counted by Postgres rather than by the lane. |
| Stream comparison: steer ingress | 3.04 Dragonfly commands per steer, 1.94 Postgres transactions; 1 639/s at p95 7.4 ms (8 submitters, 50 fleets), measured before M225; a mark now also rings its partition's doorbell channel, so the expected figure is 4.04 until it is re-measured | Ingress writes the admission ledger, about two Postgres transactions per steer. The readiness index fills to the population and holds until a runner drains it. |
| Delivery, healthy | 49.6 jobs/s per worker with one 250 ms destination in sixteen | Ten times the five-per-second estimate the refactor argument was made from — but see the next row. |
| Delivery, head-of-line | the OTHER fifteen destinations' p95 3 752 ms against the slow one's 3 758 ms | With 6% of jobs slow, the healthy 94% wait exactly as long. One stream, one worker, one queue position at a time. |
| Delivery, retry | 95.9% of the window in the ladder with two refusing destinations in sixteen (not re-measured at `774f99cbd`: the default run refuses nothing, and that baseline now carries a retry occupancy of 0) | Eight jobs that never resolve cost every job behind them the whole ladder. |
| Setup chat claim under saturation | Before → after the claim stopped waiting on a held draft, medians of three runs each. Sampled open-transaction age p95 959 → 17 ms; chat lease p95 3 006 → 707 ms; any-lease p95 3 207 → 745 ms. Pool wait p95 28 → 13 ms; 41.7 → 62.7 leases/s. All six runs doubled no lease, overran no host and recovered the pool. Shape: 40 hosts × 4 workers, a 20-connection pool, 5 chat tenants × 8 drafts and 200 fleets. One writer holds a tenant's drafts 1 s of every 1.25 s (`make bench-lease BENCH_SHAPE=saturation`). The baseline `lease-saturation.rig.json` is the before set's reset-rig run. | One busy draft used to hold its claim's host row for the whole write, so every tenant behind it waited. Skipped instead, the claim's transaction lasts only its own work. The chat tail left is the held tenant's drafts waiting for the writer's gaps. |
| Stream comparison: cardinality | 4.6 KB of Dragonfly per idle fleet, flat from 10 to 10 000 (4 624 / 4 608 / 4 641 / 4 641 B); peek 0.09–0.24 ms, stream read 0.06–0.26 ms, candidate query 0.54 ms at 10 000 | Linear. A million idle fleets is roughly 4.6 GB of Dragonfly and no slower a hot path. |
| Live tail fan-out | 3.05–3.06 allocations per delivered frame with 1 024 viewers on one fleet, flat from 200 B to 64 KiB (3.12–3.14 at 256, 3.45–3.52 at 64, 31–36 at one); 8.3 / 10.2 / 20.0 µs of runtime busy time per delivered frame and publish-to-receive p95 3.1 / 4.1 / 7.4 ms at 1 024 viewers for 200 B / 4 KiB / 64 KiB; every frame delivered, no lag notice (`make bench-tail`) | About three allocations per viewer per frame: the hub's per-reader copy of the message and the SSE frame built from it. Fan-out cost grows with viewers × frames, and with payload size through the copy. |
| Live tail streams | 13.5 KB of heap per open, idle per-fleet stream, flat from 64 to 4 096 streams (13 800 / 13 494 / 13 521 / 13 529 B), so 4 096 streams hold about 53 MiB; one frame sent to every stream arrives at p95 0.18 / 0.14 / 0.14 / 0.13 ms, none lost (`make bench-tail`, after the hub shared each frame; 21.7 KB per stream before) | Flat per stream: at the `SSE_MAX_STREAMS` default of 256 that is 3.3 MiB. Memory and latency bind nothing up to 4 096 streams; file descriptors bind first. |

The ledger queue replaces the stream comparison's idle path with a Postgres probe.
Its database work depends on eligible active fleets as well as poll rate.
The delivery queue retains its separate ordering behavior: a slow destination delays later jobs.

What is deliberately not here: a production number, because nothing is
deployed; behaviour under real customer traffic, because there is none yet; and
any threshold, because the lanes print a delta against their baseline and never
fail a build on it. The rig measures one process against local datastores,
which is a floor and not a capacity plan.

### Admissions ledger measurements

The `0.65.0` production ledger path was measured with `BENCH_FLEETS=<n> BENCH_RUNNERS=8 make bench-lease PROFILE=rig KEEP_TEST_STATE=1`.
The canonical coverage run reset the owned rig before measurement; benchmarks reused those migrated services and swept their own fixtures.
Each row is the median of three runs on the shared development machine, with pool size 20.

All 15 runs completed: 1,500 drain events, zero failures, zero unsettled admissions and 9,240 of 9,240 fixture records swept.
These are production claim and report transactions, including admission settlement.

| Fleets / events | Drain statements per event | Drain commits per event | Dragonfly calls per event | Completed events / second | Claim round trips per lease | Idle discovery statements per poll |
| --- | --- | --- | --- | --- | --- | --- |
| 20 | 63.650 | 38.800 | 3.150 | 90.23 | 11.550 | 1.000 |
| 40 | 57.025 | 29.600 | 3.100 | 108.18 | 7.475 | 1.000 |
| 80 | 53.450 | 24.363 | 3.075 | 130.26 | 6.263 | 1.000 |
| 160 | 52.813 | 22.838 | 3.088 | 136.55 | 5.019 | 1.000 |
| 200 | 52.725 | 22.535 | 3.045 | 142.37 | 4.860 | 1.000 |

The 20-to-200 event increase used 8.28 times as many drain statements for ten times the completed work.
This measures client statement volume; it does not establish a bound on database rows examined or server CPU.
The empty-ledger precheck skips fleet scans; nonempty discovery still examines eligible fleets.

At 200 events the median lease and report p95 were 43.679 ms and 33.695 ms.
The raw claim `rate_per_second` counts polls, including misses; the table uses the drain's explicit completed-event rate.

The measured source was `ab66bb2ec992c87f0158470672a33bab30a58f38`, with integrated base `124c2bce303c95533745312c1ec725092bb604cb`.
Documentation edits were pending; runtime sources matched that commit throughout all measurements.

Idle Dragonfly samples include roughly 0.0024 commands per poll from the measurement window; serving discovery makes no Dragonfly command.
The later failed-park repair adds fenced cleanup when parking fails; successful claim and report statements remain unchanged.
No later timing run was made for that repair.

The fresh comparison checkout at `0fa9e55356571d57df69cd3a13fb3b90930b0e49` exited successfully but completed none of its 200 drain events, recorded 17 failures and left readiness depth 200.
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
| `R` | `agentsfleetd` API replica count | Deployment plan |
| `S` | Peak concurrent SSE tails | Product / dashboard usage |
| `P_conn` | Datastore concurrent-connection ceiling | The machine's file-descriptor limit; no longer a plan tier |
| `P_mem` | Configured `maxmemory` across the cluster's primaries | `scripts/dragonfly-cluster.sh` (512mb per node) |
| `poll_s` | Runner idle poll interval = `NO_WORK_RETRY_AFTER / 1000` | Config |

### Procedure

```
Step 1: Datastore connection budget (no per-fleet, no per-viewer term)
  datastore_conns = R * 3   (shared commands + hub + outbound reader)
  ASSERT datastore_conns + measured_reconnect_headroom <= P_conn
    if violated → raise the machine's descriptor limit, or add replicas
  NOTE: Z and S do not appear in the steady-state datastore socket count.
        S still sizes SSE_MAX_STREAMS, response memory, and HTTP capacity.

Step 2: Idle request rate (the lease-poll loop)
  idle_rps = N / poll_s
  fleet_discovery_statements_per_second = idle_rps
  Size Postgres using query latency at the target active fleet count Z.
  Authentication and accepted-kind placement add their own reads.
  Watch working-set growth against P_mem: retention, not eviction, is the
  configured policy, so a primary at its ceiling refuses writes.

Step 3: Hot-path throughput (the real wall)
  lease_report_qps = peak events/sec across the fleet
  size R + Postgres so lease/report p99 stays within budget under lease_report_qps
  (PG connection pooler assumed; sizing in the deployment runbook)

Step 4: Emit configuration
  DRAGONFLY_REQUEST_TIMEOUT_MS = 5000    (do not raise)
  DRAGONFLY_CONNECT_TIMEOUT_MS = 5000    (raise only for a measured cross-region handshake)
  NO_WORK_RETRY_AFTER   = <step 2 result>
  LEASE_TTL             = 30000   (compile-time; renewal carries long runs)
  agentsfleetd_replicas         = <step 3 result>
  runner_hosts             = N
```

### Anti-patterns (do NOT do these)

1. **Size Dragonfly connections by fleet or viewer count.** There is no per-fleet and no per-viewer connection. Normal boot opens `3·R` connections: shared commands, hub, and outbound reader.
2. **Tune `XREADGROUP BLOCK`.** It no longer exists on the hot path. Use `NO_WORK_RETRY_AFTER` for the idle-cost/latency trade.
3. **Add runners to fix lease/report latency.** Runners add compute, not control-plane throughput. Scale `agentsfleetd` replicas + Postgres for hot-path latency.
4. **Raise `DRAGONFLY_REQUEST_TIMEOUT_MS` above 5000.** A healthy datastore's p99 is single-digit-ms, and in-region 6PN is below that; >5 s is failure, not slowness.
5. **Put `SUBSCRIBE` on the shared command socket.** The hub owns a separate connection and fans out locally; request-path commands keep their multiplexed socket.
6. **Treat SSE streams as dedicated threads.** Rust serves async bodies under `SSE_MAX_STREAMS`; API admission and viewer capacity are separate budgets.
7. **Ignore active fleet count in the Postgres budget.** One discovery statement per poll can still perform many indexed head lookups. Measure at the intended fleet population.
8. **Replace randomized candidate ordering with a fixed prefix.** A bounded result with a fixed order can repeatedly exclude eligible fleets. Notification and affinity priority do not provide a strict fairness guarantee.
9. **Sum admission backlog gauges across replicas.** Each census reads the same deployment-wide ledger. Use the maximum; summing duplicates the same work.

---

## Failure and rebalance behavior

### Runner host loss

A runner that dies holds no datastore connection to leak and no Dragonfly consumer to reclaim. Its in-flight lease expires at `lease_expires_at`; the next runner's `lease` reclaim path re-issues the event with a higher fencing token ([`runner_fleet.md`](./runner_fleet.md) §"Failure recovery model"). Recovery latency is `LEASE_TTL` + poll density, the floor that section calls S0. There is **no connection storm** on runner loss — the survivors just keep polling.

**Failure domain scales with the assigned `worker_count`.** A host running a pool of N concurrent leases (M88_002) drops **N** in-flight runs on loss, not one. Each lease expires and re-leases independently (no batch coupling), so no work is lost — but N runs restart instead of one. This is the cost of the per-host utilization win; `worker_count=1` keeps the failure domain at one run. Operators size N against this tradeoff.

### Runner host add

A new runner registers and starts polling `lease`. No rebalance of in-flight work, no Dragonfly connection migration, no coordination. Sticky routing prefers the runner that ran the previous run but never blocks on it.

### Datastore failover (primary swap)

Reconnect, re-subscribe, and the `catching_up` gap viewers are told about: [`datastore_scaling.md`](./datastore_scaling.md) §"Target design".
Measure provider-specific failover and command-error behavior on the intended deployment before claiming recovery guarantees.

---

## What is explicitly out of scope here

- **Adaptive connection pooling.** The Rust command path uses a multiplexed connection, so the retired pool knobs do not apply.
- **The datastore's own target design.** [Datastore scaling](./datastore_scaling.md) is canonical for it and wins over this page: self-hosted Dragonfly in one region now, Dragonfly Cloud Swarm later if its control plane earns its bill. This page sizes the runtime that runs on top.
- **Postgres scaling.** Pgbouncer + plan sizing covered in the deployment runbook, not here — though after the cutover it is the **primary** scaling axis, so the runbook carries more weight than it did.
- **Placement / scheduler.** Label-aware assignment (`required_tags ⊆ runner.labels`) is M85_001; capacity-aware placement and autoscale-by-queue-depth stay out of scope (the non-goals fence in runner_fleet.md).
- **Multi-region topology.** Follow [datastore scaling](./datastore_scaling.md); provider-specific latency and limits require measurements on the selected deployment.
