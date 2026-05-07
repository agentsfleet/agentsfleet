//! RPC dispatch handler for the executor sidecar.
//!
//! Maps JSON-RPC method names to session lifecycle operations.
//! This is the core of the executor API (§2.0).
//!
//! Generic JSON helpers live in `json_helpers.zig`. ExecutionPolicy parsing
//! lives in `context_budget.zig`. ExecutionId parsing lives in `types.zig`.
//! FailureClass→RPC code mapping lives in `protocol.zig`. Tests live in
//! `handler_test.zig`.

const std = @import("std");
const logging = @import("log");
const json = @import("json_helpers.zig");
const protocol = @import("protocol.zig");
const types = @import("types.zig");
const context_budget = @import("context_budget.zig");
const wire = @import("wire.zig");
const Session = @import("session.zig");
const SessionStore = @import("runtime/session_store.zig");
const executor_metrics = @import("executor_metrics.zig");
const runner = @import("runner.zig");
const network = @import("network.zig");
const progress_writer_mod = @import("progress_writer.zig");

const log = logging.scoped(.executor_handler);

/// Error-response message bodies. Centralising these keeps each unique
/// failure mode greppable (one place to find every message users see) and
/// satisfies RULE UFS — string literals belong in named constants.
const Msg = struct {
    const invalid_jsonrpc: []const u8 = "Invalid JSON-RPC request";
    const unknown_method: []const u8 = "Unknown method";
    const missing_params: []const u8 = "Missing params";
    const params_not_object: []const u8 = "Params must be object";
    const missing_workspace_path: []const u8 = "Missing workspace_path";
    const missing_execution_id: []const u8 = "Missing execution_id";
    const invalid_execution_id: []const u8 = "Invalid execution_id";
    const session_not_found: []const u8 = "Session not found";
    const session_cancelled: []const u8 = "Session cancelled";
    const lease_expired: []const u8 = "Lease expired";
    const alloc_session_failed: []const u8 = "Failed to allocate session";
    const dupe_session_inputs_failed: []const u8 = "Failed to dupe session inputs";
    const store_session_failed: []const u8 = "Failed to store session";
    const parse_policy_failed: []const u8 = "Failed to parse execution policy";
    const content_escape_failed: []const u8 = "Content escape failed";
};

