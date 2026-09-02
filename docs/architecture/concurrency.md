# Concurrency architecture — threads, channels, locks, shutdown

Canonical concurrency model for `agentsfleetd` (control plane) and
`agentsfleet-runner` (execution plane). This is the file the `name_architecture`
dispatch consults before naming a thread, channel, or lock, or asserting a
shutdown ordering. Sibling of [`data_flow.md`](./data_flow.md) (the same runtime
traced per event) and [`runner_fleet.md`](./runner_fleet.md) (the control/execution
split). Channel and stream **names** are canonical in `data_flow.md`; this file
owns the thread/lock/shutdown layer on top of them.

The concurrency rules `C1–C5` are the system's concrete invariants and bind both
planes. Their statement beside the Allocator rules `A1–A6` lives in the Zig
discipline façade (`dispatch/write_zig.md`), which is what the runner's
compliance roster (`audits/zig-discipline-roster.txt`) expands against; the
control plane holds the same five in Rust, where the compiler carries three of
them.

---

## Facts

Every row is extracted from the sections below; the owner column names the section that carries the full story.

| Invariant | Value | Mechanism | Owner section |
|---|---|---|---|
| Concurrency rules | C1–C5 | declared producer/consumer, receiver owns the payload · stop→join→drop · no blocking under a consumer's lock · one documented lock per aggregate · task/thread-confined by default | §The five invariants |
| Long-lived work | 7 supervised control-plane tasks (8 with a span endpoint configured) + one per connection · 3 runner threads | each with a declared spawn point, protection, and stop path | §Thread map |
| Shutdown flags | none | the half-dead-node window the two flags protected is an ordering here, not shared mutable state | §Why there is no signal watcher |
| Registered locks | 8 | the pub/sub socket is behind none of them — it is owned by one task and reached by command | §Lock-invariant registry |
| Deadline ownership | runner: exactly one scheduler per process root · control plane: one deadline per call site | registrations target a connection *generation*, never a descriptor; arming is fail-CLOSED everywhere | §The deadline-ownership invariant |
| Shutdown order | decide why → cancel and join → drop | teardown is in the outer `run`, so every early return still tears down | §Shutdown choreography |
| Cancellation reach | every long-lived task selects its own I/O against the token | a genuinely blocked `accept()` is interrupted mid-read, not at the next poll interval | §Shutdown choreography |
| Test handshake | handshakes, never sleeps | channels and `CancellationToken` on the control plane, `common.Event` on the runner; `start_paused` pays no wall clock for a join timeout | §Shutdown choreography |
| Discipline scope | one roster line per folder | enforcement scope is data, not logic; RULE NLR owns files outside the roster | §Expanding the discipline base |

## Traps

Each trap is enforced in its owner section; this list is the index.

- Never `tokio::spawn` outside the supervisor or the accept loop's per-connection arm — a detached task outlives the pools it reads through (§Thread map).
- Never issue a `SUBSCRIBE`/`UNSUBSCRIBE` from anywhere but the pump task; a subscriber enqueues a command instead, and the enqueue is what may happen under the channel map's lock because it cannot block (§Lock-invariant registry).
- Never do a blocking wire send while holding the map lock — the C3 fix that ended the hub hazard (§Lock-invariant registry).
- A scheduler callback is a bounded, non-reentrant leaf — it must never call back into scheduler barriers (§Lock-invariant registry).
- Never free shared state before its tasks and threads have joined; a timed-out drain never proceeds to free (§Shutdown choreography).
- New cross-boundary channels declare one producer and one consumer, and the payload's ownership at the boundary; reshaping existing ones is a separate judgment with this doc as input (§Channel inventory).
- Channel and stream *names* are canonical in `data_flow.md`, not here (preamble).

## The five invariants (rules C1–C5)

1. **C1 — declared producer and consumer, receiver owns the payload.** Every
   channel that crosses a task or thread boundary has a single declared producer
   and a declared consumer, and ownership at the boundary is unambiguous: on the
   control plane the value moves to the receiver and the compiler is the proof;
   on the runner the payload carries its own allocator and the receiver frees it
   in a `defer` at the top of the handler. A channel read by several consumers
   hands each of them its own copy and tells a slow one what it missed.
