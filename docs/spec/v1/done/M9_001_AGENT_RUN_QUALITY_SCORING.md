# M9_001: Agent Run Quality Scoring Engine

**Prototype:** v1.0.0
**Milestone:** M9
**Workstream:** 001
**Date:** Mar 13, 2026
**Status:** DONE
**Priority:** P0 — foundation layer; all other M9 workstreams depend on this score output
**Batch:** B1 — first to ship; unblocks M9_002, M9_003, M9_004
**Depends on:** M6_002 (credit lifecycle), M8_001 (UUID schema contracts)

---

## Entity Model

`agent_id` is the universal identifier for agents throughout M9. It maps to the existing `agent_profiles.profile_id` (UUID) from schema 006. A migration renames `profile_id` → `agent_id` across all existing tables and references before M9 tables are created.

User-facing terminology is "agent" everywhere: API paths (`/v1/agents/{agent_id}/...`), CLI (`zombiectl agent ...`), PostHog events, and DB table names.

---

## 1.0 Quality Dimensions

**Status:** DONE

Define the deterministic dimensions used to compute a per-run quality score.
Each dimension is independently measurable, idempotent, and reproducible from run metadata alone.

**Dimensions:**
- 1.1 DONE Define three active scoring axes plus one stubbed axis:
  - **Completion (40%)** — did the run reach terminal state cleanly? `DONE` = 100, `BLOCKED` with retries exhausted = 30, `BLOCKED` by stage graph = 10, any error propagation = 0
  - **Error rate (30%)** — ratio of stages with `exit_ok=true` to total stages executed. All stages pass = 100, 1 failure out of 3 = 67, etc.
  - **Latency (20%)** — total `wall_seconds` vs workspace rolling baseline (p50). At or below p50 = 100, linear degradation up to 3x p50 = 0. If no baseline exists (< 5 prior runs), axis scores 50 (neutral).
  - **Resource efficiency (10%)** — STUBBED at 50 until M4_008 (Firecracker sandbox) provides CPU/memory metrics. Score formula is versioned; historical scores preserved when this axis activates.
- 1.2 DONE Assign stable weights per axis: completion 40%, error rate 30%, latency 20%, resource efficiency 10% — weights are workspace-scoped config values stored on the entitlement row, not hardcoded in the scoring function; default weights ship with these values
- 1.3 DONE Produce a normalized integer score 0–100 per run: `score = clamp(0, 100, round(sum(axis_score * weight)))`. Score is deterministic given the same run metadata (no randomness, no LLM calls in scoring path). Division-by-zero guards: if baseline_count < 1, latency axis = 50; if no usage rows exist, completion axis = 0 with warning logged.
- 1.4 DONE Assign a tier label from score: Unranked (no prior runs; score still emitted but tier remains `UNRANKED`), Bronze (0–39), Silver (40–69), Gold (70–89), Elite (90–100)

---

## 2.0 Latency Baseline

**Status:** DONE

Maintain a rolling workspace-level latency baseline computed from `usage_ledger.agent_seconds` data.

**Dimensions:**
- 2.1 DONE Compute workspace rolling p50 and p95 from the last 50 completed runs (or all runs if < 50 exist). Store as `workspace_latency_baseline` row: `workspace_id`, `p50_seconds`, `p95_seconds`, `sample_count`, `computed_at`.
- 2.2 DONE Baseline is recomputed after every scored run. If sample_count < 5, all runs in that workspace score latency axis at 50 (neutral) until sufficient data accumulates.
- 2.3 DONE Baseline is workspace-scoped (not agent-scoped) because agents within a workspace share the same repo and infra. Agent-scoped baselines can be introduced when there's sufficient per-agent run volume.

---

## 3.0 Scoring Execution Model

**Status:** DONE

Score computation is synchronous, in-worker, and fail-safe.

