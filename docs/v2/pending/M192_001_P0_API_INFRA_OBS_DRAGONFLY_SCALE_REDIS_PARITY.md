<!--
SPEC AUTHORING RULES (load-bearing — the one comment that survives):
- Body order = the executing agent's read order. Fill via the orly-spec-new
  skill (authoring order lives there); after filling, DELETE every "tpl:"
  guidance comment — the SPEC TEMPLATE GATE blocks tpl residue, unfilled
  {slots}, and missing required sections (audits/spec-template.sh --staged).
- No time/effort/hour/day estimates anywhere. No effort columns, complexity
  ratings, percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only
  sequencing signal. A section that contradicts these rules loses — delete it.
-->

# M192_001: Prove and prepare the sharded Dragonfly migration

**Prototype:** v2.0.0
**Milestone:** M192
**Workstream:** 001
**Date:** Sep 11, 2026
**Status:** PENDING
**Priority:** P0
**Categories:** API, INFRA, OBS
**Batch:** B2
**Branch:** docs/m192-dragonfly-migration; authoring only, implementation branch pending CHORE(open).
**Test Baseline:** pending; CHORE(open) records B0, canonical baseline counts are due before the implementation Pull Request.
**Depends on:** M188_001 drivers exist; its address/fixture safety deferral is pulled into §1 before any remote workload.
**Provenance:** Codex revision following Fable review and Indy's approval to redesign sharding and prototype risks.
**Canonical architecture:** `docs/architecture/datastore_scaling.md`; runtime context in `docs/architecture/data_flow.md`.

## Overview

**Goal (testable):** a sharded Dragonfly build passes prototype, application, capacity, Cloud recovery, and migration-rehearsal proofs before live cutover.
**Problem:** standard pub/sub, cross-slot scripts, global coordination, queue-only acceptance, and weak evidence cannot establish Swarm readiness.
**Solution summary:** prototype first, then refactor around sharding, durable event/auth state, bounded resources, and verifiable results.
One new daemon targets local Dragonfly clusters and Indy-created Cloud Swarm. Existing Redis stays deployed until the coordinated switch; no temporary runtime provider mode.

## PR Intent & comprehension handshake

- **PR title:** feat: prepare sharded Dragonfly migration with fault-tested recovery
- **Intent:** preserve fleet behavior while proving migration readiness; never label readiness as completed live migration.
- **Handshake:** accept the approved sharded redesign, prototype risk boundaries, then integrate the proven designs.
- **ASSUMPTIONS I'M MAKING:** incorporate the requested Fable corrections; local-first cluster deployment follows Indy's quoted override below; live actions and paid capacity still need concrete approval.

## Implementing agent — read these first

1. `docs/architecture/datastore_scaling.md` and `docs/v2/reviews/M192_REVIEW_RESOLUTION.md`, target design and risk ledger.
2. `docs/v2/done/M188_001_P1_API_INFRA_OUTBOUND_AND_LEASE_THROUGHPUT_BENCH.md`, including deferred address/fixture hardening.
3. `rustd/crates/afd_redis/src/hub/pump.rs`, `rustd/crates/afd_redis/src/streams/once.rs`, and `rustd/crates/afd_sse/src/channel.rs`.
4. `rustd/crates/afd_fleet/src/lease/report.rs`, `rustd/crates/afd_ingress/src/deliver.rs`, and `docs/AUTH_DEVICE_LOGIN.md`.
5. https://github.com/dragonflydb/dragonfly/blob/1e5f9944834b6ed999a2baf137e929e6de3e3009/docs/pub-sub.md and its adjacent `cluster-mode.md`; validate against the pinned release.

## Files Changed (blast radius)