2. **C2 — stop → join → drop.** Shutdown signals stop, joins the worker, and
   only then releases shared state. A bounded drain that times out never frees
   state a straggler can still touch.
3. **C3 — no blocking work under a lock the consumer needs.** A blocking socket
   write or push is never done while holding a lock the consumer must acquire to
   make progress. Lock state is an explicit parameter, not an ambient assumption.
4. **C4 — one documented lock per shared aggregate.** Each shared aggregate has
   exactly one lock whose doc comment states precisely what it protects and any
   ordering constraint, and the guard's scope is visible where it is taken.
5. **C5 — confined by default.** State touched by one task or thread carries no
   lock but says so; on the control plane an exclusive borrow is the statement,
   on the runner it is a `// only touched by thread X` comment plus
   `*Locked`-suffixed lock-required entry points.

The control plane's primitives are tokio's: `CancellationToken` for stop,
`tokio::select!` to race I/O against it, `tokio::time::timeout` for a deadline,
`broadcast` and `mpsc` for fan-out and commands, `tokio::sync::Mutex` where a
guard is held across an await and `std::sync::Mutex` where a leaf map is not.
The runner's are in [`lib/common/sync.zig`](../../src/lib/common/sync.zig):
`Mutex` (arg-free `lock`/`unlock` over `std.Io.Mutex`), `Condition`, a rebuilt
`WaitGroup`, and `Event` — the one-shot, poll-based replacement for the
`std.Thread.ResetEvent` that Zig 0.16 removed, used for deterministic
stop→join handshakes in lifecycle tests.

---

## Thread map

Every long-lived task and thread, who spawns it, the shared state it touches,
how that state is protected, and how it is stopped and joined.

### `agentsfleetd` (control plane)

Nothing runs outside the supervisor. `Supervisor::spawn`
(`rustd/crates/agentsfleetd/src/supervisor.rs`) is the only caller of
`tokio::spawn` in the daemon apart from the accept loop's per-connection arm and
the hub's own pump: it hands each task a `CancellationToken`, keeps its
`JoinHandle`, and `shutdown` consumes the supervisor so nothing a task borrowed
can be dropped until every handle has been joined. A task that will not stop
inside `JOIN_TIMEOUT` (10 s) is reported by name rather than hanging the process.

The inventory is asserted, not described: `test_boot_to_ready_on_compose`
(`rustd/crates/agentsfleetd/tests/integration_serve.rs`) compares a booted
daemon's whole inventory against the names below, so a task added to boot without
a name — or a sweeper that quietly went back to a bare spawn — is a failing test.

| Task | Spawned by | Touches | Protection | Stop path |
|---|---|---|---|---|
| accept loop (`accept_loop`) | `serve::listen` | the listener; the shared `Router` by clone | none shared mutable | `select!` over `token.cancelled()` and a genuinely blocked `accept()` → loop breaks → joined |
| connection (one per socket) | the accept loop | one connection's request stream | none shared mutable; the router is cloned per connection | the same token: `select!` over `cancelled()` and the served connection |
| SSE hub pump (`hub_pump`) | `afd_redis`'s `SubscriptionHub::start`; stopped by the supervised task `serve::spawn_background` registers | the one shared pub/sub connection + the `channels` map | it owns the socket outright — no lock; the map under its own mutex | the supervised task observes cancellation and calls `hub.shutdown()`, which clears the map so every reader is told; the pump returns when the last command sender drops |
| liveness sweeper (`sweeper:liveness`) | `sweepers::spawn` | Postgres through its own pool handle | none shared | `select!` over `cancelled()` and the interval sleep → loop breaks → joined |
| reclaim sweeper (`sweeper:reclaim`) | `sweepers::spawn` | Postgres + Redis, and the sweep's own keyset cursor | `Mutex<Cursor>` (leaf) | as above |
| retention sweeper (`sweeper:retention`) | `sweepers::spawn` | Postgres through its own pool handle | none shared | as above |
| repair-verification dispatcher (`sweeper:repair-verification`) | `sweepers::spawn` | Postgres + Redis, and its own pacing value | `Mutex<Duration>` (leaf) | as above |
| telemetry flush (`otlp_export`) | `serve::open_telemetry`, and only where an OTLP endpoint is configured — a deployment with none supervises the seven above and no more | the four SDK providers (tracer, cumulative and delta meters, logger); the exporting itself happens on the SDK's own batch threads and periodic readers, never on this task | the SDK's batch queues; spans and metric cycles it failed to deliver are counted on atomics | awaits cancellation, then `Exports::flush` force-flushes every provider before the pools they describe are dropped → joined |
| analytics flush (`analytics_flush`) | `serve::open`, last | the product-analytics client's queued events | none shared mutable | awaits cancellation, then flushes before the client is dropped |

