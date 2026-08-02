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
const WS_OF_TENANT = "(SELECT id FROM core.workspaces WHERE tenant_id = $1::uuid)";
/// Fleet ids in those workspaces.
const AGENTS_OF_TENANT = "(SELECT id FROM core.fleets WHERE workspace_id IN " ++ WS_OF_TENANT ++ ")";

/// Child-before-parent delete order. Every statement binds `$1` = tenant_id.
/// The fleet-scoped child deletes run before `core.fleets` (which their
/// subqueries read), and all workspace children run before `core.workspaces`.
const PURGE_STATEMENTS = [_][]const u8{
    // No ledger delete. It resolves to the tenant through a NOT NULL foreign
    // key now, so dropping the tenant below erases it by cascade — and no role
    // reachable from here may delete a charge any other way (schema/710 grants
    // no DELETE at all, to anyone). An explicit sweep would fail closed on
    // privilege rather than tidy up.
    // Keyed, no FK — memory fleet_id is the owning fleet UUID (schema/820).
    "DELETE FROM memory.memory_entries WHERE fleet_id IN " ++ AGENTS_OF_TENANT,
    // `fleet.metering_periods` is gone: derived per-renewal detail
    // with no product consumer, deleted rather than carried. runner_leases and
    // runner_affinity carry an ON DELETE CASCADE FK to core.fleets but stay
    // swept explicitly here — before core.fleets below — so an erased account
    // leaves no identifying rows behind, not only whatever the cascade catches.
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
    "DELETE FROM core.tenants WHERE id = $1::uuid",
};

pub const PurgeResult = struct {
    /// False when the subject was unknown or already purged — the idempotent
    /// replay case.
    purged: bool = false,
    /// Fleets erased by this purge that the caller never named in `enumerated`,
    /// counted INSIDE the purge transaction. Non-zero means a fleet appeared
    /// after the caller's enumeration and its upstream timer went to the grave
    /// with the row that named it.
    ///
    /// Identity, not cardinality: a count comparison reads clean whenever a
    /// fleet is created and another deleted in the same window, which is
    /// exactly when a leak is most likely and least visible.
    unenumerated_fleets: i64 = 0,
};

/// Purge the tenant owning `oidc_subject` plus all dependent rows, in one
/// transaction. Idempotent: an unknown or already-purged subject is a no-op
/// returning `.purged = false`. A mid-purge failure rolls back so Clerk can
/// retry.
///
/// `enumerated` is the fleet-id set the caller already handled upstream; every
/// fleet this purge erases that is absent from it is reported back through
/// `unenumerated_fleets`. Pass an empty slice to have the whole tenant counted
/// as unhandled.
pub fn purgeByOidcSubject(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    oidc_subject: []const u8,
    enumerated: []const []const u8,
) !PurgeResult {
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
    // authoritative view of what the purge is about to erase, and the only
    // place a fleet created after the caller's enumeration becomes visible.
    const unenumerated = try countUnenumeratedFleets(conn, tenant_id, enumerated);
    for (PURGE_STATEMENTS) |stmt| {
        _ = try conn.exec(stmt, .{tenant_id});
    }
    _ = try conn.exec(S_COMMIT, .{});
    return .{ .purged = true, .unenumerated_fleets = unenumerated };
}

/// Tenant fleets whose id is absent from `enumerated`. Compared as text so the
/// bound array needs no element cast; the scan is bounded by one tenant's
/// fleets. An empty `enumerated` counts every fleet, which is the truthful
/// answer when the caller handled none of them.
fn countUnenumeratedFleets(conn: *pg.Conn, tenant_id: []const u8, enumerated: []const []const u8) !i64 {
    var q = PgQuery.from(try conn.query(
        "SELECT COUNT(*)::bigint FROM core.fleets WHERE workspace_id IN " ++ WS_OF_TENANT ++
            " AND id::text <> ALL($2::text[])",
        .{ tenant_id, enumerated },
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
            "(SELECT w.id FROM core.workspaces w WHERE w.tenant_id = " ++
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
