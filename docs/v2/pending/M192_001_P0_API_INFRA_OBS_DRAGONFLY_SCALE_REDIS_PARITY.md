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

# M192_001: Add Dragonfly scaling with measured Redis parity

**Prototype:** v2.0.0
**Milestone:** M192
**Workstream:** 001
**Date:** Sep 06, 2026
**Status:** PENDING
**Priority:** P0, required before claiming Dragonfly readiness or million-fleet capacity.
**Categories:** API, INFRA, OBS
**Batch:** B2, consumes the shared benchmark drivers from M188_001.
**Branch:** docs/m192-dragonfly-scale-redis-parity, documentation only; implementation has not started.
**Test Baseline:** not run during authoring; record canonical unit and integration counts at CHORE(open).
**Depends on:** M188_001 for measurement drivers and result files. Streaming acceptance uses the merged runtime; the parked SSE follow-up is not a prerequisite.
**Provenance:** LLM-drafted (Codex, Sep 06, 2026), from the user's Dragonfly and same-deployment requirements.
**Canonical architecture:** `docs/architecture/datastore_scaling.md`; existing flows in `data_flow.md` and `runner_fleet.md`.

## Overview

**Goal (testable):** Redis and Dragonfly Cloud Swarm pass the same application and recovery scenarios, with measured limits and no unexplained Redis regression.
**Problem:** endpoint replacement alone cannot prove queue recovery, bounded memory, fair discovery, or cluster correctness.
**Solution summary:** measure first, then improve durability, retention, coordination, and cluster support through the existing operation-specific Rust boundaries.

Dragonfly Cloud Swarm is mandatory, with no single-shard migration stage. Redis remains supported and remains the deployment default throughout implementation.
Replacing Swarm or removing Redis support requires an explicit user decision. A new benchmark-only environment cannot replace same-deployment evidence.

## PR Intent & comprehension handshake

- **PR title (eventual):** feat: add Dragonfly scaling with measured Redis parity
- **Intent:** retain users' existing fleet behavior while adding tested Dragonfly capacity and recovery.
- **Handshake:** implement on a dedicated feature branch after baseline measurement, preserve Redis behavior, and compare each deployed increment with an identified baseline.
- **ASSUMPTIONS I'M MAKING:** the million-fleet target needs explicit traffic and concurrency assumptions; this authoring task authorizes no deployment or account changes.

## Implementing agent — read these first

1. `docs/architecture/datastore_scaling.md`, the required destination and Redis preservation rules.
2. `docs/v2/pending/M188_001_P1_API_INFRA_OUTBOUND_AND_LEASE_THROUGHPUT_BENCH.md`, shared drivers, profiles, and attribution.
3. `rustd/crates/afd_ingress/src/deliver.rs`, provider deduplication and accepted ingress behavior.
4. `rustd/crates/afd_redis/src/streams/once.rs`, atomic multi-key append and replay windows.
5. https://www.dragonflydb.io/docs/cloud/datastores, managed topology, eviction, replicas, and connection requirements.

## Files Changed (blast radius)

The following table scopes implementation. Existing uncommitted branch work belongs to its current task and must not be staged by this workstream.

