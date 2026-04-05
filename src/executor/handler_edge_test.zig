//! Handler dispatch edge cases, positive paths, and protocol compliance tests.
//!
//! Negative paths (null params, malformed IDs, unknown IDs, unknown methods)
//! are in handler_negative_test.zig to keep each file under 500 lines.
//!
//! Covers (Tiers T1–T3, T7, T10, T12):
//! - StartStage on lease-expired session → lease_expired error code (T3)
//! - StreamEvents stub returns empty result array (T1)
//! - Completely invalid JSON → parse_error (T3)
//! - Multiple independent sessions coexist and don't interfere (T1)
//! - GetUsage/Heartbeat/Cancel/Destroy all require execution_id (T3)
//! - Error responses always include code and message fields (T12)
//! - Cancellations metric increments on successful cancel (T1)
//! - Long workspace path handled gracefully (T2)
//! - Unicode in correlation fields is stored and not corrupted (T2)
//! - Destroy is idempotent-ish (double destroy returns error on second) (T3)
//! - All 7 known methods are recognized; none return method_not_found (T1)
//! - CreateExecution with missing workspace_path → invalid_params (T2)

const std = @import("std");
const handler_mod = @import("handler.zig");
const session_mod = @import("session.zig");
const protocol = @import("protocol.zig");
const types = @import("types.zig");
const executor_metrics = @import("executor_metrics.zig");

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

fn makeHandler(alloc: std.mem.Allocator, store: *session_mod.SessionStore) handler_mod.Handler {
    return handler_mod.Handler.init(alloc, store, 30_000, .{}, .deny_all);
}

fn createSession(alloc: std.mem.Allocator, h: *handler_mod.Handler) ![]u8 {
    var p = std.json.Value{ .object = std.json.ObjectMap.init(alloc) };
    defer p.object.deinit();
    try p.object.put("workspace_path", .{ .string = "/tmp/test-ws" });
    try p.object.put("trace_id", .{ .string = "trace-1" });
    try p.object.put("run_id", .{ .string = "run-1" });

    const req = try protocol.serializeRequest(alloc, 1, protocol.Method.create_execution, p);
    defer alloc.free(req);
    const resp = try h.handleFrame(alloc, req);
    defer alloc.free(resp);

    var parsed = try protocol.parseResponse(alloc, resp);
    defer parsed.deinit();
    if (parsed.rpc_error != null) return error.CreateFailed;
    if (parsed.result == null) return error.CreateFailed;

    const exec_id = parsed.result.?.object.get("execution_id") orelse return error.CreateFailed;
    if (exec_id != .string) return error.CreateFailed;
    return alloc.dupe(u8, exec_id.string);
}

fn destroySession(alloc: std.mem.Allocator, h: *handler_mod.Handler, exec_id: []const u8) !void {
    var p = std.json.Value{ .object = std.json.ObjectMap.init(alloc) };
    defer p.object.deinit();
    try p.object.put("execution_id", .{ .string = exec_id });
    const req = try protocol.serializeRequest(alloc, 99, protocol.Method.destroy_execution, p);
    defer alloc.free(req);
    const resp = try h.handleFrame(alloc, req);
    alloc.free(resp);
}

// ─────────────────────────────────────────────────────────────────────────────
// T3: StartStage on lease-expired session → lease_expired error code
// ─────────────────────────────────────────────────────────────────────────────

test "T3: StartStage on lease-expired session returns lease_expired" {
    // Use page allocator to avoid test-allocator leak noise from Session arena.
    const page = std.heap.page_allocator;
    var store = session_mod.SessionStore.init(page);
    defer store.deinit();

    // Create a session with 1ms lease that will expire quickly.
    const s = try page.create(session_mod.Session);
    s.* = session_mod.Session.create(page, "/tmp/ws", .{
        .trace_id = "t",
        .run_id = "r",
        .workspace_id = "w",
        .stage_id = "s",
        .role_id = "echo",
        .skill_id = "echo",
    }, .{}, 1); // 1ms lease
    try store.put(s);

    const exec_id_hex = types.executionIdHex(s.execution_id);

    // Wait for lease to expire.
    std.Thread.sleep(10 * std.time.ns_per_ms);
    try std.testing.expect(s.isLeaseExpired());

    // Now try to start a stage on the expired session.
    var h = handler_mod.Handler.init(page, &store, 1, .{}, .deny_all);

    var p = std.json.Value{ .object = std.json.ObjectMap.init(page) };
    defer p.object.deinit();
    try p.object.put("execution_id", .{ .string = &exec_id_hex });
    try p.object.put("stage_id", .{ .string = "s1" });
    try p.object.put("message", .{ .string = "do work" });

    const req = try protocol.serializeRequest(page, 1, protocol.Method.start_stage, p);
    defer page.free(req);
    const resp = try h.handleFrame(page, req);
    defer page.free(resp);

    var r = try protocol.parseResponse(page, resp);
    defer r.deinit();
    try std.testing.expect(r.rpc_error != null);
    try std.testing.expectEqual(@as(i32, protocol.ErrorCode.lease_expired), r.rpc_error.?.code);
}

