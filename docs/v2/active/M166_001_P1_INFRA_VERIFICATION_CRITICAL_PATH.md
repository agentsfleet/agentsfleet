<!--
SPEC AUTHORING RULES (load-bearing — the one comment that survives):
- Body order = the executing agent's read order. Fill via the kishore-spec-new
  skill (authoring order lives there); after filling, DELETE every "tpl:"
  guidance comment — the SPEC TEMPLATE GATE blocks tpl residue, unfilled
  {slots}, and missing required sections (audits/spec-template.sh --staged).
- No time/effort/hour/day estimates anywhere. No effort columns, complexity
  ratings, percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only
  sequencing signal. A section that contradicts these rules loses — delete it.
-->

# M166_001: Cut the verification critical path locally and in Continuous Integration

**Prototype:** v2.0.0
**Milestone:** M166
**Workstream:** 001
**Date:** Aug 16, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — Pull Request (PR) feedback is dominated by an instrumented integration run that is serial and then repeated by another canonical lane.
**Categories:** INFRA
**Batch:** B1 — starts after M164_002 fixes the coverage denominator and its required-component assertions.
**Branch:** perf/m166-verification-critical-path
**Test Baseline:** unit=4124 integration=704
**Depends on:** M164_002 (the faster graph must preserve its product-only denominator, folder floors, and required-component checks)
**Provenance:** agent-generated from the live M136 verification run and the current Make and workflow sources, Aug 16, 2026
**Canonical architecture:** `docs/architecture/testing.md` §Coverage

---

## Overview

**Goal (testable):** The unchanged canonical verification commands execute every registered test root exactly once per full verification, preserve every coverage and isolation guarantee, and reduce the median local and Continuous Integration (CI) critical path by at least 35% against the same-commit baseline.
**Problem:** `make test-unit-all` reaches `test-coverage-zig`, which runs the full live-service integration binary under kcov after the unit components. `make test-integration` then resets the datastores and runs that same integration graph again. CI repeats the shape across `test.yml` and `test-integration.yml`; the coverage job names its serial integration component as the long pole. Developers pay for duplicate work locally, while CI still waits on one serial instrumented suite even when its unit components run concurrently.
**Solution summary:** One machine-readable verification graph owns every test root, its coverage role, and its isolation requirements. The unit lane produces reusable unit coverage components without running the live integration roots. The integration lane executes the daemon root in deterministic coverage shards with isolated datastore state and executes the runner-kernel root once under coverage, then grades the union only after every owner reports a non-empty result. CI calls the same Make recipes in one workflow-run-scoped Directed Acyclic Graph (DAG), moves provenance-matched artifacts only between jobs in that run, and preserves the existing required check names without executing a compatibility copy of any root. Structural checks fail duplicate roots, missing roots, stale artifacts, unisolated shards, and workflow drift before a flattering timing can pass.

## PR Intent & comprehension handshake

