const std = @import("std");
const zap = @import("zap");
const pg = @import("pg");
const common = @import("handlers/common.zig");
const error_codes = @import("../errors/codes.zig");
const workspace_billing = @import("../state/workspace_billing.zig");
const workspace_credit = @import("../state/workspace_credit.zig");

pub const CreditPolicy = enum {
    none,
    execution_required,
};

pub const Requirement = struct {
    minimum_role: common.AuthRole = .user,
    credit_policy: CreditPolicy = .none,
};

pub const Access = struct {
    credit: ?workspace_credit.CreditView = null,
    billing_state: ?workspace_billing.StateView = null,

    pub fn deinit(self: Access, alloc: std.mem.Allocator) void {
        if (self.credit) |value| alloc.free(value.currency);
        if (self.billing_state) |bs| {
            alloc.free(bs.plan_sku);
            if (bs.subscription_id) |v| alloc.free(v);
        }
    }
};

fn authorizeWorkspace(
    r: zap.Request,
    req_id: []const u8,
    conn: *pg.Conn,
    principal: common.AuthPrincipal,
    workspace_id: []const u8,
) bool {
    if (common.authorizeWorkspaceAndSetTenantContext(conn, principal, workspace_id)) return true;
    common.errorResponse(r, .forbidden, error_codes.ERR_FORBIDDEN, "Workspace access denied", req_id);
    return false;
}

const ExecutionResult = struct {
    credit: workspace_credit.CreditView,
    billing_state: workspace_billing.StateView,
};

fn requireExecutionAllowed(
    r: zap.Request,
    req_id: []const u8,
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
    actor: []const u8,
) ?ExecutionResult {
    const billing_state = workspace_billing.reconcileWorkspaceBilling(conn, alloc, workspace_id, std.time.milliTimestamp(), actor) catch |err| {
        if (workspace_billing.errorCode(err)) |code| {
            common.errorResponse(r, .internal_server_error, code, workspace_billing.errorMessage(err) orelse "Workspace billing failure", req_id);
            return null;
        }
        common.internalOperationError(r, "Failed to reconcile workspace billing state", req_id);
        return null;
    };

    const credit = workspace_credit.enforceExecutionAllowed(conn, alloc, workspace_id, billing_state.plan_tier) catch |err| {
        alloc.free(billing_state.plan_sku);
        if (billing_state.subscription_id) |v| alloc.free(v);
        if (workspace_credit.errorCode(err)) |code| {
            common.errorResponse(r, .forbidden, code, workspace_credit.errorMessage(err) orelse "Workspace credit failure", req_id);
            return null;
        }
        common.internalOperationError(r, "Failed to validate workspace credit balance", req_id);
        return null;
    };

    return .{ .credit = credit, .billing_state = billing_state };
}

