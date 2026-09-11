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
| What was reviewed? | Claude Fable 5.1 reviews through 8b415cc90; this revision addresses the eight adversarial findings and server-source additions, retaining Indy's cluster-only deployment override. |
| What changed? | Proposed design, prototype proofs, evidence grading, workload preparation, and readiness/live delivery boundaries. |
| What is proven? | Code references and documentation were inspected; no Dragonfly prototype, workload, Cloud test, or migration was run. |
| What still needs a decision? | Fixture-only outbound versus a new delivery feature; the proposed two-spec delivery split; service/cost budgets and live approval. |

## What it is

This record maps the original findings and subsequent re-reviews to documentation corrections and outstanding proofs.
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
| 1 | Standard PUBLISH and RESP2 hub: `afd_redis/src/streams/tail.rs:13`, `hub/pump.rs:62`. | New daemon uses routed RESP3 sharded pub/sub with the existing activity channel bytes; no temporary standalone mode. | Documented; cluster transport/recovery proofs NOT RUN. |
| 2 | Two-slot admission script: `afd_redis/src/streams/once.rs:161`, `streams.rs:79`. | Remove admission Lua across all producers; import all three source claim namespaces into PostgreSQL, preserving identity and expiry/no-expiry. Queue XADD is single-key. | Supersedes the tagged-claim design; migration and outbox proofs NOT RUN. |
| 3 | No production outbound enqueue: `afd_outbound/src/worker.rs:136`, `afd_fleet/src/lease/report.rs:20`. | Fixture-only performance remains proposed; inventory actual historical outbound entries before any cutover. | Source checked; no connector-delivery result claimed. |
| 4 | Fixed result files and absent seed/offered-rate controls: `afd_bench/src/report.rs:100`, `knobs.rs:15`. | Twelve unique raw samples and sidecars; B/B0 production build/dependency equality includes Cargo.lock. | Capture and grader NOT RUN. |
| 5 | Hand-written environment labels prove no origin. | Authenticated CI evidence plus pinned provider-backed GET identity verification; response secrets discarded before saving. | Mechanism verified in official source; authenticated probe NOT RUN. |
| 6 | Main/release auto-deploy to Upstash: `.github/workflows/deploy-dev.yml`, `deploy-dev-fly.yml:41`, `release.yml:520`. | Keep the candidate on its feature branch until Indy's coordinated switch; workflow preflight protects secret/image mutation and a durable import receipt protects admission. | Import/preflight/restart proofs NOT RUN; live switch and retirement need manual evidence. |
| 7 | Public ID/receipt coupling and receive discriminator: `afd_wire/src/event.rs:375`, `afd_fleet/src/lease/event.rs:104`. | Stable logical ID; accepted-to-received transition, receive charge and marker are atomic after existing pre-charge gates; first-attempt counters have their own durable guard. | Billing/state/crash proofs NOT RUN. |
| 8 | Prefix isolation and single-container reset: `afd_redis/tests/support/redis_harness.rs:52`, `make/test-infra.mk:218`. | Keep prefix isolation, DB0, owned-primary reset and replica checks; never flush Cloud. | Cluster reset proof NOT RUN. |
| 9 | Source standalone access is required for import. | Offline tool reads old Redis state; baseline and §2 use isolated Redis fixtures. The shipped new daemon supports clusters only. | Combined admission/Swarm cutover rehearsal NOT RUN. |
| 10 | M188 deferred address/fixture hardening. | §1 hardens both datastore addresses and discovered nodes before baseline/remote use. | Safety/cancellation proofs NOT RUN. |
| 11 | Dedicated blocking reader, trimming, and group recovery: `afd_redis/src/dedicated.rs`, `streams.rs:209`, `afd_outbound/src/worker.rs:129`. | Extend existing Dedicated ownership with cluster routing. Retention and durable group recovery still need redesign. | Correction: source already isolates outbound blocking sockets; remaining proofs NOT RUN. |
| 12 | Names and acceptance surfaces: `make/acceptance.mk:12,17,23`. | Keep acceptance-e2e, acceptance-execution, and cli-acceptance references; install manages groups rather than producing work. | Source checked; original CLI-only claim was incorrect. |

Rust paths are relative to `rustd/crates/`; workflow paths are repo-relative.

### Seven findings from the review of 7b233ba30

The table below records the b6b033e05 response to that review. Its temporary standalone design is superseded by Indy's later override below.
Billing, PostgreSQL authority, evidence, inventory, and baseline requirements remain; historical wording here is not a current deployment instruction.