| File | Action | Why |
|---|---|---|
| `AGENTS.md`, `docs/architecture/{datastore_scaling,data_flow,runner_fleet,concurrency,scaling,testing,observability,roadmap,README}.md` | EDIT | Record target sharding, runtime changes, and evidence boundaries. |
| This spec; `docs/v2/pending/M192_002_P0_INFRA_OBS_DRAGONFLY_CUTOVER_REDIS_RETIREMENT.md`; `docs/v2/reviews/M192_REVIEW_RESOLUTION.md`; `docs/v2/reviews/datastore-scale-evidence.md` | EDIT / CREATE | Readiness/live split, review dispositions, immutable evidence index. |
| `rustd/crates/afd_bench/{Cargo.toml,src/**/*.rs,tests/**/*.rs}`, `make/bench.mk`, `bench/{profiles,baselines}/datastore/*.json` | EDIT / CREATE | Baseline capture, hardening, prototype collectors, combined workloads, strict grader. |
| `rustd/{Cargo.toml,Cargo.lock}`, `rustd/crates/afd_redis/{Cargo.toml,src/**/*.rs,tests/**/*.rs}` | EDIT / CREATE | Cluster, sharded hub, keys, blocking connections, scans, and fault tests. |
| `rustd/crates/{afd_core,afd_billing,afd_events,afd_ingress,afd_cron,afd_gate,afd_approval,afd_runner,afd_fleet_lifecycle}/{Cargo.toml,src/**/*.rs,tests/**/*.rs}` | EDIT / CREATE | Every real admission/replay producer and install/group lifecycle. |
| `rustd/crates/{afd_fleet,afd_wire,afd_outbound}/{Cargo.toml,src/**/*.rs,tests/**/*.rs}` | EDIT / CREATE | Logical/physical identity, fencing, partitioned coordination, fixture-only outbound. |
| `rustd/crates/{afd_sse,afd_api,afd_observability,agentsfleetd}/{Cargo.toml,src/**/*.rs,tests/**/*.rs}` | EDIT / CREATE | Unchanged channel parser, cluster probes, import-receipt guard, telemetry, application transport proofs. |
| New numbered `schema/*.sql` migrations; `rustd/crates/afd_db/{src/migration.rs,tests/*.rs}` | CREATE migrations / EDIT registration and tests | Admission, ledger/grants, numeric indexes, counters and auth tables; shipped slots stay frozen. Fresh/populated upgrade and interrupted-rerun proofs. |
| `rustd/crates/{afd_tenant,afd_connector,afd_api_tenant}/{Cargo.toml,src/**/*.rs,tests/**/*.rs}` | EDIT / CREATE | PostgreSQL device/nonce stores, existing protocol semantics, commit-before-release and no Redis fallback. |
| `make/{test-infra,test-integration-rustd,acceptance}.mk`, `Dockerfile`, `docker-compose.yml`, `.github/workflows/{bench,test-integration-rustd,deploy-dev,deploy-dev-fly,deploy-dev-verify,release}.yml`, `deploy/fly/agentsfleetd-*/fly.toml` | EDIT after required approval | Local cluster/image proof; existing vault-to-Fly secret flow and import preflight; no new cloud provisioning system. |
| `playbooks/operations/datastore_scaling/{001_playbook.md,*.sh}`, `playbooks/founding/02_preflight/00_gate.sh`, `playbooks/README.md` | CREATE / EDIT | Bounded source migration tool, rehearsal, and cutover procedure. |
| `docs/AUTH_DEVICE_LOGIN.md`, `ui/packages/app/tests/**/*.ts`, `cli/tests/**/*.ts` | EDIT if affected | Authentication migration and real existing acceptance fixtures. |
| Separate `~/Projects/docs` branch and its changelog | EDIT during implementation if public behavior changes | Document preserved ID shape and revised retry/stream semantics; never edit from this worktree. |

Expand globs at PLAN; separate application, test, and operational edits. Workflow/deploy mutations need concrete approval; authoring starts no datastores.

## Applicable Rules

- UFS, OWN, ECL, TIM, STR, TCF, VLT, NSQ, STS, ORP, and TST-NAM from `docs/greptile-learnings/RULES.md`.
- `dispatch/write_rust.md`, `docs/RUST_ERROR_STANDARD.md`, `dispatch/write_sql.md`: source chains, ownership, atomic migration and tenant boundaries.
- `dispatch/name_architecture.md`, `docs/DOCUMENTATION_RULES.md`, `docs/AUTH.md`, `docs/AUTH_DEVICE_LOGIN.md`: coherent target and one-time authentication.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|---|---|---|
| SPEC TEMPLATE | Authoring | Complete sections, real references, mapped proofs, canonical commands. |
| Architecture, AUTH, SCHEMA | Implementation | Target recorded before code; identity and migration fault proofs. |
| LENGTH, UFS, LOGGING, ERROR REGISTRY | Source edits | Cohesive modules, typed failures, bounded and redacted signals. |
| VERIFY | Implementation | Canonical suites plus recorded prototypes, Cloud evidence, and rehearsal. |

## Prior-Art / Reference Implementations

Reuse M188 drivers, real Redis fault fixtures, redis-rs cluster routing, and Dragonfly's documented sharded pub/sub and migration procedures.

## Sections (implementation slices)

Execution: §1 → local cluster setup → §0 → §2 → §5 → §3 → §4 → §6 → §7. §2 proves admission on the existing Redis fixture; §1 needs no CI/compose edit.
### §0: Prototype Dragonfly primitives and sharded coordination

