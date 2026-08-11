//! Integration tier for §2 Dimension 2.2 — `test_pool_bounded_progress_and_timeout`.
//!
//! §2 makes three claims about a saturated pool and deliberately declines to
//! make a fourth:
//!
//!   1. releasing an occupied slot lets at least one queued waiter progress;
//!   2. every waiter either succeeds or receives the configured typed timeout;
//!   3. completion, timeout, and failure all leave zero leaked connections.
//!
//! The fourth — which waiter wins, or any fairness or ordering guarantee — is
//! NOT claimed, and this file is careful not to accidentally assert it. The
//! vendored pool wakes waiters from a poll loop rather than a queue, so any
//! ordering a test observed would be a coincidence of scheduling that a future
//! `Io` with a real timed wait would break. Asserting "at least one progresses"
//! is the strongest true statement available; asserting "the first one does"
//! would be a test that passes today and lies about the guarantee.
//!
//! ## Why a real pool rather than a fake
//!
//! The property under test is the pool's own blocking behaviour — its
//! availability accounting, its timeout budget, and its release signalling. A
//! fake that reimplements those has to reproduce the bug before it can catch
//! it. `size = 1` is what makes the contention deterministic: with one
//! connection, holding it saturates the pool exactly, so no test needs to guess
//! how many acquires it takes to starve.
//!
//! `LIVE_DB=1` + `TEST_DATABASE_URL` (set by `make test-integration`);
//! self-skips otherwise.

const std = @import("std");
const pg = @import("pg");
const db = @import("./pool.zig");

const common = @import("common");
const env = common.env;
const pool_mod = @import("pool.zig");

/// Enough waiters that "at least one progresses" is a real claim rather than a
/// restatement of "the only waiter progresses", and few enough that the
/// timeout arm stays quick.
const N_WAITERS: usize = 4;

/// Short enough to keep the timeout arm fast, long enough that a waiter which
/// COULD acquire is not failed by scheduling noise.
const ACQUIRE_TIMEOUT_MS: u32 = 250;
/// The progress arm needs a budget no waiter can exhaust while the holder
/// sleeps, or a pass would be indistinguishable from a timeout.
const GENEROUS_TIMEOUT_MS: u32 = 10_000;
/// How long the holder keeps the only connection before releasing it.
const HOLD_NS: u64 = 50 * std.time.ns_per_ms;

const Waiter = struct {
    pool: *db.Pool,
    /// Set when this waiter acquired and released cleanly.
    acquired: std.atomic.Value(bool) = .init(false),
    /// Set when this waiter received the pool's typed timeout.
    timed_out: std.atomic.Value(bool) = .init(false),
    /// Set when this waiter received any OTHER error. A waiter that neither
    /// acquires, times out, nor errors has silently vanished — the assertions
    /// below check the three are exhaustive rather than assuming it.
    other_error: std.atomic.Value(bool) = .init(false),

    fn run(self: *Waiter) void {
        const conn = self.pool.acquire() catch |err| {
            if (err == error.Timeout) {
                self.timed_out.store(true, .release);
            } else {
                self.other_error.store(true, .release);
            }
            return;
        };
        // Released immediately. A waiter that acquired and held would serialise
        // the remaining waiters behind it and turn the progress arm into a test
        // of this fixture's own hold time.
        self.pool.release(conn);
        self.acquired.store(true, .release);
    }
};

/// A size-1 pool with an explicit acquire budget, or null when no live database
/// is configured.
fn openSaturablePool(alloc: std.mem.Allocator, timeout_ms: u32) !?*db.Pool {
    const url = env.testLiveValue("TEST_DATABASE_URL") orelse return null;
    // parseUrl allocates host/auth strings that must outlive the pool, so they
    // come from the page allocator exactly as production does.
    var opts = try pool_mod.parseUrl(std.heap.page_allocator, url);
    opts.size = 1;
    opts.timeout = timeout_ms;
    const inner = pg.Pool.init(common.globalIo(), alloc, opts) catch return null;
    return db.adopt(inner, alloc) catch {
        inner.deinit();
        return null;
    };
}

/// Prove the pool is whole: it can still hand out its full complement. Run
/// after every arm, because "no connection leaked" is the claim that a test
/// which only counts errors would never notice failing.
fn expectNoLeakedConnections(pool: *db.Pool) !void {
    const conn = pool.acquire() catch |err| {
        std.debug.print("pool could not produce its one connection after the arm: {s}\n", .{@errorName(err)});
        return error.LeakedConnection;
    };
    pool.release(conn);
}

test "integration: test_pool_bounded_progress_and_timeout — every waiter on a held pool receives the typed timeout" {
    const alloc = std.testing.allocator;
    const pool = (try openSaturablePool(alloc, ACQUIRE_TIMEOUT_MS)) orelse return error.SkipZigTest;
    defer pool.deinit();

    // Saturate: with size = 1 this is the whole pool, so every waiter below is
    // guaranteed to contend rather than to get lucky.
    const held = try pool.acquire();

    var waiters: [N_WAITERS]Waiter = undefined;
    for (&waiters) |*w| w.* = .{ .pool = pool };

    var threads: [N_WAITERS]std.Thread = undefined;
    for (&threads, &waiters) |*t, *w| t.* = try std.Thread.spawn(.{}, Waiter.run, .{w});
    for (&threads) |*t| t.join();

    // The slot was never released, so nothing could progress — and crucially
    // every waiter still TERMINATED. A waiter that blocked forever would have
    // hung the join above rather than reaching this assertion.
    for (&waiters) |*w| {
        try std.testing.expect(!w.acquired.load(.acquire));
        try std.testing.expect(w.timed_out.load(.acquire));
        try std.testing.expect(!w.other_error.load(.acquire));
    }

    pool.release(held);
    try expectNoLeakedConnections(pool);
}

test "integration: test_pool_bounded_progress_and_timeout — releasing the slot lets a queued waiter progress" {
    const alloc = std.testing.allocator;
    const pool = (try openSaturablePool(alloc, GENEROUS_TIMEOUT_MS)) orelse return error.SkipZigTest;
    defer pool.deinit();

    const held = try pool.acquire();

    var waiters: [N_WAITERS]Waiter = undefined;
    for (&waiters) |*w| w.* = .{ .pool = pool };

    var threads: [N_WAITERS]std.Thread = undefined;
    for (&threads, &waiters) |*t, *w| t.* = try std.Thread.spawn(.{}, Waiter.run, .{w});

    // Hold long enough that every waiter is demonstrably queued before the
    // release, so the progress observed below is caused by the release rather
    // than by a waiter that happened to arrive after it.
    std.Io.sleep(common.globalIo(), .fromNanoseconds(HOLD_NS), .awake) catch {};
    pool.release(held);

    for (&threads) |*t| t.join();

    var progressed: usize = 0;
    for (&waiters) |*w| {
        // Exhaustive: the three outcomes are the only ones, so a waiter that
        // reported none of them means `run` grew a fourth path silently.
        const outcomes = @intFromBool(w.acquired.load(.acquire)) +
            @intFromBool(w.timed_out.load(.acquire)) +
            @intFromBool(w.other_error.load(.acquire));
        try std.testing.expectEqual(@as(usize, 1), outcomes);
        try std.testing.expect(!w.other_error.load(.acquire));
        if (w.acquired.load(.acquire)) progressed += 1;
    }

    // AT LEAST one, deliberately — not "all", and not a named one. Which waiter
    // wins is scheduling, and §2 declines to claim an ordering policy.
    try std.testing.expect(progressed >= 1);

    try expectNoLeakedConnections(pool);
}
