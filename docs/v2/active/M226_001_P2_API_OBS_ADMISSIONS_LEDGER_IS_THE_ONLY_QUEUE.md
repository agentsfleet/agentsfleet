<!--
SPEC AUTHORING RULES (load-bearing — the one comment that survives):
- Body order = the executing agent's read order. Fill via the orly-spec-new
  skill (authoring order lives there); after filling, DELETE every "tpl:"
  guidance comment — the SPEC TEMPLATE GATE blocks tpl residue, unfilled
  {slots}, and missing required sections (.orly/audits/spec-template.sh --staged).
- No time/effort/hour/day estimates anywhere. No effort columns, complexity
  ratings, percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only
  sequencing signal. A section that contradicts these rules loses — delete it.
-->

# M226_001: An admitted event is leased straight from the admissions ledger, with no Dragonfly copy of the queue

**Prototype:** v2.0.0
**Milestone:** M226
**Workstream:** 001
**Date:** Oct 13, 2026
**Status:** IN_PROGRESS
**Priority:** P2 — remove the second work queue and accept Save and run once the initial admission commits
**Categories:** API, OBS
**Batch:** B1 — one Pull Request; the queue cannot half-move, so the cutover, the deletions and the docs ship together
**Branch:** `feat/m226-admissions-ledger-queue`
**Baseline revision:** 0fa9e55356571d57df69cd3a13fb3b90930b0e49
**Test Baseline:** unit failed after 10,668 passed, one failed and 1,390 skipped/ignored; integration failed after 608 passed and one failed; both use the exact comparison revision
**Baseline evidence:** `docs/reviews/admissions-queue.md` records commands, isolated environment, partial selections and the two deadline failures; `/tmp/m226-baseline-unit-4.log` and `/tmp/m226-baseline-integration-2.log` retain raw output
**Depends on:** M225_001 — its doorbell (a sharded publish on every mark that wakes a parked lease poll) lands first; this spec moves the doorbell off the readiness index it deletes
**Provenance:** LLM-drafted (Claude Opus 5.5, Oct 13, 2026) from the adversarial review of the pickup path during M225_001 review round 2: three source maps of `feat/m225-lease-pickup-wake-hint` at `b25be9c72`, re-checked against `main` at `4a53b948c`
**Canonical architecture:** `docs/architecture/datastore_scaling.md` §The admissions ledger is the queue; `docs/architecture/runner_fleet.md` §Datastore topology; `docs/architecture/lease_flow.md`

---

## Overview

**Goal (testable):** `test_cycle_leaves_no_dragonfly_queue_keys_and_populates_rows` — an admitted event leases from `fleet.admissions`, and a full admit → lease → report cycle leaves no `fleet:{id}:events` stream and no `fleet:ready:` key in Dragonfly.
**Problem:** One fact, "this fleet has work", lives in four stores: the ledger row, the per-fleet stream entry, a readiness mark in one of sixteen hashes, and (since M225_001) a directory entry. No transaction spans Postgres and Dragonfly, so each gap has its own repairer: replay, reconcile with progress/scan/walk, cursor and group restore, the two-arm `MARK_DELIVERED`, the history view's duplicate merge, the reclaim sweeper's re-mark and token compare-and-delete clears. The user-visible cost: a failed stream append leaves a steer unseen until the replay sweeper takes rows older than 30 s on a 30 s pass (`rustd/crates/afd_runner/src/sweep/replay.rs:36,47`), and every new pickup feature has to keep four copies agreeing.
**Solution summary:** The ledger becomes the queue. A lease poll's claim locks the oldest unsettled row of an eligible fleet whose affinity slot is free (`FOR UPDATE SKIP LOCKED`), bumps the fence and stamps the row in one transaction; "has work" is a partial index, an expired lease re-qualifies its row by the same predicate, a park and a gate answer stamp and clear the row, and the report settles it inside the transaction it already runs. Dragonfly keeps the jobs where it copies nothing: the live tail and the doorbell over sharded publish, key-value, command-line login sessions, and the outbound queue. The readiness index, the inbound streams, and every repairer listed above are deleted.

## PR Intent & comprehension handshake

- **PR title (eventual):** refactor(lease): lease straight from the admissions ledger
- **Intent (one sentence):** a steer reaches a runner through one durable record, so no sweeper is needed to repair a copy of it and pickup has one place to get fast.
- **Handshake** — one committed admission will supply the runner's work and its recovery state; Dragonfly will announce new work without holding another copy. ASSUMPTIONS I'M MAKING: start with §1's bounded benchmark and additive queue columns; keep the serving selector on its existing queue until the measured decisions are recorded; preserve the current runner protocol, billing, placement and held-sandbox behavior.
- **First executable slice** — add the nullable queue stamps, then measure a benchmark-only ledger selector over the drain fixture. Its issued work runs the existing gate, lease and report code through `Plane::lease_claimed`; benchmark housekeeping marks a successfully reported row settled. This measures the proposed claim and idle probe, not the completed production settlement or crash-recovery design. The development cutover resets disposable authored drafts and fleets; it does not backfill old queue state.
- **Verification plan** — fresh-schema/runtime-role proof, competing claims and locked/parked-head proofs, then the 200-fleet drain and idle probe. Full declared baseline and final lanes remain due at the Pull Request boundary.
- **Quality ceiling** — one durable queue removes the recovery copies. The measured transaction cost decides whether the claim needs statement consolidation; no new queue service is proposed.
- **Surface check** — routes, response fields and command-line interface unchanged; regenerate OpenAPI for the corrected Save and run description. Architecture and release/version review are due at close; additive and receipt-removal migrations require the specific owner approval recorded in Discovery. Published documentation is outside the owner-approved repository scope.

## Implementing agent — read these first

1. `docs/architecture/runner_fleet.md` — §Datastore topology and the lease clear rules (drained, parked) this design must preserve.
2. `docs/architecture/datastore_scaling.md` — the ledger queue, admission budgets and Dragonfly's remaining roles.
3. `rustd/crates/afd_fleet/src/lease/sql/queue.rs` — `SELECT_CANDIDATES` and `CLAIM_HEAD`: discovery, slot fencing and the exact admission head.
4. `rustd/crates/afd_runner/src/sql/sweep/expire_abandoned_active_leases_batch.sql` — the repository's `FOR UPDATE SKIP LOCKED` shape, used by every replica's sweepers.
5. `rustd/crates/afd_fleet/src/lease/sql/meter.rs` — `METER_LEASE`, whose slot, lease and admission guards refuse a dead holder's late report; settlement shares the report transaction.

