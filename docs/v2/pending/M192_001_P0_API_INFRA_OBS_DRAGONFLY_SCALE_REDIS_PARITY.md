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
**Solution summary:** prototype first, then refactor around sharding, durable identity, bounded resources, and verifiable results.

Swarm is the retirement target; M192_001 keeps standalone Redis deployable by explicit configuration. Proposed M192_002 removes that mode after live observation.

## PR Intent & comprehension handshake

- **PR title:** feat: prepare sharded Dragonfly migration with fault-tested recovery
- **Intent:** preserve fleet behavior while proving migration readiness; never label readiness as completed live migration.
- **Handshake:** accept the approved sharded redesign, prototype risk boundaries, then integrate the proven designs.
- **ASSUMPTIONS I'M MAKING:** incorporate the requested Fable corrections; fixture-only outbound and the delivery split remain proposals. No live action is approved.

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
| `rustd/crates/{afd_sse,afd_api,afd_observability,agentsfleetd}/{Cargo.toml,src/**/*.rs,tests/**/*.rs}` | EDIT / CREATE | Fan-in parsing, startup probes, telemetry, application transport proofs. |
| `schema/{800_fleet_events,880_fleet_activity_counters,890_fleet_activity_counter_triggers}.sql`, `schema/*queue*.sql`, `rustd/crates/afd_db/{src/migration.rs,tests/*.rs}` | EDIT / CREATE | Admission/dispatch identity, state transitions, counters, and populated upgrade proof. |
| `make/{test-infra,test-integration-rustd,acceptance}.mk`, `docker-compose.yml`, `.github/workflows/{bench,test-integration-rustd,deploy-dev,deploy-dev-fly,release}.yml` | EDIT after required approval | One integration lane, isolated cluster, provenance, safe landing. |
| `playbooks/operations/datastore_scaling/{001_playbook.md,*.sh}`, `playbooks/README.md` | CREATE / EDIT | Bounded source migration tool, rehearsal, and cutover procedure. |
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
Candidate pins and image-digest requirements live in the canonical architecture; verify support in §0.

## Sections (implementation slices)

Execution: §1 → approved local cluster setup → §0 → §5 → §2 → §3 → §4 → §6 → §7. §1 needs no CI/compose edit.
### §0: Prototype Dragonfly primitives and sharded coordination

Dependencies: §1 and explicit approval for isolated cluster compose edits. Test-only prototypes precede the production refactor.
Use the candidate versions and fault procedures in `datastore_scaling.md`; resolve the image digest before running any prototype.
Use two distinct slot owners and replicas; measure the canonical fault matrix, resource bounds, lag, fairness, and recovery; retain integration proofs.
Compare per-fleet streams with bounded partitioned-stream layouts if population cost misses budget; public ordering and fencing govern the choice.
Record results in the evidence index. Failed or missing prototype evidence blocks the corresponding production integration and Cloud trial.

- **Dimension 0.1**: sharded pub/sub routes from another seed and recovers without unbounded buffers → Test `test_sharded_tail_prototype_recovers`.
- **Dimension 0.2**: retained single-key scripts and dedicated readers survive topology movement → Test `test_cluster_primitives_prototype_survives_movement`.
- **Dimension 0.3**: durable identity survives queue loss and reordered physical receipts → Test `test_durable_identity_prototype_survives_replay`.
- **Dimension 0.4**: sharded readiness and destination scheduling preserve bounded fair progress → Test `test_sharded_coordination_prototype_bounds_hotspots`.

### §1: Safe historical baseline and trustworthy capture

Dependencies: none; address hardening and capture come first. No production source/schema change before B; bench-only plumbing is allowed.
Prove B/B0 production source/schema/build and dependency-closure equality, including Cargo.lock; allow only proven bench-exclusive deltas. Capture three samples per existing `make bench-steer`, `bench-lease`, `bench-outbound`, and `bench-cardinality`.
Archive each fixed-path result immediately under a unique campaign/lane/sample path.
A collector writes sidecars containing B, parameters, payload bytes, window, resources, raw server/topology output, and SHA-256 digests.
Historical drivers have no seed or offered-rate guarantee; preserve their actual measurements and mark unavailable fields explicitly.
Fix the M188 rig-label/address gap before capture or remote use: verify both datastores and every discovered node, reject shared targets, scope leases and consumers.
Test cancellation cleanup and orphan recovery. Missing provenance, changed bytes, inconsistent topology, or overwritten samples must fail the grader.

