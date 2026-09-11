---
type: explanation
audience: contributor
verified: 2026-09-12
product_version: 0.30.0
executable: false
---

# M192 review resolution and prototype risks

| Question | Answer |
|---|---|
| What was reviewed? | Claude Fable reviews through b01294265; current corrections remove auth import, preserve concurrent verify retries, bound subscription ownership and name PlanetScale evidence. Indy subsequently excludes reverse migrations and selects forward recovery after schema changes. Template governance is separately recorded below. |
| What changed? | Proposed design, prototype proofs, evidence grading, workload preparation, and readiness/live delivery boundaries. |
| What is proven? | Code references and documentation were inspected; §1's unchanged-application Redis baseline ran on the repository-owned local rig. No Dragonfly prototype, Cloud test, or migration was run. |
| What still needs a decision? | Service/cost budgets and infrastructure, actual PostgreSQL failover durability, any detected historical billing-damage disposition, and live approval. Fixture-only outbound and the readiness/live split remain the scoped implementation defaults; no auth-risk exception is planned. |

## What it is

This record maps the original findings and subsequent re-reviews to documentation corrections and outstanding proofs.
The specifications carry executable acceptance requirements; this record does not award runtime acceptance.
[M192_001](../active/M192_001_P0_API_INFRA_OBS_DRAGONFLY_SCALE_REDIS_PARITY.md) covers readiness; proposed [M192_002](../pending/M192_002_P0_INFRA_OBS_DRAGONFLY_CUTOVER_REDIS_RETIREMENT.md) covers live retirement.

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
| 4 | Fixed result files and absent seed/offered-rate controls: `afd_bench/src/report.rs:100`, `knobs.rs:15`. | Twelve unique raw samples and sidecars; B/B0 production build/dependency equality includes Cargo.lock. | Local rig capture and fail-closed grader pass; Dragonfly evidence remains NOT RUN. |
| 5 | Hand-written environment labels prove no origin. | Authenticated CI evidence plus pinned provider-backed GET identity verification; response secrets discarded before saving. | Mechanism verified in official source; authenticated probe NOT RUN. |
| 6 | Main/release auto-deploy to Upstash: `.github/workflows/deploy-dev.yml`, `deploy-dev-fly.yml:41`, `release.yml:520`. | Keep the candidate on its feature branch until Indy's coordinated switch; workflow preflight protects secret/image mutation and a durable import receipt protects admission. | Import/preflight/restart proofs NOT RUN; live switch and retirement need manual evidence. |
| 7 | Public ID/receipt coupling and receive discriminator: `afd_wire/src/event.rs:375`, `afd_fleet/src/lease/event.rs:104`. | Stable logical ID; accepted-to-received transition, receive charge and marker are atomic after existing pre-charge gates; first-attempt counters have their own durable guard. | Billing/state/crash proofs NOT RUN. |
| 8 | Prefix isolation and single-container reset: `afd_redis/tests/support/redis_harness.rs:52`, `make/test-infra.mk:218`. | Keep prefix isolation, DB0, owned-primary reset and replica checks; never flush Cloud. | Cluster reset proof NOT RUN. |
| 9 | Source standalone access is required for import. | Offline tool reads old Redis state; baseline and §2 use isolated Redis fixtures. The shipped new daemon supports clusters only. | Combined admission/Swarm cutover rehearsal NOT RUN. |
| 10 | M188 deferred address/fixture hardening. | §1 hardens both datastore addresses and discovered nodes before baseline/remote use. | Local rig ownership, topology, cancellation, and cleanup proofs pass; remote/cluster proofs remain NOT RUN. |
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
| 7 / Low | Bench-only changes can alter shared production dependencies through Cargo.lock. | §1/Dimensions 1.1 and 1.3 compare source/schema/build inputs and resolved production dependency/feature closure at B/B0, allowing only proven bench-exclusive lockfile deltas. | Local capture proves the production closure is equal; the grader fails changed shared dependency/version/source/checksum/edges. |

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

