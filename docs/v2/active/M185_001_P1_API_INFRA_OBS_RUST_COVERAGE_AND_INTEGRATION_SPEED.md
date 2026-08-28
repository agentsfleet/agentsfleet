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

# M185_001: Rust coverage is complete and the live lane is faster

**Prototype:** v2.0.0
**Milestone:** M185
**Workstream:** 001
**Date:** Aug 28, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — the daemon's reliability proof is below its declared bar and its only live-datastore lane pays avoidable compilation, connection, cache, and disk costs
**Categories:** API INFRA OBS
**Batch:** B1 — follows M184's measured crate decomposition and changes the Rust verification boundary
**Branch:** feat/m185-rust-coverage-speed
**Test Baseline:** latest green CI Rust corpus `unit + integration = 1629`; local CHORE(open) Rust unit lane green, while the repository unit target exposed one unrelated app flake in each of two runs (`2405/2406` app tests passed); local integration reached `2 passed` before six concurrent Redis connections timed out, and the focused retry passed `6/6`. The local wall time is contention-tainted by two sibling worktrees and is not a performance baseline.
**Depends on:** M184_001 — its measured critical path proves sibling width, not crate count, is the parallelism lever
**Provenance:** LLM-drafted (GPT-5, Aug 28, 2026)
**Canonical architecture:** `docs/architecture/testing.md` §Coverage

---

## Overview

**Goal (testable):** `test_rust_coverage_lane_reaches_the_declared_bar_with_one_instrumented_build_and_preserves_the_original_failure`
**Problem:** The Rust flag reports 20,947 of 25,961 product lines covered, the latest patch status is below the declared target, and the live lane's successful reference run spends most of its wall time compiling the daemon twice. A dead Redis endpoint also holds one test binary far beyond its configured deadline, whole-target caching moves several gigabytes per run, and disk exhaustion replaces the originating linker error with a status-parser failure.
**Solution summary:** Reuse one instrumented build for migration and tests, bound Redis connection establishment, make lane result propagation independent of writable status files, partition the HTTP shell only along measured sibling boundaries, close product-code coverage with behavioural and failure-injection tests, consolidate observed test support, and delete proven orphans. The existing make targets, datastore reset contract, public HTTP behaviour, wire shapes, and 100% repository coverage contract remain the interface.

## PR Intent & comprehension handshake

- **PR title (eventual):** Make Rust coverage complete and faster
- **Intent (one sentence):** Give engineers a faster, disk-bounded live Rust proof that reaches the repository's declared coverage bar and reports the real cause of every failure.
- **Handshake:** I understand this as a reliability and verification refactor, not a request to inflate coverage by exclusions or low-value line execution. ASSUMPTIONS I'M MAKING: 99% is an implementation checkpoint while the committed 100% contract remains authoritative; `make test-integration-rustd` stays the sole live Postgres and Redis lane; public routes, schemas, commands, flags, and wire payloads do not change; an internal crate boundary ships only when timing proves sibling parallelism; and generated Cargo caches are never deleted automatically.
- **Golden path:** the make target starts compose and verifies the Redis certificate, resets Postgres schemas and Redis state, runs the daemon's real migrator under the same LLVM instrumentation used by the tests, executes unit plus ignored live tests once, preserves the child status while streaming output, emits phase and coverage evidence, writes LCOV, enforces the declared line bar, and uploads that exact report. No second schema path, migration implementation, test pass, or coverage denominator exists.

## Implementing agent — read these first

