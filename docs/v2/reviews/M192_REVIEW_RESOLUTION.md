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
| What was reviewed? | Claude Fable 5.1 reviews of d165f5f10 and the correction range d165f5f10..7b233ba30. |
| What changed? | Proposed design, prototype proofs, evidence grading, workload preparation, and readiness/live delivery boundaries. |
| What is proven? | Code references and documentation were inspected; no Dragonfly prototype, workload, Cloud test, or migration was run. |
| What still needs a decision? | Fixture-only outbound versus a new delivery feature; the proposed two-spec delivery split; service/cost budgets and live approval. |

## What it is

This record maps the twelve original findings and seven re-review findings to documentation corrections and outstanding proofs.
The specifications carry executable acceptance requirements; this record does not award runtime acceptance.
[M192_001](../pending/M192_001_P0_API_INFRA_OBS_DRAGONFLY_SCALE_REDIS_PARITY.md) covers readiness; proposed [M192_002](../pending/M192_002_P0_INFRA_OBS_DRAGONFLY_CUTOVER_REDIS_RETIREMENT.md) covers live retirement.

## Why it exists

Indy approved redesigning the live tail around Dragonfly's supported approach and asked for prototypes that expose migration risks.
That approval permits substantial refactoring; it does not establish the benefit of every proposed partitioned queue or readiness layout.
Swarm handles server sharding; application partitioning must address a measured bottleneck or failed budget.

## How it behaves

### Original finding dispositions after re-review

Source citations below were checked at `7b233ba30`; they describe the implementation, not completed migration work.
A documented correction remains untested until its mapped proof runs.

| # | Finding and source | Current requirement | Evidence status |
|---|---|---|---|
| 1 | Standard PUBLISH and RESP2 hub: `afd_redis/src/streams/tail.rs:13`, `hub/pump.rs:62`. | Cluster mode uses routed RESP3 sharded pub/sub; explicit standalone mode retains ordinary pub/sub until retirement. | Documented; mode-specific transport/recovery proofs NOT RUN. |
| 2 | Two-slot admission script: `afd_redis/src/streams/once.rs:161`, `streams.rs:79`. | Remove admission Lua across all producers; import all three source claim namespaces into PostgreSQL, preserving identity and expiry/no-expiry. Queue XADD is single-key. | Supersedes the tagged-claim design; migration and outbox proofs NOT RUN. |
| 3 | No production outbound enqueue: `afd_outbound/src/worker.rs:136`, `afd_fleet/src/lease/report.rs:20`. | Fixture-only performance remains proposed; inventory actual historical outbound entries before any cutover. | Source checked; no connector-delivery result claimed. |
| 4 | Fixed result files and absent seed/offered-rate controls: `afd_bench/src/report.rs:100`, `knobs.rs:15`. | Twelve unique raw samples and sidecars; B/B0 production build/dependency equality includes Cargo.lock. | Capture and grader NOT RUN. |
| 5 | Hand-written environment labels prove no origin. | Authenticated CI evidence plus pinned provider-backed GET identity verification; response secrets discarded before saving. | Mechanism verified in official source; authenticated probe NOT RUN. |
| 6 | Main/release auto-deploy to Upstash: `.github/workflows/deploy-dev.yml`, `deploy-dev-fly.yml:41`, `release.yml:520`. | Main stays deployable on explicit standalone transport; retire that path only after all environments switch. No indefinite global hold. | Upgrade, later-build deployment, and retirement proofs NOT RUN. |
| 7 | Public ID/receipt coupling and receive discriminator: `afd_wire/src/event.rs:375`, `afd_fleet/src/lease/event.rs:104`. | Stable logical ID; accepted-to-received transition, receive charge and marker are atomic after existing pre-charge gates; first-attempt counters have their own durable guard. | Billing/state/crash proofs NOT RUN. |
| 8 | Prefix isolation and single-container reset: `afd_redis/tests/support/redis_harness.rs:52`, `make/test-infra.mk:218`. | Keep prefix isolation, DB0, owned-primary reset and replica checks; never flush Cloud. | Cluster reset proof NOT RUN. |
| 9 | Source standalone access is required for import. | Offline tool reads old state; readiness daemon also retains selected standalone transport. Retirement removes daemon standalone support. | Both upgrade and live-rehearsal proofs NOT RUN. |
| 10 | M188 deferred address/fixture hardening. | §1 hardens both datastore addresses and discovered nodes before baseline/remote use. | Safety/cancellation proofs NOT RUN. |
| 11 | Dedicated blocking reader, trimming, and group recovery: `afd_redis/src/dedicated.rs`, `streams.rs:209`, `afd_outbound/src/worker.rs:129`. | Extend existing Dedicated ownership with cluster routing. Retention and durable group recovery still need redesign. | Correction: source already isolates outbound blocking sockets; remaining proofs NOT RUN. |
| 12 | Names and acceptance surfaces: `make/acceptance.mk:12,17,23`. | Keep acceptance-e2e, acceptance-execution, and cli-acceptance references; install manages groups rather than producing work. | Source checked; original CLI-only claim was incorrect. |

