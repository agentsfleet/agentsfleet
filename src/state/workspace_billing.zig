//! Workspace billing — thin orchestrator.
//!
//! Public API is preserved; implementation split across focused submodules:
//!   - `workspace_billing/row.zig`   — StateRow, DB reads, parsers
//!   - `workspace_billing/db.zig`    — DB writes, entitlements, audit
//!   - `workspace_billing/apply.zig` — transition application and state mutation

const std = @import("std");
const pg = @import("pg");
const PgQuery = @import("../db/pg_query.zig").PgQuery;
const error_codes = @import("../errors/codes.zig");
const model = @import("./workspace_billing_model.zig");
const transition = @import("./workspace_billing_transition.zig");

const row_mod = @import("./workspace_billing/row.zig");
const db = @import("./workspace_billing/db.zig");
const apply_mod = @import("./workspace_billing/apply.zig");

const log = std.log.scoped(.state);

pub const FREE_PLAN_SKU = model.FREE_PLAN_SKU;
pub const SCALE_PLAN_SKU = model.SCALE_PLAN_SKU;
pub const DEFAULT_GRACE_PERIOD_MS = model.DEFAULT_GRACE_PERIOD_MS;
pub const PlanTier = model.PlanTier;
pub const BillingStatus = model.BillingStatus;
pub const PendingStatus = model.PendingStatus;
pub const BillingLifecycleEvent = model.BillingLifecycleEvent;
pub const StateView = model.StateView;
pub const UpgradeInput = model.UpgradeInput;
pub const BillingLifecycleEventInput = model.BillingLifecycleEventInput;

const EMPTY_JSON = "{}";

pub fn errorCode(err: anyerror) ?[]const u8 {
    return switch (err) {
        error.FreeWorkspaceLimitExceeded => error_codes.ERR_WORKSPACE_FREE_LIMIT,
        error.InvalidSubscriptionId => error_codes.ERR_BILLING_INVALID_SUBSCRIPTION_ID,
        error.WorkspaceBillingStateMissing => error_codes.ERR_BILLING_STATE_MISSING,
        error.InvalidWorkspaceBillingState => error_codes.ERR_BILLING_STATE_INVALID,
        else => null,
    };
}

pub fn errorMessage(err: anyerror) ?[]const u8 {
    return switch (err) {
        error.FreeWorkspaceLimitExceeded => "Free plan is limited to one workspace. Upgrade an existing workspace to Scale before creating another.",
        error.InvalidSubscriptionId => "subscription_id is required",
        error.InvalidBillingEventReason => "billing event reason is required",
        error.WorkspaceBillingStateMissing => "Workspace billing state missing",
        error.InvalidWorkspaceBillingState => "Workspace billing state invalid",
        else => null,
    };
}

pub fn provisionFreeWorkspace(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
    actor: []const u8,
) !void {
    const now_ms = std.time.milliTimestamp();
    try db.applyEntitlementPlan(conn, alloc, workspace_id, .free, now_ms);
    try db.upsertBillingState(conn, alloc, workspace_id, .{
        .plan_tier = .free,
        .plan_sku = FREE_PLAN_SKU,
        .billing_status = .active,
        .adapter = db.configuredAdapterModeLabel(alloc),
        .subscription_id = null,
        .payment_failed_at = null,
        .grace_expires_at = null,
        .pending_status = null,
        .pending_reason = null,
    }, now_ms);
    try db.insertAudit(conn, alloc, workspace_id, "FREE_PROVISIONED", null, .free, null, .active, "workspace_created", actor, EMPTY_JSON);
    log.info("billing.provisioned workspace_id={s} plan=free actor={s}", .{ workspace_id, actor });
}

pub fn enforceFreeWorkspaceCreationAllowed(
    conn: *pg.Conn,
    tenant_id: []const u8,
    exclude_workspace_id: ?[]const u8,
) !void {
    var q = PgQuery.from(try conn.query(
        \\SELECT COUNT(*)::BIGINT
        \\FROM workspaces w
        \\LEFT JOIN workspace_billing_state s ON s.workspace_id = w.workspace_id
        \\WHERE w.tenant_id = $1
        \\  AND ($2::TEXT IS NULL OR w.workspace_id <> $2::uuid)
        \\  AND COALESCE(s.plan_tier, 'FREE') <> 'SCALE'
    , .{ tenant_id, exclude_workspace_id }));
    defer q.deinit();
    const row = (try q.next()) orelse return error.InvalidWorkspaceBillingState;
    const count = try row.get(i64, 0);
    if (count > 0) {
        log.warn("billing.free_workspace_limit_exceeded tenant_id={s} existing_count={d}", .{ tenant_id, count });
        return error.FreeWorkspaceLimitExceeded;
    }
}