This records the eight corrections made at 8b452d2e0; the final review below supersedes its billing-backfill and client-recovery details. Closure means documented correction, not runtime acceptance.
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

### Final review of 8b452d2e0: adversarial assessment

Historical disposition: the later 6603eb7d8 follow-up below supersedes this section's auth mitigation/exception and RTT-attribution wording; other requirements remain.

The remaining findings do not prevent §1 baseline preparation. Hub recovery and billing rules must be corrected before their integration; live connectivity/authentication and measured budgets gate rollout.
No benchmark, prototype, Cloud probe or migration has run. Changes below are spec requirements, not claims that defects are fixed in runtime code.

| # | Importance / blocking boundary | Adversarial conclusion and spec change |
|---|---|---|
| 1 | Blocks hub integration (§0.1/§5) | Confirmed: redis-rs forwards SUnsubscribe but does not repair it on a healthy socket. The hub reconciles desired membership, reissues SSUBSCRIBE, waits for acknowledgment, coalesces retries, and handles delayed own acknowledgments/final-drop races without resurrecting subscriptions. |
| 2 | Blocks populated upgrade (§2/§7) | The previous blanket NOT NULL backfill was wrong for deleted fleets; source history can be gone. Keep unknown legacy billing_fleet_id null with NULLs-distinct uniqueness; all three real writers supply non-null identities. Preserve rows/amounts without guessing. Null alone does not block; detected billing damage needs an explicit disposition. |
| 3 | Required billing correctness (§2.2) | Replace table-level UPDATE with explicit revoke and grants on the six accumulator columns. Verify effective api_runtime permissions on populated upgrade; identity writes fail while real accumulation and FK deletion actions pass. Merely adding a narrow grant would leave the broad grant in force. |
| 4 | Blocks any partial wallet/ledger implementation (§2.2) | Remove the tenant WHERE suggestion. A skipped conflict arm could leave the independent wallet CTE draining. Tenant/fleet identity comes from the lease guard; tests verify ownership and whole-statement rollback on ledger failure. Fleet budget enforcement is included, not only reporting. |
| 5 | Restore procedure required; automatic-failover risk gates rollout (§6.2) | Adopt fenced, scoped multi-primary deletion of auth sessions, connector nonces and gate-response mirrors, followed by approval/anomaly reconciliation. Reject the assertion that this fixes automatic failover or that Indy has accepted replay. Keep single-use requirements; a demonstrated failure requires mitigation or an explicit scoped Indy exception before rollout. |
| 6 | Required cluster correctness (§5.1) | Keep default primary routing; disallow both read_from_replicas and replica-selecting read_routing_strategy. SPUBLISH/SSUBSCRIBE are classified read-only by this client; enabling replica reads could misroute the live tail. |
| 7 | Blocks real topology access if absent (§5.1/§6.3) | Name +cluster for CLUSTER SLOTS and test channel globs. Keep DFLYCLUSTER/DFLYMIGRATE denied to the daemon. Exact Cloud ACL acceptance remains a probe. |
| 8 | Correct proof scope (§6.3/§6.4) | Move Fly assertions out of local §5.1. Probe advertised primaries, then the newly advertised primary after managed failover. The cited managed_service_info branch is GetEmulatedShardInfo, so it does not prove that every Swarm topology hides replicas; avoid depending on hidden or hypothetical addresses either way. |
| 9 | Performance measurement before optimization (§6.4) | Confirmed per-frame await. Measure batch N, actual RTT and runner-request p95/p99; roughly N × RTT is the network contribution. Adopt bounded same-channel pipelining only if needed, preserving order/backpressure and best-effort loss semantics. |
| 10 | Operational correctness; RTT is an input (§7.2/live §2) | Record Fable's report of roughly 5 ms Fly iad → AWS us-east-1 as a planning input, not a fabricated Indy quote or measured SLO. Prefer stopping Machines; pin running/total counts and use --ha=false if deploying from zero, then establish/verify the approved count. Suspend actual starter workflows including deploy-dev-verify. |

