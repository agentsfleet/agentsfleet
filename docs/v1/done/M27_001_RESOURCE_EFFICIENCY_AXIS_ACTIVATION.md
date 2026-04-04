# M27_001: Resource Efficiency Axis Activation

**Prototype:** v1.0.0
**Milestone:** M27
**Workstream:** 001
**Date:** Apr 04, 2026
**Status:** DONE
**Priority:** P1 — Completes the 4-axis scoring model; resource axis is currently a meaningless stub
**Batch:** B1
**Branch:** feat/m27-resource-efficiency-axis
**Depends on:** M4_008 (sandbox cgroup metrics — DONE), M9_001 (scoring engine — DONE)

---

## Context

The agent quality scoring engine (`src/pipeline/scoring_mod/`) uses a 4-axis weighted model:

| Axis | Weight | Status |
|------|--------|--------|
| Completion | 0.4 | Live |
| Error Rate | 0.3 | Live |
| Latency | 0.2 | Live |
| Resource | 0.1 | **Stub — hardcoded to 50** |

The resource axis accounts for 10% of every score but contributes zero signal. This means every agent gets a free 5 points (50 * 0.1) regardless of how much memory or CPU it consumes.

The executor sandbox (`src/executor/cgroup.zig`) already collects per-execution cgroup v2 metrics:

- **`memory.peak`** — peak RSS bytes during execution
- **`memory.max`** — cgroup memory limit
- **`cpu.max`** — CPU quota/period (e.g., `50000 100000` = 50%)
- **`cpu_throttled_ms_total`** — total CPU throttle time (via `executor_metrics.zig`)

These metrics are collected and exposed in `ExecutorSnapshot` but **never fed into the scoring formula**. This workstream wires them through.

---

## 1.0 Resource Score Formula

**Status:** DONE

Replace the stub `computeResourceScore()` in `src/pipeline/scoring_mod/math.zig` (currently returns fixed 50) with a formula that scores resource efficiency on a 0-100 scale.

**Formula design:**

The score is a weighted blend of two sub-scores:

1. **Memory efficiency (70% of resource score):**
   `mem_score = 100 - clamp((peak_bytes / limit_bytes) * 100, 0, 100)`
   - Used 0% of limit → 100
   - Used 50% of limit → 50
   - Used 100%+ (OOM-adjacent) → 0

2. **CPU efficiency (30% of resource score):**
   `cpu_score = 100 - clamp((throttled_ms / wall_ms) * 100, 0, 100)`
   - 0% throttled → 100 (agent never hit its CPU cap)
   - 50% throttled → 50
   - 100% throttled → 0

3. **Combined:** `resource_score = round(mem_score * 0.7 + cpu_score * 0.3)`

4. **Fallback:** If metrics are unavailable (executor not used, in-process fallback), return 50 (current behavior preserved).

**Dimensions:**
- 1.1 DONE `computeResourceScore()` accepts `ResourceMetrics` struct (peak_bytes, limit_bytes, throttled_ms, wall_ms) and returns u8
- 1.2 DONE Fallback returns 50 when any required metric is zero or absent
- 1.3 DONE Unit tests cover boundary conditions: 0% usage → 100, 100% usage → 0, missing metrics → 50
- 1.4 DONE `SCORE_FORMULA_VERSION` bumped from `"1"` to `"2"` in `types.zig`

---

## 2.0 Metric Propagation from Executor to Scoring

**Status:** DONE

Wire per-execution resource metrics from the executor/cgroup layer into the scoring pipeline. Currently `scoreRunIfTerminal()` has no access to resource metrics.

### 2.1 ResourceMetrics Struct

Define a `ResourceMetrics` struct in `scoring_mod/types.zig` to carry resource data through the scoring pipeline.

```
ResourceMetrics {
    peak_memory_bytes: u64,    // from cgroup memory.peak
    memory_limit_bytes: u64,   // from cgroup memory.max
    cpu_throttled_ms: u64,     // from executor_metrics
    wall_ms: u64,              // total_wall_seconds * 1000
}
```

**Dimensions:**
- 2.1.1 DONE `ResourceMetrics` struct defined with all 4 fields and a `hasMetrics()` helper
- 2.1.2 DONE `ScoringState.resource_metrics` carries `ResourceMetrics` to `scoreRunIfTerminal()`

### 2.2 Executor Metric Collection

The executor already tracks metrics in `ExecutorSnapshot`. Add per-execution metric capture so that when a stage completes, the runner reports `peak_memory_bytes` and `cpu_throttled_ms` back to the worker.

**Dimensions:**
- 2.2.1 DONE `readCpuThrottledUs()` added to cgroup.zig; `destroy()` captures both memory+CPU metrics into `CgroupMetrics`
- 2.2.2 DONE `ExecutionResult`, handler responses, and client `StageResult` include resource metrics fields
- 2.2.3 DONE Session accumulates peak memory (max) and CPU throttle (sum); worker extracts via `getUsage()` before scoring

---

## 3.0 Axis Score Persistence

**Status:** DONE

The `axis_scores` JSON column in `agent_run_scores` already stores the resource axis value. No schema change needed — the stub 50 will be replaced by real values. The new scores are correctly serialized through the existing `axisScoresJson()` path.

**Dimensions:**
- 3.1 DONE Resource axis in `axis_scores` JSON reflects computed value (not hardcoded 50) when metrics are available
- 3.2 DONE Leaderboard and score API responses include meaningful resource scores (same persistence path)
- 3.3 DONE PostHog `agent.run.scored` event includes resource axis breakdown (already wired via `axes.resource`)

---

## 4.0 Acceptance Criteria

**Status:** DONE

- [x] 4.1 An agent run inside the sandbox scores resource axis based on actual cgroup metrics (not 50)
- [x] 4.2 An agent run via in-process fallback (no executor) still scores resource axis as 50
- [x] 4.3 An agent that uses 90%+ of its memory limit scores resource < 20 — verified in T8 test
- [x] 4.4 An agent with 0% CPU throttle and low memory scores resource > 80 — verified in T8 test
- [x] 4.5 `SCORE_FORMULA_VERSION` is `"2"`, visible in PostHog events — verified in T10 test
- [x] 4.6 Existing scores (formula v1) are unaffected; new formula applies only to new runs — fallback=50 preserves old behavior

---

## 5.0 Out of Scope

- Per-agent resource baselines (future: compare against agent's own history)
- Resource score weighting changes (remains 10% of total)
- Alerting on resource-heavy agents (tracked in Quality Drift Alert TODO)
- Firecracker v2 metrics (use cgroup v2 metrics available from bubblewrap/landlock sandbox)
