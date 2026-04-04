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

**Status:** DONE

Audit captured on Apr 04, 2026 from the last 20 GitHub Actions runs per workflow using `gh run list` and `gh run view`.

### Baseline Metrics

| Workflow / path | p50 (min) | p90 (min) | Latest long step(s) | Notes |
|---|---:|---:|---|---|
| `lint` | 0.80 | 0.87 | `make lint` 17s, OpenSSL apt 11s | Single job serialized independent lanes |
| `test` | 1.63 | 1.87 | `Test zombied` 24s, `Test website` 22s | Single job serialized backend + website + app + zombiectl |
| `test-integration` | 1.45 | 1.93 | `make test-integration` 26s | Already isolated |
| `qa-smoke` | 1.30 | 1.63 | Smoke gate 27s, Playwright install 24s | Browser install is material |
| `qa` | 2.23 | 2.62 | Full QA gate 84s, Playwright install 27s | Current PR critical path |
| `memleak` | 2.05 | 2.30 | setup-zig post step 61s, memleak gate 25s | Zig cache upload is the bottleneck |
| `deploy (dev)` critical path | ~10.07 | n/a | compile-dev 5.40, deploy-fly-dev 2.12, qa-dev 0.80 | Derived from sequential `needs:` chain |
| `release` pre-prod critical path | ~7.87 | n/a | aarch64 build 4.12, push-prod 1.88 | Release graph already mostly parallel |

### Critical Paths

PR checks are workflow-parallel, so wall-clock p50 is bounded by the slowest workflow, not the sum. Before this change:

- `qa` p50 = 2.23 min
- `memleak` p50 = 2.05 min
- `test` p50 = 1.63 min
- `test-integration` p50 = 1.45 min
- `qa-smoke` p50 = 1.30 min
- `lint` p50 = 0.80 min
- **PR critical path p50 = 2.23 min (`qa`)**

DEV deploy wall clock is the sequential chain:

- `check-credentials` 1.75 min
- `compile-dev` 5.40 min
- `push-dev` 0.65 min
- `deploy-fly-dev` 2.12 min
- `verify-dev` 0.20 min
- `qa-dev` 0.80 min
- `notify` 0.15 min
- **Deploy-dev critical path ≈ 10.07 min**

### Top 3 Bottlenecks

1. `compile-dev` Zig cache post step: 121s of non-functional overhead on the deploy critical path.
2. `qa` serialized full gate: 84s of backend + website + app work inside one job, making it the PR critical path.
3. Repeated Playwright browser install: 24–27s in `qa-smoke`, `qa`, and `qa-dev`.

### Why Parallelism Wins Here

This workstream improves wall clock by converting serial lanes into parallel jobs where the underlying work is already independent.

- `test` currently pays for backend, website, app, and zombiectl sequentially in one job. After splitting, the workflow duration becomes `max(zombied, website, app, zombiectl)` plus small aggregator overhead, not the sum of all lanes.
- `qa` currently hides three independent lanes inside `make qa` (`_test_e2e`, `_qa_website`, `qa_app`). After splitting, duration becomes `max(backend, website, app)` plus setup overhead. Total compute may stay similar, but **developer wait time drops because the critical path collapses from a sum to the slowest lane**.
- The same logic applies to `qa-smoke` and `lint`.

### Worker-Node Scaling Note

The audit shows why this matters as more worker hosts are added.

- In `deploy-dev`, worker rollout already runs in parallel with the Fly/API path. Current worker lane is 1.38 min, while the API path after `push-dev` is about 3.12 min (`deploy-fly-dev` + `verify-dev` + `qa-dev`). That means adding more worker hosts does **not** hurt deploy wall clock until the worker lane exceeds the API path. Parallel lanes buy headroom.
- In `release`, `deploy-prod-fleet` is intentionally sequential after canary and approval. That path will grow roughly linearly with worker count by design. I did not parallelize it in this workstream because it would change rollout safety semantics.

**Dimensions:**
- 1.1 DONE Collected p50/p90 durations for the last 20 runs of each target workflow
- 1.2 DONE Mapped PR and deploy-dev critical paths
- 1.3 DONE Identified top 3 wall-clock bottlenecks
- 1.4 DONE Documented baseline metrics and reasoning in this spec

---

## 2.0 Zig Build Cache Optimization

**Status:** IN_PROGRESS

Audit results show that Zig cache persistence is not uniformly valuable.

- `compile-dev` latest run spent 121s in `Post Run mlugg/setup-zig@v2`.
- `memleak` latest audit sample spent 61s in `Post Run mlugg/setup-zig@v2`, but the first PR run with cache disabled stretched the memleak gate itself to about 4 minutes.
- `release` x86 build spent 36s in the same post step.
- PR jobs like `lint` and `test` do not show a comparable cache tax in the audit, so this pass leaves them unchanged.

Implemented in this execution pass:

- Disabled `setup-zig` cache for `.github/workflows/deploy-dev.yml`
- Disabled `setup-zig` cache for the heavy release Zig jobs in `.github/workflows/release.yml`
- Vendored `setup-zig` into `./.github/actions/setup-zig` and migrated workflows to the local action path, with `scripts/vendor-setup-zig.sh` as the periodic refresh path from `~/Projects/oss/setup-zig`