Billing qualification: source deletion can erase attribution, but it does not prove every orphan has suffered a collision. Preserve unimplicated historical nulls and document detection limits.
The existing schema's global key may already have merged stage charges, dropped receive rows and undercounted a fleet's budget (afd_billing/src/sql.rs). A new key prevents recurrence; it cannot reconstruct missing history or authorize financial adjustments.

| Additional source checked | Evidence |
|---|---|
| redis-rs 1.6.0 src/subscription_tracker.rs:70–96 and src/cluster_handling/async_connection/mod.rs:211–226,1292–1295 | Tracking follows requests; only Disconnection triggers reconnect and its resubscribe path. Other pushes reach the caller. |
| Dragonfly e94300e6 src/facade/dragonfly_connection.cc:734–739 | Server force-unsubscribe sends sunsubscribe/channel/0 over a still-open RESP3 socket. |
| redis-rs 1.6.0 src/commands/mod.rs:228–233 and src/cluster_handling/client.rs:486–505 | Sharded pub/sub is read-only for routing; default primary strategy can be changed by either replica-routing API. |
| Dragonfly e94300e6 src/server/cluster/cluster_family.cc:52,147–153 | CLUSTER belongs to @slow; the cited managed-service filtering branch is in the emulated-topology function. |
| schema/710_usage_ledger.sql:50,91; rustd/crates/afd_fleet/src/lease/sql/renew.rs:119–149 and sql/report.rs:140–170 | Fleet FK nulls on deletion, broad UPDATE exists, wallet and ledger are sibling CTEs, and six columns accumulate. |
| rustd/crates/afd_fleet/src/lease/activity.rs:115; deploy/fly/agentsfleetd-dev/fly.toml; .github/workflows/deploy-dev-verify.yml:80 | Each frame publish is awaited; the daemon has no proxy service in that file; verification can restart a stopped Machine. |

