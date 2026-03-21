# M3_004: Autodrainer & Autoupgrader

**Version:** v2
**Milestone:** M3
**Workstream:** 004
**Date:** Mar 21, 2026
**Status:** PENDING
**Priority:** P1
**Depends on:** M3_003 (autoworkerready — workers serving runs)
**Batch:** B9 — sequential after M3_003

---

## Problem

Two lifecycle operations require automation:
1. **Drain:** When a worker fails, its in-flight runs must be redistributed before the worker is removed
2. **Upgrade:** When a new release ships, workers must be upgraded with zero downtime

## Part A: Autodrainer

### 1.0 Failure Detection & Drain

**Status:** PENDING

**Dimensions:**
- 1.1 PENDING Trigger: Grafana alert (worker health check failure)
- 1.2 PENDING Redistribute pending runs via `XAUTOCLAIM` to healthy workers
- 1.3 PENDING Mark failing server as drained in control plane
- 1.4 PENDING If replacement needed: emit scaling recommendation to autoprocurer (M1_003)
- 1.5 PENDING Call `destroy(handle)` on provider plugin if server is unrecoverable

### 2.0 Acceptance Criteria

- [ ] 2.1 In-flight runs redistributed without loss
- [ ] 2.2 Drained server removed from consumer group
- [ ] 2.3 Replacement triggered if needed

---

## Part B: Autoupgrader

### 3.0 Rolling Upgrade

**Status:** PENDING

**Dimensions:**
- 3.1 PENDING Trigger: new release tag (periodic check or webhook)
- 3.2 PENDING Upgrade workers one at a time (rolling — no simultaneous restart)
- 3.3 PENDING Per worker: drain → upgrade binary → restart → verify health (reuse M3_003 checks)
- 3.4 PENDING Rollback: if health check fails after upgrade, revert to previous binary and alert

### 4.0 Acceptance Criteria

- [ ] 4.1 Zero-downtime upgrade across all workers
- [ ] 4.2 Rollback functional on health check failure
- [ ] 4.3 Discord notification on upgrade completion