Dependencies: §1 and local cluster compose setup within Indy's stated scope. Test-only prototypes precede the runtime refactor; this pass edits documentation only.
Use canonical candidate pins, listen/publish wiring, health and snapshot/restart proofs; resolve the image digest first. Keep administrative commands on the main port and measure fault recovery/resources.
Use the canonical single-service multi-process cluster: 127.0.0.1:7001..700N advertisements, host-published ports, daemon network_mode: "service:dragonfly", stable --cluster_node_id, per-node --dir/snapshot_cron, config bootstrap on every start, two primaries and replicas.
Compare per-fleet streams with bounded partitioned-stream layouts if population cost misses budget; public ordering and fencing govern the choice.
The hub handles SUnsubscribe by reconciling viewers and reissuing SSUBSCRIBE; test healthy-socket/final-drop races and reconnect after repair with no duplicate local frame delivery. Archive results; missing/failed proofs block dependent integration.

- **Dimension 0.1**: sharded pub/sub routes from another seed and recovers without unbounded buffers → Test `test_sharded_tail_prototype_recovers`.
- **Dimension 0.2**: retained single-key scripts and dedicated readers survive topology movement → Test `test_cluster_primitives_prototype_survives_movement`.
- **Dimension 0.3**: durable identity survives queue loss and reordered physical receipts → Test `test_durable_identity_prototype_survives_replay`.
- **Dimension 0.4**: sharded readiness and destination scheduling preserve bounded fair progress → Test `test_sharded_coordination_prototype_bounds_hotspots`.

### §1: Safe historical baseline and trustworthy capture

Dependencies: none; address hardening and capture come first. No production source/schema change before B; bench-only plumbing is allowed.
Prove B/B0 production source/schema/build and dependency-closure equality, including Cargo.lock; allow only proven bench-exclusive deltas. Capture three samples per existing `make bench-steer`, `bench-lease`, `bench-outbound`, and `bench-cardinality`.
Archive each fixed-path result immediately under a unique campaign/lane/sample path with sidecars containing B, parameters, payload bytes, window, resources, raw server/topology output, and SHA-256 digests.
Historical drivers have no seed or offered-rate guarantee; preserve their actual measurements and mark unavailable fields explicitly.
Fix the M188 rig-label/address gap before capture or remote use: verify both datastores and every discovered node, reject shared targets, scope leases and consumers.
Test cancellation cleanup and orphan recovery. Missing provenance, changed bytes, inconsistent topology, or overwritten samples must fail the grader.

- **Dimension 1.1**: tampered or incomparable evidence fails closed → Test `test_incomparable_datastore_runs_are_rejected`.
- **Dimension 1.2**: rig labels and credentials cannot authorize a shared or remote target → Test `test_shared_deployment_refuses_saturation_profile`.
- **Dimension 1.3**: capture retains twelve distinct historical samples and valid sidecars → Test `test_redis_baseline_records_complete_evidence`.

### §2: Durable admission and stable logical identity

Dependencies: §0.3 and §1, not §5. Prove durable acceptance and all producer migrations on isolated Redis before cluster integration; no early runtime deployment.
Use the canonical PostgreSQL producer-identity/expiry authority and ordered outbox; remove admission Lua claims across all producers. Preserve numeric IDs and separate physical XACK receipts.
Use canonical receipt/ledger atomicity, zero charges, renewal policy, gates and counters; no second billing marker. Prove cross-fleet budget enforcement and wallet/ledger rollback together. Missing/mismatched import receipt closes all admission paths.
Cover every producer/expiry and numeric history/cursors. All three billing writers supply immutable billing_fleet_id; legacy orphan nulls survive without guessing. Revoke broad UPDATE, retain SELECT and grant UPDATE only on six accumulator columns; prove actual accumulation reads/writes. No tenant WHERE that skips ledger while wallet drains. Audit collisions; require a disposition for detected damage.
The pinned report path has no outbound enqueue caller. This draft preserves that behavior; fixture-only outbound tests cannot claim delivered connector answers.

- **Dimension 2.1**: each admission crash boundary retains accepted work → Test `test_acceptance_recovers_at_each_crash_boundary`.
- **Dimension 2.2**: duplicate receipts produce exactly one eligible receive debit and no repeated terminal settlement → Test `test_queue_loss_replays_without_duplicate_settlement`.
- **Dimension 2.3**: database failure or incomplete import refuses every admission path without false success → Test `test_durable_store_failure_refuses_acceptance`.

### §3: Retention, memory limits, and backpressure

Dependencies: §2 and §5. Use No Eviction and bounded per-fleet/deployment durable admission.
Replace unconditional MAXLEN trimming with cleanup constrained by pending work and durable replay coverage; acknowledged history remains bounded.
Recover missing consumer groups from durable dispatch/settlement state; recreating at $ must not skip accepted events.
Measure all retained key classes, pending entries, replicas, subscription buffers, and PostgreSQL backlog; classify OOM and quota refusals explicitly.

- **Dimension 3.1**: slow consumers and group loss retain unfinished work → Test `test_retention_preserves_pending_work_under_pressure`.
- **Dimension 3.2**: quota and OOM responses apply explicit backpressure → Test `test_full_datastore_applies_explicit_backpressure`.
- **Dimension 3.3**: capacity includes every retained state class → Test `test_capacity_report_accounts_for_all_retained_state`.

