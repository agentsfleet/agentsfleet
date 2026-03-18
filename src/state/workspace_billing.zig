const std = @import("std");
const pg = @import("pg");
const entitlements = @import("./entitlements.zig");
const error_codes = @import("../errors/codes.zig");
const id_format = @import("../types/id_format.zig");
const model = @import("./workspace_billing_model.zig");
const transition = @import("./workspace_billing_transition.zig");

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
const DEFAULT_AGENT_SCORING_WEIGHTS_JSON = "{\"completion\":0.4,\"error_rate\":0.3,\"latency\":0.2,\"resource\":0.1}";

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

const StateRow = struct {
    plan_tier: PlanTier,
    billing_status: BillingStatus,
    plan_sku: []u8,
    adapter: []u8,
    subscription_id: ?[]u8,
    payment_failed_at: ?i64,
    grace_expires_at: ?i64,
    pending_status: ?PendingStatus,
    pending_reason: ?[]u8,

    fn deinit(self: *StateRow, alloc: std.mem.Allocator) void {
        alloc.free(self.plan_sku);
        alloc.free(self.adapter);
        if (self.subscription_id) |v| alloc.free(v);
        if (self.pending_reason) |v| alloc.free(v);
    }
    fn clearSubscription(self: *StateRow, alloc: std.mem.Allocator) void {
        if (self.subscription_id) |value| {
            alloc.free(value);
            self.subscription_id = null;
        }
    }
};

pub fn provisionFreeWorkspace(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
    actor: []const u8,
) !void {
    const now_ms = std.time.milliTimestamp();
    try applyEntitlementPlan(conn, alloc, workspace_id, .free, now_ms);
    try upsertBillingState(conn, alloc, workspace_id, .{
        .plan_tier = .free,
        .plan_sku = FREE_PLAN_SKU,
        .billing_status = .active,
        .adapter = configuredAdapterModeLabel(alloc),
        .subscription_id = null,
        .payment_failed_at = null,
        .grace_expires_at = null,
        .pending_status = null,
        .pending_reason = null,
    }, now_ms);
    try insertAudit(conn, alloc, workspace_id, "FREE_PROVISIONED", null, .free, null, .active, "workspace_created", actor, EMPTY_JSON);
}

