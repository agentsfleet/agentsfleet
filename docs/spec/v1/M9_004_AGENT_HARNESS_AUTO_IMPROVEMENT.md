# M9_004: Agent Harness Auto-Improvement Loop

**Prototype:** v1.0.0
**Milestone:** M9
**Workstream:** 004
**Date:** Mar 13, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — the payoff of the gamification system; ships after B2 is stable
**Batch:** B3 — starts after M9_003 failure analysis and injection are proven
**Depends on:** M9_003 (failure analysis + context injection), M9_002 (profile + trajectory API)

**Harness boundary:** M9_004 operates on dynamic agent harness profiles. Proposals may tune stage-to-agent bindings and stage-local limits, but every referenced agent/skill must still pass the existing control-plane compile validation, registry checks, and workspace entitlement limits before it can be stored or applied.

**Carry-forward from M9_003:** M9_003 is now closed. M9_004 no longer carries failure-analysis completion work and focuses only on the proposal/trust/approval lifecycle built on top of the shipped M9_003 signals.

---

## 1.0 Improvement Proposal Generation

**Status:** DONE

After sufficient score history accumulates (minimum 5 runs), the system can generate a structured
improvement proposal targeting the agent's harness configuration.

**Dimensions:**
- 1.1 DONE Trigger proposal generation when: agent has >= 5 scored runs AND current 5-run rolling avg score < previous 5-run rolling avg score (trajectory is declining) OR avg score < 60 for any 5-run window. Trigger check runs synchronously after score persist, but only after a new score row is inserted so duplicate score writes do not retrigger the lifecycle. If triggered, persist a placeholder proposal row and defer materialization to async reconcile work.
- 1.2 DONE Proposal is a structured document:
  ```sql
  CREATE TABLE agent_improvement_proposals (
      proposal_id          UUID PRIMARY KEY,
      agent_id             UUID NOT NULL REFERENCES agent_profiles(agent_id),
      workspace_id         UUID NOT NULL REFERENCES workspaces(workspace_id),
      trigger_reason       TEXT NOT NULL,  -- lifecycle vocabulary enforced in application code
      proposed_changes     TEXT NOT NULL,  -- JSON array of change objects
      config_version_id    UUID NOT NULL,  -- version at time of proposal (CAS guard)
      approval_mode        TEXT NOT NULL,  -- AUTO | MANUAL enforced in application code
      generation_status    TEXT NOT NULL,  -- PENDING | READY | REJECTED enforced in application code
      status               TEXT NOT NULL,  -- review/apply lifecycle enforced in application code
      rejection_reason     TEXT,
      auto_apply_at        BIGINT,  -- NULL if MANUAL
      applied_by           TEXT,    -- 'operator:<identity>' or 'system:auto'
      created_at           BIGINT NOT NULL,
      updated_at           BIGINT NOT NULL,
      CONSTRAINT ck_proposals_uuidv7 CHECK (substring(proposal_id::text from 15 for 1) = '7')
  );
  CREATE INDEX idx_proposals_agent ON agent_improvement_proposals(agent_id, created_at DESC);
  CREATE INDEX idx_proposals_veto_window ON agent_improvement_proposals(status, auto_apply_at);
  ```
  DB grants:
  ```sql
  GRANT SELECT, INSERT, UPDATE ON agent_improvement_proposals TO worker_accessor;
  GRANT SELECT, UPDATE ON agent_improvement_proposals TO api_accessor;
  ```
- Groundwork rule is now fully realized for Section 1.0: score-triggered rows are first persisted with `generation_status = 'PENDING'`, `proposed_changes = '[]'`, `status = 'PENDING_REVIEW'`, and `auto_apply_at = NULL`. The async generator later fills `proposed_changes`, flips `generation_status` to `READY`, or rejects the row with a stable rejection code.
- 1.3 DONE `proposed_changes` targets dynamic harness topology changes, not numeric-only runtime knobs:
  - `stage_insert` — insert a non-gate stage before an existing stage while preserving compile validity
  - `stage_binding` — rebind an existing stage's agent/skill assignment when the resulting profile still compiles
  - Generated proposals in this slice are dynamic-agent oriented: the reconciler derives the current gate stage from the active profile and emits a structured `stage_insert` proposal that copies that stage's `role` and pinned `skill`, rather than assuming static `echo` / `scout` / `warden` bindings

  **Explicitly excluded** from proposable fields (rejected at schema validation):
  - `system_prompt_appendix` — direct prompt injection vector; removed to eliminate LLM-generated text in future system prompts
  - Any auth, billing, or network config field
  - Model selection or provider configuration
  - Any agent/stage mutation that references an unregistered skill, violates workspace entitlement limits, or bypasses harness compile validation

  Each change object remains structured JSON:
  `{"target_field":"stage_insert","current_value":null,"proposed_value":{"agent_id":"...","insert_before_stage_id":"verify","stage_id":"verify-precheck","role":"autoworkerready","skill":"clawhub://usezombie/autoworkerready@1.0.0","artifact_name":"verify-precheck.md","commit_message":"agent: add verify-precheck.md","gate":false,"on_pass":"verify","on_fail":"retry"},"rationale":"..."}`

  Validation rules enforced before a generated proposal may be stored as `READY`:
  - referenced agent ids must exist in the workspace
  - custom skills must be pinned and allowed by workspace entitlements
  - the resulting candidate profile must still parse and compile
  - entitlement checks run against the candidate profile and reject on profile or stage limit overflow
