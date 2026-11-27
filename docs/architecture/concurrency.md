---
type: reference
audience: contributor
verified: 2026-11-27
product_version: 0.90.0
executable: false
---

# Concurrency

## What it is

This page lists every long-lived task, cross-task channel and lock in `agentsfleetd` and `agentsfleet-runner`.
It also gives the five rules that bind them, the shutdown order, and the row-lock order of daemon SQL statements.
Read it before adding a task, a channel or a lock.

Both binaries are Rust on tokio, an asynchronous runtime.
[Data flow](./data_flow.md) owns channel and NATS subject names; this page owns who owns them and how they stop.

## Why it exists

A task with no owner leaks, and a lock taken in two orders deadlocks.
Listing each one in one place lets a reviewer check a new one against the rest.

## How it behaves

### The five rules

1. **C1: one producer, one consumer, and the receiver owns the payload.** A channel with several consumers gives each a copy.
   It tells a slow consumer what it missed.
2. **C2: stop, then join, then drop.** Shutdown signals stop, waits for the task, and only then frees shared state.
3. **C3: no blocking work under a lock the consumer needs.** A socket write never happens while holding such a lock.
4. **C4: one documented lock per shared aggregate.** Its doc comment says what it protects and any ordering rule.
5. **C5: confined by default.** State one task touches carries no lock, and an exclusive borrow says so.

The compiler proves C1's ownership move, C2's no-free-while-borrowed, and C5's exclusive borrow.
C3 and C4 hold at review; no checker enforces them.

The primitives are tokio's on both planes.
`CancellationToken` stops a task, `tokio::select!` races I/O against it, and `tokio::time::timeout` bounds a call.
`broadcast` and `mpsc` carry fan-out and commands.

`tokio::sync::Mutex` guards state held across an await; `std::sync::Mutex` guards a leaf map that is not.
The runner adds a `watch` channel for the heartbeat's assignment and a `JoinSet` for its workers.

### Daemon tasks

`Supervisor::spawn` in the daemon binary gives each process-lifetime task a cancellation token and keeps its handle.
`test_boot_to_ready_on_compose` compares a booted daemon's task inventory against `BACKGROUND_TASKS` (`daemon/main/src/inventory.rs`).
The daemon runs no timed loop; every task waits on a socket, a bus message, a runner connection or cancellation.

| Task | Spawned by | Shared state and its guard | Stop path |
|---|---|---|---|
| `accept_loop` | `serve::listen` | The listener; the router is cloned per connection | `select!` on the token and a blocked `accept()`, then joined |
| One connection per socket | The accept loop | Its own request stream | The same token |
| SSE response body | An HTTP handler | A stream permit and hub receivers, owned by the body | Dropping the body releases both |
| `connector:outbound` | `outbound::spawn` | Lane heads of `thread.deliveries`, under a per-lane advisory lock, `SKIP LOCKED` and the holder's token | `select!` on the token, a ping and a finished lane; lanes drain, then joined |
| NATS hub pump, one per subject with readers | `Bus::subscribe`, for the first local reader | The subject's one native subscription | The last reader's drop aborts it; `Bus::shutdown` joins every pump |
| NATS gap watcher | `afd_bus::nats::connect` | Client events and the publish debt, under a short mutex | `Bus::shutdown` joins it before the client drains |
| `bus_shutdown` | The supervisor | The NATS adapter | Calls `Bus::shutdown` on cancellation |
| Runner wake listener | `Wake::listen`, from the gateway scheduler | One `af.thread.*.wake` subscription and a queue of 64 fleets | The stop token, or a closed subscription, after which the scheduler resubscribes |
| `runner_stream_scheduler` | `serve::runner_stream::spawn` | The connection registry; Postgres through the API pool | A child token, then joined |
| `runner_stream_listener` | `serve::runner_stream::spawn` | The runner listener and one gateway connection per runner | Stops accepting, then joined |
| `otlp_export` | `serve::open_telemetry`, only when an OpenTelemetry Protocol (OTLP) endpoint is set | The telemetry providers' batch queues | Force-flushes every provider, then joined |
| `analytics_flush` | `serve::open`, last | The product-analytics client's queue | Flushes on cancellation |

New process-lifetime work belongs to the supervisor.
Request-owned work keeps an abortable handle and a bounded completion path.

### Runner tasks

