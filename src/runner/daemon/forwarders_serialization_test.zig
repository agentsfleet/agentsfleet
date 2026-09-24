//! Allocation failure after a buffered frame must leave a valid batch.

const std = @import("std");
const common = @import("common");
const contract = @import("contract");
const client_mod = @import("control_plane_client.zig");
const dts = @import("deadline_test_support.zig");
const forwarders = @import("forwarders.zig");

test "a failed second frame rolls back its comma and partial JSON" {
    const alloc = std.testing.allocator;
    var deadlines: dts.TestScheduler = .{};
    defer deadlines.deinit();
    var client = client_mod.init(alloc, common.globalIo(), try deadlines.start(alloc), "http://127.0.0.1:9");
    defer client.deinit();
    var fwd = forwarders.ActivityForwarder{
        .alloc = alloc,
        .cp = &client,
        .runner_token = "agt_rtest",
        .lease_id = "lease_test",
        .deadline_ms = 5_000,
        .eager_first_frame_done = true,
        .eager_first_chunk_done = true,
    };
    defer fwd.deinit();
    forwarders.ActivityForwarder.forward(@ptrCast(&fwd), .{
        .tool_call_started = .{ .name = "first", .args_redacted = "{}" },
    });
    const before = try alloc.dupe(u8, fwd.buf.items);
    defer alloc.free(before);

    var failing = std.testing.FailingAllocator.init(alloc, .{ .fail_index = 0 });
    fwd.alloc = failing.allocator();
    const oversized_args = "x" ** (32 * 1024);
    forwarders.ActivityForwarder.forward(@ptrCast(&fwd), .{
        .tool_call_started = .{ .name = "oversized", .args_redacted = oversized_args },
    });
    fwd.alloc = alloc;
    try std.testing.expectEqual(@as(usize, 1), fwd.count);
    try std.testing.expectEqualSlices(u8, before, fwd.buf.items);

    forwarders.ActivityForwarder.forward(@ptrCast(&fwd), .{
        .tool_call_started = .{ .name = "recovered", .args_redacted = "{}" },
    });
    try std.testing.expectEqual(@as(usize, 2), fwd.count);
    const framed = try std.fmt.allocPrint(alloc, "[{s}]", .{fwd.buf.items});
    defer alloc.free(framed);
    const parsed = try std.json.parseFromSlice([]contract.activity.ActivityFrame, alloc, framed, .{});
    defer parsed.deinit();
    try std.testing.expectEqualStrings("first", parsed.value[0].tool_call_started.name);
    try std.testing.expectEqualStrings("recovered", parsed.value[1].tool_call_started.name);
}