- **Dimension 1.1**: tampered or incomparable evidence fails closed → Test `test_incomparable_datastore_runs_are_rejected`.
- **Dimension 1.2**: rig labels and credentials cannot authorize a shared or remote target → Test `test_shared_deployment_refuses_saturation_profile`.
- **Dimension 1.3**: capture retains twelve distinct historical samples and valid sidecars → Test `test_redis_baseline_records_complete_evidence`.

### §2: Durable admission and stable logical identity

Dependencies: §0.3, §1, and §5. Commit the PostgreSQL acceptance row and logical event ID before exposing runnable work or returning success.
Use the canonical PostgreSQL producer-identity/expiry authority and ordered outbox; remove admission Lua claims across all producers. Preserve numeric IDs and separate physical XACK receipts.
Apply the canonical accepted → received → terminal state machine: receive state/debit/ledger commit atomically after existing pre-charge gates. Guard first-attempt counters separately; preserve post-charge approvals. Insert conflicts never decide billing.
Cover steer, webhooks, App fan-out, cron, continuation, and repair verification; migrate expiry/no-expiry claims and cleanups. Update ingress comments and history/counters; install creates groups.
The pinned report path has no outbound enqueue caller. This draft preserves that behavior; fixture-only outbound tests cannot claim delivered connector answers.

- **Dimension 2.1**: each admission crash boundary retains accepted work → Test `test_acceptance_recovers_at_each_crash_boundary`.
- **Dimension 2.2**: duplicate receipts produce exactly one eligible receive debit and no repeated terminal settlement → Test `test_queue_loss_replays_without_duplicate_settlement`.
- **Dimension 2.3**: database failure refuses acceptance without false success → Test `test_durable_store_failure_refuses_acceptance`.

### §3: Retention, memory limits, and backpressure

Dependencies: §2. Use No Eviction and bounded per-fleet/deployment durable admission.
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
If partitioning is required, use fenced ownership, versioned routing, and replay for map changes; otherwise retain the measured passing design.
For fixture outbound, partition by destination hash, serialize within each destination, and bound concurrent destinations and retries across workers.
Prove recovery and fairness with skewed runner eligibility; aggregate throughput alone cannot pass a starvation check.

- **Dimension 4.1**: concurrent ingress and stale clears preserve partitioned readiness → Test `test_ready_races_preserve_work_and_bound_poll_cost`.
- **Dimension 4.2**: skewed eligibility and slow fixture destinations cannot starve unrelated work → Test `test_skewed_workload_preserves_discovery_and_delivery_fairness`.
- **Dimension 4.3**: partition movement and worker loss preserve ordered recovery → Test `test_coordination_recovers_during_partition_movement`.

### §5: Integrate the proven Dragonfly topology

Dependencies: §0 and §1. Keep the standalone transport deployable; cluster mode uses redis-rs routing and primary reads. Configuration selects one mode without fallback.
Cluster mode uses SPUBLISH/SSUBSCRIBE/SUNSUBSCRIBE with RESP3 and bounded owner-node resources; standalone preserves PUBLISH and ordinary subscriptions.
Keep stream key S and mode-specific activity channels from the canonical architecture; update afd_sse parsing and workspace fan-in together.
Single-key XADD publishes PostgreSQL admissions; no target Redis ingress claims. Probe retained scripts and use a test-only CROSSSLOT negative control.
Extend existing Dedicated ownership with node routing/redirects; standalone already isolates blocking reads. Scan all primaries with topology reconciliation.
Capability/auth errors fail readiness; mode-specific pub/sub probes must pass. Cluster failure never selects standalone.
Local tests keep prefix isolation and database 0; owned-cluster reset covers all primaries and verifies replicas before tests. Never flush a Cloud/shared target.

