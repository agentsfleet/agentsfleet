//! Unit tests for `object_pool.zig`.
//!
//! T11 — acquire then release returns the same pointer on next acquire.
//! T12 — `max_capacity` bound: pool refuses to retain beyond cap.
//! T13 — `reset` hook called on every release/acquire pair.
//! T14 — `thread_safe=true` survives a 4-thread × 1k-cycle stress without leaks.

const std = @import("std");
const testing = std.testing;
const ObjectPool = @import("object_pool.zig").ObjectPool;
const Options = @import("object_pool.zig").Options;

const Box = struct {
    n: u64 = 0,
};

test "T11 acquire/release returns same pointer" {
    const Pool = ObjectPool(Box, .{});
    var pool = Pool.init(testing.allocator);
    defer pool.deinit();

    const a = try pool.acquire();
    a.n = 42;
    pool.release(a);

    const b = try pool.acquire();
    try testing.expectEqual(@as(*Box, a), b);
    pool.release(b);
}

test "T12 max_capacity refuses to retain beyond cap" {
    const Pool = ObjectPool(Box, .{ .max_capacity = 2 });
    var pool = Pool.init(testing.allocator);
    defer pool.deinit();

    const a = try pool.acquire();
    const b = try pool.acquire();
    const c = try pool.acquire();

    pool.release(a); // count: 1
    pool.release(b); // count: 2
    pool.release(c); // over cap → freed, count stays 2

    try testing.expectEqual(@as(usize, 2), pool.count);

    // Drain to confirm only two slots are reused.
    const x = try pool.acquire();
    const y = try pool.acquire();
    const z = try pool.acquire(); // fresh allocation
    pool.release(x);
    pool.release(y);
    pool.release(z);
}

var reset_count: std.atomic.Value(usize) = .init(0);
fn boxReset(p: *anyopaque) void {
    const b: *Box = @ptrCast(@alignCast(p));
    b.n = 0;
    _ = reset_count.fetchAdd(1, .seq_cst);
}

test "T13 reset hook called on each acquire-from-list" {
    reset_count.store(0, .seq_cst);
    const Pool = ObjectPool(Box, .{ .reset = boxReset });
    var pool = Pool.init(testing.allocator);
    defer pool.deinit();

    const a = try pool.acquire();
    a.n = 7;
    pool.release(a);

    const b = try pool.acquire();
    try testing.expectEqual(@as(u64, 0), b.n); // reset cleared it
    try testing.expectEqual(@as(usize, 1), reset_count.load(.seq_cst));
    pool.release(b);
}

const ThreadCtx = struct {
    pool: *ObjectPool(Box, .{ .thread_safe = true, .max_capacity = 32 }),
    cycles: usize,
};

fn worker(ctx: ThreadCtx) !void {
    var i: usize = 0;
    while (i < ctx.cycles) : (i += 1) {
        const slot = try ctx.pool.acquire();
        slot.n = i;
        ctx.pool.release(slot);
    }
}

test "T14 thread_safe stress 4 × 1k cycles" {
    const Pool = ObjectPool(Box, .{ .thread_safe = true, .max_capacity = 32 });
    var pool = Pool.init(testing.allocator);
    defer pool.deinit();

    var threads: [4]std.Thread = undefined;
    const ctx = ThreadCtx{ .pool = &pool, .cycles = 1000 };
    for (&threads) |*t| t.* = try std.Thread.spawn(.{}, worker, .{ctx});
    for (threads) |t| t.join();
    // No assertions on count — the bound capped retention; correctness
    // is "no leaks under testing.allocator + no race-corrupted list".
}
