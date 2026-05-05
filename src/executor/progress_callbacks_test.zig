const std = @import("std");
const pc = @import("progress_callbacks.zig");

test "rpc_version is 2 (intentional break from v1 single-reply RPC)" {
    try std.testing.expectEqual(@as(u32, 2), pc.rpc_version);
}

test "FrameKind round-trips through toSlice/fromSlice for every variant" {
    const variants = [_]pc.FrameKind{
        .tool_call_started,
        .agent_response_chunk,
        .tool_call_completed,
        .tool_call_progress,
    };
    for (variants) |v| {
        const decoded = pc.FrameKind.fromSlice(v.toSlice()) orelse return error.TestFailed;
        try std.testing.expectEqual(v, decoded);
    }
    try std.testing.expect(pc.FrameKind.fromSlice("garbage") == null);
}

test "ProgressFrame.kind matches the active union variant" {
    const f1 = pc.ProgressFrame{ .agent_response_chunk = .{ .text = "hi" } };
    try std.testing.expectEqual(pc.FrameKind.agent_response_chunk, f1.kind());

    const f2 = pc.ProgressFrame{ .tool_call_started = .{ .name = "fly", .args_redacted = "{}" } };
    try std.testing.expectEqual(pc.FrameKind.tool_call_started, f2.kind());
}

test "encode → decode round-trip for tool_call_started" {
    const alloc = std.testing.allocator;
    const original = pc.ProgressFrame{ .tool_call_started = .{
        .name = "http_request",
        .args_redacted = "{\"url\":\"${secrets.fly.api_token}\"}",
    } };
    const wire = try pc.encodeProgress(alloc, 42, original);
    defer alloc.free(wire);

    var parsed = try std.json.parseFromSlice(std.json.Value, alloc, wire, .{});
    defer parsed.deinit();
    const result = try pc.decodeProgressFromValue(parsed.value);
    try std.testing.expectEqual(@as(u64, 42), result.request_id);
    try std.testing.expectEqual(pc.FrameKind.tool_call_started, result.frame.kind());
    try std.testing.expectEqualStrings("http_request", result.frame.tool_call_started.name);
    try std.testing.expectEqualStrings(
        "{\"url\":\"${secrets.fly.api_token}\"}",
        result.frame.tool_call_started.args_redacted,
    );
}

test "encode → decode round-trip for agent_response_chunk" {
    const alloc = std.testing.allocator;
    const original = pc.ProgressFrame{ .agent_response_chunk = .{
        .text = "All apps healthy. Redis connections at 32% of cap.",
    } };
    const wire = try pc.encodeProgress(alloc, 7, original);
    defer alloc.free(wire);

    var parsed = try std.json.parseFromSlice(std.json.Value, alloc, wire, .{});
    defer parsed.deinit();
    const result = try pc.decodeProgressFromValue(parsed.value);
    try std.testing.expectEqualStrings(original.agent_response_chunk.text, result.frame.agent_response_chunk.text);
}

test "encode → decode round-trip for tool_call_completed" {
    const alloc = std.testing.allocator;
    const original = pc.ProgressFrame{ .tool_call_completed = .{ .name = "fly_status", .ms = 8210 } };
    const wire = try pc.encodeProgress(alloc, 1, original);
    defer alloc.free(wire);

    var parsed = try std.json.parseFromSlice(std.json.Value, alloc, wire, .{});
    defer parsed.deinit();
    const result = try pc.decodeProgressFromValue(parsed.value);
    try std.testing.expectEqualStrings("fly_status", result.frame.tool_call_completed.name);
    try std.testing.expectEqual(@as(i64, 8210), result.frame.tool_call_completed.ms);
}

test "encode → decode round-trip for tool_call_progress (heartbeat)" {
    const alloc = std.testing.allocator;
    const original = pc.ProgressFrame{ .tool_call_progress = .{ .name = "deploy", .elapsed_ms = 4000 } };
    const wire = try pc.encodeProgress(alloc, 99, original);
    defer alloc.free(wire);

    var parsed = try std.json.parseFromSlice(std.json.Value, alloc, wire, .{});
    defer parsed.deinit();
    const result = try pc.decodeProgressFromValue(parsed.value);
    try std.testing.expectEqual(@as(i64, 4000), result.frame.tool_call_progress.elapsed_ms);
}

test "isProgressPayload distinguishes Progress notifications from terminal responses" {
    const alloc = std.testing.allocator;
    const progress_wire = try pc.encodeProgress(alloc, 1, .{ .agent_response_chunk = .{ .text = "x" } });
    defer alloc.free(progress_wire);

    var prog_parsed = try std.json.parseFromSlice(std.json.Value, alloc, progress_wire, .{});
    defer prog_parsed.deinit();
    try std.testing.expect(pc.isProgressPayload(prog_parsed.value));

    const terminal_wire = "{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"content\":\"ok\"}}";
    var term_parsed = try std.json.parseFromSlice(std.json.Value, alloc, terminal_wire, .{});
    defer term_parsed.deinit();
    try std.testing.expect(!pc.isProgressPayload(term_parsed.value));
}

test "decodeProgressFromValue reuses caller-owned parsed value" {
    const alloc = std.testing.allocator;
    const wire =
        \\{"jsonrpc":"2.0","id":7,"method":"Progress","params":{"kind":"agent_response_chunk","text":"hello"}}
    ;
    var parsed = try std.json.parseFromSlice(std.json.Value, alloc, wire, .{});
    defer parsed.deinit();
    const decoded = try pc.decodeProgressFromValue(parsed.value);
    try std.testing.expectEqual(@as(u64, 7), decoded.request_id);
    try std.testing.expectEqual(pc.FrameKind.agent_response_chunk, decoded.frame.kind());
    try std.testing.expectEqualStrings("hello", decoded.frame.agent_response_chunk.text);
}

test "decodeProgressFromValue surfaces UnknownFrameKind without parsing" {
    const alloc = std.testing.allocator;
    const wire =
        \\{"jsonrpc":"2.0","id":1,"method":"Progress","params":{"kind":"telepathy"}}
    ;
    var parsed = try std.json.parseFromSlice(std.json.Value, alloc, wire, .{});
    defer parsed.deinit();
    try std.testing.expectError(error.UnknownFrameKind, pc.decodeProgressFromValue(parsed.value));
}

test "Hello encode → decode round-trip" {
    const alloc = std.testing.allocator;
    const wire = try pc.encodeHello(alloc, .{ .rpc_version = pc.rpc_version });
    defer alloc.free(wire);

    const decoded = try pc.decodeHello(alloc, wire);
    try std.testing.expectEqual(pc.rpc_version, decoded.rpc_version);
}

test "decodeHello rejects malformed payloads" {
    const alloc = std.testing.allocator;
    try std.testing.expectError(error.InvalidHello, pc.decodeHello(alloc, "{}"));
    try std.testing.expectError(error.InvalidHello, pc.decodeHello(alloc, "{\"hello\":42}"));
    try std.testing.expectError(error.InvalidHello, pc.decodeHello(alloc, "{\"hello\":{\"rpc_version\":\"two\"}}"));
}
