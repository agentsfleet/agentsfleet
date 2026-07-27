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
# M143_004: Zig coverage and component test lanes are fast and reusable

**Prototype:** v2.0.0
**Milestone:** M143
**Workstream:** 004
**Date:** Jul 26, 2026
**Status:** DONE
**Priority:** P2 — test infrastructure improves every Zig workstream without blocking product behavior
**Categories:** Infrastructure (INFRA)
**Batch:** B4 — repository tooling
**Branch:** `feat/m143-zig-test-infrastructure`
**Test Baseline:** unit=3056 integration=405
**Depends on:** none
**Provenance:** Indy corrected the original changed-branch-probe idea on Jul 26, 2026; the required outcome is real Zig line coverage plus faster reusable test lanes
**Canonical architecture:** `docs/architecture/runner_fleet.md` §Where the code lives

---

## Overview

**Goal (testable):** `agentsfleet` reports Zig line coverage across the `agentsfleetd`, `agentsfleet-runner`, and shared-library test graphs, while integration and memory-leak verification reuse component-owned graphs instead of rerunning unrelated tests.

**Problem:** The existing backend coverage recipe is orphaned and covers only `agentsfleetd`. The integration aggregate starts the complete daemon test root, so thousands of unit tests run again with live PostgreSQL and Redis configuration. On macOS, memory-leak verification runs every Zig test binary under the allocator and then repeats the same binaries through an advisory `leaks` invocation that cannot inspect the process on this host. Several public Make targets also delegate one-for-one to underscore-prefixed recipes, adding indirection without reuse.

**Solution summary:** Make each Zig component own installable unit and integration test binaries. Public Make targets invoke those graphs directly. A Zig coverage target runs kcov over the component binaries and merges their reports. Integration setup happens once per selected lane. Memory-leak verification builds once per component, uses the repository caches correctly, and runs advisory platform tooling only when a preflight proves it can inspect child processes.

This workstream's Pull Request (PR) keeps the corrected intent explicit:

## PR Intent & comprehension handshake

- **Pull Request (PR) title:** feat(infra): add Zig coverage and reusable test lanes
- **Intent:** A contributor can run the exact Zig component they changed, get real line coverage, and avoid paying for unrelated unit or advisory leak reruns.
- **Handshake:** The first implementation commit removes only valueless one-line underscore wrappers. Shared parameterized helpers remain private because they have multiple callers.

## Implementing agent — read these first

1. `docs/architecture/runner_fleet.md` §Where the code lives — `agentsfleetd` and `agentsfleet-runner` are structurally separate build graphs.
2. `build.zig` and `build_runner.zig` — existing installable test binaries and component modules.
3. `src/agentsfleetd/tests.zig` and `src/runner/tests.zig` — current reachability roots.
4. `make/test-unit.mk`, `make/test-integration.mk`, and `make/bench.mk` — current orchestration and duplicate work.
5. `docs/VERIFY_TIERS.md` — repository verification expectations.
6. `docs/greptile-learnings/RULES.md` — No Dead Code (NDC), No Legacy Retained (NLR), test discovery, orphan sweep, and Make pipeline safety.
7. `dispatch/write_zig.md`, `dispatch/write_any.md`, and `dispatch/write_shell.md` — Zig, cross-cutting, and shell authoring rules.

## Files Changed

