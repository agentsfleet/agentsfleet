# M3_001: Firecracker Sandbox Resource Governance

**Version:** v2
**Milestone:** M3
**Workstream:** 001
**Date:** Mar 21, 2026
**Status:** PENDING
**Priority:** P0 — required before multi-tenant production
**Depends on:** v1 M4_005 (Events, Observability, Config), v1 M4_007 (Runtime Env Contract)
**Supersedes:** v1 M4_008 (bubblewrap + Landlock — ships in v1, Firecracker replaces it in v2)
**Batch:** B7 — independent, can parallel with M3_002

---

## Problem

NullClaw agent executions run directly on the worker host with no resource boundaries. A single runaway agent can exhaust memory (OOM kills the worker), fill disk (blocks all runs), or pin CPU (starves concurrent runs). Landlock provides filesystem path restriction but zero resource capping.

This is not a hardening concern — it is a production blocker. Without per-execution resource governance, a single tenant's agent run can take down the entire worker fleet.

v1 ships with bubblewrap + Landlock + cgroups v2 (see `docs/spec/v1/M4_008_BWRAP_LANDLOCK_SANDBOX.md`). v2 replaces that with Firecracker microVMs for full kernel isolation.

## Decision: Firecracker microVMs

cgroups v2 alone solves resource capping but not execution isolation (network, filesystem view, kernel attack surface). For a multi-tenant agent platform where arbitrary LLM-generated code runs, the isolation boundary must be a VM, not a namespace.

**Why not cgroups v2 alone:**
- No network namespace isolation without additional CNI setup
- Shared kernel — agent code can exploit kernel vulnerabilities
- No filesystem snapshot/restore — dirty workspace state leaks between runs
- cgroups require root or delegated controllers — operational complexity equivalent to VMs

**Why Firecracker:**
- Sub-200ms boot from pre-warmed snapshots (measured by AWS, Fly.io, Hetzner users)
- Hard memory ceiling (balloon driver), vCPU pinning, virtio-blk disk limits — kernel-enforced
- Network isolation via tap device + iptables egress allowlist — no CNI
- Immutable rootfs + overlay workspace — clean slate per execution
- Already proven for multi-tenant code execution (Lambda, Fly Machines, Koyeb)

**What changes in the worker:**
- Worker becomes an orchestrator — it no longer runs NullClaw in-process
- Worker prepares a VM payload (spec, worktree snapshot, agent config, BYOK credentials)
- Worker boots a Firecracker VM, monitors it, collects output
- NullClaw runs inside the VM as a thin runner binary

---

## 1.0 Runner Binary

**Status:** PENDING

Thin Zig binary that runs inside the Firecracker VM. Receives payload, executes NullClaw, writes output. Implement first — everything else depends on having something to run inside the VM.

**Dimensions:**
- 1.1 PENDING Implement runner binary: reads payload from virtio-blk, runs NullClaw agent stages, writes artifacts to output block device
- 1.2 PENDING Runner communicates completion/failure via vsock to host worker
- 1.3 PENDING Runner enforces internal wall-clock timeout as a secondary kill switch

---

## 2.0 VM Lifecycle Manager

**Status:** PENDING

Implement the Firecracker VM boot/monitor/teardown lifecycle in the worker.

**Dimensions:**
- 2.1 PENDING Define VM payload format (spec, worktree tarball, agent config, credentials envelope)
- 2.2 PENDING Implement VM boot from pre-warmed snapshot with resource caps (memory_mb, vcpu_count, disk_mb from config)
- 2.3 PENDING Implement VM output collection (stdout/stderr capture, artifact extraction from overlay)
- 2.4 PENDING Implement VM teardown with hard timeout kill (run_timeout_ms enforced at VM level)

---

## 3.0 Resource Cap Configuration

**Status:** PENDING

Expose per-execution resource limits as configuration, with sane defaults and per-workspace overrides.

**Dimensions:**
- 3.1 PENDING Add resource cap fields to ServeConfig: SANDBOX_MEMORY_MB (default 512), SANDBOX_VCPU_COUNT (default 1), SANDBOX_DISK_MB (default 1024)
- 3.2 PENDING Add per-workspace resource override in workspace settings (optional, falls back to global)
- 3.3 PENDING Add admission control: worker checks available host resources before claiming work (memory watermark, disk free threshold)
- 3.4 PENDING Expose resource cap metrics: sandbox_memory_mb_allocated, sandbox_vcpu_allocated, sandbox_boots_total, sandbox_oom_kills_total

---

## 4.0 Network Egress Policy

**Status:** PENDING

Agent VMs must only reach approved endpoints. No open internet access.

**Dimensions:**
- 4.1 PENDING Define egress allowlist format (LLM provider endpoints, GitHub API, control plane callback)
- 4.2 PENDING Implement iptables/nftables rules on tap device per VM
- 4.3 PENDING Log blocked egress attempts as security events

---

## 5.0 M9_001 Resource Efficiency Axis

**Status:** PENDING

Wire real VM metrics into the M9_001 scoring engine resource efficiency axis (currently stubbed at 50).

**Dimensions:**
- 5.1 PENDING Replace hardcoded 50 stub with real metrics from VM execution (memory high-water mark, CPU utilization, disk usage vs caps)
- 5.2 PENDING Scoring function signature unchanged — only the data source changes

---

## 6.0 Verification Units

**Status:** PENDING

**Dimensions:**
- 6.1 PENDING Integration test: VM boots, executes echo agent, returns output within resource caps
- 6.2 PENDING Integration test: VM exceeding memory cap is OOM-killed and worker reports failure gracefully
- 6.3 PENDING Integration test: VM exceeding disk cap gets I/O error, worker reports failure
- 6.4 PENDING Integration test: VM exceeding timeout is force-killed, worker transitions run to BLOCKED

---

## 7.0 Acceptance Criteria

**Status:** PENDING

- [ ] 7.1 Agent execution runs inside Firecracker VM, not on worker host
- [ ] 7.2 Memory, CPU, disk, and timeout caps are enforced at VM level
- [ ] 7.3 Network egress restricted to allowlisted endpoints
- [ ] 7.4 Worker gracefully handles VM crashes, OOM kills, and timeouts
- [ ] 7.5 Resource efficiency axis in M9_001 reads real VM metrics
- [ ] 7.6 Demo evidence: run completes inside VM with resource metrics visible in /metrics

---

## 8.0 Implementation Order

1. Runner binary (§1.0) — build first, everything needs something to run inside the VM
2. VM lifecycle manager (§2.0) — boot/monitor/teardown
3. Resource cap configuration (§3.0) — ServeConfig fields + admission control + metrics
4. Network egress policy (§4.0) — iptables on tap device per VM
5. M9_001 resource efficiency axis (§5.0) — wire real metrics
6. Verification units (§6.0) — integration tests

---

## 9.0 Out of Scope

- VM snapshot pool warming and lifecycle management (separate ops workstream)
- GPU passthrough for model inference inside VMs
- Live migration of running VMs between hosts
- Custom rootfs images per workspace (single rootfs for v2)
