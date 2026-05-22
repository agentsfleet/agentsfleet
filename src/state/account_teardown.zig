//! Hard-purge of a personal account, triggered by a Clerk `user.deleted`
//! webhook (`identity_events_clerk.runDelete`).
//!
//! Deletes the subject's tenant and every dependent row in foreign-key order.
//! `core.zombies`, `vault.secrets`, `core.platform_llm_keys`,
//! `core.zombie_sessions`, and `core.zombie_approval_gates` reference
//! `workspaces`/`zombies` WITHOUT `ON DELETE CASCADE`, so the workspace and
//! tenant deletes hit an FK violation — 500 the webhook and make Clerk retry
//! forever — unless their children go first. Cascade-backed children
//! (zombie_events, integration_grants, agent_keys, api_keys, tenant_billing,
//! tenant_providers) drop with their parent.
//!
//! Per-zombie Redis event streams (`zombie:{id}:events`) are left to expire via
//! their TTL — the same fallback the per-zombie delete path documents when
//! stream cleanup is skipped; the worker no-ops empty streams.

const std = @import("std");
const pg = @import("pg");
const logging = @import("log");

const PgQuery = @import("../db/pg_query.zig").PgQuery;

const log = logging.scoped(.account_teardown);

const S_BEGIN = "BEGIN";
const S_COMMIT = "COMMIT";
const S_ROLLBACK = "ROLLBACK";

/// Workspaces owned by the tenant. `$1` = tenant_id.
const WS_OF_TENANT = "(SELECT workspace_id FROM core.workspaces WHERE tenant_id = $1::uuid)";
/// Zombie ids in those workspaces.
const ZOMBIES_OF_TENANT = "(SELECT id FROM core.zombies WHERE workspace_id IN " ++ WS_OF_TENANT ++ ")";

/// Child-before-parent delete order. Every statement binds `$1` = tenant_id.
/// The zombie-scoped child deletes run before `core.zombies` (which their
/// subqueries read), and all workspace children run before `core.workspaces`.
const PURGE_STATEMENTS = [_][]const u8{
    // Keyed, no FK — telemetry workspace_id is TEXT.
    "DELETE FROM core.zombie_execution_telemetry WHERE workspace_id IN (SELECT workspace_id::text FROM core.workspaces WHERE tenant_id = $1::uuid)",
    // Keyed, no FK — memory instance_id is the TEXT namespace 'zmb:{uuid}'.
    "DELETE FROM memory.memory_entries WHERE instance_id IN (SELECT 'zmb:' || id::text FROM core.zombies WHERE workspace_id IN " ++ WS_OF_TENANT ++ ")",
    "DELETE FROM core.zombie_approval_gates WHERE zombie_id IN " ++ ZOMBIES_OF_TENANT,
    "DELETE FROM core.zombie_sessions WHERE zombie_id IN " ++ ZOMBIES_OF_TENANT,
    "DELETE FROM core.zombies WHERE workspace_id IN " ++ WS_OF_TENANT,
    "DELETE FROM vault.secrets WHERE workspace_id IN " ++ WS_OF_TENANT,
    "DELETE FROM core.platform_llm_keys WHERE source_workspace_id IN " ++ WS_OF_TENANT,
    "DELETE FROM core.workspaces WHERE tenant_id = $1::uuid",
    "DELETE FROM core.memberships WHERE tenant_id = $1::uuid",
    "DELETE FROM core.users WHERE tenant_id = $1::uuid",
    "DELETE FROM core.tenants WHERE tenant_id = $1::uuid",
};

/// Purge the tenant owning `oidc_subject` plus all dependent rows, in one
/// transaction. Idempotent: an unknown or already-purged subject is a no-op
/// returning `false`. A mid-purge failure rolls back so Clerk can retry.
pub fn purgeByOidcSubject(conn: *pg.Conn, alloc: std.mem.Allocator, oidc_subject: []const u8) !bool {
    const tenant_id = (try fetchTenantId(conn, alloc, oidc_subject)) orelse return false;
    defer alloc.free(tenant_id);

    _ = try conn.exec(S_BEGIN, .{});
    errdefer _ = conn.exec(S_ROLLBACK, .{}) catch |err| log.warn("ignored_error", .{ .err = @errorName(err) });
    for (PURGE_STATEMENTS) |stmt| {
        _ = try conn.exec(stmt, .{tenant_id});
    }
    _ = try conn.exec(S_COMMIT, .{});
    return true;
}

/// Resolve the subject's tenant_id as text. Caller owns the returned slice.
fn fetchTenantId(conn: *pg.Conn, alloc: std.mem.Allocator, oidc_subject: []const u8) !?[]u8 {
    var q = PgQuery.from(try conn.query(
        "SELECT tenant_id::text FROM core.users WHERE oidc_subject = $1",
        .{oidc_subject},
    ));
    defer q.deinit();
    const row = (try q.next()) orelse return null;
    return try alloc.dupe(u8, try row.get([]const u8, 0));
}
