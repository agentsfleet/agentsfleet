//! Workspace authorization funnel — ONE Postgres round trip on the happy path.
//!
//! `authorizeWorkspace` / `authorizeWorkspaceAndSetTenantContext` decide
//! ownership with a single statement (`common_authz_sql.zig`): the effective
//! tenant (user row first, token claim as fallback) and the workspace match
//! resolve together, and the `_SET_CONTEXT` variant writes the Row-Level
//! Security (RLS) session context inside the same statement — only on allow,
//! because `set_config` lives in the SELECT list of a WHERE-gated row. The
//! pre-merge shape spent two to three sequential round trips per request on
//! exactly this decision.
//!
//! The statement count in this file is the design: one authorize statement per
//! request on the happy path; `resolvePrincipalTenant` (cold paths that need a
//! tenant with no workspace), the audited cross-tenant bypass, and the
//! fleet→workspace resolve each keep their own.

const std = @import("std");
const constants = @import("common");
const pg = @import("pg");
const PgQuery = @import("../../db/pg_query.zig").PgQuery;
const db = @import("../../db/pool.zig");
const AuthPrincipal = @import("../../auth/principal.zig").AuthPrincipal;
const cross_tenant_audit = @import("../../auth/cross_tenant_audit.zig");
const id_format = @import("../../types/id_format.zig");
const sql = @import("common_authz_sql.zig");
const logging = @import("log");

const log = logging.scoped(.auth);
const TENANT_ID_BUFFER_BYTES: usize = 64;

pub fn getFleetWorkspaceId(conn: *pg.Conn, alloc: std.mem.Allocator, fleet_id: []const u8) ?[]const u8 {
    var q = PgQuery.from(conn.query(sql.SELECT_FLEET_WORKSPACE, .{fleet_id}) catch return null);
    defer q.deinit();
    const row_opt = q.next() catch return null;
    const row = row_opt orelse return null;
    const ws = row.get([]u8, 0) catch return null;
    return alloc.dupe(u8, ws) catch null;
}

const ContextWrite = enum { none, on_allow };

/// The two binds a merged tenant-resolving statement needs: the OIDC subject
/// (user-row arm) and the well-formed tenant claim (fallback arm). Null when
/// the principal carries no tenant authority at all (runner, or nothing to
/// bind) — callers deny without a round trip. Shared by the authorize funnel
/// and the tenant-scoped list statements that fold the same resolve.
pub const TenantBinds = struct {
    subject: ?[]const u8,
    claim: ?[]const u8,
};

pub fn principalTenantBinds(principal: AuthPrincipal) ?TenantBinds {
    if (principal.mode == .runner) return null;
    const subject: ?[]const u8 = if (principal.mode == .jwt_oidc) principal.user_id else null;
    // A malformed claim can only ever deny (it sits behind COALESCE), so it
    // degrades to absent rather than becoming a statement-level cast error.
    const claim: ?[]const u8 = if (principal.tenant_id) |t|
        (if (id_format.isUuid(t)) t else null)
    else
        null;
    if (subject == null and claim == null) return null;
    return .{ .subject = subject, .claim = claim };
}

/// The merged verdict. Returns the owning tenant (copied into `tenant_buf`)
/// when the principal's effective tenant owns the workspace; null denies.
///
/// Ordering is load-bearing: the workspace-scope claim check runs BEFORE the
/// statement, so the `_SET_CONTEXT` variant can never write RLS context for a
/// request that a later app-side check would deny — and a scope mismatch costs
/// no round trip at all.
fn authoritativeWorkspaceTenant(
    conn: *pg.Conn,
    principal: AuthPrincipal,
    workspace_id: []const u8,
    tenant_buf: []u8,
    context: ContextWrite,
) ?[]const u8 {
    if (principal.workspace_scope_id) |scoped_workspace_id| {
        if (!std.mem.eql(u8, scoped_workspace_id, workspace_id)) return null;
    }

    // The user row is authoritative for OIDC principals; claim-bound modes
    // (api_key, cli_credential) already resolved their tenant at auth time,
    // so their subject binds NULL and the claim alone decides. No tenant
    // source at all — the statement could never match; skip the round trip.
    const binds = principalTenantBinds(principal) orelse return null;

    var q = PgQuery.from(switch (context) {
        .none => conn.query(sql.AUTHORIZE_WORKSPACE, .{ workspace_id, binds.subject, binds.claim }),
        .on_allow => conn.query(sql.AUTHORIZE_WORKSPACE_SET_CONTEXT, .{ workspace_id, binds.subject, binds.claim }),
    } catch return null);
    defer q.deinit();
    const row = (q.next() catch return null) orelse return null;
    const tenant_id = row.get([]const u8, 0) catch return null;
    if (tenant_id.len == 0 or tenant_id.len > tenant_buf.len) {
        log.err("workspace_tenant_id_out_of_bounds", .{ .workspace_id = workspace_id, .len = tenant_id.len, .cap = tenant_buf.len });
        return null;
    }
    @memcpy(tenant_buf[0..tenant_id.len], tenant_id);
    return tenant_buf[0..tenant_id.len];
}

pub fn authorizeWorkspace(conn: *pg.Conn, principal: AuthPrincipal, workspace_id: []const u8) bool {
    var tenant_buf: [TENANT_ID_BUFFER_BYTES]u8 = undefined;
    if (authoritativeWorkspaceTenant(conn, principal, workspace_id, &tenant_buf, .none) != null) return true;
    return crossTenantBypass(conn, principal, workspace_id, .authorize_only);
}

pub fn authorizeWorkspaceAndSetTenantContext(conn: *pg.Conn, principal: AuthPrincipal, workspace_id: []const u8) bool {
    var tenant_buf: [TENANT_ID_BUFFER_BYTES]u8 = undefined;
    if (authoritativeWorkspaceTenant(conn, principal, workspace_id, &tenant_buf, .on_allow) != null) return true;
    return crossTenantBypass(conn, principal, workspace_id, .set_context);
}

pub fn setTenantSessionContext(conn: *pg.Conn, tenant_id: []const u8) bool {
    _ = conn.exec(sql.SET_TENANT_CONTEXT, .{tenant_id}) catch return false;
    return true;
}

/// Resolve OIDC users through the authoritative user row when one exists.
/// Cold-path only (workspace create, tenant-scoped lists that carry no
/// workspace id); the workspace-scoped hot path resolves inside the merged
/// authorize statement instead.
pub fn resolvePrincipalTenant(
    conn: *pg.Conn,
    principal: AuthPrincipal,
    tenant_buf: []u8,
) !?[]const u8 {
    switch (principal.mode) {
        // A CLI credential's tenant is already authoritative: its auth lookup
        // joins core.users and puts u.tenant_id on the principal, so re-reading
        // the same user row here would be a second round trip for the same value.
        .api_key, .cli_credential => return principal.tenant_id,
        .runner => return null,
        .jwt_oidc => {},
    }
    if (principal.user_id) |subject| {
        var q = PgQuery.from(try conn.query(sql.SELECT_USER_TENANT_BY_SUBJECT, .{subject}));
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
        var q = PgQuery.from(conn.query(sql.SELECT_WORKSPACE_TENANT, .{workspace_id}) catch return false);
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
