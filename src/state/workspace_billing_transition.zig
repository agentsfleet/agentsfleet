const std = @import("std");
const model = @import("./workspace_billing_model.zig");

pub const Snapshot = struct {
    plan_tier: model.PlanTier,
    billing_status: model.BillingStatus,
    pending_status: ?model.PendingStatus,
    grace_expires_at: ?i64,
};

pub const Outcome = union(enum) {
    activate_scale,
    start_grace: struct { grace_expires_at: i64 },
    downgrade_to_free: struct { reason: []const u8 },
};

pub fn decidePending(snapshot: Snapshot, now_ms: i64) ?Outcome {
    const pending = snapshot.pending_status orelse return null;
    return switch (pending) {
        .activate_scale => .activate_scale,
        .payment_failed => if (snapshot.plan_tier == .scale and snapshot.billing_status != .grace)
            .{ .start_grace = .{ .grace_expires_at = now_ms + model.DEFAULT_GRACE_PERIOD_MS } }
        else
            null,
        .downgrade_to_free => .{ .downgrade_to_free = .{ .reason = "forced_downgrade" } },
    };
}

pub fn decideGraceExpiry(snapshot: Snapshot, now_ms: i64) ?Outcome {
    if (snapshot.plan_tier != .scale) return null;
    if (snapshot.billing_status != .grace) return null;
    const grace_expires_at = snapshot.grace_expires_at orelse return null;
    if (now_ms < grace_expires_at) return null;
    return .{ .downgrade_to_free = .{ .reason = "grace_expired" } };
}

test "payment failure starts grace only for active scale workspaces" {
    const outcome = decidePending(.{
        .plan_tier = .scale,
        .billing_status = .active,
        .pending_status = .payment_failed,
        .grace_expires_at = null,
    }, 100) orelse return error.TestExpectedEqual;

    switch (outcome) {
        .start_grace => |grace| try std.testing.expectEqual(@as(i64, 100 + model.DEFAULT_GRACE_PERIOD_MS), grace.grace_expires_at),
        else => return error.TestExpectedEqual,
    }
}

test "payment failure is ignored once workspace is already in grace" {
    try std.testing.expect(decidePending(.{
        .plan_tier = .scale,
        .billing_status = .grace,
        .pending_status = .payment_failed,
        .grace_expires_at = 123,
    }, 100) == null);
}

test "grace expiry downgrades only after the expiry threshold" {
    try std.testing.expect(decideGraceExpiry(.{
        .plan_tier = .scale,
        .billing_status = .grace,
        .pending_status = null,
        .grace_expires_at = 200,
    }, 199) == null);

    const outcome = decideGraceExpiry(.{
        .plan_tier = .scale,
        .billing_status = .grace,
        .pending_status = null,
        .grace_expires_at = 200,
    }, 200) orelse return error.TestExpectedEqual;

    switch (outcome) {
        .downgrade_to_free => |downgrade| try std.testing.expectEqualStrings("grace_expired", downgrade.reason),
        else => return error.TestExpectedEqual,
    }
}