### §4: Sharded readiness and bounded fair scheduling

Dependencies: §0.4, §2, and §3. Measure shared coordination; refactor hot keys when the workload misses frozen budgets.
Prototype partitioned readiness by stable fleet hash; choose partition count from measurements, and version its map if adopted.
Each runner rotates a bounded partition cursor and candidate budget. Token-checked clearing and durable repair preserve newly ready work.
If partitioning is required, fence/version/replay map changes; otherwise retain the measured passing design. Only partition-specific §4.1/4.3 asserts may be N/A with archived budget proof; base races and worker-loss recovery still pass.
For fixture outbound, compare destination partitioning with the existing worker; adopt only when measured budgets require it. Preserve destination order and bounded concurrent retries either way.
Prove recovery and fairness with skewed runner eligibility; aggregate throughput alone cannot pass a starvation check.

- **Dimension 4.1**: concurrent ingress and stale clears preserve partitioned readiness → Test `test_ready_races_preserve_work_and_bound_poll_cost`.
- **Dimension 4.2**: skewed eligibility and slow fixture destinations cannot starve unrelated work → Test `test_skewed_workload_preserves_discovery_and_delivery_fairness`.
- **Dimension 4.3**: partition movement and worker loss preserve ordered recovery → Test `test_coordination_recovers_during_partition_movement`.

### §5: Integrate cluster transport and durable auth state

Dependencies: §0, §1, and §2. Remove all admission Lua first; route every command to primaries. Never enable read_from_replicas or replica-selecting read_routing_strategy, including for sharded pub/sub.
Use SPUBLISH/SSUBSCRIBE/SUNSUBSCRIBE with RESP3; one subscribing ClusterConnection object per hub, reused during repair, with bounded node sockets. No daemon standalone fallback; retain afd_redis::client::Redis for tools/fixtures.
Keep fleet:<id>:events and fleet:<id>:activity unchanged; route the channel by its own slot. Preserve afd_sse parsing and verify workspace fan-in.
Single-key XADD publishes PostgreSQL admissions; no target Redis ingress claims. Move device sessions and all connector nonce families to the canonical PostgreSQL authority; no target session Lua or Redis auth cache. Probe retained coordination scripts and a CROSSSLOT control.
Extend existing Dedicated ownership with node routing/redirects; standalone already isolates blocking reads. Scan all primaries with topology reconciliation.
Cluster/auth errors fail readiness; explicitly allow +cluster for topology discovery and deny DFLYCLUSTER/DFLYMIGRATE to the daemon. Sharded pub/sub and import-receipt checks precede admission/dispatch.
Prove guarded auth transitions, durable commit before protected responses/exchange, existing same-device retries and bounded expiry cleanup; receipt guards auth too. WAIT is an optional diagnostic, never the security mechanism.
Local tests keep prefix isolation and database 0; owned-cluster reset covers all primaries/replicas. Never flush a Cloud/shared target.

- **Dimension 5.1**: unsafe topology, TLS, auth, eviction, or pub/sub capability refuses boot → Test `test_datastore_preflight_refuses_invalid_configuration`.
- **Dimension 5.2**: resharding preserves outbox logical order despite ambiguous appends → Test `test_cluster_resharding_preserves_outbox_identity`.
- **Dimension 5.3**: subscriptions, scans, and dedicated readers recover across nodes → Test `test_cluster_connections_recover_without_missing_scoped_state`.
- **Dimension 5.4**: durable auth transitions preserve single use and permitted retries → Test `test_durable_auth_survives_queue_failure`.

### §6: Application, Cloud, and sustained-load readiness

Dependencies: §2 through §5 and Indy-approved budgets/infrastructure for 1,000 runner processes and paid Swarm. Reuse real fixtures locally, then on Indy's dedicated Cloud rig.
Run `make test-integration-rustd`, `make acceptance-e2e`, and `make cli-acceptance`; use synthetic execution providers and no real outbound messages.
Extend M188 with combined acceptance-to-completion load: 1,000,000 active fleet records, 1,000 concurrent runs, 1,000 real runner processes, 16,667 offered events/second.
Use 1,000-byte and 16,000-byte/skew cases; freeze both stores’ budgets: commits, admission/auth p95/p99 and rates, allocator/pool waits, WAL/I/O, auth expiry storage/cleanup, cost, backlog/drain, frames/bytes per second, viewer fan-out and lag. Count fan-out admissions separately.
Measure frame-batch/runner-request latency before pipelining. Freeze three matched fault-free Dragonfly reference samples per case; post-recovery completion p99 <=1.10x reference and completion rate >=0.95x, plus absolute budgets and fault/drain windows. Historical Redis B is not this denominator; reject generator saturation/backlog.
Prove GET identity, Fly TLS/ACL access to advertised/newly promoted primaries, replicas >=1, backups and provenance. Verify the documented PlanetScale commit/promotion posture; record the remaining promotion failure question and any platform-wide Indy disposition. Rehearse restore/mirror cleanup; queue failover must never reopen PostgreSQL auth state.

