// Approval gate — 3D Secure model for high-stakes Zombie actions.
//
// Evaluates tool invocations against a gate policy. Matching actions
// pause the event loop, emit a Slack message, and block on Redis for
// human approval. Anomaly patterns (runaway loops) trigger auto-kill.
//
// Call order: checkAnomaly() → evaluateGate() → requestApproval() → waitForDecision().
// Resolution: dashboard handler OR Slack callback OR sweeper calls resolve(),
// which atomically transitions pending → terminal status and wakes the worker.

const std = @import("std");
const pg = @import("pg");
const Allocator = std.mem.Allocator;
const queue_redis = @import("../queue/redis_client.zig");
const ec = @import("../errors/error_registry.zig");
const id_format = @import("../types/id_format.zig");
const config_gates = @import("config_gates.zig");

const approval_gate_db = @import("approval_gate_db.zig");
const approval_gate_anomaly = @import("approval_gate_anomaly.zig");

const log = std.log.scoped(.approval_gate);

pub const GateDecision = enum { auto_approve, requires_approval, auto_kill };

pub const GateResult = enum { approved, denied, timed_out };

/// Status values stored in core.zombie_approval_gates.status column.
/// Typed enum — no raw strings for gate status (RULES.md #36).
pub const GateStatus = enum {
    pending,
    approved,
    denied,
    timed_out,
    auto_killed,

    pub fn toSlice(self: GateStatus) []const u8 {
        return switch (self) {
            .pending => "pending",
            .approved => "approved",
            .denied => "denied",
            .timed_out => "timed_out",
            .auto_killed => "auto_killed",
        };
    }

    pub fn isTerminal(self: GateStatus) bool {
        return self != .pending;
    }
};

pub const ActionDetail = struct {
    tool: []const u8,
    action: []const u8,
    params_summary: []const u8,
    // Inbox-visible fields surfaced to operators in the dashboard.
    // Pre-serialized JSONB string for evidence_json keeps writers free of
    // an alloc dependency at the call site; '{}' is the "no evidence" default.
    gate_kind: []const u8 = "",
    proposed_action: []const u8 = "",
    evidence_json: []const u8 = "{}",
    blast_radius: []const u8 = "",
    timeout_ms: i64 = 24 * 60 * 60 * 1000,
};

// ── Gate evaluation ─────────────────────────────────────────────────────

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

    if (parseConditionParts(condition, " == ")) |parts| {
        const actual = getContextField(obj, parts.field) orelse return true;
        return std.mem.eql(u8, actual, parts.value);
    }
    if (parseConditionParts(condition, " != ")) |parts| {
        const actual = getContextField(obj, parts.field) orelse return true;
        return !std.mem.eql(u8, actual, parts.value);
    }

    // Invalid condition — treat as match (safe default: gate fires).
    log.warn("approval_gate.invalid_condition condition={s}", .{condition});
    return true;
}

const ConditionParts = struct { field: []const u8, value: []const u8 };

fn parseConditionParts(condition: []const u8, op: []const u8) ?ConditionParts {
    const idx = std.mem.indexOf(u8, condition, op) orelse return null;
    const field = std.mem.trim(u8, condition[0..idx], " ");
    const rhs = std.mem.trim(u8, condition[idx + op.len ..], " ");
    if (rhs.len >= 2 and rhs[0] == '\'' and rhs[rhs.len - 1] == '\'') {
        return ConditionParts{ .field = field, .value = rhs[1 .. rhs.len - 1] };
    }
    return ConditionParts{ .field = field, .value = rhs };
}

fn getContextField(obj: std.json.ObjectMap, field: []const u8) ?[]const u8 {
    const val = obj.get(field) orelse return null;
    return switch (val) {
        .string => |s| s,
        else => null,
    };
}