- **Dimension 5.1**: unsafe topology, TLS, auth, eviction, or pub/sub capability refuses boot → Test `test_datastore_preflight_refuses_invalid_configuration`.
- **Dimension 5.2**: resharding preserves outbox logical order despite ambiguous appends → Test `test_cluster_resharding_preserves_outbox_identity`.
- **Dimension 5.3**: subscriptions, scans, and dedicated readers recover across nodes → Test `test_cluster_connections_recover_without_missing_scoped_state`.

### §6: Application, Cloud, and sustained-load readiness

Dependencies: §2 through §5. Reuse real application fixtures on local multi-shard Dragonfly and dedicated Cloud Swarm.
Run `make test-integration-rustd`, `make acceptance-e2e`, and `make cli-acceptance`; use synthetic execution providers and no real outbound messages.
Extend M188 with combined acceptance-to-completion load: 1,000,000 active fleet records, 1,000 concurrent runs, 1,000 real runner processes, 16,667 offered events/second.
Use 1,000-byte and 16,000-byte/skew cases; freeze PostgreSQL and Dragonfly budgets together: commits, admission p99, allocator/pool waits, WAL/I/O, resources/cost, backlog/drain. Count fan-out admissions separately.
Reject generator saturation or growing unaccounted backlog. Comparable Dragonfly revisions require three samples, p99 <=1.10x baseline, completion >=0.95x, and no correctness failures.
Before Cloud load, prove the canonical authenticated GET datastore-identity mechanism; archive only allowlisted fields. Grade TLS, failover, restore, resize, resharding, and CI provenance.

- **Dimension 6.1**: real application and streaming paths preserve documented behavior → Test `test_dragonfly_preserves_application_outcomes`.
- **Dimension 6.2**: failure/session semantics survive races and script loss → Test `test_dragonfly_preserves_failure_and_session_semantics`.
- **Dimension 6.3**: missing Cloud proof or fabricated provenance refuses readiness → Test `test_dragonfly_evidence_refuses_unsafe_or_incomplete_runs`.
- **Dimension 6.4**: sustained workload and managed faults meet frozen budgets → Test `test_dragonfly_sustained_workload_has_complete_evidence`.

### §7: Migration tooling and deployment-safe rehearsal

Dependencies: §6. The bounded offline tool imports old source state; the readiness daemon retains explicit standalone transport until M192_002 retirement.
Rehearse both the initial durable-admission upgrade on populated standalone Redis and later Swarm cutover using the canonical source key-prefix table, fenced writers, and expiry preservation.
Handle events accepted before durable admission existed. Inventory historical outbound jobs and block on unsupported nonempty state; never assume the queue is empty.
Every prefix has counts/types, authority, and drain/import/rebuild proof; unmatched keys block. Include no-expiry claims, gate mirrors, nonces, anomaly windows, leases, and billing reconciliation.
Before merge, prove approved schema/data upgrade steps and standalone deployability, including a later unrelated build with unchanged Upstash config; no indefinite main-wide hold.
This proposed readiness PR completes M192_001 only. M192_002 retains live approval, switch, observation, and retirement as P0; no parked-spec exception is assumed.

- **Dimension 7.1**: rehearsal preserves old state through import, abort, and recovery → Test `test_cutover_recovery_preserves_source_state`.
- **Dimension 7.2**: readiness and later builds remain deployable on standalone without implicit cutover → Test `test_readiness_build_preserves_standalone_deployability`.

## Interfaces

Keep REDIS_URL_API and numeric event_id. REDIS_MODE defaults to standalone; explicit cluster enables Swarm. Reject unknown values and automatic fallback; M192_002 retires standalone.
Add `make bench-datastore CHECK=prototype|baseline|durability|retention|coordination|cluster|readiness|rehearsal|rollout` as the spec-consumed evidence grader.
The target is planned, grades evidence only, and preserves M188's non-failing comparator; `rollout` requires M192_002 manual live proof.

