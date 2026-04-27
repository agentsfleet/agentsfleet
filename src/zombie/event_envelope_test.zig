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

test "decodeFromXReadGroup parses a well-formed entry" {
    const fields = [_][]const u8{
        "type",         "webhook",
        "actor",        "webhook:github",
        "workspace_id", "ws-2",
        "request",      "{\"event\":\"push\"}",
        "created_at",   "1745568123456",
    };
    const env = try EventEnvelope.decodeFromXReadGroup("9999-0", "zb-7", &fields);
    try std.testing.expectEqualStrings("9999-0", env.event_id);
    try std.testing.expectEqualStrings("zb-7", env.zombie_id);
    try std.testing.expectEqualStrings("webhook:github", env.actor);
    try std.testing.expectEqual(EventEnvelope.EventType.webhook, env.event_type);
    try std.testing.expectEqualStrings("{\"event\":\"push\"}", env.request_json);
    try std.testing.expectEqual(@as(i64, 1745568123456), env.created_at);
}

test "decodeFromXReadGroup tolerates field reordering" {
    const fields = [_][]const u8{
        "request",      "{}",
        "created_at",   "0",
        "actor",        "cron:0_*/30_*_*_*",
        "type",         "cron",
        "workspace_id", "ws-3",
    };
    const env = try EventEnvelope.decodeFromXReadGroup("1-0", "zb-9", &fields);
    try std.testing.expectEqual(EventEnvelope.EventType.cron, env.event_type);
}

test "decodeFromXReadGroup rejects missing required fields" {
    const fields = [_][]const u8{
        "type",  "chat",
        "actor", "steer:foo",
        // workspace_id, request, created_at missing
    };
    const result = EventEnvelope.decodeFromXReadGroup("1-0", "zb-1", &fields);
    try std.testing.expectError(EventEnvelope.DecodeError.MissingField, result);
}

test "decodeFromXReadGroup rejects unknown event_type" {
    const fields = [_][]const u8{
        "type",         "telepathy",
        "actor",        "steer:foo",
        "workspace_id", "ws-1",
        "request",      "{}",
        "created_at",   "0",
    };
    const result = EventEnvelope.decodeFromXReadGroup("1-0", "zb-1", &fields);
    try std.testing.expectError(EventEnvelope.DecodeError.UnknownEventType, result);
}

test "decodeFromXReadGroup rejects non-numeric created_at" {
    const fields = [_][]const u8{
        "type",         "chat",
        "actor",        "steer:foo",
        "workspace_id", "ws-1",
        "request",      "{}",
        "created_at",   "yesterday",
    };
    const result = EventEnvelope.decodeFromXReadGroup("1-0", "zb-1", &fields);
    try std.testing.expectError(EventEnvelope.DecodeError.InvalidCreatedAt, result);
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

test "encode → decode round-trip preserves all fields" {
    const alloc = std.testing.allocator;
    const original = EventEnvelope{
        .event_id = "1234-5",
        .zombie_id = "zb-rt",
        .workspace_id = "ws-rt",
        .actor = "continuation:steer:rt",
        .event_type = .continuation,
        .request_json = "{\"k\":\"v\"}",
        .created_at = 9000000,
    };
    const argv = try original.encodeForXAdd(alloc);
    defer EventEnvelope.freeXAddArgv(alloc, argv);

    // Strip the field/value pairs back into the same []const []const u8
    // shape decodeFromXReadGroup expects.
    const decoded = try EventEnvelope.decodeFromXReadGroup(original.event_id, original.zombie_id, argv);
    try std.testing.expectEqualStrings(original.actor, decoded.actor);
    try std.testing.expectEqualStrings(original.workspace_id, decoded.workspace_id);
    try std.testing.expectEqualStrings(original.request_json, decoded.request_json);
    try std.testing.expectEqual(original.event_type, decoded.event_type);
    try std.testing.expectEqual(original.created_at, decoded.created_at);
}
