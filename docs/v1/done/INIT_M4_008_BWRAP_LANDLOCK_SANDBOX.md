# INIT_M4_008: Bubblewrap + Landlock Sandbox

**Prototype:** v1.0.0
**Milestone:** M4
**Workstream:** 008
**Date:** Mar 23, 2026
**Status:** DONE
**Priority:** P0 — initialization/split record for executor-based sandboxing
**Batch:** B4 — needs M4_005
**Depends on:** M4_005 (Events, Observability, Config), M4_007 (Runtime Env Contract)
**Supersedes:** direct worker-hosted shell-wrapper-only sandboxing
**Follow-on Spec:** `docs/spec/v1/M12_002_SANDBOX_EXECUTOR_API_AND_HOST_BACKEND.md`

---

## 1.0 Problem

**Status:** DONE

NullClaw agent executions need per-run resource boundaries. A single runaway agent can exhaust memory (OOM kills the worker), fill disk (blocks all runs), or pin CPU (starves concurrent runs). Landlock alone provides filesystem path restriction but zero resource capping.

**Dimensions:**
- 1.1 DONE Captured the v1 sandbox problem as execution isolation plus resource governance
- 1.2 DONE Recorded that process-group kill-switch alone is insufficient as the long-term boundary
- 1.3 DONE Recorded that stage-boundary durability is acceptable for v1

---

## 2.0 Initial Direction

**Status:** DONE

The original direction was host-level sandboxing with bubblewrap, Landlock, and cgroups v2. During implementation review, the architecture tightened:

- worker orchestration and dangerous agent execution must be separate runtime boundaries
- the worker should talk to a typed executor API
- the first executor implementation should be a local Unix-socket sidecar
- the sidecar should embed NullClaw and own Linux sandbox enforcement
- Firecracker remains the later backend behind the same executor contract

**Dimensions:**
- 2.1 DONE Kept bubblewrap + Landlock + cgroup/resource governance as the host-backend target
- 2.2 DONE Removed the assumption that worker should directly own all sandbox enforcement
- 2.3 DONE Split implementation planning into a new executor-focused spec
- 2.4 DONE Kept Firecracker out of this milestone and behind a future backend swap

---

## 3.0 What This INIT Spec Closes

**Status:** DONE

This file is complete as an initialization record because it no longer claims direct implementation closure. It closes the old planning frame and hands implementation to the new spec.

**Dimensions:**
- 3.1 DONE Archived the original direct-worker sandbox framing
- 3.2 DONE Recorded the executor-sidecar decision
- 3.3 DONE Linked the follow-on implementation spec
- 3.4 DONE Avoided a false claim that Linux enforcement is already complete

---

## 4.0 Acceptance Criteria

**Status:** DONE

- [x] 4.1 Direct-worker sandbox plan is retired without pretending implementation is finished
- [x] 4.2 Executor-sidecar direction is recorded as the replacement architecture
- [x] 4.3 Actual implementation work is tracked in `M12_002`

---

## 5.0 Out of Scope

- Claiming Linux sandbox enforcement is already complete
- Claiming observability work is complete without verification evidence
- Firecracker implementation
- Mid-session live migration
