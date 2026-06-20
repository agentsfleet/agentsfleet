//! Async approval-gate primitives: the event→gate reference plus decision
//! read that let the lease path re-evaluate a pending gate on every poll
//! instead of parking a handler thread (the deleted blocking wait).
//!
//! Key shapes (prefixes owned by the error registry):
//!   byevent ref: fleet:gate:byevent:{fleet_id}:{event_id} → "action_id|deadline_ms"
//!   decision:    fleet:gate:response:{action_id}           → approve | deny

const std = @import("std");
const clock = @import("common").clock;
const queue_redis = @import("../queue/redis_client.zig");
const ec = @import("../errors/error_registry.zig");

/// Action ids are UUIDv7 strings (id_format.allocUuidV7).
const ACTION_ID_LEN: usize = 36;
const REF_SEPARATOR = "|";
const REF_KEY_FMT = "{s}{s}:{s}";
const S_GET = "GET";
/// The byevent ref must outlive the gate deadline so a re-encountered event
/// can still resolve; grace covers sweeper cadence + clock slop.
const REF_TTL_GRACE_SECONDS: i64 = 600;

/// Decisions resolveApproval ever stores — timeouts resolve to deny.
pub const StoredDecision = enum { approved, denied };

pub const EventGateRef = struct {
    const Self = @This();

    // SAFETY: parseRef fills the prefix it reports via action_id_len before
    // the struct is returned; readers go through actionId().
    action_id_buf: [ACTION_ID_LEN]u8 = undefined,
    action_id_len: u8 = 0,
    deadline_ms: i64 = 0,

    pub fn actionId(self: *const Self) []const u8 {
        return self.action_id_buf[0..self.action_id_len];
    }
};

/// One lease-poll evaluation outcome for a recorded gate.
pub const PendingEval = union(enum) {
    approved,
    denied,
    /// No decision, deadline ahead — keep answering no-work.
    pending,
    /// No decision, deadline passed — caller resolves timed_out.
    expired,
};

pub fn recordEventGateRef(
    redis: *queue_redis.Client,
    fleet_id: []const u8,
    event_id: []const u8,
    action_id: []const u8,
    deadline_ms: i64,
) !void {
    var key_buf: [256]u8 = undefined;
    const key = try std.fmt.bufPrint(&key_buf, REF_KEY_FMT, .{ ec.GATE_EVENT_REF_KEY_PREFIX, fleet_id, event_id });
    var val_buf: [64]u8 = undefined;
    const val = try std.fmt.bufPrint(&val_buf, "{s}" ++ REF_SEPARATOR ++ "{d}", .{ action_id, deadline_ms });
    const remaining_s = @divTrunc(deadline_ms - clock.nowMillis(), std.time.ms_per_s) + REF_TTL_GRACE_SECONDS;
    const ttl: u32 = @intCast(@max(@as(i64, ec.GATE_PENDING_TTL_SECONDS), remaining_s));
    try redis.setEx(key, val, ttl);
}

pub fn lookupEventGateRef(
    redis: *queue_redis.Client,
    fleet_id: []const u8,
    event_id: []const u8,
) !?EventGateRef {
    var key_buf: [256]u8 = undefined;
    const key = try std.fmt.bufPrint(&key_buf, REF_KEY_FMT, .{ ec.GATE_EVENT_REF_KEY_PREFIX, fleet_id, event_id });
    var resp = try redis.commandAllowError(&.{ S_GET, key });
    defer resp.deinit(redis.alloc);
    const raw = switch (resp) {
        .bulk => |b| b orelse return null,
        .simple => |s| s,
        else => return null,
    };
    return parseRef(raw);
}

fn parseRef(raw: []const u8) ?EventGateRef {
    const sep = std.mem.indexOf(u8, raw, REF_SEPARATOR) orelse return null;
    const action_id = raw[0..sep];
    if (action_id.len == 0 or action_id.len > ACTION_ID_LEN) return null;
    const deadline_ms = std.fmt.parseInt(i64, raw[sep + REF_SEPARATOR.len ..], 10) catch return null;
    var ref = EventGateRef{ .action_id_len = @intCast(action_id.len), .deadline_ms = deadline_ms };
    @memcpy(ref.action_id_buf[0..action_id.len], action_id);
    return ref;
}

pub fn readDecision(redis: *queue_redis.Client, action_id: []const u8) !?StoredDecision {
    var key_buf: [256]u8 = undefined;
    const key = try std.fmt.bufPrint(&key_buf, "{s}{s}", .{ ec.GATE_RESPONSE_KEY_PREFIX, action_id });
    var resp = try redis.commandAllowError(&.{ S_GET, key });
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

/// One lease-poll evaluation of a recorded gate.
pub fn evaluateRef(redis: *queue_redis.Client, ref: *const EventGateRef, now_ms: i64) !PendingEval {
    if (try readDecision(redis, ref.actionId())) |decision| {
        return switch (decision) {
            .approved => .approved,
            .denied => .denied,
        };
    }
    if (now_ms > ref.deadline_ms) return .expired;
    return .pending;
}

// ── In-file parse tests (no Redis needed) ───────────────────────────────

test "parseRef: round-trips action_id and deadline" {
    const ref = parseRef("0193e9a0-0000-7000-8000-00000000aaaa|1765000000000").?;
    try std.testing.expectEqualStrings("0193e9a0-0000-7000-8000-00000000aaaa", ref.actionId());
    try std.testing.expectEqual(@as(i64, 1_765_000_000_000), ref.deadline_ms);
}

test "parseRef: rejects malformed values" {
    try std.testing.expect(parseRef("") == null);
    try std.testing.expect(parseRef("no-separator") == null);
    try std.testing.expect(parseRef("|123") == null);
    try std.testing.expect(parseRef("id|not-a-number") == null);
    try std.testing.expect(parseRef("this-action-id-is-way-too-long-to-be-a-uuid-string|1") == null);
}

test {
    _ = @import("approval_gate_async_test.zig");
}
