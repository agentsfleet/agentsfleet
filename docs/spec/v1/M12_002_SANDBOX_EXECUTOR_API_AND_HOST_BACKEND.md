# M12_002: Sandbox Executor API And Host Backend

**Prototype:** v1.0.0
**Milestone:** M12
**Workstream:** 002
**Date:** Mar 23, 2026
**Status:** PENDING
**Priority:** P0 — required to turn the sandbox direction into an implementable runtime boundary
**Batch:** B1 — foundational runtime split
**Depends on:** INIT_M4_008 (direction), M4_005 (observability/config), M4_007 (runtime env contract)

---

## 1.0 Problem

**Status:** PENDING

The current host-sandbox work began as a worker-local shell-wrapper design. That is too weak as the long-term execution boundary:
- the worker retains too much blast radius for timeout/OOM/cancellation failures
- sandbox lifecycle is not authoritative in one runtime
- future Firecracker migration would require another control-path rewrite

The worker needs a stable executor API. The first backend should be a local Unix-socket sidecar that embeds NullClaw and owns host-level Linux sandboxing.

**Dimensions:**
- 1.1 PENDING Define worker-to-executor responsibilities and ownership boundaries
- 1.2 PENDING Define stage-boundary durability and non-goals for mid-session survival
- 1.3 PENDING Keep the API backend-neutral so Firecracker can replace the host backend later

---

## 2.0 Executor API

**Status:** PENDING

Define the worker-facing contract for dangerous execution.

**Dimensions:**
- 2.1 PENDING Typed lifecycle RPCs: `CreateExecution`, `StartStage`, `StreamEvents`, `CancelExecution`, `GetUsage`, `DestroyExecution`
- 2.2 PENDING Correlation contract includes `trace_id`, `run_id`, `workspace_id`, `stage_id`, `role_id`, `skill_id`
- 2.3 PENDING Lease/heartbeat model so executor cancels orphaned runs when worker disappears
- 2.4 PENDING Explicit failure classes: startup posture, policy deny, timeout kill, OOM/resource kill, executor crash, transport loss

---

## 3.0 Local Sandbox Executor Sidecar

**Status:** PENDING

Ship the first executor implementation as a local Unix-socket gRPC sidecar.

**Dimensions:**
- 3.1 PENDING `sandbox-executor` process starts locally and serves the executor API over a Unix socket
- 3.2 PENDING Sidecar embeds NullClaw and owns the dynamic stage execution path
- 3.3 PENDING Worker no longer assumes dangerous agent execution lives in its own process boundary
- 3.4 PENDING Upgrade/restart semantics documented and surfaced honestly: stage-boundary retry, no mid-token continuity claim

---

## 4.0 Host Backend: bubblewrap + Landlock + Resource Governance

**Status:** PENDING

The first executor backend is still host-level Linux sandboxing. Keep the useful pieces from the original M4_008 design and discard the worker-local assumptions.

**Dimensions:**
- 4.1 PENDING Bubblewrap namespace policy owned by executor, not worker-side shell wrapper glue
- 4.2 PENDING Real Landlock filesystem policy: workspace RW, explicit readonly system paths, deny by default elsewhere
- 4.3 PENDING Resource governance: cgroups v2 or transient systemd scope for memory, CPU, and disk-write limits
- 4.4 PENDING Network policy: explicit allowlist/egress restriction owned by executor backend

---

## 5.0 Runtime Config And Observability

**Status:** PENDING

Expose the executor boundary explicitly in config, metrics, logs, and errors.

**Dimensions:**
- 5.1 PENDING Config keys for executor address, startup timeout, lease timeout, sandbox backend, and host resource caps
- 5.2 PENDING Metrics for executor sessions, executor failures, OOM kills, memory allocation, and CPU throttling
- 5.3 PENDING Structured executor/sandbox logs remain OTLP-friendly and trace-correlated
- 5.4 PENDING Error code coverage aligns with sandbox and executor failure classes

---

## 6.0 Verification Units

**Status:** PENDING

Linux-only proof is required before this work can be moved to `docs/done/v1/`.

**Dimensions:**
- 6.1 PENDING Integration test: worker reaches executor over the real transport and gets stage output back
- 6.2 PENDING Integration test: timeout path kills execution through the real executor boundary
- 6.3 PENDING Integration test: writes outside workspace are denied by Landlock policy
- 6.4 PENDING Integration test: memory or CPU cap violation is surfaced as a typed executor/sandbox failure

---

## 7.0 Acceptance Criteria

**Status:** PENDING

- [ ] 7.1 Worker drives execution only through the executor API
- [ ] 7.2 Local sidecar implementation exists and is the first concrete backend
- [ ] 7.3 Host backend enforces bubblewrap + Landlock + resource governance on Linux
- [ ] 7.4 Failure, cancellation, and restart semantics are explicit and observable
- [ ] 7.5 Metrics/logs/error codes reflect executor and sandbox failures without ambiguity
- [ ] 7.6 Linux verification proves the boundary works end-to-end

---

## 8.0 Out of Scope

- Firecracker implementation itself
- Mid-session live migration
- Closed-loop harness auto-approval redesign
- Pretending `M12_001` is done without trace/doc completion evidence
