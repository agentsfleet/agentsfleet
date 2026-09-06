---
type: explanation
audience: contributor
verified: 2026-09-06
product_version: 0.28.0
executable: false
---

# Datastore scaling requirements

| Decision | Requirement |
|---|---|
| Required target | Dragonfly Cloud Swarm must pass the application and recovery tests before this work is complete. |
| Existing deployment | Redis remains supported and remains the deployment default during implementation. |
| Rust client | Use redis-rs through the existing operation-specific boundaries. |
| Evidence | Compare Redis before and after each change on the same deployment and workload. |
| Scale | Report measured capacity against an explicit workload; fleet population alone proves no throughput guarantee. |

## What it is

This page records the approved requirements for datastore scaling. It does not claim that Dragonfly support or million-fleet capacity has shipped.

The [datastore scaling roadmap](./roadmap.md#datastore-scaling-and-redis-parity) links the pending implementation and benchmark plans.
The existing runtime topology remains documented in [data_flow.md](./data_flow.md) and [runner_fleet.md](./runner_fleet.md).
Implementation must update those pages in the same commit as any topology change.

## Why it exists

Redis carries event streams, readiness hints, authentication state, and outbound delivery.
Those uses require different retention and failure rules.
Treating all of them as disposable cached values would weaken accepted-work recovery.

Dragonfly Cloud Swarm is the required destination.
There is no Dragonfly single-shard migration stage.
Changing that destination requires an explicit user decision.
Redis support remains part of the acceptance requirements.

## How it behaves

The work has separate sections for durability, retention and backpressure, shared coordination, and cluster compatibility.
Benchmark preparation precedes behavior changes.
Each section produces Redis regression evidence before a Dragonfly cutover is considered.

Use redis-rs through `FleetStreams`, `ReadyIndex`, `SessionStore`, `OutboundQueue`, and `SubscriptionHub`.
Keep local identity caches independent of this migration.
A generic cache interface must not erase queue, authentication, or delivery semantics.

Work accepted by the target design must remain recoverable after the in-memory datastore is lost, provided the durable database survives.
This requirement includes pending inbound work and queued outbound answers.
Third-party side effects retain their documented retry and idempotency limits; this page makes no exactly-once promise for external services.

Build and test multi-shard behavior from the first Dragonfly integration.
The evidence sequence is Redis baseline, Redis with the added capability, then Dragonfly Cloud Swarm.
Swarm requires cluster-aware connections, compatible multi-key operations, subscription recovery, and scans covering the intended shards.
Replica reads must not weaken admission, authentication, fencing, or deduplication decisions.

Deploy implementation increments to the same application services while retaining their Redis endpoint.
Record an immutable revision and configuration for every comparison.
Changes in instance count, database capacity, region, payload distribution, or background traffic must be disclosed and separated from code improvements.

Changing the selected datastore requires an explicit rollout action after evidence passes.
Do not send an accepted event to both backends for comparison.
A failed connection must not silently select another backend or an empty queue.

## Limits

One million stored fleets, active fleets, concurrent runs, and runner processes are separate workload dimensions.
Reports must state all four, together with offered load, completion rate, queue age, memory, and database cost.
Small deployment probes prove behavior at their tested load; they do not prove the million-fleet target.

Fault injection and saturation testing use isolated datastores and synthetic destinations.
Shared deployments use bounded fixture traffic and cleanup that only removes the run's own records.
No shared Redis flush, database reset, failover exercise, or provider switch follows from spec approval alone.

Prepare a local multi-shard Dragonfly cluster and load tests before activating the Cloud trial.
Use the trial to validate managed networking, failover, capacity, and Swarm behavior.
Missing Cloud evidence leaves the Dragonfly acceptance rows incomplete.

## Related pages

- [Scaling](./scaling.md) describes existing capacity assumptions that measurements must verify.
- [Datastore scaling roadmap](./roadmap.md#datastore-scaling-and-redis-parity) identifies the shared benchmark drivers and implementation plan.
- [Dragonfly Cloud data stores](https://www.dragonflydb.io/docs/cloud/datastores) describes single-shard, Swarm, eviction, and replica settings.
- [Dragonfly compatibility](https://www.dragonflydb.io/docs/command-reference/compatibility) distinguishes command support from identical behavior.
- [Dragonfly AOF documentation](https://www.dragonflydb.io/docs/managing-dragonfly/aof) must be checked when reviewing durability assumptions.
