# M27_002: Score-Gated Billing Credit

**Prototype:** v1.0.0
**Milestone:** M27
**Workstream:** 002
**Date:** Apr 04, 2026
**Status:** DONE
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

**Status:** DONE

Insert a score check between scoring and billing finalization in the worker pipeline. `scoreRunForBillingGate()` is called synchronously in `handleDoneOutcome()` before `finalizeRunForBilling()`, so the score is available at decision time.

### 1.1 Gate Logic

**Dimensions:**
- 1.1.1 ✅ Constant `BILLING_QUALITY_THRESHOLD = 40` defined in `src/pipeline/scoring_mod/types.zig`, re-exported via `scoring.zig`
- 1.1.2 ✅ Gate check wired in `handleDoneOutcome()` in `src/pipeline/worker_stage_outcomes.zig` via `scoreRunForBillingGate()` → `.score_gated` outcome
- 1.1.3 ✅ Runs already marked `.non_billable` for other reasons (retries, cancellation, resource limits) remain non-billable — gate is additive only, never overrides to `.completed`
- 1.1.4 ✅ Unscored runs (scoring disabled, empty `agent_id`) return `null` from `scoreRunForBillingGate` and follow existing `.completed` billing path

---

## 2.0 Observability

**Status:** DONE (2.1, 2.2) / DEFERRED (2.3)

**Dimensions:**
- 2.1 ✅ `usage_ledger` finalization record uses `lifecycle_event = "run_not_billable_score_gated"` for score-gated runs — distinct from `"run_not_billable"` (cancel/retry) and `"run_completed"`
- 2.2 ✅ PostHog event `agent.run.billing_gated` emitted with properties: `run_id`, `score`, `threshold`, `workspace_id`, `agent_id` via `posthog_events.trackRunBillingGated()`
- 2.3 DEFERRED — `zombiectl workspace billing` CLI surface showing score-gated runs distinctly. **`workspace billing` subcommand does not exist today** — only `workspace upgrade-scale` exists. This requires: new `GET /v1/workspaces/:id/billing/runs` API endpoint + new CLI subcommand. Deferred to a follow-on workstream. The `lifecycle_event = "run_not_billable_score_gated"` rows are already written and queryable.

---

## 3.0 Threshold Configuration

**Status:** DONE

**Dimensions:**
- 3.1 ✅ Threshold is `BILLING_QUALITY_THRESHOLD = 40` in `scoring_mod/types.zig` — not a magic number
- 3.2 ✅ Value (40) matches Bronze tier floor in `math.tierFromScore()` — guarded by unit test `"billing gate threshold is 40 matching Bronze floor"`

---

## 4.0 Acceptance Criteria

**Status:** DONE

- [x] 4.1 A run scoring 39 (Bronze) is automatically marked non-billable — `billable_quantity = 0` in usage_ledger (`billing_gate_test.zig: "Bronze boundary 39"`)
- [x] 4.2 A run scoring 40 (Silver floor) is billed normally — `billable_quantity > 0` (`billing_gate_test.zig: "Silver floor 40"`)
- [x] 4.3 A run already non-billable for other reasons (e.g., cancelled) remains non-billable even if score is 80 (`billing_gate_test.zig: "non_billable run writes run_not_billable"`)
- [x] 4.4 A run where scoring fails (no score persisted) is billed normally — score gate does not block billing (`billing_gate_scoring_test.zig: "returns null for empty agent_id"`, `"returns null when scoring disabled"`)
- [x] 4.5 PostHog event `agent.run.billing_gated` is emitted with correct properties (`posthog_events.trackRunBillingGated`)
- [ ] 4.6 Score-gated runs appear in billing CLI output with distinct non-billable reason — DEFERRED with dim 2.3

---

## 5.0 Out of Scope

- Per-workspace threshold configuration
- Retroactive credit refunds for already-billed low-score runs
- Operator notification/email when runs are score-gated (covered by Quality Drift Alert TODO)
- Partial billing (e.g., bill 50% for Silver runs) — binary billable/non-billable only
- Revenue impact modeling — this is a product/business decision, not a code concern
- `zombiectl workspace billing` read surface — deferred to follow-on workstream (dim 2.3 above)
