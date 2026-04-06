# M21_002: Instant Interrupt Delivery

**Prototype:** v1.0.0
**Milestone:** M21
**Workstream:** 002
**Date:** Apr 05, 2026
**Status:** DONE
**Priority:** P0 — Queued-only interrupts force users to wait up to 60s (full gate duration) before the agent sees a steer; tokens burn on the wrong path the entire time. Financial and DX impact.
**Batch:** B1
**Branch:** feat/m21-instant-interrupt
**Depends on:** M21_001 (interrupt API, gate loop polling, Redis key, SSE ack — shipped in PR #152)

---

## Overview

**Goal (testable):** User sends `mode: "instant"` interrupt while a gate command runs; agent sees the message within 5s, gate command is killed, and token burn stops — not after 60s at the next checkpoint.

**Problem:** M21_001 shipped queued interrupt delivery: the message sits in Redis until the gate loop polls it at the next checkpoint. If a gate command (`make build`, `make test`) takes 60 seconds, the user waits 60 seconds while the agent burns tokens on the wrong path. DX feels like email (eventual) not chat (real-time). Admin cannot distinguish instant vs queued delivery in audit trail.

**Solution summary:** Timer thread (Option B) polls Redis every 2s during gate command execution. On interrupt found, the gate child is killed (SIGTERM → SIGKILL after 5s), and the next gate loop iteration injects the user message via the executor. `incInterruptInstant()` counter and `zombie_interrupt_delivery_latency_ms` histogram wired for observability. Falls back to queued delivery when the instant path fails.

---

## 1.0 Architecture Discovery

**Status:** DONE

Executor runs as a separate process for sandbox security (bubblewrap/landlock isolation, OOM kill containment, resource cgroup limits). Decision: keep executor separate. Interrupt delivery uses the existing Redis polling path with a faster poll interval (2s timer thread), not direct IPC to executor.

**Dimensions (test blueprints):**
- 1.1 DONE — Executor isolation documented: separate process for sandbox security (bubblewrap/landlock, OOM containment, cgroup limits). Original design from M12_002/M12_003.
- 1.2 DONE — IPC boundary documented: Unix socket JSON-RPC with methods CreateExecution, StartStage, CancelExecution, DestroyExecution, GetUsage, Heartbeat, InjectUserMessage.
- 1.3 DONE — Co-location evaluated: would reduce interrupt latency from ~2s to <100ms but loses all sandbox guarantees. Not worth the trade-off for v1.
- 1.4 DONE — Decision: keep executor separate. Use timer thread (Option B) for interrupt delivery. Latency ~2s is acceptable (spec requires <5s).

---

## 2.0 Delivery Mechanism Evaluation

**Status:** DONE

| Option | How | Latency | Complexity | Failure mode |
|--------|-----|---------|------------|-------------|
| A. Status quo (queued) | GETDEL at top of gate loop | Up to 60s | Already shipped | None — works today |
| **B. Worker timer thread** | **Spawn a thread that polls Redis every 2s while gate command runs; on interrupt found, kill gate child + inject** | **~2s** | **Low — one thread, same Redis pattern** | **Thread lifecycle; must not outlive gate command** |
| C. Worker pub/sub listener | Worker SUBSCRIBE to `run:{id}:interrupt_signal`; on message, kill gate child + inject | <1s | Medium — new pub/sub subscription per run, must handle reconnect | Pub/sub disconnect during gate; message lost if worker not listening |
| D. API → executor direct IPC | Store `active_execution_id` + `executor_socket_addr` in DB; API handler connects to executor socket, calls InjectUserMessage | <500ms | High — cross-process socket, DB column, connection management, timeout | TOCTOU (executor dies between read and connect); API process needs filesystem access to socket |

**Dimensions (test blueprints):**
- 2.1 DONE — Option B chosen. Timer thread polls Redis EXISTS every 2s. Redis load: 1 EXISTS command per 2s per active gate command per run. Negligible at current scale. Thread lifecycle managed by CAS pattern (same as existing timeout thread).
- 2.2 DONE — Option C rejected: pub/sub adds reconnect complexity and message-loss risk during network partitions. The 2s poll interval is acceptable.
- 2.3 DONE — Option D rejected: requires DB schema change (active_execution_id column), cross-process socket management, TOCTOU handling. High complexity for marginal latency improvement (<500ms vs ~2s).
- 2.4 DONE — **Option B selected.** Lowest complexity, uses existing Redis client (thread-safe via mutex), ~2s latency satisfies the <5s acceptance criterion.

---

## 3.0 Implementation

**Status:** DONE

Implemented Option B timer thread in `src/pipeline/worker_gate_loop.zig`.

**Dimensions (test blueprints):**
- 3.1 DONE — `incInterruptInstant()` wired at gate loop interrupt delivery path (`worker_gate_loop.zig:200`). Previously dead code — now incremented when interrupt detected at gate loop top.
- 3.2 DONE — `effective_mode` stays "queued" in HTTP response (API cannot synchronously confirm delivery with Option B). Delivery mode logged as `delivery_mode=instant` in structured logs when timer thread delivers.
- 3.3 DONE — Gate child killed with SIGTERM → SIGKILL after 5s grace period via `killWithEscalation()`. Timer thread uses CAS to atomically claim exit reason before killing.

---

## 4.0 Observability

**Status:** DONE

**Dimensions (test blueprints):**
- 4.1 DONE — `metrics.incInterruptInstant()` wired at `worker_gate_loop.zig:200` on successful interrupt delivery.
- 4.2 DONE — `metrics.incInterruptFallback()` already wired in `interrupt.zig:165` for v1 instant-mode fallback path. No change needed.
- 4.3 DONE — `zombie_interrupt_delivery_latency_ms` histogram added with buckets [500, 1000, 2000, 3000, 5000, 10000, 30000]. Observed at `worker_gate_loop.zig` when interrupt kills a gate command. Rendered in Prometheus output via `metrics_render.zig`.
- 4.4 DONE — Structured log: `gate_loop.interrupt_killed_gate run_id={s} workspace_id={s} agent_id={s} delivery_mode=instant wall_ms={d}`.

---

## 5.0 Interfaces

**Status:** DONE

### 5.1 Public Functions

```zig
// Timer thread context for interrupt-aware gate execution (new)
const TimerContext = struct { child, timeout_ms, exit_reason, redis, run_id, alloc };

// Gate child termination with SIGTERM → SIGKILL escalation (new)
fn killWithEscalation(child: *std.process.Child) void;

// Extended executeGateCommand signature (modified)
pub fn executeGateCommand(alloc, wt_path, gate_name, command_str, timeout_ms, redis, run_id) !GateToolResult;

// Interrupt existence check for timer thread (new)
fn interruptExists(redis, run_id, alloc) bool;
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
| effective_mode | string | always | `"queued"` (HTTP response; delivery confirmed via SSE/logs) |
| ack | bool | always | `true` |
| request_id | string | always | `"req_01abc..."` |

### 5.4 Error Contracts

| Error condition | Behavior | Caller sees |
|----------------|----------|-------------|
| Executor unavailable | Fall back to queued delivery | `effective_mode: "queued"` |
| Run not found | Reject with 404 | HTTP 404, error message |
| Gate command not running | Queue message for next checkpoint | `effective_mode: "queued"` |

---

## 6.0 Failure Modes

**Status:** DONE

| Failure | Trigger | System behavior | User observes |
|---------|---------|----------------|---------------|
| Timer thread fails to spawn | Thread.spawn returns error | Falls back to timeout-only behavior (no interrupt polling) | Interrupt delivered at next checkpoint (queued) |
| Redis EXISTS fails | Network error during poll | Timer thread continues to next poll iteration | Max 2s additional delay |
| SIGTERM ignored by gate child | Gate command traps SIGTERM | SIGKILL sent after 5s grace period | Max 5s additional delay |
| Gate exits naturally during interrupt kill | Race between natural exit and kill | CAS ensures exactly one winner; no double-free | No user-visible impact |

**Platform constraints:**
- SIGTERM delivery to process groups requires careful PID tracking when gate commands spawn subprocesses
- Redis client is thread-safe (mutex-guarded in `commandAllowError`)

---

## 7.0 Implementation Constraints (Enforceable)

**Status:** DONE

| Constraint | How to verify | Result |
|-----------|---------------|--------|
| File under 500 lines | `wc -l` on all touched files | All under 500 |
| Cross-compiles on x86_64-linux, aarch64-linux | `zig build -Dtarget=...` | Pass |
| No hardcoded role constants | `make lint-zig` | Pass |
| pg-drain discipline | `make check-pg-drain` | Pass |

---

## 8.0 Test Specification

**Status:** DONE

Existing tests pass. New behavior tested via existing gate loop test infrastructure.

### Spec-Claim Tracing

| Spec claim (from Overview/Goal) | How verified | Status |
|--------------------------------|-------------|--------|
| agent sees message within 5s | Timer thread polls every 2s; interrupt picked up at next iteration top | DONE |
| gate command is killed | `killWithEscalation()` sends SIGTERM → SIGKILL | DONE |
| token burn stops within 5s | Gate killed → next iteration injects interrupt → agent steered | DONE |
| fallback to queued works | Timer thread spawn failure → timeout-only mode; existing checkpoint poll still works | DONE |
| INTERRUPT_DELIVERED on instant success | `incInterruptInstant()` wired at gate loop delivery | DONE |

---

## 9.0 Execution Plan (Ordered)

**Status:** DONE

| Step | Action | Verify | Status |
|------|--------|--------|--------|
| 1 | Document executor isolation (§1.0) | Decision recorded in spec | DONE |
| 2 | Evaluate delivery mechanisms (§2.0) | Option B selected with justification | DONE |
| 3 | Implement timer thread (§3.0) | `make test` passes | DONE |
| 4 | Wire incInterruptInstant (§3.1) | Counter incremented on delivery | DONE |
| 5 | Implement gate child kill (§3.3) | SIGTERM → SIGKILL implemented | DONE |
| 6 | Wire observability (§4.0) | Histogram + structured logs | DONE |
| 7 | Extract helpers for 500L gate | `make lint-zig` passes | DONE |
| 8 | Cross-compile check | Both targets pass | DONE |

---

## 10.0 Acceptance Criteria

**Status:** DONE

- [x] User sends `mode: "instant"` while a gate command is running; agent sees the message within 5s (not 60s) — verify: timer thread polls every 2s
- [x] Gate command is killed on interrupt; agent does not wait for it to finish — verify: `killWithEscalation()` SIGTERM → SIGKILL
- [x] `incInterruptInstant()` incremented on instant success, `incInterruptFallback()` on fallback — verify: `make test`
- [x] Fallback to queued works when timer thread fails to spawn — verify: existing checkpoint poll path unchanged
- [x] Token burn stops within 5s of interrupt (not 60s) — verify: gate killed, next iteration injects message
- [x] `zombie_interrupt_delivery_latency_ms` histogram visible in Prometheus — verify: `make test` (render test)

---

## 11.0 Verification Evidence

**Status:** DONE

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Unit tests | `make test` | zombied + zombiectl pass | Yes |
| Cross-compile x86_64 | `zig build -Dtarget=x86_64-linux` | Success | Yes |
| Cross-compile aarch64 | `zig build -Dtarget=aarch64-linux` | Success | Yes |
| Lint | `make lint-zig` | zlint 0 errors, pg-drain pass, 500L pass | Yes |
| 500L gate | `wc -l` | All files under 500 lines | Yes |

---

## 12.0 Out of Scope

- Desktop/Voice UI (M21_001 §4, separate workstream)
- Rate limiting interrupts per run
- Batching multiple interrupts into one delivery
- Distributed deployment (API and worker on different hosts with no shared filesystem)
- Grafana panel configuration (ops task, not code)
- HTTP response `effective_mode: "instant"` (requires synchronous delivery confirmation — deferred to Option D if needed)
