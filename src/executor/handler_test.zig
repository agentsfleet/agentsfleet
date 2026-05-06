//! Tests for the executor RPC handler. Split from `handler.zig` per RULE FLL.

const std = @import("std");
const protocol = @import("protocol.zig");
const types = @import("types.zig");
const json = @import("json_helpers.zig");
const SessionStore = @import("runtime/session_store.zig");
const handler_mod = @import("handler.zig");

const Handler = handler_mod.Handler;
const parseExecutionId = types.parseExecutionId;
const jsonEscapeAlloc = json.escapeAlloc;
const getObjectParam = json.getObjectParam;
const getArrayParam = json.getArrayParam;
const failureToRpcError = protocol.failureToRpcError;

test "parseExecutionId round trips with executionIdHex" {
    const id = types.generateExecutionId();
    const hex = types.executionIdHex(id);
    const parsed = parseExecutionId(&hex);
    try std.testing.expect(parsed != null);
    try std.testing.expectEqualSlices(u8, &id, &parsed.?);
}

test "parseExecutionId rejects wrong length" {
    try std.testing.expect(parseExecutionId("short") == null);
    try std.testing.expect(parseExecutionId("") == null);
}

test "parseExecutionId rejects non-hex characters" {
    try std.testing.expect(parseExecutionId("zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz") == null);
}

test "handler dispatch returns error for malformed JSON frame" {
    const alloc = std.testing.allocator;
    var store = SessionStore.init(alloc);
    defer store.deinit();
    var handler = Handler.init(alloc, &store, 30_000, .{}, .deny_all);

    const resp_json = try handler.handleFrame(alloc, "{{bad json");
    defer alloc.free(resp_json);

    var resp = try protocol.parseResponse(alloc, resp_json);
    defer resp.deinit();
    try std.testing.expect(resp.rpc_error != null);
    try std.testing.expectEqual(@as(i32, protocol.ErrorCode.parse_error), resp.rpc_error.?.code);
}

test "handler dispatch returns method_not_found for unknown method" {
    const alloc = std.testing.allocator;
    var store = SessionStore.init(alloc);
    defer store.deinit();
    var handler = Handler.init(alloc, &store, 30_000, .{}, .deny_all);

    const req = try protocol.serializeRequest(alloc, 1, "NoSuchMethod", null);
    defer alloc.free(req);
    const resp_json = try handler.handleFrame(alloc, req);
    defer alloc.free(resp_json);

    var resp = try protocol.parseResponse(alloc, resp_json);
    defer resp.deinit();
    try std.testing.expect(resp.rpc_error != null);
    try std.testing.expectEqual(@as(i32, protocol.ErrorCode.method_not_found), resp.rpc_error.?.code);
}

test "handler CreateExecution returns execution_id" {
    const alloc = std.testing.allocator;
    var store = SessionStore.init(alloc);
    defer store.deinit();
    var handler = Handler.init(alloc, &store, 30_000, .{}, .deny_all);

    var params = std.json.Value{ .object = std.json.ObjectMap.init(alloc) };
    defer params.object.deinit();
    try params.object.put("workspace_path", .{ .string = "/tmp/test" });
    try params.object.put("zombie_id", .{ .string = "r" });

    const req = try protocol.serializeRequest(alloc, 1, protocol.Method.create_execution, params);
    defer alloc.free(req);
    const resp_json = try handler.handleFrame(alloc, req);
    defer alloc.free(resp_json);

    var resp = try protocol.parseResponse(alloc, resp_json);
    defer resp.deinit();
    try std.testing.expect(resp.rpc_error == null);
    try std.testing.expect(resp.result != null);

    try std.testing.expectEqual(@as(usize, 1), store.activeCount());

    store.deinit();
    store = SessionStore.init(alloc);
}

test "handler CreateExecution without params returns invalid_params" {
    const alloc = std.testing.allocator;
    var store = SessionStore.init(alloc);
    defer store.deinit();
    var handler = Handler.init(alloc, &store, 30_000, .{}, .deny_all);

    const req = try protocol.serializeRequest(alloc, 1, protocol.Method.create_execution, null);
    defer alloc.free(req);
    const resp_json = try handler.handleFrame(alloc, req);
    defer alloc.free(resp_json);

    var resp = try protocol.parseResponse(alloc, resp_json);
    defer resp.deinit();
    try std.testing.expect(resp.rpc_error != null);
    try std.testing.expectEqual(@as(i32, protocol.ErrorCode.invalid_params), resp.rpc_error.?.code);
}

