# M2_004: Autoinfraready

**Version:** v2
**Milestone:** M2
**Workstream:** 004
**Date:** Mar 21, 2026
**Status:** PENDING
**Priority:** P1
**Depends on:** M2_003 (autovaultsaver — vault item finalized)
**Batch:** B6 — sequential after M2_003

---

## Problem

After a server is provisioned, hardened, and vaulted, the team needs notification and the server needs to be marked as ready for worker deployment.

## Decision

Build an autoinfraready agent that sends a Discord notification and marks the server as ready in the control plane.

---

## 1.0 Readiness Gate

**Status:** PENDING

**Dimensions:**
- 1.1 PENDING Verify vault item has all required fields
- 1.2 PENDING Smoke-test SSH connectivity via Tailscale
- 1.3 PENDING Mark server as ready in control plane (e.g., GitHub variable or internal state)

---

## 2.0 Notification

**Status:** PENDING

**Dimensions:**
- 2.1 PENDING Send Discord notification with server name, region, provider, and Tailscale IP
- 2.2 PENDING Include time elapsed from procurement to ready

---

## 3.0 Acceptance Criteria

- [ ] 3.1 Discord notification sent
- [ ] 3.2 Server marked ready for worker deployment
- [ ] 3.3 Handoff to autoworkerstandup triggered
