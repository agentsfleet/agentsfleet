const std = @import("std");
const protocol = @import("protocol.zig");

test "serializeRequest and parseRequest round trip" {
    const alloc = std.testing.allocator;
    const serialized = try protocol.serializeRequest(alloc, 42, protocol.Method.create_execution, null);
    defer alloc.free(serialized);

    var req = try protocol.parseRequest(alloc, serialized);
    defer req.deinit();
    try std.testing.expectEqual(@as(u64, 42), req.id);
    try std.testing.expectEqualStrings(protocol.Method.create_execution, req.method);
}

test "serializeRequest with params round trips" {
    const alloc = std.testing.allocator;
    var params = std.json.Value{ .object = std.json.ObjectMap.init(alloc) };
    defer params.object.deinit();
    try params.object.put("workspace_path", .{ .string = "/tmp/test" });

    const serialized = try protocol.serializeRequest(alloc, 1, protocol.Method.create_execution, params);
    defer alloc.free(serialized);

    var req = try protocol.parseRequest(alloc, serialized);
    defer req.deinit();
    try std.testing.expectEqual(@as(u64, 1), req.id);
    try std.testing.expect(req.params != null);
}

test "parseRequest rejects malformed JSON" {
    const alloc = std.testing.allocator;
    const result = protocol.parseRequest(alloc, "not json at all{{{");
    try std.testing.expectError(error.SyntaxError, result);
}

test "parseRequest rejects missing id" {
    const alloc = std.testing.allocator;
    const result = protocol.parseRequest(alloc, "{\"method\":\"Heartbeat\"}");
    try std.testing.expectError(error.InvalidRequest, result);
}

test "parseRequest rejects missing method" {
    const alloc = std.testing.allocator;
    const result = protocol.parseRequest(alloc, "{\"id\":1}");
    try std.testing.expectError(error.InvalidRequest, result);
}

test "parseResponse parses success response" {
    const alloc = std.testing.allocator;
    const payload = "{\"id\":1,\"result\":true}";
    var resp = try protocol.parseResponse(alloc, payload);
    defer resp.deinit();
    try std.testing.expectEqual(@as(u64, 1), resp.id);
    try std.testing.expect(resp.result != null);
    try std.testing.expect(resp.rpc_error == null);
}

test "parseResponse parses error response" {
    const alloc = std.testing.allocator;
    const payload = "{\"id\":2,\"error\":{\"code\":-32601,\"message\":\"Unknown method\"}}";
    var resp = try protocol.parseResponse(alloc, payload);
    defer resp.deinit();
    try std.testing.expectEqual(@as(u64, 2), resp.id);
    try std.testing.expect(resp.rpc_error != null);
    try std.testing.expectEqual(@as(i32, protocol.ErrorCode.method_not_found), resp.rpc_error.?.code);
    try std.testing.expectEqualStrings("Unknown method", resp.rpc_error.?.message);
}

test "parseResponse rejects missing id" {
    const alloc = std.testing.allocator;
    const result = protocol.parseResponse(alloc, "{\"result\":true}");
    try std.testing.expectError(error.InvalidRequest, result);
}

// ── T7: Regression — error code values are pinned ────────────────────
test "ErrorCode values match JSON-RPC spec" {
    // Standard JSON-RPC 2.0 error codes.
    try std.testing.expectEqual(@as(i32, -32700), protocol.ErrorCode.parse_error);
    try std.testing.expectEqual(@as(i32, -32600), protocol.ErrorCode.invalid_request);
    try std.testing.expectEqual(@as(i32, -32601), protocol.ErrorCode.method_not_found);
    try std.testing.expectEqual(@as(i32, -32602), protocol.ErrorCode.invalid_params);
    try std.testing.expectEqual(@as(i32, -32603), protocol.ErrorCode.internal_error);
    // Application-specific codes.
    try std.testing.expectEqual(@as(i32, -1), protocol.ErrorCode.execution_failed);
    try std.testing.expectEqual(@as(i32, -2), protocol.ErrorCode.timeout_killed);
    try std.testing.expectEqual(@as(i32, -3), protocol.ErrorCode.oom_killed);
    try std.testing.expectEqual(@as(i32, -4), protocol.ErrorCode.policy_denied);
    try std.testing.expectEqual(@as(i32, -5), protocol.ErrorCode.lease_expired);
    try std.testing.expectEqual(@as(i32, -6), protocol.ErrorCode.landlock_denied);
    try std.testing.expectEqual(@as(i32, -7), protocol.ErrorCode.resource_killed);
}

// ── T7: Regression — Method name constants are stable ────────────────
test "Method constants match expected strings" {
    try std.testing.expectEqualStrings("CreateExecution", protocol.Method.create_execution);
    try std.testing.expectEqualStrings("StartStage", protocol.Method.start_stage);
    try std.testing.expectEqualStrings("StreamEvents", protocol.Method.stream_events);
    try std.testing.expectEqualStrings("CancelExecution", protocol.Method.cancel_execution);
    try std.testing.expectEqualStrings("GetUsage", protocol.Method.get_usage);
    try std.testing.expectEqualStrings("DestroyExecution", protocol.Method.destroy_execution);
    try std.testing.expectEqualStrings("Heartbeat", protocol.Method.heartbeat);
}

// ── T8: Security — MAX_FRAME_SIZE is pinned ──────────────────────────
test "MAX_FRAME_SIZE is 16 MiB" {
    try std.testing.expectEqual(@as(u32, 16 * 1024 * 1024), protocol.MAX_FRAME_SIZE);
}

// ── T2: Edge case — parseRequest with empty string ───────────────────
test "parseRequest rejects empty payload" {
    const alloc = std.testing.allocator;
    const result = protocol.parseRequest(alloc, "");
    try std.testing.expectError(error.UnexpectedEndOfInput, result);
}

// ── T2: Edge case — parseResponse with only whitespace ───────────────
test "parseResponse rejects whitespace-only payload" {
    const alloc = std.testing.allocator;
    const result = protocol.parseResponse(alloc, "   ");
    try std.testing.expectError(error.UnexpectedEndOfInput, result);
}
