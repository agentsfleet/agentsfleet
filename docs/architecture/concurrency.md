# Concurrency architecture — threads, channels, locks, shutdown

Canonical concurrency model for `agentsfleetd` (control plane) and
`agentsfleet-runner` (execution plane). This is the file the `name_architecture`
dispatch consults before naming a thread, channel, or lock, or asserting a
shutdown ordering. Sibling of [`data_flow.md`](./data_flow.md) (the same runtime
traced per event) and [`runner_fleet.md`](./runner_fleet.md) (the control/execution
split). Channel and stream **names** are canonical in `data_flow.md`; this file
owns the thread/lock/shutdown layer on top of them.

The concurrency rules `C1–C5` are the system's concrete invariants and bind both
planes. Both planes are Rust on tokio, where the compiler carries three of the
five.

---

## Facts

Every row is extracted from the sections below; the owner column names the section that carries the full story.

| Invariant | Value | Mechanism | Owner section |
|---|---|---|---|
| Concurrency rules | C1–C5 | declared producer/consumer, receiver owns the payload · stop→join→drop · no blocking under a consumer's lock · one documented lock per aggregate · task/thread-confined by default | §The five invariants |
| Long-lived work | supervised control-plane tasks, async connections, and runner tasks | each with a declared spawn point, protection, and stop path | §Thread map |
| Shutdown flags | none | the half-dead-node window the two flags protected is an ordering here, not shared mutable state | §Why there is no signal watcher |
| Registered locks | 14 | the pub/sub socket is behind none of them — it is owned by one task and reached by command | §Lock-invariant registry |
| Deadline ownership | one deadline per call site, on both planes | `tokio::time::timeout` or the HTTP client's own timeout; no shared registration map | §The deadline-ownership invariant |
| Shutdown order | decide why → cancel and join → drop | teardown is in the outer `run`, so every early return still tears down | §Shutdown choreography |
| Cancellation reach | every long-lived task selects its own I/O against the token | a genuinely blocked `accept()` is interrupted mid-read, not at the next poll interval | §Shutdown choreography |
| Test handshake | handshakes, never sleeps | channels, `Notify` and `CancellationToken` on both planes; `start_paused` pays no wall clock for a join timeout | §Shutdown choreography |
| Discipline scope | no roster | C1–C5 hold at review; RULE NLR owns cleanup of the files a change touches | §Expanding the discipline base |

## Traps

Each trap is enforced in its owner section; this list is the index.

- Never `tokio::spawn` outside the supervisor or the accept loop's per-connection arm — a detached task outlives the pools it reads through (§Thread map).
- Never issue an `SSUBSCRIBE`/`SUNSUBSCRIBE` from anywhere but the hub's control task; a subscriber enqueues a command and the dispatch task sends a signal instead, and the enqueue is what may happen under the channel map's lock because it cannot block (§Lock-invariant registry).
- Never await a round trip on the hub's dispatch task; a subscribe that waits there holds every frame behind it (§Thread map).
- Never do a blocking wire send while holding the map lock — the C3 fix that ended the hub hazard (§Lock-invariant registry).
- Never free shared state before its tasks and threads have joined; a timed-out drain never proceeds to free (§Shutdown choreography).
- New cross-boundary channels declare one producer and one consumer, and the payload's ownership at the boundary; reshaping existing ones is a separate judgment with this doc as input (§Channel inventory).
- Channel and stream *names* are canonical in `data_flow.md`, not here (preamble).

## The five invariants (rules C1–C5)

1. **C1 — declared producer and consumer, receiver owns the payload.** Every
   channel that crosses a task or thread boundary has a single declared producer
   and a declared consumer, and ownership at the boundary is unambiguous: on both
   planes the value moves to the receiver and the compiler is the proof. A
   channel read by several consumers
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
   lock but says so; on both planes an exclusive borrow is the statement.

