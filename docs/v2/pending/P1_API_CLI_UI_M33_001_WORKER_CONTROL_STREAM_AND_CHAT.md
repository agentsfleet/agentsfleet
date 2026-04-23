# M33_001: Worker Control Stream, Dynamic Discovery, Per-Zombie Cancel, Chat

**Prototype:** v2.0.0
**Milestone:** M33
**Workstream:** 001
**Date:** Apr 23, 2026
**Status:** PENDING
**Priority:** P1 — fixes the worker's one-shot discovery gap. Without this, a zombie created after worker startup never runs until a restart, and a zombie killed while in-flight keeps consuming events until shutdown. Blocks M37_001 §2 dims and the whole M31 acceptance.
**Batch:** B1 — first worker workstream in the M31 series. Parallel with M34_001 (event history) and M35_001 (per-session policy). Blocks M36_001 (live watch consumes the same activity stream).
**Branch:** feat/m33-worker-control-stream (to be created when work starts)
**Depends on:** M19_003 (install-from-path — gives the create-flow a real client). No dependency on M35/M36/M37.

**Canonical architecture:** `docs/ARCHITECTURE_ZOMBIE_EVENT_FLOW.md` §1–§5, §8 (invariants 1, 5, 7, 8), §10. This spec implements those invariants; it does not redefine them.

---

## Overview

**Goal (testable):** A zombie created via `POST /v1/.../zombies` is claimed by a worker-side thread and ready to process events within ≤1 second of the 201, with no worker restart required. A zombie killed via `POST /v1/.../zombies/{id}/kill` has its in-flight execution aborted within milliseconds (not on the 5s XREADGROUP cycle). An operator runs `zombiectl zombie chat {id}` and gets an interactive session — history replayed, stdin prompts, `[claw]`-prefixed streamed responses, Ctrl-C exits without killing the zombie.

**Problem (from `docs/ARCHITECTURE_ZOMBIE_EVENT_FLOW.md` §10 ownership):** the watcher, the control stream, the per-zombie cancel flag, the `WorkerState` drain primitive, and the chat CLI/UI all need to exist for the zombie event flow to work dynamically. Today (as of 0.28.0): worker does a one-shot `listActiveZombieIds` at startup and never observes `core.zombies` again. `innerCreateZombie` does not write to Redis. There is no `/kill` endpoint that participates in a control loop. There is no interactive chat CLI — `/steer` exists but has no client ergonomics.

**Solution summary:** Add one watcher thread inside `zombied worker` that consumes a new shared Redis stream `zombie:control`. Every site in `zombied-api` that writes `core.zombies` publishes a control message. The watcher reacts by spawning/canceling zombie threads. Each zombie thread gets its own atomic cancel flag. A ported `WorkerState` drain primitive (from pre-M10 `src/pipeline/worker_state.zig`) gives graceful shutdown. `innerCreateZombie` is extended to do INSERT + `XGROUP CREATE MKSTREAM` + `XADD zombie:control` synchronously before returning 201 — invariant 1 ("stream exists before any producer/consumer can arrive"). `zombiectl zombie chat` wraps `/steer` + polls `GET /events` (or tails activity when M36_001 lands SSE) for interactive round-trip. UI chat widget hits the same path.

---

## Files Changed (blast radius)

### Worker (Zig)

