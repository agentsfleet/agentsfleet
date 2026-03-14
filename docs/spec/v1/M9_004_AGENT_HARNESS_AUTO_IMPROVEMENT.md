# M9_004: Agent Harness Auto-Improvement Loop

**Prototype:** v1.0.0
**Milestone:** M9
**Workstream:** 004
**Date:** Mar 13, 2026
**Status:** PENDING
**Priority:** P1 — the payoff of the gamification system; ships after B2 is stable
**Batch:** B3 — starts after M9_003 failure analysis and injection are proven
**Depends on:** M9_003 (failure analysis + context injection), M9_002 (profile + trajectory API)

---

## 1.0 Improvement Proposal Generation

**Status:** PENDING

After sufficient score history accumulates (minimum 5 runs), the system can generate a structured
improvement proposal targeting the agent's harness configuration.

**Dimensions:**
- 1.1 PENDING Trigger proposal generation when: agent has >= 5 scored runs AND current 5-run rolling avg score < previous 5-run rolling avg score (trajectory is declining) OR avg score < 60 for any 5-run window. Trigger check is synchronous (fast comparison after score persist). If triggered, **enqueue** proposal generation as async work — do not generate inline.
- 1.2 PENDING Proposal is a structured document:
  ```sql
  CREATE TABLE agent_improvement_proposals (
      proposal_id          UUID PRIMARY KEY,
      agent_id             UUID NOT NULL REFERENCES agent_profiles(agent_id),
      workspace_id         UUID NOT NULL REFERENCES workspaces(workspace_id),
      trigger_reason       TEXT NOT NULL CHECK (trigger_reason IN ('DECLINING_SCORE', 'SUSTAINED_LOW_SCORE')),
      proposed_changes     TEXT NOT NULL,  -- JSON array of change objects
      config_version_id    UUID NOT NULL,  -- version at time of proposal (CAS guard)
      approval_mode        TEXT NOT NULL CHECK (approval_mode IN ('AUTO', 'MANUAL')),
      status               TEXT NOT NULL DEFAULT 'PENDING_REVIEW'
                           CHECK (status IN ('PENDING_REVIEW', 'VETO_WINDOW', 'APPROVED', 'REJECTED', 'APPLIED', 'VETOED', 'CONFIG_CHANGED')),
      rejection_reason     TEXT,
      auto_apply_at        BIGINT,  -- NULL if MANUAL
      applied_by           TEXT,    -- 'operator:<identity>' or 'system:auto'
      created_at           BIGINT NOT NULL,
      updated_at           BIGINT NOT NULL,
      CONSTRAINT ck_proposals_uuidv7 CHECK (substring(proposal_id::text from 15 for 1) = '7')
  );
  CREATE INDEX idx_proposals_agent ON agent_improvement_proposals(agent_id, created_at DESC);
  CREATE INDEX idx_proposals_veto_window ON agent_improvement_proposals(status, auto_apply_at)
      WHERE status = 'VETO_WINDOW';
  ```
  DB grants:
  ```sql
  GRANT SELECT, INSERT, UPDATE ON agent_improvement_proposals TO worker_accessor;
  GRANT SELECT, UPDATE ON agent_improvement_proposals TO api_accessor;
  ```
- 1.3 PENDING `proposed_changes` targets numeric harness-level fields only:
  - `max_tokens` — bounded between 1000 and workspace entitlement max
  - `timeout_seconds` — bounded between 30 and `RUN_TIMEOUT_MS / 1000`
  - `tool_allowlist` — can only **restrict** (remove tools), never expand beyond current profile's allowed tools

  **Explicitly excluded** from proposable fields (rejected at schema validation):
  - `system_prompt_appendix` — direct prompt injection vector; removed to eliminate LLM-generated text in future system prompts
  - Any auth, billing, or network config field
  - Model selection or provider configuration

  Each change object: `{"target_field": "max_tokens", "current_value": 8000, "proposed_value": 4000, "rationale": "last 5 runs averaged 2100 tokens; reducing cap saves cost"}`

  Range validation: if `proposed_value` is outside the bounded range for `target_field`, the entire proposal is rejected with reason `VALUE_OUT_OF_RANGE`.

- 1.4 PENDING Proposal generation uses an LLM call (async, enqueued as a work item). The prompt includes: last 10 run analyses + current harness config + the bounded field constraints. Output is validated against the `proposed_changes` schema before persisting. Malformed output → reject without retry, log `agent.proposal.generation_failed`. Empty output or refusal → treat as malformed.

---

## 2.0 Agent Trust Level

**Status:** PENDING

An agent earns autonomous approval rights by demonstrating sustained high-quality execution.
Trust is computed, not granted — it cannot be manually assigned.

