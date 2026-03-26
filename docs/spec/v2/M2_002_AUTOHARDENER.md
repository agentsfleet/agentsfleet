# M2_002: Autohardener

**Version:** v2
**Milestone:** M2
**Workstream:** 002
**Date:** Mar 21, 2026
**Status:** PENDING
**Priority:** P1
**Depends on:** M2_001 (autoprovisioner — SSH via Tailscale available)
**Batch:** B4 — sequential after M2_001

---

## Problem

Provisioned servers need a security baseline before running workloads: firewall rules, fail2ban, Debian security updates, and KVM verification (required for Firecracker). After hardening, the server's final state must be recorded in 1Password so downstream agents can consume it.

## Decision

Build an autohardener agent that SSHes to the server via Tailscale, applies the hardening baseline, and writes the finalized server record to vault.

---

## 1.0 Security Baseline

**Status:** PENDING

**Dimensions:**
- 1.1 PENDING Configure UFW: deny all inbound except Tailscale, allow outbound to required endpoints
- 1.2 PENDING Install and configure fail2ban
- 1.3 PENDING Apply Debian security updates (`unattended-upgrades`)
- 1.4 PENDING Disable root login, enforce key-only auth
- 1.5 PENDING Verify KVM is available (`/dev/kvm` accessible) — fail early if not

---

## 2.0 Verification

**Status:** PENDING

**Dimensions:**
- 2.1 PENDING Run hardening audit (check UFW rules, fail2ban status, KVM access)
- 2.2 PENDING Record audit results in vault item

---

## 3.0 Vault Finalization

**Status:** PENDING

Update the vault item (created by M1_003) with the post-hardening state. This is the canonical record downstream agents read.

**Dimensions:**
- 3.1 PENDING Update 1Password item `zombie-{env}-worker-{name}` with: `hardened-at`, KVM status, UFW confirmation
- 3.2 PENDING Required fields must all be present before marking complete: `ssh-private-key`, `hostname`, `tailscale-ip`, `provider`, `provider-server-id`, `region`, `provisioned-at`, `hardened-at`
- 3.3 PENDING Validate all fields present; fail loudly if any are missing

---

## 4.0 Acceptance Criteria

- [ ] 4.1 UFW active with correct rules
- [ ] 4.2 fail2ban running
- [ ] 4.3 KVM accessible to non-root user
- [ ] 4.4 Vault item fully populated and readable by downstream agents (autoworkerstandup, autoupgrader)
- [ ] 4.5 Handoff to autoworkerstandup (M3_002) triggered