| File | Action | Why |
|---|---|---|
| `src/cmd/worker/state.zig` | CREATE | Port of pre-M10 `src/pipeline/worker_state.zig`. In-flight counter, drain signal, accepting-work flag. ≤120 lines. |
| `src/cmd/worker/registry.zig` | CREATE | Minimal `cancels: HashMap(zombie_id → *Atomic(bool))` + `threads: ArrayList(std.Thread)`. ≤80 lines. |
| `src/cmd/worker/watcher.zig` | CREATE | One loop: `XREADGROUP zombie:control`, diff pg, spawn/cancel, 30s periodic reconcile. ≤250 lines. |
| `src/cmd/worker.zig` | MODIFY | Replace one-shot `listActiveZombieIds` + ArrayList spawn with: start `WorkerState`, start watcher, on SIGTERM start drain, wait in_flight==0 with timeout. |
| `src/cmd/worker_zombie.zig` | MODIFY | `ZombieWorkerConfig` gains `cancel_flag: *Atomic(bool)`. `watchShutdown` ORs cancel_flag with shutdown_flag. On exit, decrement `WorkerState.in_flight`. |
| `src/queue/redis_control.zig` | CREATE | `xaddZombieCreated/StatusChanged/ConfigChanged(redis, ...)` producers + `ensureControlConsumerGroup`. ≤150 lines. |
| `src/queue/redis_zombie.zig` | MODIFY | Expose `ensureZombieConsumerGroup` for use by API handler (currently only called by worker). |

### HTTP (Zig)

| File | Action | Why |
|---|---|---|
| `src/http/handlers/zombies/api.zig` | MODIFY | `innerCreateZombie` post-INSERT adds: `ensureZombieConsumerGroup` + `xaddZombieCreated`. Both synchronous, both before the 201. Wrap in a helper so the error path is explicit. |
| `src/http/handlers/zombies/lifecycle.zig` | CREATE | `POST /v1/.../zombies/{id}/kill`, `POST .../pause`, `POST .../resume`. Each: UPDATE + `xaddZombieStatusChanged`. ≤200 lines. |
| `src/http/handlers/zombies/steer.zig` | MODIFY | Existing M23_001 steer handler grows `actor` field in the written Redis key → surfaces into `zombie_events.actor` via M34_001's write path. (If M35 lands later, the field is accepted but unused.) |
| `src/http/router.zig` + `src/http/route_table.zig` + `src/http/route_manifest.zig` | MODIFY | Register the three new lifecycle routes. |

### CLI (JavaScript)

| File | Action | Why |
|---|---|---|
| `zombiectl/src/commands/zombie_chat.js` | CREATE | Interactive chat client: (1) on open, GET `/events?limit=20&filter=chat` and pretty-print history; (2) readline loop; (3) each line → POST `/steer`; (4) poll `/events` every 1s for rows after session start; (5) print `[claw] <response_text>` for new `processed` rows; (6) SIGINT → clean exit, zombie keeps running. ≤250 lines. |
| `zombiectl/src/commands/zombie_kill.js` | CREATE | Thin wrapper over `POST /v1/.../zombies/{id}/kill`. ≤50 lines. |
| `zombiectl/src/commands/zombie.js` | MODIFY | Register `chat` + `kill` subcommands. |
| `zombiectl/test/zombie-chat.unit.test.js` | CREATE | Mocks `/events` + `/steer` + readline input. Covers: history replay, send+receive loop, Ctrl-C exit, zombie-kept-running invariant. |
| `zombiectl/test/zombie-kill.unit.test.js` | CREATE | Covers success + 404 + auth-fail paths. |

### UI (TypeScript/React)

| File | Action | Why |
|---|---|---|
| `ui/packages/app/lib/api/zombies.ts` | MODIFY | Add `killZombie(wsId, zId)`, `pauseZombie`, `resumeZombie`, `sendChat(wsId, zId, msg)`, `getRecentEvents(wsId, zId, limit)`. |
| `ui/packages/app/app/(dashboard)/zombies/[id]/components/ChatWidget.tsx` | CREATE | Chat UI: history + input + `[claw]` streamed replies. Polls `/events` every 1s; upgrades to SSE when M36_001 lands. ≤200 lines. |
| `ui/packages/app/app/(dashboard)/zombies/[id]/components/StatusControls.tsx` | CREATE | Kill / Pause / Resume buttons; posts + optimistic UI. ≤120 lines. |
| `ui/packages/app/tests/chat-widget.test.tsx` | CREATE | MSW-backed: history replay, send, receive, error toast. |

