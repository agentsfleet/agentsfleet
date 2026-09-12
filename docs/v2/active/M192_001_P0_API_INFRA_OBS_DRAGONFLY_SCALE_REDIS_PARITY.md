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

# M192_001: Build the sharded Dragonfly datastore with proven Redis parity

**Prototype:** v2.0.0
**Milestone:** M192
**Workstream:** 001
**Date:** Sep 12, 2026
**Status:** IN_PROGRESS
**Priority:** P0
**Categories:** API, INFRA, OBS
**Batch:** B2
**Branch:** feat/m192-dragonfly-cluster
**Baseline revision:** 995566da8118cbb88b0b32bb5b44a19d2e704f5e
**Test Baseline:** pending — measured before the Pull Request.
**Depends on:** M188_001 drivers (merged). The historical Redis baseline is closed PR #681 (`agentsfleet/agentsfleet#681`, unmerged); its twelve samples are reference data and are not recaptured here.
**Provenance:** Second revision. Redesigned after the closed baseline PR, from a live probe of a four-node Dragonfly v1.40.2 cluster (Discovery below).
**Canonical architecture:** `docs/architecture/datastore_scaling.md` (target sharding, evidence rules); runtime context in `data_flow.md` and `runner_fleet.md`.

## Overview

**Goal (testable):** the daemon runs its fleet workflow against a real multi-node Dragonfly cluster with the same outcomes it produces on standalone Redis, and every accepted event survives loss of the in-memory datastore.
**Problem:** the live probe shows the current design cannot run on a cluster at all — the at-most-once append is a two-key Lua script that answers `CROSSSLOT`, readiness and outbound delivery are single hot keys, the hub uses global pub/sub which a cluster broadcasts to every node, and acceptance is queue-only so a lost queue is lost work.
**Solution summary:** move acceptance into PostgreSQL so the queue carries only single-key appends; route every command, script, subscription and scan by slot through one topology-aware `afd_redis` boundary; shard readiness by fleet hash; prove each change on standalone Redis and on the local cluster in the same integration lane. Redis stays the deployment default; Cloud Swarm proof and live cutover belong to a follow-up spec once the local matrix is green.

## PR Intent & comprehension handshake

- **PR title:** feat: build the sharded Dragonfly datastore with proven Redis parity
- **Intent:** keep users' fleets behaving identically while the datastore layer becomes cluster-correct and crash-durable.
- **Handshake:** prototype the four cluster risks first, integrate only proven shapes, land Redis parity evidence with every section, never claim a deploy or a cutover.
- **ASSUMPTIONS I'M MAKING:** (1) no live deployment, secret, Cloud account, or workflow file changes without a concrete ask; (2) the historical baseline numbers are sufficient and are not re-run; (3) sharded pub/sub is used on standalone Redis too, so the product has one publish semantics rather than two.

## Implementing agent — read these first

1. `docs/architecture/datastore_scaling.md` — target design, key layout, what each backend must prove.
2. `rustd/crates/afd_redis/src/client.rs`, `rustd/crates/afd_redis/src/hub/pump.rs`, `rustd/crates/afd_redis/src/ready.rs`, `rustd/crates/afd_redis/src/streams/once.rs` — the boundary being made topology-aware.
3. `rustd/crates/afd_ingress/src/deliver.rs`, `rustd/crates/afd_events/src/steer.rs`, `rustd/crates/afd_cron/src/fire.rs` — every producer that must accept durably.
4. `docs/architecture/testing.md` §Rust test naming and §ISO-1..3 — new suites follow the file's convention and isolate by minted prefix.
5. https://github.com/dragonflydb/dragonfly/blob/main/docs/cluster-mode.md and https://www.dragonflydb.io/docs/command-reference/compatibility, validated against the pinned image.

## Files Changed (blast radius)