The control plane's primitives are tokio's: `CancellationToken` for stop,
`tokio::select!` to race I/O against it, `tokio::time::timeout` for a deadline,
`broadcast` and `mpsc` for fan-out and commands, `tokio::sync::Mutex` where a
guard is held across an await and `std::sync::Mutex` where a leaf map is not.
The runner uses the same primitives: `CancellationToken` for stop, a `watch`
channel for the heartbeat's assignment, and a `JoinSet` for its workers
(`rustd/crates/afr_supervisor/src/worker_pool.rs`).

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
| SSE response body | HTTP handler | stream permit and hub subscription receivers | owned by the body; no dedicated operating-system thread | dropping the body releases its permit and receivers; hub closure ends a fleet tail |
| outbound answer worker (`connector:outbound`) | `agentsfleetd::outbound::spawn` when its dedicated Dragonfly connection opens | blocking stream reader and shared writers | one reader owns its socket; it cannot block the shared command connection | cancellation races the read; supervisor joins the worker |
| SSE hub control task (`hub/pump.rs`) | `afd_dragonfly`'s `SubscriptionHub::start`; stopped by the supervised task `serve::spawn_background` registers | the one shared pub/sub connection's commands, its redial, and a node repair's re-subscribes | it owns the connection outright — no lock; the map under its own sharded lock | the supervised task observes cancellation and calls `hub.shutdown()`, which clears the map so every reader is told; the task returns when the last command sender drops, aborting the dispatch task on its way out |
| SSE hub dispatch task (`hub/dispatch.rs`) | the control task, once per connection | the connection's pushes: frames out to readers, confirmations into gaps, a lost node's attribution | reads the `channels` map; never touches the connection, so no round trip can hold a frame | aborted by the control task when its connection is redialled or the hub drops |
| liveness sweeper (`sweeper:liveness`) | `sweepers::spawn` | Postgres through its own pool handle | none shared | `select!` over `cancelled()` and the interval sleep → loop breaks → joined |
| reclaim sweeper (`sweeper:reclaim`) | `sweepers::spawn` | Postgres + Dragonfly, and the sweep's own keyset cursor | `Mutex<Cursor>` (leaf) | as above |
| retention sweeper (`sweeper:retention`) | `sweepers::spawn` | Postgres through its own pool handle | none shared | as above |
| fleet census sweeper (`sweeper:fleet-census`) | `sweepers::spawn` | Postgres through its own pool handle, reads only; publishes into the `agentsfleet_fleets` snapshot cells | none shared | as above |
| repair-verification dispatcher (`sweeper:repair-verification`) | `sweepers::spawn` | Postgres + Dragonfly, and its own pacing value | `Mutex<Duration>` (leaf) | as above |
| telemetry flush (`otlp_export`) | `serve::open_telemetry`, and only where an OTLP endpoint is configured — a normal boot without one supervises eight tasks | the four SDK providers (tracer, cumulative and delta meters, logger); the exporting itself happens on the SDK's own batch threads and periodic readers, never on this task | the SDK's batch queues; spans and metric cycles it failed to deliver are counted on atomics | awaits cancellation, then `Exports::flush` force-flushes every provider before the pools they describe are dropped → joined |
| analytics flush (`analytics_flush`) | `serve::open`, last | the product-analytics client's queued events | none shared mutable | awaits cancellation, then flushes before the client is dropped |

A sweep that fails is reported and retried on the next pass: every pass here is
idempotent and bounded, and a sweeper that exited on a datastore blip would need
a daemon restart to get liveness back.

The fleet runtime's remaining background work — an in-process event bus,
install-step workers, the signup metadata fetch —
arrives as supervised tasks with bounded drains when it arrives, because there is
no unsupervised spawn path here to arrive as anything else.

### `agentsfleet-runner` (execution plane)