- **PR title (eventual):** perf(test): shorten the canonical verification critical path
- **Intent (one sentence):** Give developers and CI the same faster verification graph without losing a test, coverage line, failure signal, or datastore-isolation guarantee.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `make/test-unit.mk` and `make/test-integration.mk` — the duplicate integration ownership, datastore reset, kcov collection, and failure-tally behaviour that must be preserved.
2. `.github/workflows/test.yml` and `.github/workflows/test-integration.yml` — the current CI critical paths, cache keys, container permissions, and host-network constraints.
3. `src/build/daemon_tests.zig` — the single unit and integration test-root registration point and existing filter semantics.
4. `scripts/check_zig_coverage.py` and `scripts/check_ci_lane_config_test.py` — the existing fail-closed report union and source-level workflow guards.
5. `docs/architecture/testing.md` §Coverage — canonical coverage denominator, Low Level Virtual Machine (LLVM) requirement, component completeness, and local/CI parity rules.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `make/test.mk` | EDIT | Define one verification graph surface and its timing and coverage invariants. |
| `make/test-unit.mk` | EDIT | Produce unit coverage components without owning the live integration execution. |
| `make/test-integration.mk` | EDIT | Own the single sharded live integration execution, reset, aggregation, and failure result. |
| `make/test-verification.mk` | CREATE | Hold bounded integration workers, runtime isolation, provenance manifests, and final grading below the file-size ceiling. |
| `src/build/daemon_tests.zig`, `src/build/test_list.zig`, `src/build/test_runner_list.zig` | EDIT | Build deterministic integration shards and emit lane-qualified compiler registrations. |
| `src/build/test_runner_shard.zig` | CREATE | Select and execute compiler-registered tests by stable runtime shard assignment. |
| `scripts/check_verification_graph.py` | CREATE | Validate root ownership, artifact provenance, shard completeness, isolation, and timing evidence. |
| `scripts/check_verification_graph_test.py` | CREATE | Failure-injecting unit tests for every graph and evidence assertion. |
| `scripts/verification_evidence.py`, `scripts/run_with_timeout.py`, `scripts/run_with_timeout_test.py` | CREATE | Validate runtime/timing/workflow evidence and bound each owner process group. |
| `scripts/check_zig_test_reachability.py`, `scripts/check_zig_test_reachability_cli_test.py`, `scripts/check_zig_test_lanes_test.py` | EDIT | Consume lane-qualified listings and pin the new ownership graph. |
| `scripts/check_zig_coverage.py`, `scripts/check_zig_coverage_test.py` | EDIT | Union integration shard reports while preserving required components, files, lines, roots, and floors. |
| `scripts/check_ci_lane_config_test.py` | EDIT | Prove CI calls the canonical recipes once and transfers only provenance-matched artifacts. |
| `.github/workflows/test.yml` | EDIT | Run the complete same-run Zig verification DAG, grade its final union, and publish timing evidence. |
| `.github/workflows/test-integration.yml` | EDIT | Preserve the required integration check context without executing a second registered root. |
| `docs/architecture/testing.md` | EDIT | Record single ownership, sharding, isolation, artifact provenance, and timing evidence. |
| `docs/v2/active/M166_001_P1_INFRA_VERIFICATION_CRITICAL_PATH.md` | EDIT | Mark Dimensions DONE and capture acceptance evidence during execution. |

## Applicable Rules

- **`~/Projects/dotfiles/docs/greptile-learnings/RULES.md`** — **NDC** (delete superseded serial and duplicate paths), **NLR** (fix touched stale lane ownership), **UFS** (component, shard, artifact, and summary keys have one definition site), **ORP** (sweep old workflow and target references), **TST** (every generated shard remains rooted in explicit Zig test registration), **TST-NAM** and **TNM** (tests state the behaviour and carry no milestone marker), **MKP** (Make pipelines preserve the first failing status), **GRD** (current Make recipes and workflows are the source of truth).
- `dispatch/write_zig.md` — build-graph shape, public-shape verdict, lifecycle wiring, and both Linux cross-compiles.
- `dispatch/write_python.md` — standard-library parsing, boundary validation, context-managed files, and specific failures.
- `dispatch/write_shell.md` — quoted recipe and workflow-shell expansions, array-safe arguments, cleanup, and pipeline status.
- `docs/architecture/testing.md` — architecture consult; the document and implementation change together.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes — build graph changes | Preserve explicit test roots, run both Linux cross-compiles, and prove every shard discovers tests. |
| PUB / Struct-Shape | yes — `src/build/daemon_tests.zig` changes | FILE SHAPE DECISION at PLAN; no new product-runtime public surface. |
| File & Function Length (≤350/≤50/≤70) | yes — Make, Zig, and Python files change | Put graph validation in the new checker and split helpers before any cap is crossed. |
| UFS (repeated/semantic literals) | yes — graph roles, artifact keys, shard names | Define each identifier once and generate consumers from the validated graph. |
| UI Substitution / DESIGN TOKEN | no — no user interface files | N/A |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | yes — process and datastore lifecycles change | Every worker cleans up, every connection drains, failures name the shard, and no schema file changes. |
| MILESTONE-ID | yes — source files change | No milestone, Section, or Dimension marker appears outside this spec. |

