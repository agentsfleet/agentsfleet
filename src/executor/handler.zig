//! RPC dispatch handler for the executor sidecar.
//!
//! Maps JSON-RPC method names to session lifecycle operations.
//! This is the core of the executor API (§2.0).

const std = @import("std");
const protocol = @import("protocol.zig");
const types = @import("types.zig");
const session_mod = @import("session.zig");
const executor_metrics = @import("executor_metrics.zig");

const log = std.log.scoped(.executor_handler);

pub const Handler = struct {
    store: *session_mod.SessionStore,
    lease_timeout_ms: u64,
    resource_limits: types.ResourceLimits,
    alloc: std.mem.Allocator,

    pub fn init(
        alloc: std.mem.Allocator,
        store: *session_mod.SessionStore,
        lease_timeout_ms: u64,
        resource_limits: types.ResourceLimits,
    ) Handler {
        return .{
            .store = store,
            .lease_timeout_ms = lease_timeout_ms,
            .resource_limits = resource_limits,
            .alloc = alloc,
        };
    }

    /// Top-level frame handler called by the transport layer.
    pub fn handleFrame(self: *Handler, alloc: std.mem.Allocator, payload: []const u8) ![]u8 {
        var req = protocol.parseRequest(alloc, payload) catch {
            return self.errorResponse(alloc, 0, protocol.ErrorCode.parse_error, "Invalid JSON-RPC request");
        };
        defer req.deinit();

        return self.dispatch(alloc, req.id, req.method, req.params);
    }

    fn dispatch(self: *Handler, alloc: std.mem.Allocator, id: u64, method: []const u8, params: ?std.json.Value) ![]u8 {
        if (std.mem.eql(u8, method, protocol.Method.create_execution)) {
            return self.handleCreateExecution(alloc, id, params);
        } else if (std.mem.eql(u8, method, protocol.Method.start_stage)) {
            return self.handleStartStage(alloc, id, params);
        } else if (std.mem.eql(u8, method, protocol.Method.cancel_execution)) {
            return self.handleCancelExecution(alloc, id, params);
        } else if (std.mem.eql(u8, method, protocol.Method.get_usage)) {
            return self.handleGetUsage(alloc, id, params);
        } else if (std.mem.eql(u8, method, protocol.Method.destroy_execution)) {
            return self.handleDestroyExecution(alloc, id, params);
        } else if (std.mem.eql(u8, method, protocol.Method.heartbeat)) {
            return self.handleHeartbeat(alloc, id, params);
        } else if (std.mem.eql(u8, method, protocol.Method.stream_events)) {
            // StreamEvents is deferred to v1.1 — return success with empty events for now.
            return std.fmt.allocPrint(alloc, "{{\"{s}\":{d},\"{s}\":[]}}", .{ "id", id, "result" });
        } else {
            return self.errorResponse(alloc, id, protocol.ErrorCode.method_not_found, "Unknown method");
        }
    }

    fn handleCreateExecution(self: *Handler, alloc: std.mem.Allocator, id: u64, params: ?std.json.Value) ![]u8 {
        const p = params orelse return self.errorResponse(alloc, id, protocol.ErrorCode.invalid_params, "Missing params");
        if (p != .object) return self.errorResponse(alloc, id, protocol.ErrorCode.invalid_params, "Params must be object");

        const workspace_path = getStringParam(p, "workspace_path") orelse return self.errorResponse(alloc, id, protocol.ErrorCode.invalid_params, "Missing workspace_path");

        const correlation = types.CorrelationContext{
            .trace_id = getStringParam(p, "trace_id") orelse "",
            .run_id = getStringParam(p, "run_id") orelse "",
            .workspace_id = getStringParam(p, "workspace_id") orelse "",
            .stage_id = getStringParam(p, "stage_id") orelse "",
            .role_id = getStringParam(p, "role_id") orelse "",
            .skill_id = getStringParam(p, "skill_id") orelse "",
        };

        const session_ptr = self.alloc.create(session_mod.Session) catch {
            return self.errorResponse(alloc, id, protocol.ErrorCode.internal_error, "Failed to allocate session");
        };
        session_ptr.* = session_mod.Session.create(
            self.alloc,
            workspace_path,
            correlation,
            self.resource_limits,
            self.lease_timeout_ms,
        );

        self.store.put(session_ptr) catch {
            session_ptr.destroy();
            self.alloc.destroy(session_ptr);
            return self.errorResponse(alloc, id, protocol.ErrorCode.internal_error, "Failed to store session");
        };

        executor_metrics.incExecutorSessionsCreated();
        executor_metrics.setExecutorSessionsActive(@intCast(self.store.activeCount()));

        const hex = types.executionIdHex(session_ptr.execution_id);
        log.info("executor.create_execution execution_id={s} run_id={s}", .{ &hex, correlation.run_id });

        return std.fmt.allocPrint(alloc,
            \\{{"id":{d},"result":{{"execution_id":"{s}"}}}}
        , .{ id, &hex });
    }

    fn handleStartStage(self: *Handler, alloc: std.mem.Allocator, id: u64, params: ?std.json.Value) ![]u8 {
        const p = params orelse return self.errorResponse(alloc, id, protocol.ErrorCode.invalid_params, "Missing params");
        if (p != .object) return self.errorResponse(alloc, id, protocol.ErrorCode.invalid_params, "Params must be object");

        const exec_id_hex = getStringParam(p, "execution_id") orelse return self.errorResponse(alloc, id, protocol.ErrorCode.invalid_params, "Missing execution_id");
        const exec_id = parseExecutionId(exec_id_hex) orelse return self.errorResponse(alloc, id, protocol.ErrorCode.invalid_params, "Invalid execution_id");

        const session = self.store.get(exec_id) orelse return self.errorResponse(alloc, id, protocol.ErrorCode.execution_failed, "Session not found");

        if (session.isCancelled()) {
            return self.errorResponse(alloc, id, protocol.ErrorCode.execution_failed, "Session cancelled");
        }
        if (session.isLeaseExpired()) {
            executor_metrics.incExecutorLeaseExpired();
            return self.errorResponse(alloc, id, protocol.ErrorCode.lease_expired, "Lease expired");
        }

        // Placeholder — returns empty result. Real NullClaw invocation is
        // tracked in M12_003_EXECUTOR_NULLCLAW_INVOCATION. When wired:
        // runner.execute(session, agent_config, tools, message) returns
        // real content and the handler serializes it into the response.
        const stage_id = getStringParam(p, "stage_id") orelse "";
        const role_id = getStringParam(p, "role_id") orelse "";
        const hex = types.executionIdHex(exec_id);
        log.info("executor.start_stage execution_id={s} stage_id={s} role_id={s}", .{ &hex, stage_id, role_id });

        // Record a placeholder result. The real implementation in the sidecar
        // binary calls the NullClaw runner here.
        const result = types.ExecutionResult{
            .content = "",
            .token_count = 0,
            .wall_seconds = 0,
            .exit_ok = true,
            .failure = null,
        };
        session.recordStageResult(result);
        session.touchLease();

        return std.fmt.allocPrint(alloc,
            \\{{"id":{d},"result":{{"content":"{s}","token_count":{d},"wall_seconds":{d},"exit_ok":{s}}}}}
        , .{ id, result.content, result.token_count, result.wall_seconds, if (result.exit_ok) "true" else "false" });
    }

    fn handleCancelExecution(self: *Handler, alloc: std.mem.Allocator, id: u64, params: ?std.json.Value) ![]u8 {
        const p = params orelse return self.errorResponse(alloc, id, protocol.ErrorCode.invalid_params, "Missing params");
        if (p != .object) return self.errorResponse(alloc, id, protocol.ErrorCode.invalid_params, "Params must be object");

        const exec_id_hex = getStringParam(p, "execution_id") orelse return self.errorResponse(alloc, id, protocol.ErrorCode.invalid_params, "Missing execution_id");
        const exec_id = parseExecutionId(exec_id_hex) orelse return self.errorResponse(alloc, id, protocol.ErrorCode.invalid_params, "Invalid execution_id");

        const session = self.store.get(exec_id) orelse return self.errorResponse(alloc, id, protocol.ErrorCode.execution_failed, "Session not found");
        session.cancel();
        executor_metrics.incExecutorCancellations();

        return std.fmt.allocPrint(alloc, "{{\"{s}\":{d},\"{s}\":{s}}}", .{ "id", id, "result", "true" });
    }

    fn handleGetUsage(self: *Handler, alloc: std.mem.Allocator, id: u64, params: ?std.json.Value) ![]u8 {
        const p = params orelse return self.errorResponse(alloc, id, protocol.ErrorCode.invalid_params, "Missing params");
        if (p != .object) return self.errorResponse(alloc, id, protocol.ErrorCode.invalid_params, "Params must be object");

        const exec_id_hex = getStringParam(p, "execution_id") orelse return self.errorResponse(alloc, id, protocol.ErrorCode.invalid_params, "Missing execution_id");
        const exec_id = parseExecutionId(exec_id_hex) orelse return self.errorResponse(alloc, id, protocol.ErrorCode.invalid_params, "Invalid execution_id");

        const session = self.store.get(exec_id) orelse return self.errorResponse(alloc, id, protocol.ErrorCode.execution_failed, "Session not found");
        const usage = session.getUsage();

        return std.fmt.allocPrint(alloc,
            \\{{"id":{d},"result":{{"token_count":{d},"wall_seconds":{d},"exit_ok":{s}}}}}
        , .{ id, usage.token_count, usage.wall_seconds, if (usage.exit_ok) "true" else "false" });
    }

    fn handleDestroyExecution(self: *Handler, alloc: std.mem.Allocator, id: u64, params: ?std.json.Value) ![]u8 {
        const p = params orelse return self.errorResponse(alloc, id, protocol.ErrorCode.invalid_params, "Missing params");
        if (p != .object) return self.errorResponse(alloc, id, protocol.ErrorCode.invalid_params, "Params must be object");

        const exec_id_hex = getStringParam(p, "execution_id") orelse return self.errorResponse(alloc, id, protocol.ErrorCode.invalid_params, "Missing execution_id");
        const exec_id = parseExecutionId(exec_id_hex) orelse return self.errorResponse(alloc, id, protocol.ErrorCode.invalid_params, "Invalid execution_id");

        const session = self.store.remove(exec_id) orelse return self.errorResponse(alloc, id, protocol.ErrorCode.execution_failed, "Session not found");
        session.destroy();
        self.alloc.destroy(session);
        executor_metrics.setExecutorSessionsActive(@intCast(self.store.activeCount()));

        return std.fmt.allocPrint(alloc, "{{\"{s}\":{d},\"{s}\":{s}}}", .{ "id", id, "result", "true" });
    }

    fn handleHeartbeat(self: *Handler, alloc: std.mem.Allocator, id: u64, params: ?std.json.Value) ![]u8 {
        const p = params orelse return self.errorResponse(alloc, id, protocol.ErrorCode.invalid_params, "Missing params");
        if (p != .object) return self.errorResponse(alloc, id, protocol.ErrorCode.invalid_params, "Params must be object");

        const exec_id_hex = getStringParam(p, "execution_id") orelse return self.errorResponse(alloc, id, protocol.ErrorCode.invalid_params, "Missing execution_id");
        const exec_id = parseExecutionId(exec_id_hex) orelse return self.errorResponse(alloc, id, protocol.ErrorCode.invalid_params, "Invalid execution_id");

        const session = self.store.get(exec_id) orelse return self.errorResponse(alloc, id, protocol.ErrorCode.execution_failed, "Session not found");
        session.touchLease();

        return std.fmt.allocPrint(alloc, "{{\"{s}\":{d},\"{s}\":{s}}}", .{ "id", id, "result", "true" });
    }

    fn errorResponse(self: *Handler, alloc: std.mem.Allocator, id: u64, code: i32, message: []const u8) ![]u8 {
        _ = self;
        executor_metrics.incExecutorFailures();

        return std.fmt.allocPrint(alloc,
            \\{{"id":{d},"error":{{"code":{d},"message":"{s}"}}}}
        , .{ id, code, message });
    }
};