## Failure Modes

| Mode | Handling and negative proof |
|---|---|
| Unsupported standard pub/sub, lost subscriptions | Fail readiness; sharded routing, churn, and bounded-lag proof in `test_sharded_tail_prototype_recovers`. |
| CROSSSLOT, MOVED, blocked shared socket | Single-key runtime scripts, negative controls, and existing Dedicated ownership; `test_cluster_primitives_prototype_survives_movement`. |
| Queue loss, ambiguous writes, identity collision | Durable admission and stable identity; `test_durable_identity_prototype_survives_replay`. |
| Premature trim, missing groups, full datastore | Replay-aware cleanup and admission; `test_retention_preserves_pending_work_under_pressure`. |
| Hot key, sparse eligible runners, stalled destination | Sharded bounded scheduling; `test_sharded_coordination_prototype_bounds_hotspots`. |
| Shared rig, unsafe discovered node, cleanup crash | Refuse mutation and recover fixtures; `test_shared_deployment_refuses_saturation_profile`. |
| Forged summary, missing Cloud run, saturated generator | Fail grading; `test_dragonfly_evidence_refuses_unsafe_or_incomplete_runs`. |
| Lost claim TTL, source writers after switch, unsafe rollback | Reconcile and fence migration; `test_cutover_recovery_preserves_source_state`. |
| Merge breaks standalone or silently selects Swarm | Require deployability and explicit-mode proofs; `test_readiness_build_preserves_standalone_deployability`. |

## Invariants

1. Runnable work and success follow durable acceptance; transactions and crash injection enforce ordering.
2. Logical identity survives receipts and queue loss; database uniqueness and fencing prevent duplicate settlement and billing.
3. Sharded resources are bounded; admission, token checks, per-partition budgets, and cleanup tests enforce limits.
4. Shared deployments never receive destructive fixtures; address, ownership, and discovered-node validation precede mutation.
5. Evidence labels cannot establish provenance; the grader verifies raw digests, server identity, authenticated runs, and manual live approval separately.
6. An incomplete risk proof blocks its dependent integration; the prototype evidence index is a required grader input.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|---|---|---|---|---|---|
| Subscription recovery and lag | ops | Movement, reconnect, queue pressure | Node role, outcome, bytes, duration | No channel/fleet labels | `test_sharded_tail_prototype_recovers` |
| Admission, replay, and settlement | ops | Commit, replay, refusal | Outcome, count, duration | No payload or credentials | `test_queue_loss_replays_without_duplicate_settlement` |
| Partition fairness and memory | ops | Poll, cleanup, sample | Bounded partition index, queue age, bytes | No tenant labels | `test_sharded_coordination_prototype_bounds_hotspots` |
| Evidence verdict | ops | Grading | Revision, digest, environment class | Redacted addresses | `test_dragonfly_evidence_refuses_unsafe_or_incomplete_runs` |

Record typed signal names in observability.md; no product analytics/funnel changes.
## Test Specification (tiered)