pub fn upgradeWorkspaceToScale(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
    input: UpgradeInput,
) !StateView {
    const trimmed_subscription = std.mem.trim(u8, input.subscription_id, " \t\r\n");
    if (trimmed_subscription.len == 0) return error.InvalidSubscriptionId;

    var previous = try ensureStateRow(conn, alloc, workspace_id);
    defer previous.deinit(alloc);

    const now_ms = std.time.milliTimestamp();
    try db.applyEntitlementPlan(conn, alloc, workspace_id, .scale, now_ms);
    try db.upsertBillingState(conn, alloc, workspace_id, .{
        .plan_tier = .scale,
        .plan_sku = SCALE_PLAN_SKU,
        .billing_status = .active,
        .adapter = db.configuredAdapterModeLabel(alloc),
        .subscription_id = trimmed_subscription,
        .payment_failed_at = null,
        .grace_expires_at = null,
        .pending_status = null,
        .pending_reason = null,
    }, now_ms);
    try db.insertAudit(conn, alloc, workspace_id, "SCALE_ACTIVATED", previous.plan_tier, .scale, previous.billing_status, .active, "upgrade_to_scale", input.actor, EMPTY_JSON);
    return .{
        .plan_tier = .scale,
        .billing_status = .active,
        .plan_sku = try alloc.dupe(u8, SCALE_PLAN_SKU),
        .subscription_id = try alloc.dupe(u8, trimmed_subscription),
        .grace_expires_at = null,
    };
}

pub fn reconcileWorkspaceBilling(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
    now_ms: i64,
    actor: []const u8,
) !StateView {
    var state = try ensureStateRow(conn, alloc, workspace_id);
    defer state.deinit(alloc);

    if (transition.decidePending(apply_mod.snapshotFromState(state), now_ms)) |outcome| {
        try apply_mod.applyTransitionOutcome(conn, alloc, workspace_id, &state, now_ms, actor, outcome);
    }

    if (transition.decideGraceExpiry(apply_mod.snapshotFromState(state), now_ms)) |outcome| {
        try apply_mod.applyTransitionOutcome(conn, alloc, workspace_id, &state, now_ms, actor, outcome);
    }

    return try apply_mod.viewFromState(alloc, state);
}

pub fn applyBillingLifecycleEvent(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
    input: BillingLifecycleEventInput,
) !StateView {
    const trimmed_reason = std.mem.trim(u8, input.reason, " \t\r\n");
    if (trimmed_reason.len == 0) return error.InvalidBillingEventReason;

    const pending_status: PendingStatus = switch (input.event) {
        .payment_failed => .payment_failed,
        .downgrade_to_free => .downgrade_to_free,
    };

    var state = try ensureStateRow(conn, alloc, workspace_id);
    defer state.deinit(alloc);

    const now_ms = std.time.milliTimestamp();
    try db.upsertBillingState(conn, alloc, workspace_id, .{
        .plan_tier = state.plan_tier,
        .plan_sku = state.plan_sku,
        .billing_status = state.billing_status,
        .adapter = state.adapter,
        .subscription_id = state.subscription_id,
        .payment_failed_at = state.payment_failed_at,
        .grace_expires_at = state.grace_expires_at,
        .pending_status = pending_status,
        .pending_reason = trimmed_reason,
    }, now_ms);
    try db.insertAudit(
        conn,
        alloc,
        workspace_id,
        apply_mod.auditEventTypeForLifecycleEvent(input.event),
        state.plan_tier,
        state.plan_tier,
        state.billing_status,
        state.billing_status,
        trimmed_reason,
        input.actor,
        EMPTY_JSON,
    );

    return reconcileWorkspaceBilling(conn, alloc, workspace_id, now_ms, input.actor);
}

fn ensureStateRow(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
) !row_mod.StateRow {
    const existing = row_mod.loadStateRow(conn, alloc, workspace_id) catch |err| switch (err) {
        error.WorkspaceBillingStateMissing => null,
        else => return err,
    };
    if (existing) |r| return r;

    try provisionFreeWorkspace(conn, alloc, workspace_id, "system");
    return (try row_mod.loadStateRow(conn, alloc, workspace_id)).?;
}

test {
    _ = @import("./workspace_billing_test.zig");
    _ = @import("./workspace_billing/row.zig");
    _ = @import("./workspace_billing/db.zig");
}