fn getStringParam(obj: std.json.Value, key: []const u8) ?[]const u8 {
    if (obj != .object) return null;
    const val = obj.object.get(key) orelse return null;
    return switch (val) {
        .string => |s| s,
        else => null,
    };
}

fn parseExecutionId(hex: []const u8) ?types.ExecutionId {
    if (hex.len != 32) return null;
    var id: types.ExecutionId = undefined;
    for (0..16) |i| {
        id[i] = std.fmt.parseInt(u8, hex[i * 2 .. i * 2 + 2], 16) catch return null;
    }
    return id;
}

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
    var store = session_mod.SessionStore.init(alloc);
    defer store.deinit();
    var handler = Handler.init(alloc, &store, 30_000, .{});

    const resp_json = try handler.handleFrame(alloc, "{{bad json");
    defer alloc.free(resp_json);

    var resp = try protocol.parseResponse(alloc, resp_json);
    defer resp.deinit();
    try std.testing.expect(resp.rpc_error != null);
    try std.testing.expectEqual(@as(i32, protocol.ErrorCode.parse_error), resp.rpc_error.?.code);
}

test "handler dispatch returns method_not_found for unknown method" {
    const alloc = std.testing.allocator;
    var store = session_mod.SessionStore.init(alloc);
    defer store.deinit();
    var handler = Handler.init(alloc, &store, 30_000, .{});

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
    var store = session_mod.SessionStore.init(alloc);
    defer store.deinit();
    var handler = Handler.init(alloc, &store, 30_000, .{});

    var params = std.json.Value{ .object = std.json.ObjectMap.init(alloc) };
    defer params.object.deinit();
    try params.object.put("workspace_path", .{ .string = "/tmp/test" });
    try params.object.put("run_id", .{ .string = "r" });

    const req = try protocol.serializeRequest(alloc, 1, protocol.Method.create_execution, params);
    defer alloc.free(req);
    const resp_json = try handler.handleFrame(alloc, req);
    defer alloc.free(resp_json);

    var resp = try protocol.parseResponse(alloc, resp_json);
    defer resp.deinit();
    try std.testing.expect(resp.rpc_error == null);
    try std.testing.expect(resp.result != null);

    // Verify session was stored.
    try std.testing.expectEqual(@as(usize, 1), store.activeCount());

    // Clean up session.
    store.deinit();
    store = session_mod.SessionStore.init(alloc);
}