- **Dimension 6.1**: real application and streaming paths preserve documented behavior → Test `test_dragonfly_preserves_application_outcomes`.
- **Dimension 6.2**: failure/session semantics survive races and script loss → Test `test_dragonfly_preserves_failure_and_session_semantics`.
- **Dimension 6.3**: missing Cloud proof or fabricated provenance refuses readiness → Test `test_dragonfly_evidence_refuses_unsafe_or_incomplete_runs`.
- **Dimension 6.4**: sustained workload and managed faults meet frozen budgets → Test `test_dragonfly_sustained_workload_has_complete_evidence`.

### §7: Migration tooling and deployment-safe rehearsal
Dependencies: §6. The offline tool reads old Redis state; the new daemon accepts only Dragonfly clusters. Rehearse the one coordinated durability/Swarm cutover.
Stop old Fly Machines and actual restart/deploy jobs including deploy-dev-verify; pin running/total counts, including deploy from zero. Fence/import/reconcile then record receipt; missing/wrong/unreadable receipt closes admission. Rehearse provider redelivery.
Handle events accepted before durable admission existed. Inventory historical outbound jobs and block on unsupported nonempty state; never assume the queue is empty.
Every prefix has counts/types and disposition; unknown keys block. Count auth sessions/nonces only, exclude them from export/import and restart in-flight flows. Rehearse reverse DDL before exact migration-ledger removal; the old build migrate must exit zero before source resumes.
The existing deploy-dev → deploy-dev-fly path calls a self-tested shell preflight before secret/Machine changes; test with command stubs. Keep the branch unmerged until Indy's coordinated switch is ready.
M192_002 consumes the reviewed M192_001 candidate before merge for live switch preparation; local readiness does not claim a deploy. Its manual proofs own live observation/retirement.

- **Dimension 7.1**: rehearsal preserves old state through import, abort, and recovery → Test `test_cutover_recovery_preserves_source_state`.
- **Dimension 7.2**: workflow dry-run refuses unsafe secret/image changes → Test `test_deploy_preflight_requires_completed_import`.

## Interfaces
Keep REDIS_URL_API, channel names, and numeric event_id; no REDIS_MODE. Indy supplies the Swarm vault reference; existing Fly secret staging selects the endpoint after import preflight.
Add `make bench-datastore CHECK=prototype|baseline|durability|retention|coordination|cluster|readiness|rehearsal|rollout` as the spec-consumed evidence grader.
Keep these grader modes; archive canonical lane logs, exit status, counts and revision as evidence. The planned target grades only; `rollout` requires manual live proof and M188's comparator stays non-failing.

## Failure Modes

| Mode | Handling and negative proof |
|---|---|
| Unsupported standard pub/sub, lost subscriptions | Fail readiness; sharded routing, churn, and bounded-lag proof in `test_sharded_tail_prototype_recovers`. |
| CROSSSLOT, MOVED, blocked shared socket | Single-key runtime scripts, negative controls, and existing Dedicated ownership; `test_cluster_primitives_prototype_survives_movement`. |
| Queue loss, ambiguous writes, identity collision | Durable admission and stable identity; `test_durable_identity_prototype_survives_replay`. |
| Lost auth write, uncertain commit, stale restore | PostgreSQL authority, commit-before-release and refusal on uncertainty; `test_durable_auth_survives_queue_failure`. |
| Premature trim, missing groups, full datastore | Replay-aware cleanup and admission; `test_retention_preserves_pending_work_under_pressure`. |
| Hot key, sparse eligible runners, stalled destination | Sharded bounded scheduling; `test_sharded_coordination_prototype_bounds_hotspots`. |
| Shared rig, unsafe discovered node, cleanup crash | Refuse mutation and recover fixtures; `test_shared_deployment_refuses_saturation_profile`. |
| Forged summary, missing Cloud run, saturated generator | Fail grading; `test_dragonfly_evidence_refuses_unsafe_or_incomplete_runs`. |
| Lost claim TTL, source writers after switch, unsafe rollback | Reconcile and fence migration; `test_cutover_recovery_preserves_source_state`. |
| Deploy before import, wrong target, or unapproved secret mutation | Close admission and fail preflight before mutation; `test_deploy_preflight_requires_completed_import`. |

## Invariants

