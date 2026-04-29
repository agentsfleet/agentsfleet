//! Integration tests for the executor RPC lifecycle.
//!
//! These tests exercise the full server → handler → session → client flow
//! over a real Unix socket. They cover the critical gaps identified in
//! coverage review: handler dispatch, client API, and end-to-end flow.

const std = @import("std");
const builtin = @import("builtin");
const transport = @import("transport.zig");
const handler_mod = @import("handler.zig");
const session_mod = @import("session.zig");
const protocol = @import("protocol.zig");
const types = @import("types.zig");

const TEST_SOCKET = "/tmp/zombie-executor-integration-test.sock";

// Full lifecycle: CreateExecution → StartStage → GetUsage → Heartbeat → CancelExecution → DestroyExecution
test "integration: full RPC lifecycle over Unix socket" {
    if (builtin.os.tag == .windows) return error.SkipZigTest;

    const alloc = std.testing.allocator;
    var store = session_mod.SessionStore.init(alloc);
    defer store.deinit();

    var rpc_handler = handler_mod.Handler.init(alloc, &store, 30_000, .{}, .deny_all);

    // Wrap handler for transport.
    const Wrapper = struct {
        var g_handler: *handler_mod.Handler = undefined;
        fn handle(a: std.mem.Allocator, payload: []const u8, conn_fd: ?std.posix.socket_t) anyerror![]u8 {
            return g_handler.handleFrameWithFd(a, payload, conn_fd);
        }
    };
    Wrapper.g_handler = &rpc_handler;

    var server = transport.Server.init(alloc, TEST_SOCKET, Wrapper.handle);
    try server.bind();
    defer server.stop();

    const server_thread = try std.Thread.spawn(.{}, transport.Server.serve, .{&server});

    std.Thread.sleep(50 * std.time.ns_per_ms);

    var client = transport.Client.init(alloc, TEST_SOCKET);
    try client.connect();
    defer client.close();

    const sock = client.fd.?;

    // 1. CreateExecution
    var create_params = std.json.Value{ .object = std.json.ObjectMap.init(alloc) };
    defer create_params.object.deinit();
    try create_params.object.put("workspace_path", .{ .string = "/tmp/test-workspace" });
    try create_params.object.put("trace_id", .{ .string = "trace-123" });
    try create_params.object.put("zombie_id", .{ .string = "run-456" });
    try create_params.object.put("workspace_id", .{ .string = "ws-789" });
    try create_params.object.put("session_id", .{ .string = "stage-1" });

    try protocol.sendRequest(alloc, sock, 1, protocol.Method.create_execution, create_params);
    const create_frame = try protocol.readFrameFromFd(alloc, sock);
    defer alloc.free(create_frame);

    var create_resp = try protocol.parseResponse(alloc, create_frame);
    defer create_resp.deinit();
    try std.testing.expect(create_resp.rpc_error == null);
    try std.testing.expect(create_resp.result != null);

    const exec_id = create_resp.result.?.object.get("execution_id").?.string;
    try std.testing.expectEqual(@as(usize, 32), exec_id.len);

    // Verify session was created.
    try std.testing.expectEqual(@as(usize, 1), store.activeCount());

    // 2. StartStage — sends message for M12_003 runner invocation.
    //    Without NullClaw API keys, the runner returns a failure (startup_posture).
    //    The lifecycle test verifies the RPC round-trip, not NullClaw execution.
    var stage_params = std.json.Value{ .object = std.json.ObjectMap.init(alloc) };
    defer stage_params.object.deinit();
    try stage_params.object.put("execution_id", .{ .string = exec_id });
    try stage_params.object.put("message", .{ .string = "Test stage execution" });

    try protocol.sendRequest(alloc, sock, 2, protocol.Method.start_stage, stage_params);
    const stage_frame = try protocol.readFrameFromFd(alloc, sock);
    defer alloc.free(stage_frame);

    var stage_resp = try protocol.parseResponse(alloc, stage_frame);
    defer stage_resp.deinit();
    // Runner may succeed (if API keys available) or return an error (no API keys).
    // Either way, the RPC round-trip must produce valid JSON-RPC.
    try std.testing.expect(stage_resp.rpc_error != null or stage_resp.result != null);

    // 3. GetUsage
    var usage_params = std.json.Value{ .object = std.json.ObjectMap.init(alloc) };
    defer usage_params.object.deinit();
    try usage_params.object.put("execution_id", .{ .string = exec_id });

    try protocol.sendRequest(alloc, sock, 3, protocol.Method.get_usage, usage_params);
    const usage_frame = try protocol.readFrameFromFd(alloc, sock);
    defer alloc.free(usage_frame);

    var usage_resp = try protocol.parseResponse(alloc, usage_frame);
    defer usage_resp.deinit();
    try std.testing.expect(usage_resp.rpc_error == null);

    // 4. Heartbeat
    var hb_params = std.json.Value{ .object = std.json.ObjectMap.init(alloc) };
    defer hb_params.object.deinit();
    try hb_params.object.put("execution_id", .{ .string = exec_id });

    try protocol.sendRequest(alloc, sock, 4, protocol.Method.heartbeat, hb_params);
    const hb_frame = try protocol.readFrameFromFd(alloc, sock);
    defer alloc.free(hb_frame);

    var hb_resp = try protocol.parseResponse(alloc, hb_frame);
    defer hb_resp.deinit();
    try std.testing.expect(hb_resp.rpc_error == null);

    // 5. CancelExecution
    var cancel_params = std.json.Value{ .object = std.json.ObjectMap.init(alloc) };
    defer cancel_params.object.deinit();
    try cancel_params.object.put("execution_id", .{ .string = exec_id });

    try protocol.sendRequest(alloc, sock, 5, protocol.Method.cancel_execution, cancel_params);
    const cancel_frame = try protocol.readFrameFromFd(alloc, sock);
    defer alloc.free(cancel_frame);

    var cancel_resp = try protocol.parseResponse(alloc, cancel_frame);
    defer cancel_resp.deinit();
    try std.testing.expect(cancel_resp.rpc_error == null);

    // 6. DestroyExecution
    var destroy_params = std.json.Value{ .object = std.json.ObjectMap.init(alloc) };
    defer destroy_params.object.deinit();
    try destroy_params.object.put("execution_id", .{ .string = exec_id });

    try protocol.sendRequest(alloc, sock, 6, protocol.Method.destroy_execution, destroy_params);
    const destroy_frame = try protocol.readFrameFromFd(alloc, sock);
    defer alloc.free(destroy_frame);

    var destroy_resp = try protocol.parseResponse(alloc, destroy_frame);
    defer destroy_resp.deinit();
    try std.testing.expect(destroy_resp.rpc_error == null);

    // Verify session was cleaned up.
    try std.testing.expectEqual(@as(usize, 0), store.activeCount());

    client.close();
    server.stop();
    server_thread.join();
}