test "handler StreamEvents returns empty array" {
    const alloc = std.testing.allocator;
    var store = SessionStore.init(alloc);
    defer store.deinit();
    var handler = Handler.init(alloc, &store, 30_000, .{}, .deny_all);

    const req = try protocol.serializeRequest(alloc, 1, protocol.Method.stream_events, null);
    defer alloc.free(req);
    const resp_json = try handler.handleFrame(alloc, req);
    defer alloc.free(resp_json);

    var resp = try protocol.parseResponse(alloc, resp_json);
    defer resp.deinit();
    try std.testing.expect(resp.rpc_error == null);
}

// Edge case — unicode in correlation fields.
test "handler CreateExecution with unicode correlation fields" {
    const alloc = std.testing.allocator;
    var store = SessionStore.init(alloc);
    defer store.deinit();
    var handler = Handler.init(alloc, &store, 30_000, .{}, .deny_all);

    var params = std.json.Value{ .object = std.json.ObjectMap.init(alloc) };
    defer params.object.deinit();
    try params.object.put("workspace_path", .{ .string = "/tmp/日本語-workspace" });
    try params.object.put("zombie_id", .{ .string = "run-café-☕" });
    try params.object.put("trace_id", .{ .string = "trace-émoji-👨‍💻" });

    const req = try protocol.serializeRequest(alloc, 1, protocol.Method.create_execution, params);
    defer alloc.free(req);
    const resp_json = try handler.handleFrame(alloc, req);
    defer alloc.free(resp_json);

    var resp = try protocol.parseResponse(alloc, resp_json);
    defer resp.deinit();
    try std.testing.expect(resp.rpc_error == null);
    try std.testing.expectEqual(@as(usize, 1), store.activeCount());
}

// Error path — StartStage with non-existent session.
test "handler StartStage with unknown execution_id returns execution_failed" {
    const alloc = std.testing.allocator;
    var store = SessionStore.init(alloc);
    defer store.deinit();
    var handler = Handler.init(alloc, &store, 30_000, .{}, .deny_all);

    const fake_id = types.executionIdHex(types.generateExecutionId());
    var params = std.json.Value{ .object = std.json.ObjectMap.init(alloc) };
    defer params.object.deinit();
    try params.object.put("execution_id", .{ .string = &fake_id });
    try params.object.put("session_id", .{ .string = "s" });

    const req = try protocol.serializeRequest(alloc, 1, protocol.Method.start_stage, params);
    defer alloc.free(req);
    const resp_json = try handler.handleFrame(alloc, req);
    defer alloc.free(resp_json);

    var resp = try protocol.parseResponse(alloc, resp_json);
    defer resp.deinit();
    try std.testing.expect(resp.rpc_error != null);
    try std.testing.expectEqual(@as(i32, protocol.ErrorCode.execution_failed), resp.rpc_error.?.code);
}

// Error path — DestroyExecution unknown session.
test "handler DestroyExecution with unknown execution_id returns error" {
    const alloc = std.testing.allocator;
    var store = SessionStore.init(alloc);
    defer store.deinit();
    var handler = Handler.init(alloc, &store, 30_000, .{}, .deny_all);

    const fake_id = types.executionIdHex(types.generateExecutionId());
    var params = std.json.Value{ .object = std.json.ObjectMap.init(alloc) };
    defer params.object.deinit();
    try params.object.put("execution_id", .{ .string = &fake_id });

    const req = try protocol.serializeRequest(alloc, 1, protocol.Method.destroy_execution, params);
    defer alloc.free(req);
    const resp_json = try handler.handleFrame(alloc, req);
    defer alloc.free(resp_json);

    var resp = try protocol.parseResponse(alloc, resp_json);
    defer resp.deinit();
    try std.testing.expect(resp.rpc_error != null);
    try std.testing.expectEqual(@as(i32, protocol.ErrorCode.execution_failed), resp.rpc_error.?.code);
}

