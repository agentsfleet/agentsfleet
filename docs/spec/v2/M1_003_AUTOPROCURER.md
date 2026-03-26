# M1_003: Autoprocurer

**Version:** v2
**Milestone:** M1
**Workstream:** 003
**Date:** Mar 21, 2026
**Status:** PENDING
**Priority:** P1
**Depends on:** None (OVHCloud is the sole provider; no provider abstraction required)
**Batch:** B1

---

## Problem

Server procurement is manual (M4_001 playbook). The human buys a server from OVHCloud, records the IP and credentials, and hands off to the agent pipeline. This is the last human bottleneck in the scaling path.

## Decision

Build an autoprocurer agent that provisions servers from OVHCloud on demand, writes the server's IP and credentials to vault, and hands off to autoprovisioner (M2_001).

Provider abstraction (multi-vendor plugin system, YAML playbook, autoreviewer) is deferred — OVHCloud is the only approved provider today, and the abstraction cost is not justified until a second provider is actually needed.

---

## 1.0 Procurement Trigger

**Status:** PENDING

**Dimensions:**
- 1.1 PENDING Receive scaling trigger (manual operator signal or future autoforecaster recommendation)
- 1.2 PENDING Select OVHCloud region and machine type: prefer `has_kvm: true`, lowest cost that meets spec (KVM required for Firecracker)

---

## 2.0 OVHCloud Provisioning

**Status:** PENDING

**Dimensions:**
- 2.1 PENDING Call OVHCloud API to order server: `{ region, machine_type, os, hostname }`
- 2.2 PENDING Poll OVHCloud API until server is `ready` or timeout
- 2.3 PENDING On success: extract `{ ip, root_password }` from OVHCloud response
- 2.4 PENDING On failure: alert operator; do not proceed

---

## 3.0 Vault Handoff

**Status:** PENDING

**Dimensions:**
- 3.1 PENDING Create 1Password item `zombie-{env}-worker-{name}` in `ZMB_CD_PROD` (or `ZMB_CD_DEV`)
- 3.2 PENDING Required fields at creation: `ip`, `root-password` (initial credential), `hostname`, `provider=ovhcloud`, `provider-server-id`, `region`, `provisioned-at`
- 3.3 PENDING Trigger autoprovisioner (M2_001) — reads from this vault item

---

## 4.0 Decision Logging

**Status:** PENDING

**Dimensions:**
- 4.1 PENDING Record provisioning duration and OVHCloud server ID in vault item
- 4.2 PENDING Discord notification on successful procurement

---

## 5.0 Acceptance Criteria

- [ ] 5.1 Server provisioned without human intervention
- [ ] 5.2 Vault item created with all required fields
- [ ] 5.3 Handoff to autoprovisioner triggered automatically
- [ ] 5.4 Operator alerted on failure
