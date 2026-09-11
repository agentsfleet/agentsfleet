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
| Target | Dragonfly Cloud Swarm; no single-shard migration or permanent Redis server support. |
| Design | Use Dragonfly-supported primitives; preserving Redis internals is not a goal. |
| Client | Keep redis-rs where the prototype proves its capabilities; use no custom cluster protocol. |
| Readiness | Prototype risks, then integrate and validate before live cutover. |
| Evidence | One historical Redis campaign; subsequent acceptance measures Dragonfly behavior, recovery, and capacity. |
| Completion | Implementation readiness and live Redis retirement have separate proofs. |

## What it is

This page defines the target design. The configured Redis deployment remains unchanged until the approved live cutover.
No prototype, Cloud test, or migration has run as part of this documentation revision.

[M192_001](../v2/pending/M192_001_P0_API_INFRA_OBS_DRAGONFLY_SCALE_REDIS_PARITY.md) owns prototypes, implementation, Cloud readiness, and the migration rehearsal.
[M192_002](../v2/pending/M192_002_P0_INFRA_OBS_DRAGONFLY_CUTOVER_REDIS_RETIREMENT.md) owns the proposed live cutover and retirement follow-up.
The [review resolution](../v2/reviews/M192_REVIEW_RESOLUTION.md) records the findings and remaining evidence.

## Why it exists

The runtime uses ordinary pub/sub and untagged multi-key scripts.
Dragonfly's documented cluster behavior requires sharded pub/sub and one slot per atomic operation.
Changing a connection URL cannot address these differences.

Redis protocol compatibility keeps useful client tooling. It does not require retaining the daemon's connection topology, key shapes, or server support.
Workload and failure tests decide whether the Dragonfly design is suitable.

## How it behaves

### Prototype before integration

Start with Dragonfly v1.40.2, release commit `e94300e6990093ec093cfb00d60c2e77ea4907e4`, and redis-rs 1.6.0 from the lockfile.
Resolve and record an immutable container digest before running tests; a moving image tag is insufficient.
These are prototype candidates, not verified compatibility claims; version changes require rerunning the complete affected matrix.

Use at least two local primaries owning distinct slots, with replicas for restart and movement cases.
Local administration uses the version-matched `DFLYCLUSTER CONFIG` and `migrations[]` procedure, including configuration restoration after restart.
Cloud-managed failover must still be tested on Swarm; local control commands are not Cloud acceptance evidence.

Each prototype records its revision, setup, fault trigger, expected result, observed result, raw files, and disposition.
A failed prototype stops the dependent integration until corrected and rerun.
Retain successful proofs as integration tests; remove replaced experiments rather than adding an unused runtime.

### Sharded live tail

Use `SPUBLISH`, `SSUBSCRIBE`, and `SUNSUBSCRIBE`. Standard and pattern pub/sub cannot be used on the target.
Use RESP3 push delivery through redis-rs, with a bounded subscription registry and bounded queues for viewers.
The prototype must establish the client API and recovery behavior before those choices become production code.

The hub shares subscriptions by channel and owning primary, rather than promising one subscription socket per process.
Topology refresh and reconnect rebuild referenced subscriptions; dropping the final reader releases its subscription and unused connection resources.
Count connections, tasks, queued bytes, lag, and recovery time across repeated movement and cancellation.

A publisher starting through another seed must route to the channel owner; cross-node broadcast is not promised.
Probe both wrong-node redirection and routed delivery, including subscription acknowledgment before publishing.
On movement, disconnect, or migration push notifications, refresh ownership and restore subscriptions within the frozen recovery budget.

Permanent command, authentication, or topology errors fail readiness; they must not become endless reconnect loops hidden behind successful boot.
Slow readers receive explicit lag or stream closure so clients can recover durable history.
Transient token frames remain best-effort; durable history backfill does not promise recovery of every token frame.

### Keys and atomic work

Let `S` be the exact existing event-stream key returned by `fleet_stream_key(fleet_id)`.
Keep that stream name. Build related claim keys with the literal hash tag `{S}`, substituting S's full bytes inside the braces.
Use `activity:{S}` for the sharded live-tail channel, with the same literal-tag substitution.