A sweep that fails is reported and retried on the next pass: every pass here is
idempotent and bounded, and a sweeper that exited on a datastore blip would need
a daemon restart to get liveness back.

The fleet runtime's remaining background work — an in-process event bus, the
outbound connector worker, install-step workers, the signup metadata fetch —
arrives as supervised tasks with bounded drains when it arrives, because there is
no unsupervised spawn path here to arrive as anything else.

### `agentsfleet-runner` (execution plane)

Rooted at `src/runner/main.zig`, isolated from datastore code (enforced by
`_runner_isolation_check`).

| Thread | Spawned by | Touches | Protection | Stop path |
|---|---|---|---|---|
| execution workers (N) | `runner/daemon/worker_pool.workerLoop` | **no shared mutable state by construction** — each worker owns its lease/child; all workers borrow the ONE process scheduler | none needed (C5 by construction); scheduler access is internally locked | `stop_requested` / `drain_requested` flags → each drains its child → joined |
| deadline scheduler worker | `runner/daemon/runner_deadline.Owned.start` (M139) | the earliest-deadline `std.Treap` + registration map — never a socket | `scheduler.mutex`; interruption reaches a transport only through its owner's generation check | `stop()` refuses new arms, drains, quiesces callbacks; `deinit` joins **after** `runLoop` has joined every worker (LIFO defer in `main.zig`) |
| netns setup | `runner/network/EgressScope` (`ChildNetnsSetup.run`) | the child's network namespace during setup | scoped to one child launch | joined before the child executes |

---

## Channel inventory

Cross-task and cross-process channels, with producer/consumer roles and payload
ownership. Redis stream/channel **names** are canonical in
[`data_flow.md`](./data_flow.md) §"Two streams + one pub/sub channel"; the roles
below are the concurrency view.

| Channel | Kind | Producer → Consumer | Payload ownership |
|---|---|---|---|
| hub commands | unbounded `mpsc` | any subscriber or dropped `Subscription` → the one pump task | `Subscribe`/`Unsubscribe` moves to the pump; unbounded so the enqueue can happen under the channel map's lock without blocking |
| channel fan-out | `broadcast`, 256 messages per channel | the pump (producer) → every reader subscribed to that channel | each reader receives its own clone; a reader that falls 256 behind is told the count it missed rather than losing them silently (C1) |
| cancellation | `CancellationToken` | the supervisor → every supervised task and every live connection | edge-triggered; a task selects it against its own I/O, so it is interrupted mid-read |
| `fleet:{id}:events` | Redis stream + consumer group `fleet_lease` | steer/webhook/cron/continuation `XADD` → `agentsfleetd` non-blocking `XREADGROUP` per lease | durable; `XACK`ed at report, idempotent on replay |
| `fleet:{id}:activity` | Redis pub/sub (ephemeral) | `agentsfleetd` `PUBLISH` (+ runner-forwarded frames) → the hub's one shared `SUBSCRIBE` connection, fanned out by copy | ephemeral; each SSE stream owns its copied frame |
| `fleet:control` | **removed at the M80 cutover** | — | — |

The hub holds exactly **one** pub/sub connection for all viewers, refcounting
`SUBSCRIBE` per channel-with-viewers — the per-stream connections are gone
(`data_flow.md`), and `test_hub_refcount_single_connection` is what holds it. New
cross-boundary channels declare one producer and one consumer and say who owns
the payload (C1); reshaping the existing ones is a separate judgment with this
doc as input.

