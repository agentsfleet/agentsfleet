//! Hard-purge of a personal account, triggered by a Clerk `user.deleted`
//! webhook (`identity_events_clerk.runDelete`).
//!
//! Deletes the subject's tenant and every dependent row in foreign-key order.
//! `core.fleets`, `vault.secrets`, `core.platform_provider_defaults`,
//! `core.fleet_sessions`, and `core.fleet_approval_gates` reference
//! `workspaces`/`fleets` WITHOUT `ON DELETE CASCADE`, so the workspace and
//! tenant deletes hit an FK violation — 500 the webhook and make Clerk retry
//! forever — unless their children go first. Cascade-backed children
//! (fleet_events, integration_grants, fleet_keys, api_keys, tenant_billing,
//! tenant_model_selection) drop with their parent.
//!
//! Per-fleet Redis event streams (`fleet:{id}:events`) are left to expire via
//! their TTL — the same fallback the per-fleet delete path documents when
//! stream cleanup is skipped; the worker no-ops empty streams.

const std = @import("std");
const pg = @import("pg");
const logging = @import("log");

const PgQuery = @import("../db/pg_query.zig").PgQuery;
const approval_gate_db = @import("../fleet_runtime/approval_gate_db.zig");

const log = logging.scoped(.account_teardown);

const S_BEGIN = "BEGIN";
const S_COMMIT = "COMMIT";

/// Workspaces owned by the tenant. `$1` = tenant_id.
const WS_OF_TENANT = "(SELECT workspace_id FROM core.workspaces WHERE tenant_id = $1::uuid)";
/// Fleet ids in those workspaces.
const AGENTS_OF_TENANT = "(SELECT id FROM core.fleets WHERE workspace_id IN " ++ WS_OF_TENANT ++ ")";

/// Child-before-parent delete order. Every statement binds `$1` = tenant_id.
/// The fleet-scoped child deletes run before `core.fleets` (which their
/// subqueries read), and all workspace children run before `core.workspaces`.
const PURGE_STATEMENTS = [_][]const u8{
    // Keyed, no FK — telemetry workspace_id is TEXT.
    "DELETE FROM core.fleet_execution_telemetry WHERE workspace_id IN (SELECT workspace_id::text FROM core.workspaces WHERE tenant_id = $1::uuid)",
    // Keyed, no FK — memory fleet_id is the owning fleet UUID (schema/013).
    "DELETE FROM memory.memory_entries WHERE fleet_id IN " ++ AGENTS_OF_TENANT,
    // metering_periods is keyed by event_id (no FK); runner_leases/runner_affinity
    // now carry an ON DELETE CASCADE FK to core.fleets but are still
    // swept explicitly here — before core.fleets below — so an erased account
    // leaves no identifying rows behind, not only whatever the cascade would catch.
    "DELETE FROM fleet.metering_periods WHERE event_id IN (SELECT event_id FROM fleet.runner_leases WHERE tenant_id = $1::uuid)",
    "DELETE FROM fleet.runner_leases WHERE tenant_id = $1::uuid",
    "DELETE FROM fleet.runner_affinity WHERE fleet_id IN " ++ AGENTS_OF_TENANT,
    // Gates are append-only by trigger; the purge transaction opts out via
    // SET_GATE_PURGE_BYPASS_SQL below. Deleted by workspace OR fleet so a
    // row referencing either parent cannot strand the erasure on its FK.
    "DELETE FROM core.fleet_approval_gates WHERE workspace_id IN " ++ WS_OF_TENANT ++ " OR fleet_id IN " ++ AGENTS_OF_TENANT,
    "DELETE FROM core.fleet_sessions WHERE fleet_id IN " ++ AGENTS_OF_TENANT,
    "DELETE FROM core.fleets WHERE workspace_id IN " ++ WS_OF_TENANT,
    "DELETE FROM vault.secrets WHERE workspace_id IN " ++ WS_OF_TENANT,
    "DELETE FROM core.platform_provider_defaults WHERE source_workspace_id IN " ++ WS_OF_TENANT,
    "DELETE FROM core.workspaces WHERE tenant_id = $1::uuid",
    "DELETE FROM core.memberships WHERE tenant_id = $1::uuid",
    "DELETE FROM core.users WHERE tenant_id = $1::uuid",
    "DELETE FROM core.tenants WHERE tenant_id = $1::uuid",
};

