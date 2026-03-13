# M9_001: Agent Run Quality Scoring Engine

**Prototype:** v1.0.0
**Milestone:** M9
**Workstream:** 001
**Date:** Mar 13, 2026
**Status:** PENDING
**Priority:** P0 — foundation layer; all other M9 workstreams depend on this score output
**Batch:** B1 — first to ship; unblocks M9_002, M9_003, M9_004
**Depends on:** M6_002 (credit lifecycle), M8_001 (UUID schema contracts)

---

## 1.0 Quality Dimensions

**Status:** PENDING

Define the deterministic dimensions used to compute a per-run quality score.
Each dimension is independently measurable, idempotent, and reproducible from run metadata alone.

**Dimensions:**
- 1.1 PENDING Define the four scoring axes: completion (did the run reach terminal state cleanly?), error rate (unhandled exceptions or non-zero exits), latency percentile (p50/p95 vs workspace baseline), resource efficiency (CPU/memory vs declared sandbox limits)
- 1.2 PENDING Assign stable weights per axis: completion 40%, error rate 30%, latency 20%, resource efficiency 10% — weights are config, not hardcoded
- 1.3 PENDING Produce a normalized integer score 0–100 per run; score is deterministic given the same run metadata (no randomness, no LLM calls in scoring path)
- 1.4 PENDING Assign a tier label from score: Unranked (no prior runs), Bronze (0–39), Silver (40–69), Gold (70–89), Elite (90–100)

---

## 2.0 Scoring Event Emission

**Status:** PENDING

Emit the score as a structured, observable event immediately after run finalization.

**Dimensions:**
- 2.1 PENDING Emit `agent.run.scored` event to PostHog with fields: `run_id`, `agent_id`, `workspace_id`, `score`, `tier`, `axis_scores` (map), `scored_at`
- 2.2 PENDING Emit same payload to internal outbox table for persistence (reuse M5_004 adapter outbox pattern)
- 2.3 PENDING Score computation happens in zombied after run reaches terminal state; no score on in-flight runs
- 2.4 PENDING Score is immutable once emitted — no retroactive rescoring; if weights change, future runs use new weights, historical scores are preserved as-is

---

## 3.0 Acceptance Criteria

**Status:** PENDING

- [ ] 3.1 Score 0–100 produced for every run that reaches terminal state (success or failure)
- [ ] 3.2 `agent.run.scored` event visible in PostHog within 5 seconds of run finalization
- [ ] 3.3 Tier label correct for all boundary values (0, 39, 40, 69, 70, 89, 90, 100)
- [ ] 3.4 Score is identical when computed twice from the same run metadata (determinism test)

---

## 4.0 Out of Scope

- In-flight run scoring (score only at terminal state)
- User-configurable per-workspace weight overrides (deferred to M9 follow-on)
- LLM-assisted qualitative scoring (output content analysis)
- Score appeals or corrections
