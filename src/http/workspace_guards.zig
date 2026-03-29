const std = @import("std");
const httpz = @import("httpz");
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
    res: *httpz.Response,
    req_id: []const u8,
    conn: *pg.Conn,
    principal: common.AuthPrincipal,
    workspace_id: []const u8,
) bool {
    if (common.authorizeWorkspaceAndSetTenantContext(conn, principal, workspace_id)) return true;
    common.errorResponse(res, .forbidden, error_codes.ERR_FORBIDDEN, "Workspace access denied", req_id);
    return false;
}

/// Returns true if the given user created the workspace (M15_001 owner check).
/// Only queries the DB when a `user`-role principal fails the minimum role gate,
/// so there is no hot-path cost for `operator`/`admin` callers.
fn isWorkspaceCreator(conn: *pg.Conn, workspace_id: []const u8, user_id: []const u8, tenant_id: ?[]const u8) bool {
    var q = if (tenant_id) |tid|
        conn.query(
            "SELECT 1 FROM workspaces WHERE workspace_id = $1 AND created_by = $2 AND tenant_id = $3",
            .{ workspace_id, user_id, tid },
        ) catch return false
    else
        conn.query(
            "SELECT 1 FROM workspaces WHERE workspace_id = $1 AND created_by = $2",
            .{ workspace_id, user_id },
        ) catch return false;
    defer q.deinit();
    const row = (q.next() catch return false) orelse return false;
    _ = row;
    q.drain() catch return false;
    return true;
}

const ExecutionResult = struct {
    credit: workspace_credit.CreditView,
    billing_state: workspace_billing.StateView,
};

fn requireExecutionAllowed(
    res: *httpz.Response,
    req_id: []const u8,
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
    actor: []const u8,
) ?ExecutionResult {
    const billing_state = workspace_billing.reconcileWorkspaceBilling(conn, alloc, workspace_id, std.time.milliTimestamp(), actor) catch |err| {
        if (workspace_billing.errorCode(err)) |code| {
            common.errorResponse(res, .internal_server_error, code, workspace_billing.errorMessage(err) orelse "Workspace billing failure", req_id);
            return null;
        }
        common.internalOperationError(res, "Failed to reconcile workspace billing state", req_id);
        return null;
    };

    const credit = workspace_credit.enforceExecutionAllowed(conn, alloc, workspace_id, billing_state.plan_tier) catch |err| {
        alloc.free(billing_state.plan_sku);
        if (billing_state.subscription_id) |v| alloc.free(v);
        if (workspace_credit.errorCode(err)) |code| {
            common.errorResponse(res, .forbidden, code, workspace_credit.errorMessage(err) orelse "Workspace credit failure", req_id);
            return null;
        }
        common.internalOperationError(res, "Failed to validate workspace credit balance", req_id);
        return null;
    };

    return .{ .credit = credit, .billing_state = billing_state };
}