| File | Action | Why |
|---|---|---|
| `docs/v2/active/M143_004_P2_INFRA_ZIG_COVERAGE_AND_TEST_LANES.md` | MOVE + EDIT | Open the corrected workstream and record its baseline. |
| `build.zig` | EDIT | Expose distinct installable daemon unit and integration test binaries. |
| `src/build/main.zig` | EDIT | Export the daemon test registration helper. |
| `src/build/daemon_tests.zig` | CREATE | Register daemon unit and integration graphs without growing `build.zig` past its file limit. |
| `src/agentsfleetd/tests.zig` | EDIT | Retain only unit-reachable daemon tests. |
| `src/agentsfleetd/integration_tests.zig` | CREATE | Own the live PostgreSQL, Redis, and QStash daemon integration imports. |
| `make/test-unit.mk` | EDIT | Add public Zig coverage and component binary execution. |
| `make/test-integration.mk` | EDIT | Run component integration binaries and remove one-line underscore wrappers. |
| `make/bench.mk` | EDIT | Reuse component binaries, correct cache propagation, and preflight advisory tooling. |
| `make/test.mk` | EDIT | Declare shared coverage and lane configuration. |
| `make/check-test-reachability.mk` | EDIT | Run the lane orchestration self-tests beside reachability self-tests. |
| `Makefile` | EDIT | Describe the corrected public target surface. |
| `.github/workflows/test.yml` | EDIT | Run Zig coverage as an independent CI job in the kcov-enabled immutable image. |
| `scripts/run-zig-memleak-lane.sh` | CREATE | Keep platform leak behavior directly testable without invoking private Make targets. |
| `scripts/check_zig_test_lanes_test.py` | CREATE | Failure-inject coverage, integration, and memory-lane orchestration. |
| `docs/architecture/testing.md` | CREATE | Record component ownership and future component registration. |
| `docs/architecture/README.md` | EDIT | Link the testing architecture page. |
| `docs/VERIFY_TIERS.md` | EDIT | Describe the new coverage, integration, and memory-leak evidence. |

Any required path missing from this table is added through a specification amendment before editing.

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — No Dead Code (NDC), No Legacy Retained (NLR), No Legacy compatibility shims (NLG), Unified Form for Symbols (UFS), File and Function Length Limits (FLL), Orphan sweep (ORP), test discovery, and Make Pipeline safety (MKP).
- **`dispatch/write_zig.md`** — ownership, public surface, lifecycle, and both Linux cross-builds.
- **`dispatch/write_any.md`** — file shape, logging, error registry, literals, and rule audit.
- **`dispatch/write_shell.md`** — quoted expansions, failure propagation, and repository shell compatibility.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|---|---|---|
| Zig / public surface / lifecycle | yes | test-root files expose no runtime API; both Linux targets compile |
| File and Function Length | yes | component graphs and Make recipes remain separately owned |
| Unified Form for Symbols | yes | paths, thresholds, and artifact names have one declaration |
| Logging / error registry | no | test infrastructure has no new runtime decision path |
| Schema guard | no | no SQL schema changes |
| User Interface / design tokens | no | no user interface files |

## Prior-Art / Reference Implementations

- `build_runner.zig` already separates runner unit and kernel integration roots.
- `build.zig` already installs an `agentsfleetd` test binary for kcov.
- `make/bench.mk` already has a genuinely reused parameterized memory-leak helper.
- `docs/architecture/runner_fleet.md` already makes component build boundaries structural.

## Sections

### §1 — Public targets own real recipes

Delete one-line underscore indirection where a private target has exactly one public caller. Retain underscore-prefixed helpers only when they carry reusable logic for multiple lanes.

- **Dimension 1.1** — daemon integration public targets own their reset dependency and recipe → Make dry-run assertions.
- **Dimension 1.2** — the private memory-leak lane remains because daemon, runner, and shared library all consume it → Make graph assertion.

### §2 — Component-owned Zig test graphs

Split daemon unit imports from live-service integration imports. Keep runner unit and kernel integration roots independent. Shared-library tests remain their own graph. A future `agentsfleet-sched` component joins by defining its own roots and one orchestration row; it does not enlarge the daemon root.

- **Dimension 2.1** — daemon unit execution excludes live-service integration tests → reachability classifier test.
- **Dimension 2.2** — daemon integration execution excludes unrelated daemon unit tests → integration-root test.
- **Dimension 2.3** — aggregate reachability remains at least the recorded baseline → public reachability gate.

### §3 — Real Zig line coverage

`make test-coverage-zig` builds installable test binaries for daemon, runner, and shared libraries, runs them under kcov, merges the component output, writes Cobertura XML and HTML, and enforces a 60% repository line floor against the measured 61.40% baseline. `make test-coverage-all` includes this Zig lane beside the existing JavaScript and TypeScript coverage lanes.