## Files Changed (blast radius)

Discovery grep, from repo root on `main`: `git grep -l -w -E 'ReadyIndex|ReadyCursor|READY_PARTITIONS|mark_ready|restore_group|delivered_cursor|XAUTOCLAIM|fleet_lease|fleet_ready_depth'`. Re-run it at CHORE(open); every hit not below is added before EXECUTE.

| File | Action | Why |
|------|--------|-----|
| `VERSION`, `cli/package.json`, `rustd/Cargo.toml`, `rustd/Cargo.lock` | EDIT | synchronize the owner-approved minor release, `0.65.0`, across the CLI and Rust workspace |
| `schema/2110_fleet_admissions_queue.sql` | CREATE | additive lease, park and settle stamps, the unsettled partial index and runtime grants |
| `schema/2120_fleet_admissions_drop_receipt.sql` | CREATE | drops admission `receipt`, `replay_count`, `replay_claim_expires_at`, their dependent indexes and lease `receipt` after their readers are removed (owner approval, Discovery Q3); prepare for review before registration |
| `scripts/check_architecture_doc_schema.py`, `scripts/check_architecture_doc_test_schema.sh` | EDIT | owner-approved schema-checker update applies ordered column additions and removals while retaining drift refusal and testing unsupported changes |
| `rustd/crates/afd_db/src/migration.rs` | EDIT | registers both migrations |
| `rustd/crates/afd_admission/src/{admit,budget/mod,lib,sql}.rs`, `budget/ceiling/mod.rs` | EDIT | admit commits the row and rings the doorbell; both backlog budgets count unsettled rows |
| `rustd/crates/afd_admission/src/{admit_receipt,replay,cursor,reconcile/mod}.rs`, `reconcile/` | DELETE | stream append, receipt write-back and the three repairers of it |
| `rustd/crates/afd_dragonfly/src/ready/mod.rs`, `ready/` | DELETE | the readiness index and partitions; the doorbell moves to its own module |
| `rustd/crates/afd_dragonfly/src/{streams/mod,lib,capacity,kv}.rs`, `streams/consume.rs` | EDIT | inbound append, group create and autoclaim go; the outbound stream and the tail keep theirs |
| `rustd/crates/afd_fleet/src/lease/{assign/mod,envelope,event,reclaim,store,settle,finalize}.rs`, `assign/`, `pull/held.rs`, `report/steps.rs`, `sql/{lease,report}.rs` | EDIT | the claim takes a ledger row; park, expiry and settle move the row; no XACK, trim or `MARK_DELIVERED` |
| `rustd/crates/afd_fleet/src/lease/{restore,mark}.rs` | DELETE | group restore and mark clears |
| `rustd/crates/afd_runner/src/sweep/{replay,reconcile/mod}.rs`, `sweep/reconcile/` | DELETE | sweepers of a copy that no longer exists |
| `rustd/crates/afd_runner/src/sweep/{mod,rebuild/mod,reclaim/mod,census/mod}.rs`, `sweep/rebuild/`, `sweep/reclaim/`, `sql/sweep.rs` | EDIT | rebuild keeps only the outbound pass; reclaim loses the stream probe and re-mark; census publishes the backlog gauge |
| `rustd/crates/afd_approval/src/inbox/resolve/mod.rs` | EDIT | an answer clears the park stamp and rings the doorbell |
| `rustd/crates/afd_gate/src/gate/{pause,store/mod}.rs` | EDIT | no readiness clear; `status = active` already gates the claim |
| `rustd/crates/afd_fleet_lifecycle/src/{install,purge,lib}.rs` | EDIT | no stream or group at install; no readiness clear at purge |
| `rustd/crates/afd_events/src/history/queued/mod.rs`, `history/statement/queued.rs`, `src/sql.rs` | EDIT | queued steers read unleased ledger rows; the duplicate merge goes |
| `rustd/crates/afd_observability/src/metrics/declared/fleet.rs`, `src/producers/fleet/mod.rs`, `docs/metrics.census.tsv`, `playbooks/operations/observability/assets/dashboard.json` | EDIT | `agentsfleet_fleet_ready_depth` goes; the backlog gauge changes producer |
| `rustd/crates/afd_bench/src/lane/{lease/mod,steer/mod,cardinality/mod,sweep}.rs` and their directories | EDIT | §1 ledger-claim variant; lanes stop seeding or counting readiness |
| `rustd/crates/afd_bench/src/bin/lease.rs`, `rustd/crates/afd_bench/Cargo.toml`, `rustd/crates/afd_bench/tests/ledger_claim.rs` | EDIT / CREATE (test) | prove the production ledger drain, sweep database fixtures, and refuse incomplete measurements |
| `rustd/crates/afd_bench/src/lane/lease/saturation/mod.rs` | EDIT | adapt the upstream saturation viewer to the database-only fleet lifecycle constructor |
| `rustd/crates/afd_bench/tests/abort_wiring.rs` | EDIT | reuse the bounded unreachable database fixture so failed ledger polls exercise the abort threshold |
| `rustd/crates/afd_bench/src/error/{kind,pre_flight}.rs` | EDIT | refuse incomplete drains instead of publishing zero cost after an abort; classify them as failures after fixture creation |
| `rustd/crates/afd_db/tests/integration_admissions_queue.rs`, `rustd/crates/afd_db/tests/db_suite.rs` | CREATE / EDIT (suite) | prove the additive migration and runtime role against the actual schema |
| `rustd/crates/afd_fleet/tests/integration_ledger_queue.rs`, `integration_ledger_queue/`, `rustd/crates/afd_fleet/tests/fleet_suite.rs` | CREATE / EDIT (suite) | prove competing replicas, parked/locked head ordering, reclaim and transactional rollback without a working Dragonfly |
| `rustd/crates/afd_fleet/tests/support/{fleet_fixtures,fleet_lease_seed,fleet_lease_reads,fleet_report_seed,fleet_report_reads}.rs` | EDIT | share the admission settlement read and remove obsolete stream, receipt and readiness descriptions from the ledger fixtures |
| `rustd/crates/afd_fleet/src/lease/sql/meter.rs`, `sql/meter_lease.sql`, and `sql/lease/*.sql` | EDIT / CREATE | current settlement implementation and domain SQL files introduced since the draft |
| `rustd/crates/afd_fleet/src/lease/{affinity,reclaim}.rs` | EDIT | reuse the existing slot and expired-lease operations on the ledger claim's transaction connection |
| `rustd/crates/afd_bench/src/lane/lease/drain/{mod,runner,settled,stage,platform,summary,window,tests}.rs`, `drain/sql/` | EDIT / CREATE | measure the production ledger claim and settlement, count residual admissions, and refuse incomplete drains |
| `rustd/crates/afd_db/tests/integration_migrate/shape.rs`, `docs/architecture/postgres_schemas.md` | EDIT | keep the existing complete schema inventory aligned with the additive columns |
| `rustd/crates/afd_fleet/src/lease/sql/{mod,queue}.rs` | EDIT / CREATE | typed statement registry for the ledger claim prototype |
| `rustd/crates/agentsfleetd/src/plane/{mod,tests}.rs`, `src/serve/optional.rs` | EDIT | composition-root references to the independent doorbell |
| `docs/architecture/{README,testing}.md`, `docs/architecture/scenarios/{two-fleets-github-slack,github-pr-reviewer}.md`, `playbooks/operations/teardown/dragonfly/{001_playbook.md,03_verify.sh}` | EDIT | live queue references and the GitHub scenario's renamed admission heading |
| tests under `rustd/crates/{afd_dragonfly,afd_fleet,afd_runner,afd_approval,afd_events,afd_admission,agentsfleetd}/tests/` that the discovery grep names | EDIT / DELETE | readiness and stream suites go; lease, reclaim and approval suites assert ledger state |
| `rustd/crates/afd_admission/src/{stage/,sql/,error/,confirm.rs,backlog.rs,wake.rs}` | EDIT / CREATE / DELETE | ledger-only acceptance, bounded budgets, durable confirmation and obsolete recovery removal |
| `rustd/crates/afd_authoring/src/creation/start/confirm.rs` | EDIT | activation confirms the committed admission identity without a stream receipt |
| `rustd/crates/afd_fleet/src/lease/{mod.rs,pull/,sql/,park.rs}`, `rustd/crates/afd_fleet/src/error/` | EDIT / CREATE / DELETE | fenced queue transitions and removal of inbound queue errors and adapters |
| `rustd/crates/afd_dragonfly/src/{doorbell/,streams/,lib.rs,error/}`, `rustd/crates/afd_approval/src/{sql/,inbox/resolve/}` | EDIT / CREATE / DELETE | independent sharded notification and durable gate unpark |
| `rustd/crates/afd_authoring/src/creation/{activate,system,progress,stored}.rs`, `creation/sql/`, and affected authoring tests | EDIT / DELETE | activation needs no inbound stream setup; preserve committed initial-run acceptance |
| `rustd/crates/agentsfleetd/src/sweepers/mod.rs`, `rustd/crates/afd_runner/src/sql/sweep/`, `rustd/crates/afd_runner/src/sweep/reclaim/` | EDIT / DELETE | remove the inbound-only sweeper and its supervised task and query inventory |
| `rustd/crates/afd_observability/src/{metrics/label/fleet.rs,producers/fleet/,metrics/label/tests.rs}`, affected observability tests | EDIT / DELETE | accepted ledger outcomes and removal of obsolete queue/replay producers |
| `rustd/crates/afd_events/src/history/{mod.rs,statement/}`, `rustd/crates/afd_dragonfly/src/test_util.rs` | EDIT / DELETE | one statement snapshot for queued and delivered history; remove inbound test adapters |
| `docs/architecture/{runner_fleet,datastore_scaling,lease_flow,scaling,data_flow,concurrency,observability}.md` | EDIT | one queue, its accepted-admission labels and backlog measurements, described as the design |
| `docs/architecture/measurements/admissions-queue/*.json` | CREATE | retain the measured stream comparison and ledger prototype with their counts and rig parameters |
| `rustd/crates/afd_fleet_lifecycle/Cargo.toml`, `rustd/crates/agentsfleetd/src/plane/stores.rs`, lifecycle and API test fixtures constructing `Fleets` | EDIT | remove the purge store's unused Dragonfly dependency and constructor parameter |
| `rustd/crates/afd_bench/src/bin/{tail,cardinality,steer}.rs` | EDIT | fixture sweep no longer requires an inbound queue handle |
| `rustd/crates/afd_authoring/src/{creation/mod.rs,error/mod.rs}`, `rustd/crates/agentsfleetd/src/plane/surfaces.rs`, API authoring constructor fixtures | EDIT | remove stream-only setup state and its unreachable failure |
| `rustd/crates/afd_events/src/lib.rs`, `rustd/crates/afd_events/src/history/queued/` | EDIT / DELETE | export the unified history read texts and remove the two-query merge |
| `rustd/crates/afd_authoring/Cargo.toml` | EDIT | Dragonfly is now only a test fixture dependency of authoring |
| `rustd/crates/afd_fleet/Cargo.toml`, `rustd/crates/afd_fleet/src/lease/pull/tests.rs` | EDIT | remove unused stream decoding dependencies; terminal settlement failures propagate |
| `rustd/crates/agentsfleetd/src/serve/mod.rs`, `rustd/crates/afd_api/tests/harness/fleet/mod.rs`, `harness/fleet_seams.rs` | EDIT | remove queue arguments and unused clones from callers of stores that now use only Postgres |
| `rustd/crates/afd_api/tests/authoring/saving/` | EDIT | initial-run fixtures assert committed admissions without inbound stream cleanup |
| `rustd/crates/afd_admission/Cargo.toml`, `rustd/crates/afd_admission/tests/` | EDIT / DELETE | ledger acceptance, notification failures and concurrent backlog budgets replace receipt repair tests |
| `rustd/crates/afd_dragonfly/src/outbound/` | EDIT | remove comments referring to deleted inbound stream operations; outbound behavior is unchanged |
| `rustd/crates/afd_runner/Cargo.toml`, `rustd/crates/afd_runner/tests/integration_census.rs` | EDIT | remove unused replay test dependencies and prove census backlog publication and withdrawal |
| `rustd/crates/afd_api/tests/integration_slack_mention/resident.rs`, `rustd/crates/afd_api/tests/slack_mention_live/fixture.rs` | EDIT | prove resident installation and admission survive a lost Dragonfly wake; remove obsolete stream-creation failure assumption |
| `rustd/crates/afd_fleet_ops/tests/integration_runner_leases{,_retained}.rs`, `rustd/crates/afd_authoring/tests/draft/admission/fixture.rs` | EDIT | remove obsolete runtime lease receipt columns from retained-history and occupancy fixtures |
| `rustd/crates/afd_gate/tests/` | EDIT / DELETE | remove the readiness-clear failure expectation after pause becomes a database-only transition |
| `rustd/crates/afd_api/tests/authoring/{fixture,mod}.rs`, `rustd/crates/agentsfleetd/tests/integration_runner_shapes.rs` | EDIT | remove unused receipt-refusal fixture wiring and the deleted lease receipt shape field |
| `rustd/crates/afd_dragonfly/src/preflight/`, `rustd/crates/afd_api_tenant/src/handler/fleet/native/create.rs`, `public/openapi.json` | EDIT | correct queue documentation and regenerate the API description without changing response shapes |
| `rustd/crates/afd_dragonfly/Cargo.toml`, `rustd/crates/afd_dragonfly/src/{error/,namespace/mod.rs}`, `rustd/crates/afd_dragonfly/tests/error_surface.rs` | EDIT / DELETE | remove token-minting errors and the crypto dependency left by the deleted readiness index |
| `rustd/crates/afd_api/tests/integration_workspace_approvals.rs` | EDIT | give concurrently running approval cases distinct unique user subjects |
| `rustd/crates/agentsfleetd/tests/support/{e2e,e2e_seed}.rs`, `support/definition_install/mod.rs`, `integration_sweepers.rs`, `integration_runner_e2e/{mod,labels,money_gate,refusals}.rs`, `rustd/crates/afd_approval/src/inbox/sweep.rs`, `rustd/crates/afd_fleet/src/lease/sql/tool_detail.rs` | EDIT | align placement fixture isolation and comments with the ledger queue; preserve operator label placement coverage |
| `playbooks/operations/observability/assets/alerts.json`, `playbooks/operations/observability/lib.sh` | EDIT / DELETE | remove the retired readiness alert and replay-floor rendering that reads the deleted sweeper |
| `docs/reviews/admissions-queue.md` | CREATE | retain baseline counts, failure coverage, concurrency and resource evidence, measurements and review outcomes |
| `rustd/crates/afd_dragonfly/src/streams/render.rs`, `rustd/crates/afd_dragonfly/tests/keys_and_config/mod.rs` | DELETE / EDIT | remove stream rendering and its sample test after their production consumer is removed |
| `rustd/crates/afd_dragonfly/tests/integration_doorbell.rs`, `rustd/crates/afd_dragonfly/tests/datastore_suite.rs` | CREATE / EDIT | prove cross-process sharded notification delivery and release every subscription at shutdown |
| `rustd/crates/afd_fleet/tests/integration_lease_plan.rs`, `integration_lease_plan/` | EDIT / CREATE | prove composite prior-event lookup, bounded head reads, indexed renewal/settlement and skipped fleet scans on an empty ledger with 20,001 active fleets |
| `rustd/crates/afd_events/tests/queued/{mod.rs,snapshot.rs}` | EDIT / CREATE | count actual public history statements before and after delivery; one statement supplies one snapshot |
| `rustd/crates/afd_authoring/tests/draft/creation/{mod,confirmation}.rs`, `rustd/crates/afd_fleet/tests/integration_lease_gates/faults.rs`, `integration_terminal_redelivery.rs` | EDIT / CREATE | prove durable initial-event confirmation, preservation of terminal refusals and stale terminal-settlement fencing |