## Prior-Art / Reference Implementations

- **Reference:** `make/test-unit.mk` component fan-out and `scripts/check_zig_coverage.py` report union — retain the existing independent output directories, recorded exit status, non-empty report checks, and fail-closed union; extend the pattern to integration shards.
- **Reference:** `.github/workflows/test-integration.yml` host datastore plus project CI-image layout — preserve its host networking, cache, certificate, and container boundary while changing execution ownership.
- **Reference:** `src/build/daemon_tests.zig` — retain one canonical integration root and derive shard artifacts from its explicit registrations rather than maintaining parallel hand-written test lists.

## Sections (implementation slices)

### §1 — One graph owns every test root

The repository gains a machine-readable inventory of canonical test roots, execution owners, coverage roles, and isolation needs. A full verification can prove completeness before doing expensive work. **Implementation default:** derive the inventory from existing build registration and named Make data, then validate it independently; no second hand-maintained list may silently diverge.

- **Dimension 1.1** — every registered unit, integration, and runner-kernel root appears exactly once in the full verification graph → Test `test_registered_roots_have_one_execution_owner`
- **Dimension 1.2** — a missing or duplicate root fails before any test binary runs and names every offending root → Test `test_duplicate_or_missing_root_fails_closed`

### §2 — The live integration graph executes once

The unit umbrella stops running the live integration root. The integration umbrella becomes the only owner and grades its instrumented result as both integration proof and coverage input. Repeated command use remains safe because artifacts carry source, toolchain, graph, and environment provenance. **Implementation default:** reuse a matching result; reject or rebuild anything stale rather than trusting file presence.

- **Dimension 2.1** — the canonical full sequence executes the live integration root once while preserving all existing unit and package coverage lanes → Test `test_full_verification_executes_integration_root_once`
- **Dimension 2.2** — coverage and integration verdicts consume the same successful execution result; neither can report green when the other failed → Test `test_integration_and_coverage_share_one_verdict`
- **Dimension 2.3** — a result from another commit, toolchain, graph, datastore layout, or incomplete run is rejected with the mismatched field named → Test `test_stale_or_partial_artifact_is_rejected`

### §3 — Instrumented integration work is isolated and parallel

The long serial kcov component becomes deterministic shards. Every shard gets isolated PostgreSQL schemas, Redis namespace, QStash fixture state, and HTTP port space, so concurrency cannot turn shared cleanup into flakes. **Implementation default:** shard from explicit registrations using stable content-derived assignment; shard count changes rebalance work without changing the set of discovered tests.

- **Dimension 3.1** — the shard union discovers exactly the same integration tests as the unsharded root, with no omission or duplicate → Test `test_shard_union_matches_unsharded_discovery`
- **Dimension 3.2** — concurrent shards cannot read, mutate, flush, or drop another shard's datastore or port state → Test `test_parallel_shards_have_disjoint_runtime_state`
- **Dimension 3.3** — one failed, timed-out, empty, or crashed shard fails the aggregate and names that shard while retaining sibling diagnostics → Test `test_one_bad_shard_fails_the_aggregate`
- **Dimension 3.4** — the coverage union accepts only non-empty reports from every expected shard and preserves all M164_002 component, root, file, line, folder-floor, and target assertions → Test `test_sharded_coverage_preserves_denominator_assertions`

### §4 — CI runs the same graph and proves the speedup

CI uses the same Make-owned graph, moves only provenance-matched artifacts between jobs in one workflow run, and reports cold- and warm-cache critical paths. No workflow embeds a second test list or direct replacement command. The separately required `test-integration` context remains as a compatibility aggregate and owns no registered root; the substantive `test` context cannot turn green until the same-run integration aggregate succeeds. **Implementation default:** CI orchestrates caches and artifacts; Make owns test selection and verdicts.

