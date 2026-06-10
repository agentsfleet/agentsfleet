// Batching mechanics for the activity forwarder. The client points at a
// closed loopback port, so a flush's POST fails fast and is swallowed
// (best-effort contract) — the assertions are about the batch state machine,
// not the wire.

const std = @import("std");
const testing = std.testing;
const common = @import("common");
const contract = @import("contract");
const client_mod = @import("control_plane_client.zig");
const forwarders = @import("forwarders.zig");

const DEAD_URL = "http://127.0.0.1:9";

fn frameFixture() contract.activity.ActivityFrame {
    return .{ .tool_call_started = .{ .name = "probe", .args_redacted = "{}" } };
}

fn testForwarder(c: *client_mod) forwarders.ActivityForwarder {
    return .{
        .alloc = testing.allocator,
        .cp = c,
        .runner_token = "zrn_test",
        .lease_id = "lease_test",
        .deadline_ms = client_mod.ACTIVITY_DEADLINE_MS,
    };
}

test "frames serialize on arrival and join into one comma-separated batch" {
    var c = client_mod.init(testing.allocator, common.globalIo(), DEAD_URL);
    defer c.deinit();
    var fwd = testForwarder(&c);
    defer fwd.deinit();

    forwarders.ActivityForwarder.forward(@ptrCast(&fwd), frameFixture());
    forwarders.ActivityForwarder.forward(@ptrCast(&fwd), frameFixture());

    try testing.expectEqual(@as(usize, 2), fwd.count);
    try testing.expectEqual(@as(usize, 2), std.mem.count(u8, fwd.buf.items, "tool_call_started"));
    try testing.expectEqual(@as(usize, 1), std.mem.count(u8, fwd.buf.items, "},{"));
}

test "the frame-count cap auto-flushes and resets the batch" {
    var c = client_mod.init(testing.allocator, common.globalIo(), DEAD_URL);
    defer c.deinit();
    var fwd = testForwarder(&c);
    defer fwd.deinit();

    var i: usize = 0;
    while (i < forwarders.ACTIVITY_BATCH_MAX_FRAMES) : (i += 1) {
        forwarders.ActivityForwarder.forward(@ptrCast(&fwd), frameFixture());
    }
    // The Nth frame tripped the cap: POST attempted (fails fast, swallowed),
    // batch reset.
    try testing.expectEqual(@as(usize, 0), fwd.count);
    try testing.expectEqual(@as(usize, 0), fwd.buf.items.len);
}

test "flushIfStale ships a buffered frame once the window passes" {
    var c = client_mod.init(testing.allocator, common.globalIo(), DEAD_URL);
    defer c.deinit();
    var fwd = testForwarder(&c);
    defer fwd.deinit();

    forwarders.ActivityForwarder.forward(@ptrCast(&fwd), frameFixture());
    try testing.expectEqual(@as(usize, 1), fwd.count);

    // Inside the window: nothing ships.
    fwd.flushIfStale(fwd.first_buffered_ms + 1);
    try testing.expectEqual(@as(usize, 1), fwd.count);

    // Past the window: the tick flush fires.
    fwd.flushIfStale(fwd.first_buffered_ms + forwarders.ACTIVITY_FLUSH_WINDOW_MS + 1);
    try testing.expectEqual(@as(usize, 0), fwd.count);
}
