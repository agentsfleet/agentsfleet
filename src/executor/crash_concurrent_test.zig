//! Concurrency, parallel crash, and crash-recovery scenario tests.
//!
//! Scenario matrix (all must pass):
//! 1. Concurrent CreateExecution from N workers → unique execution IDs (T5)
//! 2. Concurrent Heartbeat while a cancel is in flight → session ends cancelled (T5)
//! 3. N workers create + destroy in parallel → store returns to zero (T5)
//! 4. Mass session expiry: 20 sessions all expire simultaneously (T5)
//! 5. Orphan reaper while heartbeat is active → active session NOT reaped (T3)
//! 6. Worker crash (close without destroy) → session becomes orphan, reaped (T3)
//! 7. Dual crash: worker + executor both gone → clean restart, new session (T3)
//! 8. Concurrent cancel + destroy on same session → no double-free / panic (T5)
//! 9. Concurrent GetUsage from multiple threads → consistent non-corrupt results (T5)
//! 10. Concurrent StartStage on cancelled session → all get execution_failed (T5)
//! 11. Session store grows and shrinks correctly across N parallel lifecycle runs (T5)
//! 12. Executor restart: new store has no stale state from previous instance (T3)

const std = @import("std");
const builtin = @import("builtin");
const transport = @import("transport.zig");
const handler_mod = @import("handler.zig");
const session_mod = @import("session.zig");
const protocol = @import("protocol.zig");
const types = @import("types.zig");
const client_mod = @import("client.zig");
const executor_metrics = @import("executor_metrics.zig");

// ─────────────────────────────────────────────────────────────────────────────
// 1. Concurrent CreateExecution → unique execution IDs
// ─────────────────────────────────────────────────────────────────────────────

test "T5: concurrent CreateExecution from 8 workers produces unique execution IDs" {
    if (builtin.os.tag == .windows) return error.SkipZigTest;

    const alloc = std.testing.allocator;
    var store = session_mod.SessionStore.init(alloc);
    defer store.deinit();

    var rpc_handler = handler_mod.Handler.init(alloc, &store, 30_000, .{}, .deny_all);

    const N = 8;
    var ids: [N][32]u8 = undefined;
    var mu = std.Thread.Mutex{};
    var id_count: usize = 0;

    const Worker = struct {
        fn run(
            h: *handler_mod.Handler,
            a: std.mem.Allocator,
            out_ids: *[N][32]u8,
            out_count: *usize,
            m: *std.Thread.Mutex,
            idx: usize,
        ) void {
            var params = std.json.Value{ .object = std.json.ObjectMap.init(a) };
            defer params.object.deinit();
            params.object.put("workspace_path", .{ .string = "/tmp/ws" }) catch return;
            params.object.put("trace_id", .{ .string = "t" }) catch return;
            params.object.put("zombie_id", .{ .string = "r" }) catch return;

            const req = protocol.serializeRequest(a, @intCast(idx), protocol.Method.create_execution, params) catch return;
            defer a.free(req);

            const resp_bytes = h.handleFrame(a, req) catch return;
            defer a.free(resp_bytes);

            var resp = protocol.parseResponse(a, resp_bytes) catch return;
            defer resp.deinit();
            if (resp.result == null) return;

            const exec_id = resp.result.?.object.get("execution_id") orelse return;
            if (exec_id != .string) return;
            if (exec_id.string.len != 32) return;

            m.lock();
            defer m.unlock();
            @memcpy(&out_ids[out_count.*], exec_id.string);
            out_count.* += 1;
        }
    };

    var threads: [N]std.Thread = undefined;
    for (&threads, 0..) |*t, i| {
        t.* = std.Thread.spawn(.{}, Worker.run, .{
            &rpc_handler, alloc, &ids, &id_count, &mu, i,
        }) catch unreachable;
    }
    for (&threads) |*t| t.join();

    // All N workers must have received a unique ID.
    try std.testing.expectEqual(N, id_count);

    // Check uniqueness.
    for (0..N) |i| {
        for (0..N) |j| {
            if (i != j) {
                const different = !std.mem.eql(u8, &ids[i], &ids[j]);
                try std.testing.expect(different);
            }
        }
    }

    // Verify all sessions exist (deferred store.deinit() above cleans up).
    try std.testing.expectEqual(N, store.activeCount());
}