| Dimension | Tier | Test | Asserts |
|---|---|---|---|
| 0.1 | integration | `test_sharded_tail_prototype_recovers` | Wrong-node routing, subscription acknowledgment, movement, lag, unsubscribe races, and tenant isolation pass. |
| 0.2 | integration | `test_cluster_primitives_prototype_survives_movement` | Cross-slot negative control fails; actual single-key script invariants survive redirects/cache loss; Dedicated cancellation preserves command responsiveness. |
| 0.3 | integration | `test_durable_identity_prototype_survives_replay` | Crash before/after commit or append, high imported IDs, clock rollback, and late replay preserve identity and single settlement. |
| 0.4 | integration | `test_sharded_coordination_prototype_bounds_hotspots` | Hot fleets, skewed tags, slow destinations, and partition-owner loss cannot starve unrelated eligible work. |
| 1.1 | unit | `test_incomparable_datastore_runs_are_rejected` | Tampering, missing samples, parameter/identity drift, and production/shared dependency or feature changes fail, even when edited files are bench-only. |
| 1.2 | unit / integration | `test_shared_deployment_refuses_saturation_profile` | Unsafe seed/discovered addresses open no destructive connection; fixtures cannot lease or acknowledge non-fixture work. |
| 1.3 | integration | `test_redis_baseline_records_complete_evidence` | Three raw outputs per existing lane, digests, B/B0 source/schema/build/dependency equality including Cargo.lock, and cleanup are verified. |
| 2.1 | integration | `test_acceptance_recovers_at_each_crash_boundary` | No runnable entry before durable commit; committed accepted work replays after every injected stop. |
| 2.2 | integration | `test_queue_loss_replays_without_duplicate_settlement` | Admission creates accepted, not received; duplicates/races and charge-boundary crashes yield exactly one policy-appropriate receive debit, zero for pre-charge refusal, preserved post-charge approval, and one terminal settlement; first-attempt counters/history remain correct. |
| 2.3 | integration | `test_durable_store_failure_refuses_acceptance` | Failed durable commit produces classified retryable refusal and no runnable queue entry. |
| 3.1 | integration | `test_retention_preserves_pending_work_under_pressure` | Pending-aware cleanup and missing-group recovery lose no accepted work under retention pressure. |
| 3.2 | integration | `test_full_datastore_applies_explicit_backpressure` | No silent drops; bounded durable admission and retry classes remain correct. |
| 3.3 | integration | `test_capacity_report_accounts_for_all_retained_state` | Population, pending/completed work, replay state, buffers, replicas, and PostgreSQL bytes are accounted separately. |
| 4.1 | integration | `test_ready_races_preserve_work_and_bound_poll_cost` | Token races, partition-map changes, candidate caps, and durable readiness repair hold. |
| 4.2 | integration | `test_skewed_workload_preserves_discovery_and_delivery_fairness` | Per-partition and eligible-work queue age stay within frozen budgets; fixture delivery is labeled synthetic. |
| 4.3 | integration | `test_coordination_recovers_during_partition_movement` | Destination ordering, fencing, task/socket limits, and recovery budgets hold during ownership changes. |
| 5.1 | integration | `test_datastore_preflight_refuses_invalid_configuration` | Wrong engine/topology, trust, credentials, eviction, and unsupported commands cannot produce ready service. |
| 5.2 | integration | `test_cluster_resharding_preserves_outbox_identity` | Lost replies/redirects may duplicate physical receipts; ordered durable admission, lease fencing, and settlement preserve logical work without Redis ingress claims. |
| 5.3 | integration | `test_cluster_connections_recover_without_missing_scoped_state` | Routed cross-seed frames resume; scoped scans reconcile; bounded subscription/socket counts survive churn. |
| 6.1 | e2e | `test_dragonfly_preserves_application_outcomes` | Login, approvals, steer, lease, report, fleet/workspace streaming, and reconnect backfill pass on both Dragonfly environments. |
| 6.2 | integration | `test_dragonfly_preserves_failure_and_session_semantics` | One-time login, expiry, Lua reload, lost responses, and tenant isolation remain correct. |
| 6.3 | unit / integration | `test_dragonfly_evidence_refuses_unsafe_or_incomplete_runs` | Wrong/stale Cloud identity, auth/API failure, missing fields, endpoint mismatch, leaked response secrets, relabeled runs, absent CI origin, tampering, and fixture leaks fail. |
| 6.4 | e2e | `test_dragonfly_sustained_workload_has_complete_evidence` | Both datastore budgets pass: per-fleet offered/accepted/completed rates, fan-out amplification, commits, pool/allocator wait, WAL/I/O, backlog/drain, cost, and generator headroom. |
| 7.1 | integration | `test_cutover_recovery_preserves_source_state` | Every canonical prefix, no-expiry/tombstone claims, nonces, anomaly/gate windows, IDs, and billing reconcile at standalone upgrade and Swarm cutover; unknown keys and overlapping writers fail. |
| 7.2 | unit / integration | `test_readiness_build_preserves_standalone_deployability` | Populated schema/data upgrade, standalone default/PUBLISH, explicit cluster, no fallback, and later unrelated deploy pass; no indefinite hold or mixed admission writers. |

