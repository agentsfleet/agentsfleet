---
type: explanation
audience: contributor
verified: 2026-09-12
product_version: 0.30.0
executable: false
---

# Datastore scaling requirements and target design

| Decision | Requirement |
|---|---|
| Required target | Dragonfly Cloud Swarm must pass the application and recovery tests before this work is complete. |
| Existing deployment | Redis is not a supported backend. Dragonfly, cluster-only, is the datastore; the cutover is recorded in the M192_001 Discovery (Indy, 2026-09-12). |
| Rust client | redis-rs `cluster_async` over RESP3 through the existing operation-specific boundaries is the only transport; primaries only; there is no standalone path and no topology selector. |
| Evidence | Compare the local cluster before and after each change on the same rig and workload; the Redis baseline (closed PR #681) is a historical reference point only. |
| Scale | Report measured capacity against an explicit workload; fleet population alone proves no throughput guarantee. |

## What it is

This page records the approved requirements for datastore scaling and the target design the implementation builds toward. It does not claim that Dragonfly support or million-fleet capacity has shipped; the sections below marked *target* describe what the active spec builds, and the runtime pages (`data_flow.md`, `runner_fleet.md`) describe what runs today until a Section lands and updates them in the same commit.

The [datastore scaling roadmap](./roadmap.md#datastore-scaling) links the implementation spec.

## Why it exists

Redis carries event streams, readiness hints, authentication state, and outbound delivery. Those uses require different retention and failure rules. Treating all of them as disposable cached values would weaken accepted-work recovery.

Dragonfly Cloud Swarm is the required destination. There is no Dragonfly single-shard migration stage. Changing that destination requires an explicit user decision. Redis support remains part of the acceptance requirements.

## What the cluster probe showed (2026-09-12)

A four-node Dragonfly v1.40.2 cluster (two primaries, one replica each, one network namespace, `127.0.0.1:7001-7004`) was driven with the daemon's current key and script shapes. The results decided the target design:

| Current shape | On the cluster | Consequence |
|---|---|---|
| `append_once`: one Lua over `webhook:dedup:<fleet>:<id>` and `fleet:<fleet>:events` | `CROSSSLOT Keys in request don't hash to the same slot` | Acceptance identity cannot live in a Redis claim beside the stream; it moves to PostgreSQL. |
| `fleet:ready`: one hash for the deployment | One slot, one node, every poll and every mark | Readiness is partitioned by fleet hash. |
| `PUBLISH`/`SUBSCRIBE` on `fleet:<id>:activity` | A cluster broadcasts every publish to every node | Sharded pub/sub (`SPUBLISH`/`SSUBSCRIBE`), routed by the channel's slot. |
| `SSUBSCRIBE` sent to a node that does not own the slot | Answered `OK`, not `MOVED` | Subscriber placement is proven under slot migration, not assumed from Redis behaviour. |
| `SCAN` for `auth:session:*` | Scans one node | Scans fan out over every primary from the live topology. |
| `DFLYCLUSTER CONFIG` with a `migrations` entry | 4096 slots moved and finalised; keys followed | Slot movement is a test fixture the owned local cluster can drive. |

## Target design

**One boundary, one transport.** `afd_datastore` (renamed from `afd_redis`; the `REDIS_*` environment names are unchanged) keeps its surfaces — `Redis`, `Dedicated`, `FleetStreams`, `ReadyIndex`, `SessionStore`, `SubscriptionHub` — over one `ClusterConnection` per process for ordinary commands. There is no `REDIS_TOPOLOGY` and no standalone backend: a seed that is not a cluster refuses boot. A blocking consumer (`Dedicated`) holds its OWN `ClusterConnection`, never a borrowed one, because the driver keeps exactly one socket per node and applies one reply deadline to every command on a connection — a parked read on a shared handle would stall the owning node's only socket and impose the park-sized deadline on every other caller. Business crates never learn the topology, and no business code branches on it. Correctness-sensitive commands go to primaries; `read_from_replicas` is never enabled.

**Acceptance is a PostgreSQL row, the queue is a receipt.** Every producer (steer, webhook, schedule fire, gate continuation, install, outbound answer) commits an admission row carrying its producer identity before any success response. The single-key `XADD` follows and is recorded back as the physical receipt; a replay dispatcher re-appends admitted rows that never got one. Duplicate producer identity is a unique-index conflict, which is what the Redis dedup claim used to be. Settlement and billing key on the admission row, so a replayed receipt cannot debit twice. Losing the in-memory datastore loses no accepted work while PostgreSQL survives.

**Every multi-key operation shares a slot or does not exist.** The at-most-once append is the crate's only two-key script, and its marker key carries the stream key as its hash tag — `{fleet:<id>:events}:<scope-prefix><once-id>` — so both keys share a slot by construction. Every other script — token-checked readiness clear, increment-in-window, session verify/approve/abort — is single-key. A new multi-key script is refused by the cluster suite.

**Sharded pub/sub.** The hub subscribes with `SSUBSCRIBE` and the tail publishes with `SPUBLISH`. Channel names (`fleet:<id>:activity`) and the SSE parser are unchanged; the channel routes by its own slot. Measured on the local cluster: a slot migration strands the subscription — the server pushes `sunsubscribe` and the new owner counts zero subscribers — so the hub re-issues `SSUBSCRIBE` on that push and after a reconnect, with no duplicate local frame.

**Readiness is partitioned.** `fleet:ready` becomes `fleet:ready:{p}` for a fixed partition count chosen from measurement and recorded here when §4 lands. Each runner poll rotates a partition cursor under a candidate budget; the token-checked clear stays a single-key script per partition.

**Retention is bounded below by unfinished work.** Trimming never removes an entry that is pending or has no receipt; acknowledged history keeps a fixed bound. A lost consumer group is recreated from the ledger's replay cursor. Admission budgets refuse with an explicit class; `OOM` and quota replies are classified, never swallowed. Preflight refuses an eviction-enabled node.

**Local rig.** The compose service `dragonfly` runs four cluster processes in one network namespace announced on `127.0.0.1` with host ports equal to announced ports, bootstraps replication and `DFLYCLUSTER CONFIG` on every start, and is reset only when ownership is proven. A fifth process in the same container, `--cluster_mode=emulated --tls`, serves the TLS trust proof alone; ordinary suites take the plaintext cluster so hundreds of connects do not each pay a handshake. There is no Redis service.

## How it behaves

The evidence sequence is: prototype on the local cluster, integrate the proven shape behind the boundary, prove cluster behaviour in the lane, then Dragonfly Cloud Swarm. Redis parity is not a gate, because Redis is not a supported backend.

Work accepted by the target design remains recoverable after the in-memory datastore is lost, provided the durable database survives. This includes pending inbound work and queued outbound answers. Third-party side effects retain their documented retry and idempotency limits; this page makes no exactly-once promise for external services.

Deploy implementation increments to the same application services while retaining their Dragonfly endpoint. Record an immutable revision and configuration for every comparison. Changes in instance count, database capacity, region, payload distribution, or background traffic must be disclosed and separated from code improvements.

Changing the selected datastore requires an explicit rollout action after evidence passes, and an import receipt the daemon checks before accepting work. A failed connection must not silently fall back to a standalone path or an empty queue.

## Limits

One million stored fleets, active fleets, concurrent runs, and runner processes are separate workload dimensions. Reports must state all four, together with offered load, completion rate, queue age, memory, and database cost. Small deployment probes prove behavior at their tested load; they do not prove the million-fleet target.

Fault injection and saturation testing use isolated datastores and synthetic destinations. Shared deployments use bounded fixture traffic and cleanup that only removes the run's own records. No shared Redis flush, database reset, failover exercise, or provider switch follows from spec approval alone.

The historical Redis baseline (closed PR #681) is a reference for the local rig only. Cloud evidence is required before the Dragonfly acceptance rows are complete; missing Cloud evidence leaves them incomplete.

## Related pages

- [Scaling](./scaling.md) describes existing capacity assumptions that measurements must verify.
- [Data flow](./data_flow.md) and [runner fleet](./runner_fleet.md) describe the running topology; each Section updates them with its code.
- [Test isolation on a shared datastore (rules ISO-1 to ISO-3)](./testing.md#test-isolation-on-a-shared-datastore-rules-iso-1-to-iso-3) governs isolation on the shared lane datastores.
- [Dragonfly cluster mode](https://github.com/dragonflydb/dragonfly/blob/main/docs/cluster-mode.md), [compatibility](https://www.dragonflydb.io/docs/command-reference/compatibility), and [AOF](https://www.dragonflydb.io/docs/managing-dragonfly/aof) are checked against the pinned image.