| File | Action | Why |
|------|--------|-----|
| `AGENTS.md`, `docs/architecture/README.md`, `docs/architecture/datastore_scaling.md` | EDIT / CREATE | Make the target and evidence rules discoverable. |
| `docs/v2/pending/M192_001_P0_API_INFRA_OBS_DRAGONFLY_SCALE_REDIS_PARITY.md` | CREATE / MOVE | Author here; lifecycle moves the same spec to active and done. |
| `docs/architecture/scaling.md`, `docs/architecture/data_flow.md`, `docs/architecture/runner_fleet.md`, `docs/architecture/concurrency.md` | EDIT | Update actual flows and measured limits with their implementation. |
| `docs/architecture/testing.md`, `docs/architecture/observability.md` | EDIT | Explain backend evidence and recovery signals. |
| `docs/v2/reviews/datastore-scale-evidence.md` | CREATE | Index immutable result files and their deployment revisions. |
| `make/bench.mk`, `bench/harness/*.rs`, `bench/steer/main.rs`, `bench/lease/main.rs`, `bench/outbound/main.rs`, `bench/cardinality/main.rs` | EDIT after M188 | Reuse the measurement drivers and add a strict evidence grader. |
| `bench/harness/datastore.rs`, `bench/profiles/datastore/*.json`, `bench/baselines/datastore/*.json` | CREATE | Backend matrix, workload manifests, and redacted baseline summaries. |
| `make/test-infra.mk`, `make/test-integration-rustd.mk`, `docker-compose.yml` | EDIT | Select disposable backends without creating another integration lane. |
| `rustd/Cargo.toml`, `rustd/Cargo.lock` | EDIT | Enable required redis-rs capabilities and register test support. |
| `rustd/crates/afd_redis/src/*.rs`, `rustd/crates/afd_redis/src/streams/*.rs`, `rustd/crates/afd_redis/src/hub/*.rs` | EDIT | Connections, atomic keys, retention, readiness, scanning, and subscriptions. |
| `rustd/crates/afd_redis/tests/*.rs`, `rustd/crates/afd_redis/tests/support/*.rs` | EDIT / CREATE | Real-backend parity, pressure, topology, and recovery tests. |
| `rustd/crates/afd_ingress/src/*.rs`, `rustd/crates/afd_ingress/tests/*.rs`, `rustd/crates/afd_events/src/steer.rs`, `rustd/crates/afd_cron/src/fire.rs`, `rustd/crates/afd_gate/src/gate/park.rs`, `rustd/crates/afd_approval/src/inbox/*.rs` | EDIT / CREATE | Durable inbound acceptance and replay through the shared boundary. |
| `rustd/crates/afd_fleet_lifecycle/src/install.rs`, `rustd/crates/afd_fleet_lifecycle/src/purge.rs` | EDIT | Include lifecycle ingress and cleanup. |
| `rustd/crates/afd_fleet/src/lease/*.rs`, `rustd/crates/afd_fleet/src/lease/sql/*.rs` | EDIT | Preserve fencing, settlement, and acknowledgment behavior. |
| `rustd/crates/afd_outbound/src/*.rs`, `rustd/crates/afd_outbound/tests/*.rs` | EDIT / CREATE | Recoverable delivery, bounded concurrency, and destination fairness. |
| `rustd/crates/afd_runner/src/sweep/*.rs`, `rustd/crates/agentsfleetd/src/sweepers.rs`, `rustd/crates/agentsfleetd/src/outbound.rs` | EDIT | Replay supervision and recovery lifecycle. |
| `rustd/crates/agentsfleetd/src/preflight.rs`, `rustd/crates/agentsfleetd/src/preflight/config.rs`, `rustd/crates/agentsfleetd/src/serve/*.rs` | EDIT | Explicit backend topology and boot validation. |
| `schema/*queue*.sql`, `schema/embed.zig`, `rustd/crates/afd_db/src/migration.rs`, `rustd/crates/afd_db/tests/*.rs` | CREATE / EDIT | Durable acceptance schema and upgrade proof; allocate the migration number at PLAN. |
| `rustd/crates/afd_sse/src/*.rs`, `rustd/crates/afd_api/tests/integration_*live.rs`, `rustd/crates/afd_api/tests/integration_fleet_streams.rs`, `rustd/crates/afd_api/tests/integration_datastore*.rs`, `rustd/crates/afd_api/tests/*suite.rs` | EDIT / CREATE | Prove real API and streaming behavior on each backend. |
| `rustd/crates/afd_observability/src/producers/fleet*.rs`, `rustd/crates/afd_observability/src/producers/fleet/*.rs`, `rustd/crates/afd_observability/src/metrics/declared/fleet.rs` | EDIT | Bounded recovery, backlog, and scheduling signals. |
| `playbooks/operations/datastore-scaling.md` | CREATE | Same-deployment probes, trial preparation, and reversible rollout procedure. |

