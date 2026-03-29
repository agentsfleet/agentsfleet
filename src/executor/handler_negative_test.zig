//! Handler negative-path tests: null params, malformed IDs, unknown IDs, unknown methods.
//!
//! Extracted from handler_edge_test.zig to keep each file under 500 lines.
//! Covers (T3):
//! - All 6 data methods with null params → invalid_params
//! - Malformed execution_id (not 32-char hex) → invalid_params
//! - Valid-format but unknown execution_id → execution_failed for every method
//! - Unknown method names → method_not_found

const std = @import("std");
const handler_mod = @import("handler.zig");
const session_mod = @import("session.zig");
const protocol = @import("protocol.zig");

fn makeHandler(alloc: std.mem.Allocator, store: *session_mod.SessionStore) handler_mod.Handler {
    return handler_mod.Handler.init(alloc, store, 30_000, .{});
}

// ─────────────────────────────────────────────────────────────────────────────
// T3: All data methods with null params → invalid_params
// ─────────────────────────────────────────────────────────────────────────────

test "T3: CreateExecution with null params returns invalid_params" {
    const alloc = std.testing.allocator;
    var store = session_mod.SessionStore.init(alloc);
    defer store.deinit();
    var h = makeHandler(alloc, &store);

    const req = try protocol.serializeRequest(alloc, 1, protocol.Method.create_execution, null);
    defer alloc.free(req);
    const resp = try h.handleFrame(alloc, req);
    defer alloc.free(resp);

    var r = try protocol.parseResponse(alloc, resp);
    defer r.deinit();
    try std.testing.expect(r.rpc_error != null);
    try std.testing.expectEqual(@as(i32, protocol.ErrorCode.invalid_params), r.rpc_error.?.code);
}

test "T3: StartStage with null params returns invalid_params" {
    const alloc = std.testing.allocator;
    var store = session_mod.SessionStore.init(alloc);
    defer store.deinit();
    var h = makeHandler(alloc, &store);

    const req = try protocol.serializeRequest(alloc, 2, protocol.Method.start_stage, null);
    defer alloc.free(req);
    const resp = try h.handleFrame(alloc, req);
    defer alloc.free(resp);

    var r = try protocol.parseResponse(alloc, resp);
    defer r.deinit();
    try std.testing.expect(r.rpc_error != null);
    try std.testing.expectEqual(@as(i32, protocol.ErrorCode.invalid_params), r.rpc_error.?.code);
}

test "T3: CancelExecution with null params returns invalid_params" {
    const alloc = std.testing.allocator;
    var store = session_mod.SessionStore.init(alloc);
    defer store.deinit();
    var h = makeHandler(alloc, &store);

    const req = try protocol.serializeRequest(alloc, 3, protocol.Method.cancel_execution, null);
    defer alloc.free(req);
    const resp = try h.handleFrame(alloc, req);
    defer alloc.free(resp);

    var r = try protocol.parseResponse(alloc, resp);
    defer r.deinit();
    try std.testing.expect(r.rpc_error != null);
    try std.testing.expectEqual(@as(i32, protocol.ErrorCode.invalid_params), r.rpc_error.?.code);
}

test "T3: GetUsage with null params returns invalid_params" {
    const alloc = std.testing.allocator;
    var store = session_mod.SessionStore.init(alloc);
    defer store.deinit();
    var h = makeHandler(alloc, &store);

    const req = try protocol.serializeRequest(alloc, 4, protocol.Method.get_usage, null);
    defer alloc.free(req);
    const resp = try h.handleFrame(alloc, req);
    defer alloc.free(resp);

    var r = try protocol.parseResponse(alloc, resp);
    defer r.deinit();
    try std.testing.expect(r.rpc_error != null);
    try std.testing.expectEqual(@as(i32, protocol.ErrorCode.invalid_params), r.rpc_error.?.code);
}

test "T3: DestroyExecution with null params returns invalid_params" {
    const alloc = std.testing.allocator;
    var store = session_mod.SessionStore.init(alloc);
    defer store.deinit();
    var h = makeHandler(alloc, &store);

    const req = try protocol.serializeRequest(alloc, 5, protocol.Method.destroy_execution, null);
    defer alloc.free(req);
    const resp = try h.handleFrame(alloc, req);
    defer alloc.free(resp);

    var r = try protocol.parseResponse(alloc, resp);
    defer r.deinit();
    try std.testing.expect(r.rpc_error != null);
    try std.testing.expectEqual(@as(i32, protocol.ErrorCode.invalid_params), r.rpc_error.?.code);
}

