//! Agent trigger counter.
//! Re-exported by metrics_counters.zig; lives in its own module to keep
//! metrics_counters.zig under the 350-line gate (RULE FLL).

const std = @import("std");

const AgentFields = struct {
    agent_triggered_total: u64,
};

var g_triggered = std.atomic.Value(u64).init(0);

pub fn incAgentsTriggered() void {
    _ = g_triggered.fetchAdd(1, .monotonic); // safe because: independent stat counter; /metrics readers tolerate staleness
}

pub fn snapshotAgentFields() AgentFields {
    return .{
        .agent_triggered_total = g_triggered.load(.acquire), // safe because: pairs with no publisher; scrape-time read of an independent counter
    };
}

pub fn resetForTest() void {
    g_triggered.store(0, .release); // safe because: test-only reset between serial tests; no concurrent reader during reset
}

test "single trigger increments the counter by exactly one" {
    resetForTest();
    defer resetForTest();
    const before = snapshotAgentFields().agent_triggered_total;
    incAgentsTriggered();
    const after = snapshotAgentFields().agent_triggered_total;
    try std.testing.expectEqual(@as(u64, 1), after - before);
}

test "concurrent triggers from N threads lose no update" {
    resetForTest();
    defer resetForTest();
    const Runner = struct {
        fn run(iters: usize) void {
            var i: usize = 0;
            while (i < iters) : (i += 1) incAgentsTriggered();
        }
    };
    const thread_count = 4;
    const per_thread = 1_000;
    var threads: [thread_count]std.Thread = undefined;
    for (&threads) |*t| t.* = try std.Thread.spawn(.{}, Runner.run, .{per_thread});
    for (threads) |t| t.join();
    try std.testing.expectEqual(
        @as(u64, thread_count * per_thread),
        snapshotAgentFields().agent_triggered_total,
    );
}