Rust paths are relative to `rustd/crates/`; workflow paths are repo-relative.

### Seven findings from the review of 7b233ba30

The user requested these corrections before another Claude review. This revision incorporates the two recommended design changes into the review draft.
The request does not authorize infrastructure changes, live switching, or source deletion.

| # / severity | Finding | Correction and mapped proof | Remaining validation |
|---|---|---|---|
| 1 / High | A cluster-only readiness merge freezes unrelated main deployments. | M192_001 §5/§7 keeps standalone default and explicit cluster with mode-specific pub/sub; Dimension 7.2 proves populated upgrade and a later unrelated standalone deployment. M192_002 Dimension 3.3 removes standalone only after all environments pass observation. | Both-mode deployability, safe initial durability upgrade, no automatic fallback, and exact retirement-build tests NOT RUN. |
| 2 / High | Admission-created rows turn all leases into Repeat, skipping receive debit. | M192_001 §2/Dimension 2.2 adopts the canonical row state machine and transactional receipt/debit/marker and separately guarded first-attempt counters; retain post-charge approval behavior. Update money gates, billing ownership, schema trigger, ingress comments, and public history together. | Duplicates, transient failure, permanent refusal, rollback, lost response, imported ledger state, and terminal settlement proofs NOT RUN. |
| 3 / Medium | PostgreSQL acceptance and Redis claims compete as deduplication authorities. | PostgreSQL is the sole authority for producer identity, expiry, acceptance, and ordered dispatch. Remove append_once/forget_once callers across ingress, cron, approval continuation, and repair cleanup. Dimension 5.2 grades outbox identity/order, not two-key Lua. | Provider ID is distinct from logical event PK; namespace sharing, expiry replacement, tombstones, no-expiry cleanup, and late replay proofs NOT RUN. |
| 4 / Medium | Cloud control-plane verification has no named mechanism. | Canonical workload evidence pins official provider source implementing authenticated GET `/v1/datastores/<DATASTORE_ID>` and identifies response fields. Dimension 6.3 rejects unavailable/mismatched/stale identity and leaked secrets; successful probe required before Cloud load. | API key permissions and actual account/datastore access remain unproven. No live request made. |
| 5 / Medium | New PostgreSQL commit load has no budget. | Canonical PostgreSQL budget table and §6/Dimension 6.4 require admission/commit rates, pool and allocator waits, WAL/I/O, backlog/drain, replica lag, resources/cost, and fan-out amplification. | Indy must freeze numeric thresholds for both stores; throughput and batching gains remain unproven. |
| 6 / Low | Migration inventory names concepts rather than keys. | Canonical Source-state inventory names all three claims, stream/groups, ready, outbound, sessions, gate mirrors, connector nonces, anomaly windows, and durable counterparts. Dimension 7.1 rejects unknown keys/types and validates every disposition. | Real source census, all prefix proofs, remaining expiry/no-expiry, fencing, and reconciliation NOT RUN. |
| 7 / Low | Bench-only changes can alter shared production dependencies through Cargo.lock. | §1/Dimensions 1.1 and 1.3 compare source/schema/build inputs and resolved production dependency/feature closure at B/B0, allowing only proven bench-exclusive lockfile deltas. | Shared dependency/version/source/checksum/edge changes must fail; capture/equality proof NOT RUN. |

### Prototype admission and completion

Each risk proof records a failing control, the mitigation, observed results, immutable raw files, and the candidate revision/configuration.
A passing documentation check does not clear a risk.
The prototype grader requires historical capture and the complete risk matrix; dependent integration refuses missing or failed evidence.

