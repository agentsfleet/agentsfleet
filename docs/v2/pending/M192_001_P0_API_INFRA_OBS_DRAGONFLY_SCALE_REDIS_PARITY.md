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

# M192_001: Migrate to Dragonfly Swarm and retire Redis

**Prototype:** v2.0.0
**Milestone:** M192
**Workstream:** 001
**Date:** Sep 11, 2026
**Status:** PENDING
**Priority:** P0, required before claiming Dragonfly readiness or million-fleet capacity.
**Categories:** API, INFRA, OBS
**Batch:** B2, consumes the completed M188_001 benchmark drivers.
**Branch:** docs/m192-dragonfly-migration, documentation only; implementation has not started.
**Test Baseline:** pending; CHORE(open) pins the comparison revision; canonical counts are recorded before the implementation Pull Request.
**Depends on:** M188_001 for measurement drivers and result files. Streaming acceptance uses the merged runtime; the parked SSE follow-up is not a prerequisite.
**Provenance:** LLM-drafted (Codex, Sep 06, 2026), from the user's Dragonfly and same-deployment requirements.
**Canonical architecture:** `docs/architecture/datastore_scaling.md`; existing flows in `data_flow.md` and `runner_fleet.md`.
## Overview

**Goal (testable):** Dragonfly Cloud Swarm preserves application behavior and accepted work, meets frozen capacity budgets, and supports verified Redis retirement.
**Problem:** endpoint replacement alone cannot prove queue recovery, bounded memory, fair discovery, or cluster correctness.
**Solution summary:** measure first, then improve durability, retention, coordination, and cluster support through the existing operation-specific Rust boundaries.

Dragonfly Cloud Swarm is the sole deployment target, with no single-shard migration stage. Keep redis-rs and Redis protocol compatibility. Capture one historical Redis baseline on an isolated rig.

Permanent Redis support and repeated same-deployment Redis comparisons are removed by the approved scope change.
## PR Intent & comprehension handshake

- **PR title (eventual):** feat: migrate to Dragonfly Swarm and retire Redis
- **Intent:** preserve fleet behavior and accepted work while moving the deployment to tested Dragonfly capacity and recovery.
- **Handshake:** capture the existing implementation once, build and measure on Dragonfly, rehearse migration, and retire Redis after verified cutover.
- **ASSUMPTIONS I'M MAKING:** approval covers this revised direction; live cutover, trial activation, and billing changes require their concrete operational approval.
## Implementing agent — read these first

1. `docs/architecture/datastore_scaling.md`, the required destination, application guarantees, and retirement rules.
2. `docs/v2/done/M188_001_P1_API_INFRA_OUTBOUND_AND_LEASE_THROUGHPUT_BENCH.md`, completed drivers in `rustd/crates/afd_bench/`.
3. `rustd/crates/afd_ingress/src/deliver.rs`, provider deduplication and accepted ingress behavior.
4. `rustd/crates/afd_redis/src/streams/once.rs`, atomic multi-key append and replay windows.
5. https://www.dragonflydb.io/docs/cloud/datastores, managed topology, eviction, replicas, and connection requirements.
## Files Changed (blast radius)

The following table scopes implementation. Existing uncommitted branch work belongs to its current task and must not be staged by this workstream.

