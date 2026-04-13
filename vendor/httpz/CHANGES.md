# Vendor patches over upstream httpz

Source: https://github.com/karlseguin/http.zig
Pinned upstream commit: `86ec1ae5fca398a58dfb06ae7fd8395b8ab2ab76`

This directory is a verbatim copy of upstream at the commit above plus the local
patches listed below. Drop the vendor copy and re-pin to upstream once the
patches land there.

## Patch 1 — Worker.deinit must stop the thread pool before freeing its arena

**File:** `src/worker.zig`, non-blocking `Worker(WSH).deinit`

**Symptom upstream:** Intermittent SIGSEGV during integration test teardown on
Linux (non-blocking event loop). Crash sites observed:

- `src/worker.zig:804` — `switch (conn.protocol)` in `processData`, on a `Conn`
  freed by `conn_mem_pool.deinit` / `shutdownConcurrentList`.
- `src/thread_pool.zig:272` — `const args = queue[tail]` in `getNext`, on a
  queue whose backing arena was freed by `thread_pool.deinit`.

**Root cause:** `thread_pool.deinit()` only releases the per-worker arena —
it does not call `thread_pool.stop()`, which is what sets the `stopped` flag
and broadcasts `read_cond` so pool worker threads exit `getNext`. The init-time
`errdefer` at `worker.zig:475-478` does call `stop` before `deinit`; the normal
`deinit` path at `worker.zig:503` does not. After the event loop exits and the
listen thread joins, the per-worker pool threads are still alive — either
blocked on `read_cond.wait` (memory still mapped at this instant) or executing
a queued `processData(conn, ...)` task — and the very next access touches
freed memory.

The Blocking worker variant (`worker.zig:108-109`) does it correctly. Only the
non-blocking variant is buggy. Linux CI uses non-blocking; macOS uses Blocking,
which is why this only repros on Linux.

**Fix:** Call `self.thread_pool.stop()` immediately before
`self.thread_pool.deinit()` in non-blocking `Worker.deinit`. `stop` sets the
flag, broadcasts the cond, and joins the pool threads — so by the time
`deinit` runs, no pool worker is alive to dereference anything.

**Upstream PR:** TBD — to be opened against karlseguin/http.zig with this
patch and a regression test for stop-during-shutdown.