1. `make/test-integration-rustd.mk` — the canonical reset, migration, ignored-test, and coverage lane being changed.
2. `rustd/crates/afd_db/src/test_util.rs` — the one migrated database, minted-identifier isolation, and scratch-database exception the live suite must preserve.
3. `docs/v2/done/M184_001_P1_API_FLEET_CRATE_DECOMPOSITION.md` — measured proof that chains serialize and only sibling crate width buys clean-build parallelism.
4. `docs/LOGGING_STANDARD.md` — boundary pairing, Rust field hoisting, error codes, and event compatibility.
5. `docs/RUST_ERROR_STANDARD.md` — per-crate error ownership, `#[from]` composition, contextual `map_err`, and truthful source chains.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `docs/v2/pending/M185_001_P1_API_INFRA_OBS_RUST_COVERAGE_AND_INTEGRATION_SPEED.md` | CREATE | milestone rulebook and evidence record; CHORE moves it through active and done |
| `make/test-integration-rustd.mk` | EDIT | share one instrumented corpus, retain canonical targets, enforce coverage, and expose phase evidence |
| `make/test-infra.mk` | EDIT if measured | retain reset semantics while removing only proven orchestration latency |
| `scripts/rustd_lane_result.py` and its test | EDIT | stream and classify the child result without a disk-backed status dependency |
| `scripts/rustd_lane_benchmark.py` and its test | CREATE | compare repeatable before/after lane evidence without changing the gate's semantics |
| `.github/workflows/test-integration-rustd.yml` | EDIT | cache only useful coverage artifacts, disable waste proven by measurement, and publish timing evidence |
| `codecov.yml` | EDIT | correct stale Rust claims while preserving the 100% project and patch contract |
| `rustd/Cargo.toml` and `rustd/Cargo.lock` | EDIT | add only measured sibling HTTP members and tune CI-specific profile behaviour without weakening local debuggability |
| `rustd/crates/afd_http/**` | CREATE if graph proof passes | small shared HTTP protocol, admission, envelope, authentication, and request context substrate |
| `rustd/crates/afd_api_tenant/**`, `rustd/crates/afd_api_runner/**`, `rustd/crates/afd_api_operator/**` | CREATE if graph proof passes | independent handler planes that compile concurrently and link narrower integration binaries |
| `rustd/crates/afd_api/**` | EDIT | retain router composition and move code/tests to measured sibling owners |
| `rustd/crates/**/src/**/*.rs` and `rustd/crates/**/tests/**/*.rs` | EDIT only from coverage evidence | close reachable product branches through behavioural, boundary, and failure-injection tests |
| `rustd/crates/afd_redis/src/client.rs`, `rustd/crates/afd_redis/src/error.rs`, and `rustd/crates/afd_redis/tests/integration_ready.rs` | EDIT | bound connection establishment and prove timeout/error/log outcomes |
| `rustd/crates/agentsfleetd/src/supervisor.rs` and its tests | EDIT | complete the supervised-task lifecycle event pair |
| `rustd/crates/afd_db/tests/integration_migrate.rs`, `rustd/crates/afd_db/tests/integration_migrate_faults.rs`, and `rustd/crates/afd_db/tests/integration_pool.rs` | EDIT | consume the existing crate-owned scratch database utility |
| `rustd/crates/afd_db/tests/support/test_database.rs` | DELETE | remove the duplicate scratch-database lifecycle and subscriber |
| `docs/architecture/testing.md` | EDIT | describe the real I/O-bearing Rust coverage lane and local enforcement |
| `crates` | DELETE | remove the tracked, zero-byte, unreferenced root orphan |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — RULE GRD grounds the lane and coverage claims; RULE NDC, RULE NLR, RULE HLP, RULE ORP, and RULE CHR remove dead or duplicate surfaces completely; RULE ECL and RULE ERR-RS preserve timeout classification and causes; RULE UFS prevents duplicated protocol/event spellings; RULE TST-NAM and RULE TNM keep durable behavioural test names; RULE ITF preserves real-schema fixtures; RULE MKP requires truthful pipeline exit propagation; RULE FLL constrains every touched source and test file.
- **`dispatch/write_rust.md` and `docs/RUST_ERROR_STANDARD.md`** — every Rust fallible seam remains a `Result` pipeline with one owning error type and no stringified cause.
- **`docs/LOGGING_STANDARD.md`** — Redis and supervisor boundary events are paired, structured, byte-compatible or intentionally migrated, and tested with an enabled subscriber.
- **`dispatch/write_python.md`** — lane and benchmark scripts keep subprocess ownership, typed parsing, and negative-path tests explicit.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| SPEC TEMPLATE | yes — lifecycle spec | staged spec has every required section, no template residue, and declared commands verbatim |
| RUST ERROR | yes — Redis timeout and crate seams | run the Rust error audit and test source-chain classification |
| LOGGING | yes — Redis and supervisor events | preserve or intentionally migrate event bytes; assert started plus exactly one terminal event |
| UFS / MILESTONE-ID / GREPTILE | yes — Rust, Python, Make, and workflow edits | named constants, behavioural names, and the listed rule codes drive each edit |
| File & Function Length (≤350/≤50/≤70) | yes | split HTTP planes, tests, and runners by cohesive concern before a touched file crosses a cap |
| ORPHAN / Dead Code Sweep | yes — files and helpers are removed | repo-wide word-boundary greps prove zero live references before commit |