test "handler CreateExecution without params returns invalid_params" {
    const alloc = std.testing.allocator;
    var store = session_mod.SessionStore.init(alloc);
    defer store.deinit();
    var handler = Handler.init(alloc, &store, 30_000, .{});

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
    var store = session_mod.SessionStore.init(alloc);
    defer store.deinit();
    var handler = Handler.init(alloc, &store, 30_000, .{});

    const req = try protocol.serializeRequest(alloc, 1, protocol.Method.stream_events, null);
    defer alloc.free(req);
    const resp_json = try handler.handleFrame(alloc, req);
    defer alloc.free(resp_json);

    var resp = try protocol.parseResponse(alloc, resp_json);
    defer resp.deinit();
    try std.testing.expect(resp.rpc_error == null);
}

// ── T2: Edge case — unicode in correlation fields ────────────────────
test "handler CreateExecution with unicode correlation fields" {
    const alloc = std.testing.allocator;
    var store = session_mod.SessionStore.init(alloc);
    defer store.deinit();
    var handler = Handler.init(alloc, &store, 30_000, .{});

    var params = std.json.Value{ .object = std.json.ObjectMap.init(alloc) };
    defer params.object.deinit();
    try params.object.put("workspace_path", .{ .string = "/tmp/日本語-workspace" });
    try params.object.put("run_id", .{ .string = "run-café-☕" });
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

// ── T3: Error path — StartStage with non-existent session ────────────
test "handler StartStage with unknown execution_id returns execution_failed" {
    const alloc = std.testing.allocator;
    var store = session_mod.SessionStore.init(alloc);
    defer store.deinit();
    var handler = Handler.init(alloc, &store, 30_000, .{});

    const fake_id = types.executionIdHex(types.generateExecutionId());
    var params = std.json.Value{ .object = std.json.ObjectMap.init(alloc) };
    defer params.object.deinit();
    try params.object.put("execution_id", .{ .string = &fake_id });
    try params.object.put("stage_id", .{ .string = "s" });

    const req = try protocol.serializeRequest(alloc, 1, protocol.Method.start_stage, params);
    defer alloc.free(req);
    const resp_json = try handler.handleFrame(alloc, req);
    defer alloc.free(resp_json);

    var resp = try protocol.parseResponse(alloc, resp_json);
    defer resp.deinit();
    try std.testing.expect(resp.rpc_error != null);
    try std.testing.expectEqual(@as(i32, protocol.ErrorCode.execution_failed), resp.rpc_error.?.code);
}

// ── T3: Error path — DestroyExecution unknown session ────────────────
test "handler DestroyExecution with unknown execution_id returns error" {
    const alloc = std.testing.allocator;
    var store = session_mod.SessionStore.init(alloc);
    defer store.deinit();
    var handler = Handler.init(alloc, &store, 30_000, .{});

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

// ── T4: Output — verify errorResponse JSON structure ─────────────────
test "handler errorResponse output is valid parseable JSON-RPC" {
    const alloc = std.testing.allocator;
    var store = session_mod.SessionStore.init(alloc);
    defer store.deinit();
    var handler = Handler.init(alloc, &store, 30_000, .{});

    // Trigger a known error path.
    const resp_json = try handler.handleFrame(alloc, "{}");
    defer alloc.free(resp_json);

    // Must be valid JSON.
    var parsed = try std.json.parseFromSlice(std.json.Value, alloc, resp_json, .{});
    defer parsed.deinit();

    // Must have "id" and "error" keys.
    try std.testing.expect(parsed.value.object.get("id") != null);
    try std.testing.expect(parsed.value.object.get("error") != null);

    // Error must have "code" and "message".
    const err_obj = parsed.value.object.get("error").?;
    try std.testing.expect(err_obj.object.get("code") != null);
    try std.testing.expect(err_obj.object.get("message") != null);
}

// ── T8: Security — workspace_path with path traversal ────────────────
// Note: in v1, the handler stores workspace_path as-is and Landlock
// enforces filesystem policy. This test documents that the handler does
// NOT do path validation — Landlock is the security boundary. If we
// later add validation, this test should be updated to expect rejection.
test "handler CreateExecution accepts path traversal without crashing" {
    const alloc = std.testing.allocator;
    var store = session_mod.SessionStore.init(alloc);
    defer store.deinit();
    var handler = Handler.init(alloc, &store, 30_000, .{});

    var params = std.json.Value{ .object = std.json.ObjectMap.init(alloc) };
    defer params.object.deinit();
    try params.object.put("workspace_path", .{ .string = "/tmp/../../../etc/passwd" });
    try params.object.put("run_id", .{ .string = "r" });

    const req = try protocol.serializeRequest(alloc, 1, protocol.Method.create_execution, params);
    defer alloc.free(req);
    const resp_json = try handler.handleFrame(alloc, req);
    defer alloc.free(resp_json);

    // Handler creates the session (Landlock enforces policy at execution time).
    var resp = try protocol.parseResponse(alloc, resp_json);
    defer resp.deinit();
    try std.testing.expect(resp.rpc_error == null);
}

// ── T11: Perf/leak — repeated handler calls don't leak ───────────────
test "handler 100 create+destroy cycles no leak" {
    const alloc = std.testing.allocator;
    var store = session_mod.SessionStore.init(alloc);
    defer store.deinit();
    var handler = Handler.init(alloc, &store, 30_000, .{});

    for (0..100) |i| {
        // Create.
        var params = std.json.Value{ .object = std.json.ObjectMap.init(alloc) };
        try params.object.put("workspace_path", .{ .string = "/tmp/test" });
        try params.object.put("run_id", .{ .string = "r" });

        const req = try protocol.serializeRequest(alloc, i + 1, protocol.Method.create_execution, params);
        const resp_json = try handler.handleFrame(alloc, req);
        params.object.deinit();
        alloc.free(req);

        // Extract execution_id from response.
        var resp = try protocol.parseResponse(alloc, resp_json);
        const exec_id = resp.result.?.object.get("execution_id").?.string;
        const exec_id_copy = try alloc.dupe(u8, exec_id);
        resp.deinit();
        alloc.free(resp_json);

        // Destroy.
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