- 1.4 DONE Proposal generation is asynchronous but deterministic in this slice. A reconcile/background tick scans `generation_status = 'PENDING'` rows, materializes structured `proposed_changes`, validates them against the active config version and workspace entitlements, and updates the row to `READY` or `REJECTED`. No LLM call is used in this slice; M10_003 remains the separate track for LLM-assisted scoring work.

**Verification note:** compile-time verification and test wiring are complete in this branch, and DB-backed proposal tests now cover placeholder persistence, generated topology payloads, rejection paths, and dynamic auto-agent team fixtures. Aggregate runtime verification remains blocked locally by test Postgres authentication for user `clawable`.

---

## 2.0 Agent Trust Level

**Status:** DONE

An agent earns autonomous approval rights by demonstrating sustained high-quality execution.
Trust is computed, not granted — it cannot be manually assigned.

**Dimensions:**
- 2.1 DONE Define `TRUSTED` threshold: agent has >= 10 consecutive scored runs all in Gold or Elite tier (score >= 70 each); tracked as `trust_streak_runs` on `agent_profiles`
- 2.2 DONE Trust evaluation uses M9_003 failure classification to distinguish infrastructure failures from agent-attributable failures:
  - **Infrastructure failures** (`failure_is_infra = true`: TIMEOUT, OOM, CONTEXT_OVERFLOW, AUTH_FAILURE) do NOT reset `trust_streak_runs`. The run is excluded from the streak count (neither increments nor resets).
  - **Agent-attributable failures** (`failure_is_infra = false`: BAD_OUTPUT_FORMAT, TOOL_CALL_FAILURE, UNHANDLED_EXCEPTION, UNKNOWN) with score < 70 reset `trust_streak_runs` to 0.
  - **Successful runs scoring Gold+ (>= 70)** increment `trust_streak_runs` by 1.
  This is computed from the scored-run history in the scoring persistence path and only excludes the explicit infra classes above; any counted sub-Gold run resets the streak.
- 2.3 DONE `agent_profiles` exposes `trust_level` (enum: `UNEARNED` | `TRUSTED`) and `trust_streak_runs` (int); surfaced in `GET /v1/agents/{agent_id}` and passed through `zombiectl agent profile <agent-id>` output
- 2.4 DONE PostHog event `agent.trust.earned` emitted when agent crosses from UNEARNED → TRUSTED; `agent.trust.lost` emitted on reset — both include `agent_id`, `run_id`, `consecutive_count_at_event`

**Verification note:** the trust-state implementation is wired in the scoring persist path, the agent profile API, and `zombiectl` profile output. Local DB-backed runtime verification remains blocked in this environment by Postgres authentication for user `clawable`, but the branch now includes explicit trust-streak tests for Gold promotion, infra exclusion, and trusted-to-unearned reset, plus CLI unit coverage for trust fields in profile output.

---

## 3.0 Confidence-Based Auto-Approval

**Status:** IN_PROGRESS

TRUSTED agents bypass the manual approval gate. Proposals enter a 24-hour veto window
instead, during which the operator can inspect and cancel before the change applies.

