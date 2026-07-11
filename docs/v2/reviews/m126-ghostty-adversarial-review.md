# M126 adversarial review — concurrency, Allocator, crash management: ghostty vs agentsfleet

**Date:** Jul 11, 2026 · **Method:** five parallel review agents — two adversarial passes over
`src/` (races, leaks), three practice-mining passes over `~/Projects/oss/ghostty/` (checkout
8cddd384, Mar 2026) covering Allocator discipline, concurrency design, and crash management.
Every finding below was verified by reading the full surrounding function, not grep alone.
**Baseline at review time:** `unit=2500 integration=299` test blocks across 411 wired files
(`make _lint_zig_test_depth`, `main` @ `3b1719a4b`).

This document is the canonical findings record for milestone M126 (workstreams 001–003).
Severity tiers: **P0** broken now · **P1** fails under plausible timing · **P2** fragile/latent.

---

## 1. Race findings (agentsfleetd + runner)

| ID | Sev | Location | Defect |
|----|-----|----------|--------|
| R1 | P1 | `src/agentsfleetd/cmd/serve.zig:294-331`, `cmd/serve_shutdown.zig:39-44` | SIGTERM landing between `installSignalHandlers` and `publishServer` makes the one-shot `signalWatcher` fire against a null server and exit permanently; all five background loops (sweepers, outbound worker) observe the same flag and exit while `srv.listen()` keeps serving. Later SIGTERMs re-store an already-true flag — nothing stops the server; systemd escalates to SIGKILL; leases never reclaim. |
| R2 | P1 | `src/agentsfleetd/events/subscription_hub.zig:167,195` | `sendSubscribe`/unsubscribe write to a blocking Transport Layer Security (TLS) socket **while holding the hub mutex**. Zero `SO_SNDTIMEO` in `src/`. A non-reading Redis peer fills the kernel send buffer → one Server-Sent Events (SSE) attach blocks the reader thread, every subscribe/unsubscribe, and `stop()`; httpz pool pins; daemon sheds 429s until TCP keepalive (~60s+) kills the connection. The module's own comment (`resubscribeAll`, :279-284) acknowledges the hazard for the resubscribe path only. |
| R3 | P1 | `src/agentsfleetd/http/handlers/fleets/create_install_steps.zig:92-96`, `handlers/common.zig:121-127` | Detached install worker wakes at +250–650ms and calls `pool.acquire()` / `publishInstallStep(job.queue, …)` on a pool/queue that graceful shutdown **does** deinit (`cmd/serve.zig:128,143`). The guarding WaitGroup is only ever set by `http/test_harness.zig:232`; the comment claiming production "exits without a graceful pool teardown" is wrong. Use-after-free during shutdown. |
| R4 | P2 | `subscription_hub.zig:96-105` | `stop()` deinits the TLS connection **without the mutex** after a 5s drain that can time out (`hub_stop_undrained`), racing a live stream thread's locked `unsubscribe`. |
| R5 | P2 | `subscription_hub.zig:216-234` vs `:167/:195` | One `std.crypto.tls.Client` read by the reader thread (no lock) and written by request threads (under mutex). Not documented thread-safe; the header's "heals through reconnect" claim is an untested load-bearing assumption. |
| R6 | P2 | `src/lib/logging/sinks.zig:106-112` | Unregister drain-ticket wait (`emit_completed >= drain_target`) counts **any** completions, not pre-removal ones — post-removal emits completing fast can satisfy the wait while a pre-removal emit still holds the freed `BufferedSink` ctx (`:155-163`). Test-infra only; `sinks_test.zig` asserts the property single-threaded. |
| R7 | P2 | `cmd/serve.zig:340-345` (`deinitStreaming`) | Hub/registry maps freed after bounded 5s waits even when `drain_incomplete`/`hub_stop_undrained` was logged — a straggler stream thread still touches them via `unsubscribe`/`deregister`. |
| R8 | P2 | `src/agentsfleetd/credentials/broker.zig:19-25` | Documented time-of-check-to-time-of-use (TOCTOU): concurrent cold misses for one key both mint. External blast radius: a rotating-refresh provider can revoke the token family on refresh-token reuse detection. |
| R9 | P2 | `src/agentsfleetd/observability/otlp/exporter.zig:50-58` | `install` check-then-act on `g_running`: two concurrent installs → double flush thread + leaked handle on `g_thread` overwrite. Latent (only boot calls it today). |
| R10 | P2 | `src/agentsfleetd/observability/metrics_runner.zig:139-152` | `resolveSlot` SPIN_CAP fall-through: an initializer suspended >4096 spins lets the same runner id claim a duplicate slot. Metrics cardinality skew only; acknowledged in comment. |