| # / severity | Finding | Correction and mapped proof | Remaining validation |
|---|---|---|---|
| 1 / High | A cluster-only readiness merge freezes unrelated main deployments. | M192_001 §5/§7 keeps standalone default and explicit cluster with mode-specific pub/sub; Dimension 7.2 proves populated upgrade and a later unrelated standalone deployment. M192_002 Dimension 3.3 removes standalone only after all environments pass observation. | Both-mode deployability, safe initial durability upgrade, no automatic fallback, and exact retirement-build tests NOT RUN. |
| 2 / High | Admission-created rows turn all leases into Repeat, skipping receive debit. | M192_001 §2/Dimension 2.2 adopts the canonical row state machine and transactional receipt/debit/marker and separately guarded first-attempt counters; retain post-charge approval behavior. Update money gates, billing ownership, schema trigger, ingress comments, and public history together. | Duplicates, transient failure, permanent refusal, rollback, lost response, imported ledger state, and terminal settlement proofs NOT RUN. |
| 3 / Medium | PostgreSQL acceptance and Redis claims compete as deduplication authorities. | PostgreSQL is the sole authority for producer identity, expiry, acceptance, and ordered dispatch. Remove append_once/forget_once callers across ingress, cron, approval continuation, and repair cleanup. Dimension 5.2 grades outbox identity/order, not two-key Lua. | Provider ID is distinct from logical event PK; namespace sharing, expiry replacement, tombstones, no-expiry cleanup, and late replay proofs NOT RUN. |
| 4 / Medium | Cloud control-plane verification has no named mechanism. | Canonical workload evidence pins official provider source implementing authenticated GET `/v1/datastores/<DATASTORE_ID>` and identifies response fields. Dimension 6.3 rejects unavailable/mismatched/stale identity and leaked secrets; successful probe required before Cloud load. | API key permissions and actual account/datastore access remain unproven. No live request made. |
| 5 / Medium | New PostgreSQL commit load has no budget. | Canonical PostgreSQL budget table and §6/Dimension 6.4 require admission/commit rates, pool and allocator waits, WAL/I/O, backlog/drain, replica lag, resources/cost, and fan-out amplification. | Indy must freeze numeric thresholds for both stores; throughput and batching gains remain unproven. |
| 6 / Low | Migration inventory names concepts rather than keys. | Canonical Source-state inventory names all three claims, stream/groups, ready, outbound, sessions, gate mirrors, connector nonces, anomaly windows, and durable counterparts. Dimension 7.1 rejects unknown keys/types and validates every disposition. | Real source census, all prefix proofs, remaining expiry/no-expiry, fencing, and reconciliation NOT RUN. |
| 7 / Low | Bench-only changes can alter shared production dependencies through Cargo.lock. | §1/Dimensions 1.1 and 1.3 compare source/schema/build inputs and resolved production dependency/feature closure at B/B0, allowing only proven bench-exclusive lockfile deltas. | Shared dependency/version/source/checksum/edge changes must fail; capture/equality proof NOT RUN. |

### Indy deployment override and review of b6b033e05

**Indy, verbatim:** "we just stick to local that runs containers today (with the cluster config, no single mode crap for dragonfly)".
**Indy, verbatim:** "in production this would be stood up by Indy on dragondb just like indy did for upstash and stick the key in deployment to deploy-dev.yml".
**Indy, verbatim:** "dont over engineer here keep things simple".

Interpretation: local multi-node cluster tests, one cluster-only new daemon, and Indy-created Swarm through the existing vault-to-Fly deployment path.
This supersedes temporary standalone deployability and its later code removal. It does not waive data preservation, billing correctness, or concrete live-action approval.
`deploy-dev.yml` calls `deploy-dev-fly.yml`; the latter loads the vault URL and stages REDIS_URL_API into Fly secrets. Production follows release.yml if inventoried.
Keep code on the readiness branch until the coordinated switch is prepared; there is no early cluster-only merge that freezes unrelated main deployments.