[PostgreSQL column grants](https://www.postgresql.org/docs/17/sql-grant.html) support the narrow update boundary; the old table-level privilege must be revoked during upgrade.
[Fly availability defaults](https://fly.io/docs/apps/app-availability/) distinguish service processes from processes without services; do not claim that every default deploy starts two active workers.
[The auth recovery boundary](../../AUTH_DEVICE_LOGIN.md#planned-dragonfly-recovery-boundary) now records the selected PostgreSQL mitigation and preserves the current protocol, including its intentional same-fingerprint response retry.

### Follow-up at 6603eb7d8: final feedback and adversarial review

Historical disposition at 6603eb7d8. The b01294265 follow-up below supersedes auth import and the generic PostgreSQL evidence requirement. Source checks remain valid; runtime behavior is NOT RUN.

| Feedback | Decision and blocking boundary | Spec proof |
|---|---|---|
| 1: Single-use replay is a blocker; add WAIT | Agree on the invariant and the pre-existing source exposure. Reject WAIT as a complete mitigation: acknowledged data may still be lost on promotion, and WAIT on a successor cannot attest to a lost old-primary consume. Choose the review's PostgreSQL alternative now, within existing tenant/connector services, with commit-before-release and no dual auth authority. Blocks auth integration until proven, not §1 baseline work. | Canonical durable-auth design, §5.4 and §6.2; device retries/expiry/attempts/cancellation and every nonce family, races, commit uncertainty, queue failover and restore. WAIT remains optional diagnostics only. |
| 2a: Reconnect after hub resubscribe | Add the explicit no-duplicate-frame proof after hub repair followed by reconnect. The client's set avoids duplicate tracker entries, but the hub must also avoid duplicate local forwarding. This does not assert exactly-once best-effort pub/sub across failures. | §0.1 uniquely published frames, wanted-membership reconciliation and final-drop races. |
| 2b: Accumulation needs SELECT too | Retain existing table SELECT/INSERT while replacing broad UPDATE with the six-column grant. Prove the actual DO UPDATE reads and writes under api_runtime as well as rejected identity changes. | §2.2; no new marker table or tenant WHERE that skips the ledger while draining the wallet. |
| 2c: Deleted-fleet audit limits | Preserve the qualification: a deleted fleet is not itself corruption, and missing history can hide collisions. Null legacy identity alone does not block; detected damage needs Indy's bounded disposition. | Canonical ledger audit and §2.2/§7.1; never reconstruct missing charges by guessing. |
| 3: RTT attribution | Attribute acceptance of about 5 ms Fly iad → Dragonfly Cloud AWS us-east-1 to Indy, confirmed by the final handoff, rather than to Fable. Preserve the supplied decision without inventing an unavailable original sentence or timestamp; it is not measured request p99. | Both Discovery records and the canonical frame/request budget. |

The revised auth design changes backing stores, not the cryptographic or HTTP protocol. It adds two bounded PostgreSQL state tables and uses existing pools/services; no new service, consensus layer, provider mode, or general auth framework is planned.

| Source / counterexample | What it establishes |
|---|---|
| Dragonfly e94300e6 src/server/server_family.cc:3641–3781 | WAIT captures global shard LSNs, solicits replica ACKs and returns a count; replica/role changes and timeout are explicit. The wait loop sleeps 100 ms when the first count is insufficient. It is real, but neither one in-zone RTT nor safe promotion is established. |
| redis-rs 1.6.0 src/cluster_handling/routing.rs:558,662 | WAIT fans out to all primaries and takes the minimum; this is an availability/latency coupling, not an atomic consume-plus-replication transaction. |
| Consume succeeds on A; A loses the consume before WAIT; cluster retry sends WAIT to B | B's current journal does not contain the old mutation. A successful acknowledgment cannot prove a mutation absent from that journal. This is a design counterexample, not a reported test result. |
| rustd/crates/afd_tenant/src/session/mod.rs; afd_redis/src/session/verify_consume.lua; afd_connector/src/state/nonce.rs and connect.rs | Existing services own the device state machine, same-fingerprint retry and nonce spend-before-exchange; move storage while retaining caller authorization order and cryptographic boundaries. |
| schema/710_usage_ledger.sql:91; three ledger statements | SELECT already exists; effective privileges must allow real accumulation while rejecting identity mutation. |

[Dragonfly's WAIT documentation](https://www.dragonflydb.io/docs/command-reference/generic/wait) explicitly limits failover guarantees. [Upstash's consistency documentation](https://upstash.com/docs/redis/features/consistency) establishes asynchronous replication, not an observed incident on this deployment.
[PostgreSQL synchronous replication](https://www.postgresql.org/docs/17/warm-standby.html#SYNCHRONOUS-REPLICATION) and [commit configuration](https://www.postgresql.org/docs/17/runtime-config-wal.html#GUC-SYNCHRONOUS-COMMIT) distinguish durable local commit from a properly configured failover guarantee.

The separate adversarial pass after the amendments found and corrected these gaps:

| Gap | Correction / remaining boundary |
|---|---|
| Moving auth to PostgreSQL could merely move the replay window | Require primary reads and acknowledged-commit durability under the actual service's promotion policy. synchronous_commit=on alone is insufficient. Verify that policy before Cloud readiness; lossy/unverified promotion blocks rollout. Its configuration remains unverified, not silently assumed. |
| Source re-import could recreate a spent nonce | Auth stays closed before the protected import receipt. Initial import preserves expiry and terminal state; after completion the tool verifies only and refuses another auth copy. Queue snapshot restore never overwrites PostgreSQL auth. §2.3/§5.4/§7.1 cover these boundaries. |
| Files Changed implied editing already-shipped schema slots | Replace that scope with new numbered migrations and current Rust registration. Fresh/populated upgrade, interrupted rerun and pre-admission abort must work; the retired Zig embed is not a dependency. This is required before §2 schema implementation. |
| §6 used an unnamed “baseline” for a workload absent from historical Redis | Name three matched fault-free Dragonfly references per case, fixed fault/recovery windows, relative post-recovery checks and absolute budgets. Historical B cannot be the denominator. Collect these before fault grading. |
| New auth persistence lacked cost/scope coverage | Include tenant/connector/API composition, tables/grants and tests; freeze auth transaction/commit rates, request latency, pool contention, WAL and expiry cleanup under combined load. This extends the existing PostgreSQL budget, not the Dragonfly throughput claim. |

Remaining execution prerequisites are explicit: §1 needs CHORE(open), B0 and an owned reset rig; §0 needs local cluster setup; §6 needs numeric budgets, paid capacity approval, real Cloud identity/TLS/ACL and PostgreSQL durability evidence. Shard hostname/IP behavior and vendor IP restriction support remain named Cloud probes, not a peering project.
No remaining text defect found in this pass prevents §1. This is readiness to start the ordered implementation, not proof that later integration, load or live cutover can pass.

The docs/TEMPLATE.md update carries the general lessons into authoring: name enforcement and failure assumptions, verify both protocol sides, trace real writers, handle historical identity and populated upgrades, budget the full path with a named comparator, distinguish evidence tiers and decision provenance, and run a separate adversarial pass. It preserves the existing sections, rubric and 320-line filled-spec limit.

### Follow-up at b01294265: cutover simplification and governance

Historical disposition before Indy's recovery override below. Finding 2's reverse-migration requirement is superseded; other corrections remain. All runtime and Cloud proofs remain NOT RUN; §1 has no new prerequisite.

| Finding | Adversarial assessment and correction | Proof boundary |
|---|---|---|
| 1: Remove auth import | Adopt explicit invalidation of in-flight device/connect flows. Count only; exclude their values from exports/imports and start new PostgreSQL auth tables empty. Source TTLs of 300/600 seconds do not prove a minimum cutover duration; the restart rule applies even to a fast cutover. Existing credentials and installed grants survive. | §7.1 and live §2.1 prove old flows reject and new flows work, with no auth-copy or auth-format receipt field. |
| 2: Old migrator refuses new versions | Confirmed unconditional unknown-version check before AheadPolicy. Fly runs migrate as its release command. Reverse each new migration in reverse dependency order, restore original data/constraints/grants, then remove only its matching migration ledger entry atomically where supported. Never erase bookkeeping first or bypass refusal. | §7.1 proves interrupted reversal and old-build migrate exit zero, then boot/source checks under the fence before reopening. After new work/auth admission, use forward recovery. |
| 3: Concurrent verify loser | A zero-row guarded update must re-read committed state with a fresh snapshot and evaluate the same-fingerprint retry window. Serialization failures retry the transaction. No unconditional 410 from the row count. | §5.4 races same-fingerprint verifies and proves both receive the permitted identical encrypted response; other fingerprints still reject. |
| 4: PlanetScale evidence | Cite architecture plus durable replica-confirmation documentation. Additional operations documentation names most-caught-up promotion. Narrow the remaining question to loss/partition of the acknowledging replica; record the provider answer and actual deployed posture. Any negative/ambiguous result goes to Indy as a platform-wide billing/admission/auth risk, without presumed acceptance. | §6 evidence and live preflight require the answer/disposition; local proofs and §1 proceed. No vendor ticket was sent. |
| 5: Subscription ownership | One subscribing ClusterConnection object per daemon hub, shared across channels/viewers and reused on repair. Multiple node sockets remain necessary. | §5.3 checks client ownership and node resources; §0.1 checks reconnect/movement without duplicate forwarding. |
| 6: Template governance | User authorization already exists in this conversation; quote below. Align dispatch authoring guidance and isolate the template change in its own forward commit, without rewriting b01294265 or changing the gate script. | Template/dispatch authoring checks; no runtime criterion is credited. |

Source checks: afd_redis/src/session.rs:88 (300-second session TTL); afd_connector/src/registry.rs:55 (600-second state TTL); afd_db/src/migrate.rs:191–201,238–275 (unknown-version refusal and transactional migration ledger); both deploy/fly/agentsfleetd-*/fly.toml files (migrate release command).
[PlanetScale replicas](https://planetscale.com/docs/postgres/scaling/replicas) documents durable confirmation before commit success. [Operations philosophy](https://planetscale.com/docs/postgres/operations-philosophy#primaries) identifies most-caught-up promotion; neither was treated as evidence that the actual deployment has passed a fault test.

**Indy's explicit template authorization, verbatim from this conversation:**
> And the learning you did in this who process and go and update the base TEMPLATE.md so the subsequent agents can come up with one shot implementation from the spec produced by orly from the template.

Scope: docs/TEMPLATE.md and the matching dispatch/write_spec.md authoring guidance. This is the separately requested authoring-policy change, outside M192 runtime implementation scope. It changes no gate script, required section, line cap or verification command. Forward reversion/reapplication separates the previous mixed template edit without rewriting committed history.

Adversarial checks after correction: count-only auth handling cannot resurrect a source token; the ordinary deployment receipt does not imply empty auth tables on later deploys; reverse-migration bookkeeping cannot conceal unapplied reversal; provider documentation does not silently authorize a platform risk exception. These requirements are documented, not tested runtime outcomes.

### Indy recovery override: pre-production forward recovery

**Indy, verbatim:** "since We are not in production yet, so i would just skip that".
Context: the user accepts removing reverse migrations and the promise of old-image rollback because the product is not in production.
The old migrator's refusal remains a valid source fact. The selected recovery policy no longer depends on booting that binary after schema changes.
Cancel before any new migration is applied; after the first new migration commits, keep affected writers fenced and fix/redeploy the compatible new build.
Complete forward migration/import reconciliation before reopening. Remove reverse DDL, migration-ledger deletion and old-build migrate-success assertions from §7.1 and the live procedure.
This scope decision preserves data and migration bookkeeping; it authorizes no database reset. The playbook records the possibility of a longer outage during repair.
The canonical design and both specs quote the decision. Fresh/populated upgrade, interrupted forward recovery, source preservation and all other correctness proofs remain required and NOT RUN.

### Final implementation-readiness review

Verdict: ready to begin the ordered implementation at §1; no open design blocker for that slice. This is a design review, not runtime acceptance.
The review checked Section dependencies, real producer/billing/auth boundaries, cluster ownership, test mappings, full-path capacity budgets, source inventory and the updated forward-recovery decision.
One remaining ordering gap was corrected: §5/§6 application tests need a valid startup receipt, while the full import tool belongs to §7. §2 now delivers the same tool's operator-only empty-source initializer for owned fixtures, with nonempty-source/wrong-target rejection; §7 adds populated import. No daemon bypass is introduced.
The live spec now states the auth restart boundary in its invariant and removes a duplicate Files Changed row.

| Boundary | Prerequisite and resulting action |
|---|---|
| Start §1 | CHORE(open) records B0 and opens the implementation lifecycle; use the owned Redis/PostgreSQL reset rig. Harden/collect twelve historical samples before production-source or schema edits. No Cloud account or new compose topology is required for this slice. |
| §0 and local integration | Bring up the specified local cluster; pass pinned-client/server prototypes, then follow §2 → §5 → §3 → §4. Failed proofs stop their dependent slice. |
| §6 capacity and Cloud | Freeze numeric budgets and approved paid capacity; obtain Cloud identity, Fly TLS/ACL, managed-fault and PlanetScale promotion evidence/disposition. These are measured/operational inputs, not reasons to delay §1. |
| Live switch | Require the completed candidate, source census, stopped writers, import reconciliation and explicit deployment approval. After a schema migration commits, recover forward; no reverse-migration requirement remains. |

No additional sharding layer, provider mode, auth-import system or rollback framework is required. Further batching/partitioning follows measured need. All runtime and Cloud proofs remain NOT RUN.

### Prototype admission and completion

Each risk proof records a failing control, the mitigation, observed results, immutable raw files, and the candidate revision/configuration.
A passing documentation check does not clear a risk.
The prototype grader requires historical capture and the complete risk matrix; dependent integration refuses missing or failed evidence.

| Prototype | Failure injection and measurable result | Promotion condition |
|---|---|---|
| Sharded live tail | Wrong node, restart, slot movement, unsubscribe races, slow viewers, and bounded memory/socket counts. | Frames resume within budget; no tenant misrouting, permanent-error reconnect loop, or unbounded buffers. |
| Outbox and retained scripts | CROSSSLOT test-only control, single-key Lua flush, lost XADD replies, moved slots, and cancelled BLOCK. | Physical duplicates preserve logical identity/order; Dedicated resources return to baseline. |
| Durable identity and replay | Stop around admission/receipt/charge commits and append; destroy queue; replay late; race expiry and gate refusal. | No lost acceptance or missing eligible debit; no double charge/run/count; pre-charge refusals remain unpaid and post-charge approval policy survives. |
| Durable authentication | Race approve/consume/cancel and nonce spends, stop around PostgreSQL commits, lose responses and restore/fail over the queue. | No unauthorized ciphertext/exchange or terminal-state resurrection; permitted retry and expiry remain intact. PostgreSQL durability and combined-load cost require separate evidence. |
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

Hashes detect altered bytes, and the historical archive is pinned to a Git evidence commit; authenticated CI provenance identifies future Cloud collector runs.
The canonical [workload evidence](../../architecture/datastore_scaling.md#workload-and-evidence) names the Cloud GET mechanism and secret-redaction boundary.
Official source proves the request shape exists, not that an available key can access the intended datastore or that the account has passed the probe.

### Scope and first executable slice

Leave the existing deployed Redis build in service while proving the new build on the local cluster; coordinate the first merge/deploy with Indy.
The readiness/live delivery split and fixture-only outbound remain proposals; this correction does not add an outbound product feature.
Indy's override removes temporary standalone transport; no automatic fallback or early-merge global deployment hold is planned.

The first implemented slice is §1: benchmark address/fixture hardening, twelve-sample capture, sidecars/digests, baseline grader, and B/B0 dependency proof.
It needed full Git history in the Rust unit and coverage jobs, with no compose change. §0 requires local cluster setup under Indy's stated scope before its implementation.
M192_001 is ACTIVE with §1 complete. M192_002 remains PENDING on the readiness work.

## Limits

No migration-confidence or capacity verdict is claimed. Dragonfly, migration, Cloud, and capacity runtime proofs above remain outstanding.
The next implementation work is local cluster setup and the isolated prototypes before production integration.
A failed prototype requires a corrected design and another run before its dependent integration proceeds.

## Related pages

- [Target design](../../architecture/datastore_scaling.md) contains the mechanisms and resource boundaries.
- [Dragonfly pub/sub](https://github.com/dragonflydb/dragonfly/blob/1e5f9944834b6ed999a2baf137e929e6de3e3009/docs/pub-sub.md) documents the required command family.
- [Dragonfly cluster design](https://github.com/dragonflydb/dragonfly/blob/1e5f9944834b6ed999a2baf137e929e6de3e3009/docs/cluster-mode.md) documents slots and migration configuration.
- [PlanetScale Neki](https://planetscale.com/docs/postgres/sharding) describes the separate PostgreSQL sharding layer.