**Dimensions:**
- 3.1 DONE When a proposal is generated for a TRUSTED agent, set `approval_mode = AUTO` and `auto_apply_at = created_at + 24h`; status transitions to `VETO_WINDOW` immediately
- 3.2 DONE `zombiectl agent proposals <agent-id>` lists VETO_WINDOW proposals prominently with a countdown: `"Auto-applies in 18h 42m — zombiectl agent proposals veto <proposal-id> to cancel"`
- 3.3 DONE `zombiectl agent proposals veto <proposal-id> [--reason "..."]` — operator cancels; status transitions to `VETOED`; agent is not penalized but reason is stored; next proposal on next trigger
- 3.4 DONE A background checker (reconcile tick) queries `WHERE status = 'VETO_WINDOW' AND auto_apply_at <= now()` and transitions matching proposals through the current auto-apply path. `applied_by` is recorded as `system:auto`, and the active harness config is advanced to the generated candidate version.
- 3.5 IN_PROGRESS **CAS guard before apply:** Before auto-applying a proposal, compare the harness config's current `config_version_id` to the proposal's `config_version_id` field. If they differ (operator changed the harness since the proposal was generated), reject the proposal with status `CONFIG_CHANGED` and reason `CONFIG_CHANGED_SINCE_PROPOSAL`. The CAS rejection is implemented for the reconcile auto-apply path; PostHog notification and the manual-approval path remain pending.
- 3.6 DONE **Reconciler for stuck proposals:** The same reconcile scan handles overdue veto-window proposals by selecting any `auto_apply_at <= now()`, so proposals missed for more than 1 hour are picked up on the next tick without a separate recovery path.

---

## 4.0 Manual Approval (UNEARNED Agents)

**Status:** DONE

Agents that have not earned TRUSTED status require explicit operator action on every proposal.

**Dimensions:**
- 4.1 DONE `zombiectl agent proposals <agent-id>` lists READY `PENDING_REVIEW` manual proposals through the new `/v1/agents/{agent_id}/proposals` API so operators can inspect structured changes before acting
- 4.2 DONE `zombiectl agent proposals <agent-id> approve <proposal-id>` approves manual proposals through `/v1/agents/{agent_id}/proposals/{proposal_id}:approve`; the backend performs the CAS check, flips the row through `APPROVED`, and applies the candidate profile in the same transaction before ending at `APPLIED`
- 4.3 DONE `zombiectl agent proposals <agent-id> reject <proposal-id> [--reason "..."]` rejects READY manual proposals through `/v1/agents/{agent_id}/proposals/{proposal_id}:reject`; the rejection reason is persisted with the final `REJECTED` state
- 4.4 DONE Manual proposals older than 7 days now auto-expire to `REJECTED` with reason `EXPIRED` inside the same reconcile tick that already handles overdue auto-apply proposals

---

## 5.0 Harness Change Application And Tracking

**Status:** DONE

Approved proposals (auto or manual) are applied atomically; every change is versioned and reversible.

**Dimensions:**
- 5.1 DONE On APPROVED or auto-apply: apply `proposed_changes` through the existing harness control plane path (compile → activate). If compile fails, reject proposal with status `REJECTED` and reason `COMPILE_FAILED`. If activate fails, reject with `ACTIVATE_FAILED`. Create a `harness_change_log` record per field changed:
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
- 5.2 DONE Applied change is immediately reflected in the next run's profile resolution (no restart required); harness control plane already supports this via `workspace_active_config`. No mid-run config mutation.
- 5.3 DONE Revert path: `zombiectl agent harness revert <agent-id> --to-change <change-id>` restores the harness to pre-change state via compile → activate with old_value; creates a new `harness_change_log` entry with `reverted_from` reference; revert does not affect trust level
- 5.4 DONE PostHog event `agent.harness.changed` emitted on apply with fields: `agent_id`, `proposal_id`, `approval_mode`, `fields_changed` (array), `trigger_reason`

**Verification note:** DB-backed proposal apply and revert coverage now verifies atomic `harness_change_log` persistence for manual and auto-applied proposals, `COMPILE_FAILED` rejection when stored proposal payloads can no longer compile, `ACTIVATE_FAILED` rejection when activation context is inconsistent, immediate `workspace_active_config` updates so the next profile resolution sees the applied harness without restart, applied-proposal telemetry payload loading used by the manual-approval HTTP path and auto-approval reconcile tick for `agent.harness.changed`, and operator-triggered revert flows that restore prior `stage_insert` / `stage_binding` topology state while appending a new `reverted_from` audit row.

---

## 6.0 Improvement Trajectory Measurement

**Status:** DONE

Measure whether applied proposals actually improve the agent's score.

