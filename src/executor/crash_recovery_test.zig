//! Advanced crash recovery and concurrent RPC stress tests.
//!
//! Extracted from crash_concurrent_test.zig (>500 lines rule).
//! Covers tests 8–12:
//! 8. Concurrent cancel + destroy on same session → no crash (T5)
//! 9. Concurrent GetUsage from 8 threads → consistent results (T5)
//! 10. Concurrent StartStage on cancelled session → all get execution_failed (T5)
//! 11. 16 parallel create+destroy cycles → store returns to zero (T5)
//! 12. sessions_created metric increments for each concurrent CreateExecution (T5)

const std = @import("std");
const builtin = @import("builtin");
const handler_mod = @import("handler.zig");
const session_mod = @import("session.zig");
const protocol = @import("protocol.zig");
const executor_metrics = @import("executor_metrics.zig");

// ─────────────────────────────────────────────────────────────────────────────
// 8. Concurrent cancel + destroy on same session — no crash
// ─────────────────────────────────────────────────────────────────────────────

test "T5: concurrent cancel and destroy on same session does not crash" {
    if (builtin.os.tag == .windows) return error.SkipZigTest;

    const alloc = std.testing.allocator;
    var store = session_mod.SessionStore.init(alloc);
    defer store.deinit();

    var rpc_handler = handler_mod.Handler.init(alloc, &store, 30_000, .{}, .deny_all);

    var create_p = std.json.Value{ .object = std.json.ObjectMap.init(alloc) };
    defer create_p.object.deinit();
    try create_p.object.put("workspace_path", .{ .string = "/tmp/ws" });
    const create_req = try protocol.serializeRequest(alloc, 1, protocol.Method.create_execution, create_p);
    defer alloc.free(create_req);
    const create_resp = try rpc_handler.handleFrame(alloc, create_req);
    defer alloc.free(create_resp);

    var parsed = try protocol.parseResponse(alloc, create_resp);
    defer parsed.deinit();
    try std.testing.expect(parsed.result != null);
    const exec_id_str = parsed.result.?.object.get("execution_id").?.string;

    const Sender = struct {
        fn run(h: *handler_mod.Handler, a: std.mem.Allocator, exec_id: []const u8, method: []const u8, req_id: u64) void {
            var p = std.json.Value{ .object = std.json.ObjectMap.init(a) };
            defer p.object.deinit();
            p.object.put("execution_id", .{ .string = exec_id }) catch return;
            const req = protocol.serializeRequest(a, req_id, method, p) catch return;
            defer a.free(req);
            const resp = h.handleFrame(a, req) catch return;
            a.free(resp);
        }
    };

    const t1 = try std.Thread.spawn(.{}, Sender.run, .{
        &rpc_handler, alloc, exec_id_str, protocol.Method.cancel_execution, 2,
    });
    const t2 = try std.Thread.spawn(.{}, Sender.run, .{
        &rpc_handler, alloc, exec_id_str, protocol.Method.destroy_execution, 3,
    });
    t1.join();
    t2.join();
}

// ─────────────────────────────────────────────────────────────────────────────
// 9. Concurrent GetUsage from 8 threads — consistent results
// ─────────────────────────────────────────────────────────────────────────────

test "T5: concurrent GetUsage from 8 threads returns consistent results" {
    if (builtin.os.tag == .windows) return error.SkipZigTest;

    const alloc = std.testing.allocator;
    var store = session_mod.SessionStore.init(alloc);
    defer store.deinit();

    var rpc_handler = handler_mod.Handler.init(alloc, &store, 30_000, .{}, .deny_all);

    var create_p = std.json.Value{ .object = std.json.ObjectMap.init(alloc) };
    defer create_p.object.deinit();
    try create_p.object.put("workspace_path", .{ .string = "/tmp/ws" });
    const create_req = try protocol.serializeRequest(alloc, 1, protocol.Method.create_execution, create_p);
    defer alloc.free(create_req);
    const create_resp = try rpc_handler.handleFrame(alloc, create_req);
    defer alloc.free(create_resp);

    var parsed = try protocol.parseResponse(alloc, create_resp);
    defer parsed.deinit();
    const exec_id_str = parsed.result.?.object.get("execution_id").?.string;

    var errors = std.atomic.Value(u32).init(0);

    const GetUser = struct {
        fn run(h: *handler_mod.Handler, a: std.mem.Allocator, exec_id: []const u8, err_count: *std.atomic.Value(u32), idx: usize) void {
            var p = std.json.Value{ .object = std.json.ObjectMap.init(a) };
            defer p.object.deinit();
            p.object.put("execution_id", .{ .string = exec_id }) catch return;
            const req = protocol.serializeRequest(a, @intCast(idx + 10), protocol.Method.get_usage, p) catch return;
            defer a.free(req);
            const resp = h.handleFrame(a, req) catch return;
            defer a.free(resp);
            var r = protocol.parseResponse(a, resp) catch {
                _ = err_count.fetchAdd(1, .monotonic);
                return;
            };
            defer r.deinit();
            if (r.rpc_error != null) _ = err_count.fetchAdd(1, .monotonic);
        }
    };

    var threads: [8]std.Thread = undefined;
    for (&threads, 0..) |*t, i| {
        t.* = std.Thread.spawn(.{}, GetUser.run, .{ &rpc_handler, alloc, exec_id_str, &errors, i }) catch unreachable;
    }
    for (&threads) |*t| t.join();

    try std.testing.expectEqual(@as(u32, 0), errors.load(.acquire));

    var destroy_p = std.json.Value{ .object = std.json.ObjectMap.init(alloc) };
    defer destroy_p.object.deinit();
    try destroy_p.object.put("execution_id", .{ .string = exec_id_str });
    const destroy_req = try protocol.serializeRequest(alloc, 99, protocol.Method.destroy_execution, destroy_p);
    defer alloc.free(destroy_req);
    const destroy_resp = try rpc_handler.handleFrame(alloc, destroy_req);
    alloc.free(destroy_resp);
}

