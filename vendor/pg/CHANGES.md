# Vendored fork of karlseguin/pg.zig

Upstream: https://github.com/karlseguin/pg.zig
Vendored at commit `1aa3e3c790b6f7fe7ad76052728db3198069d3eb` (ref `master`).

This is a verbatim copy of upstream plus the single patch below. Drop this vendor
copy and re-pin to a tagged upstream release once upstream supports a timed
connection-pool wait on a threaded `std.Io`.

## Patch: pool-acquire wait works on the threaded `Io` (`src/pool.zig`)

**Symptom.** Under Zig 0.16, `Pool.acquire()` returned `error.ConcurrencyUnavailable`
the instant a caller had to wait for a free connection (pool exhausted). With the
default API pool size (4), live request concurrency above the available connection
count produced intermittent 500s instead of queueing — a regression from 0.15.2,
where acquire blocked until a connection freed.

**Cause.** Upstream bounds the wait by `_timeout` using
`Io.Select.concurrent(Io.sleep, Io.Condition.wait)` — an *async* select combinator.
This project runs the **threaded** `Io` (`Io.Threaded` via `common.globalIo()`,
"Option A, threaded-not-async"), which cannot perform a concurrent select and
returns `error.ConcurrencyUnavailable`.

**Fix.** `Io.Condition` exposes no timed wait, so the exhaustion branch now blocks
on a plain `self._cond.waitUncancelable(io, &self._mutex)` until `release()` signals
a freed connection. This restores graceful wait-under-load.

**Trade-off.** The per-acquire `_timeout` is no longer enforced in `acquire()`
(it required the removed async select). A wedged query is still bounded by the
connection-level statement/read timeouts, which release the connection and wake a
waiter. If a hard acquire deadline is needed later, reintroduce it via a threaded
timed wait (`std.Thread.Condition.timedWait`) rather than the async `Io.Select`.

Only `Pool.acquire()` is changed; the rest of the library is upstream-verbatim.