| File | Action | Why |
|------|--------|-----|
| `AGENTS.md`, `docs/architecture/README.md`, `docs/architecture/roadmap.md`, `docs/architecture/datastore_scaling.md` | EDIT | Record the approved migration and evidence rules. |
| `docs/v2/pending/M192_001_P0_API_INFRA_OBS_DRAGONFLY_SCALE_REDIS_PARITY.md` | CREATE / MOVE | Author here; lifecycle moves the same spec to active and done. |
| `docs/architecture/scaling.md`, `docs/architecture/data_flow.md`, `docs/architecture/runner_fleet.md`, `docs/architecture/concurrency.md` | EDIT | Update actual flows and measured limits with their implementation. |
| `docs/architecture/testing.md`, `docs/architecture/observability.md` | EDIT | Explain backend evidence and recovery signals. |
| `docs/v2/reviews/datastore-scale-evidence.md` | CREATE | Index immutable result files and their deployment revisions. |
| `make/bench.mk`, `rustd/crates/afd_bench/Cargo.toml`, `rustd/crates/afd_bench/src/**/*.rs` | EDIT / CREATE | Reuse M188 drivers; add cluster statistics, combined workload measurements, and strict grading. |
| `bench/profiles/datastore/*.json`, `bench/baselines/datastore/*.json` | CREATE | Frozen budgets, workload manifests, and immutable redacted summaries. |
| `rustd/crates/afd_bench/tests/*.rs`, `rustd/crates/afd_bench/tests/support/*.rs` | EDIT / CREATE | Driver safety, evidence grading, and combined-load proofs. |
| `make/test-infra.mk`, `make/test-integration-rustd.mk`, `docker-compose.yml` | EDIT | Select disposable backends without creating another integration lane. |
| `rustd/Cargo.toml`, `rustd/Cargo.lock`, `rustd/crates/afd_redis/Cargo.toml` | EDIT | Enable redis-rs cluster capabilities and test support. |
| `rustd/crates/afd_redis/src/*.rs`, `rustd/crates/afd_redis/src/streams/*.rs`, `rustd/crates/afd_redis/src/hub/*.rs` | EDIT | Connections, atomic keys, retention, readiness, scanning, and subscriptions. |
| `rustd/crates/afd_redis/tests/*.rs`, `rustd/crates/afd_redis/tests/support/*.rs` | EDIT / CREATE | Dragonfly behavior, pressure, topology, and recovery tests. |
| `rustd/crates/afd_ingress/src/*.rs`, `rustd/crates/afd_ingress/tests/*.rs`, `rustd/crates/afd_events/src/steer.rs`, `rustd/crates/afd_cron/src/fire.rs`, `rustd/crates/afd_gate/src/gate/park.rs`, `rustd/crates/afd_approval/src/inbox/*.rs` | EDIT / CREATE | Durable inbound acceptance and replay through the shared boundary. |
| `rustd/crates/afd_fleet_lifecycle/src/install.rs`, `rustd/crates/afd_fleet_lifecycle/src/purge.rs` | EDIT | Include lifecycle ingress and cleanup. |
| `rustd/crates/afd_fleet/src/lease/*.rs`, `rustd/crates/afd_fleet/src/lease/sql/*.rs` | EDIT | Preserve fencing, settlement, and acknowledgment behavior. |
| `rustd/crates/afd_wire/src/event.rs`, `rustd/crates/afd_wire/tests/*.rs`, `rustd/crates/afd_fleet/src/lease/report/**/*.rs`, `rustd/crates/afd_fleet/tests/*.rs` | EDIT / CREATE | Preserve public event fields while separating durable identity and atomic delivery handoff. |
| `rustd/crates/afd_outbound/src/*.rs`, `rustd/crates/afd_outbound/tests/*.rs` | EDIT / CREATE | Recoverable delivery, bounded concurrency, and destination fairness. |
| `rustd/crates/afd_runner/src/sweep/*.rs`, `rustd/crates/agentsfleetd/src/sweepers.rs`, `rustd/crates/agentsfleetd/src/outbound.rs` | EDIT | Replay supervision and recovery lifecycle. |
| `rustd/crates/agentsfleetd/src/preflight.rs`, `rustd/crates/agentsfleetd/src/preflight/config.rs`, `rustd/crates/agentsfleetd/src/serve/*.rs` | EDIT | Explicit backend topology and boot validation. |
| `schema/*queue*.sql`, `rustd/crates/afd_db/src/migration.rs`, `rustd/crates/afd_db/tests/*.rs` | CREATE / EDIT | Durable acceptance schema and upgrade proof; allocate the migration number at PLAN. |
| `rustd/crates/afd_sse/src/*.rs`, `rustd/crates/afd_api/tests/integration_*live.rs`, `rustd/crates/afd_api/tests/integration_fleet_streams.rs`, `rustd/crates/afd_api/tests/integration_datastore*.rs`, `rustd/crates/afd_api/tests/*suite.rs` | EDIT / CREATE | Prove real API and streaming behavior on local Dragonfly and Cloud Swarm. |
| `rustd/crates/afd_observability/src/producers/fleet*.rs`, `rustd/crates/afd_observability/src/producers/fleet/*.rs`, `rustd/crates/afd_observability/src/metrics/declared/fleet.rs` | EDIT | Bounded recovery, backlog, and scheduling signals. |
| `playbooks/operations/datastore-scaling.md` | CREATE | Historical baseline, Cloud preparation, migration, recovery, and retirement procedure. |
| `.github/workflows/*.yml`, `deploy/**`, `playbooks/founding/**`, `playbooks/README.md` | EDIT at approved rollout | Replace Redis server dependencies and document secret migration; expand exact paths at PLAN. |