1. Runnable work and success follow durable acceptance; transactions and crash injection enforce ordering.
2. Logical identity survives receipts and queue loss; database uniqueness and fencing prevent duplicate settlement and billing.
3. Sharded resources are bounded; admission, token checks, per-partition budgets, and cleanup tests enforce limits.
4. Shared deployments never receive destructive fixtures; address, ownership, and discovered-node validation precede mutation.
5. Evidence labels cannot establish provenance; the grader verifies raw digests, server identity, authenticated runs, and manual live approval separately.
6. Auth terminal state is authoritative in PostgreSQL; guarded durable transactions precede protected responses/exchange. Failed proofs block dependent integration and readiness.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|---|---|---|---|---|---|
| Subscription recovery and lag | ops | Movement, reconnect, queue pressure | Node role, outcome, bytes, duration | No channel/fleet labels | `test_sharded_tail_prototype_recovers` |
| Admission, replay, and settlement | ops | Commit, replay, refusal | Outcome, count, duration | No payload or credentials | `test_queue_loss_replays_without_duplicate_settlement` |
| Auth state transition | ops | Commit, refusal, expiry cleanup | Outcome, count, duration | No session/nonce IDs, codes or ciphertext | `test_durable_auth_survives_queue_failure` |
| Partition fairness and memory | ops | Poll, cleanup, sample | Bounded partition index, queue age, bytes | No tenant labels | `test_sharded_coordination_prototype_bounds_hotspots` |
| Evidence verdict | ops | Grading | Revision, digest, environment class | Redacted addresses | `test_dragonfly_evidence_refuses_unsafe_or_incomplete_runs` |

Record typed signal names in observability.md; no product analytics/funnel changes.
## Test Specification (tiered)

