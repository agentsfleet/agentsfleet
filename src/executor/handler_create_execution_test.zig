//! End-to-end coverage for `parsePolicy` in `handler.zig`. The function
//! itself is file-private; we exercise it through the public handleFrame
//! entry point and assert the resulting session.policy state.

const std = @import("std");
const handler_mod = @import("handler.zig");
const protocol = @import("protocol.zig");
const types = @import("types.zig");
const Session = @import("session.zig");
const SessionStore = @import("runtime/session_store.zig");

/// JSON-RPC params for a fully-populated CreateExecution request. Returned
/// as a `std.json.Parsed` so the caller's defer .deinit() reclaims the
/// whole tree — building nested ObjectMap/Array values in-place leaks
/// because the parent's deinit doesn't recurse into element-Value storage.
const params_json: []const u8 =
    \\{
    \\  "workspace_path": "/tmp/ws",
    \\  "trace_id": "trace-1",
    \\  "zombie_id": "zmb-1",
    \\  "workspace_id": "ws-1",
    \\  "session_id": "stage-1",
    \\  "network_policy": { "allow": ["api.machines.dev", "api.upstash.com"] },
    \\  "tools": ["http_request", "memory_store"],
    \\  "secrets_map": { "slack": { "bot_token": "xoxb-AAA" } },
    \\  "context": {
    \\    "tool_window": 30,
    \\    "memory_checkpoint_every": 5,
    \\    "stage_chunk_threshold": 0.75,
    \\    "model": "claude-opus-4-7"
    \\  }
    \\}
;

fn parseCreateExecParams(alloc: std.mem.Allocator) !std.json.Parsed(std.json.Value) {
    return std.json.parseFromSlice(std.json.Value, alloc, params_json, .{});
}

test "CreateExecution stores network_policy.allow on the session" {
    const alloc = std.testing.allocator;
    var store = SessionStore.init(alloc);
    defer store.deinit();
    var handler = handler_mod.Handler.init(alloc, &store, 30_000, .{}, .deny_all);

    var parsed = try parseCreateExecParams(alloc);
    defer parsed.deinit();
    const params = parsed.value;
    const req = try protocol.serializeRequest(alloc, 1, protocol.Method.create_execution, params);
    defer alloc.free(req);

    const resp_json = try handler.handleFrame(alloc, req);
    defer alloc.free(resp_json);

    var resp = try protocol.parseResponse(alloc, resp_json);
    defer resp.deinit();
    try std.testing.expect(resp.rpc_error == null);

    const exec_id_hex = resp.result.?.object.get("execution_id").?.string;
    const exec_id = types.parseExecutionId(exec_id_hex) orelse return error.TestUnexpectedResult;
    const session = store.get(exec_id) orelse return error.TestUnexpectedResult;

    try std.testing.expectEqual(@as(usize, 2), session.policy.network_policy.allow.len);
    try std.testing.expectEqualStrings("api.machines.dev", session.policy.network_policy.allow[0]);
    try std.testing.expectEqualStrings("api.upstash.com", session.policy.network_policy.allow[1]);
}

test "CreateExecution stores tools allowlist on the session" {
    const alloc = std.testing.allocator;
    var store = SessionStore.init(alloc);
    defer store.deinit();
    var handler = handler_mod.Handler.init(alloc, &store, 30_000, .{}, .deny_all);

    var parsed = try parseCreateExecParams(alloc);
    defer parsed.deinit();
    const params = parsed.value;
    const req = try protocol.serializeRequest(alloc, 1, protocol.Method.create_execution, params);
    defer alloc.free(req);

    const resp_json = try handler.handleFrame(alloc, req);
    defer alloc.free(resp_json);
    var resp = try protocol.parseResponse(alloc, resp_json);
    defer resp.deinit();

    const exec_id = types.parseExecutionId(resp.result.?.object.get("execution_id").?.string).?;
    const session = store.get(exec_id).?;

    try std.testing.expectEqual(@as(usize, 2), session.policy.tools.len);
    try std.testing.expectEqualStrings("http_request", session.policy.tools[0]);
    try std.testing.expectEqualStrings("memory_store", session.policy.tools[1]);
}