| # / severity | Latest finding | Correction and proof | Status |
|---|---|---|---|
| 1 / Medium | Auto-deploy can admit before fenced import. | Canonical cutover receipt binds deployment/source/destination/format/reconciliation; only operator/import-tool can complete it. §2.3 closes all admission/lease/dispatch until valid; §7.2 preflights before any Fly secret/image mutation. Old writers must actually be fenced. | Spec corrected; missing/wrong/failed receipt, empty-source initialization, restart, and source-fence proofs NOT RUN. |
| 2 / Medium | §5 requires §2 while §2 requires §5. | §2 depends only on §0.3 and §1 and proves durable admission on the existing isolated Redis fixture. §5 depends on §0, §1, and §2; all admission Lua is removed first. This adds no shipped provider mode. | Acyclic order documented; implementation NOT RUN. |
| 3 / Low | Unit/integration row claims a later live deployment. | §7.2 is explicitly a pure workflow/script dry-run asserting zero unsafe secret/image mutations; M192_002 manual rows own actual switch and observation. | Test tier corrected; no live deployment claimed. |
| 4 / Low | Tagged activity channel adds an unnecessary second shape. | Keep fleet:<id>:activity and existing afd_sse parser; route sharded subscriptions by the channel's own slot. | Channel design simplified; transport/tenant proofs NOT RUN. |
| 5 / Low | Vague billing marker invites another table. | Reuse billing.usage_ledger; zero-valued receive rows remain required. Balance deductions remain in renewal; no second billing marker or new receive drain. Scope ledger conflict targets by fleet for per-fleet logical IDs. | charge.rs and schema/710 inspected; atomic receipt/ledger, renewal, and two-fleet collision proofs NOT RUN. |
| 6 / Low | TEXT sequence ordering breaks at 9/10. | Numeric millisecond/sequence comparison in history ordering and cursor predicates; retain public ID bytes. Match indexed ordering and workspace fleet tie-break; test 8/9/10/11/100 and imported IDs across pages. | Expected fix and §0.3 proof explicit; tests NOT RUN. |
| 7 / Low | Dimension 3.3 precedes 3.2. | Live spec orders both lists 3.1 evidence grader, 3.2 source-retirement guard, 3.3 manual verification. | Ordering corrected. |

Additional prerequisites: cluster.enabled must be explicitly true; null/absent/false fails Cloud identity grading.
The large runner rig and paid Swarm require Indy-approved capacity/cost before §6; production existence remains unverified.
Root Dockerfile, docker-compose.yml, deploy-dev/called Fly workflow, release workflow, and the founding credential gate are named in implementation scope.
Local cluster work follows Indy's direction; this pass edits documentation only and performs no paid or live action.

### Adversarial review of 8b415cc90

All eight findings are addressed in the spec; closure below means a documented correction, not runtime acceptance.
Local source inspection used Dragonfly v1.40.2 at e94300e6990093ec093cfb00d60c2e77ea4907e4 and Terraform provider e25af703daf80c5f0c973845ce4a5191e9523c2e.

| # | Finding | Spec disposition and required proof |
|---|---|---|
| 1 | Live cross-fleet billing collision and nullable fleet ID | §2 changes receive, renew, and report conflict targets to an immutable billing_fleet_id plus event_id/charge_type. Keep nullable fleet FK and tenant cascade. Canonical audit selects collision/orphan candidates; unresolved historical attribution blocks cutover. §2.2 covers two fleets/tenants with equal IDs, deletion, retry, and ledger immutability. |
| 2 | Local cluster addressing/bootstrap | §0 uses one supervised multi-process compose service with loopback advertisements, matching host ports, daemon sharing its network namespace, stable node IDs, and config re-push on every start. Listening interfaces allow Docker forwarding; health checks slots/replicas and host/container reachability. §0.2 verifies restart/bootstrap. |
| 3 | Persistence posture | §0 specifies separate persistent directories and snapshot schedule. §6 requires replicas >=1 per primary, backup policy and approved recovery budgets; restore requires matching shard count, topology refresh, durable replay and auth-state reconciliation. §0.2/§6.2/§6.4 own tests. |
| 4 | Fly-to-Swarm access | §6 uses public verified TLS, vault auth and restricted datastore ACLs; probe every advertised/failover address from Fly. Vendor IP restrictions remain unverified. Static egress is conditional on an enforceable allowlist and cost approval; use Fly's current app-scoped per-region model. |
| 5 | Frame rate absent from load budget | §6 freezes frames/bytes per second, viewer fan-out, lag/closure rate and recovery delay separately from event admission. The 256-frame per-channel buffer makes slow-viewer/token-load tests required. |
| 6 | Standalone client needed by tools | §5/§7 and live retirement explicitly retain afd_redis::client::Redis for import tooling and isolated fixtures; only daemon composition drops standalone construction. |
| 7 | Webhooks during fence | §7 and live §2 stop old Fly Machines and restart/deploy paths, inventory provider delivery IDs during the fence, then explicitly redeliver/reconcile after switch or abort. Verify provider access and deadlines before stopping; never claim durability for unaccepted requests. |
| 8 | Grader simplification and conditional partitioning | Keep every grader mode and archive canonical lane logs/exit/counts/revision. Only extra partition-map assertions in §4.1/§4.3 can be N/A with measured budget proof; base race/recovery tests remain mandatory. §7.2 is a unit-tested shell preflight invoked by the workflow, with command stubs. |