| Dimension | Tier | Test | Asserts |
|---|---|---|---|
| 0.1 | integration | `test_sharded_tail_prototype_recovers` | Wrong-node SSUBSCRIBE/SPUBLISH return MOVED; acknowledgments arrive; hub handles SUnsubscribe on a healthy socket, resubscribes wanted channels on the owner, coalesces pushes and never resurrects a final-drop channel; late own acknowledgments, lag and isolation pass; reconnect after hub repair produces no duplicate forwarding of uniquely published test frames. |
| 0.2 | integration | `test_cluster_primitives_prototype_survives_movement` | Bootstrap/restart restores slots and snapshot stream/group/PEL/consumer state; CROSSSLOT control fails, retained scripts survive redirects/cache loss, and Dedicated cancellation preserves responsiveness. |
| 0.3 | integration | `test_durable_identity_prototype_survives_replay` | Commit/append crashes, imported IDs, clock rollback, late replay, and 9/10/100 numeric sequence pagination preserve identity/order without missing or repeated page entries. |
| 0.4 | integration | `test_sharded_coordination_prototype_bounds_hotspots` | Hot fleets, skewed tags, slow destinations, and partition-owner loss cannot starve unrelated eligible work. |
| 1.1 | unit | `test_incomparable_datastore_runs_are_rejected` | Tampering, missing samples, parameter/identity drift, and production/shared dependency or feature changes fail, even when edited files are bench-only. |
| 1.2 | unit / integration | `test_shared_deployment_refuses_saturation_profile` | Unsafe seed/discovered addresses open no destructive connection; fixtures cannot lease or acknowledge non-fixture work. |
| 1.3 | integration | `test_redis_baseline_records_complete_evidence` | Three raw outputs per existing lane, digests, B/B0 source/schema/build/dependency equality including Cargo.lock, and cleanup are verified. |
| 2.1 | integration | `test_acceptance_recovers_at_each_crash_boundary` | No runnable entry before durable commit; committed accepted work replays after every injected stop. |
| 2.2 | integration | `test_queue_loss_replays_without_duplicate_settlement` | Admission creates accepted, not received; duplicates/races and charge-boundary crashes yield one receive ledger row even at zero cost, no pre-charge-refusal row, preserved approval/renewal policy, separate billing/budget enforcement across fleets/tenants with equal IDs after deletion, legacy null preservation, non-null new writers, role-enforced identity immutability plus SELECT on all six accumulators and successful real DO UPDATE reads/writes, whole-statement wallet rollback, tenant cascade and once-only counters/settlement. |
| 2.3 | integration | `test_durable_store_failure_refuses_acceptance` | Absent/incomplete/wrong-target/unreadable import receipt and failed commit yield retryable refusal, readiness false, no ingress success, lease, background dispatch or device/connect auth; daemon cannot self-complete receipt. |
| 3.1 | integration | `test_retention_preserves_pending_work_under_pressure` | Pending-aware cleanup and missing-group recovery lose no accepted work under retention pressure. |
| 3.2 | integration | `test_full_datastore_applies_explicit_backpressure` | No silent drops; bounded durable admission and retry classes remain correct. |
| 3.3 | integration | `test_capacity_report_accounts_for_all_retained_state` | Population, pending/completed work, replay state, buffers, replicas, and PostgreSQL bytes are accounted separately. |
| 4.1 | integration | `test_ready_races_preserve_work_and_bound_poll_cost` | Token races, candidate caps and durable repair pass; partition-map assertions alone may be N/A with archived existing-layout budget proof. |
| 4.2 | integration | `test_skewed_workload_preserves_discovery_and_delivery_fairness` | Per-partition and eligible-work queue age stay within frozen budgets; fixture delivery is labeled synthetic. |
| 4.3 | integration | `test_coordination_recovers_during_partition_movement` | Destination order, fencing, worker-loss recovery and resource budgets pass; extra map-movement assertions alone may be N/A with measured existing-layout proof. |
| 5.1 | integration | `test_datastore_preflight_refuses_invalid_configuration` | Local probes reject wrong engine/topology/trust/auth/eviction, missing CLUSTER permission and replica routing; discovery and sharded pub/sub succeed on primaries. Fly connectivity belongs to §6.3/6.4. |
| 5.2 | integration | `test_cluster_resharding_preserves_outbox_identity` | Lost replies/redirects may duplicate physical receipts; ordered durable admission, lease fencing, and settlement preserve logical work without Redis ingress claims. |
| 5.3 | integration | `test_cluster_connections_recover_without_missing_scoped_state` | Routed cross-seed frames resume; scoped scans reconcile; one subscribing client object and bounded node socket counts survive churn; repair never creates another subscriber client. |
| 5.4 | integration | `test_durable_auth_survives_queue_failure` | Guarded approve/verify/abort/cancel-all and every nonce family preserve expiry, lockout, tenant binding and the permitted same-fingerprint retry, including a concurrent verify loser re-reading the winner's committed row; crashes around commit, concurrent consumers, DB failure and lost replies release no unauthorized ciphertext/exchange. Durable state survives queue failover/restore; cleanup is bounded and cannot reset state. |
| 6.1 | e2e | `test_dragonfly_preserves_application_outcomes` | Login, approvals, steer, lease, report, fleet/workspace streaming, and reconnect backfill pass on both Dragonfly environments. |
| 6.2 | integration | `test_dragonfly_preserves_failure_and_session_semantics` | Single-use/expiry, permitted response retry, retained-script reload, lost-response and isolation tests pass. PostgreSQL consumed/aborted/spent state survives queue failover and pre-consume snapshot restore; legacy Redis auth keys never re-import. Scoped purge stays fenced through interruption; approvals reconcile. PostgreSQL rollback recovery invalidates restored auth state before reopen. |
| 6.3 | unit / integration | `test_dragonfly_evidence_refuses_unsafe_or_incomplete_runs` | Wrong/stale Cloud identity, auth/API failure, missing/null/false cluster.enabled, endpoint mismatch, unreachable advertised/newly promoted Fly primary, bad CLUSTER/channel ACLs, insufficient replicas, missing backup policy or missing PlanetScale posture evidence or unresolved platform-risk disposition, leaked response secrets, relabeled runs, absent CI origin, tampering, and fixture leaks fail. |
| 6.4 | e2e | `test_dragonfly_sustained_workload_has_complete_evidence` | Both datastore budgets pass: per-fleet offered/accepted/completed rates, fan-out amplification, commits, auth transition rate/p95/p99/expiry cleanup, pool/allocator wait, WAL/I/O, backlog/drain, cost, frame/byte rates, fan-out, lag/closure rate, batch-size/RTT/runner-request latency, matched pre-fault Dragonfly references and fixed fault/recovery windows, managed failover/resize/resharding, advertised-primary reachability, restore/drain and generator headroom. |
| 7.1 | integration | `test_cutover_recovery_preserves_source_state` | Fresh/populated upgrades and interrupted reversal converge; reverse DDL precedes exact migration-ledger removal and old-build migrate exits zero before resume. Auth counts only: no import, old flows reject/new flows pass, issued credentials/grants survive; work/claims/mirrors/billing reconcile; unknown keys, detected billing damage without disposition and overlapping writers fail; fence-window provider redelivery preserves deduplication. |
| 7.2 | unit | `test_deploy_preflight_requires_completed_import` | Self-tested shell preflight called by the workflow, with stubbed commands: missing receipt, wrong destination/build, or unfenced source produces zero secret/image mutations; completed inputs select the expected secret/image and pinned running/total Machine counts, including zero-Machine deploy. No live-deploy claim. |

## Acceptance Rubric (single scoring surface)