| File | Action | Why |
|---|---|---|
| `docker-compose.yml`, `scripts/dragonfly-cluster.sh`, `scripts/test-infra-ports.sh`, `make/test-infra.mk`, `make/test-integration-rustd.mk` | EDIT / CREATE | One compose service running a four-node cluster (two primaries, two replicas) announced on `127.0.0.1`, per-worktree ports, `TEST_DRAGONFLY_URL`, owned-cluster reset. |
| `rustd/Cargo.toml`, `rustd/Cargo.lock`, `rustd/crates/afd_redis/Cargo.toml` | EDIT | `cluster-async` feature; RESP3 push delivery. |
| `rustd/crates/afd_redis/src/{lib,config,client,error,hub,dedicated,kv,ready,session}.rs`, `src/hub/pump.rs`, `src/streams/{tail,once}.rs`, new `src/topology.rs`, `src/hub/sharded.rs`, `src/ready/partition.rs` | EDIT / CREATE | Topology selector, cluster backend behind `Redis`, sharded hub, node-routed dedicated reader, all-primaries scan, partitioned readiness, single-key append. |
| `rustd/crates/afd_redis/tests/{redis_suite,integration_cluster_*,integration_sharded_hub,integration_partitioned_ready}.rs`, `tests/support/cluster_harness.rs` | EDIT / CREATE | Prototype proofs and parity suites run against both backends. |
| New `schema/91x_*.sql` (numbers allocated at each Section), `rustd/crates/afd_db/src/migration.rs`, `rustd/crates/afd_db/tests/*.rs` | CREATE / EDIT | Durable admission ledger with producer identity, replay cursor, retention accounting. |
| `rustd/crates/{afd_ingress,afd_events,afd_cron,afd_gate,afd_approval,afd_fleet_lifecycle}/src/**` and their tests | EDIT | Every producer accepts through the admission ledger before the queue append; replay dispatcher supervises the gap. |
| `rustd/crates/afd_fleet/src/lease/**`, `rustd/crates/afd_outbound/src/**`, `rustd/crates/afd_runner/src/sweep/**`, `rustd/crates/agentsfleetd/src/{sweepers,outbound}.rs`, `serve/{runtime,optional}.rs`, `preflight.rs` | EDIT | Partition cursor per runner, destination-fair delivery, replay supervision, topology preflight and readiness. |
| `rustd/crates/afd_observability/src/producers/fleet*.rs`, `src/metrics/declared/fleet.rs` | EDIT | Replay, backlog, admission-refusal and partition signals. |
| `rustd/crates/afd_bench/src/**`, `make/bench.mk`, `bench/profiles/datastore/*.json` | EDIT / CREATE | `make bench-datastore CHECK=…` strict grader over archived local evidence. |
| `docs/architecture/{datastore_scaling,data_flow,runner_fleet,testing,observability}.md`, `docs/v2/reviews/datastore-scale-evidence.md`, `playbooks/operations/datastore_scaling/001_playbook.md` | EDIT / CREATE | Actual topology with each Section; evidence index; operator procedure. |
| `~/Projects/docs` branch | EDIT if a documented behaviour changes | Retry and stream semantics if they move; never from this worktree. |

Workflow and deploy files are out of this blast radius; CI runs the compose stack through `make`, so a new compose service needs no workflow edit.

## Applicable Rules

- `docs/greptile-learnings/RULES.md`: UFS, NDC, NLR, OWN, ECL, TIM, STR, TCF, ORP, TST-NAM, TNM, HLP, GATDL.
- `dispatch/write_rust.md` + `docs/RUST_ERROR_STANDARD.md`: one error per crate, `#[from]`, no stringified causes; Result pipelines.
- `dispatch/write_sql.md` + `docs/SCHEMA_CONVENTIONS.md`: schema-qualified, GRANTs, no static strings in schema.
- `dispatch/write_any.md`: 350-line files, logging events with `error_code`, no milestone ids in source.
- `dispatch/name_architecture.md`: every new key, channel, partition and table lands in `docs/architecture/` in the same commit.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|---|---|---|
| SPEC TEMPLATE | Authoring | Complete sections, canonical Make commands, ≤320 lines. |
| Architecture consult, SCHEMA, AUTH | Implementation | Target layout recorded before code; migration upgrade proofs; single-use tokens keep compare-then-delete. |
| LENGTH, UFS, LOGGING, ERR-RS | Every source edit | Split modules at the seam; verbs and keys as consts; typed errors with sources. |
| VERIFY | Section and boundary | Canonical Make targets; `make bench-datastore` grades archived evidence, never a live claim. |