### Schema

| File | Action | Why |
|---|---|---|
| `schema/NNN_zombie_lifecycle_controls.sql` | CREATE (pre-v2.0 teardown) | No new table; may add a check constraint on `core.zombies.status` enum if not already present. Schema Guard output required at EXECUTE. |

### Tests (Zig)

| File | Action | Why |
|---|---|---|
| `src/cmd/worker/watcher_test.zig` | CREATE | Unit: tick diff logic (pure function). ≤200 lines. |
| `src/cmd/worker/watcher_integration_test.zig` | CREATE | Integration: create zombie → watcher spawns thread within 1s; flip status=killed → thread cancels within 2s. Uses real pg + real Redis + mock executor. ≤300 lines. |
| `src/cmd/worker/drain_test.zig` | CREATE | Unit: `WorkerState` drain semantics. ≤120 lines. |

---

## Applicable Rules

- **RULE ZIG-DRAIN** — every `conn.query` gets `.drain()`.
- **RULE FLL** — every .zig touched stays ≤350 lines; extract if growing.
- **RULE TST-NAM** — no milestone IDs in test names (enforce by grep at VERIFY).
- **RULE ORP** — renamed/removed symbols (none here, new files only) — N/A.
- **Worktree-first** — never work in main; CHORE(open) creates the worktree.

---

## Sections (implementation slices)

### §1 — WorkerState drain primitive (port from pre-M10)

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 1.1 | PENDING | `src/cmd/worker/state.zig` | port `git show bb1137af^:src/pipeline/worker_state.zig` | same API: `init()`, `startDrain()`, `currentInFlightRuns()`, `isAcceptingWork()`, `completeDrain()` with zombie terminology | unit |
| 1.2 | PENDING | `src/cmd/worker.zig` signal path | SIGTERM during in-flight work | startDrain fires; watcher stops spawning; zombie threads finish current event; `WorkerState.in_flight` reaches 0; process exits. Grace timeout 270s (configurable). | integration |
| 1.3 | PENDING | drain timeout | in-flight event never completes | timeout fires; `completeDrain` logs warn; process force-exits. | unit |

### §2 — Control-stream producer (zombied-api)

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 2.1 | PENDING | `innerCreateZombie` | POST /v1/.../zombies | INSERT core.zombies, `XGROUP CREATE zombie:{id}:events zombie_workers 0 MKSTREAM`, XADD `zombie:control` type=`zombie_created`, THEN 201. Invariant 1 in arch doc. | integration |
| 2.2 | PENDING | `innerCreateZombie` failure path | XGROUP or XADD fails | 5xx with ERR_STREAM_PROVISION; `core.zombies` row deleted in same transaction (or kept with status='provisioning_failed' TBD) | integration |
| 2.3 | PENDING | `POST /v1/.../zombies/{id}/kill` | kill request | UPDATE core.zombies status='killed', XADD zombie:control type=`zombie_status_changed` status=killed, 200 | integration |
| 2.4 | PENDING | `POST /v1/.../zombies/{id}/pause` + `/resume` | toggle | analogous; status='paused'/'active'; XADD zombie:control | integration |
| 2.5 | PENDING | `POST /v1/.../zombies/{id}/steer` (existing) | carries `actor_hint` header | Redis key written with `{message, actor}`; actor surfaces into M34_001's event row | integration |