**Dimensions:**
- 2.1 PENDING Define `TRUSTED` threshold: agent has >= 10 consecutive scored runs all in Gold or Elite tier (score >= 70 each); tracked as `consecutive_gold_plus_runs` on `agent_profiles`
- 2.2 PENDING Trust evaluation uses M9_003 failure classification to distinguish infrastructure failures from agent-attributable failures:
  - **Infrastructure failures** (`failure_is_infra = true`: TIMEOUT, OOM, CONTEXT_OVERFLOW, AUTH_FAILURE) do NOT reset `consecutive_gold_plus_runs`. The run is excluded from the streak count (neither increments nor resets).
  - **Agent-attributable failures** (`failure_is_infra = false`: BAD_OUTPUT_FORMAT, TOOL_CALL_FAILURE, UNHANDLED_EXCEPTION, UNKNOWN) with score < 70 reset `consecutive_gold_plus_runs` to 0.
  - **Successful runs scoring Gold+ (>= 70)** increment `consecutive_gold_plus_runs` by 1.
  This ensures trust measures agent quality, not infrastructure reliability.
- 2.3 PENDING `agent_profiles` exposes `trust_level` (enum: `UNEARNED` | `TRUSTED`) and `consecutive_gold_plus_runs` (int); surfaced in `zombiectl agent profile <agent-id>` output
- 2.4 PENDING PostHog event `agent.trust.earned` emitted when agent crosses from UNEARNED → TRUSTED; `agent.trust.lost` emitted on reset — both include `agent_id`, `run_id`, `consecutive_count_at_event`

---

## 3.0 Confidence-Based Auto-Approval

**Status:** PENDING

TRUSTED agents bypass the manual approval gate. Proposals enter a 24-hour veto window
instead, during which the operator can inspect and cancel before the change applies.

**Dimensions:**
- 3.1 PENDING When a proposal is generated for a TRUSTED agent, set `approval_mode = AUTO` and `auto_apply_at = created_at + 24h`; status transitions to `VETO_WINDOW` immediately
- 3.2 PENDING `zombiectl agent proposals <agent-id>` lists VETO_WINDOW proposals prominently with a countdown: `"Auto-applies in 18h 42m — zombiectl agent proposals veto <proposal-id> to cancel"`
- 3.3 PENDING `zombiectl agent proposals veto <proposal-id> [--reason "..."]` — operator cancels; status transitions to `VETOED`; agent is not penalized but reason is stored; next proposal on next trigger
- 3.4 PENDING A background checker (worker goroutine, 60-second tick) queries `WHERE status = 'VETO_WINDOW' AND auto_apply_at <= now()` and transitions matching proposals through the apply path. `applied_by` recorded as `system:auto`.
- 3.5 PENDING **CAS guard before apply:** Before applying any proposal (auto or manual), compare the harness config's current `config_version_id` to the proposal's `config_version_id` field. If they differ (operator changed the harness since the proposal was generated), reject the proposal with status `CONFIG_CHANGED` and reason `CONFIG_CHANGED_SINCE_PROPOSAL`. Notify operator via PostHog event. This prevents silent overwrite of manual harness changes.
- 3.6 PENDING **Reconciler for stuck proposals:** If a proposal is in `VETO_WINDOW` and `auto_apply_at` is more than 1 hour past (indicating the background job missed it), the reconciler picks it up on next tick. This handles worker crash recovery.

---

## 4.0 Manual Approval (UNEARNED Agents)

**Status:** PENDING

Agents that have not earned TRUSTED status require explicit operator action on every proposal.

**Dimensions:**
- 4.1 PENDING `zombiectl agent proposals <agent-id>` — list PENDING_REVIEW proposals with proposed changes, rationale, and bounded ranges
- 4.2 PENDING `zombiectl agent proposals approve <proposal-id>` — operator approves; CAS version check, then status transitions to APPROVED → APPLIED in the same transaction
- 4.3 PENDING `zombiectl agent proposals reject <proposal-id> [--reason "..."]` — operator rejects; status transitions to REJECTED; reason stored
- 4.4 PENDING Proposals older than 7 days without a decision auto-expire to REJECTED with reason `EXPIRED`; agent generates a new proposal on next trigger. Expiry handled by the same background checker as auto-apply.

---

## 5.0 Harness Change Application And Tracking

**Status:** PENDING

Approved proposals (auto or manual) are applied atomically; every change is versioned and reversible.

