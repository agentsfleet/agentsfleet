Implement M4_008: Sandbox Resource Governance (Firecracker microVMs)

Read docs/spec/v1/M4_008_SANDBOX_RESOURCE_GOVERNANCE.md fully — it is the source of truth.

Context: All dependencies are DONE (M4_005 events/observability, M4_007 runtime env contract). The M9 scoring chain is complete — the resource efficiency axis (10% weight) in M9_001 is currently stubbed at 50 and must now be wired to real VM metrics from this work.

Implement in this order:

1. Runner binary (spec §4.0) — thin Zig binary for inside the VM. Reads payload from virtio-blk, runs NullClaw agent stages, writes artifacts to output block device, communicates completion/failure via
vsock, enforces internal wall-clock timeout as secondary kill switch.
2. VM lifecycle manager (spec §1.0) — in the worker: define VM payload format (spec, worktree tarball, agent config, credentials envelope), boot from pre-warmed Firecracker snapshot with resource caps
(memory_mb, vcpu_count, disk_mb), collect output (stdout/stderr + artifact extraction from overlay), teardown with hard timeout kill.
3. Resource cap configuration (spec §2.0) — ServeConfig fields: SANDBOX_MEMORY_MB (default 512), SANDBOX_VCPU_COUNT (default 1), SANDBOX_DISK_MB (default 1024). Per-workspace overrides. Admission control:
check host resources before claiming work. Expose metrics: sandbox_memory_mb_allocated, sandbox_vcpu_allocated, sandbox_boots_total, sandbox_oom_kills_total.
4. Network egress policy (spec §3.0) — iptables/nftables rules on tap device per VM. Allowlist: LLM provider endpoints, GitHub API, control plane callback. Log blocked egress as security events.
5. Wire M9_001 resource efficiency axis — replace the hardcoded 50 stub with real metrics from VM execution (memory high-water mark, CPU utilization, disk usage vs caps). The scoring function signature
should not change — just the data source.
6. Verification units (spec §5.0) — integration tests: VM boot + echo agent, OOM kill + graceful failure, disk cap + I/O error, timeout + force kill + BLOCKED transition.

Acceptance: agent execution runs inside Firecracker VM (not on host), resource caps enforced at VM level, network egress restricted to allowlist, worker handles VM crashes/OOM/timeouts gracefully,
resource efficiency axis in M9_001 reads real VM metrics, demo evidence with /metrics showing sandbox gauges.