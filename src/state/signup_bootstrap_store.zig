//! Raw SQL helpers for signup_bootstrap.zig. Split out per RULE FLL so the
//! facade stays below the 350-line gate. Every function assumes the caller
//! owns the enclosing transaction — none begin/commit on their own.

const std = @import("std");
const pg = @import("pg");
const PgQuery = @import("../db/pg_query.zig").PgQuery;

const TenantRow = struct {
    tenant_id: []const u8,
    name: []const u8,
    now_ms: i64,
};

const UserRow = struct {
    user_id: []const u8,
    tenant_id: []const u8,
    oidc_subject: []const u8,
    email: []const u8,
    display_name: ?[]const u8,
    now_ms: i64,
};

pub const WorkspaceRow = struct {
    workspace_id: []const u8,
    tenant_id: []const u8,
    name: []const u8,
    created_by: []const u8,
    now_ms: i64,
};

/// Result of the idempotent replay lookup. Owned strings freed via deinit.
pub const ExistingAccount = struct {
    user_id: []u8,
    tenant_id: []u8,
    workspace_id: []u8,
    workspace_name: []u8,

    pub fn deinit(self: *ExistingAccount, alloc: std.mem.Allocator) void {
        alloc.free(self.user_id);
        alloc.free(self.tenant_id);
        alloc.free(self.workspace_id);
        alloc.free(self.workspace_name);
    }
};

/// Join user → owner membership → tenant → earliest-named workspace. Picks
/// the earliest workspace with a non-null name so replay returns the default
/// signup workspace even if the user has created others since.
pub fn findExistingByOidcSubject(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    oidc_subject: []const u8,
) !?ExistingAccount {
    var q = PgQuery.from(try conn.query(
        \\SELECT
        \\    u.user_id::text,
        \\    t.tenant_id::text,
        \\    w.workspace_id::text,
        \\    w.name
        \\FROM core.users u
        \\JOIN core.memberships m ON m.user_id = u.user_id AND m.role = 'owner'
        \\JOIN core.tenants t ON t.tenant_id = m.tenant_id
        \\JOIN core.workspaces w ON w.tenant_id = t.tenant_id AND w.name IS NOT NULL
        \\WHERE u.oidc_subject = $1
        \\ORDER BY w.created_at ASC
        \\LIMIT 1
    , .{oidc_subject}));
    defer q.deinit();

    const row = (try q.next()) orelse return null;
    const user_id = try alloc.dupe(u8, try row.get([]const u8, 0));
    errdefer alloc.free(user_id);
    const tenant_id = try alloc.dupe(u8, try row.get([]const u8, 1));
    errdefer alloc.free(tenant_id);
    const workspace_id = try alloc.dupe(u8, try row.get([]const u8, 2));
    errdefer alloc.free(workspace_id);
    const workspace_name = try alloc.dupe(u8, try row.get([]const u8, 3));
    errdefer alloc.free(workspace_name);

    return .{
        .user_id = user_id,
        .tenant_id = tenant_id,
        .workspace_id = workspace_id,
        .workspace_name = workspace_name,
    };
}

pub fn insertTenant(conn: *pg.Conn, row: TenantRow) !void {
    _ = try conn.exec(
        \\INSERT INTO core.tenants
        \\  (tenant_id, name, created_at, updated_at)
        \\VALUES ($1::uuid, $2, $3, $3)
    , .{ row.tenant_id, row.name, row.now_ms });
}

pub fn insertUser(conn: *pg.Conn, row: UserRow) !void {
    _ = try conn.exec(
        \\INSERT INTO core.users
        \\  (user_id, tenant_id, oidc_subject, email, display_name, created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, $3, $4, $5, $6, $6)
    , .{ row.user_id, row.tenant_id, row.oidc_subject, row.email, row.display_name, row.now_ms });
}

pub fn insertMembership(
    conn: *pg.Conn,
    tenant_id: []const u8,
    user_id: []const u8,
    role: []const u8,
    now_ms: i64,
) !void {
    _ = try conn.exec(
        \\INSERT INTO core.memberships (tenant_id, user_id, role, created_at)
        \\VALUES ($1::uuid, $2::uuid, $3, $4)
    , .{ tenant_id, user_id, role, now_ms });
}

/// Returns true on insert, false on (tenant_id, name) collision. Caller
/// retries with a fresh name. One round-trip via ON CONFLICT against the
/// partial unique index `uq_workspaces_tenant_name` from schema/001 —
/// single statement keeps the connection clean inside the enclosing tx.
pub fn tryInsertWorkspace(conn: *pg.Conn, row: WorkspaceRow) !bool {
    const affected = try conn.exec(
        \\INSERT INTO core.workspaces
        \\  (workspace_id, tenant_id, name, repo_url, default_branch, created_by, created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, $3, '', 'main', $4, $5, $5)
        \\ON CONFLICT (tenant_id, name) WHERE name IS NOT NULL DO NOTHING
    , .{ row.workspace_id, row.tenant_id, row.name, row.created_by, row.now_ms });
    return (affected orelse 0) > 0;
}
