---
type: explanation
audience: contributor
verified: 2026-09-11
product_version: 0.30.0
executable: false
---

# M192 review resolution and prototype risks

| Question | Answer |
|---|---|
| What was reviewed? | The supplied Claude Fable 5.1 review of documentation commit d165f5f10. |
| What changed? | Proposed design, prototype proofs, evidence grading, workload preparation, and readiness/live delivery boundaries. |
| What is proven? | Code references and documentation were inspected; no Dragonfly prototype, workload, Cloud test, or migration was run. |
| What still needs a decision? | Fixture-only outbound versus a new delivery feature; the proposed two-spec delivery split; service/cost budgets and live approval. |

## What it is

This record maps all twelve supplied findings to a correction or a reasoned alternative.
The specifications carry executable acceptance requirements; this record does not award runtime acceptance.
[M192_001](../pending/M192_001_P0_API_INFRA_OBS_DRAGONFLY_SCALE_REDIS_PARITY.md) covers readiness; proposed [M192_002](../pending/M192_002_P0_INFRA_OBS_DRAGONFLY_CUTOVER_REDIS_RETIREMENT.md) covers live retirement.

## Why it exists

Indy approved redesigning the live tail around Dragonfly's supported approach and asked for prototypes that expose migration risks.
That approval permits substantial refactoring; it does not establish the benefit of every proposed partitioned queue or readiness layout.
Swarm handles server sharding; application partitioning must address a measured bottleneck or failed budget.

## How it behaves

### Finding dispositions

| # | Finding and verified source | Spec correction | Required proof; status |
|---|---|---|---|
| 1 | Standard PUBLISH and RESP2 hub: `afd_redis/src/streams/tail.rs:13`, `hub.rs:1`, `hub/pump.rs:62`. Upstream blocks standard/pattern pub/sub in cluster mode. | §0.1 and §5 require SPUBLISH/SSUBSCRIBE/SUNSUBSCRIBE, RESP3 push, bounded owner-node subscriptions, capability refusal, and shared channel parsing. Retire the one-socket invariant only with implementation. | Cross-seed routing, ack-before-publish, movement, reconnect, lag, resource churn, and workspace isolation; NOT RUN. |
| 2 | Claim and stream keys: `afd_redis/src/streams/once.rs:161`, `streams.rs:79`. | §5 defines S as the existing stream key, literal {S} tags for related claims/activity, actual KEYSLOT checks, and old-claim TTL/identity migration. | CROSSSLOT negative control, tagged atomic retry, hostile provider IDs, and old-claim import; NOT RUN. |
| 3 | Production report omits delivery: `afd_fleet/src/lease/report.rs:20`; enqueue reference sweep finds no production caller. | §2 explicitly preserves current behavior in this draft. Outbound scheduling prototypes use synthetic fixtures; actual source jobs must be inventoried before cutover. Adding product delivery remains an explicit decision. | No false connector-delivery claim; unexpected nonempty source jobs block cutover; NOT RUN. |
| 4 | Fixed M188 filenames, absent seed/offered-rate controls, in-process runners: `afd_bench/src/report.rs:100`, `knobs.rs:15`, `lane/steer.rs:227`, `lane/lease.rs:137`. | §1 allows bench-only capture before B, verifies unchanged production trees against B0, archives twelve outputs with sidecars, and uses actual parameters/payload/window. Large open-loop load moves to §6. | Distinct samples, digest checks, B/B0 tree equality, and missing historical fields identified; NOT RUN. |
| 5 | Hand-written JSON can claim any environment. | §1 and §6 require raw files, recomputed SHA-256/statistics, authenticated CI/artifact references, server/topology checks, and verified Cloud datastore identity. §002 requires human live verification. | Altered bytes, forged labels, absent artifact origin, inconsistent control-plane identity, and rehearsal-as-live all fail; NOT RUN. |
| 6 | Cluster-only build conflicts with automatic Upstash deploy: `deploy-dev-fly.yml:41`, `release.yml:520`. | Propose readiness M192_001 and live M192_002. Require an approved, tested deployment hold before the first implementation merge; no parked-spec exemption assumed. | Safe-landing dry run and separate live approval/observation/retirement proof; NOT RUN. |
| 7 | Public event_id equals receipt: `afd_wire/src/event.rs:375`; first receive controls debit in `afd_fleet/src/lease/event.rs:43`. | Choose durable-before-visible admission, numeric logical IDs, and separate physical receipts. Preserve public shape. Reject append-first and unconditional explicit-ID replay as insufficiently safe. | Clock rollback, imported high IDs, lost replies, late replay, terminal duplicates, and billing races; NOT RUN. |
| 8 | Prefix isolation already exists: `afd_redis/tests/support/redis_harness.rs:52`; reset is one-container FLUSHALL in `make/test-infra.mk:218`. | §5 keeps prefix isolation and database 0, adds owned-cluster node reset/verification, and withdraws logical-database isolation as a Swarm option. | Every owned primary reset, replicas reconciled, scoped cleanup, shared-target refusal; NOT RUN. |
| 9 | Final daemon cannot read source standalone Redis. | §7 uses an explicit bounded offline migration tool; only baseline/rehearsal source fixtures open standalone Redis. No daemon fallback. | Source import, writer fencing, safe abort, post-admission recovery; NOT RUN. |
| 10 | M188 deferred address/fixture hardening in its Discovery section. | §1 pulls it into scope before capture or remote runs: validate both stores and discovered nodes, scope leases/consumer groups, recover cancelled fixtures. | Shared rig label, unexpected node, non-fixture claim/ack, and cancelled cleanup controls; NOT RUN. |
| 11 | Blocking reads, MAXLEN, and group repair can stall or lose work: `afd_redis/src/outbound.rs:352`, `streams.rs:18`. | §0.2, §3, §5 specify node-owned blocking connections, cancellation, pending-aware retention, and durable missing-group recovery. | Normal-command latency during BLOCK, socket churn, unacked pressure, group deletion, and no lost accepted work; NOT RUN. |
| 12 | Producer names, auth reference, migration config, playbook layout, and acceptance targets. | Add App fan-out and repair verifier; install is group setup. Read AUTH_DEVICE_LOGIN. Pin candidate release and image digest; use version-matched migrations[]. Use operation directory/001_playbook.md. | Exact-path and transport assertions; current `make/acceptance.mk:12,17,23` defines acceptance-e2e, acceptance-execution, and cli-acceptance, contrary to the review's CLI-only assertion. |

