// Approval gate — 3D Secure model for high-stakes Zombie actions.
//
// Evaluates tool invocations against a gate policy. Matching actions
// pause the event loop, emit a Slack message, and block on Redis for
// human approval. Anomaly patterns (runaway loops) trigger auto-kill.
//
// Call order: checkAnomaly() → evaluateGate() → requestApproval() → waitForDecision()

const std = @import("std");
const pg = @import("pg");
const Allocator = std.mem.Allocator;
const queue_redis = @import("../queue/redis_client.zig");
const ec = @import("../errors/codes.zig");
const id_format = @import("../types/id_format.zig");
const activity_stream = @import("activity_stream.zig");
const config_gates = @import("config_gates.zig");

const log = std.log.scoped(.approval_gate);

pub const GateDecision = enum { auto_approve, requires_approval, auto_kill };

pub const GateResult = enum { approved, denied, timed_out };

pub const AnomalyResult = enum { normal, auto_kill };

pub const ActionDetail = struct {
    tool: []const u8,
    action: []const u8,
    params_summary: []const u8,
};

// ── Gate evaluation (spec §1.0) ─────────────────────────────────────────

/// Evaluate a tool action against the gate policy.
/// Rules are checked top-to-bottom; first match wins.
/// No match = auto-approve (silent pass-through).
pub fn evaluateGate(
    policy: config_gates.GatePolicy,
    tool: []const u8,
    action: []const u8,
    context: ?std.json.Value,
) GateDecision {
    for (policy.rules) |rule| {
        if (!matchesToolAction(rule, tool, action)) continue;
        if (rule.condition) |cond| {
            if (!evaluateCondition(cond, context)) continue;
        }
        return switch (rule.behavior) {
            .approve => .requires_approval,
            .auto_kill => .auto_kill,
        };
    }
    return .auto_approve;
}

fn matchesToolAction(
    rule: config_gates.GateRule,
    tool: []const u8,
    action: []const u8,
) bool {
    const tool_match = std.mem.eql(u8, rule.tool, "*") or std.mem.eql(u8, rule.tool, tool);
    const action_match = std.mem.eql(u8, rule.action, "*") or std.mem.eql(u8, rule.action, action);
    return tool_match and action_match;
}

/// Evaluate a simple condition expression against a JSON context.
/// Supports only: field == 'value' and field != 'value'.
/// Returns true if the condition is met (action should be gated).
fn evaluateCondition(condition: []const u8, context: ?std.json.Value) bool {
    const ctx = context orelse return true;
    const obj = switch (ctx) {
        .object => |o| o,
        else => return true,
    };

    // Parse "field == 'value'" or "field != 'value'"
    if (parseConditionParts(condition, " == ")) |parts| {
        const actual = getContextField(obj, parts.field) orelse return true;
        return std.mem.eql(u8, actual, parts.value);
    }
    if (parseConditionParts(condition, " != ")) |parts| {
        const actual = getContextField(obj, parts.field) orelse return true;
        return !std.mem.eql(u8, actual, parts.value);
    }

    // Invalid condition — treat as match (safe default: gate fires)
    log.warn("approval_gate.invalid_condition condition={s}", .{condition});
    return true;
}

const ConditionParts = struct { field: []const u8, value: []const u8 };

fn parseConditionParts(condition: []const u8, op: []const u8) ?ConditionParts {
    const idx = std.mem.indexOf(u8, condition, op) orelse return null;
    const field = std.mem.trim(u8, condition[0..idx], " ");
    const rhs = std.mem.trim(u8, condition[idx + op.len ..], " ");
    // Strip single quotes from value
    if (rhs.len >= 2 and rhs[0] == '\'' and rhs[rhs.len - 1] == '\'') {
        return ConditionParts{ .field = field, .value = rhs[1 .. rhs.len - 1] };
    }
    // Also accept unquoted for config.source_channel style
    return ConditionParts{ .field = field, .value = rhs };
}

fn getContextField(obj: std.json.ObjectMap, field: []const u8) ?[]const u8 {
    const val = obj.get(field) orelse return null;
    return switch (val) {
        .string => |s| s,
        else => null,
    };
}

// ── Anomaly detection (spec §3.0) ───────────────────────────────────────

/// Check anomaly counters. Runs BEFORE gate evaluation (fast path).
/// Uses Redis INCR + EXPIRE for sliding window per (zombie_id, tool, action).
pub fn checkAnomaly(
    redis: *queue_redis.Client,
    zombie_id: []const u8,
    tool: []const u8,
    action: []const u8,
    rules: []const config_gates.AnomalyRule,
) AnomalyResult {
    for (rules) |rule| {
        switch (rule.pattern) {
            .same_action => {},
        }
        const count = incrAnomalyCounter(redis, zombie_id, tool, action, rule.threshold_window_s) catch {
            // Redis unavailable — fail open for anomaly (spec §6.0)
            log.warn("approval_gate.anomaly_redis_fail zombie_id={s}", .{zombie_id});
            return .normal;
        };
        if (count > rule.threshold_count) {
            log.err("approval_gate.anomaly_auto_kill zombie_id={s} tool={s} action={s} count={d} threshold={d}", .{
                zombie_id, tool, action, count, rule.threshold_count,
            });
            return .auto_kill;
        }
    }
    return .normal;
}

