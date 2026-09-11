---
type: explanation
audience: contributor
verified: 2026-09-11
product_version: 0.30.0
executable: false
---

# Datastore scaling requirements

| Decision | Requirement |
|---|---|
| Required target | Dragonfly Cloud Swarm is the sole deployment target; no single-shard migration stage. |
| Redis retirement | Retire the Redis server after verified cutover and reconciliation of accepted work. |
| Rust client | Keep redis-rs and Redis protocol compatibility through the existing operation-specific boundaries. |
| Evidence | Capture one historical Redis baseline on an isolated rig, then validate Dragonfly behavior, recovery, and capacity. |
| Scale | Report measured capacity against an explicit workload; fleet population alone proves no throughput guarantee. |

## What it is

This page records the approved datastore migration requirements. Dragonfly support and million-fleet capacity remain unproven until implementation evidence passes.

The [datastore roadmap](./roadmap.md#datastore-scaling-and-redis-parity) links the benchmark drivers and pending implementation.
The configured Redis deployment remains in place until the cutover action.
The runtime topology is documented in [data_flow.md](./data_flow.md) and [runner_fleet.md](./runner_fleet.md).

## Why it exists

Redis carries event streams, readiness hints, authentication state, and outbound delivery.
These uses require different retention and failure rules.
A provider change must preserve application behavior and accepted work.

The approved destination is Dragonfly Cloud Swarm, followed by Redis retirement.
Permanent Redis server support and repeated same-deployment Redis comparisons are outside this migration.
Redis protocol compatibility remains useful because Swarm uses Redis Cluster clients.

## How it behaves

Capture the existing implementation's Redis measurements once on an isolated rig, using M188's Make targets.
Record revision, workload, application capacity, PostgreSQL capacity, network path, and datastore resources.
Freeze absolute latency, queue-age, memory, recovery, and cost budgets before grading Dragonfly acceptance.

Reuse M188's benchmark drivers for Dragonfly and extend them to measure accepted and completed work together.
Different resources or workload manifests cannot establish a provider speedup.
Redis results remain historical evidence; subsequent implementation comparisons use Dragonfly.

Use redis-rs through `FleetStreams`, `ReadyIndex`, `SessionStore`, `OutboundQueue`, and `SubscriptionHub`.
Keep local identity caches independent of this migration.
A generic cache interface must not erase queue, authentication, or delivery semantics.

Build real multi-shard support first, including cluster routing, compatible atomic keys, isolated blocking readers, subscription recovery, and scans across shards.
Then complete durable acceptance, retention, backpressure, and measured coordination improvements before Cloud acceptance.
Correctness-sensitive reads use primaries; a failed connection never silently selects another provider.

Accepted inbound work and queued outbound answers must survive in-memory datastore loss when PostgreSQL survives.
Replay preserves logical event identity, fencing, and single internal settlement across new stream IDs.
External services retain their documented retry and idempotency limits; exactly-once external effects are not promised.

Prepare local multi-shard tests before activating the Cloud trial.
Cloud proof covers managed networking, failover, restore, resizing, and resharding.
Shared deployments receive only bounded application probes; fault injection and saturation use isolated datastores and synthetic destinations.

Rehearse cutover with queued work, active leases, outbound answers, authentication state, deduplication records, and expiry.
The procedure must handle records accepted before durable replay existed, and account for each migrated, drained, rebuilt, or expired state class.
Do not send an accepted event to both backends for comparison.

Record the last safe abort point before Dragonfly accepts new work.
After that point, recovery uses the rehearsed durable replay procedure; changing an endpoint alone is insufficient.
Retire Redis only after reconciliation, verification, and the recorded observation window pass.

## Limits

One million stored fleets, active fleets, concurrent runs, and runner processes are separate workload dimensions.
Reports include all four, offered load, completion rate, queue age, memory, and database cost.
Small probes establish behavior only at their tested load.

Implementation preparation records budgets, resource requirements, secret references, and the cutover procedure before dependent runs.
Missing Cloud evidence leaves acceptance incomplete.
Spec approval alone does not activate a trial, change billing, reset shared data, or perform the live cutover.

## Related pages

- [Scaling](./scaling.md) describes capacity assumptions that measurements must verify.
- [Datastore roadmap](./roadmap.md#datastore-scaling-and-redis-parity) identifies the shared benchmark drivers and migration plan.
- [Dragonfly Cloud data stores](https://www.dragonflydb.io/docs/cloud/datastores) describes Swarm, eviction, and replica settings.
- [Dragonfly compatibility](https://www.dragonflydb.io/docs/command-reference/compatibility) distinguishes command support from identical behavior.
- [Dragonfly cluster mode](https://www.dragonflydb.io/docs/managing-dragonfly/cluster-mode) explains local setup and Cloud management boundaries.
