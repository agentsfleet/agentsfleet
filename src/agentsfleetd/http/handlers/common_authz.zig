const std = @import("std");
const constants = @import("common");
const pg = @import("pg");
const PgQuery = @import("../../db/pg_query.zig").PgQuery;
const db = @import("../../db/pool.zig");
const AuthPrincipal = @import("../../auth/principal.zig").AuthPrincipal;
const cross_tenant_audit = @import("../../auth/cross_tenant_audit.zig");
const logging = @import("log");

const log = logging.scoped(.auth);
const TENANT_ID_BUFFER_BYTES: usize = 64;

pub fn getFleetWorkspaceId(conn: *pg.Conn, alloc: std.mem.Allocator, fleet_id: []const u8) ?[]const u8 {
    var q = PgQuery.from(conn.query(
        \\SELECT workspace_id::text FROM core.fleets WHERE id = $1::uuid LIMIT 1
    , .{fleet_id}) catch return null);
    defer q.deinit();
    const row_opt = q.next() catch return null;
    const row = row_opt orelse return null;
    const ws = row.get([]u8, 0) catch return null;
    return alloc.dupe(u8, ws) catch null;
}

/// Resolve tenant ownership from the user row for OIDC principals, then verify
/// the workspace belongs to that tenant. API keys remain claim-bound and runner
/// principals fail closed. The cross-tenant override stays an additive fallback.
fn authoritativeWorkspaceTenant(
    conn: *pg.Conn,
    principal: AuthPrincipal,
    workspace_id: []const u8,
    tenant_buf: []u8,
) ?[]const u8 {
    const tenant_id =
        (resolvePrincipalTenant(conn, principal, tenant_buf) catch return null) orelse return null;

    var q = PgQuery.from(conn.query(
        "SELECT 1 FROM core.workspaces WHERE id = $1::uuid AND tenant_id = $2::uuid",
        .{ workspace_id, tenant_id },
    ) catch return null);
    defer q.deinit();
    _ = (q.next() catch return null) orelse return null;

    if (principal.workspace_scope_id) |scoped_workspace_id| {
        if (!std.mem.eql(u8, scoped_workspace_id, workspace_id)) return null;
    }
    return tenant_id;
}

pub fn authorizeWorkspace(conn: *pg.Conn, principal: AuthPrincipal, workspace_id: []const u8) bool {
    var tenant_buf: [TENANT_ID_BUFFER_BYTES]u8 = undefined;
    if (authoritativeWorkspaceTenant(conn, principal, workspace_id, &tenant_buf) != null) return true;
    return crossTenantBypass(conn, principal, workspace_id, .authorize_only);
}

pub fn setTenantSessionContext(conn: *pg.Conn, tenant_id: []const u8) bool {
    _ = conn.exec("SELECT set_config('app.current_tenant_id', $1, false)", .{tenant_id}) catch return false;
    return true;
}

/// Resolve OIDC users through the authoritative user row when one exists.
/// Database-backed API keys remain bound to their issuing tenant; runners
/// carry no tenant authority.
pub fn resolvePrincipalTenant(
    conn: *pg.Conn,
    principal: AuthPrincipal,
    tenant_buf: []u8,
) !?[]const u8 {
    switch (principal.mode) {
        .api_key => return principal.tenant_id,
        .runner => return null,
        // A CLI credential is a person, so it resolves the same way a browser
        // session does — through the authoritative user row rather than the
        // tenant its credential recorded at mint.
        .jwt_oidc, .cli_credential => {},
    }
    if (principal.user_id) |subject| {
        var q = PgQuery.from(try conn.query(
            "SELECT tenant_id::text FROM core.users WHERE oidc_subject = $1 LIMIT 1",
            .{subject},
        ));
        defer q.deinit();
        if (try q.next()) |row| {
            const tenant_id = try row.get([]const u8, 0);
            if (tenant_id.len == 0 or tenant_id.len > tenant_buf.len) {
                return error.InvalidTenantMapping;
            }
            @memcpy(tenant_buf[0..tenant_id.len], tenant_id);
            return tenant_buf[0..tenant_id.len];
        }
    }
    return principal.tenant_id;
}

