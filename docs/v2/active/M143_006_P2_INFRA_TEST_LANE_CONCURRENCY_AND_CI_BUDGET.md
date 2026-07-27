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

# M143_006: Zig test lanes run concurrently, and Continuous Integration stops paying for a cold cache

**Prototype:** v2.0.0
**Milestone:** M143
**Workstream:** 006
**Date:** Jul 27, 2026
**Status:** IN_PROGRESS
**Priority:** P2 — tooling; no product surface changes, but every branch pays the current cost on every push.
**Categories:** INFRA
**Batch:** B1 — independent of the M143 product workstreams; touches only test wiring and Continuous Integration (CI).
**Branch:** feat/m143-test-lane-concurrency
**Test Baseline:** unit=3056 integration=405
**Depends on:** M143_004 (established the split unit/integration roots, the kcov coverage lane, and the extracted memleak lane script this workstream builds on)
**Provenance:** agent-generated (measurement-driven; profiling data in Discovery)
**Canonical architecture:** `docs/architecture/testing.md`

---

## Overview

**Goal (testable):** `make memleak` completes materially faster with every lane verdict unchanged, and every Zig CI job restores a warm cache on the first push to a branch instead of rebuilding cold.

**Problem:** A full `make test-integration` takes over eleven minutes locally, of which six are the suite itself — 614 tests executed strictly one at a time, because Zig 0.16's default test runner is single-threaded by construction. The daemon unit binary registers 2958 tests and is executed three separate times per Pull Request (PR): plain, again under kcov, again under Valgrind at a ten-to-thirty-times slowdown. In CI the `memleak` workflow takes fourteen minutes on the first push to any branch and six on every push after, because it never warms its cache from `main`. The Actions cache sits at 9.96 GB against a 10 GB limit, so warm-cache hits are a coin flip for every other job too.

**Solution summary:** Make every serial stretch of the memleak and coverage lanes concurrent — the coverage lane's five kcov components, the memleak lane's per-binary gates, and the boot-drain lane's infra bring-up, which depends on nothing the component lanes produce. Fix the Continuous Integration defects that keep every branch on a cold cache: the missing `main` trigger, the pre-warm step that builds the wrong artifact, the over-broad container privilege, the unbounded cache growth, an uncached lint lane, and a cached path nothing writes. Close a merge-gate hole where the coverage job's verdict was not consumed, and stop cancelling the `main` runs whose only purpose is to warm the cache.

## PR Intent & comprehension handshake

