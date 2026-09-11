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

This page defines the target design. The existing Redis deployment stays in service until the coordinated cutover; the new daemon targets Dragonfly clusters only.
No prototype, Cloud test, or migration has run as part of this documentation revision.

The [roadmap](./roadmap.md#dragonfly-migration-and-redis-retirement) links the readiness plan and proposed live cutover follow-up.
Readiness covers prototypes, implementation, Cloud proof, and rehearsal; the follow-up owns live retirement.
Capture the unchanged Redis baseline first; prove implementation increments on the same owned local Dragonfly cluster before the explicit deployment switch.
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

Keep the existing event-stream key `S` from `fleet_stream_key(fleet_id)`.
Keep `fleet:<id>:activity` as the channel name and route sharded pub/sub by that channel's own slot.
The channel need not share the stream slot; preserve existing builders and `afd_sse` parsing, and test workspace fan-in and tenant isolation.

PostgreSQL owns admission deduplication. Remove ingress claim-plus-append Lua and migrate every `append_once` caller, including cron, approval continuation, and repair verification.
The queue publisher uses single-key `XADD`; no target Redis admission claim keys are created or rebuilt.
Single-key session consumption and readiness-token scripts remain; test script-cache loss, redirects, and lost replies using their actual invariants.
A test-only two-slot script is a negative control for cluster routing, not a reason to retain a production multi-key admission script.

### Durable event identity

Commit the acceptance row, producer deduplication identity, per-fleet logical ID, and pending dispatch state in one PostgreSQL transaction before success or runnable work.
Keep public `event_id` in numeric stream-ID form; preserve imported IDs verbatim, without introducing zero-padded public IDs.
Compare ID milliseconds and sequence numerically in dispatch, history ORDER BY, and cursor predicates; never compare decimal components as TEXT.
Use numeric columns or matching indexed expressions so pagination does not add an unbounded sort.
Test equal created_at with sequences 8, 9, 10, 11, and 100, imported unpadded IDs, and page boundaries on fleet/thread/workspace history.
Workspace ordering needs fleet_id as the final tie-break because logical IDs are per fleet; keep cursor order and seek predicates identical.
Preserve old cursor decoding; explicitly reject an ambiguous old workspace cursor with a restart instruction rather than silently skipping a tied fleet.
Serialize allocation per fleet and initialize its high watermark above all imported identities, including claim-only records.

The `(fleet_id, event_id)` primary key alone cannot deduplicate a provider retry that has not received a logical ID.
A unique producer identity uses the existing claim namespace and exact original identifier bytes, bound to the fleet and logical ID.
Preserve the shared webhook/App namespace and original retention windows; do not split identities merely because callers use different surface names.
Store absolute expiry, with null meaning no time expiry, and the outstanding-work obligation in PostgreSQL.
Expired claims can admit a new event only after the prior obligation ends; serialize expiry/replacement against concurrent retries.
Import source queue payloads before reconciling claims. A claim with proven settled work can remain a deduplication tombstone without a retained payload.
An unresolved obligation with neither queue payload nor durable payload blocks upgrade; never fabricate work, drop acceptance, or reset expiry.

A bounded sweeper dispatches committed rows in numeric logical order under per-fleet fencing.
Put the logical ID in the queue envelope and let `XADD` generate the physical receipt; record dispatch progress only after confirmed publication.
Lost append replies may create duplicate physical receipts. Lease admission selects the earliest unfinished logical event and ignores stale or settled receipts.
Reconcile queue loss against unfinished durable rows even when previously marked published; recreate readiness independently.
No transaction spans PostgreSQL and the queue; crashes around either write must remain recoverable.

The durable row transitions as follows; status constants and schema changes follow repository conventions.

| Transition | Guard and atomic effects | Retry outcome |
|---|---|---|
| Absent → accepted | Commit payload, logical ID, producer identity, and dispatch state together. No receive debit or first-receipt counter increment. | Return the existing identity on duplicate admission. |
| Accepted → received | After existing pre-charge gates pass, lock/fence the row; change state and commit the existing receive-charge row in billing.usage_ledger together. | Only the transaction that changes state is `Delivery::First`; rollback leaves accepted and unpaid. |
| Accepted → gate_blocked | A permanent pre-charge refusal records its reason without a receive debit; first-attempt event accounting still applies. | Follow the existing refusal/approval continuation rules; never re-charge the original identity. |
| Received → gate_blocked | Preserve the existing post-charge approval/refusal behavior and already-paid marker. | No second receive charge or implicit refund. |
| Received → terminal | Existing terminal outcomes, settlement, and stage-ledger effects commit under lease fencing. | Duplicate physical receipts cannot repeat terminal execution or charging. |

Transient pre-charge faults leave accepted work retryable; received work can be reclaimed without another receive debit.
Do not infer first delivery from INSERT conflict, or commit the received transition before its charge row.
`billing.usage_ledger` is the billing idempotency record; add no second billing marker table.
At the inspected revision, `afd_billing/src/charge.rs` records a zero-value receive row; balance deductions happen in the fenced renewal path.
Preserve that policy: commit the received transition with its receive ledger row even when the amount is zero; do not invent a receive balance drain.
The current ledger conflict key is `(event_id, charge_type)` in `schema/710_usage_ledger.sql`.
Per-fleet logical IDs can collide across fleets, so scope ledger uniqueness/conflict targets to `(fleet_id, event_id, charge_type)` across receive, renewal, and report.
Prove two fleets with the same logical ID retain separate rows and charges; preserve existing ledger IDs and reconcile historical collisions during import.
Preserve zero debit for pre-charge refusal and exactly one policy-appropriate receive debit after those gates pass.
`lease/pull.rs` runs the approval check after money gates; preserve that ordering and its existing charge policy, including zero-cost postures.
Update `record_received`, money gates, billing transaction ownership, ingress comments, and insert-triggered counters together.
First receipt observation is separate from charge eligibility: a durable once-only observation marker guards the event counter and received-frame intent before gates.
Both pre-charge refusals and transient retries retain that accounting; subsequent receipt/terminal transitions must not count the event again.
Save the observation marker and counter in one transaction; it never authorizes or suppresses billing.
Accepted rows without receipt observation remain hidden from received-history views; observed rows preserve existing public status and streaming behavior.
Import historical received/terminal rows and ledger evidence without charging them again; ambiguous historical payment state blocks upgrade for reconciliation.

Prototype clock rollback, concurrent admission, allocator contention, imported high IDs, expiry races, lost replies, and older replay behind newer receipts.
Include approval continuation, repair cleanup, tenant scoping, history pagination, and partial App fan-out retries.
Prove §2 against the existing isolated Redis fixture before §5 integrates the cluster transport; this test sequence adds no shipped provider-selection mode.

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

Extend the existing non-cloneable `afd_redis::Dedicated` ownership model with slot-owner routing for cluster mode.
The inspected outbound reader already owns a dedicated socket; preserve that protection when adding slot-owner routing.
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
Verify production source, schema, build configuration, and workspace manifests at B match comparison revision B0 except proven bench-only changes.
Include `rustd/Cargo.lock`: each changed package/version/source/checksum/dependency edge must be outside the production build dependency closure at both revisions.
Compare resolved production features and dependencies for every shipped binary/target, including build dependencies; shared-package changes fail baseline grading.
Record the graph comparison and lockfile diff with the baseline. A bench-only path does not prove a bench-only dependency change.
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
The named Cloud identity mechanism is an authenticated GET to `https://api.dragonflydb.cloud/v1/datastores/<DATASTORE_ID>`.
The operator supplies DATASTORE_ID from the approved Cloud inventory; use a vault-held API key as a Bearer token, never in logs.
Dragonfly's official Terraform provider implements this read in `internal/sdk/client.go` at commit `e25af703daf80c5f0c973845ce4a5191e9523c2e` (v0.0.27).
Its `internal/sdk/datastore.go` defines `datastore_id`, `addr`, `status`, and `config.cluster.enabled` plus location, tier, TLS, and cache-mode fields.
This is a source-verified mechanism, not a successful authenticated probe or a claim of a separate Terraform data source.

Before any §6 Cloud workload, run a read-only capability probe for the approved account and datastore.
Match returned identity, active status, address, TLS, cluster mode, and resources to the endpoint and in-band topology.
Require `config.cluster.enabled` to be explicitly true; null, absent, false, or malformed values fail.
Record allowlisted fields, collection time, collector/source revision, and CI run identity before and after each run.
The response includes a password: discard it, ACL secrets, and request headers in memory before any saved output; never archive full responses or Terraform state.
Refuse redirects outside the named host, authentication errors, missing fields, stale evidence, wrong IDs, and endpoint/topology mismatches.
An unavailable mechanism blocks Cloud grading until repaired or replaced by a separately reviewed mechanism; no self-declared identity fallback.

### PostgreSQL admission budgets

Freeze PostgreSQL and Dragonfly budgets together before grading combined load; approval belongs to Indy.
Owned infrastructure for 1,000 runner processes and a paid Swarm datastore require explicit capacity/cost approval before §6; a local pass grants neither.
Count offered provider requests, expanded per-fleet admissions, accepted rows, and commits separately.
The 16,667 events/second target means per-fleet admissions; App fan-out amplification must also be reported.
Without measured batching, admission alone requires one commit per event, before receive, terminal, dispatch-progress, or allocator work.
Do not infer sustainable database throughput from the Dragonfly result or assume a batching benefit.

| Required PostgreSQL budget | Evidence and refusal |
|---|---|
| Sustainable admission/completion rate | Three samples at frozen rates and payload/skew cases; no growing backlog hidden by accepted responses. |
| Admission p95/p99 and pool wait | Measure transaction/commit latency, pool occupancy/timeouts, and allocator lock wait under hot-fleet and broad fan-out load. |
| Database resources and cost | Pin CPU, memory, I/O, storage, WAL bytes/second, replica lag, connections, and cost ceiling alongside queue resources. |
| Backlog and recovery | Bound accepted-unpublished rows/bytes/age, dispatch lag, replay rate, retry volume, and drain time after faults. |

Fail grading on a missing threshold, breached budget, changed resource envelope, or generator saturation.
If admission is the bottleneck, prototype bounded batching or another measured database improvement before accepting capacity.
Batching still commits before success/visibility and must preserve per-fleet ordering, deduplication, fairness, and crash recovery.
Database sharding is a separate decision; no Neki adoption follows from a failed budget.

### Outbound and delivery boundaries

The pinned Rust report path has no production caller of `OutboundQueue::enqueue`.
For this review draft, preserve that behavior: prototype and measure the existing outbound worker using synthetic fixtures only.
Adding report-to-delivery behavior requires an explicit scope decision and public documentation; the migration must not silently introduce it.

Before cutover, inventory the source outbound queue rather than assuming it is empty.
If historical jobs exist, the migration rehearsal must prove their disposition; unsupported jobs block cutover for a decision.
Inbound producers include steer, per-fleet webhook, App fan-out, cron, continuation, and repair verification; install creates groups rather than work.

### Delivery and cutover boundary

Indy selects local container testing and manually created Cloud Swarm. The new daemon has one cluster transport and no REDIS_MODE/provider fallback.
Keep `REDIS_URL_API`; the value identifies the selected cluster endpoint with authentication and TLS settings.
Local `docker-compose.yml` starts PostgreSQL and a real multi-node Dragonfly cluster with deterministic slot configuration and health checks.
Keep the existing Redis fixture only for the frozen baseline, §2 inner-loop proof, and source-import rehearsal; it is not a Dragonfly deployment mode.
The root Dockerfile packages agentsfleetd; database containers stay separate. Change that file only where the local image proof requires it.

Indy creates the intended Swarm datastore, confirms its topology/capacity, and supplies its credential through the existing vault flow.
`deploy-dev.yml` is the job graph; `deploy-dev-fly.yml` loads the URL and stages `REDIS_URL_API` in Fly secrets before deployment.
Use the same pattern in `release.yml` for production if that environment exists; inventory it rather than assuming it exists.
Update those references and the founding credential gate together; never embed keys in YAML, images, evidence, or source.
No Terraform provisioning framework or paid resource creation by the agent is required by this plan.

Keep the implementation branch unmerged until Indy supplies the endpoint and approves the tested cutover procedure.
This leaves unrelated main deployments on the existing build path while local work proceeds; no global auto-deploy hold is introduced.
A readiness review can finish before live work, but the first cluster-only merge/deploy is the coordinated switch tracked by the live plan.
The live plan consumes the verified candidate from the readiness branch; it does not require that candidate to be merged first.

Before staging Fly secrets or replacing machines, the deploy preflight verifies the approved destination, candidate revision, and completed import receipt.
Missing inputs stop that deployment before mutation and leave the existing deployed image/configuration intact.
The playbook fences all old producers/consumers, imports old work and claims, reconciles billing/auth state, then authorizes the new deployment.
Apply admission and ledger schema changes with old processes fenced; rehearse the old-build schema restore required by a pre-admission abort.
Changing the ledger conflict key is not automatically backward-compatible; do not leave an old uniqueness constraint that rejects valid cross-fleet IDs.
Source writers stay fenced after import. A marker alone cannot stop the old binary or make a rolling overlap safe.

The import tool records one durable cutover receipt in PostgreSQL after successful reconciliation and Indy's confirmation of the source fence.
Bind it to the deployment, source/destination identities, admission format, and reconciliation digest; the deploy preflight separately binds the tested build revision.
Only the operator/import-tool role can complete the receipt; agentsfleetd reads it but cannot create or self-approve it.
An empty datastore also requires an explicit empty-source initialization through the same tool; schema migration success is not import completion.
Absent, incomplete, mismatched, or unreadable receipt keeps readiness false and every ingress/lease/background dispatch path closed, with retryable refusal.
Open neither success responses nor runnable work until the receipt and cluster capability checks pass.
Recheck the receipt at each startup and require a new one if destination or admission format changes; unrelated builds do not need a new import.

The one playbook covers local Docker image proof, source census, import/fencing, Indy-supplied secret references, Fly preflight, switch, and observation.
Rehearse these steps locally; automated workflow tests prove preflight decisions, not that a real deployment occurred.
Live deployment, observation, and retirement remain manual evidence in the live plan.
Abort before destination admission with source preserved; after admission use forward recovery or a separately proven reverse migration.
No live secret mutation, deployment, paid trial, or resource deletion is performed by this documentation pass.

### Source-state inventory

Inventory every actual source key with type, count, expiry, authority, and disposition before the combined durable-admission and Swarm cutover.
Redis patterns below describe source bytes; angle-bracket fields come from the owning builder. Enumerate actual nonce prefixes from the connector registry.

| Source key or durable state | Owning source | Required disposition |
|---|---|---|
| `fleet:<fleet_id>:events` and group `fleet_lease` | `afd_redis/src/streams.rs` | Import queue-only accepted events with original logical IDs; reconcile pending entries and lease fences; rebuild physical receipts/groups from durable state. |
| `webhook:dedup:<once_id>` | `afd_redis/src/streams/once.rs` | Import exact namespace/identifier, original logical ID, and absolute remaining expiry into PostgreSQL; preserve shared webhook/App identity and different windows. |
| `schedule:fire:<once_id>` | `afd_redis/src/streams/once.rs` | Import per-fleet/schedule/message identity and expiry into PostgreSQL; retries must not reopen the fire. |
| `fleet:repair-verification:<once_id>` | `afd_redis/src/streams/once.rs` | Import repair/continuation identities with no time expiry; release only after the durable successor/cleanup condition is proven. |
| `fleet:ready` | `afd_redis/src/ready.rs` | Rebuild from durable unfinished and eligible state; stale tokens cannot suppress new work. |
| `connector:outbound` and group `connector_workers` | `afd_redis/src/outbound.rs` | Inventory entries and pending receipts; prove drain/import for any jobs or block. Fixture-only tests never justify dropping historical jobs. |
| `auth:session:<session_id>` | `afd_redis/src/session.rs` | Preserve remaining expiry, ownership, approval, abort, and consumed state atomically; no token resurrection. |
| `fleet:gate:byevent:<fleet_id>:<event_id>`; `fleet:gate:response:<action_id>` | `afd_gate/src/gate/store.rs` | Reconcile references and decisions against durable approvals; preserve expiry or rebuild only from a proven durable equivalent. |
| `connect:slack:nonce:*`, `connect:gh:nonce:*`, `connect:zoho:nonce:*`, `connect:jira:nonce:*`, `connect:linear:nonce:*` | `afd_connector/src/registry.rs`, `state/nonce.rs` | Copy unconsumed nonce markers with remaining expiry under fencing; consumed/missing markers must stay absent. |
| `fleet:anomaly:<fleet_id>:<tool>:<action>` | `afd_gate/src/gate/store.rs` | Preserve count and remaining window; resetting it could bypass an approval trigger. |
| `fleet:<fleet_id>:activity` | `afd_redis/src/streams.rs` | Ephemeral channel, not a stored key; keep channel bytes; reconnect viewers through sharded pub/sub and durable history. |
| `core.fleet_events`, `core.fleet_sessions`, `fleet.runner_leases`, `fleet.runner_affinity`, approval and billing rows | `docs/architecture/data_flow.md` | PostgreSQL stays authoritative; reconcile identities, terminal/payment state, lease fencing, approvals, counters, and allocator high watermarks. |
| Any unmatched source key/type | Full source scan and producer inventory | Refuse admission until its owner and preservation/removal proof are recorded; never silently skip. |

Source paths in the table are relative to `rustd/crates/` unless prefixed with `docs/`.
Sample expiry against source time, subtract migration elapsed time, preserve no-expiry separately, and never extend a retry/authentication window.
A scan against active writers is not a consistent snapshot; import under fencing and repeat reconciliation before enabling admission.
Keep secret-bearing source exports encrypted and access-controlled; evidence contains counts and digests, never payloads or session contents.

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
- [Cloud identity read implementation](https://github.com/dragonflydb/terraform-provider-dfcloud/blob/e25af703daf80c5f0c973845ce4a5191e9523c2e/internal/sdk/client.go) names the authenticated datastore read.
- [Cloud identity response](https://github.com/dragonflydb/terraform-provider-dfcloud/blob/e25af703daf80c5f0c973845ce4a5191e9523c2e/internal/sdk/datastore.go) names returned fields and sensitive data.
- [redis-rs cluster support](https://docs.rs/redis/1.6.0/redis/cluster_async/index.html) describes RESP3 push and node routing; validate the pinned dependency in prototypes.
- [Dragonfly v1.40.2](https://github.com/dragonflydb/dragonfly/releases/tag/v1.40.2) is the initial server candidate; main-branch documentation does not prove release behavior.