test "T3: Heartbeat with null params returns invalid_params" {
    const alloc = std.testing.allocator;
    var store = session_mod.SessionStore.init(alloc);
    defer store.deinit();
    var h = makeHandler(alloc, &store);

    const req = try protocol.serializeRequest(alloc, 6, protocol.Method.heartbeat, null);
    defer alloc.free(req);
    const resp = try h.handleFrame(alloc, req);
    defer alloc.free(resp);

    var r = try protocol.parseResponse(alloc, resp);
    defer r.deinit();
    try std.testing.expect(r.rpc_error != null);
    try std.testing.expectEqual(@as(i32, protocol.ErrorCode.invalid_params), r.rpc_error.?.code);
}

// ─────────────────────────────────────────────────────────────────────────────
// T3: Malformed execution_id (not 32-char hex) → invalid_params
// ─────────────────────────────────────────────────────────────────────────────

const MALFORMED_IDS = [_][]const u8{
    "",                                    // empty
    "not-hex",                             // too short, non-hex
    "GGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGG",    // 32 chars but invalid hex chars
    "0000000000000000000000000000000",     // 31 chars (one short)
    "000000000000000000000000000000000",   // 33 chars (one long)
};

test "T3: StartStage with malformed execution_id returns invalid_params" {
    const alloc = std.testing.allocator;
    var store = session_mod.SessionStore.init(alloc);
    defer store.deinit();
    var h = makeHandler(alloc, &store);

    for (MALFORMED_IDS) |bad_id| {
        var p = std.json.Value{ .object = std.json.ObjectMap.init(alloc) };
        defer p.object.deinit();
        try p.object.put("execution_id", .{ .string = bad_id });
        try p.object.put("stage_id", .{ .string = "s" });

        const req = try protocol.serializeRequest(alloc, 1, protocol.Method.start_stage, p);
        defer alloc.free(req);
        const resp = try h.handleFrame(alloc, req);
        defer alloc.free(resp);

        var r = try protocol.parseResponse(alloc, resp);
        defer r.deinit();
        try std.testing.expect(r.rpc_error != null);
        try std.testing.expectEqual(@as(i32, protocol.ErrorCode.invalid_params), r.rpc_error.?.code);
    }
}

test "T3: GetUsage with malformed execution_id returns invalid_params" {
    const alloc = std.testing.allocator;
    var store = session_mod.SessionStore.init(alloc);
    defer store.deinit();
    var h = makeHandler(alloc, &store);

    var p = std.json.Value{ .object = std.json.ObjectMap.init(alloc) };
    defer p.object.deinit();
    try p.object.put("execution_id", .{ .string = "not-a-valid-id" });

    const req = try protocol.serializeRequest(alloc, 1, protocol.Method.get_usage, p);
    defer alloc.free(req);
    const resp = try h.handleFrame(alloc, req);
    defer alloc.free(resp);

    var r = try protocol.parseResponse(alloc, resp);
    defer r.deinit();
    try std.testing.expect(r.rpc_error != null);
    try std.testing.expectEqual(@as(i32, protocol.ErrorCode.invalid_params), r.rpc_error.?.code);
}

// ─────────────────────────────────────────────────────────────────────────────
// T3: Unknown execution_id (valid hex, no such session) → execution_failed
// ─────────────────────────────────────────────────────────────────────────────

const NONEXISTENT_ID = "deadbeefdeadbeefdeadbeefdeadbeef";

test "T3: StartStage with unknown execution_id returns execution_failed" {
    const alloc = std.testing.allocator;
    var store = session_mod.SessionStore.init(alloc);
    defer store.deinit();
    var h = makeHandler(alloc, &store);

    var p = std.json.Value{ .object = std.json.ObjectMap.init(alloc) };
    defer p.object.deinit();
    try p.object.put("execution_id", .{ .string = NONEXISTENT_ID });
    try p.object.put("stage_id", .{ .string = "s" });

    const req = try protocol.serializeRequest(alloc, 1, protocol.Method.start_stage, p);
    defer alloc.free(req);
    const resp = try h.handleFrame(alloc, req);
    defer alloc.free(resp);

    var r = try protocol.parseResponse(alloc, resp);
    defer r.deinit();
    try std.testing.expect(r.rpc_error != null);
    try std.testing.expectEqual(@as(i32, protocol.ErrorCode.execution_failed), r.rpc_error.?.code);
}

