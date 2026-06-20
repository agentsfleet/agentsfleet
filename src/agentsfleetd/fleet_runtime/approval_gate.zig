// Approval gate — 3D Secure model for high-stakes Fleet actions.
//
// Evaluates tool invocations against a gate policy. Matching actions emit a
// Slack message and park the event as pending; the lease path re-evaluates
// the recorded gate on every poll (approval_gate_async.zig) — no thread ever
// blocks waiting for a human. Anomaly patterns (runaway loops) auto-kill.
//
// Call order: checkAnomaly() → evaluateGate() → requestApproval() → record ref.
// Resolution: dashboard handler OR Slack callback OR sweeper calls resolve(),
// which atomically transitions pending → terminal status; the next lease poll
// observes the stored decision.

const std = @import("std");
const pg = @import("pg");
const Allocator = std.mem.Allocator;
const queue_redis = @import("../queue/redis_client.zig");
const ec = @import("../errors/error_registry.zig");
const id_format = @import("../types/id_format.zig");
const config_gates = @import("config_gates.zig");
const gate_condition = @import("gate_condition.zig");

const approval_gate_db = @import("approval_gate_db.zig");
const approval_gate_anomaly = @import("approval_gate_anomaly.zig");
const approval_gate_slack = @import("approval_gate_slack.zig");
const logging = @import("log");
const MS_PER_SECOND = 1000;

const log = logging.scoped(.approval_gate);

pub const GateDecision = enum { auto_approve, requires_approval, auto_kill };

/// Status values stored in core.fleet_approval_gates.status column.
/// Typed enum — no raw strings for gate status (RULES.md #36).
const S_S_S = "{s}{s}";

pub const GateStatus = enum {
    const Self = @This();

    pending,
    approved,
    denied,
    timed_out,
    auto_killed,

    pub fn toSlice(self: Self) []const u8 {
        return switch (self) {
            .pending => "pending",
            .approved => "approved",
            .denied => "denied",
            .timed_out => "timed_out",
            .auto_killed => "auto_killed",
        };
    }

    pub fn isTerminal(self: Self) bool {
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
    timeout_ms: i64 = 24 * 60 * 60 * MS_PER_SECOND,
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

    if (gate_condition.parse(condition)) |c| {
        const actual = getContextField(obj, c.field) orelse return true;
        const matched = std.mem.eql(u8, actual, c.value);
        return if (c.negate) !matched else matched;
    }

    // Operator-less condition — fail safe (gate fires). Parse-time validation
    // (config_gates via gate_condition.isValid → UZ-APPROVAL-005) now rejects
    // these for stored policies, so in practice only directly-constructed
    // GateRules reach this branch.
    log.warn("invalid_condition", .{ .condition = condition });
    return true;
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
/// Caller then sends the Slack message and records the event→gate ref
/// (approval_gate_async.recordEventGateRef) so later polls can resolve it.
pub fn requestApproval(
    alloc: Allocator,
    redis: *queue_redis.Client,
    fleet_id: []const u8,
    detail: ActionDetail,
) ![]const u8 {
    const action_id = try id_format.generateActivityEventId(alloc);
    errdefer alloc.free(action_id);

    var pending_key_buf: [256]u8 = undefined;
    const pending_key = try std.fmt.bufPrint(&pending_key_buf, "{s}{s}:{s}", .{
        ec.GATE_PENDING_KEY_PREFIX, fleet_id, action_id,
    });

    // JSON-escape user-supplied fields to prevent injection (RULES.md #23).
    const slack = approval_gate_slack;
    var detail_aw: std.Io.Writer.Allocating = .init(alloc);
    defer detail_aw.deinit();
    const dw = &detail_aw.writer;
    try dw.writeAll("{\"tool\":\"");
    try slack.writeJsonEscaped(dw, detail.tool);
    try dw.writeAll("\",\"action\":\"");
    try slack.writeJsonEscaped(dw, detail.action);
    try dw.writeAll("\",\"summary\":\"");
    try slack.writeJsonEscaped(dw, detail.params_summary);
    try dw.writeAll("\"}");
    const detail_json = detail_aw.written();

    try redis.setEx(pending_key, detail_json, ec.GATE_PENDING_TTL_SECONDS);

    log.info("requested", .{
        .fleet_id = fleet_id,
        .action_id = action_id,
        .tool = detail.tool,
        .action = detail.action,
    });

    return action_id;
}

/// Write a gate decision to Redis. The lease path's next poll observes it
/// via approval_gate_async.readDecision.
/// Used internally by resolve() and by callers that want Redis-only semantics.
pub fn resolveApproval(
    redis: *queue_redis.Client,
    action_id: []const u8,
    decision: []const u8,
) !void {
    var response_key_buf: [256]u8 = undefined;
    const response_key = try std.fmt.bufPrint(&response_key_buf, S_S_S, .{
        ec.GATE_RESPONSE_KEY_PREFIX, action_id,
    });
    try redis.setEx(response_key, decision, ec.GATE_PENDING_TTL_SECONDS);

    log.info("resolved", .{ .action_id = action_id, .decision = decision });
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

pub const ResolveArgs = approval_gate_db.ResolveArgs;

/// Single dedup point for gate resolution. Called by:
///   - dashboard handler (`/approvals/{gate_id}:approve|:deny`)
///   - Slack interactive callback (`/webhooks/{fleet_id}/approval`)
///   - Slack interactions handler
///   - approval_gate_sweeper (auto-timeout)
///
/// Atomicity: the DB UPDATE precondition `WHERE status='pending'` plus the
/// schema-level append-only trigger ensure at-most-one resolved transition
/// per action_id. The Redis decision write happens only on the winning path.
/// Losers receive `.already_resolved` carrying the canonical resolver
/// attribution so 409 responses can render "resolved by <X> at <when>".
///
/// See `ResolveArgs.fleet_id_filter` for the cross-fleet defense channels
/// that parse `action_id` from an untrusted URL/payload must enable.
pub fn resolve(
    pool: *pg.Pool,
    redis: *queue_redis.Client,
    alloc: Allocator,
    args: ResolveArgs,
) !ResolveOutcome {
    if (args.outcome == .pending) return error.InvalidGateStatus;

    const db_outcome = try args.atomic(pool, alloc);
    return switch (db_outcome) {
        .resolved => |row| blk: {
            resolveApproval(redis, args.action_id, decisionString(args.outcome)) catch |err| {
                log.warn("resolve_redis_fail", .{ .action_id = args.action_id, .err = @errorName(err) });
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
pub const buildSlackApprovalMessage = approval_gate_slack.buildSlackApprovalMessage;

test {
    _ = @import("approval_gate_test.zig");
    _ = @import("approval_gate_pins_test.zig");
    _ = approval_gate_slack;
    _ = approval_gate_anomaly;
}
