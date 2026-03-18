//! Transition application and state mutation for workspace billing.
//! Applies decided outcomes from workspace_billing_transition.zig to DB and
//! refreshes the in-memory StateRow so callers see consistent state after
//! multiple sequential transitions within one reconcile call.

const std = @import("std");
const pg = @import("pg");
const model = @import("../workspace_billing_model.zig");
const transition = @import("../workspace_billing_transition.zig");
const row_mod = @import("./row.zig");
const db = @import("./db.zig");

const EMPTY_JSON = "{}";

pub fn snapshotFromState(state: row_mod.StateRow) transition.Snapshot {
    return .{
        .plan_tier = state.plan_tier,
        .billing_status = state.billing_status,
        .pending_status = state.pending_status,
        .grace_expires_at = state.grace_expires_at,
    };
}

pub fn viewFromState(alloc: std.mem.Allocator, state: row_mod.StateRow) !model.StateView {
    return .{
        .plan_tier = state.plan_tier,
        .billing_status = state.billing_status,
        .plan_sku = try alloc.dupe(u8, state.plan_sku),
        .subscription_id = if (state.subscription_id) |value| try alloc.dupe(u8, value) else null,
        .grace_expires_at = state.grace_expires_at,
    };
}

pub fn auditEventTypeForLifecycleEvent(event: model.BillingLifecycleEvent) []const u8 {
    return switch (event) {
        .payment_failed => "PAYMENT_FAILED_RECORDED",
        .downgrade_to_free => "DOWNGRADE_REQUESTED",
    };
}

pub fn applyTransitionOutcome(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
    state: *row_mod.StateRow,
    now_ms: i64,
    actor: []const u8,
    outcome: transition.Outcome,
) !void {
    switch (outcome) {
        .activate_scale => try applyScaleActivation(conn, alloc, workspace_id, state.*, now_ms, actor),
        .start_grace => |grace| try applyGracePeriod(conn, alloc, workspace_id, state.*, now_ms, grace.grace_expires_at, actor),
        .downgrade_to_free => |downgrade| try downgradeStateToFree(conn, alloc, workspace_id, state, now_ms, actor, downgrade.reason),
    }
    try refreshStateMutations(state, outcome, now_ms, alloc);
}

fn applyScaleActivation(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
    state: row_mod.StateRow,
    now_ms: i64,
    actor: []const u8,
) !void {
    try db.applyEntitlementPlan(conn, alloc, workspace_id, .scale, now_ms);
    try db.upsertBillingState(conn, alloc, workspace_id, .{
        .plan_tier = .scale,
        .plan_sku = model.SCALE_PLAN_SKU,
        .billing_status = .active,
        .adapter = state.adapter,
        .subscription_id = state.subscription_id,
        .payment_failed_at = null,
        .grace_expires_at = null,
        .pending_status = null,
        .pending_reason = null,
    }, now_ms);
    try db.insertAudit(conn, alloc, workspace_id, "PENDING_SCALE_APPLIED", state.plan_tier, .scale, state.billing_status, .active, "sync_applied_scale_activation", actor, EMPTY_JSON);
}

fn applyGracePeriod(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
    state: row_mod.StateRow,
    now_ms: i64,
    grace_expires_at: i64,
    actor: []const u8,
) !void {
    try db.upsertBillingState(conn, alloc, workspace_id, .{
        .plan_tier = .scale,
        .plan_sku = model.SCALE_PLAN_SKU,
        .billing_status = .grace,
        .adapter = state.adapter,
        .subscription_id = state.subscription_id,
        .payment_failed_at = now_ms,
        .grace_expires_at = grace_expires_at,
        .pending_status = null,
        .pending_reason = null,
    }, now_ms);
    try db.insertAudit(conn, alloc, workspace_id, "PAYMENT_FAILED_GRACE_STARTED", state.plan_tier, .scale, state.billing_status, .grace, "payment_failed", actor, EMPTY_JSON);
}

fn downgradeStateToFree(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
    state: *row_mod.StateRow,
    now_ms: i64,
    actor: []const u8,
    reason: []const u8,
) !void {
    try downgradeToFree(conn, alloc, workspace_id, state.*, now_ms, actor, reason);
}

fn downgradeToFree(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
    state: row_mod.StateRow,
    now_ms: i64,
    actor: []const u8,
    reason: []const u8,
) !void {
    try db.applyEntitlementPlan(conn, alloc, workspace_id, .free, now_ms);
    try db.upsertBillingState(conn, alloc, workspace_id, .{
        .plan_tier = .free,
        .plan_sku = model.FREE_PLAN_SKU,
        .billing_status = .downgraded,
        .adapter = state.adapter,
        .subscription_id = null,
        .payment_failed_at = null,
        .grace_expires_at = null,
        .pending_status = null,
        .pending_reason = null,
    }, now_ms);
    try db.insertAudit(conn, alloc, workspace_id, "DOWNGRADED_TO_FREE", state.plan_tier, .free, state.billing_status, .downgraded, reason, actor, EMPTY_JSON);
}

fn refreshStateMutations(
    state: *row_mod.StateRow,
    outcome: transition.Outcome,
    now_ms: i64,
    alloc: std.mem.Allocator,
) !void {
    state.pending_status = null;
    if (state.pending_reason) |reason| {
        alloc.free(reason);
        state.pending_reason = null;
    }

    switch (outcome) {
        .activate_scale => {
            state.plan_tier = .scale;
            state.billing_status = .active;
            state.payment_failed_at = null;
            state.grace_expires_at = null;
        },
        .start_grace => |grace| {
            state.plan_tier = .scale;
            state.billing_status = .grace;
            state.payment_failed_at = now_ms;
            state.grace_expires_at = grace.grace_expires_at;
        },
        .downgrade_to_free => {
            state.plan_tier = .free;
            state.billing_status = .downgraded;
            state.payment_failed_at = null;
            state.grace_expires_at = null;
            state.clearSubscription(alloc);
        },
    }
}