Expand module globs into exact files at PLAN, including producer crate manifests and remaining Redis server callers. Preserve protocol/client names and historical records; inventory runtime server dependencies separately from test fixtures and documentation. Public behavior documentation needs its matching branch in the separate docs repository.

Deploy configuration and Continuous Integration (CI) workflow edits need separate authorization; use the existing deployment procedure.
## Applicable Rules

- `docs/greptile-learnings/RULES.md`: UFS, NDC, NLR, NLG, OWN, ECL, TIM, STR, TCF, CFG, ORP, VLT, NSQ, STS, and TST-NAM.
- `dispatch/write_rust.md` and `docs/RUST_ERROR_STANDARD.md`: preserve error sources and explicit resource ownership.
- `dispatch/write_sql.md`: follow existing schema and migration conventions; never weaken fencing or tenant boundaries.
- `dispatch/write_any.md` and `docs/LOGGING_STANDARD.md`: scoped cleanup, bounded source files, and redacted signals.
- `dispatch/name_architecture.md` and `docs/DOCUMENTATION_RULES.md`: update flow documentation with code and distinguish targets from observed behavior.
- `docs/AUTH.md`: preserve single-use sessions, replay windows, and rate limits across backends.
## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| SPEC TEMPLATE | Authoring | Complete required sections, no placeholders, canonical Make commands. |
| Architecture consult | Authoring and flow edits | Required destination recorded separately; update actual topology with implementation. |
| LENGTH, UFS, LOGGING, ERROR REGISTRY | Implementation | Split cohesive modules; named limits; typed errors; no credential output. |
| SCHEMA and AUTH | Implementation | Real upgrade and race tests; no automatic schema removal or weakened auth checks. |
| VERIFY | Implementation | Canonical repository commands plus the evidence matrix; no package-runner substitute. |

## Prior-Art / Reference Implementations