## Prior-Art / Reference Implementations

- **Reference:** `rustd/crates/afd_db/src/test_util.rs` — one live database, minted identifiers, scratch schema tests, and enabled tracing without a second migration path.
- **Reference:** `docs/v2/done/M184_001_P1_API_FLEET_CRATE_DECOMPOSITION.md` — cargo timing, Tarjan-style dependency proof, and rejection of serialized crate chains.
- **Reference:** `rustd/crates/afd_api` plus `docs/REST_API_DESIGN_GUIDELINES.md` — current router composition and handler behaviour remain the public contract while ownership moves internally.

## Sections (implementation slices)

### §1 — One truthful coverage execution

Migration and both test tiers share one LLVM-instrumented corpus. The lane streams output and owns the subprocess status in memory, generates LCOV only after successful execution, enforces the committed line target locally, and reports phase duration plus artifact size without storing a duplicate full log.

- **Dimension 1.1** — migration and tests reuse one coverage target without a normal daemon rebuild → Test `test_coverage_lane_reuses_the_instrumented_migration_build`
- **Dimension 1.2** — a child build, migration, test, report, or disk failure exits with its original status and diagnostic → Test `test_lane_runner_preserves_the_originating_failure`
- **Dimension 1.3** — zero discovered tests and coverage below the declared bar fail locally before upload → Test `test_coverage_lane_rejects_zero_tests_and_an_under_target_report`
- **Dimension 1.4** — CI cache contents and incremental policy are accepted only when repeat measurements reduce total wall time and stored bytes → Test `test_integration_workflow_caches_only_measured_artifacts`

### §2 — Redis deadlines and lifecycle observability

Connection establishment consumes a configured finite budget just like commands. Timeout remains distinct from a driver refusal and from invalid configuration, sources are retained when one exists, and every boundary has a correlated started plus completed or failed event without casually renaming a compatible event.

- **Dimension 2.1** — a dead endpoint returns an unavailable timeout inside the configured connection budget → Test `test_redis_connect_honours_its_deadline`
- **Dimension 2.2** — driver and certificate failures retain their real causes while elapsed deadlines truthfully carry no invented source → Test `test_redis_connect_failures_preserve_error_class_and_source`
- **Dimension 2.3** — Redis connection and supervised tasks emit complete lifecycle pairs at the standard's required severity → Test `test_runtime_boundaries_emit_exactly_one_terminal_event`

### §3 — Compile-parallel HTTP ownership

The HTTP shell is partitioned only after a source dependency graph proves an acyclic substrate and independent plane siblings. `afd_api` remains the sole router composition root. Tests move with public surfaces and each new binary links only the dependencies its plane needs. **Implementation default:** one small HTTP substrate with tenant, runner, and operator siblings because those planes share protocol policy but not domain stores; reject or amend any boundary whose cargo timings show another serialized chain.

