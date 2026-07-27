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

# M143_006: Zig test lanes run sharded and concurrent, and Continuous Integration stops paying for a cold cache

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

**Goal (testable):** the datastore-free lanes — `make test-unit-agentsfleetd`, `make test-coverage-zig`, `make memleak` — execute their registered tests across `SHARD_COUNT` concurrent processes with the leak, failure and coverage verdicts unchanged, and every Zig CI job restores a warm cache on the first push to a branch.

**Problem:** A full `make test-integration` takes over eleven minutes locally, of which six are the suite itself — 614 tests executed strictly one at a time, because Zig 0.16's default test runner is single-threaded by construction. The daemon unit binary registers 2958 tests and is executed three separate times per Pull Request (PR): plain, again under kcov, again under Valgrind at a ten-to-thirty-times slowdown. In CI the `memleak` workflow takes fourteen minutes on the first push to any branch and six on every push after, because it never warms its cache from `main`. The Actions cache sits at 9.96 GB against a 10 GB limit, so warm-cache hits are a coin flip for every other job too.

**Solution summary:** Add a shard-aware Zig test runner so one compiled test binary can be executed by N concurrent processes, each running a disjoint subset, and apply it to the binaries that touch no datastore — which is where the time actually is, since the daemon unit binary is executed three times per change and accounts for more lane time than the integration suite. Register each new test file at the level that already owns it, so it needs no hand edit. Make the remaining serial stretches of the memleak and coverage lanes concurrent. Fix the four CI defects that make the cache cold: the missing `main` trigger, the pre-warm step that builds the wrong artifact, the over-broad container privilege, and the unbounded cache growth.

## PR Intent & comprehension handshake

