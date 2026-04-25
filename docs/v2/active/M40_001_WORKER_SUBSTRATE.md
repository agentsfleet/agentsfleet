# M40_001: Worker Substrate — Control Stream, Dynamic Discovery, Per-Zombie Cancel, Drain

**Prototype:** v2.0.0
**Milestone:** M40
**Workstream:** 001
**Date:** Apr 25, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — launch-blocking. Without this the worker can't pick up a newly-installed zombie without a full restart, and kills don't propagate to in-flight executions. Every other substrate spec (M41 Execution, M42 Streaming, M43 Webhook Ingest) builds on the watcher pattern owned here.
**Categories:** API, CLI
**Batch:** B1 — first launch-blocking workstream. Parallel with M41, M42, M44, M45.
**Branch:** feat/m40-worker-substrate
**Depends on:** none structurally — this is the foundation.

**Canonical architecture:** `docs/ARCHITECHTURE.md` §5 (Architecture Direction), §9 (Steer Flow), §12 (End-to-End Technical Sequence — steps 5-7, 10).

---

## Overview

**Goal (testable):** A zombie created via `POST /v1/.../zombies` (the install endpoint) is claimed by a worker-side thread and ready to process events within ≤1 second of the API returning 201 — with no worker restart required. A zombie killed via `POST /v1/.../zombies/{id}/kill` has its in-flight execution aborted within milliseconds (not on the 5s `XREADGROUP` cycle). A `WorkerState` drain primitive lets `zombied worker` shut down gracefully — current-event finishes, no new events claimed.

**Problem:** Today the worker does a one-shot `listActiveZombieIds` query at startup and never re-reads `core.zombies`. A zombie installed at 14:00 is invisible to the worker that started at 13:00 — it stays on disk but never runs until restart. There is no `/kill` endpoint that participates in a control loop; killed zombies keep consuming events until shutdown. There is no graceful drain — `SIGTERM` interrupts mid-event work.

**Solution summary:** Introduce a single shared Redis stream `zombie:control` that the watcher thread inside `zombied worker` consumes. Every site in `zombied-api` that mutates `core.zombies` (create, status change, config patch, kill) publishes a control message synchronously **before** returning to the caller. The watcher reacts by spawning, reconfiguring, or canceling per-zombie threads. Each zombie thread carries its own atomic cancel flag. A ported `WorkerState` drain primitive (from pre-M10 `src/pipeline/worker_state.zig`) handles graceful shutdown. The `innerCreateZombie` path is extended to do INSERT + `XGROUP CREATE MKSTREAM zombie:{id}:events zombie_workers 0` + `XADD zombie:control` synchronously before returning 201 — invariant 1 ("stream + group exist before any producer/consumer can arrive").

---

## Files Changed (blast radius)

| File | Action | Why |
|---|---|---|
| `src/cmd/worker.zig` | EXTEND | Add watcher thread spawn + cancel-flag map + WorkerState drain integration |
| `src/cmd/worker_zombie.zig` | EXTEND | Per-zombie thread reads cancel flag at top of loop; checks WorkerState drain status |
| `src/cmd/worker_watcher.zig` | NEW | Watcher thread: `XREADGROUP` on `zombie:control`, dispatches to spawn/cancel/reconfig |
| `src/cmd/common.zig` | EXTEND | Migration: ensure `zombie:control` consumer group exists at first worker start |
| `src/http/handlers/zombies/create.zig` | EXTEND | `innerCreateZombie`: INSERT + XGROUP CREATE + XADD `zombie:control` atomically |
| `src/http/handlers/zombies/kill.zig` | NEW | `POST /v1/.../zombies/{id}/kill` → status update + control-stream publish |
| `src/http/handlers/zombies/patch.zig` | NEW | `PATCH /v1/.../zombies/{id}` for config changes (used by M41 Execution + M48 BYOK) |
| `zombiectl/src/program/routes.js` | EXTEND | Add `kill` route (already present per pre-flight) — verify wired through |
| `src/zombie/control_stream.zig` | NEW | Shared library: control message types, encode/decode, group-create idempotency |
| `src/state/worker_state.zig` | NEW | Drain primitive: tracks in-flight count, blocks new claims when draining, signals when 0 |
| `tests/integration/worker_dynamic_discovery_test.zig` | NEW | E2E: install zombie → ≤1s claim, kill → ≤200ms cancel |

---

## Sections (implementation slices)

### §1 — Control stream protocol
Define message types in `src/zombie/control_stream.zig`: `zombie_created`, `zombie_status_changed` (active/killed/paused), `zombie_config_changed`, `worker_drain_request`. Encode as Redis stream entries with `type` + `zombie_id` + optional payload. Idempotent `XGROUP CREATE` with `MKSTREAM` flag.

### §2 — Watcher thread + cancel-flag map
`src/cmd/worker_watcher.zig`: blocks on `XREADGROUP zombie_workers <consumer> COUNT 16 BLOCK 5000 STREAMS zombie:control >`. For each message: dispatch to handler. Maintains `std.AutoHashMap(ZombieId, *AtomicCancelFlag)` shared with the per-zombie threads. `XACK` after handler returns.