- **PR title (eventual):** `perf(m143): make Zig test lanes concurrent and fix CI cache budget`
- **Intent (one sentence):** Every Zig test lane finishes in a fraction of its current wall clock, locally and in CI, without weakening any correctness, leak, or coverage gate.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `scripts/run-zig-memleak-lane.sh` — the extracted lane script this workstream makes concurrent. Its sibling test drives it with stub tools and asserts overlap from recorded timestamps; that is the pattern any further extraction should follow.
2. `make/bench.mk` — how the three component lanes are dispatched and their verdicts aggregated. Concurrency here must not lose which lane failed.
3. `docs/architecture/testing.md` — the component ownership and lane topology this workstream extends. Update it in the same PR.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `scripts/check_ci_lane_config_test.py` | CREATE | Continuous Integration lane configuration gates. |
| `scripts/check_lane_concurrency_test.py` | CREATE | Lane concurrency and local-cost gates. |
| `scripts/select-prunable-caches.sh` | CREATE | Pure cache-reclamation selection, unit-tested away from the workflow. |
| `scripts/run-zig-memleak-lane.sh` | EDIT | Gate a lane's binaries concurrently. |
| `scripts/check_zig_test_reachability.py` | EDIT | Failure message names the file that should own the registration. |
| `make/test-unit.mk` | EDIT | Concurrent kcov components with per-component exit status. |
| `make/test-integration.mk` | EDIT | Keep-state opt-out for iterative local loops. |
| `make/bench.mk` | EDIT | Overlap the boot-drain lane's infra and migrate with the component lanes. |
| `make/dev.mk` | EDIT | `_clean` removes the cache directory the repository actually uses. |
| `.github/workflows/lint.yml` | EDIT | Cache Zig for the lint job; enforce `check-version`; drop the duplicate pg-drain step. |
| `.github/workflows/bench.yml` | EDIT | Artifact glob matches the extension the bench actually writes. |
| `.github/workflows/cross-compile.yml` | EDIT | Stop cancelling `main` cache-warm runs. |
| `make/harness.mk` | EDIT | Correct references to a `make lint` target that does not exist. |
| `Makefile` | EDIT | Help text follows the renamed lint target. |
| `make/quality.mk` | EDIT | Reconcile `.PHONY`; rename `lint-apps-ds-ctl` off the retired noun. |
| `make/build.mk` | EDIT | Declare `_docker_login` in `.PHONY`. |
| `scripts/check_readme_hero_sync.sh` | DELETE | Checker with no caller — enforcement in appearance only. |
| `scripts/regen-integration-jwts.mjs` | DELETE | One-off regeneration tool, called by nothing. |
| `.github/workflows/memleak.yml` | EDIT | Add the `main` push trigger that warms the cache. |
| `.github/workflows/test-integration.yml` | EDIT | Pre-warm the integration artifact, not the unit artifact; drop the inert global-cache path from the kernel job. |
| `.github/workflows/test.yml` | EDIT | Narrow the coverage job's container privilege; drop the inert global-cache path from four jobs. |
| `.github/workflows/cache-prune.yml` | CREATE | Scheduled reclamation of closed-PR and superseded cache entries. |
| `docs/architecture/testing.md` | EDIT | Correct the test-root topology. |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — **NDC** (no dead code: the sweep removes orphaned targets and scripts rather than leaving them beside their replacements), **ORP** (removing a target or script requires a cross-layer sweep of every caller, doc, and `.PHONY` line), **FLL** (the edited lane scripts stay inside the length caps), **TST-NAM** (no milestone identifiers in the new test names).
- `dispatch/write_shell.md` — the edited lane script and the new cache selector: quoted expansions, array arguments, temporary-file cleanup.
- `dispatch/write_python.md` — the new gate tests: standard-library only, context-managed file handles, specific exceptions.
- `docs/VERIFY_TIERS.md` — the lane commands this workstream changes are the canonical verification surface; the doc moves with them.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | no — no `*.zig` changes remain in the diff | N/A. |
| PUB / Struct-Shape | no | No new Zig surface. |
| File & Function Length (≤350/≤50/≤70) | yes — edited shell and Python | Each stays well inside the caps. |
| UFS (repeated/semantic literals) | yes | Lane names and the cache-selection reasons are named constants in the scripts that emit them. |
| UI Substitution / DESIGN TOKEN | no | No UI surface is touched. |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | no | No product code, no schema, no allocator lifecycle. |

## Prior-Art / Reference Implementations

- **Reference:** `scripts/run-zig-memleak-lane.sh` and its test — the in-repository pattern for lane logic that lives in a script so a test can drive it with stub tools rather than grep the Makefile.
- **Reference:** `.github/workflows/lint.yml` `check-openapi` — regenerate, then `git diff --exit-code`. The generated test roots reuse this exact shape rather than inventing a freshness mechanism.

## Sections (implementation slices)

### §1 — Continuous Integration lane correctness — ✅ DONE

The four CI defects that make every branch pay for a cold cache. Independent of everything below and shippable on its own.

**Implementation default:** the `memleak` `main` trigger mirrors `test.yml`'s existing paths filter rather than inventing a new one, because a filter that disagrees with the sibling workflows is how the next cache gap appears.

- **Dimension 1.1** — ✅ DONE — `memleak.yml` triggers on pushes to `main` under a paths filter, so a fresh branch restores a warm cache → Test `test_memleak_workflow_warms_from_main`
- **Dimension 1.2** — ✅ DONE — `test-integration.yml` pre-warms the artifact the job actually runs → Test `test_integration_workflow_prewarms_integration_binary`
- **Dimension 1.3** — ✅ DONE — the coverage job runs kcov under the narrowest container option set that works, not `--privileged` → Test `test_coverage_job_is_not_privileged`
- **Dimension 1.4** — ✅ DONE — a scheduled workflow reclaims cache entries for closed Pull Requests and superseded key generations, keeping total usage under the limit → Test `test_cache_prune_workflow_targets_closed_and_superseded`