## Acceptance Rubric (single scoring surface)

| # | Criterion | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|---|---|---|---|---|
| R1 | Prototype and historical preparation | `make bench-datastore CHECK=prototype` | Exit 0; all prototype proofs plus twelve immutable historical samples and valid B/B0 comparison. | P0 | |
| R2 | Durable admission and replay | `make bench-datastore CHECK=durability` | Exit 0; lost work, missed eligible receive debits, duplicate settlement/debits = 0; pre-charge refusals charged = 0. | P0 | |
| R3 | Bounded retention and fair sharding | `make bench-datastore CHECK=coordination` | Exit 0; retention, memory, partition progress, and eligible-work budgets pass. | P0 | |
| R4 | Application and cluster behavior | `make bench-datastore CHECK=cluster` | Exit 0; local and Swarm application, auth, stream, and topology proofs present. | P0 | |
| R5 | Sustained Cloud readiness | `make bench-datastore CHECK=readiness` | Exit 0; authenticated Cloud faults/load evidence and all frozen budgets pass. | P0 | |
| R6 | Migration rehearsal and safe landing | `make bench-datastore CHECK=rehearsal` | Exit 0; all source prefixes reconcile; readiness and subsequent builds deploy on standalone without implicit cutover. | P0 | |
| S1 | Conformance | `make harness-verify` | Exit 0. | P0 | |
| S2 | Unit verification | `make test-unit-all` | Exit 0; coverage gates pass. | P0 | |
| S3 | Integration verification | `make test-integration-rustd` | Exit 0; nonzero passing count. | P0 | |
| S4 | Lint | `make lint-all` | Exit 0. | P0 | |
| S5 | Version | `make check-version` | Exit 0. | P0 | |
| S6 | Secrets | `gitleaks git --no-banner` | Exit 0; no leaks. | P0 | |

Authoring checks fill no Graded cell; prototype/rehearsal success is not live migration.

## Dead Code Sweep

Inventory with `git grep -n -w -i redis`; preserve protocol/history. Sweep append_once/forget_once callers, ingress comments, counters, and mode-specific pub/sub; M192_002 retires standalone.

## Out of Scope

Exclude permanent Redis support, single-shard rollout, another daemon, paid model traffic, and exactly-once external effects.
M192_002 owns live retirement; adding the absent outbound product feature awaits an explicit scope decision.

## Product Clarity (authoring record)

1. **Successful user moment:** fleet workflows and live tails work on the tested sharded design.
2. **Preserved user behaviour:** auth, event IDs, approvals, fencing, ordering, and billing retain documented meaning.
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
Two linked readiness/live specs are proposed to avoid an implementation PR claiming an unperformed deployment or requiring a lifecycle exception.
Explicit physical-ID replay is rejected because out-of-order replay into an existing stream can fail; durable logical identity retains public ID shape.

## Discovery (consult log)

- **Consults:** Indy approved Swarm-only migration and said "i am ok to revamp to move to dragonflydb's recommended approach".
- **Refactor direction:** Indy asked "You shouldnt be shy enough to do a refactor and do it in a better sharded way as well?"; Swarm correctness is required; additional application partitioning needs measured justification.
- **Re-review correction:** requested Fable revisions replace the global deploy hold and Redis admission claims with explicit transport selection and PostgreSQL authority; fixture-only outbound/split remain proposals.
- **Metrics review:** freeze PostgreSQL admission and Dragonfly budgets together; source-verified Cloud GET needs a successful authenticated probe before Cloud load.
- **Skill-chain outcomes:** orly-spec-new incorporated Fable findings; structural authoring checks cannot establish runtime readiness.
- **Deferrals:** no unilateral runtime deferral; the proposed M192_002 owns the complete live outcome as P0.