## Prior-Art / Reference Implementations

- `rustd/crates/afd_redis/tests/integration_hub.rs` (refcount and reconnect proofs) and `integration_streams.rs` (real-server replay) — the shapes the cluster suites mirror.
- redis-rs `cluster_async::ClusterConnection` (`route_command`, `ssubscribe`, RESP3 `push_sender`) — routing is the driver's; this repository never parses a `MOVED`.
- Dragonfly `docs/cluster-mode.md` §2.2 bootstrap sequence and §4.1 config JSON — reproduced by `scripts/dragonfly-cluster.sh`.

## Sections (implementation slices)

Execution order: §0 → §5 → §2 → §3 → §4 → §6 → §7. §1 is complete as historical evidence. Every Section lands its Redis parity test and its cluster test in `make test-integration-rustd`.

### §0: Local cluster and prototype proofs

Dependencies: none. The compose service `dragonfly` runs four `dragonfly` processes in one network namespace (`--cluster_mode=yes`, stable `--cluster_node_id`, `--cluster_announce_ip=127.0.0.1`, admin ports, `--proactor_threads=2 --maxmemory=512mb` per node, one data directory per node) and bootstraps `REPLICAOF` plus `DFLYCLUSTER CONFIG` on every start; the healthcheck passes only when `CLUSTER SHARDS` reports two online primaries with replicas. Host ports equal announced ports so redirects resolve from the host and from CI. Slot movement in tests is driven through the same script (`migrate <from> <to> <range>`) against the owned cluster only.
Prototype tests are integration tests in `afd_redis` that use the raw driver where the boundary does not yet exist, and are replaced by the boundary's own suites as §5 and §4 land.

- **Dimension 0.1**: a sharded subscription made through one seed receives a `SPUBLISH` issued through another seed, and continues after the channel's slot migrates → Test `test_sharded_subscription_survives_slot_movement`.
- **Dimension 0.2**: a single-key script and a dedicated blocking reader keep answering across a slot migration, and a two-key script across slots is refused with `CROSSSLOT` (the probe result, pinned) → Test `test_cluster_primitives_survive_movement`.
- **Dimension 0.3**: a ledger-first acceptance replayed after queue loss yields one logical event per identity whatever order physical receipts arrive → Test `test_durable_identity_survives_replay`.
- **Dimension 0.4**: readiness marks spread over `fleet:ready:{p}` keep every partition's work discoverable inside a bounded poll budget while one partition is hot → Test `test_partitioned_readiness_bounds_hotspots`.

### §1: Historical Redis baseline

Complete. Closed PR #681 captured three samples per M188 lane on a reset rig (medians: steer 10,370.762 accepted/s, p95 1.187 ms; lease 78.609 polls/s; outbound fixture 48.534/s; 10k-fleet candidate query 1.306 ms; Redis growth 46,472,496 bytes). Those are reference points for the local rig only; they prove no Dragonfly improvement and no deployed capacity, and this stream never relabels them.