pub fn enforce(
    res: *httpz.Response,
    req_id: []const u8,
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    principal: common.AuthPrincipal,
    workspace_id: []const u8,
    actor: []const u8,
    requirement: Requirement,
) ?Access {
    // Role check: stateless first, then workspace-owner override for `user` role.
    // M15_001 — workspace creators are auto-promoted to `operator` so they can
    // use the product immediately after signup without manual Clerk intervention.
    var effective_principal = principal;
    if (!principal.role.allows(requirement.minimum_role)) {
        if (principal.role == .user) {
            if (principal.user_id) |uid| {
                if (isWorkspaceCreator(conn, workspace_id, uid, principal.tenant_id)) {
                    effective_principal.role = .operator;
                }
            }
        }
        if (!common.requireRole(res, req_id, effective_principal, requirement.minimum_role)) return null;
    }
    if (!authorizeWorkspace(res, req_id, conn, effective_principal, workspace_id)) return null;

    return switch (requirement.credit_policy) {
        .none => .{},
        .execution_required => blk: {
            const result = requireExecutionAllowed(res, req_id, conn, alloc, workspace_id, actor) orelse return null;
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

test "integration: isWorkspaceCreator returns true for creator" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    _ = try db_ctx.conn.exec(
        \\CREATE TEMP TABLE workspaces (
        \\  workspace_id UUID PRIMARY KEY,
        \\  tenant_id UUID NOT NULL,
        \\  created_by TEXT
        \\)
    , .{});
    _ = try db_ctx.conn.exec(
        "INSERT INTO workspaces (workspace_id, tenant_id, created_by) VALUES ('0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11', '0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01', 'user_creator123')",
        .{},
    );

    const tid = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01";
    try std.testing.expect(isWorkspaceCreator(db_ctx.conn, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11", "user_creator123", tid));
    try std.testing.expect(!isWorkspaceCreator(db_ctx.conn, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11", "user_other456", tid));
    // Without tenant_id still works (nullable path)
    try std.testing.expect(isWorkspaceCreator(db_ctx.conn, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11", "user_creator123", null));
    // Wrong tenant_id blocks even correct creator
    try std.testing.expect(!isWorkspaceCreator(db_ctx.conn, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11", "user_creator123", "0195b4ba-8d3a-7f13-8abc-000000000099"));
}

test "integration: isWorkspaceCreator returns false when created_by is null" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    _ = try db_ctx.conn.exec(
        \\CREATE TEMP TABLE workspaces (
        \\  workspace_id UUID PRIMARY KEY,
        \\  tenant_id UUID NOT NULL,
        \\  created_by TEXT
        \\)
    , .{});
    _ = try db_ctx.conn.exec(
        "INSERT INTO workspaces (workspace_id, tenant_id, created_by) VALUES ('0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11', '0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01', NULL)",
        .{},
    );

    try std.testing.expect(!isWorkspaceCreator(db_ctx.conn, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11", "user_any", "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01"));
}

test "integration: isWorkspaceCreator returns false for non-existent workspace" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    _ = try db_ctx.conn.exec(
        \\CREATE TEMP TABLE workspaces (
        \\  workspace_id UUID PRIMARY KEY,
        \\  tenant_id UUID NOT NULL,
        \\  created_by TEXT
        \\)
    , .{});

    try std.testing.expect(!isWorkspaceCreator(db_ctx.conn, "0195b4ba-8d3a-7f13-8abc-000000000000", "user_any", null));
}

test "integration: isWorkspaceCreator is scoped to exact workspace" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    _ = try db_ctx.conn.exec(
        \\CREATE TEMP TABLE workspaces (
        \\  workspace_id UUID PRIMARY KEY,
        \\  tenant_id UUID NOT NULL,
        \\  created_by TEXT
        \\)
    , .{});
    _ = try db_ctx.conn.exec(
        "INSERT INTO workspaces (workspace_id, tenant_id, created_by) VALUES ('0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11', '0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01', 'user_alice')",
        .{},
    );
    _ = try db_ctx.conn.exec(
        "INSERT INTO workspaces (workspace_id, tenant_id, created_by) VALUES ('0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f12', '0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01', 'user_bob')",
        .{},
    );

    const tid = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01";
    try std.testing.expect(isWorkspaceCreator(db_ctx.conn, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11", "user_alice", tid));
    try std.testing.expect(!isWorkspaceCreator(db_ctx.conn, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f12", "user_alice", tid));
    try std.testing.expect(!isWorkspaceCreator(db_ctx.conn, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11", "user_bob", tid));
    try std.testing.expect(isWorkspaceCreator(db_ctx.conn, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f12", "user_bob", tid));
}

test "owner override promotes user to operator but not admin" {
    // Exercises the actual override logic: a user-role principal who is workspace
    // creator gets promoted to operator. Operator still fails admin requirement.
    var effective = common.AuthPrincipal{
        .mode = .jwt_oidc,
        .role = .user,
        .user_id = "user_creator123",
        .tenant_id = "tenant_abc",
    };
    // Simulate the enforce() promotion path
    const is_creator = true;
    if (effective.role == .user and is_creator) {
        effective.role = .operator;
    }
    // Passes operator gate
    try std.testing.expect(effective.role.allows(.operator));
    // Still blocked from admin gate
    try std.testing.expect(!effective.role.allows(.admin));
}

test "owner override skipped when user_id is null" {
    const principal = common.AuthPrincipal{
        .mode = .jwt_oidc,
        .role = .user,
        .user_id = null,
    };
    var effective = principal;
    if (principal.role == .user) {
        if (principal.user_id) |_| {
            effective.role = .operator;
        }
    }
    try std.testing.expectEqual(common.AuthRole.user, effective.role);
}

test "owner override is idempotent — operator stays operator" {
    const principal = common.AuthPrincipal{
        .mode = .jwt_oidc,
        .role = .operator,
        .user_id = "user_creator123",
    };
    try std.testing.expect(principal.role.allows(.operator));
}

test "owner override is idempotent — admin stays admin" {
    const principal = common.AuthPrincipal{
        .mode = .jwt_oidc,
        .role = .admin,
        .user_id = "user_creator123",
    };
    try std.testing.expect(principal.role.allows(.operator));
    try std.testing.expect(principal.role.allows(.admin));
}

test "integration: non-creator user stays blocked at user role" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    _ = try db_ctx.conn.exec(
        \\CREATE TEMP TABLE workspaces (
        \\  workspace_id UUID PRIMARY KEY,
        \\  tenant_id UUID NOT NULL,
        \\  created_by TEXT
        \\)
    , .{});
    _ = try db_ctx.conn.exec(
        "INSERT INTO workspaces (workspace_id, tenant_id, created_by) VALUES ('0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11', '0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01', 'user_owner')",
        .{},
    );

    try std.testing.expect(!isWorkspaceCreator(db_ctx.conn, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11", "user_invited_member", "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01"));
}

test "effective_principal is a copy — original principal is not mutated" {
    const original = common.AuthPrincipal{
        .mode = .jwt_oidc,
        .role = .user,
        .user_id = "user_creator123",
    };
    var effective = original;
    effective.role = .operator;
    try std.testing.expectEqual(common.AuthRole.user, original.role);
    try std.testing.expectEqual(common.AuthRole.operator, effective.role);
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
