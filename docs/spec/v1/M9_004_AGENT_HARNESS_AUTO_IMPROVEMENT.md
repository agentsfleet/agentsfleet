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

After sufficient score history accumulates (minimum 5 runs), the agent generates a structured
improvement proposal targeting its own harness configuration.

**Dimensions:**
- 1.1 PENDING Trigger proposal generation when: agent has ≥5 scored runs AND current 5-run rolling avg score < previous 5-run rolling avg score (trajectory is declining) OR avg score < 60 for any 5-run window
- 1.2 PENDING Proposal is a structured document: `agent_improvement_proposals` record with `agent_id`, `proposal_id` (uuidv7), `trigger_reason` (enum: `DECLINING_SCORE` | `SUSTAINED_LOW_SCORE`), `proposed_changes` (jsonb array of `{target_field, current_value, proposed_value, rationale}`), `approval_mode` (enum: `AUTO` | `MANUAL`), `status` (enum: `PENDING_REVIEW` | `VETO_WINDOW` | `APPROVED` | `REJECTED` | `APPLIED` | `VETOED`), `auto_apply_at` (timestamptz, null if MANUAL), `created_at`
- 1.3 PENDING `proposed_changes` targets harness-level fields only: `max_tokens`, `timeout_seconds`, `tool_allowlist`, `system_prompt_appendix` — no changes to auth, billing, or network config
- 1.4 PENDING Proposal generation uses the agent's own LLM call with a constrained prompt: inject last 10 run analyses + current harness config; output is validated against the `proposed_changes` schema before persisting (reject malformed output, do not retry blindly)

---

## 2.0 Agent Trust Level

**Status:** PENDING

An agent earns autonomous approval rights by demonstrating sustained high-quality execution.
Trust is computed, not granted — it cannot be manually assigned.

**Dimensions:**
- 2.1 PENDING Define `TRUSTED` threshold: agent has ≥10 consecutive scored runs all in Gold or Elite tier (score ≥70 each); tracked as `consecutive_gold_plus_runs` on `agent_profiles`
- 2.2 PENDING `TRUSTED` status is re-evaluated after every run; a single Bronze or Silver result resets `consecutive_gold_plus_runs` to 0 and drops the agent back to `MANUAL` approval mode
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
- 3.4 PENDING A background job checks `auto_apply_at <= now()` and transitions `VETO_WINDOW → APPLIED`; `applied_by` recorded as `system:auto`; same atomic harness-change path as manual approval

---

## 4.0 Manual Approval (UNEARNED Agents)

**Status:** PENDING

Agents that have not earned TRUSTED status require explicit operator action on every proposal.

**Dimensions:**
- 4.1 PENDING `zombiectl agent proposals <agent-id>` — list PENDING_REVIEW proposals with proposed changes and rationale
- 4.2 PENDING `zombiectl agent proposals approve <proposal-id>` — operator approves; status transitions to APPROVED then immediately APPLIED in the same transaction
- 4.3 PENDING `zombiectl agent proposals reject <proposal-id> [--reason "..."]` — operator rejects; status transitions to REJECTED; reason stored
- 4.4 PENDING Proposals older than 7 days without a decision auto-expire to REJECTED with reason `EXPIRED`; agent generates a new proposal on next trigger

---

## 5.0 Harness Change Application And Tracking

**Status:** PENDING

Approved proposals (auto or manual) are applied atomically; every change is versioned and reversible.

**Dimensions:**
- 5.1 PENDING On APPROVED or auto-apply, apply `proposed_changes` to the agent's harness config atomically; create a `harness_change_log` record: `agent_id`, `proposal_id`, `field`, `old_value`, `new_value`, `applied_at`, `applied_by` (`operator:<identity>` or `system:auto`)
- 5.2 PENDING Applied change is immediately reflected in the next run (no restart required); no mid-run config mutation
- 5.3 PENDING Revert path: `zombiectl agent harness revert <agent-id> --to-change <change-id>` restores the harness to pre-change state; creates a new `harness_change_log` entry with `reverted_from` reference; revert does not affect trust level
- 5.4 PENDING PostHog event `agent.harness.changed` emitted on apply with fields: `agent_id`, `proposal_id`, `approval_mode`, `fields_changed` (array), `trigger_reason`, `score_before_avg`, `score_after_avg` (populated after 5 post-change runs)

---

## 6.0 Improvement Trajectory Measurement

**Status:** PENDING

Measure whether applied proposals actually improve the agent's score.

**Dimensions:**
- 6.1 PENDING After each applied change, tag the next 5 runs as `post_change_window: true` in `agent_run_scores`
- 6.2 PENDING Compute `score_delta`: avg score of post-change window minus avg score of 5 runs before the change; store on `harness_change_log`
- 6.3 PENDING `zombiectl agent improvement-report <agent-id>` — prints: trust level, proposals generated/approved/vetoed/rejected/applied, avg score delta per applied change, current vs baseline tier
- 6.4 PENDING If 3 consecutive applied proposals each produce negative `score_delta`, emit `agent.improvement.stalled` event, surface warning in CLI profile output, and reset trust level to UNEARNED regardless of consecutive_gold_plus_runs count

---

## 7.0 Acceptance Criteria

**Status:** PENDING

- [ ] 7.1 Proposal generated within one scored run of hitting the trigger condition
- [ ] 7.2 Proposal with malformed `proposed_changes` is rejected at ingest, not silently stored
- [ ] 7.3 Agent with 10 consecutive Gold+ runs shows `trust_level: TRUSTED` in profile output
- [ ] 7.4 TRUSTED agent proposal enters VETO_WINDOW with correct `auto_apply_at` timestamp
- [ ] 7.5 Operator veto within 24h prevents application; status shows VETOED, harness unchanged
- [ ] 7.6 TRUSTED agent drops a Silver run → `consecutive_gold_plus_runs` resets to 0 → next proposal requires manual approval
- [ ] 7.7 Revert restores previous value exactly; `applied_by` on revert row shows operator identity
- [ ] 7.8 No harness change applied without a proposal record in APPROVED/VETO_WINDOW state (enforced at DB constraint level)
- [ ] 7.9 Demo evidence: agent earns TRUSTED, generates auto-approved proposal, harness updates, score improves over next 5 runs

---

## 8.0 Out of Scope

- Manual override to grant TRUSTED status without earning it through run history
- Changes to auth config, billing tier, or network policy via proposals
- Multi-agent cooperative improvement (one agent learning from another's harness)
- LLM provider or model selection as a proposable change (deferred)
- Veto window length as a user-configurable setting (fixed at 24h in v1)