- **Dimension 3.1** — every extracted plane depends on the substrate and no sibling, while the composition root depends downward on all → Test `test_http_plane_dependency_graph_is_acyclic_and_sibling_shaped`
- **Dimension 3.2** — route inventory, statuses, error codes, response sentences, and existing test discovery remain unchanged → Test `test_http_partition_preserves_the_complete_route_contract`
- **Dimension 3.3** — clean and incremental cargo timings demonstrate a shorter critical path or the proposed boundary does not ship → Test `test_http_partition_reduces_the_measured_compile_critical_path`

### §4 — Behavioural coverage closure

Coverage closes in descending missed-line order: API-key and session handlers, tenant/workspace ownership, runner repair and liveness, fleet/gate/credential failures, SSE fan-in, then the remaining reachable tails. Tests enter through public handlers or real dependency seams. Pure classification may move out of I/O orchestration when it improves the design; production exclusions, fake line touches, and tests asserting an error source exists for data-only variants are forbidden.

- **Dimension 4.1** — every public route and SSE outcome has success, refusal, malformed-input, and unavailable-dependency proof → Test `test_public_rust_surfaces_cover_success_and_failure_outcomes`
- **Dimension 4.2** — sweep, gate, credential, fleet, tenant, and runner races or repair decisions have deterministic failure injection → Test `test_domain_planes_cover_repair_timeout_and_race_outcomes`
- **Dimension 4.3** — logging fields, error renderers, and source-chain branches execute under a real subscriber without changing their semantics → Test `test_error_and_observability_branches_are_executed_truthfully`
- **Dimension 4.4** — product line coverage reaches 100% with no new ignored product path and no denominator shrink disguised as progress → Test `test_rust_product_coverage_reaches_the_declared_denominator`

### §5 — One owner and no orphans

The crate-owned database fixture replaces its test-local duplicate, subscriber setup is consolidated only where a crate already shares the relevant support, and tracked empty or unused artifacts are removed. No production or workspace crate is introduced solely to share a tiny test helper.

- **Dimension 5.1** — all scratch migration tests use `afd_db::test_util::TestDatabase` and its sole cleanup contract → Test `test_migration_suites_share_the_crate_owned_scratch_database`
- **Dimension 5.2** — duplicated subscriber helpers within one crate collapse without adding production dependency edges → Test `test_each_test_support_concern_has_one_owner_per_crate`
- **Dimension 5.3** — deleted files, imports, and symbols have zero non-historical references → Test `test_removed_rust_support_and_root_orphans_have_no_references`

### §6 — Reproducible proof and truthful documentation

Cold and warm evidence use the same toolchain, runner shape, datastore reset, and coverage denominator. The declared gates remain the only repository claims. Testing and Codecov documentation state the measured I/O-bearing design rather than the superseded pure-crate claim.

- **Dimension 6.1** — repeated before/after evidence reports median phase time, peak artifact bytes, test count, and coverage denominator → Test `test_lane_benchmark_compares_equivalent_runs`
- **Dimension 6.2** — Rust errors, logging, lint, unit, live integration, version, and coverage gates all pass → Test `test_rust_verification_boundary_is_green`
- **Dimension 6.3** — architecture and Codecov prose match the executable target and target value → Test `test_rust_coverage_documentation_matches_the_gate`

## Interfaces