---

## Lock-invariant registry

Every lock in the discipline base, exactly what it protects, and its ordering
constraint. Each is documented at its declaration (C4); on the runner the roster
grep (`test_base_mutexes_documented`) holds the count of declarations equal to
the count of invariant comments.

| Lock | Declared at | Protects | Ordering |
|---|---|---|---|
| hub channel map | `afd_redis`'s `HubInner` | the `channel → (broadcast sender, reader count)` map and **nothing else** | leaf, and never held across an await — the only thing done under it is the command enqueue, which cannot block |
| runner series table | `afd_observability`'s `RunnerMetrics` | the `runner_id → counters` map, up to 4096 series | read lock for LOOKUP only; the counters are atomics incremented after the guard is released, so a slow recorder never blocks a fast one |
| reclaim cursor | `afd_runner`'s reclaim sweeper | the keyset cursor one pass resumes from | leaf — held alone, and only by the single sweeper task |
| repair pacing | `afd_runner`'s repair-verification dispatcher | the interval the dispatcher shortens while a backlog drains | leaf — held alone |
| JWKS cache | `afd_identity`'s `KeyCache` | the held key set (read/write lock) and the single-flight gate (mutex) | the flight gate is held across the fetch with the key-set lock **released**, so a cache hit never queues behind a slow provider |
| `WaitGroup.mutex` | `lib/common/sync.zig` | the counting barrier's `count`; `start`/`finish`/`wait` are all guarded | leaf — held alone |
| `scheduler.mutex` | `lib/call_deadline/scheduler.zig` | deadlines, registrations, lifecycle state, worker handle, identifier allocation | released around every target callback — a callback is a bounded, non-reentrant leaf that must never call back into scheduler barriers |
| `SocketOwner.mutex` | `lib/call_deadline/SocketOwner.zig` | generation, handle, and the interrupted flag together; held across the `shutdown(2)` so a completing attempt cannot swap in a recycled descriptor between check and syscall | leaf — held alone; taken from the scheduler worker inside a callback and from the owning caller, never nested with another lock |

The load-bearing ordering rule (the C3 fix that ended the hub's
blocking-write-under-the-map-mutex hazard): the pub/sub socket is behind no lock
at all. One task owns it, and a subscriber that wants a `SUBSCRIBE` or an
`UNSUBSCRIBE` enqueues a command. The enqueue happens while the channel map is
still locked, deliberately — that ordering is what stops an `Unsubscribe`
overtaking the `Subscribe` of a reader arriving on the same channel — and it is
safe only because the queue is unbounded and the send therefore cannot block.

### The deadline-ownership invariant (M139)

Every network call is bounded, and the two planes bound it in different places.
On the control plane the deadline is at the call site — a `tokio::time::timeout`
around the operation, and a `select!` against the cancellation token for anything
long-lived — so there is no shared registration map to keep consistent and no
generation check to get wrong. Postgres stays outside any scheduler on purpose:
the pool's acquire and connect timeouts already bound it.

The runner owns exactly **one** `ProcessScheduler`
(`daemon/runner_deadline.zig`) and passes it explicitly to every network owner —
there is no hidden global and no per-call watchdog thread. A registration
targets a `SocketOwner` **connection generation**, never a descriptor number:
the owner advances the generation before an attempt becomes interruptible and
validates it under its own lock at fire time, so a late fire against a replaced
connection returns `stale` and touches nothing. `Guard.finish()` and
`Scheduler.stop()` are quiescence barriers — after either returns, the selected
callbacks are neither running nor eligible to run, which is what makes a
stack-local owner safe to leave scope. Arming is fail-CLOSED: a scheduler that
cannot arm refuses the call; no path falls through to an unbounded run.

---

## Shutdown choreography

The stop → join → drop sequence (C2). Three steps on the control plane, and the
reduction is the result rather than the goal: most of a longer list is orderings
that a teardown had to hold by hand.

