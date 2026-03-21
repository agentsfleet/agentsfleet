# M2_003: Autovaultsaver

**Version:** v2
**Milestone:** M2
**Workstream:** 003
**Date:** Mar 21, 2026
**Status:** PENDING
**Priority:** P1
**Depends on:** M2_002 (autohardener — server hardened and verified)
**Batch:** B5 — sequential after M2_002

---

## Problem

After hardening, the server's final state (Tailscale IP, SSH key, KVM status, provider metadata) must be recorded in 1Password in the standard format so downstream agents can consume it.

## Decision

Build an autovaultsaver agent that creates/updates the canonical 1Password item for the server.

---

## 1.0 Vault Item Management

**Status:** PENDING

**Dimensions:**
- 1.1 PENDING Create or update 1Password item: `zombie-{env}-worker-{name}` in the appropriate vault (`ZMB_CD_DEV` or `ZMB_CD_PROD`)
- 1.2 PENDING Required fields: `ssh-private-key`, `hostname`, `tailscale-ip`, `provider`, `provider-server-id`, `region`, `provisioned-at`, `hardened-at`
- 1.3 PENDING Validate all fields present before marking complete

---

## 2.0 Acceptance Criteria

- [ ] 2.1 Vault item created with all required fields
- [ ] 2.2 Item readable by downstream agents (autoworkerstandup, autoupgrader)
- [ ] 2.3 Handoff to autoinfraready triggered