```
make test-integration-rustd
  remains the datastore-backed ignored-test claim and never runs in the unit lane

make test-coverage-rustd
  resets once, migrates once, executes unit plus ignored tests once, writes rustd/lcov.info,
  enforces the repository's declared Rust line target, and returns the originating failure status

Public HTTP routes, payloads, status codes, error codes, configuration knobs, schema, and wire types
  remain unchanged
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Build or disk failure | compiler/linker cannot produce an artifact | lane exits with that child status and original diagnostic; no missing-status parser error replaces it |
| Migration failure | real daemon migrator refuses or datastore is unavailable | tests do not start; the migration failure remains visible and non-zero |
| Empty test execution | filters or runner drift discover no tests | lane fails even when Cargo itself exits successfully |
| Coverage regression | reachable product lines remain below the declared bar | local make target fails before Codecov upload |
| Redis dead endpoint | manager establishment retries beyond the configured budget | connection future is cancelled at the boundary and returns the timeout class |
| Redis configuration or driver refusal | bad certificate, URL, or concrete Redis error | distinct config/unreachable classification and real source chain are retained |
| Shared-state collision | a test asserts whole-table state or reuses a fixed identifier | deterministic isolation test fails; no datastore-per-test fallback is introduced |
| False crate parallelism | a proposed plane depends on a sibling | graph/timing proof fails and the boundary is amended or rejected |
| Cache growth | CI stores stale normal, incremental, and instrumented products indiscriminately | cache audit fails and workflow retains only artifacts with measured total benefit |
| Event drift | a boundary is renamed or exits without its pair | compatibility/pairing test fails before the observability change ships |

## Invariants

1. `make test-integration-rustd` is the only developer lane requiring live Postgres and Redis — enforced by ignored integration tests and canonical make dependencies.
2. One reset and the daemon's one migrator build the lane schema — enforced by target structure and a runner test that counts migration execution.
3. Coverage describes shipped Rust under `rustd/crates/`, not tests, generated output, or deleted denominator — enforced by Codecov paths plus a local summary checker.
4. Every fixture mints scoped identifiers and every shared-state assertion carries that scope — enforced by integration stress tests and source audits for unscoped whole-table counts.
5. New Rust crate errors compose with `#[from]`; contextual `map_err` retains the cause; data-only failures need no source — enforced by `audits/rust-error.sh` and focused unit tests.
6. Boundary operations emit started plus exactly one completed or failed event; event renames require an explicit compatibility migration — enforced by captured-subscriber tests.
7. Generated targets may fail closed on insufficient disk but are never silently or automatically deleted by a verification target — enforced by lane-runner tests.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| Rust lane phase summary | engineering | infrastructure, migration, compile/test, report, and cache phases finish | phase, duration, exit status, test count, artifact bytes, covered and total lines | no URLs, certificate contents, tokens, query data, or environment values | `test_lane_benchmark_compares_equivalent_runs` |
| Redis connection lifecycle | ops | a Redis connection attempt begins and reaches one terminal outcome | role, TLS boolean, configured budget, duration, error code, outcome | no Redis URL, credentials, certificate path contents, commands, or payloads | `test_runtime_boundaries_emit_exactly_one_terminal_event` |
| Supervised task lifecycle | ops | a named task is spawned and later joins, panics, or is abandoned | stable task name, duration, outcome, registered error code on failure | no task payload or tenant data | `test_runtime_boundaries_emit_exactly_one_terminal_event` |
| Product analytics | not applicable | no user workflow changes | none | no new analytics or funnel data | `test_http_partition_preserves_the_complete_route_contract` |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | integration | `test_coverage_lane_reuses_the_instrumented_migration_build` | clean lane evidence contains one instrumented dependency build and one migration execution |
| 1.2 | unit | `test_lane_runner_preserves_the_originating_failure` | child failures including unwritable output return the original status and diagnostic |
| 1.3 | unit | `test_coverage_lane_rejects_zero_tests_and_an_under_target_report` | zero tests or a below-target summary returns non-zero |
| 1.4 | unit | `test_integration_workflow_caches_only_measured_artifacts` | workflow cache paths and profile knobs equal the benchmark-approved set |
| 2.1 | integration | `test_redis_connect_honours_its_deadline` | dead loopback endpoint with a finite budget returns timeout within bounded slack |
| 2.2 | unit | `test_redis_connect_failures_preserve_error_class_and_source` | elapsed timeout has no invented source; driver and certificate failures retain theirs |
| 2.3 | unit | `test_runtime_boundaries_emit_exactly_one_terminal_event` | captured Redis and supervisor operations contain one correlated start and terminal record |
| 3.1 | unit | `test_http_plane_dependency_graph_is_acyclic_and_sibling_shaped` | Cargo metadata contains substrate-to-plane edges and no cross-plane edge |
| 3.2 | integration | `test_http_partition_preserves_the_complete_route_contract` | route inventory and representative real requests match the locked response contract |
| 3.3 | integration | `test_http_partition_reduces_the_measured_compile_critical_path` | equivalent cargo timings show a strictly shorter serialized critical path |
| 4.1 | integration | `test_public_rust_surfaces_cover_success_and_failure_outcomes` | router and SSE tests exercise success, refusal, malformed, and unavailable outcomes |
| 4.2 | integration | `test_domain_planes_cover_repair_timeout_and_race_outcomes` | deterministic fakes and live stores reach every named repair and contention decision |
| 4.3 | unit | `test_error_and_observability_branches_are_executed_truthfully` | subscriber-enabled tests execute field, renderer, and source branches without fabricated causes |
| 4.4 | integration | `test_rust_product_coverage_reaches_the_declared_denominator` | summary reports 100% over the locked product path and non-decreasing denominator inventory |
| 5.1 | integration | `test_migration_suites_share_the_crate_owned_scratch_database` | migration and pool suites create, use, and clean the sole scratch fixture implementation |
| 5.2 | unit | `test_each_test_support_concern_has_one_owner_per_crate` | duplicate helper audit reports zero within-crate implementations |
| 5.3 | unit | `test_removed_rust_support_and_root_orphans_have_no_references` | deleted paths are absent and word-boundary repository greps return zero live hits |
| 6.1 | integration | `test_lane_benchmark_compares_equivalent_runs` | before and after evidence shares runner/toolchain/reset/denominator and reports medians |
| 6.2 | integration | `test_rust_verification_boundary_is_green` | every declared make claim plus coverage exits zero |
| 6.3 | unit | `test_rust_coverage_documentation_matches_the_gate` | docs, Codecov target, local target, path flag, and denominator agree |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Rust product coverage reaches the committed bar (§4) | `make test-coverage-rustd` | exit 0 and literal `100.00%` | P0 | |
| R2 | Equivalent live coverage runs are materially faster (§1, §3, §6) | `python3 scripts/rustd_lane_benchmark.py compare .tmp/rustd-lane-before.json .tmp/rustd-lane-after.json` | exit 0, median wall ratio at most `0.80`, equal denominator | P0 | |
| R3 | Redis dead-end startup obeys its configured deadline (§2) | `cd rustd && cargo test -p afd_redis --all-features --test integration_ready -- --ignored` | exit 0 and dead-end case below its asserted bound | P0 | |
| R4 | HTTP crates form measured parallel siblings (§3) | `cd rustd && cargo test -p afd_api --all-features http_plane_dependency_graph` | exit 0 and zero cross-sibling edges | P1 | |
| R5 | Duplicate support and tracked empty orphan are gone (§5) | `test ! -e crates && test ! -e rustd/crates/afd_db/tests/support/test_database.rs` | exit 0 | P1 | |
| R6 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | |
| S1 | Conform gates green | `make harness-verify` | exit 0 | P0 | |
| S2 | Lint passes | `make lint-all` | exit 0 | P0 | |
| S3 | Unit tests pass | `make test-unit-all` | exit 0 and unit count not below baseline | P0 | |
| S4 | Live integration passes | `make test-integration-rustd` | exit 0 and integration count not below baseline | P0 | |
| S5 | Version remains coherent | `make check-version` | exit 0 | P0 | |
| S6 | No secrets enter the diff | `gitleaks detect` | exit 0 | P0 | |