## Applicable Rules

- **`.orly/docs/greptile-learnings/RULES.md`** — NDC (every repairer is deleted, not left idle), NLR (touched readiness callers are cleaned, not wrapped), ORP (cross-layer orphan sweep over keys, metrics, docs and the dashboard), NSQ and STS (named, schema-qualified statements; no literal state strings in the migration), SGR (grants in the new migration), UFS (column and key names as constants).
- **`.orly/dispatch/write_rust.md`** and **`docs/RUST_ERROR_STANDARD.md`** — removed stream-failure error kinds leave no dead `ErrorKind` variants; new fallible paths use `error_shell!`.
- **`.orly/dispatch/write_sql.md`** — `VERSION` is `0.62.2`, so every schema change is a forward migration and obsolete queue-column removal needs owner approval for that specific change.
- **`.orly/docs/SCHEMA_CONVENTIONS.md`** — column, index and grant shape of the two migrations.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| SCHEMA GUARD | yes — two new `schema/*.sql` files and the migration array | forward migrations only; the drop waits for Q3's approval quote in Discovery |
| Architecture consult | yes — deletes the `fleet:ready:` and `fleet:{id}:events` key families and changes a Postgres schema | §6's doc edits land in the same commit as the code that changes each fact |
| UFS | yes — new column names and the doorbell channel | named constants beside the statements that use them |
| LOGGING | yes — the backlog gauge moves producer; removed sweeper events | event names from the logging standard; removed events leave no orphan reference |
| ERROR REGISTRY | yes — stream-append and group-restore failures disappear | delete their variants with their callers; no reserved codes left dangling |
| File & Function Length (≤350/≤50/≤70) | yes — `assign.rs`, `sql/lease/mod.rs`, `admit.rs` | split the ledger claim into `assign/ledger.rs` if `assign.rs` nears the cap |