// Output — verify errorResponse JSON structure.
test "handler errorResponse output is valid parseable JSON-RPC" {
    const alloc = std.testing.allocator;
    var store = SessionStore.init(alloc);
    defer store.deinit();
    var handler = Handler.init(alloc, &store, 30_000, .{}, .deny_all);

    const resp_json = try handler.handleFrame(alloc, "{}");
    defer alloc.free(resp_json);

    var parsed = try std.json.parseFromSlice(std.json.Value, alloc, resp_json, .{});
    defer parsed.deinit();

    try std.testing.expect(parsed.value.object.get("id") != null);
    try std.testing.expect(parsed.value.object.get("error") != null);

    const err_obj = parsed.value.object.get("error").?;
    try std.testing.expect(err_obj.object.get("code") != null);
    try std.testing.expect(err_obj.object.get("message") != null);
}

// Security — workspace_path with path traversal.
// Note: in v1, the handler stores workspace_path as-is and Landlock enforces
// filesystem policy. This test documents that the handler does NOT do path
// validation — Landlock is the security boundary.
test "handler CreateExecution accepts path traversal without crashing" {
    const alloc = std.testing.allocator;
    var store = SessionStore.init(alloc);
    defer store.deinit();
    var handler = Handler.init(alloc, &store, 30_000, .{}, .deny_all);

    var params = std.json.Value{ .object = std.json.ObjectMap.init(alloc) };
    defer params.object.deinit();
    try params.object.put("workspace_path", .{ .string = "/tmp/../../../etc/passwd" });
    try params.object.put("zombie_id", .{ .string = "r" });

    const req = try protocol.serializeRequest(alloc, 1, protocol.Method.create_execution, params);
    defer alloc.free(req);
    const resp_json = try handler.handleFrame(alloc, req);
    defer alloc.free(resp_json);

    var resp = try protocol.parseResponse(alloc, resp_json);
    defer resp.deinit();
    try std.testing.expect(resp.rpc_error == null);
}

// Perf/leak — repeated handler calls don't leak.
test "handler 100 create+destroy cycles no leak" {
    const alloc = std.testing.allocator;
    var store = SessionStore.init(alloc);
    defer store.deinit();
    var handler = Handler.init(alloc, &store, 30_000, .{}, .deny_all);

    for (0..100) |i| {
        var params = std.json.Value{ .object = std.json.ObjectMap.init(alloc) };
        try params.object.put("workspace_path", .{ .string = "/tmp/test" });
        try params.object.put("zombie_id", .{ .string = "r" });

        const req = try protocol.serializeRequest(alloc, i + 1, protocol.Method.create_execution, params);
        const resp_json = try handler.handleFrame(alloc, req);
        params.object.deinit();
        alloc.free(req);

        var resp = try protocol.parseResponse(alloc, resp_json);
        const exec_id = resp.result.?.object.get("execution_id").?.string;
        const exec_id_copy = try alloc.dupe(u8, exec_id);
        resp.deinit();
        alloc.free(resp_json);

        var dparams = std.json.Value{ .object = std.json.ObjectMap.init(alloc) };
        try dparams.object.put("execution_id", .{ .string = exec_id_copy });

        const dreq = try protocol.serializeRequest(alloc, i + 1000, protocol.Method.destroy_execution, dparams);
        const dresp_json = try handler.handleFrame(alloc, dreq);
        dparams.object.deinit();
        alloc.free(exec_id_copy);
        alloc.free(dreq);
        alloc.free(dresp_json);
    }
    try std.testing.expectEqual(@as(usize, 0), store.activeCount());
}

// jsonEscapeAlloc tests — security-critical.

test "jsonEscapeAlloc escapes double quotes" {
    const alloc = std.testing.allocator;
    const result = try jsonEscapeAlloc(alloc, "he said \"hi\"");
    defer alloc.free(result);
    try std.testing.expect(std.mem.indexOf(u8, result, "\\\"") != null);
}

test "jsonEscapeAlloc escapes backslashes" {
    const alloc = std.testing.allocator;
    const result = try jsonEscapeAlloc(alloc, "path\\to\\file");
    defer alloc.free(result);
    try std.testing.expect(std.mem.indexOf(u8, result, "\\\\") != null);
}

test "jsonEscapeAlloc escapes newlines and tabs" {
    const alloc = std.testing.allocator;
    const result = try jsonEscapeAlloc(alloc, "line1\nline2\tcol");
    defer alloc.free(result);
    try std.testing.expect(std.mem.indexOf(u8, result, "\\n") != null);
    try std.testing.expect(std.mem.indexOf(u8, result, "\\t") != null);
}