### §5 — Lane concurrency and local cost — ✅ DONE

The remaining serial stretches, plus the local costs the lanes impose outside CI.

- **Dimension 5.1** — ✅ DONE — the coverage lane runs its component binaries under kcov concurrently and merges as before → Test `test_coverage_components_run_concurrently`
- **Dimension 5.2** — ✅ DONE — the memleak `lib` lane gates its three binaries concurrently → Test `test_lib_lane_gates_binaries_concurrently`
- **Dimension 5.3** — ✅ DONE — the boot-drain lane's infra preparation and migration overlap the component lanes instead of following them → Test `test_boot_drain_overlaps_component_lanes`
- **Dimension 5.4** — ✅ DONE — `_clean` removes the cache directory the repository configures, not the ones it abandoned → Test `test_clean_removes_configured_cache`
- **Dimension 5.5** — ✅ DONE — the integration lane offers a documented opt-out from the teardown-and-migrate preamble for iterative local loops, defaulting to the current behaviour → Test `test_integration_keep_state_opt_out`

## Interfaces

```
KEEP_TEST_STATE=1 make test-integration
    Skip the teardown-and-migrate preamble for an iterative local loop.
    Opt-in only; unset (the default, and always in CI) performs the full reset.

scripts/select-prunable-caches.sh <pr-state-file> <retain-per-family> < caches.tsv
    stdin  TSV: id, ref, key, created_at, size_in_bytes
    stdout TSV: id, ref, key, size_in_bytes, reason  (closed-pr | superseded)
    Pure: no network, no deletion. The workflow owns the GitHub input/output.
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| A concurrent lane binary leaks | Any gated binary trips the allocator or Valgrind check | Its lane exits non-zero and the aggregate status propagates. Concurrency never converts a failure into a pass. |
| Concurrent output interleaves | Several binaries write Valgrind reports to one stream | Each binary's output is captured and replayed in list order after the wait, so a report stays readable. |
| Infra bring-up fails while lanes run | Docker unavailable, healthcheck timeout, cert extraction fails | The backgrounded bring-up is waited on separately; a failure aborts before boot-drain rather than running it against datastores that never came up. |
| A kcov component fails | A binary exits non-zero under kcov, or produces no report | Its exit status is recorded per component, the failing component named, its log tail printed, and the lane exits non-zero. |
| Cache prune targets a live entry | The selector's rules are too broad | Only closed-Pull-Request refs and generations beyond the retain count are selected; an unresolvable state is left alone rather than guessed at. |
| Cache entry vanishes mid-prune | A concurrent run evicted it first | Reported as already-gone and skipped — that outcome is the job's own goal, so it is never fatal. |
| `KEEP_TEST_STATE` hides state pollution | A developer reads the opt-out as a clean-checkout pass | Opt-in only and never set by CI, so the gate a Pull Request must clear always performs the full reset. |

## Invariants

1. Concurrency never weakens a verdict — every concurrent lane records a per-unit exit status that the aggregate propagates; enforced by tests that fail a single binary and require a non-zero lane exit.
2. A failing unit's output stays readable and attributable — enforced by tests asserting per-binary replay order and per-component failure naming.
3. The cache prune never selects a restorable entry — enforced by the selector's unit tests, including the unresolvable-state case.
4. The integration gate always resets state unless explicitly opted out — enforced by a test asserting the resolved make graph, not the literal prerequisite.
5. Every Zig test block still reaches a root — enforced by the pre-existing compiler-backed reachability gate, unchanged here.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| not applicable — no product or operator signal changes | not applicable | this workstream touches test wiring, build files, and CI configuration only; the one production-code edit adds a connection-URL capability and emits nothing | not applicable | not applicable | not applicable |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_memleak_workflow_warms_from_main` | The memleak workflow declares a `main` push trigger whose paths filter matches the sibling Zig workflows. |
| 1.2 | unit | `test_integration_workflow_prewarms_integration_binary` | The integration workflow's pre-warm step names the integration artifact and no longer names the unit artifact. |
| 1.3 | unit | `test_coverage_job_is_not_privileged` | The coverage job's container options contain no `--privileged`. |
| 1.4 | unit | `test_cache_prune_workflow_targets_closed_and_superseded` | Given a cache listing with open-Pull-Request, closed-Pull-Request and superseded entries, only the latter two are selected. |
| 5.1 | integration | `test_coverage_components_run_concurrently` | The coverage lane's component runs overlap in time and the merged report is unchanged. |
| 5.2 | integration | `test_lib_lane_gates_binaries_concurrently` | The lib lane's three binary gates overlap in time and a failure in any one fails the lane. |
| 5.3 | integration | `test_boot_drain_overlaps_component_lanes` | The boot-drain preparation starts before the component lanes converge. |
| 5.4 | unit | `test_clean_removes_configured_cache` | After `_clean`, the configured local cache directory is absent. |
| 5.5 | integration | `test_integration_keep_state_opt_out` | The opt-out skips teardown and migration; the default still performs both. |
| regression | integration | `test_lane_verdicts_unchanged` | Every lane produces the same pass/skip/fail totals as before this workstream. |
| regression | integration | `test_reachability_counts_do_not_regress` | The reachability checker's unit and integration counts are greater than or equal to the CHORE(open) baseline. |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Every CI lane defect is corrected (§1) | `make check-gh-actions-valid && python3 scripts/check_ci_lane_config_test.py` | exit 0 | P0 | ✅ exit 0 — actionlint + make-target refs green, 11 lane-config tests pass |
| R2 | Every Zig test block still reaches a root | `make check-test-reachability` | exit 0 | P0 | ✅ `test-root reachability: 536 file(s) reachable` |
| R3 | Lane concurrency preserves attribution and verdict (§5) | `python3 scripts/check_lane_concurrency_test.py` | exit 0 | P0 | ✅ `Ran 9 tests … OK` |
| R4 | Leak gate stays green | `make memleak` | exit 0 | P0 | ✅ `memleak gate passed (agentsfleetd + runner + lib lanes + boot→drain lifecycle)` |
| R5 | Unit lanes stay green | `make test-unit-all` | exit 0 | P0 | ✅ `All unit lanes passed` |
| R6 | Diff stays inside Files Changed | `git diff --name-only origin/main` | 0 paths missing from the Files Changed table | P0 | ✅ 25 files, all in the Files Changed table |
| S1 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | ✅ `All unit lanes passed` |
| S2 | Lint clean | `make lint-all` | exit 0 | P0 | ✅ `All lint checks passed` |
| S3 | Integration passes | `make test-integration` | exit 0 | P0 | ✅ `All integration tests passed` |
| S5 | No leaks | `make memleak` | exit 0 | P0 | ✅ exit 0 |
| S6 | Cross-compile | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` | exit 0 | P0 | ✅ exit 0 — x86_64-linux and aarch64-linux |
| S7 | No secrets | `gitleaks detect` | exit 0 | P0 | ✅ `no leaks found` |
| S8 | No oversize source file | `git diff --name-only main...HEAD \| grep -v '\.md$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | ✅ no output when scoped `main...HEAD`; repo gate `All new Zig files within 350-line limit` |
| S9 | Orphan sweep | Dead Code Sweep greps | 0 matches | P0 | ✅ 0 matches — `_ensure-test-bin`, `_fmt`, and both deleted scripts have no live references |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line (`342 passed`); long evidence goes to PR Session Notes with a pointer here. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE; a P1 ❌ ships only with an Indy-acked deferral quote in Discovery.

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