1. **Decide why.** `Daemon::run` awaits whichever of the server or the signal
   finishes first, and names it — `Signalled` or `ServerStopped`. Both are
   modelled because a daemon that waits solely for a signal hangs when its
   listener dies of something else: a lost bind, an accept loop that returned,
   a runtime that shut its I/O driver. That process is unkillable except by
   SIGKILL and reports nothing on its way out. The `select!` is `biased`, so the
   answer is a fact about the futures rather than about tokio's branch order.
2. **Cancel and join, unconditionally.** The teardown is in the outer `run`, not
   inside the loop — exonum's `ApiManager::run`/`run_inner` split, taken for its
   one property: every early return still tears down. `Supervisor::shutdown`
   cancels the token, then joins every handle with a `JOIN_TIMEOUT` deadline,
   reporting any task that would not stop by name instead of hanging the process.
   Cancellation is edge-triggered and reaches into a blocked read, so a task is
   interrupted where it is waiting rather than at the next poll interval —
   proven for a real blocked `accept()` by `test_task_inventory_and_cancellation`,
   which carries a control so the negative is not vacuous. Streaming stops here
   too: `hub.shutdown()` clears the channel map, so a reader parked on the hub
   gets a hub-closed error it can act on rather than waiting on a socket nobody
   is pumping.
3. **Drop last.** `shutdown` consumes the supervisor, so what the tasks borrowed
   cannot be dropped until it returns, and the pools are dropped by the caller
   after that. Invariant C2 becomes a borrow-checker fact rather than a teardown
   ordering — asserted as an observation by `Arc::strong_count` after teardown in
   `test_shutdown_joins_all_tasks`.

On the runner the same three steps are a LIFO defer chain at the root: the
scheduler is constructed after — and therefore torn down before — anything that
still arms into it, and `runner/main.zig` deinits it after `runLoop` has joined
every worker. `Scheduler.stop()` rejects new arms, interrupts and drains pending
registrations, and waits for in-flight callbacks; network users then finish their
guards before their owners deinit, and only then does scheduler storage deinit.

Handshakes, not sleeps, in every one of these tests. The runner's is
`common.Event` (`sync.zig`) — `set()` on one side, bounded `timedWait()` on the
other; the control plane's are channels and `CancellationToken`. The
abandoned-task assertions run under `#[tokio::test(start_paused)]`, so a
ten-second join timeout costs no wall clock: with every task parked the runtime
advances to the next deadline itself.

### Why there is no signal watcher and no shutdown flags

Two flags kept apart — a raw signal flag and a background-stop flag — protect one
window: a SIGTERM arriving during boot must not kill the background stack while
the server may still come up and briefly serve. That is the half-dead-node
window, and it needs two flags only where a watcher thread polls, because there
"the signal arrived" and "the server stopped" are events that genuinely race.

They cannot race in `Daemon::run`, because they are statements in order: await
whichever of server-or-signal finishes first, then cancel the supervisor, then
let the caller drop the pools. A signal during boot leaves an already-resolved
future; the server comes up, sees it resolved, and stops. Same property, one less
piece of shared mutable state, and no 100 ms of shutdown latency paid on every
task. `test_boot_window_sigterm` fails if the `select!` arms are swapped.

### Why the control plane has no central deadline scheduler

A treap-backed registration map and a worker thread exist so one thread can
interrupt another's blocked socket, and the runner needs exactly that. On the
control plane `tokio::time::timeout` at the call site is the same guarantee with
no shared map to keep consistent and no generation check to get wrong, and
`CancellationToken` is edge-triggered, so a task selecting over its own I/O and
`cancelled()` is interrupted mid-read.

---

## Expanding the discipline base (roster)

The rules above are enforced in code across the folders listed in
`audits/zig-discipline-roster.txt` — the compliance base. Inside a roster prefix,
`lint-zig.py --discipline` blocks on a freeing deinit that omits its
`self.* = undefined` poison (A5) or an owned-slice pub fn that omits its ownership
phrase (A5); outside, the same findings warn.

**Adding the next folder is one line.** Append its path prefix to the roster, run
`make lint-all`, fix what the check surfaces, and commit — no code change is needed
for the scope to grow, because enforcement scope is data, not logic. Until a
folder joins the roster, RULE NLR (touch-it-fix-it) owns cleanup of its
individual files.