// ─────────────────────────────────────────────────────────────────────────────
// 2. Concurrent Heartbeat + Cancel race — session ends cancelled
// ─────────────────────────────────────────────────────────────────────────────

test "T5: concurrent heartbeat and cancel race — session is cancelled" {
    const page = std.heap.page_allocator;
    var session = session_mod.Session.create(page, "/tmp/ws", .{
        .trace_id = "t",
        .zombie_id = "r",
        .workspace_id = "w",
        .session_id = "s",
    }, .{}, 30_000);
    defer session.destroy();

    const Heartbeater = struct {
        fn run(s: *session_mod.Session) void {
            for (0..500) |_| {
                s.touchLease();
                std.Thread.sleep(1);
            }
        }
    };
    const Canceller = struct {
        fn run(s: *session_mod.Session) void {
            s.cancel();
        }
    };

    const hb_thread = try std.Thread.spawn(.{}, Heartbeater.run, .{&session});
    const cancel_thread = try std.Thread.spawn(.{}, Canceller.run, .{&session});

    hb_thread.join();
    cancel_thread.join();

    // After both finish: session must be cancelled regardless of order.
    try std.testing.expect(session.isCancelled());
}

// ─────────────────────────────────────────────────────────────────────────────
// 3. N workers create + destroy in parallel → store returns to zero
// ─────────────────────────────────────────────────────────────────────────────

test "T5: 10 parallel worker lifecycle runs leave the session store empty" {
    if (builtin.os.tag == .windows) return error.SkipZigTest;

    const alloc = std.testing.allocator;
    var store = session_mod.SessionStore.init(alloc);
    defer store.deinit();

    var rpc_handler = handler_mod.Handler.init(alloc, &store, 30_000, .{}, .deny_all);

    const N = 10;

    const WorkerLifecycle = struct {
        fn run(h: *handler_mod.Handler, a: std.mem.Allocator, idx: usize) void {
            // CreateExecution.
            var create_p = std.json.Value{ .object = std.json.ObjectMap.init(a) };
            defer create_p.object.deinit();
            create_p.object.put("workspace_path", .{ .string = "/tmp/ws" }) catch return;

            var run_buf: [16]u8 = undefined;
            const zombie_id = std.fmt.bufPrint(&run_buf, "run-{d}", .{idx}) catch "run";
            create_p.object.put("zombie_id", .{ .string = zombie_id }) catch return;

            const create_req = protocol.serializeRequest(a, @intCast(idx * 10 + 1), protocol.Method.create_execution, create_p) catch return;
            defer a.free(create_req);
            const create_resp = h.handleFrame(a, create_req) catch return;
            defer a.free(create_resp);

            var parsed = protocol.parseResponse(a, create_resp) catch return;
            defer parsed.deinit();
            if (parsed.result == null) return;

            const exec_id = (parsed.result.?.object.get("execution_id") orelse return).string;

            // DestroyExecution.
            var destroy_p = std.json.Value{ .object = std.json.ObjectMap.init(a) };
            defer destroy_p.object.deinit();
            destroy_p.object.put("execution_id", .{ .string = exec_id }) catch return;

            const destroy_req = protocol.serializeRequest(a, @intCast(idx * 10 + 2), protocol.Method.destroy_execution, destroy_p) catch return;
            defer a.free(destroy_req);
            const destroy_resp = h.handleFrame(a, destroy_req) catch return;
            a.free(destroy_resp);
        }
    };

    var threads: [N]std.Thread = undefined;
    for (&threads, 0..) |*t, i| {
        t.* = std.Thread.spawn(.{}, WorkerLifecycle.run, .{ &rpc_handler, alloc, i }) catch unreachable;
    }
    for (&threads) |*t| t.join();

    // All sessions must have been destroyed.
    try std.testing.expectEqual(@as(usize, 0), store.activeCount());
}

// ─────────────────────────────────────────────────────────────────────────────
// 4. Mass session expiry — 20 sessions expire simultaneously
// ─────────────────────────────────────────────────────────────────────────────

