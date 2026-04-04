# M24_001: CI Pipeline Speedup

**Prototype:** v1.0.0
**Milestone:** M24
**Workstream:** 001
**Date:** Apr 03, 2026
**Status:** IN_PROGRESS
**Branch:** feat/m24-ci-pipeline-speedup
**Priority:** P1 — Reduce CI feedback loop to improve developer velocity
**Batch:** B1
**Depends on:** —

---

## 1.0 Audit Current Pipeline Timings

**Status:** PENDING

Measure wall-clock time for every CI job across all workflows (`lint`, `test`, `test-integration`, `qa-smoke`, `qa`, `memleak`, `compile-dev`, `push-dev`, `deploy-dev`, `release`). Identify the critical path and top 3 bottlenecks. Capture baseline metrics for before/after comparison.

**Dimensions:**
- 1.1 PENDING Collect p50/p90 job durations for the last 20 runs of each workflow
- 1.2 PENDING Map the critical path (longest sequential chain) for PR checks and deploy-dev
- 1.3 PENDING Identify top 3 bottlenecks by wall-clock contribution
- 1.4 PENDING Document baseline metrics in this spec for before/after comparison

---

## 2.0 Zig Build Cache Optimization

**Status:** PENDING

The Zig build cache (`mlugg/setup-zig@v2`) uploads ~425MB per job. Evaluate whether the cache restore actually reduces build time versus a cold build, and tune cache strategy accordingly. Consider sharing a single cache across jobs that build the same target.

**Dimensions:**
- 2.1 PENDING Measure cold build vs cached build time for `compile-dev`, `test`, `lint`, `qa-smoke`
- 2.2 PENDING Deduplicate cache keys — jobs building the same target should share one cache
- 2.3 PENDING Evaluate disabling cache for short-lived jobs where restore time exceeds build time savings
- 2.4 PENDING Reduce cache size by excluding non-essential artifacts from `.zig-cache`

---

## 3.0 Job Parallelization and Dependency Graph

**Status:** PENDING

Restructure job `needs:` dependencies to maximize parallelism. Some jobs may be sequentially chained when they could run concurrently. Evaluate splitting large jobs into smaller parallel units.

**Dimensions:**
- 3.1 PENDING Audit `needs:` graph in `deploy-dev.yml` — identify unnecessary sequential dependencies
- 3.2 PENDING Audit `needs:` graph in `release.yml` — identify jobs that can run in parallel
- 3.3 PENDING Split `lint` into parallel sub-jobs if component linters are independent
- 3.4 PENDING Evaluate running `test` and `test-integration` concurrently where possible

---

## 4.0 Dependency Install Optimization

**Status:** PENDING

`bun install --frozen-lockfile` and `npm install` run in multiple jobs. Evaluate caching `node_modules` and `bun.lockb` across jobs to avoid redundant installs. Playwright browser installs are another repeated cost.

**Dimensions:**
- 4.1 PENDING Cache `node_modules` via `actions/cache` keyed on lockfile hash
- 4.2 PENDING Cache Playwright browser binaries across `qa-smoke` and `qa` jobs
- 4.3 PENDING Evaluate using `actions/setup-node` built-in cache for npm jobs
- 4.4 PENDING Measure install time savings from caching vs cold install

---

## 5.0 Acceptance Criteria

**Status:** PENDING

- [ ] 5.1 PR check wall-clock time reduced by at least 30% (measured p50)
- [ ] 5.2 deploy-dev pipeline end-to-end time reduced by at least 20% (measured p50)
- [ ] 5.3 No CI correctness regressions — all existing tests continue to pass
- [ ] 5.4 Before/after metrics documented with evidence

---

## 6.0 Out of Scope

- Self-hosted runners or Larger runners (cost implications need separate evaluation)
- Rewriting tests for speed (optimize CI infrastructure, not test content)
- Changing the Zig build system itself
- Docker layer caching (already handled by buildx cache)