pub fn authorizeWorkspaceAndSetTenantContext(conn: *pg.Conn, principal: AuthPrincipal, workspace_id: []const u8) bool {
    // Authorize BEFORE writing the RLS context, so a denied request never mutates
    // app.current_tenant_id. set_config here is session-level (not transaction-
    // scoped), so writing on the failure path would leak a tenant onto the pooled
    // connection for the next request that reuses it — there is no Postgres RLS
    // backstop today. Context is written only on success.
    var tenant_buf: [TENANT_ID_BUFFER_BYTES]u8 = undefined;
    if (authoritativeWorkspaceTenant(conn, principal, workspace_id, &tenant_buf)) |tenant_id| {
        return setTenantSessionContext(conn, tenant_id);
    }
    return crossTenantBypass(conn, principal, workspace_id, .set_context);
}

const BypassMode = enum { authorize_only, set_context };

/// The audited cross-tenant override. Engages ONLY when the
/// tenant-scoped check denied AND the principal holds `workspace:any` (a single
/// scope covering read and write across tenants, held by almost no one). Emits
/// an audit record before authorizing; in `.set_context` mode it sets the RLS
/// context to the TARGET tenant so the operator acts within the victim tenant's
/// row scope — the deliberate, scope-gated, audited form of what was previously
/// the cross-tenant IDOR. A non-holder is denied here, leaving the tenant-bound
/// behaviour above exactly as it was.
fn crossTenantBypass(conn: *pg.Conn, principal: AuthPrincipal, workspace_id: []const u8, mode: BypassMode) bool {
    if (!principal.scopes.contains(.workspace_any)) return false;

    // Resolve the target workspace's tenant, copying it out before any write on
    // the same conn (the read must be drained first — RULE DRAIN).
    var tenant_buf: [TENANT_ID_BUFFER_BYTES]u8 = undefined;
    const target_tenant = blk: {
        var q = PgQuery.from(conn.query(
            "SELECT tenant_id::text FROM core.workspaces WHERE id = $1::uuid",
            .{workspace_id},
        ) catch return false);
        defer q.deinit();
        const row = (q.next() catch return false) orelse return false;
        const t = row.get([]u8, 0) catch return false;
        if (t.len == 0) return false;
        if (t.len > tenant_buf.len) {
            // A `workspace:any` holder is denied here only if the target tenant_id
            // is longer than the buffer — a misconfiguration, not a normal deny.
            // Surface it so the silent cross-tenant rejection is diagnosable.
            log.err("cross_tenant_target_tenant_id_too_long", .{ .workspace_id = workspace_id, .len = t.len, .cap = tenant_buf.len });
            return false;
        }
        @memcpy(tenant_buf[0..t.len], t);
        break :blk tenant_buf[0..t.len];
    };

    // Audit BEFORE proceeding — this is the sole bypass path, so every bypass is
    // recorded (Invariant 11).
    cross_tenant_audit.emit(principal, workspace_id, target_tenant);

    if (mode == .set_context) return setTenantSessionContext(conn, target_tenant);
    return true;
}

pub fn openHandlerTestConn(alloc: std.mem.Allocator) !?struct { pool: *db.Pool, conn: *pg.Conn } {
    const url = constants.env.testLiveValue("TEST_DATABASE_URL") orelse
        constants.env.testLiveValue("DATABASE_URL") orelse return null;
    const opts = try db.parseUrl(std.heap.page_allocator, url);
    const pool = try pg.Pool.init(constants.globalIo(), alloc, opts);
    errdefer pool.deinit();
    const conn = try pool.acquire();
    return .{ .pool = pool, .conn = conn };
}
