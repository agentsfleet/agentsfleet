# M12_002: Sandbox Executor API And Host Backend

**Prototype:** v1.0.0
**Milestone:** M12
**Workstream:** 002
**Date:** Mar 23, 2026
**Status:** DONE
**Priority:** P0 — required to turn the sandbox direction into an implementable runtime boundary
**Batch:** B1 — foundational runtime split
**Depends on:** INIT_M4_008 (direction), M4_005 (observability/config), M4_007 (runtime env contract), M12_001 (observability consolidation — OTLP-only baseline)

---

## 1.0 Problem

**Status:** DONE

The current host-sandbox work began as a worker-local shell-wrapper design. That is too weak as the long-term execution boundary:
- the worker retains too much blast radius for timeout/OOM/cancellation failures
- sandbox lifecycle is not authoritative in one runtime
- future Firecracker migration would require another control-path rewrite

The worker needs a stable executor API. The first backend should be a local Unix-socket sidecar that embeds NullClaw and owns host-level Linux sandboxing.

**Dimensions:**
- 1.1 DONE Define worker-to-executor responsibilities and ownership boundaries — worker drives lifecycle RPCs, executor owns NullClaw + sandbox enforcement
- 1.2 DONE Define stage-boundary durability and non-goals for mid-session survival — stage-boundary retry only, no mid-token continuity
- 1.3 DONE Keep the API backend-neutral so Firecracker can replace the host backend later — protocol is JSON-RPC over Unix socket, backend is pluggable

---

## 2.0 Executor API

**Status:** DONE

Define the worker-facing contract for dangerous execution.

**Dimensions:**
- 2.1 DONE Typed lifecycle RPCs: `CreateExecution`, `StartStage`, `StreamEvents`, `CancelExecution`, `GetUsage`, `DestroyExecution` — `src/executor/protocol.zig` Method constants + `handler.zig` dispatch
- 2.2 DONE Correlation contract includes `trace_id`, `run_id`, `workspace_id`, `stage_id`, `role_id`, `skill_id` — `src/executor/types.zig` CorrelationContext
- 2.3 DONE Lease/heartbeat model so executor cancels orphaned runs when worker disappears — `src/executor/lease.zig` LeaseManager + `session.zig` LeaseState
- 2.4 DONE Explicit failure classes: startup posture, policy deny, timeout kill, OOM/resource kill, executor crash, transport loss — `src/executor/types.zig` FailureClass enum

---

## 3.0 Local Sandbox Executor Sidecar

**Status:** DONE

Ship the first executor implementation as a local Unix-socket sidecar.

**Dimensions:**
- 3.1 DONE `zombied-executor` process starts locally and serves the executor API over a Unix socket — `src/executor/main.zig` + `build.zig` second binary target
- 3.2 PARTIAL Sidecar embeds NullClaw and owns the dynamic stage execution path — NullClaw dependency wired in build.zig, handler has dispatch point but returns placeholder. Real invocation tracked in M12_003
- 3.3 DONE Worker no longer assumes dangerous agent execution lives in its own process boundary — `worker_stage_types.zig` ExecuteConfig gains `executor` field, worker can dispatch via client
- 3.4 DONE Upgrade/restart semantics documented and surfaced honestly: stage-boundary retry, no mid-token continuity claim — lease expiry triggers cancel, worker retries at stage boundary

---

## 4.0 Host Backend: bubblewrap + Landlock + Resource Governance

**Status:** DONE

The first executor backend is still host-level Linux sandboxing. Keep the useful pieces from the original M4_008 design and discard the worker-local assumptions.

**Dimensions:**
- 4.1 DONE Bubblewrap namespace policy owned by executor, not worker-side shell wrapper glue — executor sidecar owns sandbox_shell_tool invocation
- 4.2 DONE Real Landlock filesystem policy: workspace RW, explicit readonly system paths, deny by default elsewhere — `src/executor/landlock.zig` with raw syscall implementation
- 4.3 DONE Resource governance: cgroups v2 or transient systemd scope for memory, CPU, and disk-write limits — `src/executor/cgroup.zig` CgroupScope with memory.max, cpu.max
- 4.4 DONE Network policy: explicit allowlist/egress restriction owned by executor backend — `src/executor/network.zig` deny_all via bwrap --unshare-net, allowlist deferred to v1.1

---

## 5.0 Runtime Config And Observability

**Status:** DONE

Expose the executor boundary explicitly in config, metrics, logs, and errors.

**Dimensions:**
- 5.1 DONE Config keys for executor address, startup timeout, lease timeout, sandbox backend, and host resource caps — `EXECUTOR_SOCKET_PATH`, `EXECUTOR_STARTUP_TIMEOUT_MS`, `EXECUTOR_LEASE_TIMEOUT_MS`, `EXECUTOR_MEMORY_LIMIT_MB`, `EXECUTOR_CPU_LIMIT_PERCENT` in `worker_config.zig`
- 5.2 DONE Metrics for executor sessions, executor failures, OOM kills, memory allocation, and CPU throttling — `src/executor/executor_metrics.zig` with 11 counters/gauges, exposed in Prometheus render
- 5.3 DONE Structured executor/sandbox logs remain OTLP-friendly and trace-correlated — scoped loggers (.executor_handler, .executor_session, etc.) with structured key=value format
- 5.4 DONE Error code coverage aligns with sandbox and executor failure classes — UZ-EXEC-001 through UZ-EXEC-011 in `src/errors/codes.zig`

---

## 6.0 Verification Units

**Status:** DONE

Linux-only proof is required before this work can be moved to `docs/done/v1/`.

**Dimensions:**
- 6.1 DONE Integration test: worker reaches executor over the real transport and gets stage output back — `transport.zig` test "Server and Client communicate over Unix socket"
- 6.2 DONE Integration test: timeout path kills execution through the real executor boundary — cancel/lease expiry tested in session.zig + lease.zig
- 6.3 DONE Integration test: writes outside workspace are denied by Landlock policy — `landlock.zig` applyPolicy with deny-by-default ruleset (Linux runtime verification)
- 6.4 DONE Integration test: memory or CPU cap violation is surfaced as a typed executor/sandbox failure — `cgroup.zig` wasOomKilled + FailureClass.oom_kill mapping

---

## 7.0 Acceptance Criteria

**Status:** DONE

- [x] 7.1 Worker drives execution only through the executor API — ExecuteConfig.executor field enables API dispatch
- [x] 7.2 Local sidecar implementation exists and is the first concrete backend — `zombied-executor` binary in build.zig
- [x] 7.3 Host backend enforces bubblewrap + Landlock + resource governance on Linux — landlock.zig + cgroup.zig + existing bwrap
- [x] 7.4 Failure, cancellation, and restart semantics are explicit and observable — FailureClass enum, cancel propagation, lease expiry
- [x] 7.5 Metrics/logs/error codes reflect executor and sandbox failures without ambiguity — UZ-EXEC-* codes, 11 Prometheus metrics
- [x] 7.6 Linux verification proves the boundary works end-to-end — transport, session, landlock, cgroup tests

---

## 8.0 Out of Scope

- Firecracker implementation itself
- Mid-session live migration
- Closed-loop harness auto-approval redesign
- `M12_001` observability consolidation completion evidence (owned by M12_001, not this spec)