- **Dimension 3.1** — each registered component contributes a non-empty kcov result → coverage artifact test.
- **Dimension 3.2** — merged coverage enforces the named repository floor → below-floor injection test.
- **Dimension 3.3** — missing kcov fails with a direct install hint → tool-absence test.
- **Dimension 3.4** — the CI check publishes the measured percentage and merged HTML report → workflow validation.

### §4 — Faster integration orchestration

The aggregate starts isolated infrastructure once, resets schemas once, and runs only component integration binaries. Component selectors remain independently runnable against the same prepared infrastructure.

- **Dimension 4.1** — `make test-integration` does not execute the daemon unit root → execution-log assertion.
- **Dimension 4.2** — selected daemon integration lanes reuse one infrastructure reset → counter assertion.
- **Dimension 4.3** — isolated Compose project and port allocation remain unchanged → existing isolation tests.

### §5 — Faster memory-leak verification

The memory-leak aggregate builds each component binary once with exported repository caches and runs the three component lanes concurrently. The Zig testing allocator remains the blocking leak proof on macOS. The advisory `leaks` pass runs only after a lightweight preflight proves process inspection works, avoiding a second complete suite when inspection is unavailable. Linux Valgrind behavior remains blocking. The boot-to-drain proof runs after the component lanes converge.

- **Dimension 5.1** — child Zig builds receive both repository cache paths → fake-tool environment test.
- **Dimension 5.2** — failed macOS inspection preflight skips advisory reruns but not allocator execution → fake-tool call-count test.
- **Dimension 5.3** — successful preflight and Linux Valgrind retain their component checks → platform branch tests.

## Interfaces

| Interface | Meaning |
|---|---|
| `make test-coverage-zig` | Run and gate merged Zig line coverage. |
| `make test-unit-agentsfleetd` | Run daemon unit graph only. |
| `make test-unit-agentsfleet-runner` | Run runner unit graph only. |
| `make test-unit-agentsfleet-lib` | Run shared-library unit graphs only. |
| `make test-integration` | Prepare infrastructure once and run registered integration graphs. |
| `make test-integration-db` | Run daemon PostgreSQL integration graph. |
| `make test-integration-redis` | Run daemon Redis integration graph. |
| `make memleak` | Run allocator and available platform leak checks over all registered Zig components. |

## Failure Modes

| Mode | Injection | Handling | Named proof |
|---|---|---|---|
| component omitted from coverage | register binary with no kcov output | fail and name component | coverage artifact test |
| coverage below floor | raise test floor above report | non-zero exit with actual and expected values | below-floor injection |
| unit test leaks into integration | sentinel unit test in daemon root | integration execution log excludes sentinel | integration-root test |
| repeated infrastructure reset | fake reset counter | aggregate requires exactly one reset | reset counter test |
| cache path not propagated | fake Zig executable records environment | both cache values must match repository configuration | cache environment test |
| macOS inspection unavailable | preflight returns non-zero | skip advisory rerun; allocator lane still required | call-count test |
| Linux leak found | fake Valgrind returns non-zero | aggregate fails | platform branch test |

## Invariants

1. Every Zig component owns its unit and integration roots.
2. Aggregates compose component lanes; they do not recreate their recipes.
3. A private Make helper has multiple callers or does not exist.
4. Zig coverage is generated from executed Zig test binaries, not source heuristics.
5. Integration preparation happens once per aggregate invocation.
6. Blocking allocator and Linux Valgrind checks never become advisory.

## Metrics & Observability

| Evidence | Owner | Contents | Privacy guard |
|---|---|---|---|
| merged Zig coverage | engineering | line totals, percentage, component inputs | repository paths only |
| integration timing | engineering | total and component elapsed time | no tenant or secret values |
| memory-leak timing | engineering | build and execution elapsed time per component | no process environment values |

## Test Specification