### §3 — Watcher (zombied-worker)

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 3.1 | PENDING | watcher startup | fresh worker process against a populated core.zombies | reconciles; spawns one thread per active zombie | integration |
| 3.2 | PENDING | watcher tick | XADD `zombie:control` type=`zombie_created` while worker running | new zombie thread spawned within 1s; XACK control | integration |
| 3.3 | PENDING | watcher kill path | XADD type=`zombie_status_changed` status=killed on a running zombie mid-execution | `cancels[id].store(true)` immediate; `executor.cancelExecution` called within 50ms; zombie thread exits within 2s; `core.zombie_events` final row has status='agent_error' with reason cancelled | integration |
| 3.4 | PENDING | watcher 30s reconcile | control message lost (simulate by skipping XADD on one create) | reconcile tick picks up the gap and spawns the missing thread within 30s | integration |
| 3.5 | PENDING | watcher crash resilience | panic in watcher thread | worker process exits non-zero; systemd/fly restart policy brings it back; next startup reconciles cleanly | integration (docker-compose restart) |

### §4 — Chat CLI (interactive)

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 4.1 | PENDING | `zombiectl zombie chat {id}` | open on zombie with 3 prior chat events | prints `[claw] <prior_msg_1>` ... `[claw] <prior_msg_3>` then `>` prompt | unit |
| 4.2 | PENDING | same | user types `poll fly` + Enter | POST `/steer` fires with `{message: "poll fly", actor: "steer:<user>"}`; within ≤5s a new `zombie_events` row appears; CLI prints `[claw] <response_text>` | integration |
| 4.3 | PENDING | same | Ctrl-C during prompt | graceful disconnect; zombie thread continues; exit code 0 | unit |
| 4.4 | PENDING | same | invalid zombie id | clear error; no POST | unit |

### §5 — Chat UI widget

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 5.1 | PENDING | `ChatWidget.tsx` | render with zombie id | fetches history (20 latest events filter=chat); renders bubbles | unit (MSW) |
| 5.2 | PENDING | same | user submits message | POST /steer; optimistic bubble; polls /events for response; renders response bubble with `[claw]` prefix | unit |
| 5.3 | PENDING | `StatusControls.tsx` | click Kill | POST /kill; optimistic status flip; on success confirmation toast | unit |

### §6 — End-to-end dogfood (integration)

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 6.1 | BLOCKED_ON §1–§5 | full acceptance path from arch doc §7 | create + chat + kill | all transitions observable; create-to-first-event latency <1s; kill-to-thread-exit <2s | integration |

---

## Interfaces

**Produced:**

- `POST /v1/workspaces/{ws}/zombies/{id}/kill` → `{status: "killed"}` (no body).
- `POST /v1/workspaces/{ws}/zombies/{id}/pause` / `resume` → `{status: "paused"|"active"}`.
- Redis stream `zombie:control` with consumer group `zombie_workers_control`. Message shape: `{event_id: uuid, type: "zombie_created"|"zombie_status_changed"|"zombie_config_changed", zombie_id: uuid, workspace_id: uuid, at_ms: i64, ...}`.

**Consumed:**

- Existing `POST /v1/.../zombies` (this spec modifies the handler).
- Existing `POST /v1/.../zombies/{id}/steer` (M23_001).
- Existing `GET /v1/.../zombies/{id}/events` — will exist once M34_001 lands; this spec's chat CLI tolerates its absence by falling back to `/v1/.../zombies/{id}/activities` or showing "history unavailable".
- NullClaw execution RPC via existing `executor_client`.

**Invariants carried forward from arch doc:** 1, 5, 7, 8 (see `docs/ARCHITECTURE_ZOMBIE_EVENT_FLOW.md` §8).

---

## Failure Modes

| Failure | Trigger | Behavior | User observes |
|---|---|---|---|
| XGROUP or XADD fails during create | Redis flap | 5xx; row rollback | client sees 503 with retry-after; no phantom zombie row |
| Control message lost (Redis flush) | ops | 30s pg reconcile picks it up; zombie starts ≤30s late | no permanent loss |
| Watcher crashes | bug | process exits 1; supervisor restarts; full reconcile | brief window (~seconds) where new zombies queue but don't spawn |
| Kill called on already-dead zombie | race | UPDATE sets status=killed idempotently; XADD fires; watcher sees id already absent from cancels → no-op | 200 returned |
| Chat CLI connection dropped | network | readline catches EOF; exits; zombie unaffected | user re-runs `zombie chat` |
| Chat CLI polls faster than zombie runs | normal | shows "thinking" indicator between send and first [claw] line | |
| Drain timeout with stuck event | stuck NullClaw call | WorkerState logs warn, force-exits; systemd brings worker back; XAUTOCLAIM reclaims the pending message | brief unavailability |

