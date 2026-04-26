//! Unit tests for `unbounded_queue.zig`.
//!
//! T15 — push one, pop one returns same pointer; second pop returns null.
//! T16 — pushBatch of N items drains in arrival order via N pops.
//! T17 — 4-producer × 4-consumer × 10k-msg stress: every push popped exactly once.
//! T18 — popBatch drains atomically; concurrent pushes during the drain may or
//!        may not be included (we document: they are *not* included — the
//!        snapshot is taken from the swap of front/back).

const std = @import("std");
const testing = std.testing;
const UnboundedQueue = @import("unbounded_queue.zig").UnboundedQueue;

const Node = struct {
    next: ?*Node = null,
    payload: u64 = 0,
};

test "T15 push/pop one" {
    var q = UnboundedQueue(*Node).init();
    var n = Node{ .payload = 7 };
    q.push(&n);
    const got = q.pop() orelse return error.Empty;
    try testing.expectEqual(@as(u64, 7), got.payload);
    try testing.expectEqual(@as(?*Node, null), q.pop());
}

test "T16 pushBatch drains in arrival order" {
    var q = UnboundedQueue(*Node).init();

    var nodes: [4]Node = .{ .{ .payload = 1 }, .{ .payload = 2 }, .{ .payload = 3 }, .{ .payload = 4 } };
    nodes[0].next = &nodes[1];
    nodes[1].next = &nodes[2];
    nodes[2].next = &nodes[3];
    nodes[3].next = null;

    q.pushBatch(&nodes[0], &nodes[3], 4);

    var i: u64 = 1;
    while (i <= 4) : (i += 1) {
        const got = q.pop() orelse return error.Empty;
        try testing.expectEqual(i, got.payload);
    }
    try testing.expectEqual(@as(?*Node, null), q.pop());
}

const StressCfg = struct {
    msgs_per_producer: usize = 2_500, // 4 producers × 2_500 = 10_000 total
    producers: usize = 4,
    consumers: usize = 4,
};

const Stress = struct {
    q: *UnboundedQueue(*Node),
    nodes: []Node,
    seen: []std.atomic.Value(u32),
    produced: std.atomic.Value(usize),
    consumed: std.atomic.Value(usize),
    total: usize,
};

fn producer(s: *Stress, slice: []Node) void {
    for (slice) |*n| {
        n.next = null;
        s.q.push(n);
        _ = s.produced.fetchAdd(1, .seq_cst);
    }
}

fn consumer(s: *Stress) void {
    while (s.consumed.load(.seq_cst) < s.total) {
        if (s.q.pop()) |n| {
            const idx: usize = @intCast(n.payload);
            _ = s.seen[idx].fetchAdd(1, .seq_cst);
            _ = s.consumed.fetchAdd(1, .seq_cst);
        } else {
            std.atomic.spinLoopHint();
        }
    }
}

test "T17 multi-producer multi-consumer stress" {
    const cfg = StressCfg{};
    const total = cfg.msgs_per_producer * cfg.producers;

    var q = UnboundedQueue(*Node).init();

    const nodes = try testing.allocator.alloc(Node, total);
    defer testing.allocator.free(nodes);
    for (nodes, 0..) |*n, i| n.* = .{ .payload = @intCast(i) };

    const seen = try testing.allocator.alloc(std.atomic.Value(u32), total);
    defer testing.allocator.free(seen);
    for (seen) |*s| s.* = .init(0);

    var stress = Stress{
        .q = &q,
        .nodes = nodes,
        .seen = seen,
        .produced = .init(0),
        .consumed = .init(0),
        .total = total,
    };

    var producers: [4]std.Thread = undefined;
    var consumers: [4]std.Thread = undefined;
    for (&producers, 0..) |*t, i| {
        const lo = i * cfg.msgs_per_producer;
        const hi = lo + cfg.msgs_per_producer;
        t.* = try std.Thread.spawn(.{}, producer, .{ &stress, nodes[lo..hi] });
    }
    for (&consumers) |*t| t.* = try std.Thread.spawn(.{}, consumer, .{&stress});
    for (producers) |t| t.join();
    for (consumers) |t| t.join();

    // Every node seen exactly once.
    for (seen, 0..) |*s, i| {
        const v = s.load(.seq_cst);
        if (v != 1) {
            std.debug.print("idx={d} seen={d}\n", .{ i, v });
            return error.NodeNotSeenExactlyOnce;
        }
    }
}

test "T18 popBatch snapshots front/back at swap time" {
    var q = UnboundedQueue(*Node).init();

    var nodes: [3]Node = .{ .{ .payload = 10 }, .{ .payload = 20 }, .{ .payload = 30 } };
    q.push(&nodes[0]);
    q.push(&nodes[1]);
    q.push(&nodes[2]);

    var batch = q.popBatch();
    try testing.expectEqual(@as(usize, 3), batch.count);

    var it = batch.iterator();
    try testing.expectEqual(@as(u64, 10), it.next().?.payload);
    try testing.expectEqual(@as(u64, 20), it.next().?.payload);
    try testing.expectEqual(@as(u64, 30), it.next().?.payload);
    try testing.expectEqual(@as(?*Node, null), it.next());

    // Queue is empty after popBatch.
    try testing.expect(q.isEmpty());
}