| Prototype | Failure injection and measurable result | Promotion condition |
|---|---|---|
| Sharded live tail | Wrong node, restart, slot movement, unsubscribe races, slow viewers, and bounded memory/socket counts. | Frames resume within budget; no tenant misrouting, permanent-error reconnect loop, or unbounded buffers. |
| Outbox and retained scripts | CROSSSLOT test-only control, single-key Lua flush, lost XADD replies, moved slots, and cancelled BLOCK. | Physical duplicates preserve logical identity/order; Dedicated resources return to baseline. |
| Durable identity and replay | Stop around admission/receipt/charge commits and append; destroy queue; replay late; race expiry and gate refusal. | No lost acceptance or missing eligible debit; no double charge/run/count; pre-charge refusals remain unpaid and post-charge approval policy survives. |
| Admission and coordination | PostgreSQL pool/allocator pressure, hot fleets, App fan-out, stalled destinations, owner movement, and population growth. | Frozen budgets for both stores, fairness, and recovery pass; further batching/partitioning requires measured proof. |
| Operational rehearsal | All inventoried prefixes, no-expiry claims, consumed nonces, queue-only work, leases/payment records, and duplicate writers. | Populated standalone upgrade and Swarm cutover reconcile; later main deploys still work; unsafe abort and unknown keys block. |
| Evidence integrity | Change raw bytes, topology, claimed Cloud environment, run IDs, or live/rehearsal labels. | Grader rejects each mutation; authenticated origin and manual approvals remain independent requirements. |

### Identity choice and its limits

The canonical [durable identity design](../../architecture/datastore_scaling.md#durable-event-identity) owns the state machine and deduplication details.
A unique generated event ID cannot deduplicate provider retries; the producer identity and its original namespace/window require a separate unique lookup.
Single authority means PostgreSQL transactions decide acceptance and receipt; queue writes remain recoverable publication with potentially duplicate physical receipts.

The received transition and its debit must commit together after existing pre-charge gates; simply changing the row early would replace a skipped-charge bug with a crash window.
The new accepted state changes insert-triggered counters and history visibility; first-attempt observation must remain independent of billing eligibility.
Source `afd_fleet/src/lease/pull.rs:170,244` observes receipt before money gates and checks approval afterwards; the correction must preserve that policy.

### Evidence trust boundary

Hashes detect altered bytes; authenticated CI provenance identifies the collector run.
The canonical [workload evidence](../../architecture/datastore_scaling.md#workload-and-evidence) names the Cloud GET mechanism and secret-redaction boundary.
Official source proves the request shape exists, not that an available key can access the intended datastore or that the account has passed the probe.

### Scope and first executable slice

Preserve the configured Redis deployment while proving Swarm; validate each increment on the same deployment before explicit cutover.
The readiness/live delivery split and fixture-only outbound remain proposals; this correction does not add an outbound product feature.
Temporary standalone transport is now part of the readiness design; no main-wide deployment hold or automatic fallback is planned.

The first implementation slice is §1: benchmark address/fixture hardening, twelve-sample capture, sidecars/digests, baseline grader, and B/B0 dependency proof.
It needs no CI or compose change. §0 requires the approved local cluster setup first; this documentation pass makes no infrastructure edit.
Both specs remain PENDING. Runtime implementation still requires CHORE(open), resolved next-Section choices, and the repository lifecycle.

## Limits

No migration-confidence or capacity verdict is claimed. Every runtime/prototype proof above remains outstanding.
The next implementation work is benchmark safety/capture plumbing and the unchanged-application Redis baseline, followed by isolated prototypes before production integration.
A failed prototype requires a corrected design and another run before its dependent integration proceeds.

## Related pages

- [Target design](../../architecture/datastore_scaling.md) contains the mechanisms and resource boundaries.
- [Dragonfly pub/sub](https://github.com/dragonflydb/dragonfly/blob/1e5f9944834b6ed999a2baf137e929e6de3e3009/docs/pub-sub.md) documents the required command family.
- [Dragonfly cluster design](https://github.com/dragonflydb/dragonfly/blob/1e5f9944834b6ed999a2baf137e929e6de3e3009/docs/cluster-mode.md) documents slots and migration configuration.
- [PlanetScale Neki](https://planetscale.com/docs/postgres/sharding) describes the separate PostgreSQL sharding layer.
