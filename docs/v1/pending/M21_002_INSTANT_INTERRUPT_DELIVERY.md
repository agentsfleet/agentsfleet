# M21_002: Instant Interrupt Delivery

**Prototype:** v1.0.0
**Milestone:** M21
**Workstream:** 002
**Date:** Apr 05, 2026
**Status:** PENDING
**Priority:** P0 — Queued-only interrupts force users to wait up to 60s (full gate duration) before the agent sees a steer; tokens burn on the wrong path the entire time. Financial and DX impact.
**Batch:** B1
**Depends on:** M21_001 (interrupt API, gate loop polling, Redis key, SSE ack — shipped in PR #152)

---

## Overview

M21_001 shipped queued interrupt delivery: the message sits in Redis until the gate loop polls it at the next checkpoint. If a gate command (`make build`, `make test`) takes 60 seconds, the user waits 60 seconds while the agent burns tokens on the wrong path.

This workstream eliminates that wait. The agent must see the interrupt within seconds, not minutes.

**User impact without this fix:**
- User sends "stop, wrong branch" while `make build` runs for 5 minutes → waits 5 minutes
- Token burn continues on wrong path until gate checkpoint
- DX feels like email (eventual) not chat (real-time)
- Admin cannot distinguish instant vs queued delivery in audit trail

---

## 1.0 Architecture Discovery: Why is the executor separate?

**Status:** PENDING

Before choosing a delivery mechanism, document the executor isolation decision and evaluate whether it constrains interrupt delivery.

**Dimensions:**
- 1.1 PENDING Document why the executor runs as a separate process (sandbox security: bubblewrap/landlock isolation, OOM kill containment, resource cgroup limits, untrusted agent code execution). Reference the original design decision and M-number.
- 1.2 PENDING Document the current IPC boundary: worker ↔ executor via Unix socket JSON-RPC. Methods: CreateExecution, StartStage, CancelExecution, DestroyExecution, GetUsage, Heartbeat, InjectUserMessage (added M21_001, no call site yet).
- 1.3 PENDING Evaluate: can the executor be co-located in-process for trusted workloads (no sandbox needed)? What would that unlock for interrupt latency? What security guarantees would be lost?
- 1.4 PENDING Decision record: keep executor separate, merge for trusted mode, or hybrid. Document the trade-off and commit to one path.

---

## 2.0 Delivery Mechanism Evaluation

**Status:** PENDING

Evaluate four options for reducing interrupt delivery latency. Pick one. The spec must justify the choice with latency, complexity, and failure mode analysis.

**Options:**

| Option | How | Latency | Complexity | Failure mode |
|--------|-----|---------|------------|-------------|
| A. Status quo (queued) | GETDEL at top of gate loop | Up to 60s | Already shipped | None — works today |
| B. Worker timer thread | Spawn a thread that polls Redis every 2s while gate command runs; on interrupt found, kill gate child + inject | ~2s | Low — one thread, same Redis pattern | Thread lifecycle; must not outlive gate command |
| C. Worker pub/sub listener | Worker SUBSCRIBE to `run:{id}:interrupt_signal`; on message, kill gate child + inject | <1s | Medium — new pub/sub subscription per run, must handle reconnect | Pub/sub disconnect during gate; message lost if worker not listening |
| D. API → executor direct IPC | Store `active_execution_id` + `executor_socket_addr` in DB; API handler connects to executor socket, calls InjectUserMessage | <500ms | High — cross-process socket, DB column, connection management, timeout | TOCTOU (executor dies between read and connect); API process needs filesystem access to socket |

**Dimensions:**
- 2.1 PENDING Evaluate Option B: timer thread that polls Redis every 2s during `executeGateCommand`. On interrupt found, send SIGTERM to gate child process, then inject message via `startStage`. Measure: how much does a 2s poll add to Redis load per active run?
- 2.2 PENDING Evaluate Option C: worker subscribes to interrupt pub/sub channel alongside the existing event pub/sub. On message, same kill+inject flow. Measure: pub/sub reconnect reliability under network partitions.
- 2.3 PENDING Evaluate Option D: API handler reads `active_execution_id` from DB, connects to executor socket. Measure: is the socket path accessible from the API process in dev (same host) and prod (container)?
- 2.4 PENDING Pick one option. Document why. Reject the others with specific reasons.

---

## 3.0 Implementation (depends on §2 decision)

**Status:** PENDING

Implement the chosen option. Dimensions TBD after §2.4 decision.

**Known dimensions regardless of option chosen:**
- 3.1 PENDING Wire `INTERRUPT_DELIVERED` reason code to the success path (currently dead — always logs `INTERRUPT_QUEUED`)
- 3.2 PENDING `effective_mode` in interrupt handler response reflects actual delivery: `"instant"` when message reached executor mid-turn, `"queued"` when fell back
- 3.3 PENDING Gate child process termination: when an interrupt arrives during a running gate command, the gate command must be killed (SIGTERM → SIGKILL after 5s) so the agent can absorb the steer immediately, not after the gate finishes

---

## 4.0 Observability

**Status:** PENDING

- 4.1 PENDING `metrics.incInterruptInstant()` — wire to success path (currently defined but never incremented)
- 4.2 PENDING `metrics.incInterruptFallback()` — only increment on actual IPC/delivery failure, not on every instant request
- 4.3 PENDING New histogram: `zombie_interrupt_delivery_latency_ms` — time from POST :interrupt to message reaching executor
- 4.4 PENDING Structured logs with `run_id`, `workspace_id`, `agent_id`, delivery mode, latency
- 4.5 PENDING Grafana panel: instant vs queued vs fallback breakdown per workspace

---

## 5.0 Acceptance Criteria

**Status:** PENDING

- [ ] 5.1 User sends `mode: "instant"` while a gate command is running; agent sees the message within 5s (not 60s)
- [ ] 5.2 Gate command is killed on interrupt; agent does not wait for it to finish
- [ ] 5.3 `run_transitions` shows `INTERRUPT_DELIVERED` on instant success, `INTERRUPT_QUEUED` on fallback
- [ ] 5.4 Fallback to queued works when executor is unavailable (TOCTOU, crash, etc.)
- [ ] 5.5 Token burn stops within 5s of interrupt (not 60s)
- [ ] 5.6 `zombie_interrupt_delivery_latency_ms` histogram visible in Prometheus

---

## 6.0 Out of Scope

- Desktop/Voice UI (M21_001 §4, separate workstream)
- Rate limiting interrupts per run
- Batching multiple interrupts into one delivery
- Distributed deployment (API and worker on different hosts with no shared filesystem)
