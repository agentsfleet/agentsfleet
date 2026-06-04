# Vendor patches over upstream httpz

Source: https://github.com/karlseguin/http.zig (branch `master`)
Pinned upstream commit: `f39f1ed803fcf080a01a3ab9c11b3cf9e0ff9aa8`

This directory is a verbatim copy of upstream at the commit above plus the single
local patch below. Master already carries the full Zig 0.16 migration (Io-threaded,
`std.Io.Writer`, tagged-union `Address`) — verified clean build + 123/123 tests on
0.16.0-final — so no 0.16 API patches are needed here. Drop the vendor copy and
re-pin to upstream once the patch below lands there.

## Patch 1 — Worker.deinit must stop the thread pool before freeing websocket + arena

**File:** `src/worker.zig`, non-blocking `Worker(WSH).deinit`

**Symptom upstream:** Intermittent SIGSEGV during shutdown teardown on Linux
(non-blocking event loop). `thread_pool.deinit()` only frees the per-worker arena;
it does NOT call `thread_pool.stop()`, which sets the `stopped` flag, broadcasts
`read_cond`, and joins the pool threads. So after the listen thread joins, pool
worker threads are still live — blocked on `read_cond.wait` or mid-`processData`
— and the normal deinit path frees the websocket (`worker.zig:504-505`) and the
pool arena out from under them. A queued `processData` task dereferences
`self.websocket` → use-after-free.

The Blocking worker variant is unaffected: its `listen()` already calls
`thread_pool.stop()` during shutdown. Linux CI uses non-blocking; macOS uses
Blocking, which is why this only repros on Linux.

**Fix:** Call `self.thread_pool.stop()` as the FIRST statement of non-blocking
`Worker.deinit` — before `self.websocket.deinit()`. Placement is load-bearing:
master keeps the websocket live and a pool task dereferences it, so the pool must
be joined before the websocket is freed. (The prior vendored base at `40be022`
disabled websocket, so stopping later was safe; master does not, so `stop()` moves
to the top of `deinit`.)

**Upstream PR:** TBD — to be opened against karlseguin/http.zig with this patch and
a stop-during-shutdown regression test.