// ─────────────────────────────────────────────────────────────────────────────
// 10. Concurrent StartStage on cancelled session → all get execution_failed
// ─────────────────────────────────────────────────────────────────────────────

test "T5: concurrent StartStage on cancelled session all return execution_failed" {
    if (builtin.os.tag == .windows) return error.SkipZigTest;

    const alloc = std.testing.allocator;
    var store = session_mod.SessionStore.init(alloc);
    defer store.deinit();

    var rpc_handler = handler_mod.Handler.init(alloc, &store, 30_000, .{}, .deny_all);

    var create_p = std.json.Value{ .object = std.json.ObjectMap.init(alloc) };
    defer create_p.object.deinit();
    try create_p.object.put("workspace_path", .{ .string = "/tmp/ws" });
    const create_req = try protocol.serializeRequest(alloc, 1, protocol.Method.create_execution, create_p);
    defer alloc.free(create_req);
    const create_resp = try rpc_handler.handleFrame(alloc, create_req);
    defer alloc.free(create_resp);

    var parsed_create = try protocol.parseResponse(alloc, create_resp);
    defer parsed_create.deinit();
    const exec_id_str = parsed_create.result.?.object.get("execution_id").?.string;

    var cancel_p = std.json.Value{ .object = std.json.ObjectMap.init(alloc) };
    defer cancel_p.object.deinit();
    try cancel_p.object.put("execution_id", .{ .string = exec_id_str });
    const cancel_req = try protocol.serializeRequest(alloc, 2, protocol.Method.cancel_execution, cancel_p);
    defer alloc.free(cancel_req);
    const cancel_resp = try rpc_handler.handleFrame(alloc, cancel_req);
    alloc.free(cancel_resp);

    const N = 5;
    var got_errors = std.atomic.Value(u32).init(0);

    const Starter = struct {
        fn run(h: *handler_mod.Handler, a: std.mem.Allocator, exec_id: []const u8, errs: *std.atomic.Value(u32), idx: usize) void {
            var p = std.json.Value{ .object = std.json.ObjectMap.init(a) };
            defer p.object.deinit();
            p.object.put("execution_id", .{ .string = exec_id }) catch return;
            p.object.put("stage_id", .{ .string = "s" }) catch return;
            p.object.put("message", .{ .string = "test" }) catch return;
            const req = protocol.serializeRequest(a, @intCast(idx + 10), protocol.Method.start_stage, p) catch return;
            defer a.free(req);
            const resp = h.handleFrame(a, req) catch return;
            defer a.free(resp);
            var r = protocol.parseResponse(a, resp) catch return;
            defer r.deinit();
            if (r.rpc_error != null and r.rpc_error.?.code == protocol.ErrorCode.execution_failed) {
                _ = errs.fetchAdd(1, .monotonic);
            }
        }
    };

    var threads: [N]std.Thread = undefined;
    for (&threads, 0..) |*t, i| {
        t.* = std.Thread.spawn(.{}, Starter.run, .{ &rpc_handler, alloc, exec_id_str, &got_errors, i }) catch unreachable;
    }
    for (&threads) |*t| t.join();

    try std.testing.expectEqual(@as(u32, N), got_errors.load(.acquire));

    var destroy_p = std.json.Value{ .object = std.json.ObjectMap.init(alloc) };
    defer destroy_p.object.deinit();
    try destroy_p.object.put("execution_id", .{ .string = exec_id_str });
    const destroy_req = try protocol.serializeRequest(alloc, 99, protocol.Method.destroy_execution, destroy_p);
    defer alloc.free(destroy_req);
    const destroy_resp = try rpc_handler.handleFrame(alloc, destroy_req);
    alloc.free(destroy_resp);
}