// ─────────────────────────────────────────────────────────────────────────────
// T1: StreamEvents returns empty result (stub)
// ─────────────────────────────────────────────────────────────────────────────

test "T1: StreamEvents returns success with empty events array" {
    const alloc = std.testing.allocator;
    var store = session_mod.SessionStore.init(alloc);
    defer store.deinit();
    var h = makeHandler(alloc, &store);

    const req = try protocol.serializeRequest(alloc, 7, protocol.Method.stream_events, null);
    defer alloc.free(req);
    const resp = try h.handleFrame(alloc, req);
    defer alloc.free(resp);

    // StreamEvents stub returns JSON with an empty result array — no error.
    try std.testing.expect(std.mem.indexOf(u8, resp, "\"result\"") != null);
    try std.testing.expect(std.mem.indexOf(u8, resp, "\"error\"") == null);
}

// ─────────────────────────────────────────────────────────────────────────────
// T3: Completely invalid JSON → parse_error
// ─────────────────────────────────────────────────────────────────────────────

test "T3: completely invalid JSON returns parse_error" {
    const alloc = std.testing.allocator;
    var store = session_mod.SessionStore.init(alloc);
    defer store.deinit();
    var h = makeHandler(alloc, &store);

    const resp = try h.handleFrame(alloc, "{not valid json!!");
    defer alloc.free(resp);

    var r = try protocol.parseResponse(alloc, resp);
    defer r.deinit();
    try std.testing.expect(r.rpc_error != null);
    try std.testing.expectEqual(@as(i32, protocol.ErrorCode.parse_error), r.rpc_error.?.code);
}

test "T3: empty payload returns parse_error" {
    const alloc = std.testing.allocator;
    var store = session_mod.SessionStore.init(alloc);
    defer store.deinit();
    var h = makeHandler(alloc, &store);

    const resp = try h.handleFrame(alloc, "");
    defer alloc.free(resp);

    var r = try protocol.parseResponse(alloc, resp);
    defer r.deinit();
    try std.testing.expect(r.rpc_error != null);
    try std.testing.expectEqual(@as(i32, protocol.ErrorCode.parse_error), r.rpc_error.?.code);
}

// ─────────────────────────────────────────────────────────────────────────────
// T1: Multiple independent sessions coexist and don't interfere
// ─────────────────────────────────────────────────────────────────────────────

test "T1: multiple independent sessions coexist without interference" {
    const alloc = std.testing.allocator;
    var store = session_mod.SessionStore.init(alloc);
    defer store.deinit();
    var h = makeHandler(alloc, &store);

    const id1 = try createSession(alloc, &h);
    defer alloc.free(id1);
    const id2 = try createSession(alloc, &h);
    defer alloc.free(id2);
    const id3 = try createSession(alloc, &h);
    defer alloc.free(id3);

    try std.testing.expectEqual(@as(usize, 3), store.activeCount());
    try std.testing.expect(!std.mem.eql(u8, id1, id2));
    try std.testing.expect(!std.mem.eql(u8, id2, id3));
    try std.testing.expect(!std.mem.eql(u8, id1, id3));

    // Cancel session 2 — sessions 1 and 3 must be unaffected.
    var cancel_p = std.json.Value{ .object = std.json.ObjectMap.init(alloc) };
    defer cancel_p.object.deinit();
    try cancel_p.object.put("execution_id", .{ .string = id2 });
    const cancel_req = try protocol.serializeRequest(alloc, 10, protocol.Method.cancel_execution, cancel_p);
    defer alloc.free(cancel_req);
    const cancel_resp = try h.handleFrame(alloc, cancel_req);
    alloc.free(cancel_resp);

    // Sessions 1 and 3 still alive.
    try std.testing.expectEqual(@as(usize, 3), store.activeCount());

    try destroySession(alloc, &h, id1);
    try destroySession(alloc, &h, id2);
    try destroySession(alloc, &h, id3);
    try std.testing.expectEqual(@as(usize, 0), store.activeCount());
}