- **Dimension 4.1** — workflow source invokes each canonical graph owner once and contains no duplicate direct integration execution → Test `test_ci_workflows_call_each_graph_owner_once`
- **Dimension 4.2** — a missing, stale, or tampered cross-job artifact makes the consuming job rebuild or fail; it never grades stale coverage → Test `test_ci_artifact_provenance_is_mandatory`
- **Dimension 4.3** — median critical-path wall time falls at least 35% locally and in CI against same-commit, same-image baselines, with cold and warm cache results reported separately → Test `test_local_and_ci_critical_path_improves_by_threshold`

### §5 — Evidence stays visible and the architecture stays true

Each run publishes the roots, shard counts, per-shard result, coverage completeness, and critical-path duration needed to diagnose a regression. The architecture page defines ownership and the rule for changing shard count or coverage composition.

- **Dimension 5.1** — local output and the CI job summary show execution count, shard result, coverage completeness, and comparable critical-path timings without environment values → Test `test_verification_summary_is_complete_and_redacted`
- **Dimension 5.2** — the architecture page and workflow guard describe and enforce the same graph, artifact, isolation, and timing rules → Test `test_testing_architecture_matches_verification_graph`

## Interfaces

```
Canonical commands remain stable:
  make test-unit-all
  make test-integration
  make memleak

.tmp/verification-graph.json
  roots[]: stable root identity, owner, coverage role, isolation class
  graph_digest: digest of the ordered validated graph

.tmp/verification-results/<execution>/manifest.json
  source_revision, toolchain_identity, graph_digest, environment_identity
  roots_executed[], shards_expected, shards_completed, outcome
  critical_path_ms, cache_state

.tmp/zig-coverage.txt
  every existing key remains unchanged
  additive keys report expected and collected integration shards

Exit status remains the caller interface: zero only when every owned root,
expected shard, coverage assertion, and timing acceptance check passes.
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Duplicate execution | Two targets or workflows claim one root | Graph check exits non-zero before execution and names both owners |
| Missing execution | Registered root has no owner or a shard discovers no tests | Aggregate exits non-zero naming the root or empty shard; coverage is not graded |
| Stale evidence | Artifact provenance differs from the current source, toolchain, graph, or environment | Consumer rejects it and either performs the owned work or fails with the mismatched field |
| Isolation collision | Two shards resolve the same schema, Redis namespace, QStash state, or port allocation | Preflight exits non-zero before either shard mutates shared state |
| Worker failure | Shard fails, times out, crashes, or omits its report | Aggregate fails, retains every sibling result, and names the bad shard |
| Coverage shrink | A shard or component produces a smaller or empty denominator | M164_002 assertions fail; no faster result is accepted |
| Workflow drift | CI embeds a direct test command or omits a graph owner | Source-level workflow test fails and names the workflow and owner |
| Timing regression | Candidate misses the improvement threshold or compares unlike cache states | Performance check fails with baseline, candidate, cache state, and computed change |

## Invariants

1. Every registered test root has exactly one execution owner — enforced by graph validation before any canonical run.
2. A result is reusable only when source, toolchain, graph, and environment provenance all match — enforced by manifest validation at every consumer.
3. Every parallel shard has disjoint mutable runtime state — enforced by generated isolation identifiers plus a collision preflight.
4. Coverage is graded only after every expected shard succeeds and contributes a non-empty report — enforced by aggregate completeness checks before the coverage checker runs.
5. CI owns orchestration only; Make owns test selection and verdicts — enforced by source-level workflow assertions against direct replacement commands.
6. A speed claim compares identical cache states and runner images — enforced by the timing evidence checker, which rejects unlike samples.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| `verification_graph_summary` | ops | a canonical local or CI verification graph completes | source revision, graph digest, cache state, root and shard counts, per-owner duration, outcome | no environment values, URLs, tokens, database strings, user data, or command arguments | `test_verification_summary_is_complete_and_redacted` |

No product analytics or funnel event changes. CI writes the same bounded fields to its job summary; local runs write the machine-readable manifest under `.tmp/`.

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_registered_roots_have_one_execution_owner` | current build registrations and graph inventory → each root has one owner |
| 1.2 | unit | `test_duplicate_or_missing_root_fails_closed` | fixture with one duplicated and one absent root → exit non-zero naming both before execution |
| 2.1 | integration | `test_full_verification_executes_integration_root_once` | canonical unit then integration sequence → one completed live integration execution and all unit/package lanes present |
| 2.2 | integration | `test_integration_and_coverage_share_one_verdict` | injected integration assertion failure → integration and coverage both red from one result |
| 2.3 | unit | `test_stale_or_partial_artifact_is_rejected` | manifests with one provenance field changed or one shard absent → each rejected naming the field |
| 3.1 | integration | `test_shard_union_matches_unsharded_discovery` | current integration root at one shard and configured shard count → identical test identity set and cardinality |
| 3.2 | integration | `test_parallel_shards_have_disjoint_runtime_state` | at least two concurrent shards with adversarial identical fixture names → no cross-shard read, mutation, flush, drop, or port collision |
| 3.3 | integration | `test_one_bad_shard_fails_the_aggregate` | inject failure, timeout, crash, and empty report into one shard at a time → aggregate red, shard named, sibling diagnostics retained |
| 3.4 | unit | `test_sharded_coverage_preserves_denominator_assertions` | omit one report and shrink one component fixture → both fail before rate grading; complete union preserves every existing key and floor |
| 4.1 | unit | `test_ci_workflows_call_each_graph_owner_once` | workflow sources → one invocation per owner and zero direct duplicate integration commands |
| 4.2 | unit | `test_ci_artifact_provenance_is_mandatory` | workflow fixture with absent validation or permissive stale fallback → source-level guard fails |
| 4.3 | integration | `test_local_and_ci_critical_path_improves_by_threshold` | three candidate and three baseline samples per cache state on one source and image → median improvement at least 35% for local and CI |
| 5.1 | unit | `test_verification_summary_is_complete_and_redacted` | successful and failed manifests containing hostile environment values → bounded fields present and sensitive values absent |
| 5.2 | unit | `test_testing_architecture_matches_verification_graph` | architecture and graph source → owner, provenance, isolation, coverage, and timing terms agree |