fn incrAnomalyCounter(
    redis: *queue_redis.Client,
    zombie_id: []const u8,
    tool: []const u8,
    action: []const u8,
    window_s: u32,
) !u32 {
    var key_buf: [256]u8 = undefined;
    const key = std.fmt.bufPrint(&key_buf, "{s}{s}:{s}:{s}", .{
        ec.GATE_ANOMALY_KEY_PREFIX, zombie_id, tool, action,
    }) catch return error.BufferOverflow;

    // INCR key — creates with value 1 if not exists
    var resp = try redis.command(&.{ "INCR", key });
    defer resp.deinit(redis.alloc);
    const count: u32 = switch (resp) {
        .integer => |n| if (n > 0) @intCast(n) else 1,
        else => return error.RedisCommandError,
    };

    // Set EXPIRE only on first increment (count == 1)
    if (count == 1) {
        var ttl_buf: [16]u8 = undefined;
        const ttl_str = std.fmt.bufPrint(&ttl_buf, "{d}", .{window_s}) catch return error.BufferOverflow;
        var expire_resp = try redis.command(&.{ "EXPIRE", key, ttl_str });
        defer expire_resp.deinit(redis.alloc);
    }

    return count;
}

/// Reset all anomaly counters for a zombie (after restart/auto-kill recovery).
pub fn resetAnomalyCounters(
    redis: *queue_redis.Client,
    alloc: Allocator,
    zombie_id: []const u8,
) void {
    // Use SCAN to find and DEL matching keys
    var cursor_buf: [32]u8 = undefined;
    var cursor: []const u8 = "0";
    var pattern_buf: [128]u8 = undefined;
    const pattern = std.fmt.bufPrint(&pattern_buf, "{s}{s}:*", .{
        ec.GATE_ANOMALY_KEY_PREFIX, zombie_id,
    }) catch return;

    var iterations: u32 = 0;
    while (iterations < 100) : (iterations += 1) {
        var resp = redis.command(&.{ "SCAN", cursor, "MATCH", pattern, "COUNT", "100" }) catch return;
        defer resp.deinit(alloc);
        const arr = switch (resp) {
            .array => |a| a orelse return,
            else => return,
        };
        if (arr.len < 2) return;

        // Next cursor
        const next_cursor = switch (arr[0]) {
            .bulk => |b| b orelse return,
            .simple => |s| s,
            else => return,
        };
        cursor = std.fmt.bufPrint(&cursor_buf, "{s}", .{next_cursor}) catch return;

        // Delete matched keys
        const keys = switch (arr[1]) {
            .array => |k| k orelse continue,
            else => continue,
        };
        for (keys) |key_val| {
            const key_str = switch (key_val) {
                .bulk => |b| b orelse continue,
                .simple => |s| s,
                else => continue,
            };
            var del_resp = redis.command(&.{ "DEL", key_str }) catch continue;
            del_resp.deinit(alloc);
        }

        if (std.mem.eql(u8, cursor, "0")) break;
    }
    log.info("approval_gate.anomaly_counters_reset zombie_id={s}", .{zombie_id});
}

// ── Approval flow (spec §2.0) ───────────────────────────────────────────

/// Serialize pending action to Redis and return the action_id.
/// Caller should then send Slack message and call waitForDecision().
pub fn requestApproval(
    alloc: Allocator,
    redis: *queue_redis.Client,
    zombie_id: []const u8,
    detail: ActionDetail,
) ![]const u8 {
    const action_id = try id_format.generateActivityEventId(alloc);
    errdefer alloc.free(action_id);

    // Serialize pending action to Redis
    var pending_key_buf: [256]u8 = undefined;
    const pending_key = try std.fmt.bufPrint(&pending_key_buf, "{s}{s}:{s}", .{
        ec.GATE_PENDING_KEY_PREFIX, zombie_id, action_id,
    });

    // JSON-escape user-supplied fields to prevent injection (RULES.md #23)
    var detail_list = std.ArrayList(u8).init(alloc);
    defer detail_list.deinit();
    const dw = detail_list.writer();
    try dw.writeAll("{\"tool\":");
    try std.json.stringify(detail.tool, .{}, dw);
    try dw.writeAll(",\"action\":");
    try std.json.stringify(detail.action, .{}, dw);
    try dw.writeAll(",\"summary\":");
    try std.json.stringify(detail.params_summary, .{}, dw);
    try dw.writeAll("}");
    const detail_json = detail_list.items;

    try redis.setEx(pending_key, detail_json, ec.GATE_PENDING_TTL_SECONDS);

    log.info("approval_gate.requested zombie_id={s} action_id={s} tool={s} action={s}", .{
        zombie_id, action_id, detail.tool, detail.action,
    });

    return action_id;
}

