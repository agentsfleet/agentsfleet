//! Concurrency coverage for the redis `Pool`'s hard `max_active` ceiling:
//! a single release wakes exactly one of N blocked waiters, and `active_count`
//! never exceeds the ceiling under thread contention. Held in their own file
//! (off the main pool test file's length budget); the shared PING fake lives
//! in `redis_pool_test_support.zig`.

const std = @import("std");
const common = @import("common");
const Pool = @import("redis_pool.zig");
const support = @import("redis_pool_test_support.zig");
const PingFake = support.PingFake;

// ── A single release wakes exactly one of N blocked waiters ─────────────
//
// max_active=1: the main thread holds the only slot, N waiters block. One
// release frees exactly one slot; the winner must HOLD it (so the losers
// can't cascade-acquire and inflate the count) while every loser exhausts
// its budget and surfaces AcquireTimeout. Exactly one winner is the contract.

const N_WAITERS = 8;
// Loser acquire budget: long enough that all waiters are still blocked when
// the single release lands, short enough to time out before the assert.
const WAITER_TIMEOUT_MS = 150;
// Slack for waiters to park before the release, and to drain after losers
// time out before the assert. Generous so the test is not timing-fragile.
const WAITER_SETTLE_MS = 30;

const OneWinnerCtx = struct {
    pool: *Pool,
    winners: std.atomic.Value(u32),
    release_hold: *std.atomic.Value(bool),

    fn run(self: *OneWinnerCtx) void {
        // Losers exhaust their budget → AcquireTimeout → return without a win.
        // The single winner increments, then holds its slot until the gate
        // clears so the other waiters cannot acquire and inflate the count.
        const c = self.pool.acquire() catch return;
        _ = self.winners.fetchAdd(1, .monotonic);
        while (self.release_hold.load(.acquire)) common.sleepNanos(1 * std.time.ns_per_ms);
        self.pool.release(c, true);
    }
};

test "Pool max_active=1: a single release wakes exactly one of N blocked waiters" {
    const alloc = std.testing.allocator;
    var fake: PingFake = undefined;
    try fake.start(true);
    defer fake.shutdown();

    const cfg = try fake.config(alloc);
    var pool = try Pool.init(common.globalIo(), alloc, cfg, .{
        .max_idle = 1,
        .max_active = 1, // single slot: every extra acquirer must block.
        .eager_min = 0,
        .acquire_timeout_ms = WAITER_TIMEOUT_MS,
    });
    defer pool.deinit();

    // Main thread holds the only slot, so all N waiters block on it.
    const held = try pool.acquire();

    var release_hold = std.atomic.Value(bool).init(true);
    var ctx = OneWinnerCtx{
        .pool = &pool,
        .winners = std.atomic.Value(u32).init(0),
        .release_hold = &release_hold,
    };
    var threads: [N_WAITERS]std.Thread = undefined;
    for (&threads) |*t| t.* = try std.Thread.spawn(.{}, OneWinnerCtx.run, .{&ctx});

    // Let every waiter reach the blocked poll, then free exactly one slot.
    common.sleepNanos(WAITER_SETTLE_MS * std.time.ns_per_ms);
    pool.release(held, true);

    // Wait past the losers' budget so they have all timed out, then assert
    // exactly one waiter claimed the freed slot.
    common.sleepNanos((WAITER_TIMEOUT_MS + WAITER_SETTLE_MS) * std.time.ns_per_ms);
    try std.testing.expectEqual(@as(u32, 1), ctx.winners.load(.acquire));

    // Release the winner and drain; no slot is left checked out.
    release_hold.store(false, .release);
    for (&threads) |*t| t.join();
    try std.testing.expectEqual(@as(usize, 0), pool.stats().active);
}

// ── active_count never exceeds the hard ceiling under contention ─────────
//
// max_active=CAP with WORKERS threads churning acquire/work/release. Each
// samples stats().active right after acquiring and flags any reading above
// the ceiling. With more workers than slots the ceiling is saturated, so the
// invariant is exercised under real pressure — not on an idle pool.

const CAP_MAX: usize = 4;
const CAP_WORKERS = 16;
const CAP_ITERS = 24;
// Hold the slot for a beat so workers genuinely overlap on the ceiling.
const CAP_WORK_NS = 200 * std.time.ns_per_us;

const CapInvariantCtx = struct {
    pool: *Pool,
    iterations: u32,
    violations: *std.atomic.Value(u32),
    saturations: *std.atomic.Value(u32),

    fn worker(self: *CapInvariantCtx) void {
        var i: u32 = 0;
        while (i < self.iterations) : (i += 1) {
            const c = self.pool.acquire() catch return;
            const active = self.pool.stats().active;
            if (active > CAP_MAX) _ = self.violations.fetchAdd(1, .monotonic);
            if (active == CAP_MAX) _ = self.saturations.fetchAdd(1, .monotonic);
            common.sleepNanos(CAP_WORK_NS);
            self.pool.release(c, true);
        }
    }
};

test "Pool hard max_active cap: active_count never exceeds the ceiling under contention" {
    const alloc = std.testing.allocator;
    var fake: PingFake = undefined;
    try fake.start(true);
    defer fake.shutdown();

    const cfg = try fake.config(alloc);
    var pool = try Pool.init(common.globalIo(), alloc, cfg, .{
        .max_idle = CAP_MAX,
        .max_active = CAP_MAX, // hard ceiling under test.
        .eager_min = 0,
        .acquire_timeout_ms = 5_000, // generous: contention must block, not time out.
    });
    defer pool.deinit();

    var violations = std.atomic.Value(u32).init(0);
    var saturations = std.atomic.Value(u32).init(0);
    var ctx = CapInvariantCtx{
        .pool = &pool,
        .iterations = CAP_ITERS,
        .violations = &violations,
        .saturations = &saturations,
    };
    var threads: [CAP_WORKERS]std.Thread = undefined;
    for (&threads) |*t| t.* = try std.Thread.spawn(.{}, CapInvariantCtx.worker, .{&ctx});
    for (&threads) |*t| t.join();

    // Core invariant: no observation exceeded the hard ceiling.
    try std.testing.expectEqual(@as(u32, 0), violations.load(.acquire));
    // Liveness witness: the ceiling was actually reached, so the invariant was
    // exercised under real saturation rather than on a trivially-idle pool.
    try std.testing.expect(saturations.load(.acquire) > 0);
    // The blocking wait path handled contention — nobody fail-fasted on timeout.
    try std.testing.expectEqual(@as(u64, 0), pool.stats().acquire_timeouts_total);
    // No slot leaked once every worker finished.
    try std.testing.expectEqual(@as(usize, 0), pool.stats().active);
}