**Dimensions:**
- 3.1 DONE Scoring runs in `zombied worker` after `executeRun()` reaches a terminal state (DONE, BLOCKED, NOTIFIED_BLOCKED). The scoring call is a single deferred function (`defer scoreRunIfTerminal(...)`) that fires on every exit path — not inline at each of the 5 exit points.
- 3.2 DONE Scoring reads data already in memory from the run context (`AgentResult` token counts, wall seconds, exit status) plus lightweight DB reads for workspace scoring config and latency baseline. Total overhead remains small relative to run finalization.
- 3.3 DONE If `scoreRun()` or any downstream persistence fails, the error is caught, logged with full context (`run_id`, `workspace_id`, `agent_id`, error class), and an `agent.scoring.failed` PostHog event is emitted. The run continues finalization normally. **Scoring must NEVER block or fail a run.** Score is `null` (absent) for that run; API handles null gracefully.
- 3.4 DONE Feature flag: `enable_agent_scoring: bool` per workspace (default: false at initial deploy, flipped to true after validation). Context injection (M9_003) requires scoring to be enabled.

---

## 4.0 Scoring Event Emission

**Status:** DONE

Emit the score as a structured, observable event immediately after run finalization.

**Dimensions:**
- 4.1 DONE Emit `agent.run.scored` event to PostHog with fields: `run_id`, `agent_id`, `workspace_id`, `score`, `tier`, `score_formula_version`, `axis_scores` (serialized JSON map of axis_name → int), `weight_snapshot` (serialized JSON map of axis_name → float), `scored_at`, plus flat axis fields for dashboard/debug query ergonomics
- 4.2 DONE Emit same payload to internal persistence path for `agent_run_scores` table write (M9_002). M9_001 ends at in-worker score computation plus `agent.run.scored` emission; DB row persistence begins in M9_002.
- 4.3 DONE Score computation happens in zombied worker after run reaches terminal state; no score on in-flight runs
- 4.4 DONE Score is immutable once emitted — no retroactive rescoring; if weights change, future runs use new weights, historical scores are preserved as-is with their `weight_snapshot`

---

## 5.0 Prometheus Metrics

**Status:** DONE

**Dimensions:**
- 5.1 DONE `agent_score_computed_total{tier}` — counter, incremented per scored run
- 5.2 DONE `zombie_agent_score_latest` — low-cardinality gauge for the latest computed score across runs; per-agent score history belongs in persistence/API, not Prometheus labels
- 5.3 DONE `agent_scoring_failed_total` — counter, incremented on fail-safe catch
- 5.4 DONE `zombie_agent_scoring_duration_ms` — histogram, time spent in `scoreRun()`

---

## 6.0 Acceptance Criteria

**Status:** DONE

- [x] 6.1 Score 0–100 produced for every run that reaches terminal state (success or failure) when `enable_agent_scoring` is true
- [x] 6.2 `agent.run.scored` event is emitted during run finalization with structured score payload (`axis_scores`, `weight_snapshot`, `score_formula_version`, `scored_at`)
- [x] 6.3 Tier label correct for all boundary values (0, 39, 40, 69, 70, 89, 90, 100)
- [x] 6.4 Score is identical when computed twice from the same run metadata (determinism test)
- [x] 6.5 Scoring failure does not block run finalization (fail-safe test: invalid scoring config triggers `agent.scoring.failed` path without crashing the worker flow)
- [x] 6.6 Latency axis scores 50 when workspace has < 5 prior runs
- [x] 6.7 Resource axis scores 50 (stubbed) for all runs until M4_008 activates

---

## 7.0 Out of Scope

- In-flight run scoring (score only at terminal state)
- User-configurable per-workspace weight overrides (deferred to M9 follow-on; weights are config but use defaults for v1)
- LLM-assisted qualitative scoring / Oracle-style review (output content analysis stays async and out-of-band)
- Future TODO recorded: add human-criteria qualitative evaluation via Oracle-style review as a separate async scoring layer, never in the synchronous worker scoring path
- Score appeals or corrections
- Agent-scoped latency baselines (workspace-scoped for v1)
- Resource axis activation (depends on M4_008 Firecracker)