## Prior-Art / Reference Implementations

- **Reference:** `rustd/crates/afd_runner/src/sql/sweep/expire_abandoned_active_leases_batch.sql` — the in-repo `FOR UPDATE SKIP LOCKED` claim every replica runs safely; the lease claim mirrors its lock-then-stamp shape.
- **Reference:** `rustd/crates/afd_fleet/src/lease/sql/meter_lease.sql` — fence-guarded report settlement; the admission stamp shares its transaction with billing and narrative updates.

## Sections (implementation slices)

### §1 — Measure the ledger claim before cutting

A bench variant of `make bench-lease` runs the §3 claim against a seeded ledger and reports leases per second, Postgres round trips per lease and the idle probe's cost, beside the current contended row (1,928 leases/s, 2.4 round trips, `docs/architecture/scaling.md:272`) and idle row (1.00 Dragonfly command, 0 Postgres statements, `scaling.md:271`). Completion: both figures recorded in `scaling.md` and Indy's answers to Q1 and Q2 quoted in Discovery. Missing either blocks §3–§5. **Implementation default:** the variant reuses the drain lane's seeding because it already proves every event processed once.

- **Dimension 1.1** — the ledger-claim lane runs and reports round trips per lease → Test `test_ledger_claim_lane_reports_round_trips`
- **Dimension 1.2** — figures recorded and Q1/Q2 answered → Test `manual_ledger_claim_figures_recorded`

