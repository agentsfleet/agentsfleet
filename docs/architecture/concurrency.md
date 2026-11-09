# Concurrency architecture — threads, channels, locks, shutdown

Canonical concurrency model for `agentsfleetd` (control plane) and
`agentsfleet-runner` (runner plane). Consult this page before naming a
thread, channel, or lock, or asserting a shutdown ordering. It is a sibling of
[`data_flow.md`](./data_flow.md) (the same runtime traced per event) and
[`runner_fleet.md`](./runner_fleet.md) (the control/runner split).

Channel and stream **names** are canonical in `data_flow.md`; this page
owns the thread/lock/shutdown layer on top of them.

The concurrency rules `C1–C5` are the product's concrete invariants and bind both
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
| Shared-state locks | inventoried below | the NATS client sits behind the adapter's read/write lock: operations share read guards, and only shutdown takes the write guard | §Lock-invariant registry |
| Deadline ownership | one deadline per call site, on both planes | `tokio::time::timeout` or the HTTP client's own timeout; no shared registration map | §The deadline-ownership invariant |
| Shutdown order | decide why → cancel and join → drop | teardown is in the outer `run`, so every early return still tears down | §Shutdown choreography |
| Cancellation reach | every long-lived task selects its own I/O against the token | a genuinely blocked `accept()` is interrupted mid-read, not at the next poll interval | §Shutdown choreography |
| Test handshake | handshakes, never sleeps | channels, `Notify` and `CancellationToken` on both planes; `start_paused` pays no wall clock for a join timeout | §Shutdown choreography |
| Setup chat claim | never waits on a row lock | the creator and draft rows are taken `NOWAIT` in a savepoint; a held one skips that draft for the next | §Runner capacity and authoring lock order |
| Discipline scope | no roster | C1–C5 hold at review; RULE NLR owns cleanup of the files a change touches | §Expanding the discipline base |

## Traps

Each trap is enforced in its owner section; this list is the index.

- Every spawned task needs a lifetime owner and stop path, including request-owned private preview publishers (§Thread map).

- Never open a second native subscription for a subject that already has local readers. The first reader opens it and starts its pump; later readers join the channel and wait until that subscription is queued (§Lock-invariant registry).

- Never await a round trip on a subject's pump. It forwards every frame for that subject, so a wait there holds them all (§Thread map).

- Never do a blocking wire send while holding the map lock. That is the C3 fix that ended the hub hazard (§Lock-invariant registry).

- Never free shared state before its tasks and threads have joined; a timed-out drain never proceeds to free (§Shutdown choreography).

- New cross-boundary channels declare one producer and one consumer, and the payload's ownership at the boundary. Reshaping existing ones is a separate judgment with this page as input (§Channel inventory).

- Channel and stream *names* are canonical in `data_flow.md`, not here (preamble).

- Never let a setup chat claim wait on a row while it holds the host row. Every other tenant on that poll would wait behind it. Take the row `NOWAIT` and skip (§Runner capacity and authoring lock order).

## The five invariants (rules C1–C5)

1. **C1: declared producer and consumer, receiver owns the payload**. Every
   channel that crosses a task or thread boundary has a single declared producer
   and a declared consumer. Ownership at the boundary is unambiguous: on both
   planes the value moves to the receiver and the compiler is the proof.

   A channel read by several consumers hands each of them its own copy and tells a slow one what it missed.
2. **C2: stop → join → drop.** Shutdown signals stop, joins the worker, and
   only then releases shared state. A bounded drain that times out never frees
   state a straggler can still touch.
3. **C3: no blocking work under a lock the consumer needs**. A blocking socket
   write or push is never done while holding a lock the consumer must acquire to
   make progress. Lock state is an explicit parameter, not an ambient assumption.
4. **C4: one documented lock per shared aggregate**. Each shared aggregate has
   exactly one lock whose doc comment states precisely what it protects and any
   ordering constraint. The guard's scope is visible where it is taken.
5. **C5: confined by default**. State touched by one task or thread carries no lock but says so. On both planes an exclusive borrow is the statement.

