//! High-level executor client for the worker side.
//!
//! Wraps the transport Client with typed methods for each executor API RPC.
//! Translates JSON responses into AgentResult and FailureClass types.

const std = @import("std");
const transport = @import("transport.zig");
const protocol = @import("protocol.zig");
const types = @import("types.zig");
const executor_metrics = @import("executor_metrics.zig");

const log = std.log.scoped(.executor_client);

pub const ClientError = error{
    ConnectionFailed,
    TransportLoss,
    ExecutionFailed,
    InvalidResponse,
    SessionNotFound,
    LeaseExpired,
    PolicyDenied,
    TimeoutKilled,
    OomKilled,
    ResourceKilled,
    LandlockDenied,
};

/// Maps an RPC error code to a FailureClass.
fn classifyError(code: i32) types.FailureClass {
    return switch (code) {
        protocol.ErrorCode.timeout_killed => .timeout_kill,
        protocol.ErrorCode.oom_killed => .oom_kill,
        protocol.ErrorCode.policy_denied => .policy_deny,
        protocol.ErrorCode.lease_expired => .lease_expired,
        protocol.ErrorCode.landlock_denied => .landlock_deny,
        protocol.ErrorCode.resource_killed => .resource_kill,
        else => .executor_crash,
    };
}

pub const ExecutorClient = struct {
    transport_client: transport.Client,
    alloc: std.mem.Allocator,
    next_id: u64 = 1,

    pub fn init(alloc: std.mem.Allocator, socket_path: []const u8) ExecutorClient {
        return .{
            .transport_client = transport.Client.init(alloc, socket_path),
            .alloc = alloc,
        };
    }

    pub fn connect(self: *ExecutorClient) ClientError!void {
        self.transport_client.connect() catch {
            log.err("executor_client.connect_failed error_code=UZ-EXEC-009 socket={s}", .{self.transport_client.socket_path});
            executor_metrics.incExecutorFailures();
            return ClientError.ConnectionFailed;
        };
        log.info("executor_client.connected socket={s}", .{self.transport_client.socket_path});
    }

    pub fn close(self: *ExecutorClient) void {
        self.transport_client.close();
    }

    fn nextId(self: *ExecutorClient) u64 {
        const id = self.next_id;
        self.next_id += 1;
        return id;
    }

    pub fn createExecution(
        self: *ExecutorClient,
        workspace_path: []const u8,
        correlation: types.CorrelationContext,
    ) ![]const u8 {
        var params = std.json.Value{ .object = std.json.ObjectMap.init(self.alloc) };
        defer params.object.deinit();

        try params.object.put("workspace_path", .{ .string = workspace_path });
        try params.object.put("trace_id", .{ .string = correlation.trace_id });
        try params.object.put("run_id", .{ .string = correlation.run_id });
        try params.object.put("workspace_id", .{ .string = correlation.workspace_id });
        try params.object.put("stage_id", .{ .string = correlation.stage_id });
        try params.object.put("role_id", .{ .string = correlation.role_id });
        try params.object.put("skill_id", .{ .string = correlation.skill_id });

        var resp = self.transport_client.sendRequest(self.nextId(), protocol.Method.create_execution, params) catch {
            log.err("executor_client.transport_loss error_code=UZ-EXEC-006 method=CreateExecution", .{});
            executor_metrics.incExecutorFailures();
            return ClientError.TransportLoss;
        };
        defer resp.deinit();

        if (resp.rpc_error) |err| {
            log.err("executor_client.create_execution_failed code={d} msg={s}", .{ err.code, err.message });
            return ClientError.ExecutionFailed;
        }

        const result = resp.result orelse return ClientError.InvalidResponse;
        if (result != .object) return ClientError.InvalidResponse;
        const exec_id_val = result.object.get("execution_id") orelse return ClientError.InvalidResponse;
        return switch (exec_id_val) {
            .string => |s| try self.alloc.dupe(u8, s),
            else => ClientError.InvalidResponse,
        };
    }

    /// Result from a StartStage RPC call.
    ///
    /// Ownership: `.content` is allocated via the ExecutorClient's allocator.
    /// The caller must free it when done (currently always "" from the
    /// placeholder handler — real content arrives when NullClaw runner
    /// is wired in the sidecar binary).
    pub const StageResult = struct {
        content: []const u8,
        token_count: u64,
        wall_seconds: u64,
        exit_ok: bool,
        failure: ?types.FailureClass,
    };

    pub fn startStage(
        self: *ExecutorClient,
        execution_id: []const u8,
        stage_id: []const u8,
        role_id: []const u8,
        skill_id: []const u8,
    ) !StageResult {
        var params = std.json.Value{ .object = std.json.ObjectMap.init(self.alloc) };
        defer params.object.deinit();

        try params.object.put("execution_id", .{ .string = execution_id });
        try params.object.put("stage_id", .{ .string = stage_id });
        try params.object.put("role_id", .{ .string = role_id });
        try params.object.put("skill_id", .{ .string = skill_id });

        var resp = self.transport_client.sendRequest(self.nextId(), protocol.Method.start_stage, params) catch {
            log.err("executor_client.transport_loss error_code=UZ-EXEC-006 method=StartStage execution_id={s}", .{execution_id});
            executor_metrics.incExecutorFailures();
            return ClientError.TransportLoss;
        };
        defer resp.deinit();

        if (resp.rpc_error) |err| {
            log.err("executor_client.start_stage_failed code={d} msg={s}", .{ err.code, err.message });
            return .{
                .content = "",
                .token_count = 0,
                .wall_seconds = 0,
                .exit_ok = false,
                .failure = classifyError(err.code),
            };
        }

        const result = resp.result orelse return ClientError.InvalidResponse;
        if (result != .object) return ClientError.InvalidResponse;

        return .{
            .content = try self.alloc.dupe(u8, getStringField(result, "content") orelse ""),
            .token_count = getIntField(result, "token_count"),
            .wall_seconds = getIntField(result, "wall_seconds"),
            .exit_ok = getBoolField(result, "exit_ok"),
            .failure = null,
        };
    }

    pub fn cancelExecution(self: *ExecutorClient, execution_id: []const u8) !void {
        var params = std.json.Value{ .object = std.json.ObjectMap.init(self.alloc) };
        defer params.object.deinit();
        try params.object.put("execution_id", .{ .string = execution_id });

        var resp = self.transport_client.sendRequest(self.nextId(), protocol.Method.cancel_execution, params) catch {
            log.err("executor_client.transport_loss error_code=UZ-EXEC-006 method=CancelExecution execution_id={s}", .{execution_id});
            executor_metrics.incExecutorFailures();
            return ClientError.TransportLoss;
        };
        defer resp.deinit();
    }

    pub fn getUsage(self: *ExecutorClient, execution_id: []const u8) !StageResult {
        var params = std.json.Value{ .object = std.json.ObjectMap.init(self.alloc) };
        defer params.object.deinit();
        try params.object.put("execution_id", .{ .string = execution_id });

        var resp = self.transport_client.sendRequest(self.nextId(), protocol.Method.get_usage, params) catch {
            log.err("executor_client.transport_loss error_code=UZ-EXEC-006 method=GetUsage execution_id={s}", .{execution_id});
            executor_metrics.incExecutorFailures();
            return ClientError.TransportLoss;
        };
        defer resp.deinit();

        if (resp.rpc_error != null) return ClientError.ExecutionFailed;
        const result = resp.result orelse return ClientError.InvalidResponse;
        if (result != .object) return ClientError.InvalidResponse;

        return .{
            .content = "",
            .token_count = getIntField(result, "token_count"),
            .wall_seconds = getIntField(result, "wall_seconds"),
            .exit_ok = getBoolField(result, "exit_ok"),
            .failure = null,
        };
    }

    pub fn destroyExecution(self: *ExecutorClient, execution_id: []const u8) !void {
        var params = std.json.Value{ .object = std.json.ObjectMap.init(self.alloc) };
        defer params.object.deinit();
        try params.object.put("execution_id", .{ .string = execution_id });

        var resp = self.transport_client.sendRequest(self.nextId(), protocol.Method.destroy_execution, params) catch {
            log.err("executor_client.transport_loss error_code=UZ-EXEC-006 method=DestroyExecution execution_id={s}", .{execution_id});
            executor_metrics.incExecutorFailures();
            return ClientError.TransportLoss;
        };
        defer resp.deinit();
    }

    pub fn heartbeat(self: *ExecutorClient, execution_id: []const u8) !void {
        var params = std.json.Value{ .object = std.json.ObjectMap.init(self.alloc) };
        defer params.object.deinit();
        try params.object.put("execution_id", .{ .string = execution_id });

        var resp = self.transport_client.sendRequest(self.nextId(), protocol.Method.heartbeat, params) catch {
            log.err("executor_client.transport_loss error_code=UZ-EXEC-006 method=Heartbeat execution_id={s}", .{execution_id});
            executor_metrics.incExecutorFailures();
            return ClientError.TransportLoss;
        };
        defer resp.deinit();
    }
};