Regression: existing package coverage floors, Zig folder floors, required-component and product-root assertions, integration test count, memory-leak lane, and both Linux cross-compiles remain green. Replay: running the canonical sequence twice with matching provenance produces the same root set and verdict; only durations may differ.

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | One validated graph owns every root exactly once (§1–§2) | `python3 scripts/check_verification_graph.py validate` | exit 0 and `duplicate_roots=0 missing_roots=0` | P0 | |
| R2 | Sharded discovery and isolation are complete (§3) | `make test-integration` | exit 0 and every expected shard reports tests, isolation, and coverage | P0 | |
| R3 | Local critical path improves by at least 35% (§4) | `python3 scripts/check_verification_graph.py compare --scope local` | exit 0 and `improvement_pct>=35` for cold and warm cache states | P0 | |
| R4 | CI uses the same graph and improves by at least 35% (§4) | `python3 scripts/check_verification_graph.py compare --scope ci` | exit 0 and `improvement_pct>=35` for cold and warm cache states | P0 | |
| R5 | CI workflow ownership cannot drift (§4–§5) | `python3 -m unittest scripts/check_ci_lane_config_test.py scripts/check_verification_graph_test.py` | exit 0 | P0 | |
| R6 | Coverage guarantees remain green (§2–§3) | `make test-unit-all && make test-integration` | exit 0; existing coverage keys and M164_002 assertions present | P0 | |
| R7 | Repository verification stays green | `make harness-verify && make lint-all && make memleak && make check-version` | exit 0 | P0 | |
| R8 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | |
| R9 | No secrets or orphaned execution paths | `gitleaks detect --no-banner && python3 scripts/check_verification_graph.py orphans` | exit 0 and `orphaned_paths=0` | P0 | |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line (`342 passed`); long evidence goes to PR Session Notes with a pointer here. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE; a P1 ❌ ships only with an Indy-acked deferral quote in Discovery.