pub const PurgeResult = struct {
    /// False when the subject was unknown or already purged — the idempotent
    /// replay case.
    purged: bool = false,
    /// Fleets present at purge time, counted INSIDE the purge transaction. The
    /// caller compares it against what it managed to unregister: a higher
    /// number means a fleet was created after the enumeration and its upstream
    /// timer went to the grave with the row that named it.
    fleets_at_purge: i64 = 0,
};

/// Purge the tenant owning `oidc_subject` plus all dependent rows, in one
/// transaction. Idempotent: an unknown or already-purged subject is a no-op
/// returning `.purged = false`. A mid-purge failure rolls back so Clerk can
/// retry.
pub fn purgeByOidcSubject(conn: *pg.Conn, alloc: std.mem.Allocator, oidc_subject: []const u8) !PurgeResult {
    const tenant_id = (try fetchTenantId(conn, alloc, oidc_subject)) orelse return .{};
    defer alloc.free(tenant_id);

    _ = try conn.exec(S_BEGIN, .{});
    // Registered BEFORE any statement inside the transaction (including the
    // bypass SET LOCAL) so a failure of ANY of them rolls back — an errdefer
    // placed later would leak the open transaction on the pooled connection.
    // Use conn.rollback() not conn.exec("ROLLBACK") — the driver's exec
    // short-circuits when the connection is in FAIL state after a statement
    // error, leaving the session stuck in an aborted tx. rollback() uses
    // execIgnoringState specifically for this case (signup_bootstrap.zig
    // precedent).
    errdefer conn.rollback() catch |err| log.warn(logging.EVENT_IGNORED_ERROR, .{ .err = @errorName(err) });
    _ = try conn.exec(approval_gate_db.SET_GATE_PURGE_BYPASS_SQL, .{});
    // Read inside the transaction, before anything is deleted: this is the
    // authoritative count of what the purge is about to erase, and the only
    // place a fleet created after the caller's enumeration becomes visible.
    const fleets_at_purge = try countTenantFleets(conn, tenant_id);
    for (PURGE_STATEMENTS) |stmt| {
        _ = try conn.exec(stmt, .{tenant_id});
    }
    _ = try conn.exec(S_COMMIT, .{});
    return .{ .purged = true, .fleets_at_purge = fleets_at_purge };
}

fn countTenantFleets(conn: *pg.Conn, tenant_id: []const u8) !i64 {
    var q = PgQuery.from(try conn.query(
        "SELECT COUNT(*)::bigint FROM core.fleets WHERE workspace_id IN " ++ WS_OF_TENANT,
        .{tenant_id},
    ));
    defer q.deinit();
    const row = (try q.next()) orelse return 0;
    return row.get(i64, 0);
}

/// Fleet ids owned by the subject's tenant, resolved while the rows still
/// exist — the purge below erases them, so the caller collects these FIRST to
/// unregister upstream schedule timers (the rows cascade away; the provider
/// registration does not). Null = unknown/already-purged subject. Caller frees
/// each id and the slice.
pub fn fleetIdsByOidcSubject(conn: *pg.Conn, alloc: std.mem.Allocator, oidc_subject: []const u8) !?[][]const u8 {
    var q = PgQuery.from(try conn.query(
        "SELECT z.id::text FROM core.fleets z WHERE z.workspace_id IN " ++
            "(SELECT w.workspace_id FROM core.workspaces w WHERE w.tenant_id = " ++
            "(SELECT u.tenant_id FROM core.users u WHERE u.oidc_subject = $1))",
        .{oidc_subject},
    ));
    defer q.deinit();
    var ids: std.ArrayList([]const u8) = .empty;
    errdefer {
        for (ids.items) |id| alloc.free(id);
        ids.deinit(alloc);
    }
    while (try q.next()) |row| {
        try ids.append(alloc, try alloc.dupe(u8, try row.get([]const u8, 0)));
    }
    if (ids.items.len == 0) {
        ids.deinit(alloc);
        return null;
    }
    return try ids.toOwnedSlice(alloc);
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