## 2. Leak findings

| ID | Sev | Location | Defect |
|----|-----|----------|--------|
| L1 | P1 | `src/agentsfleetd/fleet/liveness_sweeper.zig:119-130` | `errdefer freeRunnerRefItems(alloc, refs.items)` frees the duped `id`s but never `refs.deinit(alloc)` — the ArrayList backing buffer leaks on any error after the first append. The sweeper's `run()` loop (:47-52) catches, logs, sleeps, retries forever on the daemon General Purpose Allocator (GPA): a flapping Postgres accumulates one buffer per sweep interval for the process lifetime. Siblings `reclaim_sweeper.zig:107-110` and `approval_gate_sweeper.zig:98` do it correctly — confirmed outlier. |
| L2 | P2 | `liveness_sweeper.zig:124-128` | Struct-literal field leak: Zig does not unwind earlier struct-literal fields when a later one errors — if `row.get(i64, 1)` fails after the `.id` dupe, the dupe is orphaned (not yet in `refs.items`, invisible to L1's errdefer). `connector_outbound.zig:194-196` documents this exact hazard and structures `intoOwned` around it. |
| L3 | P2 | `src/agentsfleetd/queue/connector_outbound.zig:226-239` (`parseJobFields`) | Duplicate stream-field key (e.g. two `provider` fields) overwrites an owned dupe without freeing; `freeOwned` frees only the survivor. Consumer runs forever on the daemon GPA — permanent leak per malformed entry. The out-of-memory (OOM) unwind of this decoder is tested (`checkAllAllocationFailures`, :289-293); the dup-key shape is not. |

## 3. Verified clean (adversarially checked, dropped)

All 268 `PgQuery.from` sites carry a local `defer q.deinit()` (paren-matching scan, zero
violations, zero suppressions); redis `Pool` acquire/release/waitForActiveSlot accounting;
OpenTelemetry Protocol (OTLP) `ring.zig` multi-producer single-consumer ring (full-detection
prevents ABA); `Subscription` futex-epoch pop; JWKS single-flight + stale-serve;
`call_deadline` fire-under-lock (file-descriptor-reuse safe); slack signing-secret
compare-and-swap cache; runner worker pool (zero shared mutable state by construction,
partial-spawn errdefer joins); `events/bus.zig` stop ordering (tested 200×); the SSE
ring/registry teardown choreography on the happy path; `crypto_store` load/store (defer-
balanced, key material zeroed); per-request arena in `http/server.zig:228-229`.

## 4. Coverage assessment

**`make memleak` reality** (`make/bench.mk:11-55`): builds ONE binary — `agentsfleetd-tests`
(root `src/agentsfleetd/tests.zig`). Linux: valgrind `--leak-check=full` over that suite
(ReleaseSafe, openssl off), blocking. macOS: plain suite run + advisory `leaks -atExit`.

Blind spots:
1. **Runner and lib suites never run under valgrind** — the fork/exec/reap and supervisor
   paths are leak-checked only by `std.testing.allocator` in their own lane.
2. **`page_allocator` singletons invisible to all detectors:** `state/model_rate_cache.zig:122`,
   `fleet_runtime/approval_gate_db.zig:111`, `db/pool.zig:217`. A regression passes every gate.
3. **GPA leak verdicts discarded:** `agentsfleetd/main.zig:140` and
   `runner/daemon/worker_pool.zig:104` both `defer _ = gpa.deinit();` — leak traces print to
   stderr at teardown; nothing fails.
4. **Drain lint** (`lint-zig.py`, `make/quality.mk:56`) passes a function if `PgQuery.from(`
   merely appears — it does not verify the `defer q.deinit()` pair it exists to guarantee.

Adoption stats: 256/636 files reference `testing.allocator`; `checkAllAllocationFailures`
10 files / 25 sites; `FailingAllocator` 14 files; 56 `*integration_test.zig` files (52 daemon,
4 runner). **No ThreadSanitizer in any lane** (ghostty ships none either).

Top untested race/leak-prone surfaces: (1) `serve_background` + `serve_shutdown` boot/shutdown
choreography — zero tests reference `serve_background`; (2) `subscription_hub` reconnect racing
attach/unsubscribe/stop, non-reading peer, concurrent read+write on one connection;
(3) `create_install_steps` in the production configuration (null WaitGroup) vs teardown;
(4) OTLP exporter lifecycle with a live flush thread; (5) `sinks.zig` unregister under
concurrent emits; (6) `liveness_sweeper.fetchDueRunners` error paths (the concurrent-sweep
integration variant runs on `page_allocator`, so even happy-path leaks are unchecked);
(7) `crypto_store.load/store` allocation-failure injection (live-Postgres test only).

## 5. Ghostty transferable practices (mined from source, cited)

### Allocator discipline
1. **tripwire fault injection** (`src/tripwire.zig`, ~290 self-contained lines): comptime-erased
   fail points (`enabled = builtin.is_test`, inline call convention — zero production cost);
   tests loop `for (std.meta.tags(FailPoint))` injecting `error.OutOfMemory` while
   `std.testing.allocator` proves the errdefer chain freed everything, and assert **state
   rollback** (`Tabstops.zig:255-271`, `PageList.zig:5503-5527`). Ghostty uses this instead of
   `checkAllAllocationFailures` (zero uses there).
2. **errdefer ladder**: each `try` acquisition followed immediately by its own errdefer
   (`PageList.MemoryPool.init:85-102`); block-scoped errdefers with one composite errdefer
   after ownership handoff (`Surface.zig:616-664`); `errdefer comptime unreachable` after the
   last fallible op makes the commit region compiler-proven atomic (22 uses).
3. **Arena as ownership unit**: `_arena` field per config-shaped object — deinit is one call
   (`Config.zig:3757-3815`); reload = build-new/replay/swap/deinit-old; parent allocator
   recovered via `arena.child_allocator`; per-operation scratch arenas; arena-in-message for
   cross-thread bundle transfer (`Surface.zig:1405-1418` → `renderer/generic.zig:652,803`);
   `stackFallback(4096, alloc)` for bounded temporaries.
4. **Leaf structures unmanaged** (take `alloc` per call, store nothing); only lifecycle roots
   keep an `alloc` field; `self.* = undefined` poisoning in every deinit.
5. **Ownership in fixed doc phrases** — "caller must free" / "takes ownership" on every
   allocating public fn; caller-owns-arguments, callee-clones-what-it-keeps (DerivedConfig,
   `App.zig:135-137`); scratch ArrayList + `toOwnedSlice` + labeled block for owned-slice
   returns.
6. **Backing allocator chosen once in `main`**: GPA in Debug (leak report at `deinit`),
   `c_allocator` in release, Valgrind detected at runtime (`global.zig:74-96`); global state
   never holds the allocator for Zig code.

### Concurrency discipline
1. **Thread-as-struct** with init/threadMain/deinit split; init never starts the thread; deinit
   documents "caller must join first" (`renderer/Thread.zig:122-196`).
2. **Shutdown is always stop-signal → join → deinit shared state** (`Surface.zig:772-798`) —
   never free anything shared while either thread lives; a dying consumer keeps draining its
   mailbox until ordered to stop (`termio/Thread.zig:226-233`).
3. **Single-Producer Single-Consumer (SPSC) bounded queue + wakeup handle per thread**;
   producers push-then-notify; consumer drains the whole queue per wakeup under one lock
   acquisition (`datastruct/blocking_queue.zig`).
4. **Message payloads own their memory**: small-inline / stable-pointer / alloc-with-carried-
   Allocator union (`datastruct/message_data.zig`); **receiver frees, in a defer, at the top of
   the handler**; `@sizeOf(Message)` pinned by test (`termio/message.zig:110-113`).
5. **Never blocking-push while holding a lock the consumer needs**: instant-try, else notify +
   unlock + forever-push + relock; lock state is an explicit `MutexState` enum parameter
   (`termio/mailbox.zig:61-93`, `Termio.zig:400-410`).
6. **One documented mutex per shared aggregate** stating exactly what it protects
   (`renderer/State.zig:10-14`); `lock(); defer unlock();` adjacent; copy-out into an arena
   inside the tightest critical section, compute unlocked (`generic.zig:1155-1280`).
7. **Thread-confined state is the default**, with "only touched by thread X, no lock needed"
   comments (`Termio.zig:759-764`); `*Locked` fn-name suffix marks lock-required entry points.
8. **Blocking readers get a quit-fd** polled alongside the data fd; shutdown kills the data
   source first, then writes the quit byte (tolerating BrokenPipe), then joins
   (`Exec.zig:194-227,1283-1346`).
9. **Atomics at the weakest sufficient ordering** — `.monotonic` did-it-change counters,
   `.seq_cst` rare health transitions; no hand-rolled lock-free structures until benchmarks
   demand it.
10. **Testing**: ghostty ships zero ThreadSanitizer and exactly ONE real-thread test
    (`terminal/search/Thread.zig:848-905`) — deterministic via `std.Thread.ResetEvent` with
    bounded `timedWait`, exercising the exact production stop→join sequence. No sleeps, no
    polling. Race confidence is structural, not tooling-based.

### Crash management (parked — future milestone; recorded for that spec)
No custom panic handler (panics abort into breakpad via sentry-native); `threadlocal
ThreadState` copied into reports by a `before_send` callback (`crash/sentry.zig:26-38,172-244`);
**local-only transport** — envelopes written to a state dir, never transmitted; hidden
crash-on-purpose command per thread to test the pipeline (`input/Binding.zig:905-925`);
`killpg` loop + `WNOHANG` reap until the process group is confirmed dead
(`Exec.zig:1151-1222`, `Command.zig:420-432`); child resets 13 signal dispositions before exec
(`pty.zig:215-234`); abnormal-exit classification by runtime threshold (≤250ms + nonzero code
= launch failure, `Surface.zig:1198-1232`); degrade-and-drain worker threads + healthy/unhealthy
transition events. Ghostty ships ReleaseFast and leans on capture; a tenant-state daemon keeps
ReleaseSafe and adds capture on top.

## 6. Assert audit (kristoff.it "You Must Fix Your Asserts") — verdict: compliant

We ship `ReleaseSafe` in every lane (`make/build.mk`, `make/dev.mk`, CI cross-compile), so
`std.debug.assert` and `unreachable` stay **checked** in production — panic, never unchecked
illegal behavior. Only benchmark targets build ReleaseFast (`make/bench.mk`), the blog's
sanctioned trade-off. Production asserts are internal invariants (`fleet_memory.zig:58`,
`redis_pool.zig:266`, comptime size/prefix checks), not input validation. Every
`catch unreachable` in `fleet_runtime/config_gates.zig` sits inside `test` blocks — permitted.
No action required; the `if (builtin.mode == .Debug)` guard pattern is available if an
expensive integrity check is ever added.

## 7. Where agentsfleet is already ahead of ghostty

A genuine Postgres race suite (100-way claim/renew convergence, metering no-lost-debits,
PATCH-vs-PATCH over real HTTP), real-thread tests for bus/hub/registry/redis-pool/JWKS/broker,
25 `checkAllAllocationFailures` sites, a test-root reachability gate, and ReleaseSafe in
production. Ghostty's edge is **design discipline** (SPSC ownership transfer, shutdown
ordering, documented lock invariants) and **tripwire** — not test volume.