| File to delete | Verify |
|----------------|--------|
| `scripts/check_readme_hero_sync.sh` | `test ! -f scripts/check_readme_hero_sync.sh` |
| `scripts/regen-integration-jwts.mjs` | `test ! -f scripts/regen-integration-jwts.mjs` |

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| the removed serial kcov component loop | `grep -rn "for component in" make/test-unit.mk` | 0 matches |
| the removed serial binary loop in the memleak lane | `grep -rn 'for binary in "$@"' scripts/run-zig-memleak-lane.sh` | 0 matches |
| `_ensure-test-bin` (orphaned when the memleak lane moved its build into the lane script) | `git grep -n -w -- _ensure-test-bin -- Makefile 'make/*.mk' .github .githooks` | 0 matches |
| `_fmt` (the writing half of the formatter; `_fmt_check` is the gate) | `git grep -n -w -- _fmt -- Makefile 'make/*.mk' .github .githooks` | 0 matches |

## Out of Scope

- Generating test-file registration. The reachability gate already detects an unregistered file, names it, and now names the file that should own it; a generator would save one `_ = @import(…)` line, at the cost of a script that writes into production source. The premise that motivated it — a shared import list every branch appends to — did not survive measurement: 75 production modules already own their test partners, so most new test files never touch a shared root.
- Sharding any test lane. A shard-aware runner was built, proven leak-equivalent, and wired to the daemon unit binary — then removed, because measurement did not support it: 1.44x on the unit lane (17 seconds, floored by one 48s test), no gain available on coverage, and the memleak lane's 14-to-8-minute improvement came entirely from lane concurrency without it. Roughly 300 lines replacing the standard library's leak detector, for 17 seconds, is the wrong trade at these numbers. It becomes the right one only once that one test is addressed; the implementation is in this branch's history.
- Sharding the integration lane. Real isolation needs one Redis instance per shard, not one instance with per-shard logical databases: `FLUSHALL` is not database-scoped and would let one shard's reset destroy its siblings mid-run, and Redis Pub/Sub is not database-scoped either, so shards would receive each other's messages on any shared channel — which this suite asserts on heavily. That is N containers and N certificates for the smaller half of the win; the datastore-free binaries account for more lane time than the integration suite does.
- Amortizing the integration suite's per-test harness setup by sharing Postgres and Redis pools across tests. The suite's cost is a flat ~0.5s floor per test plus a real tail; both are real, and both need their own measurement before anyone touches six hundred test bodies.
- Trimming the slow tests: the integration tail (`catalog`, `dashboard`, `sse_streaming`) and, in the unit lane, `workspace_stream_soak_test`'s concurrent churn at 48.1s — roughly 85% of that lane's wall clock and also run under Valgrind. Every way of making it cheaper weakens a concurrency soak, so it needs an explicit decision and its own measurement, not a bundled change.
- Reducing the number of times the daemon unit binary is executed per Pull Request. Merging the unit, coverage, and leak lanes would couple three independent gates into one verdict, which costs more than it saves.
- Any change to the `cross-compile` workflow beyond what the cache-prune workflow reclaims.