- **PR title (eventual):** `perf(m143): shard Zig test lanes and fix CI cache budget`
- **Intent (one sentence):** Every Zig test lane finishes in a fraction of its current wall clock, locally and in CI, without weakening any correctness, leak, or coverage gate.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `src/build/test_runner_list.zig` — the existing custom `.simple` test runner. The shard runner is its sibling: same `builtin.test_functions` walk, but it executes rather than lists. Mirror its allocator discipline and its stance on best-effort output.
2. `$(zig env | grep lib_dir)/compiler/test_runner.zig` `mainTerminal` — the upstream runner the shard runner must remain behaviourally equivalent to, including per-test allocator reset, `Io` instance lifecycle, leak detection, skip/fail accounting and exit code.
3. `src/build/test_list.zig` — how a custom runner is attached to a `Compile` without disturbing the default-runner lanes; explains why the executing lanes were deliberately left on the default runner.
4. `docs/architecture/testing.md` — the component ownership and lane topology this workstream extends. Update it in the same PR.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/build/test_runner_shard.zig` | CREATE | Shard-aware executing test runner; the one new mechanism this workstream rests on. |
| `src/build/daemon_tests.zig` | EDIT | Attach the shard runner to the installed daemon unit and integration artifacts. |
| `src/build/lib_tests.zig` | EDIT | Same attachment for the shared-library binaries. |
| `build_runner.zig` | EDIT | Same attachment for the runner graph's installed test artifacts. |
| `scripts/run-zig-shards.sh` | CREATE | Fans a built test binary out over N shard processes, aggregates exit codes, preserves per-shard output. |
| `scripts/check_zig_shard_runner_test.py` | CREATE | Leak-equivalence, partition-exactness and malformed-environment gates for the shard runner. |
| `scripts/check_ci_lane_config_test.py` | CREATE | Continuous Integration lane configuration gates. |
| `scripts/check_lane_concurrency_test.py` | CREATE | Lane concurrency and local-cost gates. |
| `scripts/select-prunable-caches.sh` | CREATE | Pure cache-reclamation selection, unit-tested away from the workflow. |
| `scripts/run-zig-memleak-lane.sh` | EDIT | Gate a lane's binaries concurrently. |
| `scripts/check_zig_test_reachability.py` | EDIT | Failure message names the file that should own the registration. |
| `make/test-unit.mk` | EDIT | Sharded unit lane; concurrent kcov components. |
| `make/test-integration.mk` | EDIT | Keep-state opt-out for iterative local loops. |
| `make/bench.mk` | EDIT | Overlap the boot-drain lane's infra and migrate with the component lanes. |
| `make/dev.mk` | EDIT | `_clean` removes the cache directory the repository actually uses. |
| `.github/workflows/lint.yml` | EDIT | Cache Zig for the lint job; enforce `check-version`. |
| `scripts/check_readme_hero_sync.sh` | DELETE | Checker with no caller — enforcement in appearance only. |
| `scripts/regen-integration-jwts.mjs` | DELETE | One-off regeneration tool, called by nothing. |
| `.github/workflows/memleak.yml` | EDIT | Add the `main` push trigger that warms the cache. |
| `.github/workflows/test-integration.yml` | EDIT | Pre-warm the integration artifact, not the unit artifact; drop the inert global-cache path from the kernel job. |
| `.github/workflows/test.yml` | EDIT | Narrow the coverage job's container privilege; drop the inert global-cache path from four jobs. |
| `.github/workflows/cache-prune.yml` | CREATE | Scheduled reclamation of closed-PR and superseded cache entries. |
| `docs/architecture/testing.md` | EDIT | Document sharding, generated roots, and per-shard isolation. |
| `docs/VERIFY_TIERS.md` | EDIT | Record the sharded lane commands and the shard environment variables. |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — **UFS** (shard/lane identifiers and the shard environment variable names become named constants shared verbatim across the Zig runner, the shell fan-out, and the Makefile), **FLL** (the new runner and the generator both stay inside the file and function length caps), **NDC** (the hand-maintained root import lists are replaced, not left beside their generated successors), **ORP** (removing the hand-maintained roots and any superseded lane helper requires a cross-layer sweep), **XCC** (Zig changes cross-compile to both Linux targets before commit), **TST-NAM** (no milestone identifiers in the new test names), **NSQ** (the per-shard database provisioning uses named constants and schema-qualified statements).
- `dispatch/write_zig.md` — the shard runner is new Zig: memory safety, `errdefer` placement, pub-surface shape verdict, and the length caps all apply.
- `dispatch/write_shell.md` — `scripts/run-zig-shards.sh` and the edited lane script: quoted expansions, array arguments, temporary-file cleanup.
- `dispatch/write_python.md` — the root generator: standard-library only, context-managed file handles, specific exceptions.
- `docs/VERIFY_TIERS.md` — the lane commands this workstream changes are the canonical verification surface; the doc moves with them.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes — new and edited `*.zig` | Cross-compile both Linux targets; tagged-union results in the runner's outcome accounting; `errdefer` on every allocation in the shard runner. |
| PUB / Struct-Shape | yes — `src/build/test_runner_shard.zig` is a new file with a public entry point | File Shape Decision recorded at PLAN; the runner is operations-over-value like its `test_runner_list.zig` sibling. |
| File & Function Length (≤350/≤50/≤70) | yes | The runner splits its shard selection, its per-test execution, and its result reporting into separate functions; the generator splits discovery from emission. |
| UFS (repeated/semantic literals) | yes | Shard environment variable names, the lane names, and the generated-root banner are named constants; the Zig runner and the shell fan-out share the identifier spelling verbatim. |
| UI Substitution / DESIGN TOKEN | no | No UI surface is touched. |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | LIFECYCLE yes; others no | The shard runner owns an allocator and an `Io` instance per test and must pair every init with its deinit exactly as the upstream runner does. No schema migration, no new error registry code, no product logging. |

## Prior-Art / Reference Implementations

- **Reference:** `src/build/test_runner_list.zig` — the in-repository proof that a custom runner attaches cleanly to a `Compile` and reads `builtin.test_functions`. The shard runner diverges only in executing the functions and in replicating the upstream runner's accounting.
- **Reference:** upstream `lib/compiler/test_runner.zig` `mainTerminal` — the behavioural contract for leak detection, skip/fail counting, and exit code. Divergence from it is a weakened gate, not a design choice.
- **Reference:** `.github/workflows/lint.yml` `check-openapi` — regenerate, then `git diff --exit-code`. The generated test roots reuse this exact shape rather than inventing a freshness mechanism.

## Sections (implementation slices)

### §1 — Continuous Integration lane correctness — ✅ DONE

The four CI defects that make every branch pay for a cold cache. Independent of everything below and shippable on its own.

**Implementation default:** the `memleak` `main` trigger mirrors `test.yml`'s existing paths filter rather than inventing a new one, because a filter that disagrees with the sibling workflows is how the next cache gap appears.

- **Dimension 1.1** — ✅ DONE — `memleak.yml` triggers on pushes to `main` under a paths filter, so a fresh branch restores a warm cache → Test `test_memleak_workflow_warms_from_main`
- **Dimension 1.2** — ✅ DONE — `test-integration.yml` pre-warms the artifact the job actually runs → Test `test_integration_workflow_prewarms_integration_binary`
- **Dimension 1.3** — ✅ DONE — the coverage job runs kcov under the narrowest container option set that works, not `--privileged` → Test `test_coverage_job_is_not_privileged`
- **Dimension 1.4** — ✅ DONE — a scheduled workflow reclaims cache entries for closed Pull Requests and superseded key generations, keeping total usage under the limit → Test `test_cache_prune_workflow_targets_closed_and_superseded`

### §3 — Shard-aware test runner — IN_PROGRESS (mechanism DONE; coverage + memleak lanes not yet wired)

The single mechanism the remaining sections rest on. One compiled binary, N processes, disjoint test subsets, aggregated verdict.

**Implementation default:** shard assignment is by index modulo count over the compiler-registered order, and the fan-out assigns the longest-known tests first where a prior timing record exists, because the integration suite's slowest ten percent carry forty-three percent of its runtime and a naive split leaves one shard running alone.

- **Dimension 3.1** — ✅ DONE — the runner executes exactly the tests whose index satisfies the shard predicate, and the union across all shards is the full registered set with no overlap → Test `test_shard_partition_is_exact_and_disjoint`
- **Dimension 3.2** — ✅ DONE — the runner detects a `std.testing.allocator` leak and exits non-zero, matching the upstream runner → Test `test_shard_runner_fails_on_leak`
- **Dimension 3.3** — ✅ DONE — skips, failures, and logged errors are counted and reported per shard, and the aggregate exit code is non-zero when any shard fails → Test `test_shard_runner_reports_and_propagates`
- **Dimension 3.4** — ✅ DONE — with no shard environment set, the runner behaves as a single shard covering every test → Test `test_unsharded_default_runs_everything`
- **Dimension 3.5** — ✅ DONE — the fan-out script preserves each shard's output and surfaces the failing shard's output first → Test `test_fanout_preserves_shard_output`

### §5 — Lane concurrency and local cost — ✅ DONE

The remaining serial stretches, plus the local costs the lanes impose outside CI.

- **Dimension 5.1** — ✅ DONE — the coverage lane runs its component binaries under kcov concurrently and merges as before → Test `test_coverage_components_run_concurrently`
- **Dimension 5.2** — ✅ DONE — the memleak `lib` lane gates its three binaries concurrently → Test `test_lib_lane_gates_binaries_concurrently`
- **Dimension 5.3** — ✅ DONE — the boot-drain lane's infra preparation and migration overlap the component lanes instead of following them → Test `test_boot_drain_overlaps_component_lanes`
- **Dimension 5.4** — ✅ DONE — `_clean` removes the cache directory the repository configures, not the ones it abandoned → Test `test_clean_removes_configured_cache`
- **Dimension 5.5** — ✅ DONE — the integration lane offers a documented opt-out from the teardown-and-migrate preamble for iterative local loops, defaulting to the current behaviour → Test `test_integration_keep_state_opt_out`

## Interfaces

```
Shard selection (read by src/build/test_runner_shard.zig, set by scripts/run-zig-shards.sh):
  AGENTSFLEET_TEST_SHARD_INDEX   0-based; absent or empty => single-shard mode
  AGENTSFLEET_TEST_SHARD_COUNT   >=1; absent or empty => 1