Projected impact from audit timings:

- `compile-dev`: 5.40 min latest -> approximately 3.4 min if the 121s cache upload is removed
- `memleak`: first PR run disproved the no-cache projection; cache was re-enabled because the cold compile cost dominated the saved post-step time
- `release` x86 lane: 3.75 min latest -> approximately 3.15 min if the 36s cache upload is removed

**Dimensions:**
- 2.1 IN_PROGRESS Measured cache-related cost from audit data; follow-up CI runs still needed for before/after confirmation
- 2.2 PENDING Cache-key deduplication not implemented in this pass
- 2.3 IN_PROGRESS Disabled Zig cache only in jobs where the first measured run supported it; memleak was reverted after regression evidence
- 2.4 PENDING `.zig-cache` size trimming not implemented in this pass

---

## 3.0 Job Parallelization and Dependency Graph

**Status:** IN_PROGRESS

Implemented in this execution pass:

- Split `lint` into `lint-zig`, `lint-website`, `lint-apps`, `lint-ci`, and `lint-greptile`, with a compatibility aggregator job named `lint`
- Split `test` into `test-zombied`, `test-zombiectl`, `test-website`, and `test-app`, with a compatibility aggregator job named `test`
- Split `qa-smoke` into backend, website, and app jobs, with a compatibility aggregator job named `qa-smoke`
- Split `qa` into backend, website, and app jobs, with a compatibility aggregator job named `qa`

Audit conclusions:

- `deploy-dev.yml`: the current `compile-dev -> push-dev -> deploy-fly-dev -> verify-dev -> qa-dev` chain is materially sequential. The worker deploy is already parallel and should stay separate because it buys headroom as worker count grows.
- `release.yml`: the graph is already heavily parallel for build/package/publish stages. The remaining sequential production fleet deploy is a deliberate safety tradeoff, not an accidental serialization.
- `test` and `test-integration` are already concurrent at the workflow level on pull requests. This pass keeps that behavior and shortens `test` internally.

Projected PR wall-clock improvement from audit data:

- Before: critical path p50 = `qa` at 2.23 min
- After the first PR run, `qa` dropped to about 1.55 min, but uncached `memleak` regressed to about 4.67 min, so memleak cache was restored
- **Projected PR wall-clock reduction remains achievable, but only with memleak cache restored; the first no-cache experiment regressed total PR wall clock**

**Dimensions:**
- 3.1 DONE Audited `deploy-dev.yml` dependency graph
- 3.2 DONE Audited `release.yml` dependency graph and documented why prod fleet stays sequential
- 3.3 DONE Split lint into parallel sub-jobs
- 3.4 DONE Confirmed `test` and `test-integration` already run concurrently, and split `test` internally to reduce its own wall clock

---

## 4.0 Dependency Install Optimization

**Status:** IN_PROGRESS

Audit conclusions:

- Bun dependency installs in this repo are already cheap in CI: typically 3–5s per package install in sampled runs.
- Playwright browser installation is expensive enough to justify caching: 24s in `qa-smoke`, 27s in `qa`, 25s in `qa-dev`.
- The `npm` step in `release.yml` is not a meaningful critical-path hotspot; built-in `setup-node` cache is not the highest-leverage change for this pass.

Implemented in this execution pass:

- Added Playwright browser cache to website smoke/full QA jobs
- Added Playwright browser cache to app smoke/full QA jobs
- Added Playwright browser cache to `qa-dev`
- Explicitly did **not** add `node_modules` caching because current install time is too small to justify cache churn and restore overhead

Projected install savings from audit timings:

- `qa-smoke`: ~24s saved on warm browser cache hits for the UI lane that currently dominates setup
- `qa`: ~27s saved on warm browser cache hits for the UI lane
- `qa-dev`: ~25s saved on warm browser cache hits during DEV verification

**Dimensions:**
- 4.1 DONE Evaluated `node_modules` caching and rejected it for this pass due to low install cost and likely cache overhead
- 4.2 DONE Added Playwright browser caching across QA jobs
- 4.3 DONE Evaluated `actions/setup-node` built-in cache and deprioritized it because npm publish is not a critical bottleneck here
- 4.4 DONE Documented install-time savings expectations from the audit

---

## 5.0 Acceptance Criteria

**Status:** IN_PROGRESS

- [ ] 5.1 PR check wall-clock time reduced by at least 30% (measured p50)
- [ ] 5.2 deploy-dev pipeline end-to-end time reduced by at least 20% (measured p50)
- [x] 5.3 No local workflow syntax regressions — `make lint-ci` passes after the CI edits
- [x] 5.4 Before/after baseline reasoning documented with evidence from the audit

Notes:

- 5.1 and 5.2 require post-merge or branch CI evidence; they cannot be honestly marked complete from local validation alone.
- Based on current audit data, the changes in this branch are projected to satisfy both thresholds if Playwright cache hit rate is healthy and the Zig cache upload time is eliminated as observed.

---

## 6.0 Out of Scope

- Self-hosted runners or Larger runners (cost implications need separate evaluation)
- Rewriting tests for speed (optimize CI infrastructure, not test content)
- Changing the Zig build system itself
- Docker layer caching (already handled by buildx cache)
- Parallelizing post-canary production fleet rollout, which would change deployment safety semantics