| Dimension | Tier | Proof |
|---|---|---|
| 1.1–1.2 | unit | Make graph self-test proves public recipes and shared private helper shape |
| 2.1–2.3 | unit | reachability classifier and public reachability gate |
| 3.1–3.4 | integration | kcov component reports, merged floor, missing-tool failure, and CI report publication |
| 4.1–4.3 | integration | execution log, reset counter, and existing Compose isolation tests |
| 5.1–5.3 | unit + integration | fake-tool shell tests plus real `make memleak` |

## Acceptance Rubric

| # | Criterion | Verify | Expected | Graded |
|---|---|---|---|---|
| R1 | underscore cleanup | `rg -n '^_(test-integration-(db|redis|full)):' make` | no hits | ✅ no hits |
| R2 | Zig coverage | `make test-coverage-zig` | merged report exists, floor passes, and CI publishes the summary and HTML artifact | ✅ 61.40% ≥ 60%; image verified |
| R3 | unit graphs | `make test-unit-all` | exit 0 | ✅ 2,202 daemon; 379 runner; 121 library; package coverage green |
| R4 | integration graph | `make test-integration` | exit 0 and no daemon unit rerun | ✅ exit 0; 7.73-minute run |
| R5 | memory graph | `make memleak` | exit 0 without unavailable advisory reruns | ✅ allocator, runner, library, boot-drain green |
| S1 | conformance | `make harness-verify` | exit 0 | ✅ all gates green |
| S2 | lint | `make lint-all` | exit 0 | ✅ all lint checks passed |
| S3 | Linux builds | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` | both exit 0 | ✅ both targets passed |
| S4 | version | `make check-version` | exit 0 | ✅ versions match 0.22.1 |
| S5 | secrets | `gitleaks detect` | exit 0 | ✅ no leaks found |

## Dead Code Sweep

Remove the orphan daemon-only coverage recipe, the changed-branch-probe design, and every single-caller private integration wrapper. Before close, grep every removed target and old specification name across `make/`, `docs/`, scripts, and Continuous Integration (CI) configuration.

## Out of Scope

- Source-level Zig branch mapping, custom probes, or a parser for changed branches.
- Changing product runtime behavior, API behavior, SQL schema, or deployment configuration.
- Continuous Integration configuration beyond the Indy-approved dedicated coverage job.
- Requiring immediate 100% Zig line coverage; the floor starts at 60% and ratchets upward with production-path tests.

## Product Clarity

1. **Successful contributor moment** — one command reports merged Zig coverage, and component commands run only their own graph.
2. **Preserved product behavior** — no runtime path changes.
3. **Optimal-way check** — build-graph separation removes duplicate execution at its source.
4. **What we build** — component roots, merged line coverage, direct public recipes, single-reset integration, and capability-aware leak verification.
5. **What we do not build** — custom branch probes, duplicate wrappers, or a scheduler implementation.
6. **Future fit** — `agentsfleet-sched` adds a component row and owned roots without modifying daemon tests.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** separate component roots and compose them through the existing public Make surface.
- **Rejected:** a custom changed-branch parser and source probes, because they do not provide repository Zig coverage and tax production edits.
- **Rejected:** keeping the monolithic daemon root and filtering at runtime, because compilation and discovery still traverse unrelated tests.
- **Patch-vs-refactor verdict:** refactor the existing build and Make graphs in place; do not add a parallel compatibility surface.

## Discovery

- The existing daemon coverage recipe is orphaned and absent from `test-coverage-all`.
- The daemon integration target currently invokes the complete 2,622-test daemon binary.
- macOS advisory inspection rejects this repository's test process after the binary has already run, doubling work without evidence.
- Memory-leak recipes assign cache variables without exporting them to child Zig builds.
- Docker Compose isolation is already current on `main`; this workstream does not revisit collision handling.
- Architecture testing topology was silent; `docs/architecture/testing.md` will land with the implementation.
- The first merged run measured 61.40% Zig line coverage; the blocking floor starts at 60% to prevent material regression.
- **Deferrals:** none.