**Dimensions:**
- 5.1 PENDING On APPROVED or auto-apply: apply `proposed_changes` through the existing harness control plane path (compile → activate). If compile fails, reject proposal with status `REJECTED` and reason `COMPILE_FAILED`. If activate fails, reject with `ACTIVATE_FAILED`. Create a `harness_change_log` record per field changed:
  ```sql
  CREATE TABLE harness_change_log (
      change_id       UUID PRIMARY KEY,
      agent_id        UUID NOT NULL REFERENCES agent_profiles(agent_id),
      proposal_id     UUID NOT NULL REFERENCES agent_improvement_proposals(proposal_id),
      workspace_id    UUID NOT NULL REFERENCES workspaces(workspace_id),
      field_name      TEXT NOT NULL,
      old_value       TEXT NOT NULL,
      new_value       TEXT NOT NULL,
      applied_at      BIGINT NOT NULL,
      applied_by      TEXT NOT NULL,  -- 'operator:<identity>' or 'system:auto'
      reverted_from   UUID,  -- references change_id if this is a revert
      CONSTRAINT ck_harness_change_log_uuidv7 CHECK (substring(change_id::text from 15 for 1) = '7')
  );
  CREATE INDEX idx_harness_change_log_agent ON harness_change_log(agent_id, applied_at DESC);
  ```
  DB grants:
  ```sql
  GRANT SELECT, INSERT ON harness_change_log TO worker_accessor;
  GRANT SELECT, INSERT ON harness_change_log TO api_accessor;
  ```
- 5.2 PENDING Applied change is immediately reflected in the next run's profile resolution (no restart required); harness control plane already supports this via workspace_active_profile. No mid-run config mutation.
- 5.3 PENDING Revert path: `zombiectl agent harness revert <agent-id> --to-change <change-id>` restores the harness to pre-change state via compile → activate with old_value; creates a new `harness_change_log` entry with `reverted_from` reference; revert does not affect trust level
- 5.4 PENDING PostHog event `agent.harness.changed` emitted on apply with fields: `agent_id`, `proposal_id`, `approval_mode`, `fields_changed` (array), `trigger_reason`

---

## 6.0 Improvement Trajectory Measurement

**Status:** PENDING

Measure whether applied proposals actually improve the agent's score.

**Dimensions:**
- 6.1 PENDING After each applied change, tag the next 5 runs as `post_change_window: true` in `agent_run_scores` (add nullable `change_id` column referencing the proposal that triggered the change window)
- 6.2 PENDING Compute `score_delta`: avg score of post-change window minus avg score of 5 runs before the change; store on `harness_change_log` as `score_delta` (nullable, populated after window completes)
- 6.3 PENDING `zombiectl agent improvement-report <agent-id>` — prints: trust level, proposals generated/approved/vetoed/rejected/applied, avg score delta per applied change, current vs baseline tier
- 6.4 PENDING If 3 consecutive applied proposals each produce negative `score_delta`, emit `agent.improvement.stalled` event, surface warning in CLI profile output, and reset trust level to UNEARNED regardless of consecutive_gold_plus_runs count

---

## 7.0 Acceptance Criteria

**Status:** PENDING

- [ ] 7.1 Proposal generated (async, within 60 seconds) after the scoring trigger condition is met
- [ ] 7.2 Proposal with malformed `proposed_changes` is rejected at ingest, not silently stored
- [ ] 7.3 Proposal targeting `system_prompt_appendix` is rejected at schema validation
- [ ] 7.4 Proposal with `max_tokens` exceeding entitlement limit is rejected with VALUE_OUT_OF_RANGE
- [ ] 7.5 Proposal with `tool_allowlist` that expands beyond current profile is rejected
- [ ] 7.6 Agent with 10 consecutive Gold+ runs (excluding infra failures) shows `trust_level: TRUSTED` in profile output
- [ ] 7.7 TRUSTED agent proposal enters VETO_WINDOW with correct `auto_apply_at` timestamp
- [ ] 7.8 Operator veto within 24h prevents application; status shows VETOED, harness unchanged
- [ ] 7.9 TRUSTED agent drops an agent-attributable Silver run → `consecutive_gold_plus_runs` resets to 0 → next proposal requires manual approval
- [ ] 7.10 TRUSTED agent has a TIMEOUT (infra) run → `consecutive_gold_plus_runs` unchanged → trust preserved
- [ ] 7.11 CAS version check: proposal generated against config_version_id X, operator changes config, auto-apply attempt rejects with CONFIG_CHANGED_SINCE_PROPOSAL
- [ ] 7.12 Revert restores previous value exactly; `applied_by` on revert row shows operator identity
- [ ] 7.13 No harness change applied without a proposal record in APPROVED/VETO_WINDOW state (enforced at application logic level)
- [ ] 7.14 Demo evidence: agent earns TRUSTED, generates auto-approved proposal, harness updates, score improves over next 5 runs

---

## 8.0 Out of Scope

- Manual override to grant TRUSTED status without earning it through run history
- Changes to auth config, billing tier, or network policy via proposals
- `system_prompt_appendix` as a proposable field (prompt injection risk)
- Multi-agent cooperative improvement (one agent learning from another's harness)
- LLM provider or model selection as a proposable change (deferred)
- Veto window length as a user-configurable setting (fixed at 24h in v1)
- Synchronous proposal generation (always async/enqueued)
