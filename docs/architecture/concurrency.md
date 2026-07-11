# Concurrency architecture — threads, channels, locks, shutdown

Date: Jul 11, 2026
Status: Canonical concurrency model for `agentsfleetd` (control plane) and
`agentsfleet-runner` (execution plane). This is the file the `name_architecture`
dispatch consults before naming a thread, channel, or lock, or asserting a
shutdown ordering. Sibling of [`data_flow.md`](./data_flow.md) (the same runtime
traced per event) and [`runner_fleet.md`](./runner_fleet.md) (the control/execution
split). Channel and stream **names** are canonical in `data_flow.md`; this file
owns the thread/lock/shutdown layer on top of them.

The Allocator and concurrency rules `A1–A6` / `C1–C5` live in the Zig discipline
façade (`dispatch/write_zig.md`); this doc is where the `C`-rules become the
system's concrete invariants, and it is the seed the discipline roster
(`audits/zig-discipline-roster.txt`) expands against.

---

## The five invariants (rules C1–C5)

1. **C1 — SPSC channels, receiver frees.** Every channel that crosses a thread
   boundary has a single declared producer and single consumer; the payload
   carries its own allocator and the receiver frees it in a `defer` at the top of
   the handler.
2. **C2 — stop → join → deinit.** Shutdown signals stop, joins the worker, and
   only then deinits shared state. A bounded drain that times out never frees
   state a straggler thread can still touch.
3. **C3 — no blocking work under a lock the consumer needs.** A blocking socket
   write or push is never done while holding a lock the consumer must acquire to
   make progress. Lock state is an explicit parameter, not an ambient assumption.
4. **C4 — one documented mutex per shared aggregate.** Each shared aggregate has
   exactly one mutex whose doc comment states precisely what it protects and any
   ordering constraint; `lock(); defer unlock();` adjacent.
5. **C5 — thread-confined by default.** State touched by one thread carries no
   lock but says so (`// only touched by thread X`); `*Locked`-suffixed functions
   mark lock-required entry points.

The primitives are in [`src/lib/common/sync.zig`](../../src/lib/common/sync.zig):
`Mutex` (arg-free `lock`/`unlock` over `std.Io.Mutex`), `Condition`, a rebuilt
`WaitGroup`, and `Event` — the one-shot, poll-based replacement for the
`std.Thread.ResetEvent` that Zig 0.16 removed, used for deterministic
stop→join handshakes in lifecycle tests.

---

## Thread map

Every long-lived thread, who spawns it, the shared state it touches, how that
state is protected, and how the thread is stopped and joined.

### `agentsfleetd` (control plane)

The daemon's background fleet is spawned in `cmd/serve_background.zig`
(`BackgroundThreads.start`) and torn down in `cmd/serve_shutdown.zig`.

| Thread | Spawned by | Touches | Protection | Stop path |
|---|---|---|---|---|
| signal watcher | `serve_shutdown.signalWatcher` | the process-global shutdown flag | atomic flag; armed only **after** the server is published (boot-window SIGTERM survives) | one-shot; SIGTERM/SIGINT sets the flag, then it returns |
| event bus | `events/bus.runThread` | the in-proc event queue | `bus.mutex` | flag observed → loop exits → joined |
| approval-gate sweeper | `fleet_runtime/approval_gate_sweeper.run` | Postgres (own conn) | none shared (per-thread conn) | shutdown flag → exits → joined |
| liveness sweeper | `fleet/liveness_sweeper.run` | Postgres (own conn) | none shared | shutdown flag → exits → joined |
| reclaim sweeper | `fleet/reclaim_sweeper.run` | Postgres + Redis (own conns) | none shared | shutdown flag → exits → joined |
| outbound worker | `queue/outbound_worker.run` | Redis outbound stream (own conn) | none shared | shutdown flag → exits → joined |
| SSE hub reader | `events/subscription_hub_reader.readerMain` | the one shared pub/sub connection + the `channels` map | `hub.wire` (connection) + `hub.mutex` (map), acquired one at a time | `stop()` → reader observes stop → joined under `hub.wire` |
| install worker | `http/handlers/fleets/create_install_steps.worker` (detached) | pool + queue during install | guarded by `install_wg` (`WaitGroup`) | teardown `install_wg.wait()` before pool/queue deinit |
| OTLP flush | `observability/otlp/exporter.flushLoop` | the export ring | atomic single-flight claim on `g_running` | flag → loop exits → joined |
| metrics runners | `observability/metrics_fleet.Runner.run` | metrics snapshot slots | per-runner slot claim | shutdown flag → exit |
| call-deadline watchdog | `lib/call_deadline` loop | the per-call deadline flag | atomic flag under the call's own lock | fires or the call completes; joined by the caller |

### `agentsfleet-runner` (execution plane)

Rooted at `src/runner/main.zig`, isolated from datastore code (enforced by
`_runner_isolation_check`).

| Thread | Spawned by | Touches | Protection | Stop path |
|---|---|---|---|---|
| execution workers (N) | `runner/daemon/worker_pool.workerLoop` | **no shared mutable state by construction** — each worker owns its lease/child | none needed (C5 by construction) | `stop_requested` / `drain_requested` flags → each drains its child → joined |
| netns setup | `runner/network/EgressScope` (`ChildNetnsSetup.run`) | the child's network namespace during setup | scoped to one child launch | joined before the child executes |

