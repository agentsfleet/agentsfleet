const std = @import("std");
const constants = @import("common");
const pg = @import("pg");
const PgQuery = @import("../../db/pg_query.zig").PgQuery;
const db = @import("../../db/pool.zig");
const AuthPrincipal = @import("../../auth/principal.zig").AuthPrincipal;
const cross_tenant_audit = @import("../../auth/cross_tenant_audit.zig");
const logging = @import("log");

const log = logging.scoped(.auth);

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

/// Tenant-scoped ownership — UNCHANGED from the pre-scope model (Invariant 3).
/// Fail closed without a tenant. A null tenant_id must NEVER degrade to an
/// unscoped existence check — that authorizes the caller against any tenant's
/// workspace (cross-tenant IDOR). The only null-tenant principals are an
/// unprovisioned Clerk session (before the user.created metadata writeback
/// lands) and runner tokens; the former is exactly the attacker, and runners
/// authorize via runnerBearer against fleet.runners, never through here. Does
/// NOT consult the cross-tenant override — that is a strictly additive fallback.
fn ownsWithinTenant(conn: *pg.Conn, principal: AuthPrincipal, workspace_id: []const u8) bool {
    const tenant_id = principal.tenant_id orelse return false;

    var q = PgQuery.from(conn.query(
        "SELECT 1 FROM core.workspaces WHERE workspace_id = $1 AND tenant_id = $2",
        .{ workspace_id, tenant_id },
    ) catch return false);
    defer q.deinit();
    _ = (q.next() catch return false) orelse return false;

    if (principal.workspace_scope_id) |scoped_workspace_id| {
        if (!std.mem.eql(u8, scoped_workspace_id, workspace_id)) return false;
    }
    return true;
}

pub fn authorizeWorkspace(conn: *pg.Conn, principal: AuthPrincipal, workspace_id: []const u8) bool {
    if (ownsWithinTenant(conn, principal, workspace_id)) return true;
    return crossTenantBypass(conn, principal, workspace_id, .authorize_only);
}

pub fn setTenantSessionContext(conn: *pg.Conn, tenant_id: []const u8) bool {
    _ = conn.exec("SELECT set_config('app.current_tenant_id', $1, false)", .{tenant_id}) catch return false;
    return true;
}

pub fn authorizeWorkspaceAndSetTenantContext(conn: *pg.Conn, principal: AuthPrincipal, workspace_id: []const u8) bool {
    // Authorize BEFORE writing the RLS context, so a denied request never mutates
    // app.current_tenant_id. set_config here is session-level (not transaction-
    // scoped), so writing on the failure path would leak a tenant onto the pooled
    // connection for the next request that reuses it — there is no Postgres RLS
    // backstop today. Context is written only on success.
    if (ownsWithinTenant(conn, principal, workspace_id)) {
        // Non-null here: ownsWithinTenant returns false on a null tenant_id.
        return setTenantSessionContext(conn, principal.tenant_id.?);
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
    var tenant_buf: [64]u8 = undefined;
    const target_tenant = blk: {
        var q = PgQuery.from(conn.query(
            "SELECT tenant_id::text FROM core.workspaces WHERE workspace_id = $1",
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
