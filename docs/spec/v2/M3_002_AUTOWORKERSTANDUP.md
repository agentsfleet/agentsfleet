# M3_002: Autoworkerstandup

**Version:** v2
**Milestone:** M3
**Workstream:** 002
**Date:** Mar 21, 2026
**Status:** PENDING
**Priority:** P1
**Depends on:** M2_004 (autoinfraready — server marked ready)
**Batch:** B7 — blocked on M2_004

---

## Problem

Deploying the worker binary to a ready server is manual (copy binary, configure systemd, join Redis consumer group). This must be automated for scaling.

## Decision

Build an autoworkerstandup agent that deploys the latest `zombied` release to a ready server and starts the worker process.

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

## 2.0 Acceptance Criteria

- [ ] 2.1 Worker process running and joined consumer group
- [ ] 2.2 Handoff to autoworkerready triggered