Rooted at `rustd/crates/agentsfleet_runner/src/main.rs`; no runner crate links a
datastore crate ([`runner_execution.md`](./runner_execution.md#crates)).
`afr_supervisor::run` composes the long-lived work, and no state is shared behind
a lock between those tasks (`rustd/crates/afr_supervisor/src/lib.rs`).

| Task | Spawned by | Touches | Protection | Stop path |
|---|---|---|---|---|
| heartbeat | `afr_supervisor::run`, beside the drain and the pool under one `tokio::join!` | the assignment it publishes | a `watch` channel; no lock | the halt token → returns → joined |
| report spool drain | `afr_supervisor::run` | the storage home's `spool/` | file work on the blocking pool | the halt token → returns → joined |
| workers (N) | `worker_pool::serve`, one `JoinSet` | each worker owns its lease and that lease's sandbox | none shared mutable (C5); which fleet is busy belongs to one coordinator task, reached by message | leasing stops → polling stops → leases in flight run to their reports → the set drains |
| hold keeper | `Holds::start` | the frozen sandboxes held for their fleet's next lease | owned by the keeper task, reached by message | `holds.shutdown()` after the join destroys every hold |
| executor link (one per sandbox) | `afr_executor`'s client | one sandbox's executor connection | owned by the link task | ends with its sandbox |

---

## Channel inventory

Cross-task and cross-process channels, with producer/consumer roles and payload
ownership. Dragonfly stream/channel **names** are canonical in
[`data_flow.md`](./data_flow.md) §"Two streams + one pub/sub channel"; the roles
below are the concurrency view.

| Channel | Kind | Producer → Consumer | Payload ownership |
|---|---|---|---|
| hub commands | unbounded `mpsc` | any subscriber or dropped `Subscription` → the hub's control task | `Subscribe`/`Unsubscribe` moves to the control task; unbounded so the enqueue can happen under the channel map's lock without blocking |
| hub signals | unbounded `mpsc` | the hub's dispatch task → its control task | `Resubscribe` (a slot moved), `Repair` (a node's socket was lost: every live channel), `Redial` (a loss the window did not explain); unbounded so a push is never held behind the control task's round trips |
| channel fan-out | `broadcast`, 256 messages per channel | the dispatch task (producer) → every reader subscribed to that channel | every reader receives the same `Arc<Message>`, so a frame is one allocation however many watch it; a lost subscription is a `Gap` item once it is back; a reader that falls 256 behind is told the count it missed rather than losing them silently (C1) |
| cancellation | `CancellationToken` | the supervisor → every supervised task and every live connection | edge-triggered; a task selects it against its own I/O, so it is interrupted mid-read |
| `fleet:{id}:events` | Dragonfly stream + consumer group `fleet_lease` | steer/webhook/cron/continuation `XADD` → `agentsfleetd` non-blocking `XREADGROUP` per lease | durable; `XACK`ed at report, idempotent on replay |
| `connector:outbound` | Dragonfly stream + consumer group | report producer → dedicated blocking outbound reader | durable queue; worker acknowledges delivered jobs |
| `fleet:{id}:activity` | Dragonfly sharded pub/sub (ephemeral) | `agentsfleetd` `SPUBLISH` (+ runner-forwarded frames) → the hub's one shared connection, one `SSUBSCRIBE` per channel, fanned out by reference | ephemeral; every SSE stream shares the published frame, and the response body writes its payload from that one allocation |
| `fleet:control` | **removed at the M80 cutover** | — | — |

The hub holds exactly **one** pub/sub connection for all viewers, refcounting
`SSUBSCRIBE` per channel-with-viewers — the per-stream connections are gone
(`data_flow.md`), and `test_hub_refcount_single_connection` is what holds it. New
cross-boundary channels declare one producer and one consumer and say who owns
the payload (C1); reshaping the existing ones is a separate judgment with this
doc as input.

SSE response bodies share the hub; they do not each open a Dragonfly connection or reserve a thread.
The daemon listener supports HTTP/1.1 and h2c independently of the browser-facing connection.
[Data Flow, D. WATCH](./data_flow.md#d-watch--user-side-how-the-live-tail-surfaces) owns the Next.js proxy and per-hop protocol description.

---

## Lock-invariant registry

Every lock in the discipline base, exactly what it protects, and its ordering
constraint. Each is documented at its declaration (C4).

| Lock | Declared at | Protects | Ordering |
|---|---|---|---|
| hub channel map | `afd_dragonfly`'s `HubInner` | the `channel → (broadcast sender, reader count, confirmed once)` map and **nothing else** | leaf, sharded per key, and never held across an await — the only things done under it are the command enqueue and a broadcast send (a frame, or a gap on a repeated confirmation), neither of which can block |
| runner series table | `afd_observability`'s `RunnerMetrics` | the `runner_id → counters` map, up to 4096 series | read lock for LOOKUP only; the counters are atomics incremented after the guard is released, so a slow recorder never blocks a fast one |
| reclaim cursor | `afd_runner`'s reclaim sweeper | the keyset cursor one pass resumes from | leaf — held alone, and only by the single sweeper task |
| repair pacing | `afd_runner`'s repair-verification dispatcher | the interval the dispatcher shortens while a backlog drains | leaf — held alone |
| Capability caches | `afd_identity`'s `ProviderCapabilities` | separate Moka stores for fresh claims and last confirmed claims; each is bounded to 4096 subjects | conditional per-key invalidation precedes `try_get_with`, which coalesces concurrent fetches; failed refreshes retain the original stale timestamp, and unknown subjects clear the fallback |
| JWKS cache | `afd_identity`'s `KeyCache` | the held key set (read/write lock) and the single-flight gate (mutex) | the flight gate is held across the fetch with the key-set lock **released**, so a cache hit never queues behind a slow provider |
| run ledger | `afr_agent`'s `Ledger` | one run's open calls and the counter they share | taken to open a call and again to end it, never across its handler, so a child's call can open and end inside its parent's `delegate` |
| child registry | `afr_agent`'s nested `Registry` | one run's children, and the running and started counts that bound them | leaf — held alone |
| process events | `afr_executor`'s events `Shared` | one process's output state, shared by its two ends | leaf — held alone |
| admitted toolboxes | `afr_sandbox`'s `Toolboxes` | the images one host has admitted, oldest first | leaf — held alone |
| lease memory | `afr_tools`'s `Lease::memory` (tokio) | the fleet's memory backend for one lease | held for one read or write, never across a call |
| lease egress | `afr_tools`'s `Lease::egress` (tokio) | the lease's policy and the credentials it has minted | held to admit or mask, never across a send |
| exec sessions | `afr_tools`'s `Sessions` | one lease's open sessions and the places reserved for those still starting | leaf; each session's process sits behind its own tokio lock, held by the one call reading it |
| mirror locks | `afr_supervisor`'s `Mirrors` | the map of one lock per repository mirror | held only to fetch that mirror's tokio lock, which is held across the mirror's fetch and checkout |

The load-bearing ordering rule (the C3 fix that ended the hub's
blocking-write-under-the-map-mutex hazard): the pub/sub socket is behind no lock
at all. The control task owns it, and a subscriber that wants an `SSUBSCRIBE`
or an `SUNSUBSCRIBE` enqueues a command. The enqueue happens while the channel map is
still locked, deliberately — that ordering is what stops an `Unsubscribe`
overtaking the `Subscribe` of a reader arriving on the same channel — and it is
safe only because the queue is unbounded and the send therefore cannot block.

### The deadline-ownership invariant

Every network call is bounded, and both planes bound it at the call site. On the
control plane that is a `tokio::time::timeout` around the operation, and a
`select!` against the cancellation token for anything long-lived, so there is no
shared registration map to keep consistent and no generation check to get wrong.
Postgres stays outside any scheduler on purpose: the pool's acquire and connect
timeouts already bound it.

The runner bounds each call in the HTTP client that makes it: the control-plane
client's `CALL_TIMEOUT` (`rustd/crates/afr_supervisor/src/client/http.rs`), the
egress transport's request and connect timeouts
(`rustd/crates/afr_egress/src/network.rs`), and the model providers' connect and
read timeouts (`rustd/crates/afr_providers/src/connect.rs`).

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

On the runner the same three steps are `afr_supervisor::run`. The heartbeat, the
spool drain and the worker pool run under one `tokio::join!` until the halt token
fires, and leases in flight run to their reports. Then every held sandbox is
destroyed, and only after that does `run` return
(`rustd/crates/afr_supervisor/src/lib.rs`).

Handshakes, not sleeps, in every one of these tests: channels, `Notify` and
`CancellationToken` on both planes. The abandoned-task assertions run under `#[tokio::test(start_paused)]`, so a
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

### Why neither plane has a central deadline scheduler

A treap-backed registration map and a worker thread exist so one thread can
interrupt another's blocked socket. On tokio, `tokio::time::timeout` or a
client's own timeout at the call site is the same guarantee with no shared map
to keep consistent and no generation check to get wrong, and
`CancellationToken` is edge-triggered, so a task selecting over its own I/O and
`cancelled()` is interrupted mid-read.

---

## Expanding the discipline base (roster)

**No check mechanises C1–C5 beyond the compiler.** The compiler carries three
of the five on both planes; the other two hold at review.

**There is no roster to append to.** Every folder under `rustd/` is in scope and
none is checked by a roster, and RULE NLR (touch-it-fix-it) owns cleanup of the
individual files a change touches.

**A mechanical half is a real piece of work, not a line of data.** It means a
checker that reads `rustd/`, a make target that invokes it, and a Continuous
Integration (CI) job that runs the target.