- **Dimension 1.1**: incomparable evidence is refused by the grader → Test `test_incomparable_datastore_runs_are_rejected` (PR #681). DONE.
- **Dimension 1.2**: a shared or remote target cannot be saturated from the rig → Test `test_shared_deployment_refuses_saturation_profile` (PR #681). DONE.
- **Dimension 1.3**: twelve distinct samples with sidecars → Test `test_redis_baseline_records_complete_evidence` (PR #681). DONE.

### §2: Durable admission and stable logical identity

Dependencies: §0.3 and §5. Acceptance means a committed PostgreSQL admission row before any success response. The row carries producer identity (steer request, webhook delivery id, schedule fire, gate continuation, install, outbound answer), the fleet, the payload digest, and a status that a replay dispatcher advances; the queue append is a single-key `XADD` recorded back as a physical receipt. Duplicate producer identity is a unique-index conflict, which replaces every Redis dedup claim; `once.rs` and its two-key script are deleted, not kept behind a flag. Numeric event ids and the existing stream entry ids stay as the physical receipt; logical identity is the ledger row.
Replay re-appends any admitted row without a receipt after a bounded age, under the same fencing the lease path already enforces; settlement and billing key on the ledger row so a replayed receipt cannot debit twice.

- **Dimension 2.1**: a stop injected between commit, append and receipt leaves zero missing admitted events after replay → Test `test_acceptance_recovers_at_each_crash_boundary`.
- **Dimension 2.2**: destroying the cluster's data and rebuilding it replays every unfinished admission with one settlement and one debit → Test `test_queue_loss_replays_without_duplicate_settlement`.
- **Dimension 2.3**: an unavailable database refuses every producer with a retryable class and records no acceptance → Test `test_durable_store_failure_refuses_acceptance`.

### §3: Retention, memory limits, and backpressure

Dependencies: §2. `MAXLEN ~` trimming is replaced by trimming bounded below by the oldest unreceipted or pending entry; acknowledged history keeps a fixed bound. A consumer group lost on the cluster is recreated from the ledger's replay cursor, never blindly at `$`. Per-fleet and per-deployment admission budgets refuse with an explicit class before durable storage grows without limit; `OOM` and quota replies from either backend are classified and surfaced, never swallowed. Preflight refuses a Dragonfly node running `cache_mode` or eviction.

- **Dimension 3.1**: a slow consumer under trim pressure keeps every pending entry recoverable while acknowledged history stays bounded → Test `test_retention_preserves_pending_work_under_pressure`.
- **Dimension 3.2**: a full datastore produces classified backpressure and no silent drop → Test `test_full_datastore_applies_explicit_backpressure`.
- **Dimension 3.3**: the capacity report accounts for streams, partitions, pending entries, replicas and ledger backlog separately → Test `test_capacity_report_accounts_for_all_retained_state`.

### §4: Sharded readiness and bounded fair scheduling

Dependencies: §0.4, §2, §3. `fleet:ready` becomes `fleet:ready:{p}` for a fixed partition count chosen from the §0.4 measurement and recorded in `datastore_scaling.md`; each runner poll rotates a partition cursor with a candidate budget, and the token-checked clear stays a single-key script. Outbound delivery keeps one stream and adds per-destination concurrency limits so one slow destination cannot hold unrelated answers; ordering within a destination is preserved. Fairness is measured with skewed runner tags and a slow fixture destination, not inferred from throughput.

- **Dimension 4.1**: concurrent ingress and stale clears cannot lose newly ready work across partitions, and a poll never exceeds its candidate budget → Test `test_ready_races_preserve_work_and_bound_poll_cost`.
- **Dimension 4.2**: skewed eligibility and a slow destination cannot starve unrelated work beyond the frozen queue-age budget → Test `test_skewed_workload_preserves_discovery_and_delivery_fairness`.
- **Dimension 4.3**: partition slot movement and worker loss preserve ordered recovery → Test `test_coordination_recovers_during_partition_movement`.

### §5: Topology-aware transport, sharded hub, and scans

Dependencies: §0.1, §0.2. `REDIS_TOPOLOGY=standalone|cluster` (default `standalone`, unknown values refused at preflight) selects the backend behind the unchanged `Redis`, `Dedicated`, `FleetStreams`, `ReadyIndex`, `SessionStore` and `SubscriptionHub` surfaces. The cluster backend is `cluster_async` over RESP3, primaries only, never `read_from_replicas`. The hub subscribes with `SSUBSCRIBE` on both topologies and reconciles after `SUNSUBSCRIBE` pushes and reconnects with no duplicate local frame; `publish_tail` becomes `SPUBLISH`. `scan_keys` fans out over every primary from the live topology. Dedicated readers follow redirects. Preflight fails readiness on cluster or auth errors, TLS trust failures, missing sharded pub/sub, or an eviction-enabled node; `DFLYCLUSTER`/`DFLYMIGRATE` are never issued by the daemon.

- **Dimension 5.1**: malformed topology, bad TLS trust, wrong credentials, missing capability or unsafe eviction refuse boot before any work is accepted → Test `test_datastore_preflight_refuses_invalid_configuration`.
- **Dimension 5.2**: a slot migration during appends preserves one physical entry per logical event and no partial acceptance → Test `test_cluster_resharding_preserves_atomic_append`.
- **Dimension 5.3**: a node restart restores every subscription, every scan finds all scoped keys, and no blocking read shares a socket → Test `test_cluster_connections_recover_without_missing_scoped_state`.
- **Dimension 5.4**: session verify, approve and abort keep single-use semantics on both topologies → Test `test_session_transitions_preserve_single_use_on_both_backends`.

### §6: Application parity and local readiness evidence

Dependencies: §2–§5. `make test-integration-rustd` runs the API, streaming, lease, report, approval and session suites against standalone Redis and against the local cluster from one selector (`TEST_DATASTORE=redis|dragonfly`, both by default), with prefix isolation and an owned-cluster reset. The evidence grader (`make bench-datastore CHECK=prototype|durability|retention|coordination|cluster`) grades archived result files from this branch's own runs; a shared endpoint, an incomplete matrix, or missing provenance fails it. Cloud Swarm proof (TLS, managed failover, restore, resizing) is required for cutover and is recorded as the follow-up spec's first gate, not claimed here.

- **Dimension 6.1**: real application golden paths return equivalent outcomes on both backends → Test `test_backends_preserve_application_outcomes`.
- **Dimension 6.2**: lost replies, script cache loss, session races and expiry preserve error classes and one-time actions on both backends → Test `test_backends_preserve_failure_and_session_semantics`.
- **Dimension 6.3**: a shared target, an absent backend, or leaked fixtures fail verification → Test `test_backend_matrix_refuses_unsafe_or_incomplete_evidence`.

### §7: Migration tooling and rehearsal

Dependencies: §6. An offline operator tool inventories a source Redis by key class with counts and dispositions (unknown classes block), imports unfinished admissions into the ledger, records an import receipt the daemon's preflight requires before it accepts work on a new datastore, and rehearses cancel-before-schema-change followed by forward recovery on the isolated rig. No reverse migration is built.

- **Dimension 7.1**: the rehearsal preserves source state through cancellation, import and forward recovery, with single settlement → Test `test_provider_switch_and_rollback_preserve_accepted_work`.
- **Dimension 7.2**: a missing or wrong import receipt closes every admission path → Test `test_missing_import_receipt_refuses_admission`.

## Interfaces

Keep `REDIS_URL_API`, `REDIS_TLS_CA_CERT_FILE`, the deadline knobs, channel names `fleet:{id}:activity`, stream keys `fleet:{id}:events`, and numeric event ids. Add `REDIS_TOPOLOGY` as above. The lane gains `TEST_DRAGONFLY_URL` and `TEST_DATASTORE`. Add `make bench-datastore CHECK=prototype|durability|retention|coordination|cluster` in `make/bench.mk`; each mode grades archived files and exits non-zero on a missing or incomparable run. M188's `bench-compare` keeps its non-failing behaviour.

## Failure Modes

| Mode | Cause | Handling and negative test |
|---|---|---|
| Cross-slot script | Two keys in one Lua on a cluster | Single-key appends only; `CROSSSLOT` pinned; `test_cluster_primitives_survive_movement`. |
| Lost acknowledgment | Append applied, reply lost | Ledger identity, replay by row; `test_acceptance_recovers_at_each_crash_boundary`. |
| Queue destruction | Cluster data gone | Replay from ledger, one settlement; `test_queue_loss_replays_without_duplicate_settlement`. |
| Database unavailable | Admission cannot commit | Retryable refusal, no false success; `test_durable_store_failure_refuses_acceptance`. |
| Subscriber stranded | Slot moved under a sharded subscription | Reconcile on `SUNSUBSCRIBE`; `test_sharded_subscription_survives_slot_movement`. |
| Retention overflow | Slow consumer, full store | Bounded trim, classified backpressure; `test_full_datastore_applies_explicit_backpressure`. |
| Readiness race | Ingress crosses a clear | Token semantics per partition; `test_ready_races_preserve_work_and_bound_poll_cost`. |
| Starvation | Skewed tags, slow destination | Partition cursor, destination limits; `test_skewed_workload_preserves_discovery_and_delivery_fairness`. |
| Unsafe topology | Bad TLS, auth, eviction, capability | Refuse at preflight; `test_datastore_preflight_refuses_invalid_configuration`. |
| Session replay | Script cache loss, races, expiry | Compare-then-delete on both backends; `test_session_transitions_preserve_single_use_on_both_backends`. |
| Unsafe test target | Shared endpoint in the reset lane | Owned-cluster check before mutation; `test_backend_matrix_refuses_unsafe_or_incomplete_evidence`. |
| Incomplete migration | Work left on the source | Receipt gate; `test_missing_import_receipt_refuses_admission`. |

## Invariants

1. Admitted work has a committed ledger row before success; crash-boundary tests enforce it.
2. Replay cannot duplicate settlement or billing; ledger uniqueness and lease fencing enforce it, with no exactly-once promise for external side effects.
3. No trim or cleanup removes the only recoverable copy of unfinished work.
4. Every multi-key operation shares a slot by construction or does not exist; the cluster suite refuses a new one.
5. A failed backend never silently selects another; topology is explicit and refused when unknown.
6. Both backends pass identical behaviour suites in one lane; an incomplete matrix fails the grader.
7. Owned local datastores alone receive resets and fault injection; shared endpoints are refused before mutation.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|---|---|---|---|---|---|
| Admission and replay outcomes | ops | Commit, append, receipt, replay | backend, outcome, counts, duration_ms | no payload, credential or tenant label | `test_queue_loss_replays_without_duplicate_settlement` |
| Backlog age, retained bytes, admission refusal | ops | Sampling and refusal | key class, reason, bytes | bounded dimensions, no fleet id | `test_capacity_report_accounts_for_all_retained_state` |
| Partition cursor, candidates, queue age | ops | Poll and delivery sampling | partition, counts, round trips | fixture ids only in result files | `test_skewed_workload_preserves_discovery_and_delivery_fairness` |
| Topology and hub reconciliation | ops | Boot, reconnect, `SUNSUBSCRIBE` | topology, node count, channels | URIs and credentials excluded | `test_cluster_connections_recover_without_missing_scoped_state` |

Exact names land in `observability.md` with the typed registry change in the same commit.

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts |
|---|---|---|---|
| 0.1 | integration | `test_sharded_subscription_survives_slot_movement` | Cross-seed delivery; delivery resumes after migration; no duplicate frame. |
| 0.2 | integration | `test_cluster_primitives_survive_movement` | Script and blocking read answer across a migration; two-slot script is `CROSSSLOT`. |
| 0.3 | integration | `test_durable_identity_survives_replay` | One logical event per identity after queue loss and reordered receipts. |
| 0.4 | integration | `test_partitioned_readiness_bounds_hotspots` | Every partition discovered inside the poll budget under a hot partition. |
| 1.1 | unit | `test_incomparable_datastore_runs_are_rejected` | Historical (PR #681): missing budget or changed seed fails grading. |
| 1.2 | unit | `test_shared_deployment_refuses_saturation_profile` | Historical (PR #681): oversized fixture opens no connection. |
| 1.3 | integration | `test_redis_baseline_records_complete_evidence` | Historical (PR #681): every manifest field populated. |
| 2.1 | integration | `test_acceptance_recovers_at_each_crash_boundary` | Each injected stop leaves zero missing admitted events. |
| 2.2 | integration | `test_queue_loss_replays_without_duplicate_settlement` | Rebuild yields zero losses and zero duplicate debits. |
| 2.3 | integration | `test_durable_store_failure_refuses_acceptance` | Database failure produces a retryable class and no acceptance. |
| 3.1 | integration | `test_retention_preserves_pending_work_under_pressure` | Pending entries survive trim; acknowledged history bounded. |
| 3.2 | integration | `test_full_datastore_applies_explicit_backpressure` | OOM and quota replies classified; no silent drop. |
| 3.3 | integration | `test_capacity_report_accounts_for_all_retained_state` | Streams, partitions, pending, replicas, ledger reported separately. |
| 4.1 | integration | `test_ready_races_preserve_work_and_bound_poll_cost` | Stale token cannot erase newer readiness; candidate cap holds. |
| 4.2 | integration | `test_skewed_workload_preserves_discovery_and_delivery_fairness` | Unrelated work stays inside the queue-age budget. |
| 4.3 | integration | `test_coordination_recovers_during_partition_movement` | Movement and worker loss preserve ordered recovery. |
| 5.1 | unit + integration | `test_datastore_preflight_refuses_invalid_configuration` | Each bad input refuses boot; unknown `REDIS_TOPOLOGY` refused. |
| 5.2 | integration | `test_cluster_resharding_preserves_atomic_append` | One physical entry per logical event across a migration. |
| 5.3 | integration | `test_cluster_connections_recover_without_missing_scoped_state` | Subscriptions and scans recover; no shared blocking socket. |
| 5.4 | integration | `test_session_transitions_preserve_single_use_on_both_backends` | Verify, approve, abort are single-use on both topologies. |
| 6.1 | e2e | `test_backends_preserve_application_outcomes` | Login, steer, lease, report, approval, stream agree on both backends. |
| 6.2 | integration | `test_backends_preserve_failure_and_session_semantics` | Error classes and one-time actions agree. |
| 6.3 | unit + integration | `test_backend_matrix_refuses_unsafe_or_incomplete_evidence` | Shared target, missing backend, leaked fixture fail. |
| 7.1 | integration | `test_provider_switch_and_rollback_preserve_accepted_work` | Source preserved through cancel, import, forward recovery. |
| 7.2 | integration | `test_missing_import_receipt_refuses_admission` | Missing or wrong receipt closes every admission path. |

New test functions follow the convention of the file they land in (`docs/architecture/testing.md`); a spec name that a file's convention rejects is amended here in the same commit.

## Acceptance Rubric (single scoring surface)

| # | Criterion | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|---|---|---|---|---|
| R1 | Prototype proofs archived | `make bench-datastore CHECK=prototype` | Exit 0; four proofs with revision and image digest. | P0 | |
| R2 | Accepted-work recovery | `make bench-datastore CHECK=durability` | Exit 0; missing admissions = 0; duplicate settlement = 0. | P0 | |
| R3 | Bounded retention and admission | `make bench-datastore CHECK=retention` | Exit 0; every budget passes. | P0 | |
| R4 | Fair coordination | `make bench-datastore CHECK=coordination` | Exit 0; queue-age and poll budgets pass. | P0 | |
| R5 | Backend matrix | `make bench-datastore CHECK=cluster` | Exit 0; Redis and local-cluster evidence present for every suite. | P0 | |
| S1 | Repository conformance | `make harness-verify` | Exit 0. | P0 | |
| S2 | Repository unit verification | `make test-unit-all` | Exit 0; coverage gates pass. | P0 | |
| S3 | Repository integration verification | `make test-integration-rustd` | Exit 0; both backends; nonzero passing count. | P0 | |
| S4 | Repository lint | `make lint-all` | Exit 0. | P0 | |
| S5 | Version consistency | `make check-version` | Exit 0. | P0 | |
| S6 | No committed secrets | `gitleaks git --no-banner` | Exit 0. | P0 | |

Graded cells stay empty until verification records a verdict with decisive output. Cloud Swarm rows belong to the follow-up spec and are not claimed by this one.

## Dead Code Sweep

`streams/once.rs` and `OnceScope` go with §2; `READY_INDEX_KEY` as a single hash goes with §4; global `PUBLISH` goes with §5. Each removal lists its references (RULE ORP) in the commit that removes it.

## Out of Scope

- Deploying, activating a Cloud trial, changing billing, live cutover, or retiring Redis; a standalone Dragonfly stage; a reverse migration.
- Replacing PostgreSQL, redis-rs, the runner protocol, or local identity caches; a generic cache abstraction.
- Exactly-once external effects; active-active regions; database disaster recovery.

## Product Clarity (authoring record)

1. **Successful user moment:** a fleet keeps answering during a datastore node loss, and nothing accepted is lost.
2. **Preserved user behaviour:** login, approvals, triggers, ordered fleet processing, live tail, billing and connector answers keep their documented meaning.
3. **Optimal-way check:** the live probe decided the design; each risk is prototyped before it is integrated.
4. **Rebuild-vs-iterate:** rebuild the acceptance and coordination boundaries; keep the crate surfaces and business semantics.
5. **What we build:** durable admission, slot-correct primitives, sharded hub, partitioned readiness, a two-backend lane, a strict grader, a rehearsal tool.
6. **What we do NOT build:** a second runtime, a benchmark stack, a dual-write, a provider flag inside business code.
7. **Fit with existing features:** M188 drivers, the existing integration lane, fenced settlement, the merged streaming runtime.
8. **Surface order:** operator Make targets and reports first; application behaviour verified through the existing API, dashboard and CLI suites.
9. **Dashboard restraint:** no scale badge before measured evidence exists.
10. **Confused-user next step:** the grader names the failing budget or missing run and links the playbook.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** prototype, then transport, then durability, then bounds, then coordination, then the matrix and rehearsal.
- **Alternatives considered:** hash-tagging the dedup key beside the stream (keeps a Redis claim as the identity and a second write path — rejected); a `REDIS_MODE` runtime switch in business code (two semantics forever — rejected); endpoint-only replacement (fails at the first `CROSSSLOT` — rejected by the probe).
- **Patch-vs-refactor verdict:** refactor at the boundaries the probe proved wrong; every Section keeps Redis green.

## Discovery (consult log)

- **Probe (2026-09-12, Dragonfly v1.40.2, four nodes, one container):** `append_once`'s two-key script → `CROSSSLOT`; `{fleet}` co-locates; `SPUBLISH`/`SSUBSCRIBE`/`HELLO 3` supported; `SSUBSCRIBE` on a non-owning node answered `OK` rather than `MOVED` (subscriber placement must be proven under migration); `DFLYCLUSTER CONFIG` migration of 4096 slots finished and moved keys; nodes require 256 MiB per proactor thread.
- **Scope decision (Indy, 2026-09-12):** start from a new worktree; the closed PR is historical reference; do not re-run the baseline; keep the spec's line cap from stalling the work.
- **Consults / Metrics review / Skill-chain outcomes / Deferrals:** recorded per Section.