- M188_001 owns the four benchmark drivers. Reuse their fixtures and results; do not build a competing load generator.
- `rustd/crates/afd_redis/tests/integration_streams.rs` and `integration_session.rs` prove real-server replay and atomic state transitions.
- [redis-rs cluster support](https://docs.rs/redis/latest/redis/cluster_async/index.html) supplies topology routing; do not implement a cluster protocol.
## Sections (implementation slices)

Execution dependencies: §1 → §5 → §2 → §3 → §4 → §6 → §7. Existing section IDs keep proof references stable.

### §1: Workload and historical Redis baseline

Reuse M188 Make targets on an isolated Redis rig before runtime changes. Pin revision, seed, environment identity, and capacity. Capture one baseline campaign with three samples per lane; preserve raw M188 outputs and manifests.

No repeated Redis deployments are required. Existing drivers measure separate paths; combined completion, loss, and recovery evidence belongs to Dragonfly acceptance in §6 and §7. Starting dedicated-rig profile: 1,000,000 active fleet records, 1,000 concurrent runs, 1,000 runner processes, and 16,667 offered events/second.

Use 1,000-byte payloads for that profile; add 16,000-byte payloads and skewed destinations as separate cases. Preparation produces numeric latency, queue-age, memory, recovery, cost, duration, and warmup budgets before acceptance samples; missing budgets fail grading. The agent proposes technical budgets from measured behavior; Indy decides acceptable cost and service limits before dependent acceptance runs.

Credential preflight records rig PostgreSQL/Redis references, the existing deployment's migration secret references, and the dedicated Swarm Connection URI reference. Store the Swarm URI through the existing vault/deployment secret mechanism; missing references block Cloud runs without printing credentials.

- **Dimension 1.1**: missing budgets, mismatched manifests, or too few samples refuse comparison → Test `test_incomparable_datastore_runs_are_rejected`.
- **Dimension 1.2**: fixture caps reject a million-fleet load on a shared deployment before connecting → Test `test_shared_deployment_refuses_saturation_profile`.
- **Dimension 1.3**: revision-pinned Redis baseline preserves available M188 measurements and explicitly identifies metrics requiring later combined-load instrumentation → Test `test_redis_baseline_records_complete_evidence`.

### §2: Durability and accepted-work recovery

Acceptance means recoverable work exists before the success response, surviving an in-memory datastore outage. Default to a PostgreSQL-backed ingress/delivery record and replay dispatcher, reusing existing transactional records where they already provide equivalent guarantees. Cover steer, webhook, cron, continuation, install work, and outbound answers; inventory each producer before changing acceptance.

PLAN specifies transaction boundaries, stable logical IDs versus stream receipts, deduplication lifetime, terminal cleanup, and atomic settlement-to-delivery handoff. Replay preserves logical event identity and fencing across new stream IDs, including API and runner correlation.

- **Dimension 2.1**: stop between durable acceptance, queue append, and append receipt; replay leaves no missing accepted work → Test `test_acceptance_recovers_at_each_crash_boundary`.
- **Dimension 2.2**: destroy the disposable queue and rebuild it without duplicate settlement or billing → Test `test_queue_loss_replays_without_duplicate_settlement`.
- **Dimension 2.3**: unavailable durable storage returns a retryable failure and records no false acceptance → Test `test_durable_store_failure_refuses_acceptance`.

### §3: Retention, memory budgets, and backpressure

Never discard the only recoverable copy of unfinished work; acknowledged history has a bounded retention policy and auditable cleanup. Admission enforces per-fleet and deployment budgets. Exhaustion produces explicit backpressure before durable storage can grow without limit.

Dragonfly uses No Eviction for correctness-bearing state. Provider settings must be checked by preflight or a deployment probe.

- **Dimension 3.1**: slow consumers and retention pressure preserve pending work while acknowledged history stays bounded → Test `test_retention_preserves_pending_work_under_pressure`.
- **Dimension 3.2**: quota exhaustion and out-of-memory replies cause classified backpressure without silent drops → Test `test_full_datastore_applies_explicit_backpressure`.
- **Dimension 3.3**: million-fleet rig reports measured bytes per key class, pending entries, replicas, and database backlog → Test `test_capacity_report_accounts_for_all_retained_state`.

### §4: Shared coordination and fair discovery

Measure shared readiness and outbound pressure; partition when recorded load misses frozen budgets, because additional nodes do not divide a hot key. Preserve bounded poll work, token-checked readiness clearing, per-fleet fencing, and ordering within each outbound destination. Use eligible-runner distributions and slow destinations to measure starvation independently of aggregate throughput.

- **Dimension 4.1**: stale marks and concurrent ingress cannot clear newly ready work or exceed the poll budget → Test `test_ready_races_preserve_work_and_bound_poll_cost`.
- **Dimension 4.2**: skewed runner tags and a slow outbound destination cannot starve eligible unrelated work → Test `test_skewed_workload_preserves_discovery_and_delivery_fairness`.
- **Dimension 4.3**: shard movement or worker restart preserves ordering and reports recovery within the frozen budget → Test `test_coordination_recovers_during_partition_movement`.

### §5: Dragonfly topology and operation-specific Rust boundaries

Retain redis-rs and the existing Redis-compatible operations. The Dragonfly runtime requires explicit cluster configuration before boot accepts work. Add cluster-aware command routing, blocking readers, subscription recovery, and node-aware scans through the shared business layer.

Test actual atomic keys share a valid hash slot. Correctness-sensitive reads target the primary; failures never silently select another provider. Swarm is the required target.

Local development uses real multi-shard Dragonfly with pinned server versions, explicit slot ownership, and database 0. Provide cluster bootstrap and controlled movement through existing test infrastructure; Cloud-managed failover remains a separate Cloud proof.

- **Dimension 5.1**: malformed topology, TLS trust, credentials, and unsafe eviction settings fail before work is accepted → Test `test_datastore_preflight_refuses_invalid_configuration`.
- **Dimension 5.2**: real multi-shard redirection and resharding preserve atomic deduplication and retry outcomes → Test `test_cluster_resharding_preserves_atomic_append`.
- **Dimension 5.3**: reconnect restores subscriptions and scans find all scoped keys without using a shared blocking socket → Test `test_cluster_connections_recover_without_missing_scoped_state`.

### §6: Dragonfly application and sustained-load evidence

Run application fixtures through real services against local multi-shard Dragonfly and Cloud Swarm. Assert documented sessions, approvals, expiry, readiness, Streams, Lua reload, delivery, and merged streaming behavior; declare nondeterministic timing and IDs. Extend M188 with a combined acceptance-to-completion workload, pending-state measurements, and aggregate per-shard statistics.

Keep integration under `make test-integration-rustd`; use disposable multi-shard Dragonfly locally and dedicated Swarm datastores for Cloud proof. Reject shared targets before destructive resets; migrate logical-database isolation to run-scoped fixtures in database 0.

- **Dimension 6.1**: application golden paths match documented outcomes and preserve the merged runtime's streaming behavior → Test `test_dragonfly_preserves_application_outcomes`.
- **Dimension 6.2**: lost responses, Lua cache loss, session races, and expiry preserve error classes and one-time actions → Test `test_dragonfly_preserves_failure_and_session_semantics`.
- **Dimension 6.3**: a shared endpoint, missing Dragonfly evidence, or fixture cleanup mismatch fails verification → Test `test_dragonfly_evidence_refuses_unsafe_or_incomplete_runs`.

### §7: Cloud acceptance, migration, and Redis retirement

Measure implementation increments on Dragonfly. Keep application and PostgreSQL capacity fixed within each performance comparison. Separate code and capacity comparisons; record engine, topology, region, memory, replicas, network path, and background traffic.

Require three matching samples per comparison after warmup. Use identical offered load and reject generator saturation or growing unaccounted backlog. For comparable Dragonfly revisions, p99 latency is at most 1.10 times baseline and completed throughput at least 0.95 times baseline; correctness failures = 0.

All runs must satisfy frozen absolute budgets; threshold changes after baseline cannot turn a failed comparison green. Prepare local tests before activating the Cloud trial. Cloud proof must exercise TLS, managed failover, restore, resizing, and Swarm resharding.

Rehearse cutover before its live approval: stop admission, drain or migrate old queued work, reconcile active leases and outbound answers, then enable Dragonfly. Account for work accepted before durable records existed; classify sessions, deduplication keys, approvals, expiry, and readiness as migrated, drained, expired, or rebuilt. Record the safe pre-admission abort point and post-admission recovery procedure; a bare endpoint rollback cannot recover newly accepted work.

Retire Redis after reconciliation and a frozen observation window pass. Remove server dependencies while retaining redis-rs and historical baseline evidence. The evidence grader distinguishes rehearsal from live completion and requires the approved live action, observed results, and retirement record before R6 passes.

- **Dimension 7.1**: a regression, missing Cloud evidence, or incompatible environment fails strict grading → Test `test_datastore_grader_rejects_regression_and_missing_cloud_proof`.
- **Dimension 7.2**: combined Dragonfly workloads meet completion and backlog budgets and record fixture cleanup → Test `test_dragonfly_sustained_workload_has_complete_evidence`.
- **Dimension 7.3**: cutover, abort, recovery, and Redis retirement preserve accepted work and authentication semantics → Test `test_cutover_recovery_and_redis_retirement_preserve_work`.
- **Dimension 7.4**: Indy approves the rehearsed live procedure; observed cutover, reconciliation, and retirement are recorded → Test `review_live_cutover_and_retirement_evidence`.
## Interfaces

Keep `REDIS_URL_API` as the Redis-protocol connection setting and retain existing TLS/deadline settings. The application target is Dragonfly cluster mode; reject unknown topology values and standalone endpoints before accepting work. Historical Redis benchmarks run the pinned original revision; they do not require a standalone path in the final runtime.

M188 results gain backend, topology, workload hash, revision, deployment identity, resource manifest, acceptance/completion counts, loss, duplicates, backlog, and recovery fields. Add one spec-consumed Make target, `make bench-datastore CHECK=baseline|durability|retention|coordination|cluster|rollout`, in `make/bench.mk`. Each CHECK grades saved historical/Dragonfly evidence without changing deployments.

M188's informational comparator retains its non-failing behavior. The grader and cluster configuration are planned interfaces. Protocol settings retain their names; runtime support does not retain Redis as a provider.
## Failure Modes

| Mode | Cause | Handling and negative test |
|------|-------|----------------------------|
| Lost acknowledgment | Queue applies a write but reply disappears | Replay stable identity; `test_acceptance_recovers_at_each_crash_boundary`. |
| Queue destruction | Primary and replicas lost | Replay surviving durable records; `test_queue_loss_replays_without_duplicate_settlement`. |
| Database unavailable | Durable acceptance cannot commit | Retryable refusal; `test_durable_store_failure_refuses_acceptance`. |
| Retention overflow | Slow consumer or full datastore | Bound admission; retain recoverable work; `test_full_datastore_applies_explicit_backpressure`. |
| Readiness race | New ingress crosses a clear | Preserve token semantics; `test_ready_races_preserve_work_and_bound_poll_cost`. |
| Starvation | Uneven tags or destination delays | Bound unrelated queue age; `test_skewed_workload_preserves_discovery_and_delivery_fairness`. |
| Cross-slot or redirection | Cluster moves atomic keys | Route safely without partial acceptance; `test_cluster_resharding_preserves_atomic_append`. |
| Auth or transport failure | Invalid credentials, trust, topology | Classified refusal; `test_datastore_preflight_refuses_invalid_configuration`. |
| Session replay or script loss | Concurrent consume, expiry, restart | Preserve one-time semantics; `test_dragonfly_preserves_failure_and_session_semantics`. |
| Unsafe test target | Shared deployment passed to reset lane | Refuse before mutation; `test_dragonfly_evidence_refuses_unsafe_or_incomplete_runs`. |
| Misleading measurement | Different resources, overload, missing Cloud run | Fail comparison; `test_datastore_grader_rejects_regression_and_missing_cloud_proof`. |
| Incomplete migration | Work or authentication state remains unreconciled | Refuse completion until reconciled; `test_cutover_recovery_and_redis_retirement_preserve_work`. |

## Invariants

1. Accepted work has a surviving durable record before success; transactional write and crash tests enforce this.
2. Replay cannot duplicate internal settlement or billing; database uniqueness and fencing enforce this, without promising exactly-once external effects.
3. No cleanup deletes the only recoverable unfinished event; retention and admission tests enforce this.
4. A failed backend cannot select another silently; configuration validation and outage tests enforce this.
5. Local multi-shard Dragonfly and Cloud Swarm pass application fixtures; missing evidence fails the grader.
6. Scale claims include workload and capacity manifests; the grader rejects missing or incomparable evidence.
7. Shared deployments cannot receive destructive tests; endpoint ownership checks and hard fixture caps enforce this.
## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| Durable recovery and replay outcomes | ops | Acceptance, replay, terminal settlement | Backend, outcome, counts, duration | No payload, credentials, or tenant labels | `test_queue_loss_replays_without_duplicate_settlement` |
| Backlog age, retained bytes, admission refusal | ops | Queue sampling and rejected admission | Key class, reason, bytes, duration | Bounded dimensions; no fleet label | `test_capacity_report_accounts_for_all_retained_state` |
| Existing lease cost plus fairness measurements | ops | Poll and delivery sampling | Candidate counts, round trips, queue age | Fixture identifiers only in redacted result files | `test_skewed_workload_preserves_discovery_and_delivery_fairness` |
| Evidence verdict and resource manifest | ops | Comparison completes | Revision, profile hash, backend, verdict | URI and credential fields excluded | `test_datastore_grader_rejects_regression_and_missing_cloud_proof` |

Record exact names in `observability.md` with the typed registry changes; update operational responses without changing product analytics or funnels.
## Test Specification (tiered)

| Dimension | Tier | Test | Asserts |
|-----------|------|------|---------|
| 1.1 | unit | `test_incomparable_datastore_runs_are_rejected` | Missing budget, changed seed, or insufficient samples fails grading. |
| 1.2 | unit | `test_shared_deployment_refuses_saturation_profile` | Oversized fixture request opens no connection. |
| 1.3 | integration | `test_redis_baseline_records_complete_evidence` | Historical M188 outputs and revision/resource manifests exist; unavailable historical metrics are identified, never invented. |
| 2.1 | integration | `test_acceptance_recovers_at_each_crash_boundary` | Each injected stop leaves zero missing accepted events. |
| 2.2 | integration | `test_queue_loss_replays_without_duplicate_settlement` | Queue rebuild yields zero losses and duplicate debits. |
| 2.3 | integration | `test_durable_store_failure_refuses_acceptance` | Database failure cannot produce successful acceptance. |
| 3.1 | integration | `test_retention_preserves_pending_work_under_pressure` | Backlogged events remain recoverable after cleanup. |
| 3.2 | integration | `test_full_datastore_applies_explicit_backpressure` | Quota and memory errors produce no silent drop. |
| 3.3 | integration | `test_capacity_report_accounts_for_all_retained_state` | Rig measures populated, pending, completed, and replay state separately. |
| 4.1 | integration | `test_ready_races_preserve_work_and_bound_poll_cost` | Stale token cannot erase newer readiness; candidate cap holds. |
| 4.2 | integration | `test_skewed_workload_preserves_discovery_and_delivery_fairness` | Eligible unrelated work stays inside the frozen queue-age budget. |
| 4.3 | integration | `test_coordination_recovers_during_partition_movement` | Restart and movement preserve ordered delivery within the recovery budget. |
| 5.1 | integration | `test_datastore_preflight_refuses_invalid_configuration` | Bad topology, TLS, auth, or eviction cannot accept work. |
| 5.2 | integration | `test_cluster_resharding_preserves_atomic_append` | Actual distinct slots and redirects cannot create duplicate logical events. |
| 5.3 | integration | `test_cluster_connections_recover_without_missing_scoped_state` | Reconnect and scans restore all expected scoped state. |
| 6.1 | e2e | `test_dragonfly_preserves_application_outcomes` | Real API login, steer, lease, report, approval, and stream paths match documented outcomes. |
| 6.2 | integration | `test_dragonfly_preserves_failure_and_session_semantics` | Script loss, ambiguous replies, expiry, and consume races preserve outcomes. |
| 6.3 | unit / integration | `test_dragonfly_evidence_refuses_unsafe_or_incomplete_runs` | Shared targets, absent Dragonfly environments, and leaked fixtures fail verification. |
| 7.1 | unit | `test_datastore_grader_rejects_regression_and_missing_cloud_proof` | Regression, missing Cloud run, or resource drift cannot grade green. |
| 7.2 | e2e | `test_dragonfly_sustained_workload_has_complete_evidence` | Combined accepted/completed load, shard totals, backlog, and cleanup meet frozen budgets. |
| 7.3 | integration | `test_cutover_recovery_and_redis_retirement_preserve_work` | Old queued work, active leases, and authentication state reconcile before Redis retirement; abort and recovery lose no work. |
| 7.4 | manual | `review_live_cutover_and_retirement_evidence` | Follow the operational playbook; record Indy approval, live revision, reconciliation counts, observation window, and Redis retirement evidence. |

Deployed API, dashboard, and CLI acceptance use the existing Make acceptance targets; fixture providers avoid paid model calls and real outbound messages.
## Acceptance Rubric (single scoring surface)

| # | Criterion | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|-----------|--------------------|----------|----------|-----------------|
| R1 | Comparable baseline and frozen budgets | `make bench-datastore CHECK=baseline` | Exit 0; historical M188 outputs, complete manifests, frozen budgets, and three samples per baseline lane. | P0 | |
| R2 | Accepted-work recovery | `make bench-datastore CHECK=durability` | Exit 0; lost accepted work = 0; duplicate internal settlement = 0. | P0 | |
| R3 | Bounded retention and admission | `make bench-datastore CHECK=retention` | Exit 0; all memory and backlog budgets pass. | P0 | |
| R4 | Fair coordination | `make bench-datastore CHECK=coordination` | Exit 0; all eligible-work queue-age and poll budgets pass. | P0 | |
| R5 | Dragonfly topology and application evidence | `make bench-datastore CHECK=cluster` | Exit 0; local multi-shard and Cloud Swarm application evidence present. | P0 | |
| R6 | Sustained Cloud capacity and Redis retirement | `make bench-datastore CHECK=rollout` | Exit 0; frozen budgets pass; Cloud recovery and rehearsal pass; approved live cutover and Redis retirement evidence exists. | P0 | |
| S1 | Repository conformance | `make harness-verify` | Exit 0. | P0 | |
| S2 | Repository unit verification | `make test-unit-all` | Exit 0; declared coverage gates pass. | P0 | |
| S3 | Repository integration verification | `make test-integration-rustd` | Exit 0; nonzero passing test count. | P0 | |
| S4 | Repository lint | `make lint-all` | Exit 0. | P0 | |
| S5 | Repository version consistency | `make check-version` | Exit 0. | P0 | |
| S6 | No committed secrets | `gitleaks git --no-banner` | Exit 0; no leaks. | P0 | |

R-rows consume immutable historical and Dragonfly evidence; S3 alone proves only its selected environment. Graded cells remain empty until implementation verification records a verdict and decisive output. Authoring checks cannot satisfy runtime acceptance.

Missing Swarm evidence blocks completion; report unsupported capacity as a measured ceiling, never rounded up to the target.
## Dead Code Sweep

No authoring deletions. Inventory with `git grep -n -w -i redis`; classify server dependencies, protocol/client names, tests, and historical records. At PLAN, enumerate exact server references to remove and replacements to test.

Retained protocol names are valid; a blanket zero-Redis-word gate is incorrect.
## Out of Scope

- Live deployment, trial activation, billing changes, and provider switching during authoring; no intermediate standalone Dragonfly migration.
- Permanent Redis server support, repeated same-deployment Redis comparisons, and an ongoing Redis/Dragonfly parity matrix.
- Replacing PostgreSQL, redis-rs, local identity caches, or the runner protocol; inferring simultaneous model runs from fleet-record counts.
- Database disaster recovery, active-active regions, a general cache framework, and exactly-once external effects without provider idempotency.
## Product Clarity (authoring record)

1. **Successful user moment:** fleet workflows run on Dragonfly with measured capacity, recoverable work, and verified Redis retirement.
2. **Preserved user behaviour:** login, approvals, triggers, ordered fleet processing, visible activity, billing, and connector answers keep their documented meanings.
3. **Optimal-way check:** capture one historical baseline, then measure Dragonfly directly with application, capacity, and fault tests.
4. **Rebuild-vs-iterate:** refactor the operation-specific boundaries where measurements require it; keep business semantics and redis-rs.
5. **What we build:** recoverable acceptance, bounded queues, fair coordination, cluster support, backend tests, and strict deployment evidence.
6. **What we do NOT build:** another runtime, benchmark stack, cache framework, permanent Redis support, or automatic live migration.
7. **Fit with existing features:** reuse M188 drivers, merged streaming fixtures, fenced settlement, and the existing integration lane.
8. **Surface order:** operator Make commands and reports first; application behavior is verified through the existing API, dashboard, and CLI.
9. **Dashboard restraint:** no new dashboard controls or scale badges before their measured evidence exists.
10. **Confused-user next step:** the report names the failing budget, missing backend run, or recovery procedure and links its operational playbook.
## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** baseline preparation precedes cluster support, reliability changes, application evidence, and migration; section dependencies state the execution order.
- **Alternatives considered:** endpoint-only replacement leaves durability and Swarm gaps; a universal cache abstraction loses operation semantics; an immediate provider switch obscures regressions.
- **Patch-vs-refactor verdict:** a measured refactor, because durable acceptance and cluster routing cross service boundaries. Dragonfly evidence grades each increment.
## Discovery (consult log)

- **Scope decision (2026-09-06):** the user said "park entire sse" and requested that the DragonflyDB commits be pushed.
  This documentation branch excludes the uncommitted SSE implementation. M192 remains PENDING; no Dragonfly runtime acceptance is claimed.
- **Consults:** Indy approved Dragonfly as the sole deployment target, one historical Redis baseline, and removal of repeated Redis deployment comparisons.
> Indy (2026-09-11): "Yes agreed approved to proceed"; "do it in your own work tree".
- **Metrics review**
- **Skill-chain outcomes:** `orly-spec-new` refreshed scope, implementation paths, proof mappings, and decision prerequisites; runtime verification remains pending.
- **Deferrals**