| # | Criterion | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|---|---|---|---|---|
| R1 | Prototype and historical preparation | `make bench-datastore CHECK=prototype` | Exit 0; all prototype proofs plus twelve immutable historical samples and valid B/B0 comparison. | P0 | |
| R2 | Durable admission and replay | `make bench-datastore CHECK=durability` | Exit 0; lost work, missed eligible receive debits, duplicate settlement/debits = 0; pre-charge refusals charged = 0. | P0 | |
| R3 | Bounded retention and fair sharding | `make bench-datastore CHECK=coordination` | Exit 0; retention, memory, partition progress, and eligible-work budgets pass. | P0 | |
| R4 | Application and cluster behavior | `make bench-datastore CHECK=cluster` | Exit 0; local and Swarm application, auth, stream, and topology proofs present. | P0 | |
| R5 | Sustained Cloud readiness | `make bench-datastore CHECK=readiness` | Exit 0; authenticated Cloud faults/load evidence and all frozen budgets pass. | P0 | |
| R6 | Migration rehearsal and safe landing | `make bench-datastore CHECK=rehearsal` | Exit 0; all source prefixes reconcile; workflow dry-run and closed-until-import admission proofs pass; no live-deploy claim. | P0 | |
| S1 | Conformance | `make harness-verify` | Exit 0. | P0 | |
| S2 | Unit verification | `make test-unit-all` | Exit 0; coverage gates pass. | P0 | |
| S3 | Integration verification | `make test-integration-rustd` | Exit 0; nonzero passing count. | P0 | |
| S4 | Lint | `make lint-all` | Exit 0. | P0 | |
| S5 | Version | `make check-version` | Exit 0. | P0 | |
| S6 | Secrets | `gitleaks git --no-banner` | Exit 0; no leaks. | P0 | |

Authoring checks fill no Graded cell; prototype/rehearsal success is not live migration.

## Dead Code Sweep
Inventory with `git grep -n -w -i redis`; preserve protocol/history. Sweep append_once/forget_once, runtime Redis session/nonce callers, comments, counters, old pub/sub and provider proposals; preserve necessary source decoders and standalone client for tools/fixtures; remove auth import/export code. M192_002 retires source resources.

## Out of Scope
Exclude permanent Redis support, single-shard rollout, another daemon, paid model traffic, and exactly-once external effects.
M192_002 owns live retirement; adding the absent outbound product feature awaits an explicit scope decision.

## Product Clarity (authoring record)

1. **Successful user moment:** fleet workflows and live tails work on the tested sharded design.
2. **Preserved user behaviour:** event IDs, approvals, fencing, ordering, billing and established credentials/grants retain meaning; in-flight login/connect flows restart at cutover.
3. **Optimal-way check:** use target-supported primitives and prototype failure boundaries before integration.
4. **Rebuild-vs-iterate:** refactor connection ownership; let measured budgets decide further coordination partitioning.
5. **What we build:** prototypes, durable identity, sharded coordination, Cloud evidence, and safe migration tooling.
6. **What we do NOT build:** another daemon, generic cache framework, permanent Redis provider, or unapproved outbound feature.
7. **Fit with existing features:** reuse M188, real application services, and the one integration lane.
8. **Surface order:** operator evidence and prototypes precede production integration.
9. **Dashboard restraint:** no readiness badge without measured proof.
10. **Confused-user next step:** the grader names the failing risk, missing proof, or unmet budget.

## Decomposition & alternatives (patch vs refactor)

Swarm routing and sharded pub/sub are required; readiness/queue partitioning is adopted when prototypes demonstrate a need and a passing improvement.
Readiness is local/Cloud proof; Indy coordinates the first merge/deploy, and the live spec records the switch/retirement. No temporary runtime provider framework.
Explicit physical-ID replay is rejected because out-of-order replay into an existing stream can fail; durable logical identity retains public ID shape.

## Discovery (consult log)

- **Consults:** Indy approved Swarm-only migration and said "i am ok to revamp to move to dragonflydb's recommended approach".
- **Refactor direction:** Indy asked "You shouldnt be shy enough to do a refactor and do it in a better sharded way as well?"; Swarm correctness is required; additional application partitioning needs measured justification.
- **Indy override (verbatim):** "we just stick to local that runs containers today (with the cluster config, no single mode crap for dragonfly)". Interpretation: cluster-only new daemon; no temporary provider mode.
- **Indy deployment direction (verbatim):** "in production this would be stood up by Indy on dragondb just like indy did for upstash and stick the key in deployment to deploy-dev.yml". Reuse its called Fly workflow and vault flow.
- **Final review:** Indy confirms acceptance of about 5 ms Fly iad → Dragonfly Cloud AWS us-east-1 in this handoff; this is his planning decision, not measured p99. Choose PostgreSQL auth authority: WAIT cannot guarantee single use. Hub duplicate-frame, ledger SELECT and populated-migration checks are required; all runtime proofs remain NOT RUN.
- **Override boundary:** supersedes temporary standalone deployability/later-removal requirements; does not waive import/billing proofs, numeric budgets, paid-capacity consent, or live action approval. No benchmark/prototype has run.
