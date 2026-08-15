const std = @import("std");
const EventEnvelope = @import("event_envelope.zig");

test "EventType round-trips through toSlice/fromSlice for every variant" {
    const variants = [_]EventEnvelope.EventType{ .chat, .webhook, .cron, .continuation };
    for (variants) |v| {
        const s = v.toSlice();
        const decoded = EventEnvelope.EventType.fromSlice(s) orelse return error.TestFailed;
        try std.testing.expectEqual(v, decoded);
    }
}

test "EventType.fromSlice returns null on unknown values" {
    try std.testing.expect(EventEnvelope.EventType.fromSlice("garbage") == null);
    try std.testing.expect(EventEnvelope.EventType.fromSlice("") == null);
    try std.testing.expect(EventEnvelope.EventType.fromSlice("CHAT") == null);
}

test "encodeForXAdd produces 5 field/value pairs in canonical order" {
    const alloc = std.testing.allocator;
    const env = EventEnvelope{
        .event_id = "1729874000000-0",
        .fleet_id = "zb-1",
        .workspace_id = "ws-1",
        .actor = "steer:kishore",
        .event_type = .chat,
        .request_json = "{\"message\":\"hello\"}",
        .created_at = 1745568000000,
    };
    const argv = try env.encodeForXAdd(alloc);
    defer EventEnvelope.freeXAddArgv(alloc, argv);

    try std.testing.expectEqual(@as(usize, 10), argv.len);
    try std.testing.expectEqualStrings("type", argv[0]);
    try std.testing.expectEqualStrings("chat", argv[1]);
    try std.testing.expectEqualStrings("actor", argv[2]);
    try std.testing.expectEqualStrings("steer:kishore", argv[3]);
    try std.testing.expectEqualStrings("workspace_id", argv[4]);
    try std.testing.expectEqualStrings("ws-1", argv[5]);
    try std.testing.expectEqualStrings("request", argv[6]);
    try std.testing.expectEqualStrings("{\"message\":\"hello\"}", argv[7]);
    try std.testing.expectEqualStrings("created_at", argv[8]);
    try std.testing.expectEqualStrings("1745568000000", argv[9]);
}

test "encodeForXAdd frees the formatted created_at when the argv allocation fails" {
    // Two allocations, in order: the duped `created_at`, then the 10-slot argv.
    // A failure on the second must free the first — it is the only owned
    // element, and nothing else holds a reference to it.
    const env = EventEnvelope{
        .event_id = "1729874000000-0",
        .fleet_id = "zb-1",
        .workspace_id = "ws-1",
        .actor = "steer:kishore",
        .event_type = .chat,
        .request_json = "{\"message\":\"hello\"}",
        .created_at = 1745568000000,
    };

    var fail_index: usize = 0;
    while (fail_index < 8) : (fail_index += 1) {
        var fa = std.testing.FailingAllocator.init(std.testing.allocator, .{ .fail_index = fail_index });
        const alloc = fa.allocator();
        const argv = env.encodeForXAdd(alloc) catch |err| {
            try std.testing.expectEqual(error.OutOfMemory, err);
            continue;
        };
        EventEnvelope.freeXAddArgv(alloc, argv);
        return; // the ladder is exhausted — every earlier index unwound cleanly
    }
    return error.TestUnexpectedResult; // never succeeded: not a ladder proof
}
