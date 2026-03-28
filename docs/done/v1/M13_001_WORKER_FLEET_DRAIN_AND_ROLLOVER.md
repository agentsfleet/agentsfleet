# M13_001: Worker Fleet Drain and Rolling Deployment

**Prototype:** v1.0.0
**Milestone:** M13
**Workstream:** 001
**Date:** Mar 27, 2026
**Status:** DONE
**Priority:** P1 — Required for multi-worker fleet deploys; single-worker DEV operates without drain today
**Batch:** B1
**Depends on:** M4_001 (Worker Bootstrap), M12_003 (Executor NullClaw Invocation)

---

## 1.0 Worker Graceful Drain

**Status:** DONE

Worker receives SIGTERM and enters drain mode: stops claiming new jobs from the Redis stream consumer group, waits for all in-flight executions to complete (up to a configurable timeout), and reports drain progress via structured log lines. systemd `TimeoutStopSec=300` provides a 5-minute window for in-flight work before SIGKILL.

**Dimensions:**
- 1.1 ✅ SIGTERM handler installs drain signal, worker stops calling XREADGROUP
- 1.2 ✅ Drain state machine (running -> draining -> drained) with atomic transitions
- 1.3 ✅ Configurable drain timeout; after timeout, worker exits even if jobs remain (logs warning)
- 1.4 ✅ Structured log lines for drain start, in-flight count, drain complete/timeout

---

## 2.0 Executor Drain Coordination

**Status:** DONE

Executor already handles SIGTERM gracefully (stops JSON-RPC server, stops lease manager, joins threads). The critical ordering constraint is that the worker must drain BEFORE the executor stops. systemd `Requires=zombied-executor.service` on the worker unit means stopping the executor triggers worker stop first (reverse dependency). If the executor crashes mid-drain, the worker classifies the failure as `transport_loss` on the Unix socket and existing retry logic handles it.

**Dimensions:**
- 2.1 ✅ systemd stop ordering verified: worker stops before executor due to Requires= reverse dependency
- 2.2 ✅ Executor crash during worker drain produces `transport_loss`, not undefined behavior
- 2.3 ✅ Lease manager reaps orphaned sessions after executor restart (existing 30s timeout)

---

## 3.0 Fleet Rolling Deployment

**Status:** DONE

Worker hosts stored as a JSON array in a GitHub Actions variable (`PROD_WORKER_HOSTS`). CI deploys sequentially, one host at a time. Per-host sequence: drain worker, deploy executor binary, deploy worker binary, verify healthy, proceed to next host. Canary mode deploys the first host, pauses for manual approval via GitHub Actions environment protection (`production-fleet`), then continues the fleet. Discord notification per host is already wired in `deploy/baremetal/deploy.sh`.

**Dimensions:**
- 3.1 ✅ Host list config as GitHub variable (JSON array of SSH targets)
- 3.2 ✅ Sequential deploy loop (shell loop in CI, one host at a time)
- 3.3 ✅ Canary gate: first host deploys, GitHub environment approval pauses before fleet continues
- 3.4 ✅ Fleet health summary posted to Discord after all hosts complete

---

## 4.0 Acceptance Criteria

**Status:** DONE

- [x] 4.1 Worker handles SIGTERM by draining (no new jobs claimed, waits for in-flight)
- [x] 4.2 systemd stop ordering: worker drains before executor stops
- [x] 4.3 Fleet deploy rolls one host at a time with health verification
- [x] 4.4 In-flight runs survive during rolling deploy (not killed prematurely)

---

## 5.0 Out of Scope

- Mid-session migration (explicitly not supported per architecture: v1 is stage-boundary durability)
- Auto-scaling / dynamic fleet membership
- Blue-green deployment (separate infrastructure concern)
- Firecracker backend changes (v2)