---

## Product Clarity (authoring record)

1. **Successful user moment** — an engineer pushes a branch, and the CI checks that used to make them context-switch have already gone green by the time they finish reading their own diff. Locally, `make test-integration` finishes inside the span of attention that started it.
2. **Preserved user behaviour** — every existing lane command keeps its name and its verdict. `make test-integration`, `make memleak`, `make test-unit-all` and `make test-coverage-zig` all still mean exactly what they mean today; a run with no shard environment set is byte-for-byte the current behaviour.
3. **Optimal-way check** — the unconstrained-optimal shape is a test suite with no shared mutable datastore, where parallelism is free. That is a six-hundred-test refactor. Per-shard datastore isolation buys the same parallelism by preserving the ownership assumption the tests already make, and is the most direct route to the moment.
4. **Rebuild-vs-iterate** — iterate. The lane topology M143_004 established is correct; this workstream makes it concurrent. A rebuild would trade away the determinism the lanes exist to provide.
5. **What we build** — a shard-aware test runner, a fan-out script, per-shard datastore provisioning, a root generator, and four CI configuration corrections.
6. **What we do NOT build** — shared test pools (each test keeps its own setup), a bespoke test scheduler beyond longest-first ordering, a coverage or leak gate rewrite, and any change to what the gates assert.
7. **Fit with existing features** — compounds with M143_004's component ownership and the reachability gate. The one thing it must not destabilize is leak detection: the shard runner replaces the runner that performs it, and a silent weakening there is the worst outcome this workstream could produce.
8. **Surface order** — N/A — no user surface. The only externally visible artifacts are Makefile targets and CI job definitions.
9. **Dashboard restraint** — N/A — no user surface.
10. **Confused-user next step** — every new failure mode names the variable or command that caused it: a malformed shard environment names the variable and its value, a stale generated root prints the regeneration command, and a failing shard's output is surfaced ahead of the passing shards' noise.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** five Sections ordered by independence. §1 ships value with no coupling to anything else and can merge first. §2 is orthogonal to sharding but shares the file set. §3 is the mechanism; §4 is what makes §3 safe for the integration lane; §5 is the leftover serial work the mechanism does not cover.
- **Alternatives considered:** (a) compile-time sharding via generated per-shard test roots — rejected, because each root produces a distinct binary requiring its own full codegen, trading run time for build time on a lane where compile is already half the local cost. (b) `-Dtest-filter` sharding — rejected, because filters prune at compile time and are name-substring based, so the split would be both fragile and N compiles. (c) Leaving the default runner and parallelizing only across the four lanes — rejected, because the integration lane alone is six minutes and would remain the critical path.
- **Patch-vs-refactor verdict:** this is a **patch** with one genuinely new mechanism. Nothing existing is redesigned: the lane topology, the gates, the assertions, and the ownership model all survive unchanged. The single new component is the shard runner, and it is specified as behaviourally equivalent to the upstream runner it replaces.

