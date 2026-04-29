//! Execution session lifecycle. File-as-struct: this file IS `Session`.
//! Each session represents a single execution (run) driven by the worker.
//! The executor owns the NullClaw runtime and sandbox enforcement within
//! the session boundary.
//!
//! Consumers `@import` it directly:
//!
//!     const Session = @import("session.zig");
//!     var sess = try Session.create(alloc, ws, corr, lim, lease_ms, policy);
//!     defer sess.destroy();

const std = @import("std");
const types = @import("types.zig");
const context_budget = @import("context_budget.zig");

const log = std.log.scoped(.executor_session);

const Session = @This();

execution_id: types.ExecutionId,
correlation: types.CorrelationContext,
lease: types.LeaseState,
resource_limits: types.ResourceLimits,
workspace_path: []const u8,
/// Per-execution policy bundle: network allowlist, tool allowlist,
/// resolved secrets_map, context-budget knobs. Set at createExecution
/// and invariant for the session's lifetime — every stage inherits
/// these. All inner slices are arena-owned dupes.
policy: context_budget.ExecutionPolicy,
cancelled: std.atomic.Value(bool),
arena: std.heap.ArenaAllocator,

// Stage execution results (last completed stage).
last_result: ?types.ExecutionResult = null,
total_tokens: u64 = 0,
total_wall_seconds: u64 = 0,
stages_executed: u32 = 0,
/// Highest peak memory across all stages (max, not sum).
max_memory_peak_bytes: u64 = 0,
/// Total CPU throttle time across all stages.
total_cpu_throttled_ms: u64 = 0,

/// Resource limits context for scoring normalization.
const ResourceContext = struct {
    memory_limit_bytes: u64,
};

/// Create a session, duping `workspace_path` and every CorrelationContext
/// string into the session's own arena. The caller-supplied slices are
/// typically borrowed from a transient request frame that gets freed
/// before the session is used; without this dupe the session ends up
/// holding dangling pointers and SEGVs on first read (e.g. NullClaw
/// memory-init reading workspace_dir).
pub fn create(
    alloc: std.mem.Allocator,
    workspace_path: []const u8,
    correlation: types.CorrelationContext,
    resource_limits: types.ResourceLimits,
    lease_timeout_ms: u64,
    policy: context_budget.ExecutionPolicy,
) !Session {
    var arena = std.heap.ArenaAllocator.init(alloc);
    errdefer arena.deinit();
    const arena_alloc = arena.allocator();

    const wp_owned = try arena_alloc.dupe(u8, workspace_path);
    const correlation_owned = types.CorrelationContext{
        .trace_id = try arena_alloc.dupe(u8, correlation.trace_id),
        .zombie_id = try arena_alloc.dupe(u8, correlation.zombie_id),
        .workspace_id = try arena_alloc.dupe(u8, correlation.workspace_id),
        .session_id = try arena_alloc.dupe(u8, correlation.session_id),
    };

    const policy_owned = try dupePolicy(arena_alloc, policy);

    const execution_id = types.generateExecutionId();
    return .{
        .execution_id = execution_id,
        .correlation = correlation_owned,
        .lease = .{
            .execution_id = execution_id,
            .last_heartbeat_ms = std.time.milliTimestamp(),
            .lease_timeout_ms = lease_timeout_ms,
        },
        .resource_limits = resource_limits,
        .workspace_path = wp_owned,
        .policy = policy_owned,
        .cancelled = std.atomic.Value(bool).init(false),
        .arena = arena,
    };
}

pub fn cancel(self: *Session) void {
    self.cancelled.store(true, .release);
    const hex = types.executionIdHex(self.execution_id);
    log.info("session.cancelled execution_id={s}", .{&hex});
}

pub fn isCancelled(self: *const Session) bool {
    return self.cancelled.load(.acquire);
}

pub fn touchLease(self: *Session) void {
    self.lease.touch();
}

pub fn isLeaseExpired(self: *const Session) bool {
    return self.lease.isExpired();
}

