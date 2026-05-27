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
        .zombie_id = "zb-1",
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

test "buildContinuationActor prepends prefix on origin actors" {
    const alloc = std.testing.allocator;
    const out = try EventEnvelope.buildContinuationActor(alloc, "steer:kishore");
    defer alloc.free(out);
    try std.testing.expectEqualStrings("continuation:steer:kishore", out);
}

test "buildContinuationActor is idempotent on already-continuation actors" {
    const alloc = std.testing.allocator;

    const first = try EventEnvelope.buildContinuationActor(alloc, "steer:foo");
    defer alloc.free(first);

    const second = try EventEnvelope.buildContinuationActor(alloc, first);
    defer alloc.free(second);

    try std.testing.expectEqualStrings("continuation:steer:foo", second);
}

test "buildContinuationActor stays flat across origin families and 10-deep chains" {
    const alloc = std.testing.allocator;

    const origins = [_][]const u8{
        "steer:kishore",
        "webhook:github",
        "cron:0_*/30_*_*_*",
    };
    for (origins) |origin| {
        var current = try alloc.dupe(u8, origin);
        defer alloc.free(current);
        var depth: usize = 0;
        while (depth < 10) : (depth += 1) {
            const next = try EventEnvelope.buildContinuationActor(alloc, current);
            alloc.free(current);
            current = next;
        }
        // After 10 iterations every actor should still have exactly one
        // `continuation:` prefix in front of the original.
        const expected_prefix = "continuation:";
        try std.testing.expect(std.mem.startsWith(u8, current, expected_prefix));
        const remainder = current[expected_prefix.len..];
        try std.testing.expect(!std.mem.startsWith(u8, remainder, expected_prefix));
        try std.testing.expectEqualStrings(origin, remainder);
    }
}