test "T3: GetUsage with unknown execution_id returns execution_failed" {
    const alloc = std.testing.allocator;
    var store = session_mod.SessionStore.init(alloc);
    defer store.deinit();
    var h = makeHandler(alloc, &store);

    var p = std.json.Value{ .object = std.json.ObjectMap.init(alloc) };
    defer p.object.deinit();
    try p.object.put("execution_id", .{ .string = NONEXISTENT_ID });

    const req = try protocol.serializeRequest(alloc, 1, protocol.Method.get_usage, p);
    defer alloc.free(req);
    const resp = try h.handleFrame(alloc, req);
    defer alloc.free(resp);

    var r = try protocol.parseResponse(alloc, resp);
    defer r.deinit();
    try std.testing.expect(r.rpc_error != null);
    try std.testing.expectEqual(@as(i32, protocol.ErrorCode.execution_failed), r.rpc_error.?.code);
}

test "T3: CancelExecution with unknown execution_id returns execution_failed" {
    const alloc = std.testing.allocator;
    var store = session_mod.SessionStore.init(alloc);
    defer store.deinit();
    var h = makeHandler(alloc, &store);

    var p = std.json.Value{ .object = std.json.ObjectMap.init(alloc) };
    defer p.object.deinit();
    try p.object.put("execution_id", .{ .string = NONEXISTENT_ID });

    const req = try protocol.serializeRequest(alloc, 1, protocol.Method.cancel_execution, p);
    defer alloc.free(req);
    const resp = try h.handleFrame(alloc, req);
    defer alloc.free(resp);

    var r = try protocol.parseResponse(alloc, resp);
    defer r.deinit();
    try std.testing.expect(r.rpc_error != null);
    try std.testing.expectEqual(@as(i32, protocol.ErrorCode.execution_failed), r.rpc_error.?.code);
}

test "T3: Heartbeat with unknown execution_id returns execution_failed" {
    const alloc = std.testing.allocator;
    var store = session_mod.SessionStore.init(alloc);
    defer store.deinit();
    var h = makeHandler(alloc, &store);

    var p = std.json.Value{ .object = std.json.ObjectMap.init(alloc) };
    defer p.object.deinit();
    try p.object.put("execution_id", .{ .string = NONEXISTENT_ID });

    const req = try protocol.serializeRequest(alloc, 1, protocol.Method.heartbeat, p);
    defer alloc.free(req);
    const resp = try h.handleFrame(alloc, req);
    defer alloc.free(resp);

    var r = try protocol.parseResponse(alloc, resp);
    defer r.deinit();
    try std.testing.expect(r.rpc_error != null);
    try std.testing.expectEqual(@as(i32, protocol.ErrorCode.execution_failed), r.rpc_error.?.code);
}

test "T3: DestroyExecution with unknown execution_id returns execution_failed" {
    const alloc = std.testing.allocator;
    var store = session_mod.SessionStore.init(alloc);
    defer store.deinit();
    var h = makeHandler(alloc, &store);

    var p = std.json.Value{ .object = std.json.ObjectMap.init(alloc) };
    defer p.object.deinit();
    try p.object.put("execution_id", .{ .string = NONEXISTENT_ID });

    const req = try protocol.serializeRequest(alloc, 1, protocol.Method.destroy_execution, p);
    defer alloc.free(req);
    const resp = try h.handleFrame(alloc, req);
    defer alloc.free(resp);

    var r = try protocol.parseResponse(alloc, resp);
    defer r.deinit();
    try std.testing.expect(r.rpc_error != null);
    try std.testing.expectEqual(@as(i32, protocol.ErrorCode.execution_failed), r.rpc_error.?.code);
}

// ─────────────────────────────────────────────────────────────────────────────
// T3: Unknown method names → method_not_found
// ─────────────────────────────────────────────────────────────────────────────

test "T3: unknown method returns method_not_found" {
    const alloc = std.testing.allocator;
    var store = session_mod.SessionStore.init(alloc);
    defer store.deinit();
    var h = makeHandler(alloc, &store);

    const methods = [_][]const u8{
        "SomeUnknownMethod",
        "createExecution", // wrong case
        "StartStage_v2",   // future variant not yet implemented
        "",                // empty method name
    };

    for (methods, 0..) |method, i| {
        const req = try protocol.serializeRequest(alloc, @intCast(i + 100), method, null);
        defer alloc.free(req);
        const resp = try h.handleFrame(alloc, req);
        defer alloc.free(resp);

        var r = try protocol.parseResponse(alloc, resp);
        defer r.deinit();
        try std.testing.expect(r.rpc_error != null);
        try std.testing.expectEqual(@as(i32, protocol.ErrorCode.method_not_found), r.rpc_error.?.code);
    }
}