Expand module globs into exact files at PLAN. Public behavior documentation needs its matching branch in the separate docs repository.
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

### §1: Workload and same-deployment Redis baseline

Reuse M188 drivers. Freeze revision, workload seed, environment identity, and capacity before runtime changes.
Starting dedicated-rig profile: 1,000,000 active fleet records, 1,000 concurrent runs, 1,000 runner processes, and 16,667 offered events/second.
Use 1,000-byte payloads for that profile; add 16,000-byte payloads and skewed destinations as separate cases.
Freeze absolute latency, queue-age, memory, recovery, and cost budgets before collecting acceptance samples; missing budgets fail the grader.
Credential preflight requires the existing deployment's Redis secret reference and the dedicated Swarm test datastore's Connection URI from Dragonfly Cloud Connection Details.
Store the Swarm URI through the existing vault/deployment secret mechanism; missing references block Cloud runs without printing credentials.

- **Dimension 1.1**: missing budgets, mismatched manifests, or too few samples refuse comparison → Test `test_incomparable_datastore_runs_are_rejected`.
- **Dimension 1.2**: fixture caps reject a million-fleet load on a shared deployment before connecting → Test `test_shared_deployment_refuses_saturation_profile`.
- **Dimension 1.3**: revision-pinned Redis baseline includes accepted and completed rates, p95/p99, backlog, errors, and per-datastore cost → Test `test_redis_baseline_records_complete_evidence`.

### §2: Durability and accepted-work recovery

Acceptance means recoverable work exists before the success response, surviving an in-memory datastore outage.
Default to a PostgreSQL-backed ingress/delivery record and replay dispatcher, reusing existing transactional records where they already provide equivalent guarantees.
Cover steer, webhook, cron, continuation, install work, and outbound answers; inventory each producer before changing acceptance.
Replay preserves logical event identity and fencing across new stream IDs.

- **Dimension 2.1**: stop between durable acceptance, queue append, and append receipt; replay leaves no missing accepted work → Test `test_acceptance_recovers_at_each_crash_boundary`.
- **Dimension 2.2**: destroy the disposable queue and rebuild it without duplicate settlement or billing → Test `test_queue_loss_replays_without_duplicate_settlement`.
- **Dimension 2.3**: unavailable durable storage returns a retryable failure and records no false acceptance → Test `test_durable_store_failure_refuses_acceptance`.

### §3: Retention, memory budgets, and backpressure

Never discard the only recoverable copy of unfinished work; acknowledged history has a bounded retention policy and auditable cleanup.
Admission enforces per-fleet and deployment budgets. Exhaustion produces explicit backpressure before durable storage can grow without limit.
Dragonfly uses No Eviction for correctness-bearing state. Provider settings must be checked by preflight or a deployment probe.

- **Dimension 3.1**: slow consumers and retention pressure preserve pending work while acknowledged history stays bounded → Test `test_retention_preserves_pending_work_under_pressure`.
- **Dimension 3.2**: quota exhaustion and out-of-memory replies cause classified backpressure without silent drops → Test `test_full_datastore_applies_explicit_backpressure`.
- **Dimension 3.3**: million-fleet rig reports measured bytes per key class, pending entries, replicas, and database backlog → Test `test_capacity_report_accounts_for_all_retained_state`.

### §4: Shared coordination and fair discovery

Measure shared readiness and outbound pressure; partition when recorded load misses frozen budgets, because additional nodes do not divide a hot key.
Preserve bounded poll work, token-checked readiness clearing, per-fleet fencing, and ordering within each outbound destination.
Use eligible-runner distributions and slow destinations to measure starvation independently of aggregate throughput.

