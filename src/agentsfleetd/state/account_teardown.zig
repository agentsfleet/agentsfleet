//! Hard-purge of a personal account, triggered by a Clerk `user.deleted`
//! webhook (`identity_events_clerk.runDelete`).
//!
//! Deletes the subject's tenant and every dependent row in foreign-key order.
//! `core.agents`, `vault.secrets`, `core.platform_llm_keys`,
//! `core.agent_sessions`, and `core.agent_approval_gates` reference
//! `workspaces`/`agents` WITHOUT `ON DELETE CASCADE`, so the workspace and
//! tenant deletes hit an FK violation — 500 the webhook and make Clerk retry
//! forever — unless their children go first. Cascade-backed children
//! (agent_events, integration_grants, agent_keys, api_keys, tenant_billing,
//! tenant_providers) drop with their parent.
//!
//! Per-agent Redis event streams (`agent:{id}:events`) are left to expire via
//! their TTL — the same fallback the per-agent delete path documents when
//! stream cleanup is skipped; the worker no-ops empty streams.

const std = @import("std");
const pg = @import("pg");
const logging = @import("log");

const PgQuery = @import("../db/pg_query.zig").PgQuery;
const approval_gate_db = @import("../agent/approval_gate_db.zig");

const log = logging.scoped(.account_teardown);

const S_BEGIN = "BEGIN";
const S_COMMIT = "COMMIT";

/// Workspaces owned by the tenant. `$1` = tenant_id.
const WS_OF_TENANT = "(SELECT workspace_id FROM core.workspaces WHERE tenant_id = $1::uuid)";
/// Agent ids in those workspaces.
const AGENTS_OF_TENANT = "(SELECT id FROM core.agents WHERE workspace_id IN " ++ WS_OF_TENANT ++ ")";

/// Child-before-parent delete order. Every statement binds `$1` = tenant_id.
/// The agent-scoped child deletes run before `core.agents` (which their
/// subqueries read), and all workspace children run before `core.workspaces`.
const PURGE_STATEMENTS = [_][]const u8{
    // Keyed, no FK — telemetry workspace_id is TEXT.
    "DELETE FROM core.agent_execution_telemetry WHERE workspace_id IN (SELECT workspace_id::text FROM core.workspaces WHERE tenant_id = $1::uuid)",
    // Keyed, no FK — memory agent_id is the owning agent UUID (schema/013).
    "DELETE FROM memory.memory_entries WHERE agent_id IN " ++ AGENTS_OF_TENANT,
    // Fleet rows carry tenant/agent ids but no FK into core (by design) —
    // swept explicitly so an erased account leaves no identifying rows behind.
    "DELETE FROM fleet.metering_periods WHERE event_id IN (SELECT event_id FROM fleet.runner_leases WHERE tenant_id = $1::uuid)",
    "DELETE FROM fleet.runner_leases WHERE tenant_id = $1::uuid",
    "DELETE FROM fleet.runner_affinity WHERE agent_id IN " ++ AGENTS_OF_TENANT,
    // Gates are append-only by trigger; the purge transaction opts out via
    // SET_GATE_PURGE_BYPASS_SQL below. Deleted by workspace OR agent so a
    // row referencing either parent cannot strand the erasure on its FK.
    "DELETE FROM core.agent_approval_gates WHERE workspace_id IN " ++ WS_OF_TENANT ++ " OR agent_id IN " ++ AGENTS_OF_TENANT,
    "DELETE FROM core.agent_sessions WHERE agent_id IN " ++ AGENTS_OF_TENANT,
    "DELETE FROM core.agents WHERE workspace_id IN " ++ WS_OF_TENANT,
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