pub const Handler = struct {
    store: *SessionStore,
    lease_timeout_ms: u64,
    resource_limits: types.ResourceLimits,
    network_policy: network.PolicyMode,
    alloc: std.mem.Allocator,

    pub fn init(
        alloc: std.mem.Allocator,
        store: *SessionStore,
        lease_timeout_ms: u64,
        resource_limits: types.ResourceLimits,
        net_policy: network.PolicyMode,
    ) Handler {
        return .{
            .store = store,
            .lease_timeout_ms = lease_timeout_ms,
            .resource_limits = resource_limits,
            .network_policy = net_policy,
            .alloc = alloc,
        };
    }

    /// Top-level frame handler called by the transport layer. Tests invoke
    /// this 2-arg form directly; the streaming wire path goes through
    /// `handleFrameWithFd` so StartStage can write progress notifications
    /// back to the same connection mid-execution.
    pub fn handleFrame(self: *Handler, alloc: std.mem.Allocator, payload: []const u8) ![]u8 {
        return self.handleFrameWithFd(alloc, payload, null);
    }

    /// Frame handler with the optional connection fd plumbed through.
    /// Bound to the transport layer in main.zig.
    pub fn handleFrameWithFd(
        self: *Handler,
        alloc: std.mem.Allocator,
        payload: []const u8,
        conn_fd: ?std.posix.socket_t,
    ) ![]u8 {
        var req = protocol.parseRequest(alloc, payload) catch {
            return self.errorResponse(alloc, 0, protocol.ErrorCode.parse_error, Msg.invalid_jsonrpc);
        };
        defer req.deinit();

        return self.dispatch(alloc, req.id, req.method, req.params, conn_fd);
    }

    fn dispatch(
        self: *Handler,
        alloc: std.mem.Allocator,
        id: u64,
        method: []const u8,
        params: ?std.json.Value,
        conn_fd: ?std.posix.socket_t,
    ) ![]u8 {
        if (std.mem.eql(u8, method, protocol.Method.create_execution)) {
            return self.handleCreateExecution(alloc, id, params);
        } else if (std.mem.eql(u8, method, protocol.Method.start_stage)) {
            return self.handleStartStage(alloc, id, params, conn_fd);
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
            return self.errorResponse(alloc, id, protocol.ErrorCode.method_not_found, Msg.unknown_method);
        }
    }

    fn handleCreateExecution(self: *Handler, alloc: std.mem.Allocator, id: u64, params: ?std.json.Value) ![]u8 {
        const p = params orelse return self.errorResponse(alloc, id, protocol.ErrorCode.invalid_params, Msg.missing_params);
        if (p != .object) return self.errorResponse(alloc, id, protocol.ErrorCode.invalid_params, Msg.params_not_object);

        const workspace_path = json.getStr(p, wire.workspace_path) orelse return self.errorResponse(alloc, id, protocol.ErrorCode.invalid_params, Msg.missing_workspace_path);

        const correlation = types.CorrelationContext{
            .trace_id = json.getStr(p, wire.trace_id) orelse "",
            .zombie_id = json.getStr(p, wire.zombie_id) orelse "",
            .workspace_id = json.getStr(p, wire.workspace_id) orelse "",
            .session_id = json.getStr(p, wire.session_id) orelse "",
        };

        // Per-execution policy fields. All optional — absent fields mean
        // "no policy" (deny-all egress, no tool restriction, no secrets,
        // default context budget). Intermediate slices live on a scoped
        // arena that's freed below; Session.create deep-dupes everything
        // it needs into its own arena before this one tears down.
        var policy_arena = std.heap.ArenaAllocator.init(alloc);
        defer policy_arena.deinit();
        const policy = context_budget.ExecutionPolicy.fromJson(policy_arena.allocator(), p) catch {
            return self.errorResponse(alloc, id, protocol.ErrorCode.internal_error, Msg.parse_policy_failed);
        };

        const session_ptr = self.alloc.create(Session) catch {
            return self.errorResponse(alloc, id, protocol.ErrorCode.internal_error, Msg.alloc_session_failed);
        };
        session_ptr.* = Session.create(
            self.alloc,
            workspace_path,
            correlation,
            self.resource_limits,
            self.lease_timeout_ms,
            policy,
        ) catch {
            self.alloc.destroy(session_ptr);
            return self.errorResponse(alloc, id, protocol.ErrorCode.internal_error, Msg.dupe_session_inputs_failed);
        };

        self.store.put(session_ptr) catch {
            session_ptr.destroy();
            self.alloc.destroy(session_ptr);
            return self.errorResponse(alloc, id, protocol.ErrorCode.internal_error, Msg.store_session_failed);
        };

        executor_metrics.incExecutorSessionsCreated();
        executor_metrics.setExecutorSessionsActive(@intCast(self.store.activeCount()));

        const hex = types.executionIdHex(session_ptr.execution_id);
        log.info("create_execution", .{ .execution_id = &hex, .zombie_id = correlation.zombie_id });

        return std.fmt.allocPrint(alloc,
            \\{{"id":{d},"result":{{"execution_id":"{s}"}}}}
        , .{ id, &hex });
    }

    fn handleStartStage(
        self: *Handler,
        alloc: std.mem.Allocator,
        id: u64,
        params: ?std.json.Value,
        conn_fd: ?std.posix.socket_t,
    ) ![]u8 {
        const p = params orelse return self.errorResponse(alloc, id, protocol.ErrorCode.invalid_params, Msg.missing_params);
        if (p != .object) return self.errorResponse(alloc, id, protocol.ErrorCode.invalid_params, Msg.params_not_object);

        const exec_id_hex = json.getStr(p, wire.execution_id) orelse return self.errorResponse(alloc, id, protocol.ErrorCode.invalid_params, Msg.missing_execution_id);
        const exec_id = types.parseExecutionId(exec_id_hex) orelse return self.errorResponse(alloc, id, protocol.ErrorCode.invalid_params, Msg.invalid_execution_id);

        const session = self.store.get(exec_id) orelse return self.errorResponse(alloc, id, protocol.ErrorCode.execution_failed, Msg.session_not_found);

        if (session.isCancelled()) {
            return self.errorResponse(alloc, id, protocol.ErrorCode.execution_failed, Msg.session_cancelled);
        }
        if (session.isLeaseExpired()) {
            executor_metrics.incExecutorLeaseExpired();
            return self.errorResponse(alloc, id, protocol.ErrorCode.lease_expired, Msg.lease_expired);
        }

        const hex = types.executionIdHex(exec_id);

        const agent_config = json.getObjectParam(p, wire.agent_config);
        const tools_spec = json.getArrayParam(p, wire.tools);
        const message = json.getStr(p, wire.message);
        const context = json.getObjectParam(p, wire.context);

        const model_name = if (agent_config) |ac| json.getStr(ac, wire.model) orelse "default" else "default";
        log.info("runner_start", .{
            .execution_id = &hex,
            .zombie_id = session.correlation.zombie_id,
            .session_id = session.correlation.session_id,
            .model = model_name,
            .network_policy = @tagName(self.network_policy),
        });

        // Build a per-call progress writer when this frame arrived on a real
        // socket. Direct `handleFrame` callers (tests, repair paths) pass null
        // and the runner falls back to a noop observer.
        const writer_storage = if (conn_fd) |fd| progress_writer_mod{
            .fd = fd,
            .request_id = id,
            .alloc = alloc,
        } else null;
        const writer_ptr: ?*const progress_writer_mod = if (writer_storage) |*w| w else null;

        // Invoke NullClaw runner — this blocks until agent execution completes.
        const result = runner.execute(
            alloc,
            session.workspace_path,
            agent_config,
            tools_spec,
            message,
            context,
            writer_ptr,
            &session.policy,
        );
        // Ownership: every non-empty `content` is an `alloc.dupe` from the
        // runner; every empty `content` is a `""` literal returned by failure
        // or no-op branches and is therefore not heap-allocated. Free iff
        // non-empty.
        defer if (result.content.len > 0) alloc.free(result.content);
        session.recordStageResult(result);
        session.touchLease();

        if (!result.exit_ok) {
            const failure_label = if (result.failure) |f| f.label() else "unknown";
            log.err("stage_failed", .{ .execution_id = &hex, .failure = failure_label });
            const err_code = protocol.failureToRpcError(result.failure);
            return self.errorResponse(alloc, id, err_code, failure_label);
        }

        const escaped_content = json.escapeAlloc(alloc, result.content) catch {
            return self.errorResponse(alloc, id, protocol.ErrorCode.internal_error, Msg.content_escape_failed);
        };
        defer alloc.free(escaped_content);

        return std.fmt.allocPrint(alloc,
            \\{{"id":{d},"result":{{"content":"{s}","token_count":{d},"wall_seconds":{d},"exit_ok":true,"memory_peak_bytes":{d},"cpu_throttled_ms":{d}}}}}
        , .{ id, escaped_content, result.token_count, result.wall_seconds, result.memory_peak_bytes, result.cpu_throttled_ms });
    }

    fn handleCancelExecution(self: *Handler, alloc: std.mem.Allocator, id: u64, params: ?std.json.Value) ![]u8 {
        const p = params orelse return self.errorResponse(alloc, id, protocol.ErrorCode.invalid_params, Msg.missing_params);
        if (p != .object) return self.errorResponse(alloc, id, protocol.ErrorCode.invalid_params, Msg.params_not_object);

        const exec_id_hex = json.getStr(p, wire.execution_id) orelse return self.errorResponse(alloc, id, protocol.ErrorCode.invalid_params, Msg.missing_execution_id);
        const exec_id = types.parseExecutionId(exec_id_hex) orelse return self.errorResponse(alloc, id, protocol.ErrorCode.invalid_params, Msg.invalid_execution_id);

        const session = self.store.get(exec_id) orelse return self.errorResponse(alloc, id, protocol.ErrorCode.execution_failed, Msg.session_not_found);
        session.cancel();
        executor_metrics.incExecutorCancellations();

        return std.fmt.allocPrint(alloc, "{{\"{s}\":{d},\"{s}\":{s}}}", .{ "id", id, "result", "true" });
    }

    fn handleGetUsage(self: *Handler, alloc: std.mem.Allocator, id: u64, params: ?std.json.Value) ![]u8 {
        const p = params orelse return self.errorResponse(alloc, id, protocol.ErrorCode.invalid_params, Msg.missing_params);
        if (p != .object) return self.errorResponse(alloc, id, protocol.ErrorCode.invalid_params, Msg.params_not_object);

        const exec_id_hex = json.getStr(p, wire.execution_id) orelse return self.errorResponse(alloc, id, protocol.ErrorCode.invalid_params, Msg.missing_execution_id);
        const exec_id = types.parseExecutionId(exec_id_hex) orelse return self.errorResponse(alloc, id, protocol.ErrorCode.invalid_params, Msg.invalid_execution_id);

        const session = self.store.get(exec_id) orelse return self.errorResponse(alloc, id, protocol.ErrorCode.execution_failed, Msg.session_not_found);
        const usage = session.getUsage();
        const res_ctx = session.getResourceContext();

        return std.fmt.allocPrint(alloc,
            \\{{"id":{d},"result":{{"token_count":{d},"wall_seconds":{d},"exit_ok":{s},"memory_peak_bytes":{d},"cpu_throttled_ms":{d},"memory_limit_bytes":{d}}}}}
        , .{ id, usage.token_count, usage.wall_seconds, if (usage.exit_ok) "true" else "false", usage.memory_peak_bytes, usage.cpu_throttled_ms, res_ctx.memory_limit_bytes });
    }

    fn handleDestroyExecution(self: *Handler, alloc: std.mem.Allocator, id: u64, params: ?std.json.Value) ![]u8 {
        const p = params orelse return self.errorResponse(alloc, id, protocol.ErrorCode.invalid_params, Msg.missing_params);
        if (p != .object) return self.errorResponse(alloc, id, protocol.ErrorCode.invalid_params, Msg.params_not_object);

        const exec_id_hex = json.getStr(p, wire.execution_id) orelse return self.errorResponse(alloc, id, protocol.ErrorCode.invalid_params, Msg.missing_execution_id);
        const exec_id = types.parseExecutionId(exec_id_hex) orelse return self.errorResponse(alloc, id, protocol.ErrorCode.invalid_params, Msg.invalid_execution_id);

        const session = self.store.remove(exec_id) orelse return self.errorResponse(alloc, id, protocol.ErrorCode.execution_failed, Msg.session_not_found);
        session.destroy();
        self.alloc.destroy(session);
        executor_metrics.setExecutorSessionsActive(@intCast(self.store.activeCount()));

        return std.fmt.allocPrint(alloc, "{{\"{s}\":{d},\"{s}\":{s}}}", .{ "id", id, "result", "true" });
    }

    fn handleHeartbeat(self: *Handler, alloc: std.mem.Allocator, id: u64, params: ?std.json.Value) ![]u8 {
        const p = params orelse return self.errorResponse(alloc, id, protocol.ErrorCode.invalid_params, Msg.missing_params);
        if (p != .object) return self.errorResponse(alloc, id, protocol.ErrorCode.invalid_params, Msg.params_not_object);

        const exec_id_hex = json.getStr(p, wire.execution_id) orelse return self.errorResponse(alloc, id, protocol.ErrorCode.invalid_params, Msg.missing_execution_id);
        const exec_id = types.parseExecutionId(exec_id_hex) orelse return self.errorResponse(alloc, id, protocol.ErrorCode.invalid_params, Msg.invalid_execution_id);

        const session = self.store.get(exec_id) orelse return self.errorResponse(alloc, id, protocol.ErrorCode.execution_failed, Msg.session_not_found);
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

test {
    _ = @import("handler_test.zig");
}