### §3 — Per-zombie thread integration
`src/cmd/worker_zombie.zig`: at top of every loop iteration, check `cancel_flag.load(.acquire)`. If set, break out, signal WorkerState, return. Top-of-loop also calls the steer-poll (M42's responsibility, but the hook lives here).

### §4 — WorkerState drain primitive
`src/state/worker_state.zig`: ports the pre-M10 pattern. `acquire()` increments in-flight count; `release()` decrements. `requestDrain()` sets a flag; `acquire()` blocks new acquires when set. `awaitDrained()` blocks until in-flight reaches 0. SIGTERM handler in `worker.zig` calls `requestDrain` → `awaitDrained(timeout=30s)`.

### §5 — innerCreateZombie atomic publish
`src/http/handlers/zombies/create.zig`: wrap INSERT + XGROUP CREATE + XADD in a transaction-like block. If XADD fails, the row is still committed (zombie exists in DB) but the worker never claims it — surface as 500 with rollback prompt. Test: a webhook arriving 1ms after the 201 finds the stream group already.

### §6 — Kill endpoint + cancel propagation
`src/http/handlers/zombies/kill.zig`: UPDATE core.zombies SET status='killed' + XADD `zombie:control` `zombie_status_changed status=killed`. Watcher reads it, sets cancel_flag, calls `executor_client.cancelExecution(execution_id)` (executor's responsibility per M41). Per-zombie thread breaks out within ms.

---

## Interfaces

```
POST /v1/workspaces/{ws}/zombies
  body: { source_markdown, name? }
  response: 201 { id, ... }
  invariant: by the time the 201 arrives, zombie:{id}:events stream + group exist

POST /v1/workspaces/{ws}/zombies/{id}/kill
  body: { reason? }
  response: 202 { status: "killed", queued_at }
  invariant: in-flight execution canceled within 200ms of 202

PATCH /v1/workspaces/{ws}/zombies/{id}
  body: partial { config_json? }
  response: 200 { config_revision }
  consumed by: M41 (config hot-reload), M48 (BYOK provider switch)

control stream (Redis):
  zombie:control [zombie_workers consumer group]
  message types:
    - zombie_created      { zombie_id, workspace_id }
    - zombie_status_changed { zombie_id, status: active|killed|paused }
    - zombie_config_changed { zombie_id, config_revision }
    - worker_drain_request { reason? }
```

---

## Failure Modes

| Mode | Cause | Handling |
|---|---|---|
| XADD fails after INSERT | Redis down briefly | 500 to caller with `rollback_required: true`; ops alert; manual repair via reconcile job |
| Worker watcher loses connection | Network blip | Reconnect with backoff; XREADGROUP from last-acked id; never miss a message |
| Cancel flag set but executor unresponsive | Executor process hung | After 5s grace, SIGKILL the executor session; surface `execution_aborted_force` event |
| Drain timeout exceeded (30s) | Stuck event mid-tool-call | Force-cancel all in-flight, exit dirty (operator sees the timeout in stderr) |
| Config patch racing with in-flight event | PATCH lands while event processing | Config revision check: in-flight uses revision-at-claim; new revision applies to next event |

---

## Invariants

1. **Stream + group exist before any producer/consumer can arrive.** `innerCreateZombie` does INSERT + XGROUP CREATE + XADD synchronously before returning 201.
2. **Cancel is observable at every loop top.** Per-zombie thread checks cancel_flag before any blocking call.
3. **Drain blocks new claims.** Once `requestDrain` is called, no new `XREADGROUP` claim succeeds until drain is canceled.
4. **One control consumer per worker process.** Multiple workers safe via consumer group; each message processed once.

---

## Test Specification

| Test | Asserts |
|---|---|
| `test_install_then_event_lt_1s` | Install via API → watch zombie:control → claim → first event processed ≤1s after 201 |
| `test_kill_in_flight_lt_200ms` | Start a long-running tool call → POST /kill → assert cancelExecution RPC fires within 200ms |
| `test_config_patch_takes_effect_next_event` | PATCH config → in-flight event uses old config → next event uses new config |
| `test_drain_blocks_new_claims` | Send drain → verify XREADGROUP no longer claims → assert in-flight count goes to 0 → drain returns |
| `test_create_atomicity_xadd_failure` | Force XADD failure → verify 500 + zombie row rolled back (or marked needs-repair) |
| `test_watcher_reconnect` | Kill Redis connection mid-XREADGROUP → reconnect → resume from last-acked id → no missed messages |

All tests in `tests/integration/worker_dynamic_discovery_test.zig`.

---

## Acceptance Criteria

- [ ] `make test-integration` passes the 6 tests above
- [ ] `zombiectl install --from samples/platform-ops/` followed by `zombiectl steer {id} "ping"` works end-to-end without a worker restart in between
- [ ] `zombiectl kill {id}` aborts an in-flight tool call within 200ms (verified via timestamps in `core.zombie_events`)
- [ ] `make memleak` clean — no allocator leaks on watcher reconnect
- [ ] `make check-pg-drain` clean
- [ ] Cross-compile clean: `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux`
