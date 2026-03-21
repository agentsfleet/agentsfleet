# M1_003: Autoprocurer

**Version:** v2
**Milestone:** M1
**Workstream:** 003
**Date:** Mar 21, 2026
**Status:** PENDING
**Priority:** P1
**Depends on:** M1_001 (autoforecaster), M1_002 (autoprocurer provider)
**Batch:** B2 — blocked on M1_001 + M1_002

---

## Problem

Server procurement is manual (M4_001 playbook). The human buys a server from OVHCloud, records the IP and credentials, and hands off to the agent pipeline. This is the last human bottleneck in the scaling path.

## Decision

Build an autoprocurer agent that receives scaling recommendations from autoforecaster and provisions servers via the provider abstraction defined in M1_002.

---

## 1.0 Procurement Execution

**Status:** PENDING

**Dimensions:**
- 1.1 PENDING Receive scaling recommendation from autoforecaster
- 1.2 PENDING Select provider and machine type using M1_002 selection logic (KVM required, region affinity, cost, availability)
- 1.3 PENDING Call `provision(spec)` on selected provider plugin
- 1.4 PENDING Poll `status(handle)` until ready or timeout
- 1.5 PENDING On success: write `{ ip, credential_type, credential_value }` to vault and hand off to autoprovisioner (M2_001)
- 1.6 PENDING On failure: try next provider in rank order; if all fail, alert

---

## 2.0 Decision Logging

**Status:** PENDING

**Dimensions:**
- 2.1 PENDING Record provider selection rationale in vault item
- 2.2 PENDING Record provisioning duration and cost
- 2.3 PENDING Discord notification on successful procurement

---

## 3.0 Acceptance Criteria

- [ ] 3.1 Server provisioned without human intervention
- [ ] 3.2 Vault item created with all required fields (ip, credentials, provider metadata)
- [ ] 3.3 Handoff to autoprovisioner triggered automatically
- [ ] 3.4 Fallback to next provider on failure