- **Dimension 4.1**: stale marks and concurrent ingress cannot clear newly ready work or exceed the poll budget → Test `test_ready_races_preserve_work_and_bound_poll_cost`.
- **Dimension 4.2**: skewed runner tags and a slow outbound destination cannot starve eligible unrelated work → Test `test_skewed_workload_preserves_discovery_and_delivery_fairness`.
- **Dimension 4.3**: shard movement or worker restart preserves ordering and reports recovery within the frozen budget → Test `test_coordination_recovers_during_partition_movement`.

### §5: Dragonfly topology and operation-specific Rust boundaries

Retain redis-rs and the existing Redis-compatible operations. Standalone mode remains the default unless explicitly configured otherwise.
Add cluster-aware command routing, blocking readers, subscription recovery, and node-aware scans through the shared business layer.
Test actual atomic keys share a valid hash slot. Correctness-sensitive reads target the primary; failures never silently select another provider.
Swarm is the first and required Dragonfly target. Local development uses a real multi-shard cluster; no standalone Dragonfly rollout is planned.

- **Dimension 5.1**: malformed topology, TLS trust, credentials, and unsafe eviction settings fail before work is accepted → Test `test_datastore_preflight_refuses_invalid_configuration`.
- **Dimension 5.2**: real multi-shard redirection and resharding preserve atomic deduplication and retry outcomes → Test `test_cluster_resharding_preserves_atomic_append`.
- **Dimension 5.3**: reconnect restores subscriptions and scans find all scoped keys without using a shared blocking socket → Test `test_cluster_connections_recover_without_missing_scoped_state`.

### §6: Redis and Dragonfly behavioral evidence

Run the same fixtures through real application services against Redis and Dragonfly Swarm.
Compare sessions, approvals, expiry, readiness, Streams, Lua reload, and delivery; allow only fixture-declared nondeterministic identifier or timing differences.
Keep integration under `make test-integration-rustd`; use disposable Redis and multi-shard Dragonfly locally, and dedicated Swarm datastores for Cloud proof.
Extend backend selection without forwarding destructive test resets to a shared deployment or requiring logical databases unsupported by the selected topology.

- **Dimension 6.1**: application golden paths return equivalent outcomes and preserve the merged runtime's streaming behavior → Test `test_backends_preserve_application_outcomes`.
- **Dimension 6.2**: lost responses, Lua cache loss, session races, and expiry preserve error classes and one-time actions → Test `test_backends_preserve_failure_and_session_semantics`.
- **Dimension 6.3**: a shared endpoint, incomplete matrix, or fixture cleanup mismatch fails verification → Test `test_backend_matrix_refuses_unsafe_or_incomplete_evidence`.

### §7: Incremental deployments, trial, and strict comparison

Deploy each section to the same services with Redis selected; capture matching samples with constant application and PostgreSQL capacity.
Separate code and capacity comparisons; record engine, topology, region, memory, replicas, network path, and background traffic.
Require three matching samples per comparison after warmup. Use identical offered load and reject generator saturation or growing unaccounted backlog.
Default regression limits: p99 latency at most 1.10 times baseline; completed throughput at least 0.95 times baseline; no correctness failures.
All runs must satisfy frozen absolute budgets; threshold changes after baseline cannot turn a failed comparison green.
Prepare local tests before activating the Cloud trial. Cloud proof must exercise TLS, managed failover, restore, resizing, and Swarm resharding.
An eventual provider switch needs an explicit action and a rehearsed drain/replay procedure. A bare endpoint rollback cannot recover work accepted only by another backend.

- **Dimension 7.1**: a regression, missing Cloud evidence, or incompatible environment fails strict grading → Test `test_datastore_grader_rejects_regression_and_missing_cloud_proof`.
- **Dimension 7.2**: revision-bound Redis before/after evidence passes on the same deployment and records fixture cleanup → Test `test_deployed_redis_increment_has_comparable_evidence`.
- **Dimension 7.3**: provider cutover and rollback on the isolated rig preserve accepted work and single settlement → Test `test_provider_switch_and_rollback_preserve_accepted_work`.