// Handler returns error for unknown method.
test "integration: unknown method returns method_not_found" {
    if (builtin.os.tag == .windows) return error.SkipZigTest;

    const alloc = std.testing.allocator;
    var store = session_mod.SessionStore.init(alloc);
    defer store.deinit();

    var rpc_handler = handler_mod.Handler.init(alloc, &store, 30_000, .{}, .deny_all);

    // Directly test handler without socket.
    const req_json = try protocol.serializeRequest(alloc, 99, "BogusMethod", null);
    defer alloc.free(req_json);

    const resp_json = try rpc_handler.handleFrame(alloc, req_json);
    defer alloc.free(resp_json);

    var resp = try protocol.parseResponse(alloc, resp_json);
    defer resp.deinit();
    try std.testing.expect(resp.rpc_error != null);
    try std.testing.expectEqual(@as(i32, protocol.ErrorCode.method_not_found), resp.rpc_error.?.code);
}

// Handler returns error for missing params.
test "integration: create_execution without params returns invalid_params" {
    if (builtin.os.tag == .windows) return error.SkipZigTest;

    const alloc = std.testing.allocator;
    var store = session_mod.SessionStore.init(alloc);
    defer store.deinit();

    var rpc_handler = handler_mod.Handler.init(alloc, &store, 30_000, .{}, .deny_all);

    const req_json = try protocol.serializeRequest(alloc, 1, protocol.Method.create_execution, null);
    defer alloc.free(req_json);

    const resp_json = try rpc_handler.handleFrame(alloc, req_json);
    defer alloc.free(resp_json);

    var resp = try protocol.parseResponse(alloc, resp_json);
    defer resp.deinit();
    try std.testing.expect(resp.rpc_error != null);
    try std.testing.expectEqual(@as(i32, protocol.ErrorCode.invalid_params), resp.rpc_error.?.code);
}

// StartStage on non-existent session returns error.
test "integration: start_stage with invalid execution_id returns error" {
    if (builtin.os.tag == .windows) return error.SkipZigTest;

    const alloc = std.testing.allocator;
    var store = session_mod.SessionStore.init(alloc);
    defer store.deinit();

    var rpc_handler = handler_mod.Handler.init(alloc, &store, 30_000, .{}, .deny_all);

    var params = std.json.Value{ .object = std.json.ObjectMap.init(alloc) };
    defer params.object.deinit();
    try params.object.put("execution_id", .{ .string = "00000000000000000000000000000000" });

    const req_json = try protocol.serializeRequest(alloc, 1, protocol.Method.start_stage, params);
    defer alloc.free(req_json);

    const resp_json = try rpc_handler.handleFrame(alloc, req_json);
    defer alloc.free(resp_json);

    var resp = try protocol.parseResponse(alloc, resp_json);
    defer resp.deinit();
    try std.testing.expect(resp.rpc_error != null);
    try std.testing.expectEqual(@as(i32, protocol.ErrorCode.execution_failed), resp.rpc_error.?.code);
}

// SessionStore reapExpired cancels stale sessions.
// Uses page_allocator for session internals to avoid test allocator leak noise.
test "integration: lease expiry reaps orphaned sessions" {
    const page = std.heap.page_allocator;
    var store = session_mod.SessionStore.init(page);
    defer store.deinit();

    // Create a session with a very short lease.
    const session = try page.create(session_mod.Session);
    session.* = try session_mod.Session.create(page, "/tmp/test", .{
        .trace_id = "t",
        .zombie_id = "r",
        .workspace_id = "w",
        .session_id = "s",
    }, .{}, 1, .{}); // 1ms lease timeout

    try store.put(session);
    try std.testing.expectEqual(@as(usize, 1), store.activeCount());

    // Wait for lease to expire.
    std.Thread.sleep(5 * std.time.ns_per_ms);

    const reaped = store.reapExpired();
    try std.testing.expect(reaped >= 1);
    try std.testing.expectEqual(@as(usize, 0), store.activeCount());
}