test "jsonEscapeAlloc handles control characters" {
    const alloc = std.testing.allocator;
    const input = &[_]u8{0x01};
    const result = try jsonEscapeAlloc(alloc, input);
    defer alloc.free(result);
    try std.testing.expect(std.mem.indexOf(u8, result, "\\u0001") != null);
}

test "jsonEscapeAlloc empty string returns empty" {
    const alloc = std.testing.allocator;
    const result = try jsonEscapeAlloc(alloc, "");
    defer alloc.free(result);
    try std.testing.expectEqualStrings("", result);
}

test "jsonEscapeAlloc preserves UTF-8 multibyte" {
    const alloc = std.testing.allocator;
    const result = try jsonEscapeAlloc(alloc, "日本語");
    defer alloc.free(result);
    try std.testing.expectEqualStrings("日本語", result);
}

test "jsonEscapeAlloc no leak on large input" {
    const alloc = std.testing.allocator;
    const input = try alloc.alloc(u8, 10 * 1024);
    defer alloc.free(input);
    @memset(input, 'A');
    const result = try jsonEscapeAlloc(alloc, input);
    defer alloc.free(result);
    try std.testing.expectEqual(@as(usize, 10 * 1024), result.len);
}

// getObjectParam tests.

test "getObjectParam returns object value" {
    const alloc = std.testing.allocator;
    var inner = std.json.ObjectMap.init(alloc);
    defer inner.deinit();
    try inner.put("x", .{ .integer = 1 });
    const inner_val = std.json.Value{ .object = inner };

    var outer = std.json.Value{ .object = std.json.ObjectMap.init(alloc) };
    defer outer.object.deinit();
    try outer.object.put("nested", inner_val);

    const result = getObjectParam(outer, "nested");
    try std.testing.expect(result != null);
    try std.testing.expect(result.? == .object);
}

test "getObjectParam returns null for non-object" {
    const alloc = std.testing.allocator;
    var outer = std.json.Value{ .object = std.json.ObjectMap.init(alloc) };
    defer outer.object.deinit();
    try outer.object.put("name", .{ .string = "hello" });

    const result = getObjectParam(outer, "name");
    try std.testing.expect(result == null);
}

test "getObjectParam returns null for missing key" {
    const alloc = std.testing.allocator;
    var outer = std.json.Value{ .object = std.json.ObjectMap.init(alloc) };
    defer outer.object.deinit();

    const result = getObjectParam(outer, "nonexistent");
    try std.testing.expect(result == null);
}

// getArrayParam tests.

test "getArrayParam returns array value" {
    const alloc = std.testing.allocator;
    var arr = std.json.Array.init(alloc);
    defer arr.deinit();
    try arr.append(.{ .integer = 1 });
    try arr.append(.{ .integer = 2 });

    var outer = std.json.Value{ .object = std.json.ObjectMap.init(alloc) };
    defer outer.object.deinit();
    try outer.object.put("items", .{ .array = arr });

    const result = getArrayParam(outer, "items");
    try std.testing.expect(result != null);
    try std.testing.expect(result.? == .array);
}

test "getArrayParam returns null for non-array" {
    const alloc = std.testing.allocator;
    var outer = std.json.Value{ .object = std.json.ObjectMap.init(alloc) };
    defer outer.object.deinit();
    try outer.object.put("name", .{ .string = "hello" });

    const result = getArrayParam(outer, "name");
    try std.testing.expect(result == null);
}

test "getArrayParam returns null for missing key" {
    const alloc = std.testing.allocator;
    var outer = std.json.Value{ .object = std.json.ObjectMap.init(alloc) };
    defer outer.object.deinit();

    const result = getArrayParam(outer, "nonexistent");
    try std.testing.expect(result == null);
}

// failureToRpcError tests.

test "failureToRpcError maps all FailureClass variants" {
    try std.testing.expectEqual(@as(i32, protocol.ErrorCode.timeout_killed), failureToRpcError(.timeout_kill));
    try std.testing.expectEqual(@as(i32, protocol.ErrorCode.oom_killed), failureToRpcError(.oom_kill));
    try std.testing.expectEqual(@as(i32, protocol.ErrorCode.policy_denied), failureToRpcError(.policy_deny));
    try std.testing.expectEqual(@as(i32, protocol.ErrorCode.lease_expired), failureToRpcError(.lease_expired));
    try std.testing.expectEqual(@as(i32, protocol.ErrorCode.landlock_denied), failureToRpcError(.landlock_deny));
    try std.testing.expectEqual(@as(i32, protocol.ErrorCode.resource_killed), failureToRpcError(.resource_kill));
    try std.testing.expectEqual(@as(i32, protocol.ErrorCode.execution_failed), failureToRpcError(.startup_posture));
    try std.testing.expectEqual(@as(i32, protocol.ErrorCode.execution_failed), failureToRpcError(.executor_crash));
    try std.testing.expectEqual(@as(i32, protocol.ErrorCode.execution_failed), failureToRpcError(.transport_loss));
}