Fan-out:
  scripts/run-zig-shards.sh <shard-count> <binary-path> [runner-prefix...]
    exit 0 iff every shard exits 0; otherwise the first failing shard's code.

Redis connection URL (src/agentsfleetd/queue/redis_config.zig):
  rediss://[:password@]host[:port][/<logical-db>]
    absent path component => logical database 0 (today's behaviour, unchanged)

Generated roots:
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Shard count exceeds registered tests | Caller asks for more shards than there are tests | High-index shards run zero tests and exit 0; the aggregate verdict is unaffected. |
| Shard environment is malformed | Non-numeric or negative shard index or count | The runner exits non-zero with a message naming the variable and its value; it never silently degrades to running everything. |
| One shard crashes without reporting | Segmentation fault or abort inside a test process | The fan-out treats a missing verdict as failure, surfaces that shard's captured output first, and exits non-zero. |
| Shard database provisioning collides | A prior aborted run left a shard database behind | Provisioning drops and recreates the shard database; a failure to do so aborts the lane rather than reusing unknown state. |
| Redis selector unsupported by server | A deployment points at a Redis that rejects logical database selection | Connection fails with the server's error surfaced verbatim; no silent fallback to database 0, which would merge shard keyspaces. |
| Generated root drifts from disk | A test file is added without regenerating | `lint-zig` fails with the diff between committed and regenerated roots and the command to fix it. |
| Cache prune deletes a live cache | The prune workflow's selection is too broad | Prune only targets closed-Pull-Request refs and key generations older than the newest retained per prefix; a dry-run mode is the default for manual invocation. |
| Leak detection lost in a shard | The shard runner omits the upstream allocator check | Dimension 3.2's test injects a deliberate leak and requires a non-zero exit; the gate fails closed. |

## Invariants

1. The union of every shard's executed test set equals the full registered set, with no test executed twice — enforced by Dimension 3.1's test asserting partition exactness against `list-tests` output.
2. A leak, failure, or logged error in any shard produces a non-zero lane exit — enforced by the fan-out's exit-code aggregation and Dimension 3.2/3.3 tests, not by reading output.
3. No two concurrent integration shards share a Postgres database or a Redis keyspace — enforced by per-shard provisioning that derives names from the shard index and by Dimension 4.3's mutual-invisibility test.
4. Every Zig `test` block reachable on disk appears in exactly one generated root or carries a waiver — enforced by the existing reachability checker plus the regenerate-and-diff gate.
5. An unset shard environment yields today's behaviour exactly — enforced by Dimension 3.4's test, so any lane not yet migrated is unaffected.

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
| 3.1 | unit | `test_shard_partition_is_exact_and_disjoint` | Across every shard index for a given count, each registered test executes exactly once. |
| 3.2 | integration | `test_shard_runner_fails_on_leak` | A binary containing one deliberately leaking test exits non-zero under the shard runner. |
| 3.3 | unit | `test_shard_runner_reports_and_propagates` | A mixed pass/skip/fail set yields the correct counts and a non-zero exit. |
| 3.4 | unit | `test_unsharded_default_runs_everything` | With no shard environment set, the executed count equals the registered count. |
| 3.5 | integration | `test_fanout_preserves_shard_output` | With one failing shard among several, the failing shard's output is surfaced first and the script exits non-zero. |
| 5.1 | integration | `test_coverage_components_run_concurrently` | The coverage lane's component runs overlap in time and the merged report is unchanged. |
| 5.2 | integration | `test_lib_lane_gates_binaries_concurrently` | The lib lane's three binary gates overlap in time and a failure in any one fails the lane. |
| 5.3 | integration | `test_boot_drain_overlaps_component_lanes` | The boot-drain preparation starts before the component lanes converge. |
| 5.4 | unit | `test_clean_removes_configured_cache` | After `_clean`, the configured local cache directory is absent. |
| 5.5 | integration | `test_integration_keep_state_opt_out` | The opt-out skips teardown and migration; the default still performs both. |
| regression | integration | `test_lane_verdicts_unchanged_unsharded` | Every lane run with a shard count of one produces the same pass/skip/fail totals as before this workstream. |
| regression | integration | `test_reachability_counts_do_not_regress` | The reachability checker's unit and integration counts are greater than or equal to the CHORE(open) baseline. |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Every CI lane defect is corrected (§1) | `make check-gh-actions-valid && python3 scripts/check_ci_lane_config_test.py` | exit 0 | P0 | |
| R2 | Every Zig test block still reaches a root | `make check-test-reachability` | exit 0 | P0 | |
| R3 | Shard partition is exact, disjoint, and leak-detecting (§3) | `python3 -m unittest discover -s scripts -t scripts -p 'check_zig_shard*_test.py'` | exit 0 | P0 | |
| R4 | Sharded lanes keep the leak verdict (§3) | `make memleak` | exit 0 | P0 | |
| R5 | Sharded and unsharded verdicts agree (§3) | `AGENTSFLEET_TEST_SHARD_COUNT=1 make test-unit-agentsfleetd` | exit 0 | P0 | |
| R6 | Diff stays inside Files Changed | `git diff --name-only origin/main` | 0 paths missing from the Files Changed table | P0 | |
| S1 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S2 | Lint clean | `make lint-all` | exit 0 | P0 | |
| S3 | Integration passes | `make test-integration` | exit 0 | P0 | |
| S5 | No leaks | `make memleak` | exit 0 | P0 | |
| S6 | Cross-compile | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` | exit 0 | P0 | |
| S7 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S8 | No oversize source file | `git diff --name-only origin/main \| grep -v '\.md$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | |
| S9 | Orphan sweep | Dead Code Sweep greps | 0 matches | P0 | |

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
- Sharding the integration lane. Real isolation needs one Redis instance per shard, not one instance with per-shard logical databases: `FLUSHALL` is not database-scoped and would let one shard's reset destroy its siblings mid-run, and Redis Pub/Sub is not database-scoped either, so shards would receive each other's messages on any shared channel — which this suite asserts on heavily. That is N containers and N certificates for the smaller half of the win; the datastore-free binaries account for more lane time than the integration suite does.
- Amortizing the per-test harness setup by sharing Postgres and Redis pools across tests within a shard. Sharding cuts the same wall clock without touching six hundred test bodies; pool sharing is a follow-up once shard counts stop scaling.
- Trimming the individual slow tests in the tail (`catalog`, `dashboard`, `sse_streaming`). Real, but each needs its own behavioural judgement and none blocks this workstream.
- Reducing the number of times the daemon unit binary is executed per Pull Request. Merging the unit, coverage, and leak lanes would couple three independent gates; sharding makes each cheap enough that the coupling is not worth buying.
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
  - Measured before fixing the cached-but-unset global cache path: with a warm local cache, a build against a freshly created global cache directory leaves it at 0 bytes and takes the same wall clock (1.5s against 1.9s, inside noise). Zig puts this build graph's output entirely in the local cache. So the fix is to stop listing the inert path, not to set the environment variable and cache an empty directory — five jobs were listing a directory their make targets never create. The three jobs that do set the variable are left as they are: self-consistent, and only the unit build graph was measured.
  - Orphan audit at Indy's request, across all 80 make targets and 30 scripts (excluding self-definitions, `.PHONY` lines, `make help` echo text, and the checkers that scan every target name generically): four dead entries, all pre-existing. `_ensure-test-bin` was orphaned by M143_004's own refactor — extracting the memleak lane to a script moved the build inline and left the helper uncalled. `_fmt` survives only in closed-spec prose and as a string literal in a checker's test fixture, which is why a naive grep reads it as live. `scripts/check_readme_hero_sync.sh` is a checker with no caller — enforcement in appearance only. `scripts/regen-integration-jwts.mjs` is a one-off tool from a closed milestone. `scripts/scopes.admin` was also flagged; Indy chose to keep it. One false positive: `reachability_test_support.py` is imported without its extension by two test files.
  - Indy, on the proposed registration generator and on Makefile surface: dropped both. The generator's motivating premise did not survive measurement — 75 production modules already own their test partners, so the shared-import-list conflict it was solving is rare — and it would have written into production source to save one line the reachability gate already spells out. Its first implementation modelled reachability with a static import walk and rewrote 38 files before the no-op check caught it, which is the shape of risk it carried. A `sync-test-roots` make target was added and then removed: its only caller was a developer typing it, which the repository's own rule forbids. Net make-target count for this workstream is zero.
  - Adversarial review of the proposed Redis isolation, at Indy's request: the logical-database selector this spec originally carried does not isolate anything that matters. `FLUSHALL` is not database-scoped, so a shard's reset would destroy its siblings mid-run; and Redis Pub/Sub is outside the keyspace entirely, so `SELECT n` leaves channels shared — with `redis_client.publish`, `redis_subscriber`, the subscription hub and four integration suites asserting on exact event delivery, that is silent cross-shard bleed reading as SSE flake. Real isolation is one Redis instance per shard. Measuring where the time actually sits settled it: the datastore-free unit binaries account for 1091s of Continuous Integration step time against the integration suite's 479s, because that binary is executed three times per change. Section 4 removed; sharding applies to the datastore-free lanes, which need no isolation at all.
  - Architecture consult during EXECUTE: the roots are the top of a per-module ownership tree, not a flat catalogue — 75 production modules force-import their own `_test.zig` partner, and 229 of 536 candidate files are reachable only transitively through one. Generating flat root lists (the shape this section was originally specified with) would have discarded that structure and risked pulling files into module shapes their imports do not resolve against. Section amended to register at the owning level instead.
  - Architecture consult during EXECUTE: `docs/architecture/testing.md` named `src/runner/integration_tests.zig` as the runner's integration root. That file does not exist — the real root is `src/runner/sandbox_integration_test.zig`, a test file that force-imports three siblings. The doc also listed three roots where eight exist, omitting the auth portability root and both named-module roots. Corrected in this workstream; the omission is why this section was originally scoped to four roots rather than five aggregate plus three hand-authored.
- **Metrics review** — events added, extra events found during `/review`, analytics/funnel playbook update or the explicit no-change reason.
- **Skill-chain outcomes** — `/write-unit-test`, `/review`, `kishore-babysit-prs` results (order per `AGENTS.md` CHORE(close); iteration counts, findings dispositioned).
- **Deferrals** — every "deferred to follow-up" needs an **Indy-acked verbatim quote** here, format `> Indy (YYYY-MM-DD HH:MM): "<quote>" — context: <which item, why>`. An agent-unilateral deferral is **incomplete scope, not deferral**, and blocks CHORE(close) until the item lands or the quote is captured.