---

## Implementation Constraints (Enforceable)

| Constraint | Verification |
|---|---|
| `innerCreateZombie` does XGROUP + XADD before 201 | Eval E1 (integration — race test) |
| Watcher spawns thread for new zombie within 1s | Eval E2 |
| Kill aborts in-flight execution within 2s | Eval E3 |
| Drain completes when all events finish | Eval E4 |
| No in-memory state is truth (everything rebuilds from pg + streams) | Eval E5 (restart mid-event test) |
| Cross-compile linux x86_64 + aarch64 | `zig build -Dtarget=x86_64-linux` + aarch64 |

---

## Invariants (Hard Guardrails)

| # | Invariant | Enforcement |
|---|---|---|
| 1 | `core.zombies` row exists ⇒ stream + group exist | §2.1 integration + `XINFO STREAM` assert |
| 2 | Every `core.zombies` status change publishes exactly one control message | §2.3, §2.4 integration + `XLEN zombie:control` assert |
| 3 | Every spawned zombie thread has a `cancel_flag` in the registry | code-level (watcher.spawnThread is the only spawn site) |
| 4 | Kill path calls `executor.cancelExecution` before returning to XACK | §3.3 trace assertion |
| 5 | No ArrayList without associated alloc in Zig 0.15 pattern | RULE ZIG-0.15 in ZIG_RULES.md |
| 6 | No milestone IDs in `_test.zig` filenames or `test "..."` names | Eval E9 (grep) |

---

## Test Specification

### Unit tests

| Test | Dim | Expected |
|---|---|---|
| `worker state tracks in-flight across increment/decrement` | 1.1 | counter correct |
| `worker state drain waits for zero` | 1.2 | drain blocks until in_flight==0 |
| `worker state drain respects timeout` | 1.3 | returns after deadline |
| `watcher tick pure function` | 3.x | diff function returns correct spawn/cancel sets |
| `chat CLI prints history then prompts` | 4.1 | output includes `[claw]` prior messages |
| `chat CLI Ctrl-C graceful` | 4.3 | exit 0 |
| `ChatWidget renders history` | 5.1 | MSW mock returns 3 events; 3 bubbles rendered |

### Integration tests

| Test | Dim | Infra | Expected |
|---|---|---|---|
| `create-to-ready within 1s` | 2.1 + 3.2 | pg + redis + worker | POST /zombies; watcher spawns; XREADGROUP blocks within 1s |
| `create then immediate webhook` | 2.1 | same + webhook ingestor | XADD lands, thread picks it up, processes (no race) |
| `kill aborts in-flight` | 3.3 | same + mock executor with 60s hang | kill → cancelExecution → executor aborts within 50ms → thread exits within 2s |
| `30s reconcile picks up missed control msg` | 3.4 | same; inject into pg without XADD | thread spawns within 30s |
| `restart reclaims orphaned pending` | WorkerState + XAUTOCLAIM | same | after SIGKILL mid-event, restart; event replays once; zombie_events UNIQUE ensures no duplicate |
| `chat CLI end-to-end` | 4.2 | full stack | history + send + receive via polling |

### Negative tests

Synthetic: control msg with unknown type → logged + skipped + XACK'd. POST /kill on non-existent zombie → 404. POST /kill on already-killed → 200 idempotent.

### Regression guard

Current `listActiveZombieIds` one-shot discovery goes away; ensure no test depends on its specific behavior.

### Edge cases

