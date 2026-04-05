# M21_002: Instant Interrupt Delivery via Executor IPC

**Prototype:** v1.0.0
**Milestone:** M21
**Workstream:** 002
**Date:** Apr 05, 2026
**Status:** PENDING
**Priority:** P0 — Queued-only interrupts force users to wait up to 60s (full gate duration) before the agent sees a steer; tokens burn on the wrong path the entire time. Financial and DX impact.
**Batch:** B1
**Depends on:** M21_001 (interrupt API, gate loop polling, Redis key, SSE ack — all shipped in PR #152)

---

## Overview

M21_001 shipped queued interrupt delivery: the message sits in Redis until the gate loop polls it at the next checkpoint. If a gate command (`make build`, `make test`) takes 60 seconds, the user waits 60 seconds while the agent burns tokens on the wrong path.

This workstream adds **instant delivery**: the interrupt handler looks up the active executor session for the run and injects the message directly via `InjectUserMessage` IPC. The agent sees it mid-turn — within seconds, not minutes. Falls back to queued if IPC fails (TOCTOU: executor terminated between DB read and IPC call).

**User impact without this fix:**
- User sends "stop, wrong branch" while `make build` runs for 5 minutes → waits 5 minutes
- Token burn continues on wrong path until gate checkpoint
- DX feels like email (eventual) not chat (real-time)
- Admin cannot distinguish queued vs instant delivery in audit trail (`INTERRUPT_DELIVERED` reason code is dead)

---

## 1.0 Schema: `active_execution_id` on `core.runs`

**Status:** PENDING

Add a nullable column to `core.runs` that tracks the currently active executor session. Set by the worker when it creates an execution, cleared on completion/abort/crash.

**Dimensions:**
- 1.1 PENDING Add `active_execution_id TEXT` (nullable) to `core.runs` in `schema/001_initial.sql` (tear-down rebuild, no ALTER)
- 1.2 PENDING Add `executor_socket_addr TEXT` (nullable) to `core.runs` — the worker's IPC socket address so the API handler can reach the executor. Without this, the API process cannot call `InjectUserMessage` because it doesn't know which socket to connect to.

**Design note:** The API server and worker run in separate processes. The API handler needs to reach the executor via its Unix socket. Two options:
- (A) Store the socket path in the DB alongside execution_id — API handler connects directly
- (B) API handler writes to Redis, worker polls and calls IPC — this is just queued mode with extra steps

Option (A) is the only path to true instant delivery. The socket path is worker-local, so this only works when API and worker are co-located (same host or shared filesystem). For distributed deployments, a worker-side HTTP sidecar or Redis pub/sub command channel would be needed (out of scope for this workstream).

---

## 2.0 Worker: Set/Clear `active_execution_id`

**Status:** PENDING

The worker pipeline sets the column when it creates an executor session, clears it when the session ends (success, failure, crash, cancel, abort).

**Dimensions:**
- 2.1 PENDING After `executor.createExecution()` succeeds in `worker_stage_executor.zig`, UPDATE `core.runs SET active_execution_id = $1, executor_socket_addr = $2 WHERE run_id = $3`
- 2.2 PENDING On execution completion (success or failure), UPDATE `core.runs SET active_execution_id = NULL, executor_socket_addr = NULL WHERE run_id = $1`
- 2.3 PENDING On cancel/abort/crash, same NULL clear — must be in a `defer` or error path to avoid stale execution_id after crash
- 2.4 PENDING Orphan recovery (`worker_claim.zig` reconciler): clear `active_execution_id` for runs that are orphaned (worker crashed without clearing)

---

## 3.0 Interrupt Handler: Instant IPC Path

**Status:** PENDING

When `mode: "instant"` is requested, the handler reads `active_execution_id` and `executor_socket_addr` from the run row. If both are non-null, it connects to the executor socket and calls `InjectUserMessage`. On success, logs `INTERRUPT_DELIVERED` and returns `mode: "instant"` in the response. On any failure (null execution_id, IPC error, TOCTOU), falls back to queued.

**Dimensions:**
- 3.1 PENDING Read `active_execution_id, executor_socket_addr` from `core.runs` in the same query that fetches `state, workspace_id`
- 3.2 PENDING If both non-null and `mode == "instant"`: create a short-lived `ExecutorClient`, connect to socket, call `injectUserMessage(execution_id, message)`
- 3.3 PENDING On IPC success: set `effective_mode = "instant"`, increment `metrics.incInterruptInstant()`, log `INTERRUPT_DELIVERED` in `run_transitions`
- 3.4 PENDING On IPC failure (connection refused, transport loss, execution not found): fall back to queued path (Redis SETEX), increment `metrics.incInterruptFallback()`, log warning with error detail
- 3.5 PENDING TOCTOU guard: the execution may terminate between the DB read and the IPC call. The fallback to queued handles this — the gate loop will pick it up if the executor already exited. Never silently drop.
- 3.6 PENDING Connection timeout: the IPC connect must have a short timeout (500ms) — user is waiting for the HTTP response. If connect takes longer, fall back to queued immediately.

---

## 4.0 Observability

**Status:** PENDING

Operators need to see: how often instant succeeds vs falls back, what the IPC latency is, and which runs have active executors.

**Dimensions:**
- 4.1 PENDING `metrics.incInterruptInstant()` — already defined, currently never incremented. Wire it to the success path in §3.3
- 4.2 PENDING `metrics.incInterruptFallback()` — already wired for all instant requests. Change to only increment on IPC *failure*, not on every instant request
- 4.3 PENDING New metric: `zombie_interrupt_ipc_duration_ms` histogram — measures the IPC round-trip time for instant delivery
- 4.4 PENDING Structured log: `interrupt.instant_delivered run_id={s} agent_id={s} ipc_ms={d}` on success
- 4.5 PENDING Structured log: `interrupt.instant_fallback run_id={s} agent_id={s} err={s}` on IPC failure with error class
- 4.6 PENDING Grafana dashboard: interrupt delivery mode breakdown (instant vs queued vs fallback) per workspace

---

## 5.0 Acceptance Criteria

**Status:** PENDING

- [ ] 5.1 User sends `mode: "instant"` while executor is active; message is injected mid-turn within 1s of POST; `interrupt_ack` SSE event shows `mode: "instant"`
- [ ] 5.2 User sends `mode: "instant"` but executor has already exited (TOCTOU); falls back to queued; `interrupt_ack` shows `mode: "queued"`; message delivered at next gate checkpoint
- [ ] 5.3 `run_transitions` row shows `INTERRUPT_DELIVERED` reason code when instant succeeds, `INTERRUPT_QUEUED` when it falls back
- [ ] 5.4 IPC connect timeout (500ms) does not block the HTTP response — user gets ack within 1s regardless
- [ ] 5.5 Orphan recovery clears stale `active_execution_id` for crashed workers
- [ ] 5.6 `zombie_interrupt_ipc_duration_ms` histogram visible in Prometheus/Grafana

---

## 6.0 Out of Scope

- Distributed instant delivery (API and worker on different hosts) — requires worker sidecar or command channel
- Rate limiting interrupts per run (v3 concern)
- Batching multiple interrupts into one IPC call
- Desktop/Voice UI (M21_001 §4, separate workstream)