The NULLS NOT DISTINCT suggestion is deliberately replaced: schema/710_usage_ledger.sql uses ON DELETE SET NULL, so two deleted fleets with equal IDs would collide again under a null-equal key.
One immutable billing fleet column preserves identity without another ledger/marker table. Historical rows whose source fleet is already erased need evidence or explicit reconciliation; no automatic split of accumulated charges is defensible.
Indy's decision to fold the billing correction into §2 is recorded in Fable's supplied handoff; this implements that scope without claiming a separate direct quote.

Server-source additions are part of Dimension 0.1: wrong-node SSUBSCRIBE/SPUBLISH must return MOVED; a server-initiated unsubscribe after slot migration must be observed and rebuilt on the new owner.
The server behavior is source-verified; redis-rs 1.6.0 push handling remains NOT RUN.

| Pinned source | What the read establishes |
|---|---|
| [Ownership and pub/sub](https://github.com/dragonflydb/dragonfly/blob/e94300e6990093ec093cfb00d60c2e77ea4907e4/src/server/main_service.cc#L1234) | Sharded commands are slot-checked; MOVED uses configured ip/port (line 1308); standard pub/sub is refused in cluster mode (lines 2650–2695). |
| [Cluster configuration](https://github.com/dragonflydb/dragonfly/blob/e94300e6990093ec093cfb00d60c2e77ea4907e4/src/server/cluster/cluster_family.cc#L42) | Stable cluster_node_id and ADMIN-flagged DFLYCLUSTER/DFLYMIGRATE; main-port administration. |
| [Subscription removal](https://github.com/dragonflydb/dragonfly/blob/e94300e6990093ec093cfb00d60c2e77ea4907e4/src/server/channel_store.cc#L182) | Slot migration removes subscriptions and notifies their connections. |
| [Snapshot stream encoding](https://github.com/dragonflydb/dragonfly/blob/e94300e6990093ec093cfb00d60c2e77ea4907e4/src/server/rdb_save.cc#L629) | Groups, PEL and consumers are serialized; this does not prove the configured fixture restarts correctly. |
| [Persistence flags](https://github.com/dragonflydb/dragonfly/blob/e94300e6990093ec093cfb00d60c2e77ea4907e4/src/server/server_family.cc#L111) | dir and snapshot_cron exist at the release. |
| [Cloud configuration fields](https://github.com/dragonflydb/terraform-provider-dfcloud/blob/e25af703daf80c5f0c973845ce4a5191e9523c2e/internal/sdk/datastore.go#L85) | Network, TLS, ACL, replicas and backup fields exist; there is no named source-IP allowlist field here. |

The [Cloud connectivity and persistence requirements](../../architecture/datastore_scaling.md#cloud-connectivity-and-persistence) cite current vendor documentation, including Fly's app-scoped egress and Swarm restore constraints.
Public TLS/auth is the proposed simple deployment path addressing Indy's concern; no private network or vendor IP restriction is claimed to exist or have passed a live probe.
Indy's earlier verbatim local-cluster/manual-provisioning overrides remain in both specs; these corrections do not reinstate a standalone Dragonfly mode or a provisioning framework.

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
| Operational rehearsal | All inventoried prefixes, no-expiry claims, consumed nonces, queue-only work, leases/payment records, and duplicate writers. | Combined import/Swarm cutover reconciles; missing receipt closes admission and workflow mutations; unsafe abort and unknown keys block. |
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

Leave the existing deployed Redis build in service while proving the new build on the local cluster; coordinate the first merge/deploy with Indy.
The readiness/live delivery split and fixture-only outbound remain proposals; this correction does not add an outbound product feature.
Indy's override removes temporary standalone transport; no automatic fallback or early-merge global deployment hold is planned.

The first implementation slice is §1: benchmark address/fixture hardening, twelve-sample capture, sidecars/digests, baseline grader, and B/B0 dependency proof.
It needs no CI or compose change. §0 requires local cluster setup under Indy's stated scope first; this documentation pass makes no infrastructure edit.
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
