# M2_001: Autoprovisioner

**Version:** v2
**Milestone:** M2
**Workstream:** 001
**Date:** Mar 21, 2026
**Status:** PENDING
**Priority:** P1
**Depends on:** M1_003 (autoprocurer hands off IP + credentials)
**Batch:** B3 — blocked on M1_003

---

## Problem

After a server is provisioned, it needs SSH key deployment, Tailscale enrollment, and public IP access dropped. Today this is manual (M4_001 playbook steps 1.0–3.0).

## Decision

Build an autoprovisioner agent that reads the server's vault item (written by autoprocurer) and performs initial access setup.

---

## 1.0 SSH Key Deployment

**Status:** PENDING

**Dimensions:**
- 1.1 PENDING Generate Ed25519 deploy key pair
- 1.2 PENDING SSH to server using initial credentials from vault
- 1.3 PENDING Install deploy public key in `authorized_keys`
- 1.4 PENDING Store private key in vault: `zombie-{env}-worker-{name}/ssh-private-key`
- 1.5 PENDING Disable password authentication

---

## 2.0 Tailscale Enrollment

**Status:** PENDING

**Dimensions:**
- 2.1 PENDING Install Tailscale on server
- 2.2 PENDING Join Tailnet using auth key from vault
- 2.3 PENDING Record Tailscale IP in vault item
- 2.4 PENDING Drop public IP access (firewall rules — only Tailscale reachable)

---

## 3.0 Acceptance Criteria

- [ ] 3.1 SSH access works via Tailscale IP using deploy key
- [ ] 3.2 Public IP SSH is blocked
- [ ] 3.3 Vault item updated with SSH key and Tailscale IP
- [ ] 3.4 Handoff to autohardener triggered
