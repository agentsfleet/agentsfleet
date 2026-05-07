//! High-level executor client for the worker side.
//!
//! Wraps the transport Client with typed methods for each executor API RPC.
//! Translates JSON responses into AgentResult and FailureClass types.

const std = @import("std");
const logging = @import("log");
const json = @import("json_helpers.zig");
const transport = @import("transport.zig");
const protocol = @import("protocol.zig");
const types = @import("types.zig");
const context_budget = @import("context_budget.zig");
const executor_metrics = @import("executor_metrics.zig");
const client_errors = @import("client_errors.zig");
const wire = @import("wire.zig");

// Error-code constants are pub-re-exported from client_errors.zig so the
// transport-catch arms (UZ-EXEC-006) and the rpc-error arms (UZ-EXEC-003/
// 004/005/007/010/011 via `errorCodeForRpcCode`) share the same source.
const ERR_EXEC_TRANSPORT_LOSS = client_errors.ERR_EXEC_TRANSPORT_LOSS;
const ERR_EXEC_STARTUP_POSTURE = client_errors.ERR_EXEC_STARTUP_POSTURE;

const log = logging.scoped(.executor_client);

const ClientError = client_errors.ClientError;
pub const classifyError = client_errors.classifyError;
const errorCodeForRpcCode = client_errors.errorCodeForRpcCode;

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
            log.err("connect_failed", .{ .error_code = ERR_EXEC_STARTUP_POSTURE, .socket = self.transport_client.socket_path });
            executor_metrics.incExecutorFailures();
            return ClientError.ConnectionFailed;
        };
        log.info("connected", .{ .socket = self.transport_client.socket_path });
    }

    pub fn close(self: *ExecutorClient) void {
        self.transport_client.close();
    }

    fn nextId(self: *ExecutorClient) u64 {
        const id = self.next_id;
        self.next_id += 1;
        return id;
    }

    /// Per-execution policy + identity payload for `createExecution`.
    /// `workspace_path` and `correlation` are required; the policy fields
    /// default to empty (deny-all egress, no tool filter, no secrets,
    /// default context budget) so callers that don't yet care can omit them.
    pub const CreateExecutionParams = struct {
        workspace_path: []const u8,
        correlation: types.CorrelationContext,
        network_policy: context_budget.NetworkPolicy = .{},
        tools: []const []const u8 = &.{},
        /// JSON object: `{ [name]: <parsed json value> }`. Null = no secrets.
        secrets_map: ?std.json.Value = null,
        context: context_budget.ContextBudget = .{},
    };

    pub fn createExecution(
        self: *ExecutorClient,
        params_in: CreateExecutionParams,
    ) ![]const u8 {
        var params = std.json.Value{ .object = std.json.ObjectMap.init(self.alloc) };
        defer params.object.deinit();

        try params.object.put(wire.workspace_path, .{ .string = params_in.workspace_path });
        try params.object.put(wire.trace_id, .{ .string = params_in.correlation.trace_id });
        try params.object.put(wire.zombie_id, .{ .string = params_in.correlation.zombie_id });
        try params.object.put(wire.workspace_id, .{ .string = params_in.correlation.workspace_id });
        try params.object.put(wire.session_id, .{ .string = params_in.correlation.session_id });

        // Per-execution policy fields. Always serialised so the wire
        // shape is invariant; empty defaults are explicit.
        var network_policy_obj = std.json.ObjectMap.init(self.alloc);
        defer network_policy_obj.deinit();
        var allow_arr: std.json.Array = .init(self.alloc);
        defer allow_arr.deinit();
        for (params_in.network_policy.allow) |host| {
            try allow_arr.append(.{ .string = host });
        }
        try network_policy_obj.put(wire.allow, .{ .array = allow_arr });
        try params.object.put(wire.network_policy, .{ .object = network_policy_obj });

        var tools_arr: std.json.Array = .init(self.alloc);
        defer tools_arr.deinit();
        for (params_in.tools) |t| try tools_arr.append(.{ .string = t });
        try params.object.put(wire.tools, .{ .array = tools_arr });

        if (params_in.secrets_map) |sm| {
            try params.object.put(wire.secrets_map, sm);
        }

        var ctx_obj = std.json.ObjectMap.init(self.alloc);
        defer ctx_obj.deinit();
        try ctx_obj.put(wire.tool_window, .{ .integer = @intCast(params_in.context.tool_window) });
        try ctx_obj.put(wire.memory_checkpoint_every, .{ .integer = @intCast(params_in.context.memory_checkpoint_every) });
        try ctx_obj.put(wire.stage_chunk_threshold, .{ .float = @floatCast(params_in.context.stage_chunk_threshold) });
        try ctx_obj.put(wire.model, .{ .string = params_in.context.model });
        try ctx_obj.put(wire.context_cap_tokens, .{ .integer = @intCast(params_in.context.context_cap_tokens) });
        try params.object.put(wire.context, .{ .object = ctx_obj });

        var resp = self.transport_client.sendRequest(self.nextId(), protocol.Method.create_execution, params) catch {
            log.err("transport_loss", .{ .error_code = ERR_EXEC_TRANSPORT_LOSS, .method = "CreateExecution" });
            executor_metrics.incExecutorFailures();
            return ClientError.TransportLoss;
        };
        defer resp.deinit();

        if (resp.rpc_error) |err| {
            log.err("create_execution_failed", .{ .error_code = errorCodeForRpcCode(err.code), .code = err.code, .msg = err.message });
            return ClientError.ExecutionFailed;
        }

        const result = resp.result orelse return ClientError.InvalidResponse;
        if (result != .object) return ClientError.InvalidResponse;
        const exec_id_val = result.object.get(wire.execution_id) orelse return ClientError.InvalidResponse;
        return switch (exec_id_val) {
            .string => |s| try self.alloc.dupe(u8, s),
            else => ClientError.InvalidResponse,
        };
    }

    /// Result from a StartStage RPC call.
    ///
    /// Ownership: `.content` is allocated via the ExecutorClient's allocator.
    /// The caller must free it when done.
    pub const StageResult = struct {
        content: []const u8,
        token_count: u64,
        wall_seconds: u64,
        exit_ok: bool,
        failure: ?types.FailureClass,
        memory_peak_bytes: u64 = 0,
        cpu_throttled_ms: u64 = 0,
        /// Only populated via getUsage() — start_stage response does not include it.
        memory_limit_bytes: u64 = 0,
        /// ms from stage start to first token received. 0 if executor did not report.
        time_to_first_token_ms: u64 = 0,
        /// Set when the agent voluntarily ended a stage to be resumed in a fresh
        /// stage (L3 chunk-threshold trigger). Caller-owned (allocator dupe)
        /// when non-null. The worker reads this on `exit_ok=false` to drive
        /// continuation re-enqueue (§7).
        checkpoint_id: ?[]const u8 = null,
    };

    /// Agent configuration for StartStage payload.
    pub const AgentConfig = struct {
        model: []const u8 = "",
        provider: []const u8 = "anthropic",
        system_prompt: []const u8 = "",
        temperature: f64 = 0.7,
        max_tokens: u64 = 16384,
        /// LLM provider API key fetched from vault.secrets per workspace.
        /// Injected into NullClaw Config by the executor runner — never read from environ.
        api_key: []const u8 = "",
    };

    /// Full StartStage payload carrying agent execution context.
    /// Zombie-native: identity travels via CorrelationContext on CreateExecution;
    /// StartStage only carries per-turn execution payload.
    pub const StagePayload = struct {
        agent_config: AgentConfig = .{},
        message: []const u8 = "",
        tools: ?std.json.Value = null,
        context: ?std.json.Value = null,
    };

    /// Send a StartStage RPC, non-streaming. Equivalent to `startStageStreaming`
    /// with a no-op progress emitter — kept for call sites that don't (yet)
    /// want a live-tail consumer.
    pub fn startStage(
        self: *ExecutorClient,
        execution_id: []const u8,
        payload: StagePayload,
    ) !StageResult {
        return self.startStageStreaming(execution_id, payload, transport.ProgressEmitter.noop());
    }

    /// Send a StartStage RPC and dispatch executor-side progress frames to
    /// `emitter` as they arrive. Returns the terminal StageResult once the
    /// executor's terminal frame lands.
    pub fn startStageStreaming(
        self: *ExecutorClient,
        execution_id: []const u8,
        payload: StagePayload,
        emitter: transport.ProgressEmitter,
    ) !StageResult {
        var params = std.json.Value{ .object = std.json.ObjectMap.init(self.alloc) };
        defer params.object.deinit();

        try params.object.put(wire.execution_id, .{ .string = execution_id });
        try params.object.put(wire.message, .{ .string = payload.message });

        // Deinit ac separately — params.object.deinit() does not recurse into nested ObjectMaps.
        var ac = std.json.Value{ .object = std.json.ObjectMap.init(self.alloc) };
        defer ac.object.deinit();
        try ac.object.put(wire.model, .{ .string = payload.agent_config.model });
        try ac.object.put(wire.provider, .{ .string = payload.agent_config.provider });
        try ac.object.put(wire.system_prompt, .{ .string = payload.agent_config.system_prompt });
        try ac.object.put(wire.temperature, .{ .float = payload.agent_config.temperature });
        try ac.object.put(wire.max_tokens, .{ .integer = @intCast(payload.agent_config.max_tokens) });
        if (payload.agent_config.api_key.len > 0)
            try ac.object.put(wire.api_key, .{ .string = payload.agent_config.api_key });
        try params.object.put(wire.agent_config, ac);

        if (payload.tools) |t| try params.object.put(wire.tools, t);
        if (payload.context) |c| try params.object.put(wire.context, c);

        var resp = self.transport_client.sendRequestStreaming(self.nextId(), protocol.Method.start_stage, params, emitter) catch {
            log.err("transport_loss", .{ .error_code = ERR_EXEC_TRANSPORT_LOSS, .method = "StartStage", .execution_id = execution_id });
            executor_metrics.incExecutorFailures();
            return ClientError.TransportLoss;
        };
        defer resp.deinit();

        if (resp.rpc_error) |err| {
            log.err("start_stage_failed", .{ .error_code = errorCodeForRpcCode(err.code), .code = err.code, .msg = err.message });
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

        const checkpoint: ?[]const u8 = if (json.getStr(result, wire.checkpoint_id)) |s|
            try self.alloc.dupe(u8, s)
        else
            null;
        return .{
            .content = try self.alloc.dupe(u8, json.getStr(result, wire.content) orelse ""),
            .token_count = json.getIntOrZero(result, wire.token_count),
            .wall_seconds = json.getIntOrZero(result, wire.wall_seconds),
            .exit_ok = json.getBool(result, wire.exit_ok),
            .failure = null,
            .memory_peak_bytes = json.getIntOrZero(result, wire.memory_peak_bytes),
            .cpu_throttled_ms = json.getIntOrZero(result, wire.cpu_throttled_ms),
            .time_to_first_token_ms = json.getIntOrZero(result, wire.time_to_first_token_ms),
            .checkpoint_id = checkpoint,
        };
    }

    pub fn cancelExecution(self: *ExecutorClient, execution_id: []const u8) !void {
        var params = std.json.Value{ .object = std.json.ObjectMap.init(self.alloc) };
        defer params.object.deinit();
        try params.object.put(wire.execution_id, .{ .string = execution_id });

        var resp = self.transport_client.sendRequest(self.nextId(), protocol.Method.cancel_execution, params) catch {
            log.err("transport_loss", .{ .error_code = ERR_EXEC_TRANSPORT_LOSS, .method = "CancelExecution", .execution_id = execution_id });
            executor_metrics.incExecutorFailures();
            return ClientError.TransportLoss;
        };
        defer resp.deinit();
    }

    pub fn getUsage(self: *ExecutorClient, execution_id: []const u8) !StageResult {
        var params = std.json.Value{ .object = std.json.ObjectMap.init(self.alloc) };
        defer params.object.deinit();
        try params.object.put(wire.execution_id, .{ .string = execution_id });

        var resp = self.transport_client.sendRequest(self.nextId(), protocol.Method.get_usage, params) catch {
            log.err("transport_loss", .{ .error_code = ERR_EXEC_TRANSPORT_LOSS, .method = "GetUsage", .execution_id = execution_id });
            executor_metrics.incExecutorFailures();
            return ClientError.TransportLoss;
        };
        defer resp.deinit();

        if (resp.rpc_error != null) return ClientError.ExecutionFailed;
        const result = resp.result orelse return ClientError.InvalidResponse;
        if (result != .object) return ClientError.InvalidResponse;

        return .{
            .content = "",
            .token_count = json.getIntOrZero(result, wire.token_count),
            .wall_seconds = json.getIntOrZero(result, wire.wall_seconds),
            .exit_ok = json.getBool(result, wire.exit_ok),
            .failure = null,
            .memory_peak_bytes = json.getIntOrZero(result, wire.memory_peak_bytes),
            .cpu_throttled_ms = json.getIntOrZero(result, wire.cpu_throttled_ms),
            .memory_limit_bytes = json.getIntOrZero(result, wire.memory_limit_bytes),
            .time_to_first_token_ms = json.getIntOrZero(result, wire.time_to_first_token_ms),
        };
    }

    pub fn destroyExecution(self: *ExecutorClient, execution_id: []const u8) !void {
        var params = std.json.Value{ .object = std.json.ObjectMap.init(self.alloc) };
        defer params.object.deinit();
        try params.object.put(wire.execution_id, .{ .string = execution_id });

        var resp = self.transport_client.sendRequest(self.nextId(), protocol.Method.destroy_execution, params) catch {
            log.err("transport_loss", .{ .error_code = ERR_EXEC_TRANSPORT_LOSS, .method = "DestroyExecution", .execution_id = execution_id });
            executor_metrics.incExecutorFailures();
            return ClientError.TransportLoss;
        };
        defer resp.deinit();
    }

    pub fn heartbeat(self: *ExecutorClient, execution_id: []const u8) !void {
        var params = std.json.Value{ .object = std.json.ObjectMap.init(self.alloc) };
        defer params.object.deinit();
        try params.object.put(wire.execution_id, .{ .string = execution_id });

        var resp = self.transport_client.sendRequest(self.nextId(), protocol.Method.heartbeat, params) catch {
            log.err("transport_loss", .{ .error_code = ERR_EXEC_TRANSPORT_LOSS, .method = "Heartbeat", .execution_id = execution_id });
            executor_metrics.incExecutorFailures();
            return ClientError.TransportLoss;
        };
        defer resp.deinit();
    }
};

// Tests live in client_test.zig (split per RULE FLL).