**Dimensions:**
- 6.1 DONE After each applied proposal, tag the next 5 runs in `agent_run_scores` with nullable `proposal_id` referencing the applied proposal that opened the post-change window
- 6.2 DONE Compute `score_delta`: avg score of the tagged post-change window minus avg score of the 5 runs before the applied proposal; persist the finalized delta on every `harness_change_log` row written for that proposal
- 6.3 DONE `zombiectl agent improvement-report <agent-id>` now reads `/v1/agents/{agent_id}/improvement-report` and prints trust level, proposal lifecycle counts, average score delta per applied change, and current vs baseline tier
- 6.4 DONE When 3 consecutive applied proposals finalize with negative `score_delta`, the scoring path emits `agent.improvement.stalled`, resets the agent trust state to `UNEARNED`, and surfaces `improvement_stalled_warning: true` in profile/report output

**Verification note:** `zig build test` passes in this worktree, including new coverage for proposal-linked post-change windows, `score_delta` persistence, stalled-improvement trust resets, and improvement report aggregation. `bun test zombiectl/test/agent_profile.unit.test.js zombiectl/test/agent_improvement_report.unit.test.js zombiectl/test/help.test.js` passes. Aggregate `make test-unit` advances through `zombied` and `zombiectl`, then stops in the website package because `vitest` is not installed locally; `make lint` similarly reaches the website package and stops because `eslint` is not installed locally.

---

## 7.0 Acceptance Criteria

**Status:** PENDING

- [x] 7.1 Proposal is enqueued asynchronously after the scoring trigger condition is met; a background reconcile path materializes `PENDING` rows into `READY` proposals
- [x] 7.2 Proposal with malformed `proposed_changes` is rejected at validation, not silently stored as `READY`
- [x] 7.3 Proposal targeting `system_prompt_appendix` is rejected at schema validation
- [x] 7.4 Proposal that would exceed workspace entitlement stage limits is rejected with the stable entitlement reason code
- [x] 7.5 Generated proposals are persisted in final structured topology form (`stage_insert` / `stage_binding` payloads), not only placeholder rows
- [x] 7.6 Proposal attempting to reference an unregistered or entitlement-disallowed dynamic agent/skill is rejected at schema validation
- [x] 7.7 Agent with 10 consecutive Gold+ runs (excluding infra failures) shows `trust_level: TRUSTED` in profile output
- [x] 7.8 TRUSTED agent proposal enters VETO_WINDOW with correct `auto_apply_at` timestamp
- [x] 7.9 Operator veto within 24h prevents application; status shows VETOED, harness unchanged
- [x] 7.10 TRUSTED agent drops an agent-attributable Silver run → `trust_streak_runs` resets to 0 → next proposal requires manual approval
- [x] 7.11 TRUSTED agent has a TIMEOUT (infra) run → `trust_streak_runs` unchanged → trust preserved
- [x] 7.12 CAS version check: proposal generated against config_version_id X, operator changes config, auto-apply attempt rejects with CONFIG_CHANGED_SINCE_PROPOSAL
- [x] 7.13 Revert restores previous value exactly; `applied_by` on revert row shows operator identity
- [x] 7.14 No harness change applied without a proposal record in APPROVED/VETO_WINDOW state (enforced at application logic level)
- [ ] 7.15 Demo evidence: agent earns TRUSTED, generates auto-approved proposal, harness updates, score improves over next 5 runs

**Verification note:** `zig build test` passes in this worktree. `bun test zombiectl/test/agent_proposals.unit.test.js zombiectl/test/help.test.js` passes, including JSON contract coverage for proposal listing output. `HANDLER_DB_TEST_URL=postgres://usezombie:usezombie@localhost:5432/usezombiedb make test-integration-db` now passes, including the DB-backed proposal suite plus new architecture-focused coverage for veto surviving the reconcile deadline, manual approval failing closed on CAS drift while leaving the active harness unchanged, and a second reconcile pass remaining idempotent after the first auto-apply. The Zig unit suite also now covers exact-threshold and misuse edges including the sustained-low `60` boundary, `auto_apply_at == now()`, manual proposal expiry at the exact cutoff, duplicate-apply returning no-op, and veto attempts against manual proposals. `make test-unit` advances through `zombied` and `zombiectl`, including the new veto/countdown/operator UX coverage, and still stops at the website package because `vitest` is not available locally. The Zig proposal code was also split into smaller modules so the touched production files now stay under the 450-line target. Demo evidence for a full trusted-run improvement loop remains pending local reproduction.

---

## 8.0 Out of Scope

- Manual override to grant TRUSTED status without earning it through run history
- Changes to auth config, billing tier, or network policy via proposals
- `system_prompt_appendix` as a proposable field (prompt injection risk)
- Multi-agent cooperative improvement (one agent learning from another's harness)
- LLM provider or model selection as a proposable change (deferred)
- Veto window length as a user-configurable setting (fixed at 24h in v1)
- Synchronous proposal generation (always async/enqueued)
