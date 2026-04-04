# M26_001: Memleak CI Job Speedup

**Prototype:** v1.0.0
**Milestone:** M26
**Workstream:** 001
**Date:** Apr 04, 2026
**Status:** PENDING
**Priority:** P2 — Complete the PR wall-clock reduction started in M24
**Batch:** B1
**Depends on:** M24_001 (merged)

---

## Context

M24_001 (PR #145) parallelised `lint`, `test`, `qa`, and `qa-smoke`, reducing those
workflow durations by 40–60%. However the `memleak` job — which was not modified in
M24 — became the new PR wall-clock bottleneck at ~2.20–2.27 min, versus the old
bottleneck (`qa`) at 2.23 min. Net PR wall clock is therefore ~flat.

The M24 acceptance criterion 5.1 (−30% PR wall clock p50) requires `memleak` to drop
below ~1.56 min. This spec delivers that target.

### Measured baseline (post-M24 branch, Apr 04, 2026)

| Job | Duration | Notes |
|---|---:|---|
| `memleak` | 2.20–2.27 min | Two data points on branch; cache enabled |
| `compile-dev` | 5.40 min | Baseline on main; Zig cache disabled in M24 |

### Why memleak is slow

From the M24 audit:

1. **`mlugg/setup-zig@v2` post step** — 61s of cache upload/persist on every run. The
   memleak job was left with cache enabled because a cold Zig compile took ~4 min without
   it, which was worse. The post-step overhead is the tax paid for that cache.
2. **Sequential structure** — The job runs setup, build, and test in a single
   non-parallelisable sequence. There is no inner parallelism to exploit.
3. **Cache size** — The full `.zig-cache` is uploaded (~425 MB). Reducing the upload to
   only the incremental object cache (`~/.cache/zig`) cuts the post-step time without
   giving up build-time savings.

---

## 1.0 Zig Cache Size Reduction for memleak

**Status:** PENDING

Reduce the size of the Zig cache that `mlugg/setup-zig@v2` persists in the `memleak`
job. The current post-step time (61s) is dominated by uploading the full `.zig-cache`
directory, which includes build artefacts that are not reused across runs.

**Dimensions:**

- 1.1 PENDING Measure current `.zig-cache` directory size in the `memleak` runner by
  adding a `du -sh .zig-cache ~/.cache/zig` step and capturing the output from a live
  run
- 1.2 PENDING Evaluate scoping the `setup-zig` cache path to `~/.cache/zig` only
  (the compiler artefact cache) and excluding build-specific outputs under `.zig-cache`
- 1.3 PENDING Implement the scoped cache path if `mlugg/setup-zig@v2` exposes a
  `cache-dir` input; otherwise pin a custom cache key via `actions/cache` pointing only
  at the compiler cache
- 1.4 PENDING Verify post-step time reduction in a PR CI run after the change; target
  post-step ≤ 20s

---

## 2.0 Shared Cache Key Across Jobs Building the Same Target

**Status:** PENDING

Multiple CI jobs (`memleak`, `lint-zig`, `test-zombied`) rebuild the same Zig target.
Each job populates its own cache with the same artefacts, then pays the full upload cost.
Sharing a single read-only warm cache populated by an upstream job would eliminate the
duplicate upload and reduce cold-build cost on subsequent jobs.

**Dimensions:**

- 2.1 PENDING Audit which jobs build overlapping Zig targets by inspecting `zig build`
  invocations across all workflow files
- 2.2 PENDING Design a shared cache-key strategy: one "warm" job that populates the
  cache first, followed by read-only restore in downstream jobs (no re-upload)
- 2.3 PENDING Implement the shared cache in `memleak` and at least one overlapping job
  (`lint-zig` or `test-zombied`)
- 2.4 PENDING Confirm total CI minutes consumed does not increase (artefact upload is
  billed as part of runner minutes)

---

## 3.0 Memleak Gate Test Scope Review

**Status:** PENDING

The memleak gate runs the full `zig build test` suite with memory-leak detection enabled.
If a subset of tests is responsible for most of the wall-clock time (e.g. slow integration
fixtures), filtering to only the leak-sensitive hot paths could cut duration without
losing coverage.

**Dimensions:**

- 3.1 PENDING Add per-test timing to the memleak run (use `zig build -Dtest-filter` in
  verbose mode or capture timestamps around individual test cases) and identify the
  slowest 20% of tests
- 3.2 PENDING Assess whether the slowest tests are leak-relevant or are already covered
  by the standard `test` workflow (which does not enable leak detection)
- 3.3 PENDING If safe, extract a `MEMLEAK_TEST_FILTER` analogous to
  `BACKEND_E2E_FILTER_*` in `test-e2e.mk` to restrict the memleak gate to leak-sensitive
  tests only
- 3.4 PENDING Measure duration after filter applied; confirm coverage gap is zero (no
  test removed from leak checking that actually exercises memory allocation paths)

---

## 4.0 Acceptance Criteria

**Status:** PENDING

- [ ] 4.1 `memleak` job wall-clock time ≤ 1.50 min on a warm-cache run (measured from
  a PR CI run after changes land)
- [ ] 4.2 `memleak` job wall-clock time ≤ 3.00 min on a cold-cache run (first run after
  cache eviction)
- [ ] 4.3 Overall PR check wall-clock time (critical path across all workflows) reduced
  by ≥ 30% compared to M24 pre-PR baseline of 2.23 min — target ≤ 1.56 min
- [ ] 4.4 No memleak test coverage regression — all tests that previously ran under
  leak detection continue to run under leak detection
- [ ] 4.5 `make lint` passes and no new greptile anti-patterns introduced

---

## 5.0 Out of Scope

- Self-hosted or larger GitHub Actions runners (cost tradeoff requires separate
  evaluation)
- Rewriting or removing memleak tests for speed
- Changing the Zig build system or Zig version
- Optimising `compile-dev` further beyond the Zig cache disabled in M24
- Post-canary production fleet rollout parallelisation