Validate fleet identifiers before key construction; braces in external provider identifiers must not choose the first hash tag.
Update both `afd_redis`'s builder and `afd_sse`'s channel parser together, including workspace fan-in and tenant-isolation proofs.
Check actual `CLUSTER KEYSLOT` values for every atomic key set; shared readiness remains a separate operation with repair.

Migration rewrites old claim names and activity subscriptions explicitly.
Claims keep their original remaining expiry and original event identity; retries cannot reopen a deduplication window.
Do not create one deployment-wide hash tag that concentrates all fleets on one slot.

### Durable event identity

Commit recoverable ingress and its stable logical event identity in PostgreSQL before publishing any runnable queue entry or returning acceptance.
Keep the public `event_id` string in numeric stream-ID form; preserve imported event IDs verbatim.
Generate new identities from a PostgreSQL-serialized per-fleet monotonic allocator, initialized above imported IDs and saved with the acceptance row.

The durable identity travels in the internal queue envelope. The physical stream receipt may change on replay and is used only for acknowledgment and pending-entry management.
Lease bookkeeping, event history, fencing, approvals, and billing use the stable identity; the runner's public fields retain their shape.
Store dispatch state and serialize per-fleet dispatch so retries cannot reorder unfinished work behind new work.

Prototype concurrent admission, clock rollback, imported high watermarks, lost append replies, and replay of older work after newer physical entries.
Do not rely on explicit-ID `XADD` replay: a nonempty stream can reject an older ID.
Duplicate receipts must never cause duplicate runs after terminal settlement, receive charges, or terminal billing.
Commit each debit with its durable idempotency marker; crashes must not leave a marker without its charge or a charge without its marker.

Keep a durable deduplication record until its retry window and unfinished-work obligation both expire.
Queue-loss recovery rebuilds claims and readiness from durable state, independently of ephemeral claim expiry.
Test both provider retries and dispatcher retries after queue destruction.

### Measured coordination partitioning

Swarm manages server shards and slot movement; the application must not implement a replacement sharding manager.
Cluster routing, atomic-key placement, and sharded pub/sub are required by the target.
Additional partitioning of readiness indexes and queues depends on measured throughput, memory, and fairness budgets.

Prototype a versioned readiness map keyed by stable fleet hash, with bounded rotating poll cursors and token-checked clears.
Compare it against the existing layout under identical load; adopt it when required to meet budgets and proven better.
If adopted, fence map transitions and replay durable readiness rather than losing work during key replacement.

Compare destination-partitioned fixture delivery with the existing worker; preserve order per destination and bound concurrent retries.
Measure partition progress and eligible-work age separately from aggregate throughput.
A passing existing design can remain; a failed budget requires redesign, not a waived acceptance row.

### Blocking reads, scans, and retention

A blocking reader owns a dedicated node connection resolved from its stream's slot.
Refresh its owner on redirects and reconnects; cancellation closes the blocking connection without consuming the shared command pool.
Measure unrelated command latency while reads block, and verify no abandoned socket or task accumulates.

Scan all primaries, deduplicate results, and repeat across topology changes until the scoped reconciliation criterion is met.
A cursor from one node cannot stand for the whole cluster.
Fixture cleanup must verify its own records are gone without deleting another run's records.

Use No Eviction, bounded durable admission, and pending-aware stream cleanup.
Unconditional approximate `MAXLEN` trimming cannot delete the only recoverable unfinished entry.
Missing consumer groups recover against durable dispatch and settlement state; recreating at `$` must not skip accepted work.

### Workload and evidence

Bench-only capture and safety changes may precede historical baseline revision B.
Verify production source and schema trees at B match the recorded comparison revision B0.
Capture three runs of each existing M188 lane, copying each result immediately to a unique campaign/lane/sample location.

A sidecar records exact parameters, payload bytes, window, tool versions, resources, topology, and revision.
Do not invent a seed or offered-rate control absent from the historical driver.
The million-fleet, thousand-runner-process workload belongs to the Dragonfly combined-load tests, not historical capture.

Before opening any destructive workload, validate resolved datastore addresses and the dedicated deployment identity for PostgreSQL and Dragonfly.
A profile label, hostname spelling, or possession of credentials is not proof of isolation.
Reject a remote/shared endpoint under rig settings and guard every discovered cluster node against unexpected addresses.