## Interfaces

Keep `REDIS_URL_API` and existing TLS/deadline settings; any new topology selector must reject unknown values and default to standalone behavior.
M188 results gain backend, topology, workload hash, revision, deployment identity, resource manifest, acceptance/completion counts, loss, duplicates, backlog, and recovery fields.
Add one spec-consumed Make target, `make bench-datastore CHECK=baseline|durability|retention|coordination|cluster|rollout`, in `make/bench.mk`.
Each CHECK grades saved M188/backend evidence without changing deployments. M188's informational comparator retains its non-failing behavior.
The target and backend selectors are planned interfaces, not implemented commands.

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
| Session replay or script loss | Concurrent consume, expiry, restart | Preserve one-time semantics; `test_backends_preserve_failure_and_session_semantics`. |
| Unsafe test target | Shared deployment passed to reset lane | Refuse before mutation; `test_backend_matrix_refuses_unsafe_or_incomplete_evidence`. |
| Misleading measurement | Different resources, overload, missing Cloud run | Fail comparison; `test_datastore_grader_rejects_regression_and_missing_cloud_proof`. |
| Incomplete rollback | Work remains on switched backend | Refuse completion until reconciled; `test_provider_switch_and_rollback_preserve_accepted_work`. |

## Invariants

1. Accepted work has a surviving durable record before success; transactional write and crash tests enforce this.
2. Replay cannot duplicate internal settlement or billing; database uniqueness and fencing enforce this, without promising exactly-once external effects.
3. No cleanup deletes the only recoverable unfinished event; retention and admission tests enforce this.
4. A failed backend cannot select another silently; configuration validation and outage tests enforce this.
5. Supported providers pass identical behavior fixtures; an incomplete matrix fails the evidence grader.
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
| 1.3 | integration | `test_redis_baseline_records_complete_evidence` | Every required measurement and manifest field is populated. |
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
| 6.1 | e2e | `test_backends_preserve_application_outcomes` | Real API login, steer, lease, report, approval, and stream paths agree. |
| 6.2 | integration | `test_backends_preserve_failure_and_session_semantics` | Script loss, ambiguous replies, expiry, and consume races preserve outcomes. |
| 6.3 | unit / integration | `test_backend_matrix_refuses_unsafe_or_incomplete_evidence` | Shared targets, absent providers, and leaked fixtures fail verification. |
| 7.1 | unit | `test_datastore_grader_rejects_regression_and_missing_cloud_proof` | Regression, missing Cloud run, or resource drift cannot grade green. |
| 7.2 | e2e | `test_deployed_redis_increment_has_comparable_evidence` | Same-deployment revision pair preserves application behavior and budgets. |
| 7.3 | integration | `test_provider_switch_and_rollback_preserve_accepted_work` | Queued and in-flight work survives both provider transitions. |

Deployed API, dashboard, and CLI acceptance use the existing Make acceptance targets; fixture providers avoid paid model calls and real outbound messages.

## Acceptance Rubric (single scoring surface)