`afr_supervisor::run` composes the connection, heartbeat policy and worker pool.
No runner crate links a datastore crate ([runner crates](./runner_execution.md#crates)).

| Task | Spawned by | Shared state and its guard | Stop path |
|---|---|---|---|
| Native connection | `afr_supervisor::run` | Four streams and request correlation, behind bounded channels and permits | Stays up while accepted workers settle; then joined |
| Heartbeat policy | `afr_supervisor::run` | The current assignment, in a `watch` channel | Halt, then joined |
| Workers (N) | `worker_pool::serve`, one `JoinSet` | Each owns its run and sandbox (C5); one coordinator task tracks busy fleets | New runs stop; accepted work gets 30 s, cleanup 5 s, then abort and join |
| Park keeper | `Parks::start` | Parked sandboxes kept for a fleet's next run, owned by the task | `parks.shutdown()` after the join destroys every park |
| Warm-slot keeper | `afr_engine`'s `WarmSlots::start`, on a Firecracker runner | The microVMs booted ahead of their runs, owned by the task; a run claims one over a channel | `WarmSlots::shutdown` stops refilling, waits for starts in flight and destroys every unclaimed slot |
| Executor link, one per sandbox | `afr_executor`'s client, over a socket pair or vsock | One sandbox's executor connection; it dials again when a call finds the link closed | Ends with its sandbox |

Workers keep unacknowledged reports in memory; no report is written to disk.

### Channels

| Channel | Kind | Producer to consumer | Payload |
|---|---|---|---|
| Client events | `async-nats` event callback | The NATS client to the gap watcher | The callback sets a flag per cause and wakes the watcher; a burst becomes one gap |
| Subject fan-out | `broadcast`, 256 messages (`Tuning::channel_capacity`) | A subject's pump to every local reader | Each reader gets the same `Arc<Message>`; a reader 256 behind is told how many it missed |
| Cancellation | `CancellationToken` | The supervisor to every task and connection | Edge-triggered, so it interrupts a blocked read |
| Turn queue | Postgres rows, `thread.turns` | Intake to assignment passes | Thread and turn fences; the report settles the turn |
| Delivery queue | Postgres rows, `thread.deliveries` | The report to any replica's outbound worker | The claim token names the holder |
| NATS subjects | Core NATS, at most once | Daemon publishers to each process's hub | Lossy; [Datastores](./datastore_scaling.md#when-a-signal-is-lost) names each subject's backstop |

Each process holds one NATS connection for every publisher and viewer, with one native subscription per subject that has local readers.
`readers_share_one_channel_and_one_payload` and `concurrent_openers_make_exactly_one_pump` prove it (`afd_bus/src/nats/hub/mod.rs`).

### Locks

Each lock is documented where it is declared (C4).

| Lock | Declared at | Protects | Ordering |
|---|---|---|---|
| Bus adapter state | `afd_bus::nats`'s `Adapter::live`, tokio `RwLock` | The live client, channel map and watcher | Each publish and subscribe holds a read guard; only shutdown takes the write guard |
| Hub channel map | `afd_bus::nats`'s hub `Channels` | The subject-to-sender map only | Leaf; never held across an await |
| Publish debt | `afd_bus::nats`'s `Signals` | Subjects published in the last 2 s and those owed a gap, up to 4,096 | Leaf; never held across an await |
| Outbound lane map | `afd_outbound`'s `lanes::Inner` | Lane membership against lane retirement | Reserve capacity before locking; no await while held |
| Runner series table | `af_observability`'s `RunnerMetrics` | Up to 4,096 per-runner series | Read lock for lookup; counters are atomics bumped after release |
| Wake queue | `afd_bus::wake::Wake` | Up to 64 recent fleet ids (`AGENTSFLEET_RUNNER_WAKE_QUEUE_CAPACITY`) | Leaf; no I/O while held |
| Metric instruments | `af_observability`'s instrument registry | Claimed family names and live gauge handles | Leaf |
| Capability caches | `afd_identity`'s `ProviderCapabilities` | Two Moka caches of 4,096 subjects each | `try_get_with` coalesces concurrent fetches |
| JSON Web Key Set (JWKS) cache | `afd_identity`'s `KeyCache` | The key set (`RwLock`) and a single-flight gate (mutex) | The gate is held across a fetch with the key-set lock released |
| Run ledger | `afr_agent`'s `Ledger` | One run's open calls | Taken to open and to end a call, never across its handler |
| Child registry | `guffbolt`'s `agents::registry` `Shared::book` | One run's child agents and their counts | Leaf |
| Process events | `afr_executor`'s events `Shared` | One process's output state | Leaf |
| Verified toolboxes | `afr_image`'s `Toolboxes` | Images one host has verified | Leaf |
| Run memory | `afr_tools`'s `RunMemory`, tokio `RwLock` | The fleet's memory for one run | Held for one read or write |
| Run egress | `afr_tools`'s `Run::egress`, tokio mutex | The run's network policy and minted credentials | Held to admit or mask, never across a send |
| Exec sessions | `guffbolt`'s `tools::process::Sessions` | One run's open shell sessions | Leaf; each session's process has its own lock |
| Mirror locks | `afr_supervisor`'s `Mirrors` | One lock per repository mirror | The map lock fetches the mirror's lock, which spans fetch and checkout |
| Run clock, run timeline | `afr_supervisor`'s `RunClock`, `Timeline` | One run's frame clock and segment timings | Leaf |
| Egress host lock | `afr_firewall`'s egress `HELD` | The host egress lock file | Leaf |
| Memory admission | `afr_engine_firecracker`'s `Admission`, a tokio `Semaphore` | One permit per microVM under `sandboxes_max` | `try_acquire_owned`, never awaited; a `Stop` park or a destroy drops the permit |
| Workspace image lock | `afr_engine_firecracker`'s `ImageLock`, the kernel's `flock` | One workspace image while its monitor runs | Taken without waiting before the drive is attached; a second attach is refused |

C3's load-bearing case is the hub.
A subject's first reader subscribes only after the map lock is released.

Later readers wait on that channel's `subscribed` signal until the subscription is queued.
If the first reader fails, its joiners fail with it.

### Deadlines

Every network call is bounded at its call site, by `tokio::time::timeout` or the client's own timeout.
There is no shared deadline map.

Postgres is bounded by the pool's acquire and connect timeouts and by the API login's bounds below.
The runner bounds calls with `CALL_TIMEOUT` (`afr_supervisor`), the egress transport's timeouts (`af_egress`) and the provider clients' timeouts (`af_providers`).

The daemon arms only one-shot timers, each from one event.
When a runner connection ends with no successor, `afd_gateway`'s `connection/departure/mod.rs` arms two.

One requests a recovery pass at the latest deadline of the runs it held.
The other runs `afd_runner`'s silence statement after the offline window, which records `runner_offline` and cordons the runner.
Boot arms one more, at the latest outstanding run deadline (`afd_gateway`'s `scheduler/lapse/mod.rs`).

### Shutdown order

1. **Decide why.** `Daemon::run` awaits whichever finishes first, the server or a signal, with a `biased` `select!`.
   A daemon waiting only on a signal would hang when its listener dies.
   `test_boot_window_sigterm` fails if the two arms are swapped.
2. **Cancel and join.** The teardown lives in the outer `run`, so every early return still tears down.
   `Supervisor::shutdown` cancels the token and joins each handle within a ten-second `JOIN_TIMEOUT`, naming any task that would not stop.
   `Bus::shutdown` aborts every pump so a parked reader gets `Closed`; the client drains after the pumps join.
3. **Drop.** The caller drops its pools after `Supervisor::shutdown` returns.

A task that misses the join deadline is detached and reported as abandoned.
It keeps its references until it ends, so treat that report as an incomplete shutdown.

The runner joins its heartbeat and worker pool, then destroys every parked sandbox.
Tests use handshakes, not sleeps; `#[tokio::test(start_paused)]` makes the join timeout cost no wall-clock time.
`test_task_inventory_and_cancellation` proves cancellation interrupts a blocked `accept()`.

### Row-lock order

Each write takes its row locks in one order, so two writes queue instead of deadlocking.
A fleet's writers take the fleet `KEY SHARE`, then the thread, then turns, runs or gates.

| Operation | Locks, in order | Statement |
|---|---|---|
| Stop | Fleet `KEY SHARE`, thread `NO KEY UPDATE`; then turn, run, the turn's gate | `afd_turns`'s `view/stop/hold_thread.sql`, `stop_turn.sql` |
| Run issue | Fleet `KEY SHARE`, runner, thread, head turn `FOR SHARE` at the fence | `afd_fleet`'s `run/sql/run/insert_run_with_turn.sql` |
| Fold retry | Fleet `KEY SHARE`, thread `NO KEY UPDATE`, run `FOR SHARE`, the folded turns | `afd_fleet`'s `run/sql/run/assign_inputs.sql` |
| Gate answer | Fleet `KEY SHARE`, thread `NO KEY UPDATE`, gate, turns | `afd_approval`'s `sql/resolve_gate.sql` |
| Fleet `PATCH`, archive | Fleet `FOR UPDATE` | `afd_setup`'s `creation/sql/lock.sql` |
| Fleet delete | Fleet `FOR UPDATE`, then a cascade to gates and thread | `afd_fleet_lifecycle`'s `sql/delete_fleet.sql` |
| Assignment | Runner row, thread, earlier run, head turn | `afd_fleet`'s `run/sql/run/assign_turn_head.sql` |
| Renewal batch | Runner `FOR SHARE`, then each run's thread and row in fleet order | `afd_fleet`'s `run/renew/batch/` |
| Report | Run row; usage lines, daily rollups in key order, then the wallet last | `afd_fleet`'s `run/sql/commit_report.sql` and the `billing.usage_lines` triggers |
| Refused-run usage | Run `FOR NO KEY UPDATE`, wallet, then the durable task | `afd_fleet`'s `run/sql/original_request_owner.sql` |
| Decision reservation | Workspace, fleet `KEY SHARE`, then thread, run, task and source turn `FOR SHARE` | `afd_fleet`'s `durable_task/decision/sql/issue.sql` |
| Memory capture, grant change | Fleet `FOR NO KEY UPDATE` | `afd_fleet`'s `run/fence/lock_memory_fleet.sql`, `afd_memory`'s `lock_capture.sql`, `lock_access.sql` |
| Schedule create or edit | Fleet `KEY SHARE`, then an advisory lock on `uuid_hash(fleet_id)` | `afd_cron`'s `lock_fleet.sql`, `lock_fleet_in_workspace.sql` |
| Revoked runner retire | Runner row; foreign keys clear its runs and threads | `afd_runner`'s `delete_runner_if_in_state.sql` |

Stop and a gate answer take the fleet before the thread, because delete holds the fleet `FOR UPDATE` and cascades to the thread.
Taking the thread first would meet delete in the opposite order (`afd_approval`'s `integration_resolve_behind_stop.rs`).

A gate answer holds the thread for its one statement even when it loses.
An assignment pass skips any held thread; the turn waits for the fleet's next wake.

Every item insert and schedule insert checks its foreign key with `FOR KEY SHARE` on the fleet row.
Only `FOR UPDATE` blocks that, so memory capture takes `FOR NO KEY UPDATE`.
`a_capture_and_a_tool_post_of_one_run_both_commit` proves a capture and a tool-call post both commit.

Renewals never wait on each other, because they hold the runner row only shared.
A run issue holds it exclusively, so it and a renewal statement wait for each other.

The renewal funds check reads the wallet without a lock, once per payer every third renewal (`FUNDS_CHECK_TICKS`).
A decision reservation does not repeat the money reads.
Two overlapping reservations of one fleet can each pass a money check by one call's reservation.

### Every daemon statement runs under the API login's bounds

The API login carries two bounds through `ALTER ROLE` (`afd_db/src/login.rs`).
A lock wait ends after 2 s with `55P03`, and a statement after 5 s with `57014`.
The daemon opens one pool as that login, so both bounds reach every statement.

No transaction sets its own bounds with `set_config`.
`afd_db::error::is_bound_reached` reads both codes as "not now", and callers answer with a retryable unavailability.

## Limits

- No checker enforces C3 or C4; review does.
- A detached task after a join timeout may still hold references when the process exits.
- Two-replica expiry races and lock-wait timeouts are not measured.
- A reservation's money check can overshoot by one call per concurrent reservation.

## Related

- [Data flow](./data_flow.md): channel and subject names, and the live tail.
- [Datastores](./datastore_scaling.md): what each NATS subject costs when lost.
- [Runner fleet](./runner_fleet.md): runner capacity and departure.
- [Runner runtime](./runner_execution.md): the runner's process model.
- [Billing](./billing.md): the usage line triggers and wallet debit.