Run destructive and saturation workloads only on owned disposable infrastructure.
Make fixtures unable to lease or acknowledge non-fixture work, with cancellation cleanup and orphan recovery tested.
Shared application probes are bounded and never share a synthetic outbound consumer with real delivery.

Raw files include collector output, command logs, server identity, topology, samples, and cleanup results, with SHA-256 digests.
The grader recomputes digests and statistics and checks consistency; hashes detect alteration, not who generated a file.
Cloud evidence also requires an authenticated CI run reference, matching revision, immutable artifact identity, and a Cloud datastore identifier verified through its control plane.

Live actions remain manual evidence. The named human verifies the live revision, observation window, reconciliation, and retirement record.
A hand-written summary, local run relabeled Cloud, or rehearsal relabeled live cannot satisfy acceptance.
An unavailable attestation or verification API blocks the claim rather than trusting the label.

### Outbound and delivery boundaries

The pinned Rust report path has no production caller of `OutboundQueue::enqueue`.
For this review draft, preserve that behavior: prototype and measure the existing outbound worker using synthetic fixtures only.
Adding report-to-delivery behavior requires an explicit scope decision and public documentation; the migration must not silently introduce it.

Before cutover, inventory the source outbound queue rather than assuming it is empty.
If historical jobs exist, the migration rehearsal must prove their disposition; unsupported jobs block cutover for a decision.
Inbound producers include steer, per-fleet webhook, App fan-out, cron, continuation, and repair verification; install creates groups rather than work.

### Delivery and cutover boundary

The proposed first Pull Request completes M192_001 readiness, including Cloud proof and offline migration tools.
M192_002 carries every live cutover and retirement requirement as P0; readiness is not a migration-complete claim.
Before landing a cluster-only build, establish a deployment hold or staged-image procedure so merge cannot deploy it to standalone Upstash.

Only the bounded migration tool reads source standalone Redis. The daemon has no fallback provider.
Use a source fixture containing old keys, claims, active leases, sessions, and unfinished work for the rehearsal.
Import pre-durability queue records under original identities while source producers and consumers are stopped.

Development credentials are referenced by `upstash-dev/api-url` under `VAULT_DEV`; release credentials use `upstash-prod/api-url` under `VAULT_PROD`.
These are references from deployment workflows, not evidence either environment is live.
Inventory actual deployments and consumers before planning their switch or deleting any vault item.

The safe abort point is before destination admission, with source data preserved and source writers still stopped.
After destination admission, recover forward through durable replay or a separately proven reverse migration; never just change the endpoint back.
No trial activation, billing change, deployment edit, or live switch follows from a documentation revision alone.

## Limits

Prototype outputs are currently missing. Readiness requires passing fault tests, frozen service/cost budgets, Cloud evidence, and the rehearsed operational procedure.
Capacity is reported as a measured ceiling with resources and workload; fleet population alone establishes no throughput guarantee.
Human approval and access remain explicit prerequisites to external actions.
PlanetScale Neki shards PostgreSQL; adopting it is a separate database change and does not satisfy Dragonfly requirements.

## Related pages

- [Review resolution](../v2/reviews/M192_REVIEW_RESOLUTION.md) maps every finding to its correction and proof.
- [Runtime data flow](./data_flow.md) describes the implementation being replaced.
- [Dragonfly pub/sub design](https://github.com/dragonflydb/dragonfly/blob/1e5f9944834b6ed999a2baf137e929e6de3e3009/docs/pub-sub.md) describes supported command families.
- [Dragonfly cluster design](https://github.com/dragonflydb/dragonfly/blob/1e5f9944834b6ed999a2baf137e929e6de3e3009/docs/cluster-mode.md) describes configuration, migration, and routing.
- [redis-rs cluster support](https://docs.rs/redis/latest/redis/cluster_async/index.html) describes RESP3 push and node routing; validate the pinned dependency in prototypes.
- [Dragonfly v1.40.2](https://github.com/dragonflydb/dragonfly/releases/tag/v1.40.2) is the initial server candidate; main-branch documentation does not prove release behavior.