### §2 — The ledger carries queue state

A forward migration adds three nullable stamps to `fleet.admissions`: the fence of the lease holding the row, when it parked on a gate, and when its report settled it. A partial index on `(fleet_id, created_at, sequence)` covers unsettled rows. The owner confirmed this is a development deployment with disposable authored drafts and fleets. The cutover stops old readers and admission, resets that development data, applies the forward migrations and starts only ledger readers. No populated-history backfill is required.

- **Dimension 2.1** — fresh bootstrap creates the stamps, index and grants → Test `test_admissions_queue_migration_bootstraps`
- **Dimension 2.2** — the development reset removes old fleet queue state before ledger readers start → Test `manual_development_cutover_reset`

### §3 — The claim takes the work

Admission commits the ledger row, then rings the doorbell; no append, no mark. A poll's candidate scan reads eligible fleets from the unsettled index (fleet `active`, slot free or expired, oldest unsettled row not parked). The claim then, in one transaction, wins the slot, bumps `fencing_token`, locks that fleet's oldest unsettled row with `FOR UPDATE SKIP LOCKED` and stamps the fence on it. `Acquired` is built from the row; `redelivered` is `delivered_at IS NOT NULL`. Gates and issue keep today's order after the claim and stay fence-guarded. Both backlog budgets count unsettled rows, the per-fleet one bounded by `LIMIT 10001`.

- **Dimension 3.1** — an admitted event is leased from the ledger → Test `test_cycle_leaves_no_dragonfly_queue_keys_and_populates_rows`
- **Dimension 3.2** — two replicas claiming the same fleet: one lease, one empty answer → Test `test_concurrent_claims_lease_one_row`
- **Dimension 3.3** — a fleet's rows lease in `(created_at, sequence)` order, and a parked oldest row holds the rows behind it → Test `test_fleet_rows_lease_in_order_behind_a_park`
- **Dimension 3.4** — a re-claimed row is marked redelivered → Test `test_reclaimed_row_is_leased_as_redelivered`
- **Dimension 3.5** — the fleet budget refuses at 10,000 unsettled rows → Test `test_fleet_budget_counts_unsettled_rows`

### §4 — Report, park, answer and expiry move the row

`METER_LEASE` stamps `settled_at` in the report transaction, under its fence guard. A terminal refusal or block settles through the same admission fence. A park stamps the row and frees the slot; a gate answer clears the stamp and rings the doorbell. An expired lease needs no queue sweeper: the expired slot and unsettled row requalify by the §3 predicate. Every poll probes Postgres, so a lost doorbell leaves the event available on the next retry after `NO_WORK_RETRY_AFTER`.

- **Dimension 4.1** — settle stamps the row in the report transaction; a rollback leaves it unsettled → Test `failed_admission_settlement_rolls_back_the_report_and_retry_settles_once`
- **Dimension 4.2** — an expired lease is re-leased and the dead holder's late report is refused → Test `test_reclaimed_row_is_leased_as_redelivered`; superseded-report proof is also listed below
- **Dimension 4.3** — park holds later work, and gate resolution and expiry durably unpark → Test `test_approval_resolution_leases_next_poll`; ordering, expiry and lock-wait proofs are also listed below
- **Dimension 4.4** — a lost doorbell still leases on the next probe → Test `test_a_fleet_is_discovered_without_any_notification`; stale-hint proof is also listed below

### §5 — Delete the Dragonfly queue and its repairers

The readiness index, its partitions and the M225_001 directory if it still exists; the inbound stream and its consumer group; group restore and the delivered cursor; replay, reconcile and its progress/scan/walk; the reclaim sweeper's stream probe and re-mark; the pause and purge clears; `MARK_DELIVERED`'s second arm and the history view's duplicate merge. Rebuild keeps only the outbound pass. The second migration drops `receipt` and its two indexes after Q3.

- **Dimension 5.1** — a full admit → lease → report cycle leaves no inbound stream or readiness key → Test `test_cycle_leaves_no_dragonfly_queue_keys_and_populates_rows`
- **Dimension 5.2** — the receipt drop applies on a populated ledger → Test `test_receipt_drop_migration_applies_populated`
- **Dimension 5.3** — a queued steer shows once in history, from the ledger → Test `test_thread_read_includes_queued_steers`; delivery and retry identity proofs are also listed below
- **Dimension 5.4** — the census pass publishes and withdraws the admission backlog → Test `test_census_publishes_every_status`

### §6 — Architecture docs describe one queue

`runner_fleet.md`, `datastore_scaling.md`, `lease_flow.md`, `scaling.md`, `data_flow.md` and `concurrency.md` describe the ledger as the queue and Dragonfly's remaining roles. The scaling rows are re-measured, not edited by hand.

- **Dimension 6.1** — no live source or operational document requires the deleted queue; historical measurements and negative assertions retain their original names → Test `manual_source_documentation_audit`

## Interfaces