**Command source rule:** S1–S5 are copied verbatim from `.oracle/orly.json`; they are the same boundary `orly gate` executes.

**Grading protocol (VERIFY):** run the Verify command verbatim and grade only from its output. Graded is ✅ or ❌ plus one decisive output line. Every row is graded and every P0 is green before CHORE(close).

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

| File to delete | Verify |
|----------------|--------|
| `crates` | `test ! -e crates` |
| `rustd/crates/afd_db/tests/support/test_database.rs` | `test ! -e rustd/crates/afd_db/tests/support/test_database.rs` |

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| test-local `support/test_database.rs` path import | `git grep -n 'support/test_database.rs' -- ':!docs/v2/*'` | 0 matches |
| duplicate test-local `TestDatabase` definition | `git grep -n 'struct TestDatabase' -- rustd/crates/afd_db/tests` | 0 matches |

## Out of Scope

- A second live-datastore lane, datastore-per-test isolation, or migration implementation.
- Public endpoints, schemas, wire payloads, commands, flags, and the external documentation repository.
- Coverage exclusions, lowered Codecov targets, generated line touches, or deletion whose purpose is denominator reduction.
- Splitting the serialized credential → gate → fleet chain again; M184 already proved it cannot compile in parallel.
- A production workspace crate whose only consumer is test support.
- Automatic deletion of developer Cargo or Docker caches.