test "CreateExecution stores secrets_map JSON tree on the session" {
    const alloc = std.testing.allocator;
    var store = SessionStore.init(alloc);
    defer store.deinit();
    var handler = handler_mod.Handler.init(alloc, &store, 30_000, .{}, .deny_all);

    var parsed = try parseCreateExecParams(alloc);
    defer parsed.deinit();
    const params = parsed.value;
    const req = try protocol.serializeRequest(alloc, 1, protocol.Method.create_execution, params);
    defer alloc.free(req);

    const resp_json = try handler.handleFrame(alloc, req);
    defer alloc.free(resp_json);
    var resp = try protocol.parseResponse(alloc, resp_json);
    defer resp.deinit();

    const exec_id = types.parseExecutionId(resp.result.?.object.get("execution_id").?.string).?;
    const session = store.get(exec_id).?;

    const sm = session.policy.secrets_map.?;
    try std.testing.expect(sm == .object);
    const slack = sm.object.get("slack").?;
    try std.testing.expect(slack == .object);
    try std.testing.expectEqualStrings("xoxb-AAA", slack.object.get("bot_token").?.string);
}

test "CreateExecution stores context budget on the session" {
    const alloc = std.testing.allocator;
    var store = SessionStore.init(alloc);
    defer store.deinit();
    var handler = handler_mod.Handler.init(alloc, &store, 30_000, .{}, .deny_all);

    var parsed = try parseCreateExecParams(alloc);
    defer parsed.deinit();
    const params = parsed.value;
    const req = try protocol.serializeRequest(alloc, 1, protocol.Method.create_execution, params);
    defer alloc.free(req);

    const resp_json = try handler.handleFrame(alloc, req);
    defer alloc.free(resp_json);
    var resp = try protocol.parseResponse(alloc, resp_json);
    defer resp.deinit();

    const exec_id = types.parseExecutionId(resp.result.?.object.get("execution_id").?.string).?;
    const session = store.get(exec_id).?;

    try std.testing.expectEqual(@as(u32, 30), session.policy.context.tool_window);
    try std.testing.expectEqual(@as(u32, 5), session.policy.context.memory_checkpoint_every);
    try std.testing.expectApproxEqAbs(@as(f32, 0.75), session.policy.context.stage_chunk_threshold, 1e-6);
    try std.testing.expectEqualStrings("claude-opus-4-7", session.policy.context.model);
}

test "CreateExecution with no policy fields uses defaults" {
    const alloc = std.testing.allocator;
    var store = SessionStore.init(alloc);
    defer store.deinit();
    var handler = handler_mod.Handler.init(alloc, &store, 30_000, .{}, .deny_all);

    var params = std.json.Value{ .object = std.json.ObjectMap.init(alloc) };
    defer params.object.deinit();
    try params.object.put("workspace_path", .{ .string = "/tmp/ws" });
    try params.object.put("zombie_id", .{ .string = "z" });

    const req = try protocol.serializeRequest(alloc, 1, protocol.Method.create_execution, params);
    defer alloc.free(req);
    const resp_json = try handler.handleFrame(alloc, req);
    defer alloc.free(resp_json);
    var resp = try protocol.parseResponse(alloc, resp_json);
    defer resp.deinit();

    const exec_id = types.parseExecutionId(resp.result.?.object.get("execution_id").?.string).?;
    const session = store.get(exec_id).?;

    try std.testing.expectEqual(@as(usize, 0), session.policy.network_policy.allow.len);
    try std.testing.expectEqual(@as(usize, 0), session.policy.tools.len);
    try std.testing.expect(session.policy.secrets_map == null);
    try std.testing.expectEqualStrings("", session.policy.context.model);
    try std.testing.expectEqual(@as(u32, 0), session.policy.context.tool_window);
}