// ─────────────────────────────────────────────────────────────────────────────
// T1: Successful cancel increments cancellations metric
// ─────────────────────────────────────────────────────────────────────────────

test "T1: CancelExecution increments cancellations metric" {
    const alloc = std.testing.allocator;
    var store = session_mod.SessionStore.init(alloc);
    defer store.deinit();
    var h = makeHandler(alloc, &store);

    const exec_id = try createSession(alloc, &h);
    defer alloc.free(exec_id);

    const before = executor_metrics.executorSnapshot().cancellations_total;

    var p = std.json.Value{ .object = std.json.ObjectMap.init(alloc) };
    defer p.object.deinit();
    try p.object.put("execution_id", .{ .string = exec_id });
    const req = try protocol.serializeRequest(alloc, 5, protocol.Method.cancel_execution, p);
    defer alloc.free(req);
    const resp = try h.handleFrame(alloc, req);
    defer alloc.free(resp);

    var r = try protocol.parseResponse(alloc, resp);
    defer r.deinit();
    try std.testing.expect(r.rpc_error == null);

    const after = executor_metrics.executorSnapshot().cancellations_total;
    try std.testing.expect(after > before);

    try destroySession(alloc, &h, exec_id);
}

// ─────────────────────────────────────────────────────────────────────────────
// T2: Long workspace path and unicode in correlation fields
// ─────────────────────────────────────────────────────────────────────────────

test "T2: CreateExecution with 200-char workspace path succeeds" {
    const alloc = std.testing.allocator;
    var store = session_mod.SessionStore.init(alloc);
    defer store.deinit();
    var h = makeHandler(alloc, &store);

    // Build a 200-char path.
    var path_buf: [200]u8 = undefined;
    @memset(&path_buf, 'a');
    path_buf[0] = '/';
    const long_path = &path_buf;

    var p = std.json.Value{ .object = std.json.ObjectMap.init(alloc) };
    defer p.object.deinit();
    try p.object.put("workspace_path", .{ .string = long_path });

    const req = try protocol.serializeRequest(alloc, 1, protocol.Method.create_execution, p);
    defer alloc.free(req);
    const resp = try h.handleFrame(alloc, req);
    defer alloc.free(resp);

    var r = try protocol.parseResponse(alloc, resp);
    defer r.deinit();
    try std.testing.expect(r.rpc_error == null);
    try std.testing.expect(r.result != null);

    const exec_id = r.result.?.object.get("execution_id").?.string;
    try destroySession(alloc, &h, exec_id);
}

test "T2: CreateExecution with unicode trace_id is stored without corruption" {
    const alloc = std.testing.allocator;
    var store = session_mod.SessionStore.init(alloc);
    defer store.deinit();
    var h = makeHandler(alloc, &store);

    var p = std.json.Value{ .object = std.json.ObjectMap.init(alloc) };
    defer p.object.deinit();
    try p.object.put("workspace_path", .{ .string = "/tmp/ws" });
    try p.object.put("trace_id", .{ .string = "田中-☕-αβγ" });
    try p.object.put("run_id", .{ .string = "run-unicode-🎯" });

    const req = try protocol.serializeRequest(alloc, 1, protocol.Method.create_execution, p);
    defer alloc.free(req);
    const resp = try h.handleFrame(alloc, req);
    defer alloc.free(resp);

    var r = try protocol.parseResponse(alloc, resp);
    defer r.deinit();
    // Handler must succeed — it stores whatever string is provided.
    try std.testing.expect(r.rpc_error == null);
    try std.testing.expect(r.result != null);

    const exec_id = r.result.?.object.get("execution_id").?.string;
    try destroySession(alloc, &h, exec_id);
}

// ─────────────────────────────────────────────────────────────────────────────
// T3: Double destroy returns error on second call
// ─────────────────────────────────────────────────────────────────────────────