---

## Channel inventory

Cross-thread and cross-process channels, with SPSC roles and payload ownership.
Redis stream/channel **names** are canonical in `data_flow.md` §"Two streams + one
pub/sub channel"; the roles below are the concurrency view.

| Channel | Kind | Producer → Consumer | Payload ownership |
|---|---|---|---|
| in-proc event bus | bounded queue + `Condition` wakeup | request threads → the single bus thread | the bus thread frees each dequeued event (receiver-frees, C1) |
| subscription epoch/queue | per-subscription futex-epoch + bounded queue | the hub reader (producer) → one SSE stream thread (consumer) | the SSE consumer frees each frame it copies out (C1) |
| `fleet:{id}:events` | Redis stream + consumer group `fleet_lease` | steer/webhook/cron/continuation `XADD` → `agentsfleetd` non-blocking `XREADGROUP` per lease | durable; `XACK`ed at report, idempotent on replay |
| `fleet:{id}:activity` | Redis pub/sub (ephemeral) | `agentsfleetd` `PUBLISH` (+ runner-forwarded frames) → the hub's one shared `SUBSCRIBE` connection, fanned out by copy | ephemeral; each SSE stream owns its copied frame |
| `fleet:control` | **removed at the M80 cutover** | — | — |

The hub holds exactly **one** pub/sub connection for all viewers, refcounting
`SUBSCRIBE` per channel-with-viewers — the per-stream connections are gone
(`data_flow.md`). New cross-thread channels must be SPSC with a carried allocator
(C1); reshaping the existing ones is a separate judgment with this doc as input.

---

## Lock-invariant registry

Every mutex in the discipline base, exactly what it protects, and its ordering
constraint. Each is documented at its declaration (C4); the roster grep
(`test_base_mutexes_documented`) holds the count of declarations equal to the
count of invariant comments.

| Lock | Declared at | Protects | Ordering |
|---|---|---|---|
| `bus.mutex` | `events/bus.zig` | the in-proc event queue; orders a producer push against the consumer's predicate check | leaf — held alone |
| `subscription.mutex` | `events/subscription.zig` | the subscription epoch counter; the epoch is read under the lock before a futex sleep (bump-then-wake) | leaf — held alone |
| `hub.mutex` | `events/subscription_hub.zig` | the `channels → subscribers` map and **nothing else** | never held together with `hub.wire` — acquire one at a time |
| `hub.wire` | `events/subscription_hub.zig` | the one shared pub/sub connection and **all** wire sends (`SUBSCRIBE`/`UNSUBSCRIBE`) and teardown | never nested with `hub.mutex` |
| `WaitGroup.mutex` | `lib/common/sync.zig` | the counting barrier's `count`; `start`/`finish`/`wait` are all guarded | leaf — held alone |

The load-bearing ordering rule (the C3 fix that ended the hub's
blocking-write-under-the-map-mutex hazard): a wire send is bounded by a watchdog
and taken under `hub.wire` alone, never while `hub.mutex` is held.

---

## Shutdown choreography

The stop → join → deinit sequence (C2), including the ordering M126_001 corrected.

1. **Signal.** SIGTERM/SIGINT wakes the signal watcher, which sets the global
   shutdown flag. The watcher is armed only after the server is published, so a
   SIGTERM landing in the boot window no longer fires against a null server
   (the watcher re-arms and the server still stops).
2. **Observe.** Every background loop — the three sweepers, the outbound worker,
   the event bus — reads the shared flag and exits its loop; the sweeper
   round-waits re-arm so a signal mid-round is not lost.
3. **Join detached work first.** Teardown calls `install_wg.wait()` so any
   detached install worker has returned before the pool and queue it uses are
   deinited — no use-after-free against a shutting-down datastore.
4. **Tear streaming down under its own lock.** `deinitStreaming`
   (`serve_shutdown.zig`) stops the SSE hub: `stop()` runs the teardown under
   `hub.wire`, so no live stream thread can be mid-send when the connection is
   closed, and the `channels` map is freed only after the reader has joined —
   never under a straggler. The event bus `stop()` is ordered the same way.
5. **Free shared maps only after joins.** The hub/registry maps are freed only
   once their threads have joined; a bounded drain that logged an incomplete
   result never proceeds to free state a straggler could still reach.
6. **Datastores last.** The Postgres pool and Redis queue deinit after every
   thread that could touch them has joined.

The deterministic test handshake for these sequences is `common.Event`
(`sync.zig`) — `set()` on one side, bounded `timedWait()` on the other — so
lifecycle tests exercise the real stop→join order with no sleeps or polling races.

---

## Expanding the discipline base (roster)

The rules above are enforced in code across the folders listed in
`audits/zig-discipline-roster.txt` — the compliance base. Inside a roster prefix,
`lint-zig.py --discipline` blocks on a freeing deinit that omits its
`self.* = undefined` poison (A5) or an owned-slice pub fn that omits its ownership
phrase (A5); outside, the same findings warn.

**Adding the next folder is one line.** Append its path prefix to the roster, run
`make lint`, fix what the check surfaces, and commit — no code change is needed
for the scope to grow, because enforcement scope is data, not logic. Until a
folder joins the roster, RULE NLR (touch-it-fix-it) owns cleanup of its
individual files.
