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
Proposals are deterministic, scoped, and human-approved before application.

**Dimensions:**
- 1.1 PENDING Trigger proposal generation when: agent has ≥5 scored runs AND current 5-run rolling avg score < previous 5-run rolling avg score (trajectory is declining) OR avg score < 60 for any 5-run window
- 1.2 PENDING Proposal is a structured document: `agent_improvement_proposals` record with `agent_id`, `proposal_id` (uuidv7), `trigger_reason` (enum: `DECLINING_SCORE` | `SUSTAINED_LOW_SCORE`), `proposed_changes` (jsonb array of `{target_field, current_value, proposed_value, rationale}`), `status` (enum: PENDING_REVIEW | APPROVED | REJECTED | APPLIED), `created_at`
- 1.3 PENDING `proposed_changes` targets harness-level fields only: `max_tokens`, `timeout_seconds`, `tool_allowlist`, `system_prompt_appendix` — no changes to auth, billing, or network config
- 1.4 PENDING Proposal generation uses the agent's own LLM call with a constrained prompt: inject last 10 run analyses + current harness config; output is validated against the `proposed_changes` schema before persisting (reject malformed output, do not retry blindly)

---

## 2.0 Human-In-The-Loop Approval

**Status:** PENDING

No harness change is applied without explicit operator approval.
This is a hard safety invariant, not a feature flag.

**Dimensions:**
- 2.1 PENDING `zombiectl agent proposals <agent-id>` — list pending proposals with proposed changes and rationale
- 2.2 PENDING `zombiectl agent proposals approve <proposal-id>` — operator approves; status transitions to APPROVED
- 2.3 PENDING `zombiectl agent proposals reject <proposal-id> [--reason "..."]` — operator rejects; status transitions to REJECTED; reason stored
- 2.4 PENDING Proposals older than 7 days without a decision auto-expire to REJECTED with reason `EXPIRED`; agent generates a new proposal on next trigger

---

## 3.0 Harness Change Application And Tracking

**Status:** PENDING

Approved proposals are applied atomically; every change is versioned and reversible.

**Dimensions:**
- 3.1 PENDING On APPROVED status, apply `proposed_changes` to the agent's harness config atomically; create a `harness_change_log` record: `agent_id`, `proposal_id`, `field`, `old_value`, `new_value`, `applied_at`, `applied_by` (operator token identity)
- 3.2 PENDING Applied change is immediately reflected in the next run (no restart required); no mid-run config mutation
- 3.3 PENDING Revert path: `zombiectl agent harness revert <agent-id> --to-change <change-id>` restores the harness to pre-change state; creates a new `harness_change_log` entry with `reverted_from` reference
- 3.4 PENDING PostHog event `agent.harness.changed` emitted on apply with fields: `agent_id`, `proposal_id`, `fields_changed` (array), `trigger_reason`, `score_before_avg`, `score_after_avg` (populated after 5 post-change runs)

---

## 4.0 Improvement Trajectory Measurement

**Status:** PENDING

Measure whether applied proposals actually improve the agent's score.

**Dimensions:**
- 4.1 PENDING After each applied change, tag the next 5 runs as `post_change_window: true` in `agent_run_scores`
- 4.2 PENDING Compute `score_delta`: avg score of post-change window minus avg score of 5 runs before the change; store on `harness_change_log`
- 4.3 PENDING `zombiectl agent improvement-report <agent-id>` — prints: proposals generated, approved, rejected, applied; avg score delta per applied change; current vs baseline tier
- 4.4 PENDING If 3 consecutive applied proposals each produce negative `score_delta`, emit `agent.improvement.stalled` event and surface warning in CLI profile output — agent may need manual intervention

---

## 5.0 Acceptance Criteria

**Status:** PENDING

- [ ] 5.1 Proposal generated within one scored run of hitting the trigger condition
- [ ] 5.2 Proposal with malformed `proposed_changes` (missing required field) is rejected at ingest, not silently stored
- [ ] 5.3 Harness change applied correctly for all four supported target fields
- [ ] 5.4 Revert restores previous value exactly (verified by reading harness config before and after)
- [ ] 5.5 No harness change applied without an APPROVED proposal record (enforced at DB constraint level, not application level only)
- [ ] 5.6 Demo evidence: agent starts Bronze, generates 2 improvement proposals, applies them, reaches Silver tier within 20 runs

---

## 6.0 Out of Scope

- Fully autonomous harness changes without human approval (hard no — safety invariant)
- Changes to auth config, billing tier, or network policy via proposals
- Multi-agent cooperative improvement (one agent learning from another's harness)
- LLM provider or model selection as a proposable change (deferred)
