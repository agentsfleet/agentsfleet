//! StateRow type, DB reads, and parser helpers for workspace billing.

const std = @import("std");
const pg = @import("pg");
const PgQuery = @import("../../db/pg_query.zig").PgQuery;
const model = @import("../workspace_billing_model.zig");

pub const PlanTier = model.PlanTier;
pub const BillingStatus = model.BillingStatus;
pub const PendingStatus = model.PendingStatus;

pub const StateRow = struct {
    plan_tier: model.PlanTier,
    billing_status: model.BillingStatus,
    plan_sku: []u8,
    adapter: []u8,
    subscription_id: ?[]u8,
    payment_failed_at: ?i64,
    grace_expires_at: ?i64,
    pending_status: ?model.PendingStatus,
    pending_reason: ?[]u8,

    pub fn deinit(self: *StateRow, alloc: std.mem.Allocator) void {
        alloc.free(self.plan_sku);
        alloc.free(self.adapter);
        if (self.subscription_id) |v| alloc.free(v);
        if (self.pending_reason) |v| alloc.free(v);
    }

    pub fn clearSubscription(self: *StateRow, alloc: std.mem.Allocator) void {
        if (self.subscription_id) |value| {
            alloc.free(value);
            self.subscription_id = null;
        }
    }
};

pub fn parsePlanTier(raw: []const u8) ?model.PlanTier {
    if (std.ascii.eqlIgnoreCase(raw, "FREE")) return .free;
    if (std.ascii.eqlIgnoreCase(raw, "SCALE")) return .scale;
    return null;
}

pub fn parseBillingStatus(raw: []const u8) ?model.BillingStatus {
    if (std.ascii.eqlIgnoreCase(raw, "ACTIVE")) return .active;
    if (std.ascii.eqlIgnoreCase(raw, "GRACE")) return .grace;
    if (std.ascii.eqlIgnoreCase(raw, "DOWNGRADED")) return .downgraded;
    return null;
}

pub fn parsePendingStatus(raw: []const u8) ?model.PendingStatus {
    if (std.ascii.eqlIgnoreCase(raw, "ACTIVATE_SCALE")) return .activate_scale;
    if (std.ascii.eqlIgnoreCase(raw, "PAYMENT_FAILED")) return .payment_failed;
    if (std.ascii.eqlIgnoreCase(raw, "DOWNGRADE_TO_FREE")) return .downgrade_to_free;
    return null;
}

pub fn loadStateRow(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
) !?StateRow {
    var q = PgQuery.from(try conn.query(
        \\SELECT plan_tier, billing_status, plan_sku, adapter, subscription_id, payment_failed_at,
        \\       grace_expires_at, pending_status, pending_reason
        \\FROM workspace_billing_state
        \\WHERE workspace_id = $1
        \\LIMIT 1
    , .{workspace_id}));
    defer q.deinit();
    const row = (try q.next()) orelse return error.WorkspaceBillingStateMissing;
    const plan_tier_raw = try row.get([]const u8, 0);
    const billing_status_raw = try row.get([]const u8, 1);
    const pending_status_raw = try row.get(?[]const u8, 7);
    const plan_tier = parsePlanTier(plan_tier_raw) orelse return error.InvalidWorkspaceBillingState;
    const billing_status = parseBillingStatus(billing_status_raw) orelse return error.InvalidWorkspaceBillingState;
    const pending_status = if (pending_status_raw) |v| (parsePendingStatus(v) orelse return error.InvalidWorkspaceBillingState) else null;
    const plan_sku = try alloc.dupe(u8, try row.get([]const u8, 2));
    errdefer alloc.free(plan_sku);
    const adapter = try alloc.dupe(u8, try row.get([]const u8, 3));
    errdefer alloc.free(adapter);
    const subscription_id = if (try row.get(?[]const u8, 4)) |v| try alloc.dupe(u8, v) else null;
    errdefer if (subscription_id) |v| alloc.free(v);
    const payment_failed_at = try row.get(?i64, 5);
    const grace_expires_at = try row.get(?i64, 6);
    const pending_reason = if (try row.get(?[]const u8, 8)) |v| try alloc.dupe(u8, v) else null;

    return .{
        .plan_tier = plan_tier,
        .billing_status = billing_status,
        .plan_sku = plan_sku,
        .adapter = adapter,
        .subscription_id = subscription_id,
        .payment_failed_at = payment_failed_at,
        .grace_expires_at = grace_expires_at,
        .pending_status = pending_status,
        .pending_reason = pending_reason,
    };
}

test "parsePlanTier case-insensitive happy path" {
    try std.testing.expectEqual(model.PlanTier.free, parsePlanTier("free").?);
    try std.testing.expectEqual(model.PlanTier.free, parsePlanTier("FREE").?);
    try std.testing.expectEqual(model.PlanTier.free, parsePlanTier("Free").?);
    try std.testing.expectEqual(model.PlanTier.scale, parsePlanTier("SCALE").?);
    try std.testing.expectEqual(model.PlanTier.scale, parsePlanTier("scale").?);
}

test "parsePlanTier boundary single-char returns null" {
    try std.testing.expect(parsePlanTier("F") == null);
    try std.testing.expect(parsePlanTier("S") == null);
    try std.testing.expect(parsePlanTier("") == null);
}

test "parsePlanTier invalid returns null" {
    try std.testing.expect(parsePlanTier("INVALID") == null);
    try std.testing.expect(parsePlanTier("premium") == null);
}

test "parseBillingStatus unknown returns null" {
    try std.testing.expect(parseBillingStatus("UNKNOWN") == null);
    try std.testing.expect(parseBillingStatus("pending") == null);
    try std.testing.expect(parseBillingStatus("") == null);
}

test "parsePendingStatus bad value returns null" {
    try std.testing.expect(parsePendingStatus("bad") == null);
    try std.testing.expect(parsePendingStatus("INVALID") == null);
    try std.testing.expect(parsePendingStatus("") == null);
}