pub fn enforce(
    r: zap.Request,
    req_id: []const u8,
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    principal: common.AuthPrincipal,
    workspace_id: []const u8,
    actor: []const u8,
    requirement: Requirement,
) ?Access {
    // Role check is stateless — run it before workspace auth to avoid DB
    // side-effects (SET LOCAL tenant context) on connections returned to the
    // pool when the role check fails.
    if (!common.requireRole(r, req_id, principal, requirement.minimum_role)) return null;
    if (!authorizeWorkspace(r, req_id, conn, principal, workspace_id)) return null;

    return switch (requirement.credit_policy) {
        .none => .{},
        .execution_required => blk: {
            const result = requireExecutionAllowed(r, req_id, conn, alloc, workspace_id, actor) orelse return null;
            break :blk .{
                .credit = result.credit,
                .billing_state = result.billing_state,
            };
        },
    };
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

test "CreditPolicy has exactly two variants" {
    const fields = @typeInfo(CreditPolicy).@"enum".fields;
    try std.testing.expectEqual(@as(usize, 2), fields.len);
}

test "CreditPolicy.none is the zero-value variant" {
    try std.testing.expectEqual(@as(u1, 0), @intFromEnum(CreditPolicy.none));
}

test "CreditPolicy variant names match API contract" {
    try std.testing.expectEqualStrings("none", @tagName(CreditPolicy.none));
    try std.testing.expectEqualStrings("execution_required", @tagName(CreditPolicy.execution_required));
}

test "Requirement defaults: minimum_role=user, credit_policy=none" {
    const req = Requirement{};
    try std.testing.expectEqual(common.AuthRole.user, req.minimum_role);
    try std.testing.expectEqual(CreditPolicy.none, req.credit_policy);
}

test "Requirement accepts explicit field overrides" {
    const req = Requirement{
        .minimum_role = .admin,
        .credit_policy = .execution_required,
    };
    try std.testing.expectEqual(common.AuthRole.admin, req.minimum_role);
    try std.testing.expectEqual(CreditPolicy.execution_required, req.credit_policy);
}

test "Access default is empty — null credit and null billing_state" {
    const access = Access{};
    try std.testing.expectEqual(@as(?workspace_credit.CreditView, null), access.credit);
    try std.testing.expectEqual(@as(?workspace_billing.StateView, null), access.billing_state);
}

test "Access.deinit with null fields does not crash" {
    const access = Access{};
    access.deinit(std.testing.allocator);
}

test "Access.deinit frees allocated credit.currency" {
    const alloc = std.testing.allocator;
    const currency = try alloc.dupe(u8, "USD");

    var access = Access{
        .credit = .{
            .currency = currency,
            .initial_credit_cents = 1000,
            .consumed_credit_cents = 0,
            .remaining_credit_cents = 1000,
            .exhausted_at = null,
        },
        .billing_state = null,
    };
    // Should free currency without error; testing allocator will catch leaks.
    access.deinit(alloc);
    access.credit = null; // prevent double-free in case test framework retries
}

test "Access.deinit frees billing_state plan_sku and subscription_id" {
    const alloc = std.testing.allocator;
    const plan_sku = try alloc.dupe(u8, "free_v1");
    const sub_id = try alloc.dupe(u8, "sub_abc123");

    var access = Access{
        .credit = null,
        .billing_state = .{
            .plan_tier = .free,
            .billing_status = .active,
            .plan_sku = plan_sku,
            .subscription_id = sub_id,
            .grace_expires_at = null,
        },
    };
    access.deinit(alloc);
    access.billing_state = null;
}

test "Access.deinit frees billing_state with null subscription_id" {
    const alloc = std.testing.allocator;
    const plan_sku = try alloc.dupe(u8, "scale_v1");

    var access = Access{
        .credit = null,
        .billing_state = .{
            .plan_tier = .scale,
            .billing_status = .active,
            .plan_sku = plan_sku,
            .subscription_id = null,
            .grace_expires_at = null,
        },
    };
    access.deinit(alloc);
    access.billing_state = null;
}

test "Access.deinit frees both credit and billing_state together" {
    const alloc = std.testing.allocator;
    const currency = try alloc.dupe(u8, "USD");
    const plan_sku = try alloc.dupe(u8, "free_v1");
    const sub_id = try alloc.dupe(u8, "sub_xyz");

    var access = Access{
        .credit = .{
            .currency = currency,
            .initial_credit_cents = 500,
            .consumed_credit_cents = 200,
            .remaining_credit_cents = 300,
            .exhausted_at = null,
        },
        .billing_state = .{
            .plan_tier = .free,
            .billing_status = .active,
            .plan_sku = plan_sku,
            .subscription_id = sub_id,
            .grace_expires_at = null,
        },
    };
    access.deinit(alloc);
    access.credit = null;
    access.billing_state = null;
}

test "ExecutionResult holds credit and billing_state fields" {
    const result = ExecutionResult{
        .credit = .{
            .currency = "USD",
            .initial_credit_cents = 1000,
            .consumed_credit_cents = 100,
            .remaining_credit_cents = 900,
            .exhausted_at = null,
        },
        .billing_state = .{
            .plan_tier = .free,
            .billing_status = .active,
            .plan_sku = "free_v1",
            .subscription_id = null,
            .grace_expires_at = null,
        },
    };
    try std.testing.expectEqual(@as(i64, 900), result.credit.remaining_credit_cents);
    try std.testing.expectEqualStrings("free_v1", result.billing_state.plan_sku);
}

test "Requirement default minimum_role allows all AuthRole variants" {
    const req = Requirement{};
    try std.testing.expect(common.AuthRole.user.allows(req.minimum_role));
    try std.testing.expect(common.AuthRole.operator.allows(req.minimum_role));
    try std.testing.expect(common.AuthRole.admin.allows(req.minimum_role));
}

// Compile-time layout sanity checks
comptime {
    // Access must be a reasonable size (two optionals wrapping structs).
    const access_size = @sizeOf(Access);
    if (access_size == 0) @compileError("Access should not be zero-sized");

    // Requirement is tiny — two enum-width fields.
    const req_size = @sizeOf(Requirement);
    if (req_size == 0) @compileError("Requirement should not be zero-sized");

    // CreditPolicy must have exactly 2 variants.
    const cp_fields = @typeInfo(CreditPolicy).@"enum".fields;
    if (cp_fields.len != 2) @compileError("CreditPolicy must have exactly 2 variants");
}