pub fn enforceFreeWorkspaceCreationAllowed(
    conn: *pg.Conn,
    tenant_id: []const u8,
    exclude_workspace_id: ?[]const u8,
) !void {
    var q = try conn.query(
        \\SELECT COUNT(*)::BIGINT
        \\FROM workspaces w
        \\LEFT JOIN workspace_billing_state s ON s.workspace_id = w.workspace_id
        \\WHERE w.tenant_id = $1
        \\  AND ($2::TEXT IS NULL OR w.workspace_id <> $2)
        \\  AND COALESCE(s.plan_tier, 'FREE') <> 'SCALE'
    , .{ tenant_id, exclude_workspace_id });
    defer q.deinit();
    const row = (try q.next()) orelse return error.InvalidWorkspaceBillingState;
    const count = try row.get(i64, 0);
    try q.drain();
    if (count > 0) return error.FreeWorkspaceLimitExceeded;
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
    try applyEntitlementPlan(conn, alloc, workspace_id, .scale, now_ms);
    try upsertBillingState(conn, alloc, workspace_id, .{
        .plan_tier = .scale,
        .plan_sku = SCALE_PLAN_SKU,
        .billing_status = .active,
        .adapter = configuredAdapterModeLabel(alloc),
        .subscription_id = trimmed_subscription,
        .payment_failed_at = null,
        .grace_expires_at = null,
        .pending_status = null,
        .pending_reason = null,
    }, now_ms);
    try insertAudit(conn, alloc, workspace_id, "SCALE_ACTIVATED", previous.plan_tier, .scale, previous.billing_status, .active, "upgrade_to_scale", input.actor, EMPTY_JSON);
    return .{
        .plan_tier = .scale,
        .billing_status = .active,
        .plan_sku = SCALE_PLAN_SKU,
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

    if (transition.decidePending(snapshotFromState(state), now_ms)) |outcome| {
        try applyTransitionOutcome(conn, alloc, workspace_id, &state, now_ms, actor, outcome);
    }

    if (transition.decideGraceExpiry(snapshotFromState(state), now_ms)) |outcome| {
        try applyTransitionOutcome(conn, alloc, workspace_id, &state, now_ms, actor, outcome);
    }

    return try viewFromState(alloc, state);
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
    try upsertBillingState(conn, alloc, workspace_id, .{
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
    try insertAudit(
        conn,
        alloc,
        workspace_id,
        auditEventTypeForLifecycleEvent(input.event),
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

fn auditEventTypeForLifecycleEvent(event: BillingLifecycleEvent) []const u8 {
    return switch (event) {
        .payment_failed => "PAYMENT_FAILED_RECORDED",
        .downgrade_to_free => "DOWNGRADE_REQUESTED",
    };
}

fn snapshotFromState(state: StateRow) transition.Snapshot {
    return .{
        .plan_tier = state.plan_tier,
        .billing_status = state.billing_status,
        .pending_status = state.pending_status,
        .grace_expires_at = state.grace_expires_at,
    };
}

fn viewFromState(alloc: std.mem.Allocator, state: StateRow) !StateView {
    return .{
        .plan_tier = state.plan_tier,
        .billing_status = state.billing_status,
        .plan_sku = try alloc.dupe(u8, state.plan_sku),
        .subscription_id = if (state.subscription_id) |value| try alloc.dupe(u8, value) else null,
        .grace_expires_at = state.grace_expires_at,
    };
}

fn applyTransitionOutcome(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
    state: *StateRow,
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
    state: StateRow,
    now_ms: i64,
    actor: []const u8,
) !void {
    try applyEntitlementPlan(conn, alloc, workspace_id, .scale, now_ms);
    try upsertBillingState(conn, alloc, workspace_id, .{
        .plan_tier = .scale,
        .plan_sku = SCALE_PLAN_SKU,
        .billing_status = .active,
        .adapter = state.adapter,
        .subscription_id = state.subscription_id,
        .payment_failed_at = null,
        .grace_expires_at = null,
        .pending_status = null,
        .pending_reason = null,
    }, now_ms);
    try insertAudit(conn, alloc, workspace_id, "PENDING_SCALE_APPLIED", state.plan_tier, .scale, state.billing_status, .active, "sync_applied_scale_activation", actor, EMPTY_JSON);
}

fn applyGracePeriod(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
    state: StateRow,
    now_ms: i64,
    grace_expires_at: i64,
    actor: []const u8,
) !void {
    try upsertBillingState(conn, alloc, workspace_id, .{
        .plan_tier = .scale,
        .plan_sku = SCALE_PLAN_SKU,
        .billing_status = .grace,
        .adapter = state.adapter,
        .subscription_id = state.subscription_id,
        .payment_failed_at = now_ms,
        .grace_expires_at = grace_expires_at,
        .pending_status = null,
        .pending_reason = null,
    }, now_ms);
    try insertAudit(conn, alloc, workspace_id, "PAYMENT_FAILED_GRACE_STARTED", state.plan_tier, .scale, state.billing_status, .grace, "payment_failed", actor, EMPTY_JSON);
}

fn downgradeStateToFree(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
    state: *StateRow,
    now_ms: i64,
    actor: []const u8,
    reason: []const u8,
) !void {
    try downgradeToFree(conn, alloc, workspace_id, state.*, now_ms, actor, reason);
}

fn refreshStateMutations(
    state: *StateRow,
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

fn downgradeToFree(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
    state: StateRow,
    now_ms: i64,
    actor: []const u8,
    reason: []const u8,
) !void {
    try applyEntitlementPlan(conn, alloc, workspace_id, .free, now_ms);
    try upsertBillingState(conn, alloc, workspace_id, .{
        .plan_tier = .free,
        .plan_sku = FREE_PLAN_SKU,
        .billing_status = .downgraded,
        .adapter = state.adapter,
        .subscription_id = null,
        .payment_failed_at = null,
        .grace_expires_at = null,
        .pending_status = null,
        .pending_reason = null,
    }, now_ms);
    try insertAudit(conn, alloc, workspace_id, "DOWNGRADED_TO_FREE", state.plan_tier, .free, state.billing_status, .downgraded, reason, actor, EMPTY_JSON);
}

fn configuredAdapterModeLabel(alloc: std.mem.Allocator) []const u8 {
    const raw = std.process.getEnvVarOwned(alloc, "BILLING_ADAPTER_MODE") catch return "noop";
    defer alloc.free(raw);
    const trimmed = std.mem.trim(u8, raw, " \t\r\n");
    if (std.mem.eql(u8, trimmed, "manual")) return "manual";
    if (std.mem.eql(u8, trimmed, "provider_stub")) return "provider_stub";
    return "noop";
}

fn applyEntitlementPlan(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
    tier: PlanTier,
    now_ms: i64,
) !void {
    const entitlement_id = try id_format.generateEntitlementSnapshotId(alloc);
    defer alloc.free(entitlement_id);
    const policy = entitlementForTier(tier);
    _ = try conn.exec(
        \\INSERT INTO workspace_entitlements
        \\  (entitlement_id, workspace_id, plan_tier, max_profiles, max_stages, max_distinct_skills,
        \\   allow_custom_skills, enable_agent_scoring, agent_scoring_weights_json, created_at, updated_at)
        \\VALUES ($1::uuid, $2, $3, $4, $5, $6, $7, false, $8, $9, $9)
        \\ON CONFLICT (workspace_id) DO UPDATE
        \\SET plan_tier = EXCLUDED.plan_tier,
        \\    max_profiles = EXCLUDED.max_profiles,
        \\    max_stages = EXCLUDED.max_stages,
        \\    max_distinct_skills = EXCLUDED.max_distinct_skills,
        \\    allow_custom_skills = EXCLUDED.allow_custom_skills,
        \\    enable_agent_scoring = EXCLUDED.enable_agent_scoring,
        \\    agent_scoring_weights_json = EXCLUDED.agent_scoring_weights_json,
        \\    updated_at = EXCLUDED.updated_at
    , .{
        entitlement_id,
        workspace_id,
        tier.label(),
        @as(i32, policy.max_profiles),
        @as(i32, policy.max_stages),
        @as(i32, policy.max_distinct_skills),
        policy.allow_custom_skills,
        DEFAULT_AGENT_SCORING_WEIGHTS_JSON,
        now_ms,
    });
}

fn entitlementForTier(tier: PlanTier) entitlements.EntitlementPolicy {
    return switch (tier) {
        .free => .{
            .tier = .free,
            .max_profiles = 1,
            .max_stages = 3,
            .max_distinct_skills = 3,
            .allow_custom_skills = false,
        },
        .scale => .{
            .tier = .scale,
            .max_profiles = 8,
            .max_stages = 8,
            .max_distinct_skills = 16,
            .allow_custom_skills = true,
        },
    };
}

fn ensureStateRow(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
) !StateRow {
    const existing = loadStateRow(conn, alloc, workspace_id) catch |err| switch (err) {
        error.WorkspaceBillingStateMissing => null,
        else => return err,
    };
    if (existing) |row| return row;

    try provisionFreeWorkspace(conn, alloc, workspace_id, "system");
    return (try loadStateRow(conn, alloc, workspace_id)).?;
}

fn loadStateRow(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
) !?StateRow {
    var q = try conn.query(
        \\SELECT plan_tier, billing_status, plan_sku, adapter, subscription_id, payment_failed_at,
        \\       grace_expires_at, pending_status, pending_reason
        \\FROM workspace_billing_state
        \\WHERE workspace_id = $1
        \\LIMIT 1
    , .{workspace_id});
    defer q.deinit();
    const row = (try q.next()) orelse return error.WorkspaceBillingStateMissing;
    const plan_tier_raw = try row.get([]const u8, 0);
    const billing_status_raw = try row.get([]const u8, 1);
    const plan_sku = try alloc.dupe(u8, try row.get([]const u8, 2));
    errdefer alloc.free(plan_sku);
    const adapter = try alloc.dupe(u8, try row.get([]const u8, 3));
    errdefer alloc.free(adapter);
    const subscription_id = if (try row.get(?[]const u8, 4)) |v| try alloc.dupe(u8, v) else null;
    errdefer if (subscription_id) |v| alloc.free(v);
    const payment_failed_at = try row.get(?i64, 5);
    const grace_expires_at = try row.get(?i64, 6);
    const pending_status_raw = try row.get(?[]const u8, 7);
    const pending_reason = if (try row.get(?[]const u8, 8)) |v| try alloc.dupe(u8, v) else null;
    errdefer if (pending_reason) |v| alloc.free(v);
    const plan_tier = parsePlanTier(plan_tier_raw) orelse {
        try q.drain();
        return error.InvalidWorkspaceBillingState;
    };
    const billing_status = parseBillingStatus(billing_status_raw) orelse {
        try q.drain();
        return error.InvalidWorkspaceBillingState;
    };
    const pending_status = if (pending_status_raw) |v| (parsePendingStatus(v) orelse {
        try q.drain();
        return error.InvalidWorkspaceBillingState;
    }) else null;
    try q.drain();
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

fn upsertBillingState(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
    state: struct {
        plan_tier: PlanTier,
        plan_sku: []const u8,
        billing_status: BillingStatus,
        adapter: []const u8,
        subscription_id: ?[]const u8,
        payment_failed_at: ?i64,
        grace_expires_at: ?i64,
        pending_status: ?PendingStatus,
        pending_reason: ?[]const u8,
    },
    now_ms: i64,
) !void {
    const billing_id = try id_format.generateEntitlementSnapshotId(alloc);
    defer alloc.free(billing_id);
    _ = try conn.exec(
        \\INSERT INTO workspace_billing_state
        \\  (billing_id, workspace_id, plan_tier, plan_sku, billing_status, adapter, subscription_id,
        \\   payment_failed_at, grace_expires_at, pending_status, pending_reason, created_at, updated_at)
        \\VALUES ($1::uuid, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $12)
        \\ON CONFLICT (workspace_id) DO UPDATE
        \\SET plan_tier = EXCLUDED.plan_tier,
        \\    plan_sku = EXCLUDED.plan_sku,
        \\    billing_status = EXCLUDED.billing_status,
        \\    adapter = EXCLUDED.adapter,
        \\    subscription_id = EXCLUDED.subscription_id,
        \\    payment_failed_at = EXCLUDED.payment_failed_at,
        \\    grace_expires_at = EXCLUDED.grace_expires_at,
        \\    pending_status = EXCLUDED.pending_status,
        \\    pending_reason = EXCLUDED.pending_reason,
        \\    updated_at = EXCLUDED.updated_at
    , .{
        billing_id,
        workspace_id,
        state.plan_tier.label(),
        state.plan_sku,
        state.billing_status.label(),
        state.adapter,
        state.subscription_id,
        state.payment_failed_at,
        state.grace_expires_at,
        if (state.pending_status) |v| v.label() else null,
        state.pending_reason,
        now_ms,
    });
}

fn insertAudit(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
    event_type: []const u8,
    previous_plan_tier: ?PlanTier,
    new_plan_tier: PlanTier,
    previous_status: ?BillingStatus,
    new_status: BillingStatus,
    reason: []const u8,
    actor: []const u8,
    metadata_json: []const u8,
) !void {
    const audit_id = try id_format.generateEntitlementSnapshotId(alloc);
    defer alloc.free(audit_id);
    _ = try conn.exec(
        \\INSERT INTO workspace_billing_audit
        \\  (audit_id, workspace_id, event_type, previous_plan_tier, new_plan_tier, previous_status, new_status, reason, actor, metadata_json, created_at)
        \\VALUES ($1::uuid, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
    , .{
        audit_id,
        workspace_id,
        event_type,
        if (previous_plan_tier) |v| v.label() else null,
        new_plan_tier.label(),
        if (previous_status) |v| v.label() else null,
        new_status.label(),
        reason,
        actor,
        metadata_json,
        std.time.milliTimestamp(),
    });
}

fn parsePlanTier(raw: []const u8) ?PlanTier {
    if (std.ascii.eqlIgnoreCase(raw, "FREE")) return .free;
    if (std.ascii.eqlIgnoreCase(raw, "SCALE")) return .scale;
    return null;
}

fn parseBillingStatus(raw: []const u8) ?BillingStatus {
    if (std.ascii.eqlIgnoreCase(raw, "ACTIVE")) return .active;
    if (std.ascii.eqlIgnoreCase(raw, "GRACE")) return .grace;
    if (std.ascii.eqlIgnoreCase(raw, "DOWNGRADED")) return .downgraded;
    return null;
}

fn parsePendingStatus(raw: []const u8) ?PendingStatus {
    if (std.ascii.eqlIgnoreCase(raw, "ACTIVATE_SCALE")) return .activate_scale;
    if (std.ascii.eqlIgnoreCase(raw, "PAYMENT_FAILED")) return .payment_failed;
    if (std.ascii.eqlIgnoreCase(raw, "DOWNGRADE_TO_FREE")) return .downgrade_to_free;
    return null;
}

test {
    _ = @import("./workspace_billing_test.zig");
}
