# M19_001: Infrastructure Automation Pipeline

**Prototype:** v1.0.0
**Milestone:** M19
**Workstream:** 001
**Date:** Mar 28, 2026
**Status:** PENDING
**Priority:** P2 — Full server lifecycle automation; unblocks fleet scaling without manual playbook steps
**Batch:** B5
**Depends on:** M7_001 (DEV Acceptance — deploy pipeline pattern established)

---

## 1.0 Autoprocurer

**Status:** PENDING

Automates OVHCloud server ordering via API. Human provides: cloud project ID, region preference, and server spec written to vault before triggering. Agent calls OVH API to order the server, polls until delivered, and writes the resulting server IP, root password, and order ID back to vault. The next stage (autoprovisioner) reads from that vault item and proceeds without human interaction.

**Dimensions:**
- 1.1 PENDING OVH API client: submit dedicated server order with `{ region, machine_type, os, hostname }`, poll order status until `DELIVERED` or timeout
- 1.2 PENDING Write server IP, root password, and order ID to 1Password vault item `zombie-{env}-worker-{name}` in `ZMB_CD_PROD` or `ZMB_CD_DEV`
- 1.3 PENDING Idempotency: before ordering, query OVH API for an existing server matching hostname and spec; return existing record if found
- 1.4 PENDING Failure handling: surface order timeout, OVH API errors, and quota exhaustion with structured error message naming the dependency and remediation step; do not proceed on failure

---

## 2.0 Autoprovisioner

**Status:** PENDING

Takes a delivered server (IP and root password from vault) and performs initial access setup: deploy SSH key, disable root password login, install Tailscale, join tailnet. Writes tailscale IP and SSH private key to vault so downstream stages have a complete, key-only access path.

**Dimensions:**
- 2.1 PENDING SSH to server using root password read from vault; generate Ed25519 deploy key pair; install public key in `authorized_keys`; store private key in vault at `zombie-{env}-worker-{name}/ssh-private-key`
- 2.2 PENDING Disable root password authentication and configure sshd hardening (`PasswordAuthentication no`, `PermitRootLogin prohibit-password`)
- 2.3 PENDING Install Tailscale; join tailnet using auth key read from vault; verify Tailscale IP is assigned
- 2.4 PENDING Write tailscale IP and updated SSH config to vault item; drop public IP access (firewall: deny all inbound except Tailscale)

---

## 3.0 Autohardener

**Status:** PENDING

Takes a provisioned server (tailscale IP from vault) and applies the security baseline: UFW firewall, fail2ban, unattended Debian security upgrades, and KVM verification. Writes hardened status and audit results back to vault so downstream stages can confirm the server is production-ready.

**Dimensions:**
- 3.1 PENDING Configure UFW: deny all inbound, allow Tailscale subnet, allow SSH from Tailscale only; verify ruleset matches expected policy before proceeding
- 3.2 PENDING Install and configure fail2ban for SSH brute-force protection; verify service is active
- 3.3 PENDING Enable `unattended-upgrades` for automatic security patches; apply any pending security updates at standup time
- 3.4 PENDING Verify KVM hardware virtualization is available (`/dev/kvm` accessible); fail loudly with remediation message if not present — KVM is required for Firecracker executor

---

## 4.0 Autoworkerstandup

**Status:** PENDING

Takes a hardened server and deploys the `zombied` worker and executor binaries. Copies systemd units and writes `.env` from vault. Starts services, runs health verification (Redis connectivity, Postgres connectivity, executor socket ready), and sends a Discord readiness notification with the first health check result.

**Dimensions:**
- 4.1 PENDING SCP `zombied` binaries and systemd unit files to server via Tailscale SSH using credentials from vault; read latest release tag from GitHub before download
- 4.2 PENDING Write `.env` to `/etc/default/zombied-worker` from vault fields (DB URL, Redis URL, encryption key, etc.); set file permissions to `0600`
- 4.3 PENDING Enable and start executor and worker systemd services; verify health endpoints respond, Redis consumer group joined, and Postgres connectivity confirmed
- 4.4 PENDING Send Discord notification with server name, region, provider, Tailscale IP, and time elapsed from procurement to ready; include first health check result

---

## 5.0 Acceptance Criteria

**Status:** PENDING

- [ ] 5.1 Server is ordered from OVHCloud without human intervention; vault item created with IP, root password, and order ID
- [ ] 5.2 SSH access works via Tailscale IP using deploy key only; public IP SSH is blocked
- [ ] 5.3 UFW active with correct rules, fail2ban running, unattended-upgrades enabled, KVM accessible
- [ ] 5.4 Worker process running and joined Redis consumer group; health endpoints pass for Redis, Postgres, and executor socket
- [ ] 5.5 Discord notification sent on successful standup
- [ ] 5.6 Each stage is idempotent: re-running after a partial failure produces the same end state without side effects
- [ ] 5.7 Every stage reads inputs from vault and writes outputs to vault; no credentials passed as arguments or environment variables between stages

---

## 6.0 Out of Scope

- Firecracker microVM setup and configuration (separate workstream)
- Multi-cloud provider support or provider abstraction layer
- Auto-scaling based on workload metrics (autoforecaster)
- Server decommission and vault cleanup
- Server migration between regions