test "failureToRpcError maps null to execution_failed" {
    try std.testing.expectEqual(@as(i32, protocol.ErrorCode.execution_failed), failureToRpcError(null));
}

// handleGetUsage test.

test "handler GetUsage returns zero for fresh session" {
    const alloc = std.testing.allocator;
    var store = SessionStore.init(alloc);
    defer store.deinit();
    var handler = Handler.init(alloc, &store, 30_000, .{}, .deny_all);

    var cparams = std.json.Value{ .object = std.json.ObjectMap.init(alloc) };
    try cparams.object.put("workspace_path", .{ .string = "/tmp/test" });
    try cparams.object.put("zombie_id", .{ .string = "r" });

    const creq = try protocol.serializeRequest(alloc, 1, protocol.Method.create_execution, cparams);
    const cresp_json = try handler.handleFrame(alloc, creq);
    cparams.object.deinit();
    alloc.free(creq);

    var cresp = try protocol.parseResponse(alloc, cresp_json);
    const exec_id = cresp.result.?.object.get("execution_id").?.string;
    const exec_id_copy = try alloc.dupe(u8, exec_id);
    cresp.deinit();
    alloc.free(cresp_json);
    defer alloc.free(exec_id_copy);

    var gparams = std.json.Value{ .object = std.json.ObjectMap.init(alloc) };
    defer gparams.object.deinit();
    try gparams.object.put("execution_id", .{ .string = exec_id_copy });

    const greq = try protocol.serializeRequest(alloc, 2, protocol.Method.get_usage, gparams);
    defer alloc.free(greq);
    const gresp_json = try handler.handleFrame(alloc, greq);
    defer alloc.free(gresp_json);

    var gresp = try protocol.parseResponse(alloc, gresp_json);
    defer gresp.deinit();
    try std.testing.expect(gresp.rpc_error == null);
    try std.testing.expect(gresp.result != null);

    const result_obj = gresp.result.?.object;
    const token_count = result_obj.get("token_count").?.integer;
    const wall_seconds = result_obj.get("wall_seconds").?.integer;
    try std.testing.expectEqual(@as(i64, 0), token_count);
    try std.testing.expectEqual(@as(i64, 0), wall_seconds);
}

// handleHeartbeat test.

test "handler Heartbeat refreshes lease" {
    const alloc = std.testing.allocator;
    var store = SessionStore.init(alloc);
    defer store.deinit();
    var handler = Handler.init(alloc, &store, 30_000, .{}, .deny_all);

    var cparams = std.json.Value{ .object = std.json.ObjectMap.init(alloc) };
    try cparams.object.put("workspace_path", .{ .string = "/tmp/test" });
    try cparams.object.put("zombie_id", .{ .string = "r" });

    const creq = try protocol.serializeRequest(alloc, 1, protocol.Method.create_execution, cparams);
    const cresp_json = try handler.handleFrame(alloc, creq);
    cparams.object.deinit();
    alloc.free(creq);

    var cresp = try protocol.parseResponse(alloc, cresp_json);
    const exec_id = cresp.result.?.object.get("execution_id").?.string;
    const exec_id_copy = try alloc.dupe(u8, exec_id);
    cresp.deinit();
    alloc.free(cresp_json);
    defer alloc.free(exec_id_copy);

    var hparams = std.json.Value{ .object = std.json.ObjectMap.init(alloc) };
    defer hparams.object.deinit();
    try hparams.object.put("execution_id", .{ .string = exec_id_copy });

    const hreq = try protocol.serializeRequest(alloc, 2, protocol.Method.heartbeat, hparams);
    defer alloc.free(hreq);
    const hresp_json = try handler.handleFrame(alloc, hreq);
    defer alloc.free(hresp_json);

    var hresp = try protocol.parseResponse(alloc, hresp_json);
    defer hresp.deinit();
    try std.testing.expect(hresp.rpc_error == null);
}