| # | Criterion | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|-----------|--------------------|----------|----------|-----------------|
| R1 | Comparable baseline and frozen budgets | `make bench-datastore CHECK=baseline` | Exit 0; complete manifests and three comparable samples. | P0 | |
| R2 | Accepted-work recovery | `make bench-datastore CHECK=durability` | Exit 0; lost accepted work = 0; duplicate internal settlement = 0. | P0 | |
| R3 | Bounded retention and admission | `make bench-datastore CHECK=retention` | Exit 0; all memory and backlog budgets pass. | P0 | |
| R4 | Fair coordination | `make bench-datastore CHECK=coordination` | Exit 0; all eligible-work queue-age and poll budgets pass. | P0 | |
| R5 | Redis and Dragonfly topology matrix | `make bench-datastore CHECK=cluster` | Exit 0; Redis, local multi-shard, and Cloud Swarm evidence present. | P0 | |
| R6 | Same-deployment increments and rollback | `make bench-datastore CHECK=rollout` | Exit 0; no unexplained regression; Cloud recovery and switch rehearsal pass. | P0 | |
| S1 | Repository conformance | `make harness-verify` | Exit 0. | P0 | |
| S2 | Repository unit verification | `make test-unit-all` | Exit 0; declared coverage gates pass. | P0 | |
| S3 | Repository integration verification | `make test-integration-rustd` | Exit 0; nonzero passing test count. | P0 | |
| S4 | Repository lint | `make lint-all` | Exit 0. | P0 | |
| S5 | Repository version consistency | `make check-version` | Exit 0. | P0 | |
| S6 | No committed secrets | `gitleaks git --no-banner` | Exit 0; no leaks. | P0 | |

R-rows consume immutable evidence from the backend matrix and deployed probes; S3 alone only proves its selected backend.
Graded cells remain empty until implementation verification records a verdict and decisive output. Authoring checks cannot satisfy runtime acceptance.
Missing Swarm evidence blocks completion; report unsupported capacity as a measured ceiling, never rounded up to the target.

## Dead Code Sweep

No authoring deletions. Remove replaced helpers and enumerate reference sweeps before renaming symbols or queue formats during implementation.

## Out of Scope

- Deploying code, activating a trial, changing billing, or switching the provider during authoring; no intermediate standalone Dragonfly migration.
- Replacing PostgreSQL, redis-rs, local identity caches, or the runner protocol; inferring simultaneous model runs from fleet-record counts.
- Database disaster recovery, active-active regions, a general cache framework, and exactly-once external effects without provider idempotency.

## Product Clarity (authoring record)

1. **Successful user moment:** the same deployed fleet workflow works after each Redis-backed increment, with a linked comparison report and tested Dragonfly capability.
2. **Preserved user behaviour:** login, approvals, triggers, ordered fleet processing, visible activity, billing, and connector answers keep their documented meanings.
3. **Optimal-way check:** measure the real paths first; small deployed probes complement dedicated capacity and fault tests.
4. **Rebuild-vs-iterate:** refactor the operation-specific boundaries where measurements require it; keep business semantics and redis-rs.
5. **What we build:** recoverable acceptance, bounded queues, fair coordination, cluster support, backend tests, and strict deployment evidence.
6. **What we do NOT build:** another runtime, benchmark stack, cache framework, or automatic production migration.
7. **Fit with existing features:** reuse M188 drivers, merged streaming fixtures, fenced settlement, and the existing integration lane.
8. **Surface order:** operator Make commands and reports first; application behavior is verified through the existing API, dashboard, and CLI.
9. **Dashboard restraint:** no new dashboard controls or scale badges before their measured evidence exists.
10. **Confused-user next step:** the report names the failing budget, missing backend run, or recovery procedure and links its operational playbook.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** seven sections separate baseline, each of the four risks, shared compatibility tests, and deployment evidence.
- **Alternatives considered:** endpoint-only replacement leaves durability and Swarm gaps; a universal cache abstraction loses operation semantics; an immediate provider switch obscures regressions.
- **Patch-vs-refactor verdict:** a measured refactor, because durable acceptance and partitioning cross service boundaries. Each increment retains Redis and produces its own evidence.

## Discovery (consult log)

- **Scope decision (2026-09-06):** the user said "park entire sse" and requested that the DragonflyDB commits be pushed.
  This documentation branch excludes the uncommitted SSE implementation. M192 remains PENDING; no Dragonfly runtime acceptance is claimed.
- **Consults**
- **Metrics review**
- **Skill-chain outcomes**
- **Deferrals**