Two workers (future): lease-based exclusion deferred to post-M31; for now single-worker invariant enforced at deploy (one `zombied worker` replica).

---

## Execution Plan (Ordered)

| Step | Action | Verify |
|---|---|---|
| 1 | CHORE(open): worktree `../usezombie-m33-control-stream` on `feat/m33-worker-control-stream`. | `git worktree list` |
| 2 | §1 port `WorkerState` (separate small commit — reviewable first). | unit tests green |
| 3 | §2 add `redis_control.zig` + wire into `innerCreateZombie` (failures of XGROUP/XADD roll back row). | integration §2.1–2.2 |
| 4 | §2 add `lifecycle.zig` (kill/pause/resume handlers) + routes. | integration §2.3–2.4 |
| 5 | §3 build watcher + registry. Wire into `worker.zig`. Per-zombie cancel flag in `ZombieWorkerConfig`. | integration §3.1–3.5 |
| 6 | §4 chat CLI + §5 UI widget + StatusControls. | unit + integration §4/§5 |
| 7 | §6 full acceptance (depends on M34_001 at least delivered in parallel for events polling to work — if not, chat fallback is "no history"). | integration §6.1 |
| 8 | Cross-compile, lint, memleak, gitleaks. | all gates green |
| 9 | CHORE(close): spec → done/, Ripley log, release-doc `<Update>`. | PR green |

---

## Acceptance Criteria

- [ ] Create-to-ready latency ≤ 1s — Eval E2
- [ ] Kill-to-thread-exit ≤ 2s with `executor.cancelExecution` trace — Eval E3
- [ ] Drain completes when in-flight==0 — Eval E4
- [ ] Missed control message recovered within 30s — §3.4
- [ ] Worker restart mid-event replays exactly once — Eval E5
- [ ] `zombiectl zombie chat` interactive UX works per §4.1–4.3 — manual + unit
- [ ] UI chat widget + status controls functional — §5
- [ ] Full §6.1 end-to-end green — integration
- [ ] All gates: make lint, make test, make memleak (server lifecycle), cross-compile x86_64+aarch64, gitleaks — manual

---

## Eval Commands

```bash
# E1: stream exists after create
curl -X POST .../zombies -d '{...}' | jq -r .zombie_id | xargs -I{} redis-cli XINFO STREAM zombie:{}:events | grep -q 'name'

# E2: create-to-spawn timing
# scripted: create + tail structured logs for watcher.spawn zombie_id={id} and assert <1s

# E3: kill-to-cancel
# scripted: create + send long-running chat; POST /kill; tail executor logs for handleCancelExecution within 50ms of /kill response

# E4: drain
# scripted: SIGTERM during in-flight; wait for process exit; assert in_flight was 0 before exit

# E5: restart-mid-event
# scripted: create + chat; SIGKILL during http_request tool call; restart; assert zombie_events has exactly one row with event_id=X

# E9: no milestone IDs in test names
grep -rnE 'test "M[0-9]+_[0-9]+|test "§[0-9]' src/ --include='*_test.zig' && echo FAIL || echo ok

# full gates
make lint && make test && make test-integration && make memleak && \
  zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux && gitleaks detect
```

---

## Discovery (fills during EXECUTE)

---

## Out of Scope

- Multiple zombied-worker replicas + per-zombie lease. Single-replica for M31. Lease work deferred until load warrants.
- pg LISTEN/NOTIFY as redundant control path. Redis stream + 30s pg reconcile is enough for MVP.
- SSE streaming on chat (polling for M34; SSE upgrade is M36_001).
- NullClaw cron wiring (how the runtime's fires land on `zombie:{id}:events`). M35_001 territory.
- Per-zombie cancel at execution-mid-stage for non-kill reasons (e.g. paused mid-turn). MVP: pause takes effect at next event; in-flight events finish.
- Admin CLI / UI to inspect `zombie:control` state. Use `XINFO`/`XPENDING` directly for now.
