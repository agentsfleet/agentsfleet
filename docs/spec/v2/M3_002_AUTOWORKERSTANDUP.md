# M3_002: Autoworkerstandup

**Version:** v2
**Milestone:** M3
**Workstream:** 002
**Date:** Mar 21, 2026
**Status:** PENDING
**Priority:** P1
**Depends on:** M2_002 (autohardener — vault item finalized, server hardened)
**Batch:** B5 — blocked on M2_002

---

## Problem

Deploying the worker binary to a ready server is manual (copy binary, configure systemd, join Redis consumer group). Health verification and operator notification are also manual. All three steps must be automated for scaling.

## Decision

Build an autoworkerstandup agent that deploys the latest `zombied` release to a ready server, verifies it is healthy, and notifies the team.

---

## 1.0 Worker Deployment

**Status:** PENDING

**Dimensions:**
- 1.1 PENDING Read latest release tag from GitHub
- 1.2 PENDING SSH to server via Tailscale using vault credentials
- 1.3 PENDING Download and install `zombied` binary
- 1.4 PENDING Configure systemd unit with environment from vault (DB, Redis, encryption key, etc.)
- 1.5 PENDING Start worker process, verify it joins Redis consumer group

---

## 2.0 Health Verification

**Status:** PENDING

A deployed worker must be verified healthy before it serves production runs.

**Dimensions:**
- 2.1 PENDING Hit worker health endpoint (local or via Tailscale)
- 2.2 PENDING Verify Redis consumer group membership
- 2.3 PENDING Verify Postgres connectivity
- 2.4 PENDING Submit a test run (echo agent) and verify completion
- 2.5 PENDING If verification fails: alert operator and block worker from serving runs

---

## 3.0 Readiness Notification

**Status:** PENDING

**Dimensions:**
- 3.1 PENDING Send Discord notification: worker name, region, provider, Tailscale IP, time elapsed from procurement to ready
- 3.2 PENDING If DEV: set `DEV_WORKER_READY=true` GitHub variable

---

## 4.0 Acceptance Criteria

- [ ] 4.1 Worker process running and joined consumer group
- [ ] 4.2 Health checks pass (health endpoint, Redis, Postgres, test run)
- [ ] 4.3 Failed verification blocks worker from serving
- [ ] 4.4 Discord notification sent on success
