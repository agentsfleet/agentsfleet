//! RPC dispatch handler for the executor sidecar.
//!
//! Maps JSON-RPC method names to session lifecycle operations.
//! This is the core of the executor API (§2.0).

const std = @import("std");
const json = @import("json_helpers.zig");
const protocol = @import("protocol.zig");
const types = @import("types.zig");
const session_mod = @import("session.zig");
const executor_metrics = @import("executor_metrics.zig");
const runner = @import("runner.zig");
const network = @import("network.zig");

const log = std.log.scoped(.executor_handler);

pub const Handler = struct {
    store: *session_mod.SessionStore,
    lease_timeout_ms: u64,
    resource_limits: types.ResourceLimits,
    network_policy: network.NetworkPolicy,
    alloc: std.mem.Allocator,

    pub fn init(
        alloc: std.mem.Allocator,
        store: *session_mod.SessionStore,
        lease_timeout_ms: u64,
        resource_limits: types.ResourceLimits,
        net_policy: network.NetworkPolicy,
    ) Handler {
        return .{
            .store = store,
            .lease_timeout_ms = lease_timeout_ms,
            .resource_limits = resource_limits,
            .network_policy = net_policy,
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

        const workspace_path = json.getStr(p, "workspace_path") orelse return self.errorResponse(alloc, id, protocol.ErrorCode.invalid_params, "Missing workspace_path");

        const correlation = types.CorrelationContext{
            .trace_id = json.getStr(p, "trace_id") orelse "",
            .run_id = json.getStr(p, "run_id") orelse "",
            .workspace_id = json.getStr(p, "workspace_id") orelse "",
            .stage_id = json.getStr(p, "stage_id") orelse "",
            .role_id = json.getStr(p, "role_id") orelse "",
            .skill_id = json.getStr(p, "skill_id") orelse "",
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

        const exec_id_hex = json.getStr(p, "execution_id") orelse return self.errorResponse(alloc, id, protocol.ErrorCode.invalid_params, "Missing execution_id");
        const exec_id = parseExecutionId(exec_id_hex) orelse return self.errorResponse(alloc, id, protocol.ErrorCode.invalid_params, "Invalid execution_id");

        const session = self.store.get(exec_id) orelse return self.errorResponse(alloc, id, protocol.ErrorCode.execution_failed, "Session not found");

        if (session.isCancelled()) {
            return self.errorResponse(alloc, id, protocol.ErrorCode.execution_failed, "Session cancelled");
        }
        if (session.isLeaseExpired()) {
            executor_metrics.incExecutorLeaseExpired();
            return self.errorResponse(alloc, id, protocol.ErrorCode.lease_expired, "Lease expired");
        }

        const stage_id = json.getStr(p, "stage_id") orelse "";
        const role_id = json.getStr(p, "role_id") orelse "";
        const hex = types.executionIdHex(exec_id);

        // Extract M12_003 payload fields.
        const agent_config = getObjectParam(p, "agent_config");
        const tools_spec = getArrayParam(p, "tools");
        const message = json.getStr(p, "message");
        const context = getObjectParam(p, "context");

        const model_name = if (agent_config) |ac| json.getStr(ac, "model") orelse "default" else "default";
        log.info("executor.runner.start execution_id={s} stage_id={s} role_id={s} model={s} network_policy={s}", .{ &hex, stage_id, role_id, model_name, @tagName(self.network_policy) });

        // Invoke NullClaw runner — this blocks until agent execution completes.
        const result = runner.execute(
            alloc,
            session.workspace_path,
            agent_config,
            tools_spec,
            message,
            context,
        );
        session.recordStageResult(result);
        session.touchLease();

        if (!result.exit_ok) {
            const failure_label = if (result.failure) |f| f.label() else "unknown";
            log.err("executor.runner.stage_failed execution_id={s} failure={s}", .{ &hex, failure_label });
            const err_code = failureToRpcError(result.failure);
            return self.errorResponse(alloc, id, err_code, failure_label);
        }

        // Serialize response with JSON-escaped content.
        const escaped_content = jsonEscapeAlloc(alloc, result.content) catch {
            return self.errorResponse(alloc, id, protocol.ErrorCode.internal_error, "Content escape failed");
        };
        defer alloc.free(escaped_content);

        return std.fmt.allocPrint(alloc,
            \\{{"id":{d},"result":{{"content":"{s}","token_count":{d},"wall_seconds":{d},"exit_ok":true,"memory_peak_bytes":{d},"cpu_throttled_ms":{d}}}}}
        , .{ id, escaped_content, result.token_count, result.wall_seconds, result.memory_peak_bytes, result.cpu_throttled_ms });
    }

    fn handleCancelExecution(self: *Handler, alloc: std.mem.Allocator, id: u64, params: ?std.json.Value) ![]u8 {
        const p = params orelse return self.errorResponse(alloc, id, protocol.ErrorCode.invalid_params, "Missing params");
        if (p != .object) return self.errorResponse(alloc, id, protocol.ErrorCode.invalid_params, "Params must be object");

        const exec_id_hex = json.getStr(p, "execution_id") orelse return self.errorResponse(alloc, id, protocol.ErrorCode.invalid_params, "Missing execution_id");
        const exec_id = parseExecutionId(exec_id_hex) orelse return self.errorResponse(alloc, id, protocol.ErrorCode.invalid_params, "Invalid execution_id");

        const session = self.store.get(exec_id) orelse return self.errorResponse(alloc, id, protocol.ErrorCode.execution_failed, "Session not found");
        session.cancel();
        executor_metrics.incExecutorCancellations();

        return std.fmt.allocPrint(alloc, "{{\"{s}\":{d},\"{s}\":{s}}}", .{ "id", id, "result", "true" });
    }

    fn handleGetUsage(self: *Handler, alloc: std.mem.Allocator, id: u64, params: ?std.json.Value) ![]u8 {
        const p = params orelse return self.errorResponse(alloc, id, protocol.ErrorCode.invalid_params, "Missing params");
        if (p != .object) return self.errorResponse(alloc, id, protocol.ErrorCode.invalid_params, "Params must be object");

        const exec_id_hex = json.getStr(p, "execution_id") orelse return self.errorResponse(alloc, id, protocol.ErrorCode.invalid_params, "Missing execution_id");
        const exec_id = parseExecutionId(exec_id_hex) orelse return self.errorResponse(alloc, id, protocol.ErrorCode.invalid_params, "Invalid execution_id");

        const session = self.store.get(exec_id) orelse return self.errorResponse(alloc, id, protocol.ErrorCode.execution_failed, "Session not found");
        const usage = session.getUsage();
        const res_ctx = session.getResourceContext();

        return std.fmt.allocPrint(alloc,
            \\{{"id":{d},"result":{{"token_count":{d},"wall_seconds":{d},"exit_ok":{s},"memory_peak_bytes":{d},"cpu_throttled_ms":{d},"memory_limit_bytes":{d}}}}}
        , .{ id, usage.token_count, usage.wall_seconds, if (usage.exit_ok) "true" else "false", usage.memory_peak_bytes, usage.cpu_throttled_ms, res_ctx.memory_limit_bytes });
    }

    fn handleDestroyExecution(self: *Handler, alloc: std.mem.Allocator, id: u64, params: ?std.json.Value) ![]u8 {
        const p = params orelse return self.errorResponse(alloc, id, protocol.ErrorCode.invalid_params, "Missing params");
        if (p != .object) return self.errorResponse(alloc, id, protocol.ErrorCode.invalid_params, "Params must be object");

        const exec_id_hex = json.getStr(p, "execution_id") orelse return self.errorResponse(alloc, id, protocol.ErrorCode.invalid_params, "Missing execution_id");
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

        const exec_id_hex = json.getStr(p, "execution_id") orelse return self.errorResponse(alloc, id, protocol.ErrorCode.invalid_params, "Missing execution_id");
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

fn getObjectParam(obj: std.json.Value, key: []const u8) ?std.json.Value {
    if (obj != .object) return null;
    const val = obj.object.get(key) orelse return null;
    return if (val == .object) val else null;
}

fn getArrayParam(obj: std.json.Value, key: []const u8) ?std.json.Value {
    if (obj != .object) return null;
    const val = obj.object.get(key) orelse return null;
    return if (val == .array) val else null;
}

/// Map a FailureClass to a JSON-RPC error code.
fn failureToRpcError(failure: ?types.FailureClass) i32 {
    const f = failure orelse return protocol.ErrorCode.execution_failed;
    return switch (f) {
        .timeout_kill => protocol.ErrorCode.timeout_killed,
        .oom_kill => protocol.ErrorCode.oom_killed,
        .policy_deny => protocol.ErrorCode.policy_denied,
        .lease_expired => protocol.ErrorCode.lease_expired,
        .landlock_deny => protocol.ErrorCode.landlock_denied,
        .resource_kill => protocol.ErrorCode.resource_killed,
        else => protocol.ErrorCode.execution_failed,
    };
}

/// Escape a string for embedding inside a JSON string literal.
/// Caller owns returned slice.
fn jsonEscapeAlloc(alloc: std.mem.Allocator, input: []const u8) ![]u8 {
    var buf: std.ArrayList(u8) = .{};
    errdefer buf.deinit(alloc);

    for (input) |c| {
        switch (c) {
            '"' => try buf.appendSlice(alloc, "\\\""),
            '\\' => try buf.appendSlice(alloc, "\\\\"),
            '\n' => try buf.appendSlice(alloc, "\\n"),
            '\r' => try buf.appendSlice(alloc, "\\r"),
            '\t' => try buf.appendSlice(alloc, "\\t"),
            else => {
                if (c < 0x20) {
                    var tmp: [6]u8 = undefined;
                    const written = std.fmt.bufPrint(&tmp, "\\u{x:0>4}", .{c}) catch continue;
                    try buf.appendSlice(alloc, written);
                } else {
                    try buf.append(alloc, c);
                }
            },
        }
    }
    return buf.toOwnedSlice(alloc);
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
    var store = session_mod.SessionStore.init(alloc);
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
    var store = session_mod.SessionStore.init(alloc);
    defer store.deinit();
    var handler = Handler.init(alloc, &store, 30_000, .{}, .deny_all);

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
    var store = session_mod.SessionStore.init(alloc);
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

// ── T2: Edge case — unicode in correlation fields ────────────────────
test "handler CreateExecution with unicode correlation fields" {
    const alloc = std.testing.allocator;
    var store = session_mod.SessionStore.init(alloc);
    defer store.deinit();
    var handler = Handler.init(alloc, &store, 30_000, .{}, .deny_all);

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
    var handler = Handler.init(alloc, &store, 30_000, .{}, .deny_all);

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

// ── T4: Output — verify errorResponse JSON structure ─────────────────
test "handler errorResponse output is valid parseable JSON-RPC" {
    const alloc = std.testing.allocator;
    var store = session_mod.SessionStore.init(alloc);
    defer store.deinit();
    var handler = Handler.init(alloc, &store, 30_000, .{}, .deny_all);

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
    var handler = Handler.init(alloc, &store, 30_000, .{}, .deny_all);

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
    var handler = Handler.init(alloc, &store, 30_000, .{}, .deny_all);

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

// ── jsonEscapeAlloc tests (security-critical) ────────────────────────

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

// ── getObjectParam tests ─────────────────────────────────────────────

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

// ── getArrayParam tests ──────────────────────────────────────────────

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

// ── failureToRpcError tests ──────────────────────────────────────────

test "failureToRpcError maps all FailureClass variants" {
    // Explicitly mapped variants.
    try std.testing.expectEqual(@as(i32, protocol.ErrorCode.timeout_killed), failureToRpcError(.timeout_kill));
    try std.testing.expectEqual(@as(i32, protocol.ErrorCode.oom_killed), failureToRpcError(.oom_kill));
    try std.testing.expectEqual(@as(i32, protocol.ErrorCode.policy_denied), failureToRpcError(.policy_deny));
    try std.testing.expectEqual(@as(i32, protocol.ErrorCode.lease_expired), failureToRpcError(.lease_expired));
    try std.testing.expectEqual(@as(i32, protocol.ErrorCode.landlock_denied), failureToRpcError(.landlock_deny));
    try std.testing.expectEqual(@as(i32, protocol.ErrorCode.resource_killed), failureToRpcError(.resource_kill));
    // Variants that fall through to execution_failed.
    try std.testing.expectEqual(@as(i32, protocol.ErrorCode.execution_failed), failureToRpcError(.startup_posture));
    try std.testing.expectEqual(@as(i32, protocol.ErrorCode.execution_failed), failureToRpcError(.executor_crash));
    try std.testing.expectEqual(@as(i32, protocol.ErrorCode.execution_failed), failureToRpcError(.transport_loss));
}

test "failureToRpcError maps null to execution_failed" {
    try std.testing.expectEqual(@as(i32, protocol.ErrorCode.execution_failed), failureToRpcError(null));
}

// ── handleGetUsage test ──────────────────────────────────────────────

test "handler GetUsage returns zero for fresh session" {
    const alloc = std.testing.allocator;
    var store = session_mod.SessionStore.init(alloc);
    defer store.deinit();
    var handler = Handler.init(alloc, &store, 30_000, .{}, .deny_all);

    // Create a session via handler.
    var cparams = std.json.Value{ .object = std.json.ObjectMap.init(alloc) };
    try cparams.object.put("workspace_path", .{ .string = "/tmp/test" });
    try cparams.object.put("run_id", .{ .string = "r" });

    const creq = try protocol.serializeRequest(alloc, 1, protocol.Method.create_execution, cparams);
    const cresp_json = try handler.handleFrame(alloc, creq);
    cparams.object.deinit();
    alloc.free(creq);

    // Extract execution_id.
    var cresp = try protocol.parseResponse(alloc, cresp_json);
    const exec_id = cresp.result.?.object.get("execution_id").?.string;
    const exec_id_copy = try alloc.dupe(u8, exec_id);
    cresp.deinit();
    alloc.free(cresp_json);
    defer alloc.free(exec_id_copy);

    // GetUsage on fresh session.
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

    // Verify zero token_count and wall_seconds.
    const result_obj = gresp.result.?.object;
    const token_count = result_obj.get("token_count").?.integer;
    const wall_seconds = result_obj.get("wall_seconds").?.integer;
    try std.testing.expectEqual(@as(i64, 0), token_count);
    try std.testing.expectEqual(@as(i64, 0), wall_seconds);
}

// ── handleHeartbeat test ─────────────────────────────────────────────

test "handler Heartbeat refreshes lease" {
    const alloc = std.testing.allocator;
    var store = session_mod.SessionStore.init(alloc);
    defer store.deinit();
    var handler = Handler.init(alloc, &store, 30_000, .{}, .deny_all);

    // Create a session via handler.
    var cparams = std.json.Value{ .object = std.json.ObjectMap.init(alloc) };
    try cparams.object.put("workspace_path", .{ .string = "/tmp/test" });
    try cparams.object.put("run_id", .{ .string = "r" });

    const creq = try protocol.serializeRequest(alloc, 1, protocol.Method.create_execution, cparams);
    const cresp_json = try handler.handleFrame(alloc, creq);
    cparams.object.deinit();
    alloc.free(creq);

    // Extract execution_id.
    var cresp = try protocol.parseResponse(alloc, cresp_json);
    const exec_id = cresp.result.?.object.get("execution_id").?.string;
    const exec_id_copy = try alloc.dupe(u8, exec_id);
    cresp.deinit();
    alloc.free(cresp_json);
    defer alloc.free(exec_id_copy);

    // Send Heartbeat.
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
