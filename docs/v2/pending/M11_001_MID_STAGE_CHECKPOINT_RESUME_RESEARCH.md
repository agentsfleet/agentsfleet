# M11_001: Mid-Stage Checkpoint and Resume Feasibility Research

**Prototype:** v2.0.0
**Milestone:** M11
**Workstream:** 001
**Date:** Apr 05, 2026
**Status:** PENDING
**Priority:** P2 — high-impact reliability research with high implementation risk
**Batch:** B3
**Depends on:** M12_003 (executor invocation), M14_001 (orphan recovery), stable long-run production data

---

## 1.0 Problem Framing and Baseline Evidence

**Status:** PENDING

Establish whether stage-boundary durability is materially insufficient and where restart cost is highest.

**Dimensions:**
- 1.1 PENDING Quantify long-running stage durations and restart frequency from production traces/events (p50/p95/p99 by stage type)
- 1.2 PENDING Identify top restart cost drivers (token burn, repeated tool setup, repeated dependency install, provider reconnection)
- 1.3 PENDING Document current guarantees and failure behavior at stage boundaries, including orphan-run recovery interactions
- 1.4 PENDING Define explicit success criteria for a checkpoint/resume design (recovery time objective, correctness guarantees, complexity budget)

---

## 2.0 Design Space Exploration

**Status:** PENDING

Research multiple checkpoint models and evaluate tradeoffs before committing to implementation.

**Dimensions:**
- 2.1 PENDING Evaluate coarse checkpoint model (gate-iteration checkpoints only) versus fine-grained model (mid-turn tool-call checkpoints)
- 2.2 PENDING Evaluate provider/session constraints (NullClaw conversation state, tool process state, streaming buffers) that may block exact resume
- 2.3 PENDING Evaluate storage format options (append-only event log, state snapshot blob, hybrid) with deterministic replay semantics
- 2.4 PENDING Produce threat/risk analysis: state corruption, replay divergence, secret leakage, and operational blast radius

---

## 3.0 Prototype Plan and Decision Gate

**Status:** PENDING

Convert research into an executable recommendation with clear go/no-go criteria.

**Dimensions:**
- 3.1 PENDING Define minimum viable experiment in one isolated stage with synthetic crash-injection tests
- 3.2 PENDING Define acceptance harness that validates resumed execution equivalence versus clean uninterrupted execution
- 3.3 PENDING Provide implementation recommendation with explicit alternatives: `GO` (phase rollout) or `NO-GO` (keep stage-boundary durability)
- 3.4 PENDING If `GO`, define phased rollout plan with feature flag, blast-radius controls, and rollback strategy

---

## 4.0 Acceptance Criteria

**Status:** PENDING

- [ ] 4.1 Research dossier includes measured baseline data from real runs (not hypothetical estimates)
- [ ] 4.2 At least two concrete checkpoint models are evaluated with security, reliability, and complexity tradeoffs
- [ ] 4.3 Decision memo provides unambiguous GO/NO-GO recommendation and rationale
- [ ] 4.4 If GO, prototype plan includes deterministic crash-recovery test harness definition
- [ ] 4.5 Documentation is sufficient for another agent to execute phase 1 without additional tribal context

---

## 5.0 Out of Scope

- Full production implementation of checkpoint/resume in this workstream
- Cross-provider abstraction rewrite
- Changes to scoring, billing, or PR automation paths
- Any fallback that weakens existing auth/security model