// ─────────────────────────────────────────────────────────────────────────────
// 11. Session store grows and shrinks correctly across 16 parallel lifecycle runs
// ─────────────────────────────────────────────────────────────────────────────

test "T5: session store count is correct after 16 parallel create+destroy cycles" {
    if (builtin.os.tag == .windows) return error.SkipZigTest;

    const alloc = std.testing.allocator;
    var store = session_mod.SessionStore.init(alloc);
    defer store.deinit();

    var rpc_handler = handler_mod.Handler.init(alloc, &store, 30_000, .{}, .deny_all);
    var completed = std.atomic.Value(u32).init(0);
    const N = 16;

    const Runner = struct {
        fn run(h: *handler_mod.Handler, a: std.mem.Allocator, idx: usize, done: *std.atomic.Value(u32)) void {
            var cp = std.json.Value{ .object = std.json.ObjectMap.init(a) };
            defer cp.object.deinit();
            cp.object.put("workspace_path", .{ .string = "/tmp/ws" }) catch return;
            var id_buf: [16]u8 = undefined;
            const run_id = std.fmt.bufPrint(&id_buf, "r{d}", .{idx}) catch "r";
            cp.object.put("run_id", .{ .string = run_id }) catch return;

            const cr = protocol.serializeRequest(a, @intCast(idx * 3 + 1), protocol.Method.create_execution, cp) catch return;
            defer a.free(cr);
            const cresp = h.handleFrame(a, cr) catch return;
            defer a.free(cresp);

            var parsed = protocol.parseResponse(a, cresp) catch return;
            defer parsed.deinit();
            if (parsed.result == null) return;
            const eid = (parsed.result.?.object.get("execution_id") orelse return).string;

            var dp = std.json.Value{ .object = std.json.ObjectMap.init(a) };
            defer dp.object.deinit();
            dp.object.put("execution_id", .{ .string = eid }) catch return;
            const dr = protocol.serializeRequest(a, @intCast(idx * 3 + 2), protocol.Method.destroy_execution, dp) catch return;
            defer a.free(dr);
            const dresp = h.handleFrame(a, dr) catch return;
            a.free(dresp);

            _ = done.fetchAdd(1, .monotonic);
        }
    };

    var threads: [N]std.Thread = undefined;
    for (&threads, 0..) |*t, i| {
        t.* = std.Thread.spawn(.{}, Runner.run, .{ &rpc_handler, alloc, i, &completed }) catch unreachable;
    }
    for (&threads) |*t| t.join();

    try std.testing.expectEqual(@as(u32, N), completed.load(.acquire));
    try std.testing.expectEqual(@as(usize, 0), store.activeCount());
}

// ─────────────────────────────────────────────────────────────────────────────
// 12. sessions_created metric increments for each concurrent CreateExecution
// ─────────────────────────────────────────────────────────────────────────────

test "T5: sessions_created metric increments for each concurrent CreateExecution" {
    if (builtin.os.tag == .windows) return error.SkipZigTest;

    const alloc = std.testing.allocator;
    var store = session_mod.SessionStore.init(alloc);
    defer store.deinit();

    var rpc_handler = handler_mod.Handler.init(alloc, &store, 30_000, .{}, .deny_all);
    const snap_before = executor_metrics.executorSnapshot();
    const N = 5;

    const Creator = struct {
        fn run(h: *handler_mod.Handler, a: std.mem.Allocator, idx: usize) void {
            var cp = std.json.Value{ .object = std.json.ObjectMap.init(a) };
            defer cp.object.deinit();
            cp.object.put("workspace_path", .{ .string = "/tmp/ws" }) catch return;
            const req = protocol.serializeRequest(a, @intCast(idx), protocol.Method.create_execution, cp) catch return;
            defer a.free(req);
            const resp = h.handleFrame(a, req) catch return;
            a.free(resp);
        }
    };

    var threads: [N]std.Thread = undefined;
    for (&threads, 0..) |*t, i| {
        t.* = std.Thread.spawn(.{}, Creator.run, .{ &rpc_handler, alloc, i + 1 }) catch unreachable;
    }
    for (&threads) |*t| t.join();

    const snap_after = executor_metrics.executorSnapshot();
    try std.testing.expect(snap_after.sessions_created_total >= snap_before.sessions_created_total + N);
}
