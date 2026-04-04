# M9_003: Agent Failure Analysis And Context Injection

**Prototype:** v1.0.0
**Milestone:** M9
**Workstream:** 003
**Date:** Mar 13, 2026
**Status:** DONE
**Priority:** P0 — the feedback loop that enables auto-learning; M9_004 depends on this
**Batch:** B2 — starts after M9_002 score persistence is stable
**Depends on:** M9_002 (score persistence and profile API)

---

## 1.0 Failure Analysis Generation

**Status:** DONE

After each scored run, produce a structured failure analysis document that names what went wrong and why.
Analysis must be deterministic, structured, and LLM-independent (LLM may enhance but is not required).

**Dimensions:**
- 1.1 DONE Extract failure signals from run terminal state and surfaced runtime metadata: aggregate stage pass/fail state, terminal outcome (`done`, `blocked_stage_graph`, `blocked_retries_exhausted`, `error_propagation`), timeout/error names surfaced by worker execution (`RunDeadlineExceeded`, `CommandTimedOut`, `OutOfMemory`, `TokenExpired`, etc.), and scrubbed stage failure payloads used for deterministic analysis
- 1.2 DONE Classify each failure into a stable taxonomy:
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

- 1.3 DONE Produce `agent_run_analysis` record:
  ```sql
  CREATE TABLE agent_run_analysis (
      analysis_id      UUID PRIMARY KEY,
      run_id           UUID NOT NULL REFERENCES runs(run_id),
      agent_id         UUID NOT NULL REFERENCES agent_profiles(agent_id),
      workspace_id     UUID NOT NULL REFERENCES workspaces(workspace_id),
      failure_class    TEXT,  -- NULL for successful runs
      failure_is_infra BOOLEAN NOT NULL DEFAULT FALSE,
      failure_signals  JSONB NOT NULL DEFAULT '[]'::jsonb,   -- JSON array of signal strings
      improvement_hints JSONB NOT NULL DEFAULT '[]'::jsonb,  -- JSON array of structured hints
      stderr_tail      TEXT,  -- last 200 lines, secrets scrubbed
      analyzed_at      BIGINT NOT NULL,
      UNIQUE (run_id),
      CONSTRAINT ck_agent_run_analysis_uuidv7 CHECK (substring(analysis_id::text from 15 for 1) = '7'),
      CONSTRAINT ck_failure_signals_array CHECK (jsonb_typeof(failure_signals) = 'array'),
      CONSTRAINT ck_improvement_hints_array CHECK (jsonb_typeof(improvement_hints) = 'array')
  );
  CREATE INDEX idx_agent_run_analysis_agent ON agent_run_analysis(agent_id, analyzed_at DESC);
  CREATE INDEX idx_agent_run_analysis_hints_gin ON agent_run_analysis USING GIN (improvement_hints);
  ```
  DB grants:
  ```sql
  GRANT SELECT, INSERT ON agent_run_analysis TO worker_accessor;
  GRANT SELECT ON agent_run_analysis TO api_accessor;
  ```

- 1.4 DONE For successful runs, record `failure_class = NULL`, `failure_is_infra = FALSE`, and `improvement_hints` focused on efficiency: where latency or resource headroom can be recovered (e.g., "stage 'implement' used 80% of total tokens — investigate prompt compression")

- 1.5 DONE **Stderr secret scrubbing + exporter safety:** Before persisting `stderr_tail`, apply a deterministic scrubbing pass that redacts patterns matching: `API_KEY=...`, `Bearer ...`, `DATABASE_URL=...`, `ENCRYPTION_MASTER_KEY=...`, `-----BEGIN.*PRIVATE KEY-----`, and any env var from CONFIGURATION.md's "Keys That Should Never Come From CLI Flags" list. Replace matched values with `[REDACTED]`. Only the scrubbed persisted value is available to downstream exporters; raw stderr is not stored in `agent_run_analysis`.

---

## 2.0 Context Injection Into Next Run

**Status:** DONE

Before a new run starts, inject the agent's recent score trajectory and failure analysis into the run context so the agent can self-correct.