fn getStringField(obj: std.json.Value, key: []const u8) ?[]const u8 {
    if (obj != .object) return null;
    const val = obj.object.get(key) orelse return null;
    return switch (val) {
        .string => |s| s,
        else => null,
    };
}

fn getIntField(obj: std.json.Value, key: []const u8) u64 {
    if (obj != .object) return 0;
    const val = obj.object.get(key) orelse return 0;
    return switch (val) {
        .integer => |i| @intCast(@max(0, i)),
        else => 0,
    };
}

fn getBoolField(obj: std.json.Value, key: []const u8) bool {
    if (obj != .object) return false;
    const val = obj.object.get(key) orelse return false;
    return switch (val) {
        .bool => |b| b,
        else => false,
    };
}

// ── Tests ────────────────────────────────────────────────────────────────

test "classifyError maps all known error codes" {
    try std.testing.expectEqual(types.FailureClass.timeout_kill, classifyError(protocol.ErrorCode.timeout_killed));
    try std.testing.expectEqual(types.FailureClass.oom_kill, classifyError(protocol.ErrorCode.oom_killed));
    try std.testing.expectEqual(types.FailureClass.policy_deny, classifyError(protocol.ErrorCode.policy_denied));
    try std.testing.expectEqual(types.FailureClass.lease_expired, classifyError(protocol.ErrorCode.lease_expired));
    try std.testing.expectEqual(types.FailureClass.landlock_deny, classifyError(protocol.ErrorCode.landlock_denied));
    try std.testing.expectEqual(types.FailureClass.resource_kill, classifyError(protocol.ErrorCode.resource_killed));
}

