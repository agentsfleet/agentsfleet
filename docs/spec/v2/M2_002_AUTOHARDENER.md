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

Provisioned servers need a security baseline before running workloads: firewall rules, fail2ban, Debian security updates, and KVM verification (required for Firecracker in M3_001).

## Decision

Build an autohardener agent that SSHes to the server via Tailscale and applies the hardening baseline.

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

## 3.0 Acceptance Criteria

- [ ] 3.1 UFW active with correct rules
- [ ] 3.2 fail2ban running
- [ ] 3.3 KVM accessible to non-root user
- [ ] 3.4 Handoff to autovaultsaver triggered