All Rust paths above are relative to `rustd/crates/`; workflow paths are relative to `.github/workflows/`.
Line references identify the inspected parent revision; implementation may move them and must refresh its evidence references.

### Prototype admission and completion

Each risk proof records a failing control, the mitigation, observed results, immutable raw files, and the candidate revision/configuration.
A passing documentation check does not clear a risk.
The prototype grader requires historical capture and the complete risk matrix; dependent integration refuses missing or failed evidence.

| Prototype | Failure injection and measurable result | Promotion condition |
|---|---|---|
| Sharded live tail | Wrong node, restart, slot movement, unsubscribe races, slow viewers, and bounded memory/socket counts. | Frames resume within budget; no tenant misrouting, permanent-error reconnect loop, or unbounded buffers. |
| Atomic operations and blocking readers | CROSSSLOT control, Lua flush, lost responses, moved slots, and cancelled BLOCK. | Single logical append; ordinary commands stay responsive; sockets/tasks return to baseline. |
| Durable identity and replay | Stop before/after DB commit and append; destroy queue; replay old IDs after newer receipts; retry after claim expiry. | Accepted work survives; identity/order and dedup windows hold; no duplicate terminal execution or debit. |
| Readiness/destination coordination | Skewed eligibility, hot fleets, stalled destinations, map/owner movement, and population growth. | Frozen fairness, throughput, resource, and recovery budgets pass; further partitioning is adopted only when justified. |
| Operational rehearsal | Old claims/TTL, session consumption, old queue-only work, active leases, unknown outbound jobs, and duplicate writers. | Every source state class reconciles; unsafe abort and unsupported jobs block the switch. |
| Evidence integrity | Change raw bytes, topology, claimed Cloud environment, run IDs, or live/rehearsal labels. | Grader rejects each mutation; authenticated origin and manual approvals remain independent requirements. |

### Identity choice and its limits

PostgreSQL owns the acceptance record and numeric logical ID before a runnable queue entry exists.
Physical stream IDs remain acknowledgment receipts; no new public field or UUID ID shape is required.
The prototype must still prove ID ordering, history pagination, imported high watermarks, and per-fleet allocator cost.

The review's explicit-ID replay suggestion is insufficient when an older event must be reinserted after a newer physical stream ID.
Appending before durable commit also exposes work to consumers before the durable record exists.
These cases are explicit failure tests rather than assumptions about retry safety.

### Evidence trust boundary

Hashes detect changed bytes; a party able to rewrite both a file and its hash can forge either.
Cloud evidence therefore binds to authenticated CI metadata and retained artifacts plus control-plane datastore identity, not self-declared JSON labels.
The live observation and retirement verdict still belongs to the named human verifier.

### Review proposals and approved direction

Approved: Dragonfly-supported live-tail redesign, substantial refactoring where required, and prototypes before trusting migration behavior.
Proposed for this re-review: preserve the absent outbound feature and split readiness from live retirement into linked specs.
Those proposals do not authorize paid infrastructure, workflow mutation, live switching, or resource deletion.

Neki is PlanetScale's PostgreSQL sharding system. It does not distribute Dragonfly keys or remove Swarm's client and pub/sub requirements.
The application should use Swarm's managed topology and prototype additional key/queue partitioning only where measurements justify it.

## Limits

No migration-confidence or capacity verdict is claimed. Every runtime/prototype proof above remains outstanding.
The next implementation work is benchmark safety/capture plumbing and the unchanged-application Redis baseline, followed by isolated prototypes before production integration.
A failed prototype requires a corrected design and another run before its dependent integration proceeds.

## Related pages

- [Target design](../../architecture/datastore_scaling.md) contains the mechanisms and resource boundaries.
- [Dragonfly pub/sub](https://github.com/dragonflydb/dragonfly/blob/1e5f9944834b6ed999a2baf137e929e6de3e3009/docs/pub-sub.md) documents the required command family.
- [Dragonfly cluster design](https://github.com/dragonflydb/dragonfly/blob/1e5f9944834b6ed999a2baf137e929e6de3e3009/docs/cluster-mode.md) documents slots and migration configuration.
- [PlanetScale Neki](https://planetscale.com/docs/postgres/sharding) describes the separate PostgreSQL sharding layer.