## Discovery (consult log)

- **Consults** — Architecture / Legacy-Design / gate-flag triage: the question asked + Indy's decision.
  - Baseline measurements taken before authoring, primary checkout, macOS, with a sibling worktree competing for cores: `make test-integration` 11:18 wall clock; the integration binary alone 614 registered tests, 606 passed, 8 skipped, 375.6s; per-test median 0.522s, p90 1.257s, p99 7.473s, max 21.834s, slowest decile carrying 43.3% of total. Daemon unit binary 2958 registered tests. CI: `memleak` 14 min on a branch's first push against 6 min on later pushes; `test-integration` 6–8 min; Actions cache 54 entries totalling 9.96 GB against a 10 GB limit, with no `memleak` entry under `refs/heads/main`.
  - Gate-flag triage pending: the shard runner replaces the upstream runner's leak detection. Indy approved the structural depth; the FILE SHAPE DECISION and the leak-equivalence evidence are due at PLAN and VERIFY respectively.
  - Advisory review (Fable, read-only) over every make target, script and workflow. Verified ten of its claims against the files; all held. Acted on here: the `test` aggregator omitted `test-coverage-zig` from its `needs`, so the coverage gate's verdict blocked nothing; four workflows cancelled their own `main` cache-warm runs; `lint.yml` ran the pg-drain checker twice; `bench.yml` uploaded a `.json` glob against a bench that writes `.csv`; `make lint` was referenced in three places and does not exist; `.PHONY` had a duplicate and two omissions. Two of its claims were wrong on inspection and left alone: `build-linux-alpine` is called by two workflows (not orphaned), and `build-dev` is referenced by a founding playbook — though `build-dev` does invoke a `Dockerfile.dev` that does not exist, so it is broken either way and needs a decision rather than a silent delete. Also acted on: `lint-apps-ds-ctl` renamed to `lint-apps-designsystem-cli`, retiring the old command-line-interface noun — a hard cut with no alias, per the no-compatibility-aliases rule. Its CI job and check name change with it. Closed specs under `docs/v2/done/` keep the old spelling: they are records of what ran at the time. Deferred as separate work: unfiltered Pull Request triggers on three Zig workflows, the make-target reference sweep's blind spot for `make X` inside `sh -c` strings, `release.yml` having no concurrency group at all, and auto-generating `help` from the `##` comments it already duplicates.
  - Measured before fixing the cached-but-unset global cache path: with a warm local cache, a build against a freshly created global cache directory leaves it at 0 bytes and takes the same wall clock (1.5s against 1.9s, inside noise). Zig puts this build graph's output entirely in the local cache. So the fix is to stop listing the inert path, not to set the environment variable and cache an empty directory — five jobs were listing a directory their make targets never create. The three jobs that do set the variable are left as they are: self-consistent, and only the unit build graph was measured.
  - Orphan audit at Indy's request, across all 80 make targets and 30 scripts (excluding self-definitions, `.PHONY` lines, `make help` echo text, and the checkers that scan every target name generically): four dead entries, all pre-existing. `_ensure-test-bin` was orphaned by M143_004's own refactor — extracting the memleak lane to a script moved the build inline and left the helper uncalled. `_fmt` survives only in closed-spec prose and as a string literal in a checker's test fixture, which is why a naive grep reads it as live. `scripts/check_readme_hero_sync.sh` is a checker with no caller — enforcement in appearance only. `scripts/regen-integration-jwts.mjs` is a one-off tool from a closed milestone. `scripts/scopes.admin` was also flagged; Indy chose to keep it. One false positive: `reachability_test_support.py` is imported without its extension by two test files.
  - Indy, on the proposed registration generator and on Makefile surface: dropped both. The generator's motivating premise did not survive measurement — 75 production modules already own their test partners, so the shared-import-list conflict it was solving is rare — and it would have written into production source to save one line the reachability gate already spells out. Its first implementation modelled reachability with a static import walk and rewrote 38 files before the no-op check caught it, which is the shape of risk it carried. A `sync-test-roots` make target was added and then removed: its only caller was a developer typing it, which the repository's own rule forbids. Net make-target count for this workstream is zero.
  - Adversarial review of the proposed Redis isolation, at Indy's request: the logical-database selector this spec originally carried does not isolate anything that matters. `FLUSHALL` is not database-scoped, so a shard's reset would destroy its siblings mid-run; and Redis Pub/Sub is outside the keyspace entirely, so `SELECT n` leaves channels shared — with `redis_client.publish`, `redis_subscriber`, the subscription hub and four integration suites asserting on exact event delivery, that is silent cross-shard bleed reading as SSE flake. Real isolation is one Redis instance per shard. Measuring where the time actually sits settled it: the datastore-free unit binaries account for 1091s of Continuous Integration step time against the integration suite's 479s, because that binary is executed three times per change. Section 4 removed; sharding applies to the datastore-free lanes, which need no isolation at all.
  - Architecture consult during EXECUTE: the roots are the top of a per-module ownership tree, not a flat catalogue — 75 production modules force-import their own `_test.zig` partner, and 229 of 536 candidate files are reachable only transitively through one. Generating flat root lists (the shape this section was originally specified with) would have discarded that structure and risked pulling files into module shapes their imports do not resolve against. Section amended to register at the owning level instead.
  - Architecture consult during EXECUTE: `docs/architecture/testing.md` named `src/runner/integration_tests.zig` as the runner's integration root. That file does not exist — the real root is `src/runner/sandbox_integration_test.zig`, a test file that force-imports three siblings. The doc also listed three roots where eight exist, omitting the auth portability root and both named-module roots. Corrected in this workstream; the omission is why this section was originally scoped to four roots rather than five aggregate plus three hand-authored.
