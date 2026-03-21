# M4_008: Bubblewrap + Landlock Sandbox

**Prototype:** v1.0.0
**Milestone:** M4
**Workstream:** 008
**Date:** Mar 21, 2026
**Status:** PENDING
**Priority:** P0 — required before multi-tenant production
**Batch:** B4 — needs M4_005
**Depends on:** M4_005 (Events, Observability, Config), M4_007 (Runtime Env Contract)
**Supersedes:** Firecracker microVM approach (moved to `docs/spec/v2/M3_001_FIRECRACKER_SANDBOX_RESOURCE_GOVERNANCE.md`)

---

## Problem

NullClaw agent executions need per-run resource boundaries. A single runaway agent can exhaust memory (OOM kills the worker), fill disk (blocks all runs), or pin CPU (starves concurrent runs). Landlock alone provides filesystem path restriction but zero resource capping.

## Decision: bubblewrap + Landlock + cgroups v2

NullClaw's standard sandbox path is bubblewrap as primary with Landlock layered on top. Composable primitives, no profiles, no daemons.

**Why bwrap + Landlock:**
- NullClaw already has `.sandbox = .{ .mode = .bubblewrap }` — zero new dependencies
- bwrap provides PID/mount/network namespace isolation — explicit, deterministic, minimal surface
- Landlock layers filesystem access control on top — unprivileged, kernel-enforced path restriction
- cgroups v2 (systemd-managed) provides hard memory ceiling, CPU quota, and I/O limits per execution
- Sub-millisecond startup — no VM boot latency

**Why not firejail:**
- Large attack surface, historically brittle
- Profile-driven — convenience over correctness
- Not the design center for NullClaw; exists only as a last-resort fallback

**Why not Docker:**
- Daemon-dependent, heavyweight
- Not a sandbox primitive — it's a container system
- Overkill for per-execution isolation

**What this does NOT solve (deferred to v2 Firecracker):**
- Shared kernel — agent code shares the host kernel (acceptable for v1 with trusted agent configs)
- No filesystem snapshot/restore — workspace cleanup is rm-based, not immutable rootfs overlay
- Network isolation is namespace-based, not tap-device-based — simpler but less granular

**What changes in the worker:**
- Worker configures a cgroup v2 scope per execution (memory.max, cpu.max, io.max)
- Worker invokes NullClaw with bwrap sandbox mode + Landlock filesystem policy
- Worker monitors cgroup events (memory.events for OOM, cpu.stat for throttling)
- On OOM or timeout, worker kills the cgroup and transitions run to BLOCKED

---

## 1.0 cgroup v2 Scope Manager

**Status:** PENDING

Implement per-execution cgroup v2 lifecycle in the worker.

**Dimensions:**
- 1.1 PENDING Create transient systemd scope per execution: `zombied-run-{run_id}.scope`
- 1.2 PENDING Set `memory.max` from config (default 512M), `memory.swap.max=0`
- 1.3 PENDING Set `cpu.max` from config (default 100000 100000 = 1 vCPU equivalent)
- 1.4 PENDING Set `io.max` for workspace block device (default 50M write)
- 1.5 PENDING Teardown scope on execution complete or timeout (hard kill via `systemctl kill`)

---

## 2.0 NullClaw Sandbox Integration

**Status:** PENDING

Wire the worker to invoke NullClaw with bubblewrap + Landlock inside the cgroup scope.

**Dimensions:**
- 2.1 PENDING Pass `.sandbox = .{ .mode = .bubblewrap, .allow_paths, .allow_net }` to NullClaw agent.run
- 2.2 PENDING Landlock ruleset: workspace read-write, `/usr`, `/lib`, `/etc/resolv.conf` read-only, everything else denied
- 2.3 PENDING Bind-mount workspace read-write via bwrap, everything else read-only or hidden
- 2.4 PENDING Network namespace: allow only control-plane callback + LLM provider endpoints (allowlist from config)
- 2.5 PENDING Worker startup preflight: verify bwrap binary exists, Landlock supported (kernel ≥5.13), cgroups v2 available; fail hard if any missing

---

## 3.0 Resource Cap Configuration

**Status:** PENDING

Expose per-execution resource limits as configuration.

**Dimensions:**
- 3.1 PENDING Add resource cap fields to ServeConfig: `SANDBOX_MEMORY_MB` (default 512), `SANDBOX_CPU_QUOTA` (default 100000), `SANDBOX_DISK_WRITE_MB` (default 1024)
- 3.2 PENDING Add per-workspace resource override in workspace settings (optional, falls back to global)
- 3.3 PENDING Admission control: worker checks host memory watermark and disk free before claiming work
- 3.4 PENDING Expose metrics: `sandbox_memory_mb_allocated`, `sandbox_oom_kills_total`, `sandbox_cpu_throttled_seconds_total`

---

## 4.0 Verification Units

**Status:** PENDING

**Dimensions:**
- 4.1 PENDING Integration test: bwrap + Landlock sandbox runs echo agent, returns output within resource caps
- 4.2 PENDING Integration test: execution exceeding memory cap triggers OOM, worker reports failure gracefully
- 4.3 PENDING Integration test: execution exceeding timeout is killed via cgroup, run transitions to BLOCKED
- 4.4 PENDING Integration test: Landlock denies access outside workspace (write to /tmp fails)
- 4.5 PENDING Preflight check: worker startup verifies bwrap binary, Landlock support, cgroups v2

---

## 5.0 Acceptance Criteria

**Status:** PENDING

- [ ] 5.1 Agent execution runs inside bwrap namespace with Landlock filesystem policy and cgroup v2 resource caps
- [ ] 5.2 Memory, CPU, and disk write caps enforced at cgroup level
- [ ] 5.3 Landlock denies filesystem access outside workspace and explicitly allowed paths
- [ ] 5.4 Network restricted to allowlisted endpoints via network namespace
- [ ] 5.5 Worker gracefully handles OOM kills and timeouts
- [ ] 5.6 Resource metrics visible in /metrics endpoint

---

## 6.0 Out of Scope (deferred to v2)

- Firecracker microVMs (see `docs/spec/v2/M3_001_FIRECRACKER_SANDBOX_RESOURCE_GOVERNANCE.md`)
- Immutable rootfs with overlay filesystem
- GPU passthrough
- Per-execution network tap devices with iptables egress rules
- VM snapshot pool warming
- firejail support (not the design center; bwrap + Landlock is strictly better)