pub fn recordStageResult(self: *Session, result: types.ExecutionResult) void {
    self.last_result = result;
    self.total_tokens += result.token_count;
    self.total_wall_seconds += result.wall_seconds;
    self.stages_executed += 1;
    if (result.memory_peak_bytes > self.max_memory_peak_bytes) {
        self.max_memory_peak_bytes = result.memory_peak_bytes;
    }
    self.total_cpu_throttled_ms += result.cpu_throttled_ms;
}

pub fn getUsage(self: *const Session) types.ExecutionResult {
    return .{
        .content = "",
        .token_count = self.total_tokens,
        .wall_seconds = self.total_wall_seconds,
        .exit_ok = self.last_result != null and (self.last_result.?.exit_ok),
        .failure = if (self.last_result) |r| r.failure else null,
        .memory_peak_bytes = self.max_memory_peak_bytes,
        .cpu_throttled_ms = self.total_cpu_throttled_ms,
    };
}

pub fn getResourceContext(self: *const Session) ResourceContext {
    return .{
        .memory_limit_bytes = self.resource_limits.memory_limit_mb * 1024 * 1024,
    };
}

pub fn destroy(self: *Session) void {
    const hex = types.executionIdHex(self.execution_id);
    log.info("session.destroyed execution_id={s} stages={d} tokens={d}", .{ &hex, self.stages_executed, self.total_tokens });
    self.arena.deinit();
}

/// Deep-copy an `ExecutionPolicy` into `arena_alloc`. Caller-provided
/// slices (network_policy.allow, tools, secrets_map JSON tree, model name)
/// are typically borrowed from a transient request frame; without this dupe
/// the session would dangle the moment the RPC handler returns.
fn dupePolicy(arena_alloc: std.mem.Allocator, p: context_budget.ExecutionPolicy) !context_budget.ExecutionPolicy {
    const allow_owned = try dupeStringList(arena_alloc, p.network_policy.allow);
    const tools_owned = try dupeStringList(arena_alloc, p.tools);
    const secrets_owned: ?std.json.Value = if (p.secrets_map) |sm| try dupeJsonValue(arena_alloc, sm) else null;
    const model_owned = try arena_alloc.dupe(u8, p.context.model);
    return .{
        .network_policy = .{ .allow = allow_owned },
        .tools = tools_owned,
        .secrets_map = secrets_owned,
        .context = .{
            .tool_window = p.context.tool_window,
            .memory_checkpoint_every = p.context.memory_checkpoint_every,
            .stage_chunk_threshold = p.context.stage_chunk_threshold,
            .model = model_owned,
        },
    };
}

fn dupeStringList(arena_alloc: std.mem.Allocator, src: []const []const u8) ![]const []const u8 {
    const out = try arena_alloc.alloc([]const u8, src.len);
    for (src, 0..) |s, i| out[i] = try arena_alloc.dupe(u8, s);
    return out;
}

/// Recursively duplicate a `std.json.Value` into `arena_alloc`. The whole
/// tree ends up arena-owned, so a single arena.deinit reclaims everything.
fn dupeJsonValue(arena_alloc: std.mem.Allocator, v: std.json.Value) !std.json.Value {
    return switch (v) {
        .null, .bool, .integer, .float, .number_string => v,
        .string => |s| .{ .string = try arena_alloc.dupe(u8, s) },
        .array => |a| blk: {
            var out: std.json.Array = .init(arena_alloc);
            try out.ensureTotalCapacity(a.items.len);
            for (a.items) |item| out.appendAssumeCapacity(try dupeJsonValue(arena_alloc, item));
            break :blk .{ .array = out };
        },
        .object => |o| blk: {
            var out: std.json.ObjectMap = .init(arena_alloc);
            try out.ensureTotalCapacity(o.count());
            var it = o.iterator();
            while (it.next()) |entry| {
                const k = try arena_alloc.dupe(u8, entry.key_ptr.*);
                const val = try dupeJsonValue(arena_alloc, entry.value_ptr.*);
                out.putAssumeCapacity(k, val);
            }
            break :blk .{ .object = out };
        },
    };
}
