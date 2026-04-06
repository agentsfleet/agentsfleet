# M21_002: Instant Interrupt Delivery

**Prototype:** v1.0.0
**Milestone:** M21
**Workstream:** 002
**Date:** Apr 05, 2026
**Status:** IN_PROGRESS
**Priority:** P0 — Queued-only interrupts force users to wait up to 60s (full gate duration) before the agent sees a steer; tokens burn on the wrong path the entire time. Financial and DX impact.
**Batch:** B1
**Branch:** feat/m21-instant-interrupt
**Depends on:** M21_001 (interrupt API, gate loop polling, Redis key, SSE ack — shipped in PR #152)

---

## Overview

**Goal (testable):** User sends `mode: "instant"` interrupt while a gate command runs; agent sees the message within 5s, gate command is killed, and token burn stops — not after 60s at the next checkpoint.

**Problem:** M21_001 shipped queued interrupt delivery: the message sits in Redis until the gate loop polls it at the next checkpoint. If a gate command (`make build`, `make test`) takes 60 seconds, the user waits 60 seconds while the agent burns tokens on the wrong path. DX feels like email (eventual) not chat (real-time). Admin cannot distinguish instant vs queued delivery in audit trail.

**Solution summary:** Add a delivery mechanism (timer thread, pub/sub, or direct IPC — decided in §2.0) that detects interrupts while gate commands are running, kills the gate child process, and injects the user message into the executor within seconds. Wire `INTERRUPT_DELIVERED` reason code, `effective_mode` response field, and latency histogram for observability. Fall back to queued delivery when the instant path fails.

---

## 1.0 Architecture Discovery

**Status:** PENDING

Before choosing a delivery mechanism, document the executor isolation decision and evaluate whether it constrains interrupt delivery.

**Dimensions (test blueprints):**
- 1.1 PENDING
  - target: `docs/architecture/executor-isolation.md` (or equivalent design doc)
  - input: current executor process model (bubblewrap/landlock isolation, OOM kill containment, resource cgroup limits, untrusted agent code execution)
  - expected: written decision record documenting why executor runs as separate process, referencing original design decision and M-number
  - test_type: contract
- 1.2 PENDING
  - target: `worker ↔ executor IPC boundary`
  - input: current Unix socket JSON-RPC methods
  - expected: documented method list: CreateExecution, StartStage, CancelExecution, DestroyExecution, GetUsage, Heartbeat, InjectUserMessage (added M21_001, no call site yet)
  - test_type: contract
- 1.3 PENDING
  - target: executor co-location evaluation
  - input: trusted workload scenario (no sandbox needed)
  - expected: written analysis of latency gains vs security guarantees lost if executor is co-located in-process
  - test_type: contract
- 1.4 PENDING
  - target: decision record
  - input: options (keep separate, merge for trusted mode, hybrid)
  - expected: committed decision with trade-off analysis; one path chosen
  - test_type: contract

---

## 2.0 Delivery Mechanism Evaluation

**Status:** PENDING

Evaluate four options for reducing interrupt delivery latency. Pick one. The spec must justify the choice with latency, complexity, and failure mode analysis.

| Option | How | Latency | Complexity | Failure mode |
|--------|-----|---------|------------|-------------|
| A. Status quo (queued) | GETDEL at top of gate loop | Up to 60s | Already shipped | None — works today |
| B. Worker timer thread | Spawn a thread that polls Redis every 2s while gate command runs; on interrupt found, kill gate child + inject | ~2s | Low — one thread, same Redis pattern | Thread lifecycle; must not outlive gate command |
| C. Worker pub/sub listener | Worker SUBSCRIBE to `run:{id}:interrupt_signal`; on message, kill gate child + inject | <1s | Medium — new pub/sub subscription per run, must handle reconnect | Pub/sub disconnect during gate; message lost if worker not listening |
| D. API → executor direct IPC | Store `active_execution_id` + `executor_socket_addr` in DB; API handler connects to executor socket, calls InjectUserMessage | <500ms | High — cross-process socket, DB column, connection management, timeout | TOCTOU (executor dies between read and connect); API process needs filesystem access to socket |

**Dimensions (test blueprints):**
- 2.1 PENDING
  - target: Option B evaluation
  - input: timer thread polling Redis every 2s during `executeGateCommand`; on interrupt found, SIGTERM gate child, inject via `startStage`
  - expected: measured Redis load impact per active run; documented thread lifecycle management
  - test_type: contract
- 2.2 PENDING
  - target: Option C evaluation
  - input: worker subscribes to interrupt pub/sub channel alongside existing event pub/sub; on message, same kill+inject flow
  - expected: measured pub/sub reconnect reliability under network partitions
  - test_type: contract
- 2.3 PENDING
  - target: Option D evaluation
  - input: API handler reads `active_execution_id` from DB, connects to executor socket
  - expected: measured socket accessibility from API process in dev (same host) and prod (container)
  - test_type: contract
- 2.4 PENDING
  - target: decision record
  - input: evaluation results from 2.1–2.3
  - expected: one option picked with justification; others rejected with specific reasons
  - test_type: contract

---

## 3.0 Implementation

**Status:** PENDING

Implement the chosen delivery mechanism from §2.4. Dimensions TBD after decision is made. Known dimensions regardless of option chosen:

**Dimensions (test blueprints):**
- 3.1 PENDING
  - target: interrupt handler success path
  - input: instant interrupt delivery succeeds
  - expected: `INTERRUPT_DELIVERED` reason code logged (currently dead — always logs `INTERRUPT_QUEUED`)
  - test_type: integration
- 3.2 PENDING
  - target: interrupt handler response
  - input: instant delivery attempt (success or fallback)
  - expected: `effective_mode` field reflects actual delivery: `"instant"` when message reached executor mid-turn, `"queued"` when fell back
  - test_type: integration
- 3.3 PENDING
  - target: gate child process termination
  - input: interrupt arrives during running gate command
  - expected: gate command killed (SIGTERM → SIGKILL after 5s) so agent absorbs steer immediately, not after gate finishes
  - test_type: integration

---

## 4.0 Observability

**Status:** PENDING

Wire metrics, histograms, and structured logs for interrupt delivery monitoring.

**Dimensions (test blueprints):**
- 4.1 PENDING
  - target: `metrics.incInterruptInstant()`
  - input: successful instant interrupt delivery
  - expected: counter incremented on success path (currently defined but never incremented)
  - test_type: unit
- 4.2 PENDING
  - target: `metrics.incInterruptFallback()`
  - input: IPC/delivery failure forcing queued fallback
  - expected: counter incremented only on actual failure, not on every instant request
  - test_type: unit
- 4.3 PENDING
  - target: `zombie_interrupt_delivery_latency_ms` histogram
  - input: POST :interrupt to message reaching executor
  - expected: latency recorded in Prometheus histogram
  - test_type: integration
- 4.4 PENDING
  - target: structured log output
  - input: any interrupt delivery attempt
  - expected: log entry with `run_id`, `workspace_id`, `agent_id`, delivery mode, latency
  - test_type: unit

---

## 5.0 Interfaces

**Status:** PENDING

Lock the API surface for instant interrupt delivery.

### 5.1 Public Functions

```
// Existing — wire INTERRUPT_DELIVERED path
fn handleInterrupt(run_id: RunId, message: UserMessage, mode: InterruptMode) -> InterruptResult

// New — chosen delivery mechanism (TBD after §2.4)
// Signature will be added after delivery mechanism decision
```

### 5.2 Input Contracts

| Field | Type | Constraints | Example |
|-------|------|-------------|---------|
| run_id | string (UUID) | must reference active run | `"run_01abc..."` |
| message | string | non-empty, max 10KB | `"stop, wrong branch"` |
| mode | enum | `"instant"` or `"queued"` | `"instant"` |

### 5.3 Output Contracts

| Field | Type | When | Example |
|-------|------|------|---------|
| effective_mode | string | always | `"instant"` or `"queued"` |
| reason | string | always | `"INTERRUPT_DELIVERED"` or `"INTERRUPT_QUEUED"` |
| latency_ms | number | instant mode | `1200` |

### 5.4 Error Contracts

| Error condition | Behavior | Caller sees |
|----------------|----------|-------------|
| Executor unavailable | Fall back to queued delivery | `effective_mode: "queued"`, `reason: "INTERRUPT_QUEUED"` |
| Run not found | Reject with 404 | HTTP 404, error message |
| TOCTOU (executor dies between read and connect) | Fall back to queued delivery | `effective_mode: "queued"` |
| Gate command not running | Queue message for next checkpoint | `effective_mode: "queued"` |

---

## 6.0 Failure Modes

**Status:** PENDING

| Failure | Trigger | System behavior | User observes |
|---------|---------|----------------|---------------|
| Executor crash mid-delivery | Executor process dies between interrupt receipt and injection | Fall back to queued; message persists in Redis for next gate poll | `effective_mode: "queued"`; slight delay but no message loss |
| Pub/sub disconnect (Option C) | Network partition during gate command | Worker reconnects; falls back to Redis poll on reconnect failure | Delivery delayed to next poll cycle |
| Timer thread outlives gate (Option B) | Bug in thread lifecycle management | Thread detects gate completion, self-terminates | No user-visible impact if correctly handled |
| SIGTERM ignored by gate child | Gate command traps SIGTERM | Escalate to SIGKILL after 5s timeout | Max 5s additional delay |

**Platform constraints:**
- Unix socket paths have OS-dependent max length (~104 bytes on macOS, ~108 on Linux) — affects Option D socket addressing
- SIGTERM delivery to process groups requires careful PID tracking when gate commands spawn subprocesses

---

## 7.0 Implementation Constraints (Enforceable)

**Status:** PENDING

| Constraint | How to verify |
|-----------|---------------|
| Interrupt visible to agent within 5s of POST | Integration test with timer: POST interrupt, assert agent sees message < 5s |
| Gate child killed within 5s of interrupt | Integration test: start long gate command, send interrupt, assert process exit < 5s |
| Fallback to queued on any instant-path failure | Unit test: mock executor unavailable, assert `effective_mode: "queued"` |
| No message loss on fallback | Integration test: interrupt during failure, verify message delivered at next checkpoint |
| File under 500 lines | `wc -l < 500` on all touched files |
| Latency histogram populated | Integration test: send interrupt, assert histogram has observation |

---

## 8.0 Test Specification

**Status:** PENDING

### Unit Tests

| Test name | Dimension | Target | Input | Expected |
|-----------|-----------|--------|-------|----------|
| test_interrupt_instant_counter | 4.1 | metrics.incInterruptInstant | successful instant delivery | counter incremented by 1 |
| test_interrupt_fallback_counter | 4.2 | metrics.incInterruptFallback | delivery failure | counter incremented by 1 |
| test_structured_log_fields | 4.4 | interrupt log output | any interrupt | log contains run_id, workspace_id, agent_id, mode, latency |
| test_effective_mode_instant | 3.2 | interrupt response | successful instant delivery | `effective_mode: "instant"` |
| test_effective_mode_fallback | 3.2 | interrupt response | executor unavailable | `effective_mode: "queued"` |

### Integration Tests

| Test name | Dimension | Infra needed | Input | Expected |
|-----------|-----------|-------------|-------|----------|
| test_interrupt_during_gate | 3.3 | Redis + executor | interrupt while gate runs | gate killed, agent sees message < 5s |
| test_interrupt_delivered_reason | 3.1 | Redis + executor | successful instant delivery | `INTERRUPT_DELIVERED` in run_transitions |
| test_fallback_on_executor_crash | 5.4 | Redis | interrupt with dead executor | `INTERRUPT_QUEUED`, message delivered at next poll |
| test_latency_histogram | 4.3 | Redis + Prometheus | interrupt delivery | histogram observation present |

### Contract Tests

| Test name | Dimension | What it proves |
|-----------|-----------|---------------|
| test_response_shape | 5.3 | response contains effective_mode, reason, latency_ms |
| test_error_404_unknown_run | 5.4 | 404 returned for non-existent run_id |

### Spec-Claim Tracing

| Spec claim (from Overview/Goal) | Test that proves it | Test type |
|--------------------------------|-------------------|-----------|
| agent sees message within 5s | test_interrupt_during_gate | integration |
| gate command is killed | test_interrupt_during_gate | integration |
| token burn stops within 5s | test_interrupt_during_gate (agent stops generating after message) | integration |
| fallback to queued works | test_fallback_on_executor_crash | integration |
| INTERRUPT_DELIVERED on instant success | test_interrupt_delivered_reason | integration |

---

## 9.0 Execution Plan (Ordered)

**Status:** PENDING

| Step | Action | Verify |
|------|--------|--------|
| 1 | Document executor isolation (§1.0) | Decision record committed |
| 2 | Evaluate delivery mechanisms (§2.0) | Decision record with chosen option committed |
| 3 | Implement chosen delivery mechanism (§3.0) | `make test` passes |
| 4 | Wire INTERRUPT_DELIVERED reason code (§3.1) | Integration test passes |
| 5 | Wire effective_mode response field (§3.2) | Unit + integration tests pass |
| 6 | Implement gate child kill on interrupt (§3.3) | Integration test: gate killed < 5s |
| 7 | Wire observability (§4.0) | Metrics + histogram tests pass |
| 8 | Generate tests via /write-unit-test | All tests pass |
| 9 | Cross-compile check (if Zig touched) | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` |

---

## 10.0 Acceptance Criteria

**Status:** PENDING

- [ ] User sends `mode: "instant"` while a gate command is running; agent sees the message within 5s (not 60s) — verify: integration test `test_interrupt_during_gate`
- [ ] Gate command is killed on interrupt; agent does not wait for it to finish — verify: integration test asserts process exit < 5s
- [ ] `run_transitions` shows `INTERRUPT_DELIVERED` on instant success, `INTERRUPT_QUEUED` on fallback — verify: `make test-integration`
- [ ] Fallback to queued works when executor is unavailable (TOCTOU, crash, etc.) — verify: `test_fallback_on_executor_crash`
- [ ] Token burn stops within 5s of interrupt (not 60s) — verify: integration test with timer
- [ ] `zombie_interrupt_delivery_latency_ms` histogram visible in Prometheus — verify: `curl localhost:9090/metrics | grep zombie_interrupt_delivery_latency_ms`

---

## 11.0 Verification Evidence

**Status:** PENDING

Filled in during VERIFY phase.

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Unit tests | `make test` | | |
| Integration tests | `make test-integration` | | |
| Cross-compile | `zig build -Dtarget=x86_64-linux` | | |
| Lint | `make lint` | | |
| 500L gate | `wc -l` on touched files | | |
| Latency histogram | `curl localhost:9090/metrics` | | |

---

## 12.0 Out of Scope

- Desktop/Voice UI (M21_001 §4, separate workstream)
- Rate limiting interrupts per run
- Batching multiple interrupts into one delivery
- Distributed deployment (API and worker on different hosts with no shared filesystem)
- Grafana panel configuration (§4.5 from original spec — ops task, not code)
