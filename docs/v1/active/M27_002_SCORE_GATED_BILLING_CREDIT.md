# M27_002: Score-Gated Billing Credit

**Prototype:** v1.0.0
**Milestone:** M27
**Workstream:** 002
**Date:** Apr 04, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — Revenue-relevant; ties quality scoring to billing, differentiator trust signal
**Batch:** B1 (parallel with M27_001 — no dependency between them)
**Branch:** feat/m27-002-score-gated-billing
**Depends on:** M9_001 (scoring engine — DONE), M6_002 (credit lifecycle — DONE)

---

## Context

Today, every completed agent run is billed regardless of output quality. An agent that produces garbage output (wrong PR, broken code, failed all stages) still consumes credits. This erodes operator trust — they're paying for failures.

The scoring engine (M9) already classifies runs into tiers:

| Tier | Score Range | Meaning |
|------|-------------|---------|
| Bronze | 0–39 | Poor quality |
| Silver | 40–69 | Acceptable |
| Gold | 70–89 | Good |
| Elite | 90–100 | Excellent |

The billing system (M6) already supports non-billable runs via `FinalizeOutcome.non_billable` in `src/state/billing_runtime.zig`. When a run is marked non-billable:
- A `run_not_billable` event is recorded in `usage_ledger`
- `billable_quantity` is set to 0
- No workspace billing reconciliation occurs

**This workstream connects the two:** runs scoring below Bronze (score < 40) are automatically marked non-billable. The operator never pays for garbage.

This is a concrete trust signal: "We only charge for quality output."

---

## 1.0 Score-Gate Hook in Billing Finalization

**Status:** PENDING

Insert a score check between scoring and billing finalization in the worker pipeline. The scoring happens synchronously before billing finalization in `worker_stage_outcomes.zig`, so the score is available at decision time.

### 1.1 Gate Logic

After `scoreRunIfTerminal()` completes and before `finalizeRunForBilling()` is called:

1. Read the score from the just-persisted `agent_run_scores` row (or from the in-memory return value).
2. If `score < BILLING_QUALITY_THRESHOLD` (40, matching Bronze floor), override `FinalizeOutcome` to `.non_billable`.
3. Log the override: `"run {run_id} scored {score} (< {threshold}), marking non-billable"`.

**Dimensions:**
- 1.1.1 PENDING Constant `BILLING_QUALITY_THRESHOLD = 40` defined in scoring types or billing constants
- 1.1.2 PENDING Gate check wired between scoring and `finalizeRunForBilling()` in `worker_stage_outcomes.zig`
- 1.1.3 PENDING Runs already marked `.non_billable` for other reasons (retries, cancellation, resource limits) remain non-billable — the score gate is additive, never overrides to `.completed`
- 1.1.4 PENDING Unscored runs (scoring failed or agent has no profile) are NOT affected — they follow existing billing path

---

## 2.0 Observability

**Status:** PENDING

Operators and internal dashboards need visibility into score-gated billing decisions.

**Dimensions:**
- 2.1 PENDING `usage_ledger` finalization record includes a `gate_reason` or equivalent field indicating why a run was marked non-billable (e.g., `"score_below_threshold"` vs `"cancelled"` vs `"retry"`)
- 2.2 PENDING PostHog event `agent.run.billing_gated` emitted when a run is score-gated, with properties: `run_id`, `score`, `threshold`, `workspace_id`, `agent_id`
- 2.3 PENDING `zombiectl workspace billing` (or equivalent CLI surface) shows score-gated runs distinctly from other non-billable runs

---

## 3.0 Threshold Configuration

**Status:** PENDING

The threshold (40) is a constant, not per-workspace configurable. This is intentional for v1 — a single global threshold is simpler and avoids gaming. Future per-workspace overrides are out of scope.

**Dimensions:**
- 3.1 PENDING Threshold is a named constant, not a magic number
- 3.2 PENDING Threshold value (40) matches the Bronze tier floor in `math.tierFromScore()` — if tier boundaries change, the billing threshold should be reviewed

---

## 4.0 Acceptance Criteria

**Status:** PENDING

- [ ] 4.1 A run scoring 39 (Bronze) is automatically marked non-billable — `billable_quantity = 0` in usage_ledger
- [ ] 4.2 A run scoring 40 (Silver floor) is billed normally — `billable_quantity > 0`
- [ ] 4.3 A run already non-billable for other reasons (e.g., cancelled) remains non-billable even if score is 80
- [ ] 4.4 A run where scoring fails (no score persisted) is billed normally — score gate does not block billing
- [ ] 4.5 PostHog event `agent.run.billing_gated` is emitted with correct properties
- [ ] 4.6 Score-gated runs appear in billing CLI output with distinct non-billable reason

---

## 5.0 Out of Scope

- Per-workspace threshold configuration
- Retroactive credit refunds for already-billed low-score runs
- Operator notification/email when runs are score-gated (covered by Quality Drift Alert TODO)
- Partial billing (e.g., bill 50% for Silver runs) — binary billable/non-billable only
- Revenue impact modeling — this is a product/business decision, not a code concern