- **Metrics review** — events added, extra events found during `/review`, analytics/funnel playbook update or the explicit no-change reason.
- **Skill-chain outcomes**
  - `/write-unit-test` — 43 tests added across four files, covering every changed surface: Continuous Integration lane configuration, cache-selection rules including the unresolvable-state case, lane concurrency asserted from recorded start/end intervals rather than a wall-clock bound, and the integration reset default asserted from the resolved make graph rather than the literal prerequisite.
  - VERIFY — all fourteen rubric rows graded green against commands run verbatim. `make memleak` and `make test-integration` were re-run after the shard-runner removal and the target rename, since both touched the lanes.
  - Test Delta: unit 3056 → 3056 · integration 405 → 405 vs the CHORE(open) baseline. Zero growth is correct here and not a gap: the diff adds no Zig, and the surfaces it does change — workflows, make fragments, shell — are covered by the 43 tests above, which the Zig reachability counter does not measure.
  - `/review` — run pre-push; an independent adversarial pass examined whether a failing unit's exit status can be lost in the new concurrent paths, whether the cache selector can pick a restorable entry, and whether the rename left a dangling reference.
  - `kishore-babysit-prs` — follows the final push.
- **Deferrals** — every "deferred to follow-up" needs an **Indy-acked verbatim quote** here, format `> Indy (YYYY-MM-DD HH:MM): "<quote>" — context: <which item, why>`. An agent-unilateral deferral is **incomplete scope, not deferral**, and blocks CHORE(close) until the item lands or the quote is captured.