test "classifyError falls back to executor_crash for unknown codes" {
    try std.testing.expectEqual(types.FailureClass.executor_crash, classifyError(0));
    try std.testing.expectEqual(types.FailureClass.executor_crash, classifyError(-999));
    try std.testing.expectEqual(types.FailureClass.executor_crash, classifyError(42));
}

test "getStringField returns null for missing key" {
    const alloc = std.testing.allocator;
    var obj = std.json.Value{ .object = std.json.ObjectMap.init(alloc) };
    defer obj.object.deinit();
    try std.testing.expect(getStringField(obj, "missing") == null);
}

test "getIntField returns 0 for missing key" {
    const alloc = std.testing.allocator;
    var obj = std.json.Value{ .object = std.json.ObjectMap.init(alloc) };
    defer obj.object.deinit();
    try std.testing.expectEqual(@as(u64, 0), getIntField(obj, "missing"));
}

test "getBoolField returns false for missing key" {
    const alloc = std.testing.allocator;
    var obj = std.json.Value{ .object = std.json.ObjectMap.init(alloc) };
    defer obj.object.deinit();
    try std.testing.expectEqual(false, getBoolField(obj, "missing"));
}

test "getStringField returns null for non-string value" {
    const alloc = std.testing.allocator;
    var obj = std.json.Value{ .object = std.json.ObjectMap.init(alloc) };
    defer obj.object.deinit();
    try obj.object.put("key", .{ .integer = 42 });
    try std.testing.expect(getStringField(obj, "key") == null);
}

test "getIntField returns value for integer field" {
    const alloc = std.testing.allocator;
    var obj = std.json.Value{ .object = std.json.ObjectMap.init(alloc) };
    defer obj.object.deinit();
    try obj.object.put("count", .{ .integer = 7 });
    try std.testing.expectEqual(@as(u64, 7), getIntField(obj, "count"));
}

test "getBoolField returns value for bool field" {
    const alloc = std.testing.allocator;
    var obj = std.json.Value{ .object = std.json.ObjectMap.init(alloc) };
    defer obj.object.deinit();
    try obj.object.put("ok", .{ .bool = true });
    try std.testing.expectEqual(true, getBoolField(obj, "ok"));
}