## Product Clarity (authoring record)

1. **Successful user moment** — an engineer runs the one live Rust target and receives a faster green result with complete coverage, or an immediate diagnostic naming the actual failed phase.
2. **Preserved user behaviour** — the same make targets, real migrator, clean datastore reset, ignored-test boundary, HTTP responses, and Codecov flag continue to work unchanged.
3. **Optimal-way check** — one instrumented execution is the direct shape; a larger distributed test system would add orchestration without addressing compilation, timeout, or coverage debt.
4. **Rebuild-vs-iterate** — refactor the oversized HTTP compilation unit and lane runner, but retain the datastore and route contracts; a shell-only patch would leave the dominant graph and failure semantics intact.
5. **What we build** — one truthful coverage runner, bounded Redis connect, measured sibling HTTP crates, behavioural coverage tests, focused support consolidation, and reproducible timing evidence.
6. **What we do NOT build** — no new product surface, datastore lane, migration path, lowered threshold, or generic test framework.
7. **Fit with existing features** — this compounds the Rust cutover, real-store test utility, Codecov flag, and M184 crate work; it must not destabilize route parity or test isolation.
8. **Surface order** — N/A — this is an internal verification and runtime-reliability milestone with no CLI or UI surface.
9. **Dashboard restraint** — N/A — phase evidence belongs in CI output and step summaries; no product dashboard is justified.
10. **Confused-user next step** — every failure prints its originating phase, command status, remediation for missing tools or disk, and the exact canonical target to rerun.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** one workstream joins the lane, runtime deadline, HTTP graph, coverage, and hygiene because each changes the same coverage build and its denominator; splitting them would either measure an obsolete graph or execute the expensive boundary repeatedly.
- **Alternatives considered:** adding tests without changing the lane leaves duplicate compilation and disk failure; shell tuning without coverage closure preserves a red contract; many tiny crates repeat M184's serialized-chain mistake; lowering the target contradicts the committed architecture.
- **Patch-vs-refactor verdict:** this is a **refactor** because the bottleneck crosses build orchestration, runtime boundaries, crate ownership, and test seams, while the external behaviour remains locked.

## Discovery (consult log)

- **Consults** — Architecture / Legacy-Design / gate-flag triage: user approved the analysis, requested `$orly-spec-new`, and authorized implementation in a dedicated worktree. The committed 100% target remains authoritative; 99% is not treated as a new floor.
- **Metrics review** — creation baseline: successful commit `34387641b` ran the make coverage boundary in 523 wall seconds, including a separate normal migration build; 1,629 tests executed, Rust product coverage was 20,947/25,961 lines, one Redis binary consumed 28.86 seconds, and the workflow saved 5.68 GB of Cargo cache. No analytics or funnel playbook update is required because no user workflow changes.
- **Skill-chain outcomes** — `$orly-spec-new` authored this rulebook. `/orly-write-unit-test`, `/review`, and `orly-babysit-prs` remain empty until their lifecycle stages.
- **Deferrals** — none.
