//! High-level executor client for the worker side.
//!
//! Wraps the transport Client with typed methods for each executor API RPC.
//! Translates JSON responses into AgentResult and FailureClass types.

const std = @import("std");
const json = @import("json_helpers.zig");
const transport = @import("transport.zig");
const protocol = @import("protocol.zig");
const types = @import("types.zig");
const context_budget = @import("context_budget.zig");
const executor_metrics = @import("executor_metrics.zig");

const log = std.log.scoped(.executor_client);

const ClientError = error{
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
pub fn classifyError(code: i32) types.FailureClass {
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

        try params.object.put("workspace_path", .{ .string = params_in.workspace_path });
        try params.object.put("trace_id", .{ .string = params_in.correlation.trace_id });
        try params.object.put("zombie_id", .{ .string = params_in.correlation.zombie_id });
        try params.object.put("workspace_id", .{ .string = params_in.correlation.workspace_id });
        try params.object.put("session_id", .{ .string = params_in.correlation.session_id });

        // Per-execution policy fields. Always serialised so the wire
        // shape is invariant; empty defaults are explicit.
        var network_policy_obj = std.json.ObjectMap.init(self.alloc);
        defer network_policy_obj.deinit();
        var allow_arr: std.json.Array = .init(self.alloc);
        defer allow_arr.deinit();
        for (params_in.network_policy.allow) |host| {
            try allow_arr.append(.{ .string = host });
        }
        try network_policy_obj.put("allow", .{ .array = allow_arr });
        try params.object.put("network_policy", .{ .object = network_policy_obj });

        var tools_arr: std.json.Array = .init(self.alloc);
        defer tools_arr.deinit();
        for (params_in.tools) |t| try tools_arr.append(.{ .string = t });
        try params.object.put("tools", .{ .array = tools_arr });

        if (params_in.secrets_map) |sm| {
            try params.object.put("secrets_map", sm);
        }

        var ctx_obj = std.json.ObjectMap.init(self.alloc);
        defer ctx_obj.deinit();
        try ctx_obj.put("tool_window", .{ .integer = @intCast(params_in.context.tool_window) });
        try ctx_obj.put("memory_checkpoint_every", .{ .integer = @intCast(params_in.context.memory_checkpoint_every) });
        try ctx_obj.put("stage_chunk_threshold", .{ .float = @floatCast(params_in.context.stage_chunk_threshold) });
        try ctx_obj.put("model", .{ .string = params_in.context.model });
        try params.object.put("context", .{ .object = ctx_obj });

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
    /// The caller must free it when done.
    pub const StageResult = struct {
        content: []const u8,
        token_count: u64,
        wall_seconds: u64,
        exit_ok: bool,
        failure: ?types.FailureClass,
        /// M27_001: peak RSS bytes from cgroup (0 = not available).
        memory_peak_bytes: u64 = 0,
        /// M27_001: CPU throttle time in ms (0 = not available).
        cpu_throttled_ms: u64 = 0,
        /// M27_001: memory limit in bytes for scoring normalization.
        /// Only populated via getUsage() — start_stage response does not include it.
        memory_limit_bytes: u64 = 0,
        /// M18_001: ms from stage start to first token received. 0 if executor did not report.
        time_to_first_token_ms: u64 = 0,
    };

    /// Agent configuration for StartStage payload (M12_003, M16_003).
    pub const AgentConfig = struct {
        model: []const u8 = "",
        provider: []const u8 = "anthropic",
        system_prompt: []const u8 = "",
        temperature: f64 = 0.7,
        max_tokens: u64 = 16384,
        /// LLM provider API key fetched from vault.secrets per workspace (M16_003 §1).
        /// Injected into NullClaw Config by the executor runner — never read from environ.
        api_key: []const u8 = "",
        /// GitHub App installation token for branch push and PR creation (M16_003 §2).
        /// Per-run, held in worker memory only, never persisted.
        github_token: []const u8 = "",
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

    /// Send a StartStage RPC, non-streaming. Equivalent to
    /// `startStageStreaming` with a no-op progress emitter — kept for
    /// call sites that don't (yet) want a live-tail consumer.
    pub fn startStage(
        self: *ExecutorClient,
        execution_id: []const u8,
        payload: StagePayload,
    ) !StageResult {
        return self.startStageStreaming(execution_id, payload, transport.ProgressEmitter.noop());
    }

    /// Send a StartStage RPC and dispatch executor-side progress
    /// frames to `emitter` as they arrive. Returns the terminal
    /// StageResult once the executor's terminal frame lands.
    ///
    /// Today (until slice 3c wires NullClaw lifecycle hooks into the
    /// executor-side emitter) the executor emits zero progress frames,
    /// so the streaming path degenerates to one-shot — `emitter.emit`
    /// fires zero times and behaviour matches the old `startStage`.
    pub fn startStageStreaming(
        self: *ExecutorClient,
        execution_id: []const u8,
        payload: StagePayload,
        emitter: transport.ProgressEmitter,
    ) !StageResult {
        var params = std.json.Value{ .object = std.json.ObjectMap.init(self.alloc) };
        defer params.object.deinit();

        try params.object.put("execution_id", .{ .string = execution_id });
        try params.object.put("message", .{ .string = payload.message });

        // Build agent_config object.
        // Deinit ac separately — params.object.deinit() does not recurse into nested ObjectMaps.
        var ac = std.json.Value{ .object = std.json.ObjectMap.init(self.alloc) };
        defer ac.object.deinit();
        try ac.object.put("model", .{ .string = payload.agent_config.model });
        try ac.object.put("provider", .{ .string = payload.agent_config.provider });
        try ac.object.put("system_prompt", .{ .string = payload.agent_config.system_prompt });
        try ac.object.put("temperature", .{ .float = payload.agent_config.temperature });
        try ac.object.put("max_tokens", .{ .integer = @intCast(payload.agent_config.max_tokens) });
        // Pass workspace LLM API key; omit field when empty so executor
        // falls back to its own env (dev mode).
        if (payload.agent_config.api_key.len > 0)
            try ac.object.put("api_key", .{ .string = payload.agent_config.api_key });
        // Pass GitHub installation token; omit when empty.
        if (payload.agent_config.github_token.len > 0)
            try ac.object.put("github_token", .{ .string = payload.agent_config.github_token });
        try params.object.put("agent_config", ac);

        // Attach optional tools array and context object.
        if (payload.tools) |t| try params.object.put("tools", t);
        if (payload.context) |c| try params.object.put("context", c);

        var resp = self.transport_client.sendRequestStreaming(self.nextId(), protocol.Method.start_stage, params, emitter) catch {
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
            .content = try self.alloc.dupe(u8, json.getStr(result, "content") orelse ""),
            .token_count = json.getIntOrZero(result, "token_count"),
            .wall_seconds = json.getIntOrZero(result, "wall_seconds"),
            .exit_ok = json.getBool(result, "exit_ok"),
            .failure = null,
            .memory_peak_bytes = json.getIntOrZero(result, "memory_peak_bytes"),
            .cpu_throttled_ms = json.getIntOrZero(result, "cpu_throttled_ms"),
            .time_to_first_token_ms = json.getIntOrZero(result, "time_to_first_token_ms"),
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
            .token_count = json.getIntOrZero(result, "token_count"),
            .wall_seconds = json.getIntOrZero(result, "wall_seconds"),
            .exit_ok = json.getBool(result, "exit_ok"),
            .failure = null,
            .memory_peak_bytes = json.getIntOrZero(result, "memory_peak_bytes"),
            .cpu_throttled_ms = json.getIntOrZero(result, "cpu_throttled_ms"),
            .memory_limit_bytes = json.getIntOrZero(result, "memory_limit_bytes"),
            .time_to_first_token_ms = json.getIntOrZero(result, "time_to_first_token_ms"),
        };
    }

    /// M21_001 §1.3: inject a user message into the active executor turn.
    /// TODO(M21 v2): wire this into the gate loop for true instant delivery.
    /// v1 uses startStage via the repair mechanism; this method has no call site yet.
    fn injectUserMessage(self: *ExecutorClient, execution_id: []const u8, message: []const u8) !void {
        var params = std.json.Value{ .object = std.json.ObjectMap.init(self.alloc) };
        defer params.object.deinit();
        try params.object.put("execution_id", .{ .string = execution_id });
        try params.object.put("message", .{ .string = message });

        var resp = self.transport_client.sendRequest(self.nextId(), protocol.Method.inject_user_message, params) catch {
            log.err("executor_client.transport_loss error_code=UZ-EXEC-006 method=InjectUserMessage execution_id={s}", .{execution_id});
            executor_metrics.incExecutorFailures();
            return ClientError.TransportLoss;
        };
        defer resp.deinit();

        if (resp.rpc_error) |err| {
            log.err("executor_client.inject_msg_failed code={d} msg={s}", .{ err.code, err.message });
            return ClientError.ExecutionFailed;
        }
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

// Tests live in client_test.zig (split per RULE FLL). M16_003 §1/§2 in client_credentials_test.zig.