/// Block on Redis for a human decision. Returns the gate result.
/// Uses polling with short sleeps since BRPOP is not available in
/// the current Redis client (single-connection, locked).
pub fn waitForDecision(
    redis: *queue_redis.Client,
    action_id: []const u8,
    timeout_ms: u64,
) GateResult {
    var response_key_buf: [256]u8 = undefined;
    const response_key = std.fmt.bufPrint(&response_key_buf, "{s}{s}", .{
        ec.GATE_RESPONSE_KEY_PREFIX, action_id,
    }) catch return .timed_out;

    const start_ms = @as(u64, @intCast(std.time.milliTimestamp()));
    const poll_interval_ns: u64 = 2_000_000_000; // 2 seconds

    while (true) {
        const elapsed = @as(u64, @intCast(std.time.milliTimestamp())) -| start_ms;
        if (elapsed >= timeout_ms) {
            log.info("approval_gate.timeout action_id={s} elapsed_ms={d}", .{ action_id, elapsed });
            return .timed_out;
        }

        // Check for response via GET (set by approval webhook handler)
        const decision = getDecision(redis, response_key) catch {
            log.warn("approval_gate.redis_poll_fail action_id={s}", .{action_id});
            return .timed_out;
        };
        if (decision) |d| return d;

        std.time.sleep(poll_interval_ns);
    }
}

fn getDecision(redis: *queue_redis.Client, key: []const u8) !?GateResult {
    var resp = try redis.commandAllowError(&.{ "GET", key });
    defer resp.deinit(redis.alloc);
    const val = switch (resp) {
        .bulk => |b| b orelse return null,
        .simple => |s| s,
        else => return null,
    };
    if (std.mem.eql(u8, val, ec.GATE_DECISION_APPROVE)) return .approved;
    if (std.mem.eql(u8, val, ec.GATE_DECISION_DENY)) return .denied;
    return null;
}

/// Write a gate decision to Redis (called by the approval webhook handler).
pub fn resolveApproval(
    redis: *queue_redis.Client,
    action_id: []const u8,
    decision: []const u8,
) !void {
    var response_key_buf: [256]u8 = undefined;
    const response_key = try std.fmt.bufPrint(&response_key_buf, "{s}{s}", .{
        ec.GATE_RESPONSE_KEY_PREFIX, action_id,
    });
    try redis.setEx(response_key, decision, ec.GATE_PENDING_TTL_SECONDS);

    log.info("approval_gate.resolved action_id={s} decision={s}", .{ action_id, decision });
}

/// Record a gate decision in the audit table (Postgres).
pub fn recordGateDecision(
    pool: *pg.Pool,
    alloc: Allocator,
    zombie_id: []const u8,
    workspace_id: []const u8,
    action_id: []const u8,
    tool_name: []const u8,
    action_name: []const u8,
    status: []const u8,
    detail: []const u8,
) void {
    writeGateRow(pool, alloc, zombie_id, workspace_id, action_id, tool_name, action_name, status, detail) catch |err| {
        log.err("approval_gate.record_fail err={s} action_id={s}", .{ @errorName(err), action_id });
    };
}

fn writeGateRow(
    pool: *pg.Pool,
    alloc: Allocator,
    zombie_id: []const u8,
    workspace_id: []const u8,
    action_id: []const u8,
    tool_name: []const u8,
    action_name: []const u8,
    status: []const u8,
    detail: []const u8,
) !void {
    const gate_id = try id_format.generateActivityEventId(alloc);
    defer alloc.free(gate_id);

    const conn = try pool.acquire();
    defer pool.release(conn);

    const now_ms = std.time.milliTimestamp();
    _ = try conn.exec(
        \\INSERT INTO core.zombie_approval_gates
        \\  (id, zombie_id, workspace_id, action_id, tool_name, action_name, status, detail, requested_at, created_at)
        \\VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $9)
    , .{ gate_id, zombie_id, workspace_id, action_id, tool_name, action_name, status, detail, now_ms });
}

// Re-export Slack message builder from separate module
pub const buildSlackApprovalMessage = @import("approval_gate_slack.zig").buildSlackApprovalMessage;

// Tests in approval_gate_test.zig (separate file for 400-line gate).
test {
    _ = @import("approval_gate_test.zig");
    _ = @import("approval_gate_slack.zig");
}
