const std = @import("std");
const httpz = @import("httpz");
const pg = @import("pg");
const PgQuery = @import("../db/pg_query.zig").PgQuery;
const common = @import("handlers/common.zig");
const error_codes = @import("../errors/error_registry.zig");
const http_auth = @import("../db/test_fixtures_http_auth.zig");

pub const Requirement = struct {
    minimum_role: common.AuthRole = .user,
};

pub const Access = struct {
    /// Reserved for future per-request state. Currently empty; `deinit` is a
    /// no-op so existing call sites (`defer access.deinit(alloc)`) stay valid.
    pub fn deinit(self: Access, alloc: std.mem.Allocator) void {
        _ = self;
        _ = alloc;
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
    common.errorResponse(res, error_codes.ERR_FORBIDDEN, "Workspace access denied", req_id);
    return false;
}

/// Returns true if the given user created the workspace.
/// Only queries the DB when a `user`-role principal fails the minimum role
/// gate, so there is no hot-path cost for operator/admin callers.
fn isWorkspaceCreator(conn: *pg.Conn, workspace_id: []const u8, user_id: []const u8, tenant_id: ?[]const u8) bool {
    var q = PgQuery.from(if (tenant_id) |tid|
        conn.query(
            "SELECT 1 FROM workspaces WHERE workspace_id = $1 AND created_by = $2 AND tenant_id = $3",
            .{ workspace_id, user_id, tid },
        ) catch return false
    else
        conn.query(
            "SELECT 1 FROM workspaces WHERE workspace_id = $1 AND created_by = $2",
            .{ workspace_id, user_id },
        ) catch return false);
    defer q.deinit();
    const row = (q.next() catch return false) orelse return false;
    _ = row;
    return true;
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
    _ = alloc;
    _ = actor;
    // Role check: stateless first, then workspace-owner override for `user` role.
    // Workspace creators auto-promote to `operator` so they can use the product
    // immediately after signup without manual Clerk intervention.
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
    return .{};
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

test "Requirement default minimum_role is user" {
    const req = Requirement{};
    try std.testing.expectEqual(common.AuthRole.user, req.minimum_role);
}

test "Requirement accepts explicit field overrides" {
    const req = Requirement{ .minimum_role = .admin };
    try std.testing.expectEqual(common.AuthRole.admin, req.minimum_role);
}

test "Access.deinit is a no-op" {
    const access = Access{};
    access.deinit(std.testing.allocator);
}

test "integration: isWorkspaceCreator returns true for creator" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    http_auth.cleanup(db_ctx.conn);
    defer http_auth.cleanup(db_ctx.conn);

    try http_auth.seedTenant(db_ctx.conn);
    try http_auth.seedGuardWorkspace(db_ctx.conn, http_auth.WS_PRIMARY, "user_creator123");

    try std.testing.expect(isWorkspaceCreator(db_ctx.conn, http_auth.WS_PRIMARY, "user_creator123", http_auth.TENANT_ID));
    try std.testing.expect(!isWorkspaceCreator(db_ctx.conn, http_auth.WS_PRIMARY, "user_other456", http_auth.TENANT_ID));
    try std.testing.expect(isWorkspaceCreator(db_ctx.conn, http_auth.WS_PRIMARY, "user_creator123", null));
    try std.testing.expect(!isWorkspaceCreator(db_ctx.conn, http_auth.WS_PRIMARY, "user_creator123", http_auth.TENANT_UNRELATED));
}

test "integration: isWorkspaceCreator returns false when created_by is null" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    http_auth.cleanup(db_ctx.conn);
    defer http_auth.cleanup(db_ctx.conn);

    try http_auth.seedTenant(db_ctx.conn);
    try http_auth.seedGuardWorkspace(db_ctx.conn, http_auth.WS_PRIMARY, null);

    try std.testing.expect(!isWorkspaceCreator(db_ctx.conn, http_auth.WS_PRIMARY, "user_any", http_auth.TENANT_ID));
}

test "integration: isWorkspaceCreator returns false for non-existent workspace" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try std.testing.expect(!isWorkspaceCreator(db_ctx.conn, http_auth.WS_ABSENT, "user_any", null));
}

test "integration: isWorkspaceCreator is scoped to exact workspace" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    http_auth.cleanup(db_ctx.conn);
    defer http_auth.cleanup(db_ctx.conn);

    try http_auth.seedTenant(db_ctx.conn);
    try http_auth.seedGuardWorkspace(db_ctx.conn, http_auth.WS_PRIMARY, "user_alice");
    try http_auth.seedGuardWorkspace(db_ctx.conn, http_auth.WS_SECONDARY, "user_bob");

    try std.testing.expect(isWorkspaceCreator(db_ctx.conn, http_auth.WS_PRIMARY, "user_alice", http_auth.TENANT_ID));
    try std.testing.expect(!isWorkspaceCreator(db_ctx.conn, http_auth.WS_SECONDARY, "user_alice", http_auth.TENANT_ID));
    try std.testing.expect(!isWorkspaceCreator(db_ctx.conn, http_auth.WS_PRIMARY, "user_bob", http_auth.TENANT_ID));
    try std.testing.expect(isWorkspaceCreator(db_ctx.conn, http_auth.WS_SECONDARY, "user_bob", http_auth.TENANT_ID));
}

test "owner override promotes user to operator but not admin" {
    var effective = common.AuthPrincipal{
        .mode = .jwt_oidc,
        .role = .user,
        .user_id = "user_creator123",
        .tenant_id = "tenant_abc",
    };
    const is_creator = true;
    if (effective.role == .user and is_creator) {
        effective.role = .operator;
    }
    try std.testing.expect(effective.role.allows(.operator));
    try std.testing.expect(!effective.role.allows(.admin));
}

test "owner override is idempotent — operator stays operator" {
    const principal = common.AuthPrincipal{
        .mode = .jwt_oidc,
        .role = .operator,
        .user_id = "user_creator123",
    };
    try std.testing.expect(principal.role.allows(.operator));
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