test "T5: 20 sessions all expire simultaneously — reapExpired clears all" {
    const page = std.heap.page_allocator;
    var store = session_mod.SessionStore.init(page);
    defer store.deinit();

    for (0..20) |_| {
        const s = try page.create(session_mod.Session);
        // Use a string literal — stack-buffer session_id would outlive the loop
        // iteration and produce dangling pointers since Session stores the slice
        // header without copying the bytes.
        s.* = session_mod.Session.create(page, "/tmp/ws", .{
            .trace_id = "t",
            .zombie_id = "r",
            .workspace_id = "w",
            .session_id = "mass",
        }, .{}, 1); // 1ms — expires immediately
        try store.put(s);
    }

    try std.testing.expectEqual(@as(usize, 20), store.activeCount());

    std.Thread.sleep(10 * std.time.ns_per_ms);

    const reaped = store.reapExpired();
    try std.testing.expectEqual(@as(u32, 20), reaped);
    try std.testing.expectEqual(@as(usize, 0), store.activeCount());
}

// ─────────────────────────────────────────────────────────────────────────────
// 5. Orphan reaper does NOT kill active session with fresh heartbeat
// ─────────────────────────────────────────────────────────────────────────────

test "T3: reapExpired does not remove session with fresh heartbeat" {
    const page = std.heap.page_allocator;
    var store = session_mod.SessionStore.init(page);
    defer store.deinit();

    const s = try page.create(session_mod.Session);
    s.* = session_mod.Session.create(page, "/tmp/ws", .{
        .trace_id = "t",
        .zombie_id = "r",
        .workspace_id = "w",
        .session_id = "active",
    }, .{}, 30_000); // 30s lease
    try store.put(s);

    // Keep touching the lease so it never expires.
    s.touchLease();

    const reaped = store.reapExpired();
    try std.testing.expectEqual(@as(u32, 0), reaped);
    try std.testing.expectEqual(@as(usize, 1), store.activeCount());
}

// ─────────────────────────────────────────────────────────────────────────────
// 6. Worker crash (close without destroy) → session becomes orphan → reaped
// ─────────────────────────────────────────────────────────────────────────────

test "T3: worker crash simulation — no heartbeat → session reaped as orphan" {
    const page = std.heap.page_allocator;
    var store = session_mod.SessionStore.init(page);
    defer store.deinit();

    // Worker creates a session but then crashes — never sends heartbeat or destroy.
    const s = try page.create(session_mod.Session);
    s.* = session_mod.Session.create(page, "/tmp/ws", .{
        .trace_id = "t",
        .zombie_id = "r",
        .workspace_id = "w",
        .session_id = "orphan-1",
    }, .{}, 5); // 5ms lease — simulates worker crash timeout
    try store.put(s);

    try std.testing.expectEqual(@as(usize, 1), store.activeCount());

    // Simulate time passing while worker is dead (no heartbeats sent).
    std.Thread.sleep(20 * std.time.ns_per_ms);

    const reaped = store.reapExpired();
    try std.testing.expect(reaped >= 1);
    try std.testing.expectEqual(@as(usize, 0), store.activeCount());
}

// ─────────────────────────────────────────────────────────────────────────────
// 7. Executor restart: new store has no stale state from previous instance
// ─────────────────────────────────────────────────────────────────────────────

test "T3: executor restart: new SessionStore starts empty regardless of prior sessions" {
    const alloc = std.testing.allocator;

    // First store with sessions.
    var store1 = session_mod.SessionStore.init(alloc);
    const rpc_handler1 = handler_mod.Handler.init(alloc, &store1, 30_000, .{}, .deny_all);
    _ = rpc_handler1;

    var create_p = std.json.Value{ .object = std.json.ObjectMap.init(alloc) };
    defer create_p.object.deinit();
    try create_p.object.put("workspace_path", .{ .string = "/tmp/ws" });
    var handler1 = handler_mod.Handler.init(alloc, &store1, 30_000, .{}, .deny_all);
    const req = try protocol.serializeRequest(alloc, 1, protocol.Method.create_execution, create_p);
    defer alloc.free(req);
    const resp = try handler1.handleFrame(alloc, req);
    defer alloc.free(resp);

    try std.testing.expect(store1.activeCount() > 0);

    // "Executor restarts" — old store goes away, new one is created.
    store1.deinit();

    var store2 = session_mod.SessionStore.init(alloc);
    defer store2.deinit();

    // New store starts empty.
    try std.testing.expectEqual(@as(usize, 0), store2.activeCount());
}

// (Tests 8–12 are in crash_recovery_test.zig to keep this file under 500 lines)
