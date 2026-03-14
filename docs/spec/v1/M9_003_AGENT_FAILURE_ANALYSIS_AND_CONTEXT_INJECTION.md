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
- 1.1 PENDING Extract failure signals from run terminal state: exit code per stage (`AgentResult.exit_ok`), timeout flag (run exceeded `RUN_TIMEOUT_MS`), resource limit hit (OOM / CPU throttle — available only after M4_008 Firecracker), unhandled exception class if surfaced in run metadata
- 1.2 PENDING Classify each failure into a stable taxonomy:
  - `TIMEOUT` — run exceeded deadline or individual stage timed out
  - `OOM` — resource limit exceeded (available after M4_008; inferred from exit signals until then)
  - `UNHANDLED_EXCEPTION` — stage threw an error not caught by the agent
  - `BAD_OUTPUT_FORMAT` — agent produced output that didn't match expected schema (e.g., no verdict in Warden output)
  - `TOOL_CALL_FAILURE` — tool invocation returned error (file not found, shell timeout)
  - `CONTEXT_OVERFLOW` — agent exceeded token context window
  - `AUTH_FAILURE` — BYOK key invalid, GitHub token expired, or similar auth error
  - `UNKNOWN` — failure signals present but no taxonomy match

  Classification is rule-based, not LLM-based. Each class has a priority; highest-priority matching class wins.

  **Infrastructure vs agent-attributable classification:**
  - Infrastructure failures: `TIMEOUT`, `OOM`, `CONTEXT_OVERFLOW`, `AUTH_FAILURE`
  - Agent-attributable failures: `BAD_OUTPUT_FORMAT`, `TOOL_CALL_FAILURE`, `UNHANDLED_EXCEPTION`
  - `UNKNOWN` treated as agent-attributable (conservative default)

  This classification is used by M9_004 trust evaluation: infrastructure failures do NOT reset the consecutive Gold+ streak; only agent-attributable failures reset it.

- 1.3 PENDING Produce `agent_run_analysis` record:
  ```sql
  CREATE TABLE agent_run_analysis (
      analysis_id      UUID PRIMARY KEY,
      run_id           UUID NOT NULL REFERENCES runs(run_id),
      agent_id         UUID NOT NULL REFERENCES agent_profiles(agent_id),
      workspace_id     UUID NOT NULL REFERENCES workspaces(workspace_id),
      failure_class    TEXT,  -- NULL for successful runs
      failure_is_infra BOOLEAN NOT NULL DEFAULT FALSE,
      failure_signals  TEXT NOT NULL DEFAULT '[]',  -- JSON array of signal strings
      improvement_hints TEXT NOT NULL DEFAULT '[]', -- JSON array of structured hints
      stderr_tail      TEXT,  -- last 200 lines, secrets scrubbed
      analyzed_at      BIGINT NOT NULL,
      UNIQUE (run_id),
      CONSTRAINT ck_agent_run_analysis_uuidv7 CHECK (substring(analysis_id::text from 15 for 1) = '7')
  );
  CREATE INDEX idx_agent_run_analysis_agent ON agent_run_analysis(agent_id, analyzed_at DESC);
  ```
  DB grants:
  ```sql
  GRANT SELECT, INSERT ON agent_run_analysis TO worker_accessor;
  GRANT SELECT ON agent_run_analysis TO api_accessor;
  ```

- 1.4 PENDING For successful runs, record `failure_class = NULL`, `failure_is_infra = FALSE`, and `improvement_hints` focused on efficiency: where latency or resource headroom can be recovered (e.g., "stage 'implement' used 80% of total tokens — investigate prompt compression")

- 1.5 PENDING **Stderr secret scrubbing:** Before persisting `stderr_tail`, apply a scrubbing pass that redacts patterns matching: `API_KEY=...`, `Bearer ...`, `DATABASE_URL=...`, `ENCRYPTION_MASTER_KEY=...`, `-----BEGIN.*PRIVATE KEY-----`, and any env var from CONFIGURATION.md's "Keys That Should Never Come From CLI Flags" list. Replace matched values with `[REDACTED]`.

---

## 2.0 Context Injection Into Next Run

**Status:** PENDING

Before a new run starts, inject the agent's recent score trajectory and failure analysis into the run context so the agent can self-correct.

**Dimensions:**
- 2.1 PENDING Build a `ScoringContext` block from the last 5 scored runs: per-run `score`, `tier`, `failure_class` (if any), top `improvement_hint` — capped at 512 tokens; truncate oldest run first if over limit. Token counting uses a simple byte-based estimate (4 chars ≈ 1 token) for determinism.
- 2.2 PENDING Inject `ScoringContext` as a structured system-message prefix before the agent's primary instruction. Format is stable and versioned:
  ```
  ## Agent Performance Context (v1)
  Your recent run history:
  | Run | Score | Tier | Issue |
  |-----|-------|------|-------|
  | 5 (latest) | 87 | Gold | — |
  | 4 | 62 | Silver | TIMEOUT |
  | 3 | 91 | Elite | — |
  | 2 | 78 | Gold | — |
  | 1 | 45 | Silver | BAD_OUTPUT_FORMAT |
  Trend: improving. Focus: avoid timeouts on large repos.
  ```
- 2.3 PENDING If agent has no prior runs (Unranked), inject a brief orientation block: "You have no prior score history. Aim for clean terminal states, minimal resource use, and valid output format."
- 2.4 PENDING Injection is opt-in per workspace via a workspace setting `enable_score_context_injection: bool` (default: true); can be disabled without affecting scoring or persistence. Requires `enable_agent_scoring` (M9_001) to be true.
- 2.5 PENDING If the DB query for last 5 scores fails, inject the orientation block as fallback (not empty context). Log the error. Run continues normally.

---

## 3.0 Acceptance Criteria

**Status:** PENDING

- [ ] 3.1 `agent_run_analysis` row exists for every run with a score (no orphaned scores without analysis)
- [ ] 3.2 `TIMEOUT` failure correctly classified when run metadata shows deadline exceeded
- [ ] 3.3 `failure_is_infra = true` for TIMEOUT, OOM, CONTEXT_OVERFLOW, AUTH_FAILURE
- [ ] 3.4 `failure_is_infra = false` for BAD_OUTPUT_FORMAT, TOOL_CALL_FAILURE, UNHANDLED_EXCEPTION, UNKNOWN
- [ ] 3.5 `ScoringContext` block injected into run context when workspace setting is enabled
- [ ] 3.6 `ScoringContext` block is absent when workspace setting is disabled
- [ ] 3.7 Context block never exceeds 512 tokens (enforced by truncation, not best-effort)
- [ ] 3.8 Stderr tail does not contain any patterns matching the secret scrubbing list
- [ ] 3.9 Agent with 3 prior TIMEOUT failures and injected context shows measurable reduction in timeout rate over next 10 runs (demo evidence required — not a unit test)

---

## 4.0 Out of Scope

- LLM-generated natural-language failure explanations (enhancement, deferred)
- Per-run user feedback override of failure classification
- Context injection into non-agent (human-driven) runs
- Injection format changes mid-workspace (versioning handles this; no live migration)
- Full stderr capture (only last 200 lines, scrubbed)
