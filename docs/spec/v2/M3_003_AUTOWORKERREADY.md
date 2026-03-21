# M3_003: Autoworkerready

**Version:** v2
**Milestone:** M3
**Workstream:** 003
**Date:** Mar 21, 2026
**Status:** PENDING
**Priority:** P1
**Depends on:** M3_002 (autoworkerstandup — worker process running)
**Batch:** B8 — sequential after M3_002

---

## Problem

A deployed worker must be verified healthy before it serves production runs. Health check, connectivity to Redis and Postgres, and successful test run must all pass.

## Decision

Build an autoworkerready agent that runs a verification suite against a newly deployed worker.

---

## 1.0 Health Verification

**Status:** PENDING

**Dimensions:**
- 1.1 PENDING Hit worker health endpoint (local or via Tailscale)
- 1.2 PENDING Verify Redis consumer group membership
- 1.3 PENDING Verify Postgres connectivity
- 1.4 PENDING Submit a test run (echo agent) and verify completion

---

## 2.0 Reporting

**Status:** PENDING

**Dimensions:**
- 2.1 PENDING Discord notification: worker healthy, serving runs
- 2.2 PENDING If DEV: set `DEV_WORKER_READY=true` GitHub variable
- 2.3 PENDING If verification fails: alert and block from serving

---

## 3.0 Acceptance Criteria

- [ ] 3.1 Worker verified healthy and serving runs
- [ ] 3.2 Notification sent
- [ ] 3.3 Failed verification blocks worker from serving
