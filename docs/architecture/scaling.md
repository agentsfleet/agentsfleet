---
type: explanation
audience: contributor
verified: 2026-11-25
product_version: 0.90.0
executable: false
---

# Scaling

## What it is

This page says how to size an `agentsfleet` deployment and which part binds first.
The parts are `agentsfleetd` API replicas, Postgres, the NATS bus, and runner hosts.
[Datastores](./datastore_scaling.md) owns what each store holds; this page owns how big each must be.

| Fact | Value | Source |
|---|---|---|
| Bus connections | One NATS connection per replica | `afd_bus::nats::connect` |
| Idle scheduler statements | None; a pass runs only on a wake, a bus gap or freed runner capacity | `afd_gateway/src/scheduler/mod.rs` |
| Candidates per assignment pass | 64 by default | `MAX_CANDIDATES_PER_POLL`, `afd_fleet/src/run/assign/mod.rs` |
| Server-Sent Events (SSE) streams | 256 per replica by default; 503 past it | `SSE_MAX_STREAMS` |
| API requests in flight | 256 per replica by default; 429 past it | `DEFAULT_MAX_IN_FLIGHT`, `afd_http/src/in_flight.rs` |
| Run time to live (TTL) | 38 s, extended by each renewal | `RUN_TTL`, `afd_runner/src/timing/mod.rs` |
| Workers per runner host | 1 by default, at most 64 | `worker_count`; `MAX_WORKERS` in `af_core/src/limits.rs` |
| What binds first | API replicas and Postgres writes on the run and report path | [What binds first](#what-binds-first) |

## Why it exists

Sizing by the wrong unit wastes money or drops work.
Fleet count and viewer count add no bus connections, while active fleets do add Postgres work.
This page names the unit for each part.

## How it behaves

### What binds first

The run and report path does the durable writes.
A run opens its turn; renewals log usage; the report settles the turn, writes usage, releases the thread and owes any delivery.
That report is one statement (`afd_fleet/src/run/sql/commit_report.sql`).

Both `agentsfleetd` replicas and Postgres scale out, and the hot path is shardable per fleet.
The symptom is a climbing run or report p99, with Postgres connections saturated or rows contended.
The fix is more replicas and a larger Postgres.

Runners add compute, not control-plane throughput.

### Datastore limits

| Limit | Value | Where |
|---|---|---|
| NATS nodes | Two in production, two in development, three locally | [Datastores](./datastore_scaling.md#deployment) |
| NATS connections | One per replica; bound by the machine's file descriptors | `afd_bus::nats::connect` |
| Postgres `max_connections` | 25 on each PlanetScale cluster today | `deploy/fly/agentsfleetd-*/fly.toml` |
| Postgres pool per replica | `DATABASE_POOL_SIZE`; each deployment sets its own | `afd_db/src/config/sizing.rs` |
| Pooler | PlanetScale's PgBouncer on port 6432 for the API; the migrator stays on 5432 | `playbooks/founding/02_preflight/02_credentials.sh` |
| Region | Production Postgres in `us-east` beside Fly `iad`; development in `aws-us-east-2` | `playbooks/operations/database_region_move/` |

Every replica's pool plus the migrator must fit under `max_connections`.
Past it, Postgres refuses with `53300`, and sqlx retries silently until the acquire timeout.
The daemon reports that as a pool stall and answers `UZ-INTERNAL-001`; it is not an outage.

A checkout pings only a connection idle for at least `PING_AFTER_IDLE`, 1 s (`afd_db/src/pool/mod.rs`).
`agentsfleet_db_pool_checkout_pings_total` counts those pings.

### Connection and stream budgets

Publishers, wake listeners, the outbound worker and every viewer share the replica's one NATS connection.
The hub keeps one native subscription per subject with local readers, so a viewer costs a local receiver.

Each SSE response holds a permit until its body drops, and waiting bodies hold no operating-system thread.
About 300 file descriptors go to requests, Postgres and NATS, so a stock 1,024 limit leaves room for 256 streams.
The daemon neither raises `RLIMIT_NOFILE` nor checks `SSE_MAX_STREAMS` against it.

The in-flight ceiling covers API routes only, through tower's `GlobalConcurrencyLimitLayer` under `LoadShedLayer`.
A shed request gets 429 `UZ-API-001` with `Retry-After: 1`; probe and stream routes sit outside it.

### Assignment cost

An assignment pass first asks whether any turn waits (`afd_fleet/src/run/sql/run/select_waiting_work.sql`).
An empty queue ends the pass with no fleet scan.

When work waits, discovery reads each eligible fleet's oldest unsettled turn through `idx_turns_thread_id_sequence_unsettled`.
The 64-candidate limit bounds what returns, not how many active fleets Postgres examines.
So size Postgres against active fleet count and pass rate together.

Candidates come out in priority order: recent wakes, held fleets and previous holders, then the turn that has waited longest.
Assignment locks the thread row and head turn, stamps the fence and commits before pushing the run.

Each recurring read below is served by an index.

| Read | Index |
|---|---|
| A fleet's active run | `idx_runs_fleet_id_status_fencing_token` |
| A workspace's turn page | `idx_turns_workspace_id_created_at_id` |
| A fleet's turn page and chat | `idx_turns_thread_id_created_at_id`, with `stx_turns_workspace_id_thread_id` telling the planner the thread implies the workspace |
| The fleet list | `idx_fleets_workspace_id_created_at_id` |
| The runner list | `idx_runners_created_at_id` |

Fleet memory load sorts on purpose: it reads a fleet's whole memory with no `LIMIT`, where a sort beats an ordered index scan.

### Tuning knobs

| Knob | Default | Turn it when |
|---|---|---|
| `[runners] turns_per_assignment_pass` | 64 | Discovery latency and candidate attempts are measured; a larger value adds contention per pass |
| `[runner.run] ttl_seconds` or `AGENTSFLEET_RUN_TTL_SECONDS` | 38, at most 3,600 (`MAX_RUN_SECONDS`) | A tighter recovery deadline is needed; run length does not size it |
| `SSE_MAX_STREAMS` | 256 | The host's descriptor limit is raised to match, after measuring refusals and memory |
| `[server] max_in_flight_requests` | 256 | `request_shed` logs appear while CPU and the pool have room |
| `agentsfleetd` replicas | Per deployment | Run or report p99 climbs, or viewers keep hitting the SSE ceiling |
| Runner hosts | Per operator | More compute is needed |
| `worker_count` per runner | 1 | A host has spare cores while one long run holds it |

[Configuration reference](./config_reference.md) owns every key's type and range.
The bus `publish_deadline_ms` (250) and `connect_timeout_ms` (5000) are compiled defaults in `afd_bus::nats::Tuning`; nothing sets them.

### Workers per host

`worker_count` sets how many fleets one runner host runs at once.
It is a capacity knob, because workers share the host's CPU, memory, disk and network.

It also widens the failure domain: one host loss interrupts N runs, not one.
At the 64-worker ceiling with the 2-core sandbox default, a host under 128 cores is oversubscribed.

### Current measurements

Each row is a committed baseline in `bench/baselines/`, measured with `make bench-<lane> PROFILE=rig`.
The rig is one developer machine against compose datastores, so the numbers are a floor, not a capacity plan.

| Lane | Result | File and revision |
|---|---|---|
| Run drain, 200 fleets, 8 runners | 211.6 runs/s; 21.0 Postgres statements and 13.4 commits per run; run p95 28.4 ms, report p95 11.7 ms; 0 idle statements | `runs.rig.json`, `480802c84` |
| Steer intake, 50 fleets, 8 senders | 2,868 accepted/s at p95 4.3 ms; 2.0 statements per steer | `intake.rig.json`, `2bea47c3d` |
| Outbound, 200 jobs, one slow destination | 34.7 delivered/s; p95 5,386 ms | `delivery.rig.json`, `2bea47c3d` |
| Candidate query, 10,000 fleets with a turn each | 25.5 ms; 0.3 ms at 10 fleets | `idle.rig.json`, `2bea47c3d` |
| Live tail, 1,024 viewers on one fleet | About one allocation per delivered frame; receive p95 2.4 to 2.9 ms from 200 B to 64 KiB | `stream.rig.json`, `6b0caea35` |
| Live tail, 4,096 open streams | About 22.9 KB of heap per stream; one frame reaches all at p95 12.4 ms, none lost | `stream.rig.json`, `6b0caea35` |

A run's 21 statements by path are in the same file, as `drain_statements_per_lease_*`.
No production number exists, and no lane fails a build on a regression.

### Sizing procedure

```
Inputs: R replicas, N runner hosts, Z fleets, S peak SSE viewers, P_conn NATS connection limit

1. Bus:      R connections; assert R + reconnect headroom <= P_conn.
             Z and S add no bus connections.
2. Idle:     0 scheduler statements per second.
             Measure the candidate query at fleet count Z.
3. Hot path: peak turns per second -> size R and Postgres so run and report p99 hold.
4. Streams:  S / R <= SSE_MAX_STREAMS, with descriptors and memory to match.
5. Runners:  N hosts x worker_count >= peak concurrent runs.
```

### Anti-patterns

1. **Sizing bus connections by fleet or viewer count.** Each replica holds one connection.
2. **Adding runners to fix run or report latency.** Scale replicas and Postgres.
3. **Ignoring active fleet count in the Postgres budget.** One pass can do many indexed head lookups.
4. **Ordering candidates by a fixed key that ignores queue age.** Such an order can starve eligible fleets. The tie-break ages each head by how long it has waited while its thread was free, so the longest-waiting turn leads.
5. **Raising `publish_deadline_ms` to ride out an outage.** The gap marker recovers the frame; a longer wait buys nothing.
6. **Expecting a timer to recover a lost ping or wake.** None runs; the next event does.
7. **Reading the backlog from a gauge.** Count unsettled `thread.turns` rows in Postgres.

### Runner host loss and add

A dead host leaks no datastore or bus connection.
Its runs lapse at `run_expires_at`, and the next assignment re-issues each turn under a higher fence.
[Runner fleet](./runner_fleet.md#failure-recovery-model) owns the departure timers and the recovery path.

A new host opens its connection and advertises capacity, and existing runs keep their owners.

## Limits

- No production measurement exists; the rig numbers come from one machine.
- The outbound lane has not been re-measured against per-destination lanes.
- Discovery work grows with eligible active fleets; no partitioned scheduler exists.
- Multi-region deployment is unsized.

## Related

- [Datastores](./datastore_scaling.md): where each fact lives and what a lost signal costs.
- [Configuration reference](./config_reference.md): every knob's key and default.
- [Data flow](./data_flow.md): the live tail and the connection topology.
- [Runner fleet](./runner_fleet.md): enrolment, capacity and recovery.
- [Observability](./observability.md): the metrics named here.