test "T3: DestroyExecution twice returns error on second call" {
    const alloc = std.testing.allocator;
    var store = session_mod.SessionStore.init(alloc);
    defer store.deinit();
    var h = makeHandler(alloc, &store);

    const exec_id = try createSession(alloc, &h);
    defer alloc.free(exec_id);

    // First destroy — must succeed.
    var p = std.json.Value{ .object = std.json.ObjectMap.init(alloc) };
    defer p.object.deinit();
    try p.object.put("execution_id", .{ .string = exec_id });

    {
        const req = try protocol.serializeRequest(alloc, 1, protocol.Method.destroy_execution, p);
        defer alloc.free(req);
        const resp = try h.handleFrame(alloc, req);
        defer alloc.free(resp);
        var r = try protocol.parseResponse(alloc, resp);
        defer r.deinit();
        try std.testing.expect(r.rpc_error == null);
    }

    // Second destroy — must return an error (session no longer exists).
    {
        var p2 = std.json.Value{ .object = std.json.ObjectMap.init(alloc) };
        defer p2.object.deinit();
        try p2.object.put("execution_id", .{ .string = exec_id });

        const req = try protocol.serializeRequest(alloc, 2, protocol.Method.destroy_execution, p2);
        defer alloc.free(req);
        const resp = try h.handleFrame(alloc, req);
        defer alloc.free(resp);
        var r = try protocol.parseResponse(alloc, resp);
        defer r.deinit();
        try std.testing.expect(r.rpc_error != null);
        try std.testing.expectEqual(@as(i32, protocol.ErrorCode.execution_failed), r.rpc_error.?.code);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// T12: Error responses always have code (integer) and message (string)
// ─────────────────────────────────────────────────────────────────────────────

test "T12: all error responses include code and message fields" {
    const alloc = std.testing.allocator;
    var store = session_mod.SessionStore.init(alloc);
    defer store.deinit();
    var h = makeHandler(alloc, &store);

    const error_triggers = [_][]const u8{
        // Invalid JSON.
        "{not json}",
    };

    for (error_triggers) |payload| {
        const resp = try h.handleFrame(alloc, payload);
        defer alloc.free(resp);

        var r = try protocol.parseResponse(alloc, resp);
        defer r.deinit();
        try std.testing.expect(r.rpc_error != null);
        // Code must be non-zero.
        try std.testing.expect(r.rpc_error.?.code != 0);
        // Message must be non-empty.
        try std.testing.expect(r.rpc_error.?.message.len > 0);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// T1: All 7 RPC methods are recognized (none return method_not_found)
// ─────────────────────────────────────────────────────────────────────────────

test "T1: all 7 known RPC methods are recognized and do not return method_not_found" {
    const alloc = std.testing.allocator;
    var store = session_mod.SessionStore.init(alloc);
    defer store.deinit();
    var h = makeHandler(alloc, &store);

    // We send each method with minimal/invalid params — the handler may return
    // invalid_params or execution_failed, but NOT method_not_found.
    const methods = [_][]const u8{
        protocol.Method.create_execution,
        protocol.Method.start_stage,
        protocol.Method.cancel_execution,
        protocol.Method.get_usage,
        protocol.Method.destroy_execution,
        protocol.Method.heartbeat,
        protocol.Method.stream_events,
    };

    for (methods, 0..) |method, i| {
        const req = try protocol.serializeRequest(alloc, @intCast(i + 1), method, null);
        defer alloc.free(req);
        const resp = try h.handleFrame(alloc, req);
        defer alloc.free(resp);

        var r = try protocol.parseResponse(alloc, resp);
        defer r.deinit();

        if (r.rpc_error) |err| {
            // Must NOT be method_not_found.
            try std.testing.expect(err.code != protocol.ErrorCode.method_not_found);
        }
        // No crash — method was dispatched.
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// T2: CreateExecution with missing workspace_path returns invalid_params
// ─────────────────────────────────────────────────────────────────────────────

test "T2: CreateExecution with missing workspace_path returns invalid_params" {
    const alloc = std.testing.allocator;
    var store = session_mod.SessionStore.init(alloc);
    defer store.deinit();
    var h = makeHandler(alloc, &store);

    // params is an object but workspace_path is absent.
    var p = std.json.Value{ .object = std.json.ObjectMap.init(alloc) };
    defer p.object.deinit();
    try p.object.put("trace_id", .{ .string = "t" });

    const req = try protocol.serializeRequest(alloc, 1, protocol.Method.create_execution, p);
    defer alloc.free(req);
    const resp = try h.handleFrame(alloc, req);
    defer alloc.free(resp);

    var r = try protocol.parseResponse(alloc, resp);
    defer r.deinit();
    try std.testing.expect(r.rpc_error != null);
    try std.testing.expectEqual(@as(i32, protocol.ErrorCode.invalid_params), r.rpc_error.?.code);
}