The control plane's primitives are tokio's. `CancellationToken` stops,
`tokio::select!` races I/O against it, and `tokio::time::timeout` bounds a
deadline. `broadcast` and `mpsc` carry fan-out and commands.

`tokio::sync::Mutex` guards state held across an await and `std::sync::Mutex`
a leaf map that is not.

The runner uses the same primitives: `CancellationToken` for stop, and a `watch` channel for the heartbeat's assignment. The runner also uses a `JoinSet` for its workers (`afr_supervisor`'s worker pool).

---

## Thread map

Every long-lived task and thread, who spawns it, the shared state it touches,
how that state is protected, and how it is stopped and joined.

### `agentsfleetd` (control plane)

The supervisor in the daemon binary (`rustd/crates/daemon/main`) owns process-lifetime daemon tasks. Connection handlers, hub workers and bounded request-owned work have their own owners.

`Supervisor::spawn` supplies a cancellation token and retains each handle. Shutdown waits for each handle and reports tasks exceeding the ten-second join deadline. The supervised `hub_pump` shuts down the hub and its listeners.

Private answer previews use a request-owned publisher (`afd_authoring`'s preview publisher). Its `AbortOnDropHandle` aborts the task on cancellation or unwind; normal completion allows a two-second drain. Publication rechecks access before sending each preview.

The inventory is asserted, not described. `test_boot_to_ready_on_compose`
(the daemon's `integration_serve` test) compares a booted
daemon's whole inventory against the names below. A task added to boot without
a name, or a sweeper that quietly went back to a bare spawn, is a failing test.

| Task | Spawned by | Touches | Protection | Stop path |
|---|---|---|---|---|
| accept loop (`accept_loop`) | `serve::listen` | the listener; the shared `Router` by clone | none shared mutable | `select!` over `token.cancelled()` and a genuinely blocked `accept()` → loop breaks → joined |
| connection (one per socket) | the accept loop | one connection's request stream | none shared mutable; the router is cloned per connection | the same token: `select!` over `cancelled()` and the served connection |
| Server-Sent Events (SSE) response body | HTTP handler | stream permit and hub subscription receivers | owned by the body; no dedicated operating-system thread | dropping the body releases its permit and receivers; hub closure ends a fleet tail |
| outbound answer worker (`connector:outbound`) | `agentsfleetd::outbound::spawn` | claims lane heads from `fleet.obligations` on an `af.outbound.wake` ping, on a finished lane, or every `CLAIM_PASS` (30 s), and hands each row to the outbound lanes | `FOR UPDATE SKIP LOCKED` and the holder's token in Postgres; no in-process lock shared with other tasks | `select!` over `cancelled()`, the ping, a finished lane and the pass sleep → loop breaks → the lanes drain → joined |
| NATS hub pump (one per subject with readers, `afd_bus::nats`'s hub) | `Bus::subscribe`, for a subject's first local reader | that subject's one native subscription: frames out to every local reader, and an `Af-Gap` marker turned into `Gap` | the channel map under its own mutex; the pump never takes it to forward a frame | the last reader's drop aborts the pump, which unsubscribes; `Bus::shutdown` aborts and joins every pump |
| NATS gap watcher (`afd_bus::nats`'s signals watcher) | `afd_bus::nats::connect`, once per process | client events (reconnect, slow consumer), the publish debt, and the `af.bus.gap` broadcast | the debt under a short mutex the client's event callback also takes; the callback only sets flags and wakes the watcher | `Bus::shutdown` aborts and joins it before the client drains; the supervised `hub_pump` calls that shutdown on cancellation |
| runner wake listeners (sixteen, `afd_bus::wake`) | `Wake::listen`, from the gateway scheduler (`afd_gateway/src/scheduler/subscriptions.rs`) | one `af.wake.<shard>` subscription each; one shared queue of at most 64 fleet IDs by default | leaf mutex; never held across an await | the stop token, or a closed subscription; the scheduler then resubscribes with backoff |
| liveness sweeper (`sweeper:liveness`) | `sweepers::spawn` | Postgres through its own pool handle | none shared | `select!` over `cancelled()` and the interval sleep → loop breaks → joined |
| retention sweeper (`sweeper:retention`) | `sweepers::spawn` | Postgres through its own pool handle | none shared | as above |
| fleet census sweeper (`sweeper:fleet-census`) | `sweepers::spawn` | Postgres through its own pool handle, reads only; publishes into the `agentsfleet_fleets` snapshot cells | none shared | as above |
| authoring, billing, task and task-storage sweepers (`sweeper:authoring`, `sweeper:billing`, `sweeper:tasks`, `sweeper:task-storage`) | the daemon's sweepers module | Postgres through its own pool handle | none shared | as above |
| telemetry flush (`otlp_export`) | `serve::open_telemetry`, and only where an OpenTelemetry Protocol (OTLP) endpoint is configured; a normal boot without one supervises twelve tasks | the four software development kit (SDK) providers (tracer, cumulative and delta meters, logger); the exporting itself happens on the SDK's own batch threads and periodic readers, never on this task | the SDK's batch queues; spans and metric cycles it failed to deliver are counted on atomics | samples the resident-set gauge on each collection tick until cancelled, then `telemetry::flush_within` force-flushes every provider before the pools they describe are dropped → joined |
| analytics flush (`analytics_flush`) | `serve::open`, last | the product-analytics client's queued events | none shared mutable | awaits cancellation, then flushes before the client is dropped |

A sweep that fails is reported and retried on the next pass. Every pass here is idempotent and bounded. A sweeper that exited on a datastore blip would need a daemon restart to get liveness back.

New process-lifetime background work belongs to the supervisor. Request-owned work must retain an abortable handle and a bounded completion path.

### `agentsfleet-runner` (runner plane)

Rooted at the runner binary (`rustd/crates/runner/main`); no runner crate links a
datastore crate ([`runner_execution.md`](./runner_execution.md#crates)).
`afr_supervisor::run` composes the long-lived work. The private report spool protects shared clock and deadline state with its own mutex.

| Task | Spawned by | Touches | Protection | Stop path |
|---|---|---|---|---|
| heartbeat | `afr_supervisor::run`, beside both drains and the pool under one `tokio::join!` | the assignment it publishes | a `watch` channel; no lock | the halt token → returns → joined |
| runtime report spool drain | `afr_supervisor::run` | the storage home's runtime reports | file work on the blocking pool | the halt token → returns → joined |
| private report spool drain | `afr_supervisor::run` | encrypted authoring answers and separate accounting envelopes | private spool state mutex; filesystem work on the blocking pool | serving halt ends the drain; startup cleanup precedes workers |
| workers (N) | `worker_pool::serve`, one `JoinSet` | each worker owns its lease and that lease's sandbox | none shared mutable (C5); which fleet is busy belongs to one coordinator task, reached by message | leasing stops → polling stops → leases in flight run to their reports → the set drains |
| hold keeper | `Holds::start` | the frozen sandboxes held for their fleet's next lease | owned by the keeper task, reached by message | `holds.shutdown()` after the join destroys every hold |
| executor link (one per sandbox) | `afr_executor`'s client | one sandbox's executor connection | owned by the link task | ends with its sandbox |

---

## Channel inventory

Cross-task and cross-process channels, with producer/consumer roles and payload
ownership. NATS subject **names** are canonical in
[`data_flow.md`](./data_flow.md) §"Two streams + one pub/sub channel"; the roles
below are the concurrency view.

| Channel | Kind | Producer → Consumer | Payload ownership |
|---|---|---|---|
| client events | the `async-nats` event callback | the NATS client → the gap watcher | the callback sets a flag per cause (reconnect, slow consumer) and wakes the watcher, so a burst coalesces into one gap per cause and the callback never waits on I/O |
| channel fan-out | `broadcast`, 256 messages per channel (`Tuning::channel_capacity`) | the subject's pump (producer) → every local reader of that subject | every reader receives the same `Arc<Message>`, so a frame is one allocation however many watch it; a reconnect, a slow consumer or a gap marker is a `Gap` item; a reader that falls 256 behind is told the count it missed rather than losing them silently (C1) |
| cancellation | `CancellationToken` | the supervisor → every supervised task and every live connection | edge-triggered; a task selects it against its own I/O, so it is interrupted mid-read |
| `fleet.admissions` | PostgreSQL queue | producers commit admissions; lease polls claim the oldest unsettled row per eligible fleet | slot and admission fences; reports settle in the usage transaction |
| `fleet.obligations` | PostgreSQL rows claimed per lane | the report path writes the row in its transaction → any replica's outbound worker claims the lane head | durable; the claim's token names the holder, and a lapsed holder's late write matches no row |
| `af.outbound.wake` | core NATS subject (at most once) | the report path after its commit → every replica's outbound worker | the payload names the obligation, but the worker reads only the ping and claims from Postgres; a lost ping waits for the next 30 s claim pass |
| `af.fleet.<fleet_id>.tail` | core NATS subject (at most once) | `agentsfleetd` publishers (+ runner-forwarded frames) → each process's one native subscription per subject, fanned out by reference | ephemeral; every SSE stream shares the published frame, and the response body writes its payload from that one allocation |
| `af.wake.<shard>` (sixteen) | core NATS subject | admission and gate-answer commits publish fleet IDs to every replica | ephemeral; every poll still probes PostgreSQL |

Each process holds exactly **one** NATS connection for all viewers and
publishers, with one native subscription per subject that has local readers
(`rustd/crates/daemon/afd_bus/src/nats/hub/mod.rs`).
`readers_share_one_channel_and_one_payload` and
`concurrent_openers_make_exactly_one_pump` hold it.

New cross-boundary channels declare one producer and one consumer and say who owns
the payload (C1). Reshaping the existing ones is a separate judgment with this
page as input.

SSE response bodies share the hub; they do not each open a NATS connection or reserve a thread.
The daemon listener supports HTTP/1.1 and h2c independently of the browser-facing connection.
[Data Flow, section D (WATCH)](./data_flow.md#d-watch--user-side-how-the-live-tail-surfaces) owns the Next.js proxy and per-hop protocol description.

---

## Lock-invariant registry

Every lock in the discipline base, exactly what it protects, and its ordering
constraint. Each is documented at its declaration (C4).

| Lock | Declared at | Protects | Ordering |
|---|---|---|---|
| bus adapter state | `afd_bus::nats`'s `Adapter::live` (tokio `RwLock`) | the live client, channel map, signals and watcher, or `None` after shutdown | every publish and subscribe holds a read guard for its own call; `shutdown` takes the write guard, so nothing opens after the drain begins |
| hub channel map | `afd_bus::nats`'s hub `Channels` | the `subject → (broadcast sender, reader count, generation, pump)` map and **nothing else** | leaf mutex, never held across an await; the work under it is a map edit, a pump abort, or a broadcast send for a gap, none of which can block |
| publish debt | `afd_bus::nats`'s `Signals` | the subjects published inside the 2 s debt window, the subjects owed an `Af-Gap` marker, and the overflow flag, each bounded at 4,096 subjects | leaf mutex; taken by the publish path, the client's event callback and the watcher, never across an await |
| outbound lane map | `afd_outbound`'s `lanes::Inner` | destination membership and publication against lane retirement | reserve capacity before locking; verify channel identity and publish under one guard; retirement checks for queued work and removes an empty lane under that same guard; no await while held |
| runner series table | `af_observability`'s `RunnerMetrics` | the `runner_id → counters` map, up to 4096 series | read lock for LOOKUP only; the counters are atomics incremented after the guard is released, so a slow recorder never blocks a fast one |
| wake queue | `afd_bus::wake::Wake` | at most 64 distinct recent fleet IDs by default (`AGENTSFLEET_RUNNER_WAKE_QUEUE_CAPACITY`) | leaf mutex; no database or network calls while held |
| metric instruments | `af_observability`'s instrument registry | the family names already claimed, and the observable gauge handles kept alive | leaf, held alone |
| Capability caches | `afd_identity`'s `ProviderCapabilities` | separate Moka stores for fresh claims and last confirmed claims; each is bounded to 4096 subjects | conditional per-key invalidation precedes `try_get_with`, which coalesces concurrent fetches; failed refreshes retain the original stale timestamp, and unknown subjects clear the fallback |
| JSON Web Key Set (JWKS) cache | `afd_identity`'s `KeyCache` | the held key set (read/write lock) and the single-flight gate (mutex) | the flight gate is held across the fetch with the key-set lock **released**, so a cache hit never queues behind a slow provider |
| run ledger | `afr_agent`'s `Ledger` | one run's open calls and the counter they share | taken to open a call and again to end it, never across its handler, so a child's call can open and end inside its parent's `delegate` |
| child registry | `afr_agent`'s nested `Registry` | one run's children, and the running and started counts that bound them | leaf, held alone |
| process events | `afr_executor`'s events `Shared` | one process's output state, shared by its two ends | leaf, held alone |
| admitted toolboxes | `afr_sandbox`'s `Toolboxes` | the images one host has admitted, oldest first | leaf, held alone |
| lease memory | `afr_tools`'s `Lease::memory` (tokio) | the fleet's memory backend for one lease | held for one read or write, never across a call |
| lease egress | `afr_tools`'s `Lease::egress` (tokio) | the lease's policy and the credentials it has minted | held to admit or mask, never across a send |
| exec sessions | `afr_tools`'s `Sessions` | one lease's open sessions and the places reserved for those still starting | leaf; each session's process sits behind its own tokio lock, held by the one call reading it |
| mirror locks | `afr_supervisor`'s `Mirrors` | the map of one lock per repository mirror | held only to fetch that mirror's tokio lock, which is held across the mirror's fetch and checkout |
| run clock and lease timeline | `afr_supervisor`'s `RunClock` and `Timeline` | the clock one run's frames are read against, and one lease's segment timings | each a leaf, held alone |
| private report state | `afr_supervisor`'s authoring spool | clock high-water mark, uncertainty flag and monotonic report deadlines | shared by writes and cleanup; filesystem work runs on the blocking pool |
| egress host lock | `afr_sandbox`'s egress `HELD` | the host egress lock file this process holds once taken, shared by every engine it builds | leaf, held alone |

The load-bearing ordering rule is C3: no wire send happens under the channel
map's lock. `Channels::open` hands the pump half to a subject's first reader.
That reader calls the NATS client's `subscribe` only after the lock is released.

A later reader of the same subject waits on the channel's `subscribed` signal.
It returns only once the first reader's subscription is queued on the
connection. A first reader that fails drops the signal, and its joiners fail
with it (`rustd/crates/daemon/afd_bus/src/nats/mod.rs`).

### The deadline-ownership invariant

Every network call is bounded, and both planes bound it at the call site. On the
control plane that is a `tokio::time::timeout` around the operation, and a
`select!` against the cancellation token for anything long-lived. So there is no
shared registration map to keep consistent and no generation check to get wrong.

Postgres stays outside any scheduler on purpose. The pool's acquire and connect
timeouts bound the connection. The API login's lock and statement bounds cover
every statement (§"Every daemon statement runs under the API login's bounds").

The runner bounds each call in the HTTP client that makes it:

- the control-plane client's `CALL_TIMEOUT` (`afr_supervisor`);
- the egress transport's request and connect timeouts (`af_egress`);
- the model providers' connect and read timeouts (`af_providers`).

---

## Shutdown choreography

The stop → join → drop sequence (C2). Three steps on the control plane, and the
reduction is the result rather than the goal. Most of a longer list is orderings
that a teardown had to hold by hand.

1. **Decide why.** `Daemon::run` awaits whichever of the server or the signal
   finishes first, and names it `Signalled` or `ServerStopped`. Both are
   modelled because a daemon that waits solely for a signal hangs when its
   listener dies of something else. That includes a lost bind, an accept loop
   that returned and a runtime that shut its I/O driver.

   That process is unkillable except by SIGKILL and reports nothing on its way
   out. The `select!` is `biased`, so the answer is a fact about the futures
   rather than about tokio's branch order.

2. **Cancel and join, unconditionally.** The teardown is in the outer `run`, not
   inside the loop. That is exonum's `ApiManager::run`/`run_inner` split, taken
   for its one property: every early return still tears down.

   `Supervisor::shutdown` cancels the token, then joins every handle with a
   `JOIN_TIMEOUT` deadline. It reports any task that would not stop by name
   instead of hanging the process.

   Cancellation is edge-triggered and reaches
   into a blocked read. A task is interrupted where it is waiting rather
   than at the next poll interval.

   `test_task_inventory_and_cancellation` proves that for a real blocked
   `accept()`, and carries a control so the negative is not vacuous.

   Streaming stops here too. `Bus::shutdown` drains the channel map and aborts
   every pump, so a reader parked on a subscription gets `Closed`, which it can
   act on. The client drains only after every pump has joined.

3. **Drop after shutdown returns.** The caller drops its pools after `Supervisor::shutdown` finishes.
   Successful joins establish stopped tasks; timeout records an abandoned task and drops its join handle (the daemon binary's supervisor).
   Dropping that handle detaches the task, so timeout does not prove every task stopped.

A detached task retains its owned references until it ends. The intended C2 discipline therefore requires tasks to observe cancellation before the join deadline. Treat an abandoned-task report as incomplete shutdown, even though the supervisor returns.

The runner joins its heartbeat, runtime report drain, private report drain and worker pool (`afr_supervisor::run`). Private spool cleanup completes before those components start. After they return, shutdown destroys every held sandbox.

Workers finish their current lease before leaving the pool. Lease deadlines, renewal refusals and the running halt token can interrupt that work.

Handshakes, not sleeps, in every one of these tests: channels, `Notify` and
`CancellationToken` on both planes. The abandoned-task assertions run under
`#[tokio::test(start_paused)]`, so a ten-second join timeout costs no wall
clock. With every task parked the runtime advances to the next deadline itself.

### Why there is no signal watcher and no shutdown flags

Two flags kept apart, a raw signal flag and a background-stop flag, protect one
window. A SIGTERM arriving during boot must not kill the background stack while
the server may still come up and briefly serve. That is the half-dead-node
window.

It needs two flags only where a watcher thread polls. There "the signal arrived" and "the server stopped" are events that genuinely race.

They cannot race in `Daemon::run`, because they are statements in order. The
daemon awaits whichever of server-or-signal finishes first, then cancels the
supervisor, then lets the caller drop the pools. A signal during boot leaves an
already-resolved future; the server comes up, sees it resolved, and stops.

Same property, one less piece of shared mutable state, and no 100 ms of shutdown
latency paid on every task. `test_boot_window_sigterm` fails if the `select!`
arms are swapped.

### Why neither plane has a central deadline scheduler

A treap-backed registration map and a worker thread exist so one thread can
interrupt another's blocked socket. On tokio, `tokio::time::timeout` or a
client's own timeout at the call site is the same guarantee. There is no shared
map to keep consistent and no generation check to get wrong.

`CancellationToken` is edge-triggered, so a task selecting over its own I/O and
`cancelled()` is interrupted mid-read.

---

## Expanding the discipline base (roster)

**No check mechanises C1–C5 beyond the compiler.** The compiler carries three
of the five on both planes; the other two hold at review.

**There is no roster to append to.** Every folder under `rustd/` is in scope and
none is checked by a roster. RULE NLR (touch-it-fix-it) owns cleanup of the
individual files a change touches.

**A mechanical half is real work, not a line of data.** It needs three things.
A checker reads `rustd/`, a make target invokes it, and a Continuous
Integration (CI) job runs the target.

## Runner capacity and authoring lock order

All claims hold the runner row while counting live capacity and writing a lease. Both renewal kinds hold it before extending deadlines and sample database time after acquiring it. Runtime renewal takes runner → lease/affinity; its funds check reads the wallet without a lock.

A setup chat renewal takes draft → attempt → runner instead (`afd_authoring`'s lease renew). It samples database time again after the runner lock. A busy draft therefore keeps the renewal waiting without holding the runner's host row.

The claim is the one path that holds a host and then asks for a draft. It never waits for the draft (below).

Retiring a revoked runner (`afd_runner`'s `delete_revoked`) is the one opposite order left. Its delete holds the runner row while `authoring.attempts.runner_id`'s `ON DELETE SET NULL` locks that runner's attempts.

So a renewal in flight can deadlock with that delete. Postgres aborts one side within `deadlock_timeout`. A revoked runner's renewal is refused by `can_renew` either way, and an aborted delete is retried.

Runtime reports take the lease row and never the runner, authoring or wallet rows. Only the billing relay writes a wallet row, in its own transactions. It stamps the lines first and debits the wallet last, so the row is locked only from the debit to the commit.

Commit dispatch intent before network I/O, and release every database lock before provider calls. Unknown provider usage counts against nothing until the replay that learns it logs its lines.

Verify two-replica expiry races and lock-order timeouts before claiming these guarantees.

No setup chat path locks the wallet row. The claim gate reads the balance less every unsettled line, without a lock (`afd_authoring`'s model budget, `afd_billing`'s `select_balance.sql`). The only authoring wallet lock left is fleet creation's at Save (`afd_authoring`'s creation `wallet.sql`).

Stop and delete take draft → attempt and never later take the runner guard.

### Every daemon statement runs under the API login's bounds

The API login carries two bounds through `ALTER ROLE` (`afd_db::login`). A lock wait ends after 2 s with `55P03`, and a statement after 5 s with `57014`. They reach every statement `agentsfleetd` sends, because the daemon opens one pool as that login.

No product transaction sets its own with `set_config`. The four that did were removed on Nov 07, 2026; each repeated or loosened the login's bounds.

Both codes mean "not now", so one predicate reads them (`afd_db::error::is_bound_reached`). A caller answers a reached bound as retryable unavailability. The fleet task paths answer 503, billing counts it as an unanswered read, and setup activation reads it as still installing.

The decision sweep defers a candidate whose fleet row is locked to its next pass, which `PASS_TIMEOUT` still caps.

### The setup chat claim never waits on a row

A claim walks up to 64 queued drafts, oldest first, one transaction per draft (`afd_authoring`'s lease claim). Each locks the host row, then the creator's user row, then the draft.

Every draft writer takes that draft lock too: a sent message, an edit, an applied revision, a review. A claim that waited on a busy draft held its host row throughout, and every tenant behind that draft waited with it:

```
BEFORE — the claim waits                      AFTER — the claim skips
                                              
 runner R polls                                runner R polls
   │ lock host row R                             │ lock host row R
   ├─ draft A1 (tenant A) ── held by a writer    ├─ draft A1 ── held? NOWAIT refuses
   │     ⏳ waits until the writer commits        │     ↩ savepoint rolls back, A1 skipped
   │     (host row R held, ~1 s per write)       ├─ draft B1 (tenant B) ── free
   ├─ draft B1 (tenant B) ── never reached       │     ✔ admitted, lease issued
   │     until A's write ends                    └─ commit; A1 is admitted on a later
   └─ every poll repeats the wait                    poll, once its writer lets go
```


`authorize` takes the creator's row `FOR SHARE NOWAIT`, then the draft's `FOR UPDATE NOWAIT`, inside a savepoint (`afd_authoring`'s claim selection). That is the order the reads after it lock them. The statements are `hold_user.sql` and `hold_draft.sql`.

A refused lock raises `55P03 lock_not_available`. Rolling the savepoint back keeps the claim's transaction, and its host lock, usable for the next candidate. A released savepoint keeps its row locks until the claim commits, so the reads after it never wait.

The skip logs `authoring_claim_draft_skipped` at `debug`, because under contention it fires on every poll of the walk. A skipped draft is passed over, not dropped. Its order by age resumes on the first poll after the writer commits.

`make bench-lease BENCH_SHAPE=saturation` measures it with 40 hosts on a 20-connection pool. One writer holds a tenant's drafts 1 s of every 1.25 s. Medians of three runs: open-transaction age p95 959 → 17 ms, chat lease p95 3 006 → 707 ms, 41.7 → 62.7 leases/s.

No lease was doubled and no host exceeded its workers either way. The full row is in [Measured ceilings](./scaling.md#measured-ceilings).

The proofs are `afd_authoring`'s `skipping` admission tests. A claim can still be a deadlock victim on a lock it does wait for. That lock is the catalogue its model read shares (the `revalidation` admission tests).