// ── Approval flow ───────────────────────────────────────────────────────

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

    var pending_key_buf: [256]u8 = undefined;
    const pending_key = try std.fmt.bufPrint(&pending_key_buf, "{s}{s}:{s}", .{
        ec.GATE_PENDING_KEY_PREFIX, zombie_id, action_id,
    });

    // JSON-escape user-supplied fields to prevent injection (RULES.md #23).
    const slack = @import("approval_gate_slack.zig");
    var detail_list: std.ArrayList(u8) = .{};
    defer detail_list.deinit(alloc);
    const dw = detail_list.writer(alloc);
    try dw.writeAll("{\"tool\":\"");
    try slack.writeJsonEscaped(dw, detail.tool);
    try dw.writeAll("\",\"action\":\"");
    try slack.writeJsonEscaped(dw, detail.action);
    try dw.writeAll("\",\"summary\":\"");
    try slack.writeJsonEscaped(dw, detail.params_summary);
    try dw.writeAll("\"}");
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

        const decision = getDecision(redis, response_key) catch {
            log.warn("approval_gate.redis_poll_fail action_id={s}", .{action_id});
            return .timed_out;
        };
        if (decision) |d| return d;

        std.Thread.sleep(poll_interval_ns);
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

/// Write a gate decision to Redis. Wakes the worker loop polling waitForDecision.
/// Used internally by resolve() and by callers that want Redis-only semantics.
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

// ── Channel-agnostic resolve core ──────────────────────────────────────

pub const ResolveOutcome = union(enum) {
    /// First writer — DB row transitioned pending → terminal, Redis decision set.
    resolved: approval_gate_db.ResolvedRow,
    /// Already terminal — caller surfaces 409 with the original outcome + resolver.
    already_resolved: approval_gate_db.ResolvedRow,
    /// No row found for this action_id.
    not_found,
};

/// Single dedup point for gate resolution. Called by:
///   - dashboard handler (`/approvals/{gate_id}:approve|:deny`)
///   - Slack interactive callback (`/webhooks/{zombie_id}/approval`)
///   - Slack legacy interactions handler
///   - approval_gate_sweeper (auto-timeout)
///
/// Atomicity: the DB UPDATE precondition `WHERE status='pending'` plus the
/// schema-level append-only trigger ensure at-most-one resolved transition
/// per action_id. The Redis decision write happens only on the winning path.
/// Losers receive `.already_resolved` carrying the canonical resolver
/// attribution so 409 responses can render "resolved by <X> at <when>".
pub fn resolve(
    pool: *pg.Pool,
    redis: *queue_redis.Client,
    alloc: Allocator,
    action_id: []const u8,
    outcome: GateStatus,
    by: []const u8,
    reason: []const u8,
) !ResolveOutcome {
    if (outcome == .pending) return error.InvalidGateStatus;

    const db_outcome = try approval_gate_db.resolveAtomic(pool, alloc, action_id, outcome, by, reason);
    return switch (db_outcome) {
        .resolved => |row| blk: {
            resolveApproval(redis, action_id, decisionString(outcome)) catch |err| {
                log.warn("approval_gate.resolve_redis_fail action_id={s} err={s}", .{ action_id, @errorName(err) });
            };
            break :blk .{ .resolved = row };
        },
        .already_resolved => |row| .{ .already_resolved = row },
        .not_found => .not_found,
    };
}

fn decisionString(status: GateStatus) []const u8 {
    return switch (status) {
        .approved => ec.GATE_DECISION_APPROVE,
        .denied, .timed_out, .auto_killed => ec.GATE_DECISION_DENY,
        .pending => unreachable,
    };
}

// ── Re-exports ──────────────────────────────────────────────────────────

pub const checkAnomaly = approval_gate_anomaly.checkAnomaly;
pub const AnomalyResult = approval_gate_anomaly.AnomalyResult;
pub const recordGatePending = approval_gate_db.recordGatePending;
pub const resolveGateDecision = approval_gate_db.resolveGateDecision;
pub const buildSlackApprovalMessage = @import("approval_gate_slack.zig").buildSlackApprovalMessage;

test {
    _ = @import("approval_gate_test.zig");
    _ = @import("approval_gate_slack.zig");
    _ = @import("approval_gate_anomaly.zig");
}