## Dead Code Sweep

No files are planned for deletion. During REVIEW, `scripts/check_verification_graph.py orphans` must report zero superseded direct integration invocations, obsolete serial-component names, unconsumed timing artifacts, and test roots with no owner.

## Out of Scope

- Product-runtime code, API behaviour, schema migrations, and user-interface changes.
- Lower coverage floors, smaller denominators, skipped test roots, retries that turn a failed shard green, or cache-only speed claims.
- Replacing kcov or the pinned CI image unless measurement proves the existing instrument cannot meet the goal and the spec is amended before that change.
- Faster browser acceptance, deployment, or live-service acceptance lanes; this workstream owns repository verification only.

---

## Product Clarity (authoring record)

1. **Successful user moment** — an engineer pushes a PR, receives the same trustworthy verification result, and can act on it with at least 35% less critical-path waiting both locally and in CI.
2. **Preserved user behaviour** — the canonical Make commands, exit-status meaning, coverage keys and floors, failure diagnostics, and CI required checks keep working.
3. **Optimal-way check** — one graph shared by local and CI is the direct shape; optimizing only a workflow or only a laptop would leave the other path slow and allow drift.
4. **Rebuild-vs-iterate** — a graph refactor is better than another cache tweak because duplicate ownership and serial instrumented execution are structural.
5. **What we build** — graph validator, single integration ownership, isolated shards, provenance-checked artifacts, CI wiring, timing evidence, and architecture documentation.
6. **What we do NOT build** — product features, weaker gates, new test frameworks, or a dashboard; none is needed to shorten verification.
7. **Fit with existing features** — compounds with M164_002 coverage assertions and current cache warming; must not destabilize canonical Make targets or memleak.
8. **Surface order** — N/A — internal verification tooling with no Command-Line Interface product surface.
9. **Dashboard restraint** — N/A — Make output, machine-readable manifests, and CI job summaries are sufficient evidence.
10. **Confused-user next step** — the failing command names the root, shard, provenance field, or timing comparison and points to `docs/architecture/testing.md`.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** ownership first, reuse second, isolation and sharding third, CI parity fourth, evidence last. A fast graph is unacceptable until completeness and isolation can fail closed.
- **Alternatives considered:** cache tuning alone leaves the serial kcov run and duplicate integration ownership intact; deleting the normal integration lane without shard isolation saves compute but leaves CI's long pole; replacing kcov before profiling discards M164_002's proven denominator machinery.
- **Patch-vs-refactor verdict:** this is a **refactor** because execution ownership, coverage aggregation, datastore isolation, and two CI workflows must change as one graph to make the speedup real in every environment.

## Discovery (consult log)

- **Consults** — Architecture: the implementation keeps every coverage producer and consumer in one `test.yml` workflow-run DAG. GitHub artifact storage is run-scoped; splitting producers and the final grader across independently triggered workflows would require polling for another run by commit and defending cancellation, rerun, merge-SHA, and stale-attempt races that have no local equivalent. Branch protection requires contexts named `test` and `test-integration`, so the latter remains a compatibility aggregate while `test` owns the complete substantive graph. Legacy-Design / gate-flag triage: none.
- **Metrics review** — operational verification summaries added; no product analytics or funnel playbook change because no user journey changes.
- **Skill-chain outcomes** — `/write-unit-test`, `/write-integration-test`, `/review`, and `kishore-babysit-prs`: pending execution.
- **Deferrals** — none.