**Dimensions:**
- 2.1 DONE Build a `ScoringContext` block from the last 5 scored runs: per-run `score`, `tier`, `failure_class` (if any), top `improvement_hint` — capped by `scoring_context_max_tokens` from zombied config (default: 2048, min: 512, max: 8192). Truncate oldest run first if over limit. Enforce cap with the current NullClaw runtime token-estimation path used by compaction (`(chars + 3) / 4`), not an unrelated local byte-count limit.
- 2.2 DONE Expose admin control for `scoring_context_max_tokens` via CLI (`zombiectl admin config set scoring_context_max_tokens <n>`) so operators can tighten limits for abuse or raise limits for larger repositories.
- 2.3 DONE Inject `ScoringContext` as a structured system-message prefix before the agent's primary instruction. Format is stable and versioned:
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
- 2.4 DONE If agent has no prior runs (Unranked), inject a brief orientation block: "You have no prior score history. Aim for clean terminal states, minimal resource use, and valid output format."
- 2.5 DONE Injection is opt-in per workspace via a workspace setting `enable_score_context_injection: bool` (default: true); can be disabled without affecting scoring or persistence. Requires `enable_agent_scoring` (M9_001) to be true.
- 2.6 DONE If the DB query for last 5 scores fails, inject the orientation block as fallback (not empty context). Log the error. Run continues normally.

---

## 3.0 Acceptance Criteria

**Status:** DONE

- [x] 3.1 `agent_run_analysis` row exists for every run with a score (no orphaned scores without analysis)
- [x] 3.2 `TIMEOUT` failure correctly classified when run metadata shows deadline exceeded
- [x] 3.3 `failure_is_infra = true` for TIMEOUT, OOM, CONTEXT_OVERFLOW, AUTH_FAILURE
- [x] 3.4 `failure_is_infra = false` for BAD_OUTPUT_FORMAT, TOOL_CALL_FAILURE, UNHANDLED_EXCEPTION, UNKNOWN
- [x] 3.5 `ScoringContext` block injected into run context when workspace setting is enabled
- [x] 3.6 `ScoringContext` block is absent when workspace setting is disabled
- [x] 3.7 Context block never exceeds configured `scoring_context_max_tokens` (hard-enforced by runtime estimator + truncation)
- [x] 3.8 Stderr tail does not contain any patterns matching the secret scrubbing list in storage or exported logs
- [x] 3.9 Agent with 3 prior TIMEOUT failures and injected context shows measurable reduction in timeout rate over next 10 runs (demo evidence captured in `docs/evidence/M9_003_TIMEOUT_REDUCTION_DEMO.md`)

---

## 4.0 Out of Scope

- LLM-generated natural-language failure explanations (enhancement, deferred)
- Per-run user feedback override of failure classification
- Context injection into non-agent (human-driven) runs
- Injection format changes mid-workspace (versioning handles this; no live migration)
- Full stderr capture (only last 200 lines, scrubbed)

---

## 5.0 Implementation Notes (Mar 17, 2026)

- Added migration `018_agent_failure_analysis_and_context_injection.sql` with `agent_run_analysis` plus workspace settings: `enable_score_context_injection` and `scoring_context_max_tokens`.
- Worker scoring finalization now writes `agent_run_analysis` for each scored run and persists deterministic failure classification + improvement hints.
- Worker stage execution now records surfaced stage error names into scoring state so persistence can classify `TIMEOUT`, `OOM`, `CONTEXT_OVERFLOW`, `AUTH_FAILURE`, and `TOOL_CALL_FAILURE` without LLM involvement.
- `stderr_tail` is now persisted through a deterministic scrubber that redacts bearer tokens, private keys, and env-style secrets from the `CONFIGURATION.md` never-flag list before storage.
- Echo prompt injection prepends `ScoringContext` before workspace memories; when history is unavailable or query fails, orientation context is injected.
- `scoring_context_max_tokens` can now be changed per workspace through `POST /v1/workspaces/{workspace_id}/scoring/config` and `zombiectl admin config set scoring_context_max_tokens <n> --workspace-id <id>`.
- Context-cap enforcement now matches the current NullClaw runtime compaction heuristic (`(chars + 3) / 4`) instead of an ad hoc local estimate.
- DB-backed scoring tests now cover runtime-deadline timeout classification, infra-vs-agent failure flags, scrubbed `stderr_tail` persistence, context-cap truncation, and the `zombiectl` admin setter path.
- Demo evidence for timeout-rate reduction is captured in `docs/evidence/M9_003_TIMEOUT_REDUCTION_DEMO.md` from a DB-backed integration scenario using real `ScoringContext` generation plus a deterministic custom-skill harness.

---

## 6.0 Evidence And Closure

**Status:** DONE

Evidence tracker:
- `docs/evidence/M9_003_TIMEOUT_REDUCTION_DEMO.md`

Verification anchors:
- `src/pipeline/scoring_test.zig`
- `src/pipeline/scoring_mod/persistence.zig`
- `src/pipeline/worker_stage_executor.zig`
- `src/http/handlers/workspaces.zig`
- `zombiectl/src/commands/admin.js`
