# M9_003: Agent Failure Analysis And Context Injection

**Prototype:** v1.0.0
**Milestone:** M9
**Workstream:** 003
**Date:** Mar 13, 2026
**Status:** PENDING
**Priority:** P0 — the feedback loop that enables auto-learning; M9_004 depends on this
**Batch:** B2 — starts after M9_002 score persistence is stable
**Depends on:** M9_002 (score persistence and profile API)

---

## 1.0 Failure Analysis Generation

**Status:** PENDING

After each scored run, produce a structured failure analysis document that names what went wrong and why.
Analysis must be deterministic, structured, and LLM-independent (LLM may enhance but is not required).

**Dimensions:**
- 1.1 PENDING Extract failure signals from run terminal state: exit code, stderr tail (last 200 lines), timeout flag, resource limit hit (OOM / CPU throttle), unhandled exception class if surfaced in run metadata
- 1.2 PENDING Classify each failure into a stable taxonomy: `TIMEOUT`, `OOM`, `UNHANDLED_EXCEPTION`, `BAD_OUTPUT_FORMAT`, `TOOL_CALL_FAILURE`, `CONTEXT_OVERFLOW`, `AUTH_FAILURE`, `UNKNOWN` — classification is rule-based, not LLM-based
- 1.3 PENDING Produce `agent_run_analysis` record: `run_id`, `failure_class` (enum or null for success), `failure_signals` (jsonb array), `improvement_hints` (jsonb — structured pointers, not prose), `analyzed_at`
- 1.4 PENDING For successful runs, record `failure_class = null` and `improvement_hints` focused on efficiency: where latency or resource headroom can be recovered

---

## 2.0 Context Injection Into Next Run

**Status:** PENDING

Before a new run starts, inject the agent's recent score trajectory and failure analysis into the run context so the agent can self-correct.

**Dimensions:**
- 2.1 PENDING Build a `ScoringContext` block from the last 5 scored runs: per-run score, tier, failure_class, top improvement_hint — capped at 512 tokens; truncate oldest first if over limit
- 2.2 PENDING Inject `ScoringContext` as a structured system-message prefix before the agent's primary instruction; format is stable and versioned (schema version in block header)
- 2.3 PENDING If agent has no prior runs (Unranked), inject a brief orientation block: "You have no prior score history. Aim for clean terminal states, minimal resource use, and valid output format."
- 2.4 PENDING Injection is opt-in per workspace via a workspace setting `enable_score_context_injection: bool` (default: true); can be disabled without affecting scoring or persistence

---

## 3.0 Acceptance Criteria

**Status:** PENDING

- [ ] 3.1 `agent_run_analysis` row exists for every run with a score (no orphaned scores without analysis)
- [ ] 3.2 `TIMEOUT` failure correctly classified when run metadata shows `timed_out: true`
- [ ] 3.3 `ScoringContext` block injected into run context when workspace setting is enabled
- [ ] 3.4 `ScoringContext` block is absent when workspace setting is disabled
- [ ] 3.5 Context block never exceeds 512 tokens (enforced by truncation, not best-effort)
- [ ] 3.6 Agent with 3 prior TIMEOUT failures and injected context shows measurable reduction in timeout rate over next 10 runs (demo evidence required — not a unit test)

---

## 4.0 Out of Scope

- LLM-generated natural-language failure explanations (enhancement, deferred)
- Per-run user feedback override of failure classification
- Context injection into non-agent (human-driven) runs
- Injection format changes mid-workspace (versioning handles this; no live migration)
