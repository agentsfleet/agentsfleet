// Batching mechanics for the activity forwarder. The client points at a
// closed loopback port, so a flush's POST fails fast and is swallowed
// (best-effort contract) — the assertions are about the batch state machine,
// not the wire.

const std = @import("std");
const testing = std.testing;
const common = @import("common");
const contract = @import("contract");
const client_mod = @import("control_plane_client.zig");
const dts = @import("deadline_test_support.zig");
const call_deadline = @import("call_deadline");
const forwarders = @import("forwarders.zig");

const DEAD_URL = "http://127.0.0.1:9";

fn frameFixture() contract.activity.ActivityFrame {
    return .{ .tool_call_started = .{ .name = "probe", .args_redacted = "{}" } };
}

fn testForwarder(c: *client_mod) forwarders.ActivityForwarder {
    return .{
        .alloc = testing.allocator,
        .cp = c,
        .runner_token = "agt_rtest",
        .lease_id = "lease_test",
        .deadline_ms = call_deadline.ACTIVITY_DEADLINE_MS,
    };
}

test "frames serialize on arrival and join into one comma-separated batch" {
    var deadlines: dts.TestScheduler = .{};
    defer deadlines.deinit();
    var c = client_mod.init(testing.allocator, common.globalIo(), try deadlines.start(testing.allocator), DEAD_URL);
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
    var deadlines: dts.TestScheduler = .{};
    defer deadlines.deinit();
    var c = client_mod.init(testing.allocator, common.globalIo(), try deadlines.start(testing.allocator), DEAD_URL);
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

test "the byte cap auto-flushes before the frame cap" {
    var deadlines: dts.TestScheduler = .{};
    defer deadlines.deinit();
    var c = client_mod.init(testing.allocator, common.globalIo(), try deadlines.start(testing.allocator), DEAD_URL);
    defer c.deinit();
    var fwd = testForwarder(&c);
    defer fwd.deinit();

    // ~8 KiB per frame: the 64 KiB byte bound trips well before the 16-frame
    // bound — this is the clause that caps retained memory for chatty frames.
    const big_args = "x" ** (8 * 1024);
    var sent: usize = 0;
    while (sent < forwarders.ACTIVITY_BATCH_MAX_FRAMES) : (sent += 1) {
        forwarders.ActivityForwarder.forward(@ptrCast(&fwd), .{
            .tool_call_started = .{ .name = "probe", .args_redacted = big_args },
        });
        if (fwd.count == 0) break; // the byte cap flushed the batch
    }
    try testing.expect(sent + 1 < forwarders.ACTIVITY_BATCH_MAX_FRAMES);
    try testing.expectEqual(@as(usize, 0), fwd.count);
    try testing.expectEqual(@as(usize, 0), fwd.buf.items.len);
}

fn testMemoryForwarder(c: *client_mod) forwarders.MemoryForwarder {
    return .{
        .alloc = testing.allocator,
        .cp = c,
        .runner_token = "agt_rtest",
        .fleet_id = "z_test",
        .lease_id = "lease_test",
        .fencing_token = 7,
        .deadline_ms = call_deadline.ACTIVITY_DEADLINE_MS,
    };
}

test "memory forwarder drops a malformed capture payload without posting" {
    var deadlines: dts.TestScheduler = .{};
    defer deadlines.deinit();
    var c = client_mod.init(testing.allocator, common.globalIo(), try deadlines.start(testing.allocator), DEAD_URL);
    defer c.deinit();
    var fwd = testMemoryForwarder(&c);

    // parse fails → warn-and-drop; the leak detector asserts full cleanup
    forwarders.MemoryForwarder.forward(@ptrCast(&fwd), "not-json");
    forwarders.MemoryForwarder.forward(@ptrCast(&fwd), "{\"kind\":\"object-not-array\"}");
}

test "memory forwarder posts a valid delta set best-effort" {
    var deadlines: dts.TestScheduler = .{};
    defer deadlines.deinit();
    var c = client_mod.init(testing.allocator, common.globalIo(), try deadlines.start(testing.allocator), DEAD_URL);
    defer c.deinit();
    var fwd = testMemoryForwarder(&c);

    // valid empty delta array parses, the fenced POST fails fast against the
    // dead port and is swallowed (best-effort contract) — no crash, no leak
    forwarders.MemoryForwarder.forward(@ptrCast(&fwd), "[]");
}

test "flushIfStale ships a buffered frame once the window passes" {
    var deadlines: dts.TestScheduler = .{};
    defer deadlines.deinit();
    var c = client_mod.init(testing.allocator, common.globalIo(), try deadlines.start(testing.allocator), DEAD_URL);
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
