# M1_004: Autoreviewer

**Version:** v2
**Milestone:** M1
**Workstream:** 004
**Date:** Mar 21, 2026
**Status:** PENDING
**Priority:** P2
**Depends on:** M1_002 (autoprocurer provider playbook format)
**Batch:** B1 — can parallel with M1_001, M1_002

---

## Problem

The provider playbook (`providers/playbook.yml`) must stay current — pricing changes, availability shifts, new providers emerge. Manual research is slow and infrequent.

## Decision

Build an autoreviewer agent that periodically evaluates providers and proposes playbook updates as PRs for human review.

---

## 1.0 Provider Evaluation

**Status:** PENDING

**Dimensions:**
- 1.1 PENDING Fetch pricing and availability from candidate providers (weekly cadence)
- 1.2 PENDING Evaluate against criteria: KVM support, bare-metal availability in target regions, pricing vs current, compliance (data residency)
- 1.3 PENDING Compare against current playbook entries

---

## 2.0 Playbook Update Proposal

**Status:** PENDING

**Dimensions:**
- 2.1 PENDING Generate PR with proposed changes to `providers/playbook.yml`
- 2.2 PENDING Include evaluation rationale in PR description
- 2.3 PENDING Does NOT auto-merge — human approves provider additions

---

## 3.0 Acceptance Criteria

- [ ] 3.1 Weekly evaluation runs without manual trigger
- [ ] 3.2 PR created with clear rationale
- [ ] 3.3 No auto-merge — human approval required for provider trust decisions