```
HTTP: unchanged. POST /v1/runners/me/leases, the reports route and their wire shapes keep every field.
fleet.admissions       + lease_fence BIGINT NULL          fence of the lease holding the row
                       + parked_at   TIMESTAMPTZ(3) NULL  set by a park, cleared by the gate answer
                       + settled_at  TIMESTAMPTZ(3) NULL  set by the report transaction, never cleared
                       - receipt (§5, after Q3)
queue predicate        settled_at IS NULL; a fleet is eligible when its oldest such row has parked_at IS NULL
doorbell               M225_001's sharded publish, rung after the admission commit and the gate answer
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Concurrent claim | two replicas poll the same fleet | slot claim and `SKIP LOCKED` let one win; the other answers no-work and the loser's poll moves on |
| Dead holder | runner dies mid-lease | slot expiry re-qualifies the row; the new claim bumps the fence; the late report is refused by the existing guard |
| Lost doorbell | publish dropped, hub reconnecting | the parked poll's probe finds the row within one retry interval |
| Report rollback | report transaction aborts | `settled_at` stays null; the row is re-leased after expiry; billing stays single because the settle and the charge share the transaction |
| Park without answer | a person never answers | row stays parked and holds its fleet, as today; the inbox expiry sweep answers it |
| Mixed queue readers at cutover | old stream readers remain active when ledger readers start | stop old readers and admissions, reset disposable development drafts and fleets, apply migrations, then start only ledger readers; overlapping readers and populated-history upgrades are unsupported |
| Postgres unavailable | database outage | admission already fails closed on the ledger insert; polls answer an error instead of a stale Dragonfly offer |

## Invariants

1. A ledger row leases only under the fence its claim minted — enforced by the claim transaction stamping `lease_fence` from the `fencing_token` it bumped, and by `METER_LEASE`'s guard.
2. One unsettled row per fleet is leased at a time — enforced by the affinity slot claim preceding the row lock in the same transaction.
3. A settled row never leases again — enforced by the claim statement's `settled_at IS NULL` predicate. Production admission settlement has two writers: `meter_lease.sql` for reports and `settle_terminal.sql` for refusal or block. Both check the admission fence; neither clears settlement.
4. No Dragonfly write sits between the admission commit and the caller's answer except the best-effort doorbell — enforced by `admit` returning after the commit and logging, never propagating, a doorbell failure.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| `agentsfleet_fleet_ready_depth` | ops | removed with the index it sampled | — | — | source and asset audit |
| `agentsfleet_admission_backlog`, `agentsfleet_admission_backlog_oldest_age_seconds` | ops | each census pass, from the unsettled index (was the replay sweeper) | row count, oldest age | counts only | `test_census_publishes_every_status` |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | integration | `test_ledger_claim_lane_reports_round_trips` | a 200-fleet seeded drain through the ledger claim reports a nonzero leases/s and a round-trip figure, every event processed once |
| 1.2 | manual | `manual_ledger_claim_figures_recorded` | Indy reads the §1 figures in `scaling.md`; his Q1/Q2 answers are quoted in Discovery |
| 2.1 | integration | `test_admissions_queue_migration_bootstraps` | empty database → three nullable stamps, the partial index and the runtime role's grants exist |
| 2.2 | manual | `manual_development_cutover_reset` | old readers stopped and disposable development fleets removed before the ledger build starts |
| 3.1 | integration | `test_cycle_leaves_no_dragonfly_queue_keys_and_populates_rows` | admitted steer → lease → heartbeat → report; queue stamps and history exist, no inbound Dragonfly queue key |
| 3.1 | integration | `prior_lease_lookup_uses_both_event_index_keys_for_fresh_and_reclaimed_heads` | 4,000 retained events and leases → fresh and reclaimed claims constrain fleet and event identity; prepared renewal and settlement plans at 1,250–10,000 admissions → one primary-key row per executed lookup |
| 3.1 | integration | `an_empty_ledger_skips_fleet_scans_at_twenty_thousand_idle_fleets` | 20,001 active fleets, no unsettled admissions → production discovery executes zero fleet scans |
| 3.1 | integration | `backlog_growth_does_not_expand_each_fleets_head_probe` | 2,000 through 16,000 admissions → one indexed head row read for the eligible fleet |
| 3.2 | integration | `test_concurrent_claims_lease_one_row` | 100 barrier-started callers across two bounded pools; at least four SQL claims reach the contention barrier together → one lease and one fence bump |
| 3.3 | integration | `test_fleet_rows_lease_in_order_behind_a_park` | rows A, B, C with A parked → no lease; answer A → A, then B, then C |
| 3.4 | integration | `test_reclaimed_row_is_leased_as_redelivered` | lease A, expire the slot, poll → A again with `redelivered = true` |
| 3.5 | integration | `test_fleet_budget_counts_unsettled_rows` | 10,000 unsettled rows → the next admit is refused `over_budget`; settle one → admitted |
| 3.5 | integration | `first_delivery_finishes_while_a_producer_holds_the_fleet_budget_lock` | an uncommitted public admission holds the fleet budget lock → first delivery commits before that producer releases it |
| 3.1 | integration | `a_failed_delivery_stamp_rolls_back_narrative_and_counters_then_retries_once` | stamp failure → no narrative, stamp or counter change; optional counter-read failure → delivery commits; retry and repeat add one event total |
| 4.1 | integration | `failed_admission_settlement_rolls_back_the_report_and_retry_settles_once` | injected statement failure → no charge or settlement; restored boundary → one settlement, repeat adds nothing |
| 4.1 | integration | `a_report_cannot_settle_an_admission_with_a_different_fence` | superseded admission fence → no report charge or settlement |
| 4.2 | integration | `test_reclaimed_row_is_leased_as_redelivered` | expired slot → higher fence and original event with redelivery set |
| 4.2 | integration | `test_a_superseded_report_commits_nothing` | displaced holder → no report or billing effects |
| 4.3 | integration | `test_approval_resolution_leases_next_poll` | answer a parked gate → its admission leases on the next poll |
| 4.3 | integration | `test_fleet_rows_lease_in_order_behind_a_park` | parked head blocks its fleet; after resolution the original event leases before its continuation |
| 4.3 | integration | `an_expired_gate_wakes_its_fleet` | expired gate → durable park clears and the fleet is available |
| 4.3 | integration | `an_answer_during_the_park_lock_wait_cannot_strand_the_admission` | answer commits during the park lock wait → no stale park; the admission leases |
| 4.3 | integration | `answers_and_expiry_clear_a_park_that_was_uncommitted_when_resolution_started` | answer and expiry start after an uncommitted park write → resolver waits, clears the committed park, and the same head leases |
| 4.4 | integration | `test_a_fleet_is_discovered_without_any_notification` | absent hint → the Postgres probe finds the unnotified event |
| 4.4 | integration | `test_a_stale_notification_does_not_hide_unnotified_work` | stale hint → the Postgres probe still finds other available work |
| 5.1 | integration | `test_cycle_leaves_no_dragonfly_queue_keys_and_populates_rows` | complete cycle, then `SCAN` for `fleet:ready:` and `:events` → 0 keys |
| 5.2 | integration | `test_receipt_drop_migration_applies_populated` | populated ledger → drop applies; leases still flow |
| 5.3 | integration | `test_thread_read_includes_queued_steers` | one admitted steer → one queued history identity before delivery |
| 5.3 | integration | `test_thread_read_dedupes_leased_steer` | delivered steer → one narrative history identity |
| 5.3 | integration | `test_retry_keeps_one_queued_message_and_its_delivered_identity` | retry before delivery, then deliver → one stable message identity |
| 5.3 | integration | `thread_pages_use_one_statement_snapshot_before_and_after_delivery` | public history calls before delivery, after delivery and on an empty cursor page → one statement each and one message identity |
| 5.4 | integration | `test_census_publishes_every_status` | 3 unsettled rows → backlog gauge 3 and oldest age ≥ 0; a failed sample withdraws stale gauges |
| 3.1 | integration | `a_missing_confirmation_keeps_the_saved_identity_until_the_same_key_answers`, `a_confirmation_for_another_event_cannot_accept_the_initial_run`, `a_confirmation_outage_is_logged_and_a_retry_accepts_without_another_admission` | missing, mismatched or unavailable confirmation → pending with the original fleet and event; restored confirmation → accepted once |
| 4.2 | integration | `terminal_redelivery_cannot_settle_after_its_claim_is_superseded` | fence changes after delivery → stale terminal settlement retains the current claim; expiry permits settlement |
| 4.2 | integration | `an_unreadable_redelivery_preserves_an_existing_terminal_ending` | unreadable redelivery → original terminal ending remains, admission settles without another history row |
| 4.3 | integration | `a_stale_claim_cannot_park_or_release_its_replacement` | stale holder parks after reclaim → current admission and slot remain held; current holder can park |
| 4.4 | unit | `gaps_and_lag_are_counted_without_losing_the_next_valid_hint` | lag and reconnect gap → two gap counts; malformed hints ignored, next valid hint delivered, subscription released |
| 6.1 | manual | `manual_source_documentation_audit` | no runtime dependency on the removed queue; historical comparison data and absence assertions are explicitly retained |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Work leases from the ledger alone (§3) | `make test-integration-rustd` | `test_cycle_leaves_no_dragonfly_queue_keys_and_populates_rows` passed | P0 | |
| R2 | Expiry, park and lost doorbell lose no event (§4) | `make test-integration-rustd` | reclaim, superseded-report, parked-head, resolution, expiry and notification-loss tests passed | P0 | |
| R3 | No Dragonfly queue remains (§5) | `git grep -n -w -E 'ReadyIndex\|ReadyCursor\|READY_PARTITIONS\|restore_group\|delivered_cursor\|XAUTOCLAIM' -- 'rustd/crates/*/src/**'` | 0 runtime matches; documentation audit recorded separately | P0 | |
| R4 | Ledger claim measured and Q1–Q4 answered (§1) | manual — Discovery quotes and `scaling.md` rows | Q1–Q4 answers quoted, including combined Q1/Q2; two re-measured rows | P0 | |
| R5 | Diff stays inside Files Changed | `git diff --name-only origin/main` | 0 paths missing from the Files Changed table | P0 | |
| S1 | Conform gates green | `make harness-verify` | exit 0 | P0 | |
| S2 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S3 | Lint green | `make lint-all` | exit 0 | P0 | |
| S4 | Integration green | `make test-integration-rustd` | exit 0 | P0 | |
| S5 | Version in sync | `make check-version` | exit 0 | P0 | |
| S6 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S7 | Rust coverage meets the owner's requirements | `make test-coverage-rustd` | global ≥99.5%; changed lines ≥99%; each changed Rust source file ≥99%, with no added exclusions | P0 | |

**Command source rule:** copy every declared `conform` and `verify.*` invocation from `.orly/orly.json` into a Verify cell, verbatim, with an Expected value. See `.orly/dispatch/lifecycle.md` for command timing; baseline metadata is pending at opening and measured before the Pull Request.

**Grading protocol (VERIFY):** run each spec-specific Verify command verbatim; Graded = ✅/❌ + one decisive output line. Repository-command rows point to the final `orly gate pr` results in Pull Request Session Notes. **Ship gate:** every required check must pass before the Pull Request is ready; missing evidence or any ❌ returns to EXECUTE. A P0 may be MOVED only under the transfer rule in `.orly/docs/TEMPLATE.md`.

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

| File to delete | Verify |
|----------------|--------|
| `rustd/crates/afd_dragonfly/src/ready/mod.rs` | `test ! -f rustd/crates/afd_dragonfly/src/ready/mod.rs` |
| `rustd/crates/afd_admission/src/replay/mod.rs` | `test ! -f rustd/crates/afd_admission/src/replay/mod.rs` |
| `rustd/crates/afd_admission/src/reconcile/mod.rs` | `test ! -f rustd/crates/afd_admission/src/reconcile/mod.rs` |
| `rustd/crates/afd_admission/src/cursor.rs` | `test ! -f rustd/crates/afd_admission/src/cursor.rs` |
| `rustd/crates/afd_fleet/src/lease/restore.rs` | `test ! -f rustd/crates/afd_fleet/src/lease/restore.rs` |
| `rustd/crates/afd_runner/src/sweep/replay.rs` | `test ! -f rustd/crates/afd_runner/src/sweep/replay.rs` |

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| readiness and stream-queue types | `git grep -n -w -E 'ReadyIndex\|ReadyCursor\|READY_PARTITIONS\|mark_ready\|restore_group\|delivered_cursor' -- 'rustd/crates/*/src/**'` | 0 runtime matches |
| `agentsfleet_fleet_ready_depth` | `git grep -n agentsfleet_fleet_ready_depth -- rustd playbooks/operations/observability docs/metrics.census.tsv` | 0 runtime or operational matches |
| `MARK_DELIVERED`, `RECORD_RECEIPT` | `git grep -n -w -E 'MARK_DELIVERED\|RECORD_RECEIPT' -- 'rustd/crates/*/src/**'` | 0 runtime matches; test-local fixtures may stamp delivery |
| admission settlement writers | `git grep -n 'settled_at =' -- rustd/crates/afd_fleet/src/lease/sql` | only `meter_lease.sql` and `lease/settle_terminal.sql` |

## Out of Scope

- The outbound queue (`connector:outbound`): it stays a Dragonfly stream; its head-of-line row in `scaling.md` is a separate spec.
- Ledger retention: no sweeper deletes `fleet.admissions` rows today (`grep -rn fleet.admissions rustd/crates/afd_runner/src` finds none); the partial index keeps the claim off settled rows, and retention stays as it is.
- The report path's five-file split (report, steps, commit, settle, finalize) and the fence rule's eight spellings: review findings, not this queue move.
- Name collisions (`Admission`, `Delivery`, `Verdict`, `claim`, `session`): a rename spec of their own.

---

## Product Clarity (authoring record)

1. **Successful user moment** — a fleet owner sends a steer during a Dragonfly node restart and it still runs, once, without waiting for a 30 s sweeper.
2. **Preserved user behaviour** — every producer, the lease and report routes, per-fleet order, parked gates holding their fleet, sticky routing and held sandboxes.
3. **Optimal-way check** — one durable record is the direct shape; the gap is idle polls costing a Postgres probe when the doorbell is silent, sized by §1.
4. **Rebuild-vs-iterate** — rebuild of the queue only; the lease, gate and report code is iterated in place.
5. **What we build** — two migrations, the ledger claim, the park and settle stamps, the census backlog gauge, the bench variant.
6. **What we do NOT build** — a new queue service, an outbound move, a report-path refactor, renames.
7. **Fit with existing features** — compounds with M225_001's doorbell; must not destabilize the held-sandbox routing on `runner.fleet_slots`.
8. **Surface order** — Save and run accepts the committed initial admission; retrying Save keeps its fleet and initial event. Routes and response fields stay the same.
9. **Dashboard restraint** — use the existing backlog gauge for queued work and remove the readiness panel; replica samples use `max`, never a sum of the same deployment backlog.
10. **Confused-user next step** — inspect the fleet history for queued work and `agentsfleet_admission_backlog` for deployment backlog; a missed notification needs no manual queue repair.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** six Sections in dependency order: measure, schema, claim, row transitions, deletions, docs; one Pull Request, because a half-moved queue has two sources of truth again.
- **Alternatives considered:** (a) keep both stores and add a transactional outbox from the ledger to the stream: removes replay's 30 s gap but keeps the mark, the directory and every Dragonfly repairer; (b) collapse the sixteen partitions into one sorted set: fixes pickup order but keeps two queues; (c) M225_001's doorbell alone: removes the newest layer only.
- **Patch-vs-refactor verdict:** this is a **refactor** because the concept count comes from the two-store design itself; Postgres load barely moves, since a drained lease already costs 50.0 Postgres statements (`docs/architecture/scaling.md:273`) and the claim adds about one while `MARK_DELIVERED` and the receipt write go.

## Discovery (consult log)

- **Opening source check** — comparison revision `0fa9e55356571d57df69cd3a13fb3b90930b0e49`, `VERSION` `0.62.2`. M225_001 is in `done/`; its sixteen readiness partitions remain in the old serving queue. The schema uses `sequence`, admission UUIDs and `TIMESTAMPTZ(3)` markers (`schema/1720_fleet_admissions.sql`), with last registered migration 2100. The report guard is `METER_LEASE` (`afd_fleet/src/lease/sql/meter_lease.sql`), called by `Leases::claim_and_settle`. Historical completed specs and unrelated billing metric labels remain historical evidence; live queue callers and additional architecture/teardown references are in scope. No application suite ran at opening.
- **Start authorization** — Indy: "ands start on this?" — opened the isolated worktree and committed lifecycle metadata as `40c127268`; the measurement and recorded-decision boundaries above still apply.

- **Consults** — Indy (Oct 13, 2026, AskUserQuestion during M225_001 review round 2) picked "Spec it now (Recommended)" for this refactor as a pending spec, no code. Owner decisions, with their implementation boundaries:
  - **Q1/Q2 — answered** — Indy: "Proceed with ledger + Postgres fallback (Recommended)". The admissions ledger is the sole serving queue; every idle poll probes Postgres in one statement. Lost notifications cannot strand committed work. The measured cost is recorded below and in `scaling.md`.
  - **Q3 — answered** — Indy: "Approve these obsolete column removals (Recommended)". Approved the specific removals in `schema/2120_fleet_admissions_drop_receipt.sql`: `fleet.admissions.receipt`, `replay_count`, `replay_claim_expires_at`, their dependent indexes, and `runner.leases.receipt`. Register and apply only after removing their runtime readers.
  - **Q4 — answered** — Indy: "I think we are just on dev, only we can delete the authored drafts and fleets. So  impact to legacy crap". Apply a clean development-data cutover with old readers stopped. No historical admission backfill is needed; forward migration files are still retained.
  - **Documentation scope — answered** — the owner limited this work to the current repository after reviewing the proposed published setup and release note edits:

    > Indy (Oct 21, 2026): "Keep this repository only"

    Update architecture and generated API descriptions here; do not write to `~/Projects/docs`.
  - **Generic ship helper — answered** — Indy (Oct 21, 2026): "Use repository gates and accept this counter finding (Recommended)". The helper flagged `fencing_token = s.fencing_token + 1` as a credential and could not map Rust/schema paths to test commands. Record this as a numeric fencing counter and use the repository's declared `orly gate pr` and gitleaks checks. No scanner suppression or gate configuration changes are authorized.
  - **Schema checker — answered** — Indy (Oct 21, 2026): "Approve these two checker files (Recommended)". Update only `scripts/check_architecture_doc_schema.py` and `scripts/check_architecture_doc_test_schema.sh` so the inventory follows forward column migrations. The reviewed proposal keeps column drift, invalid changes, unsupported renames and table removals as failures. This specific approval permits that harness change; it adds no scanner suppression.
  - **Release version — answered** — Indy (Oct 21, 2026): "approved for minor versoin", followed by "ensure the verison is updated to 0.65, and PR is pushed, the CI jobs are green". Use `0.65.0`. The later instruction supersedes the earlier `0.63.0` selection.
  - **Final quality requirements** — Indy requires at least 99% changed-line coverage and at least 99% coverage for each changed Rust file, plus 100% TypeScript coverage; follow `docs/RUST_ERROR_STANDARD.md`, regenerate changed OpenAPI descriptions and provide benchmark evidence. Coordinate with M230 in the separate development-acceptance worktree; the owner will merge M230 first, then this Pull Request. Do not merge either stream here.
- **Metrics review** — `agentsfleet_fleet_ready_depth` removed with its dashboard panel; the backlog gauge changes producer only; analytics/funnel playbook update not required — no product event changes.
- **Measured preparation** — `BENCH_LEASE_QUEUE={ledger,streams} make bench-lease PROFILE=rig KEEP_TEST_STATE=1`, sequential runs with 200 fleets, eight runners, pool 20; every run processed 200/200 events with no failures and swept 1,216/1,216 fixture records. Statement consolidation reduced the ledger prototype from 6.98 to 4.665 claim round trips per lease; streams cost 2.425. Idle costs one Postgres statement against zero. Consolidated drains cost 55.695 versus 55.64 statements, 26.41 versus 27.85 commits, and 5.72 versus 16.155 Dragonfly calls per lease. Raw results are retained under `docs/architecture/measurements/admissions-queue/`; latency varies with this shared host. These figures establish cost, not a throughput improvement. Production report settlement and its final measurement remain due.
- **Skill-chain outcomes** — pending.
- **Deferrals** — none.
