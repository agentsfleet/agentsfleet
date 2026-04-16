//! Integration Grant workspace-owner operations.
//! GET    /v1/workspaces/{ws}/zombies/{id}/integration-grants        → innerListGrants  (bearer policy)
//! DELETE /v1/workspaces/{ws}/zombies/{id}/integration-grants/{gid}  → innerRevokeGrant (bearer policy)

const std = @import("std");
const httpz = @import("httpz");
const pg = @import("pg");
const PgQuery = @import("../../db/pg_query.zig").PgQuery;
const common = @import("common.zig");
const hx_mod = @import("hx.zig");
const ec = @import("../../errors/error_registry.zig");

const log = std.log.scoped(.integration_grants);

pub const Context = common.Context;

// M24_001: getZombieWorkspaceId lived here pre-M24 and was duplicated into
// zombie_activity_api for the IDOR guard. It now lives in common.zig so all
// workspace-scoped zombie routes share one implementation.

// ── innerListGrants ────────────────────────────────────────────────────────
// GET /v1/workspaces/{ws}/zombies/{zombie_id}/integration-grants
// bearer policy — principal set by middleware.

const GrantRow = struct {
    grant_id: []const u8,
    service: []const u8,
    status: []const u8,
    requested_at: i64,
    approved_at: ?i64,
    revoked_at: ?i64,
    reason: []const u8,
};

pub fn innerListGrants(hx: hx_mod.Hx, workspace_id: []const u8, zombie_id: []const u8) void {
    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    if (!common.authorizeWorkspace(conn, hx.principal, workspace_id)) {
        hx.fail(ec.ERR_FORBIDDEN, "Workspace access denied");
        return;
    }
    // M24_001: verify zombie belongs to the path workspace (don't leak existence
    // of zombies in other workspaces — return 404, not 403).
    const zombie_ws_id = common.getZombieWorkspaceId(conn, hx.alloc, zombie_id) orelse {
        hx.fail(ec.ERR_ZOMBIE_NOT_FOUND, "Zombie not found");
        return;
    };
    if (!std.mem.eql(u8, zombie_ws_id, workspace_id)) {
        hx.fail(ec.ERR_ZOMBIE_NOT_FOUND, "Zombie not found");
        return;
    }

    var q = PgQuery.from(conn.query(
        \\SELECT grant_id, service, status, requested_at, approved_at, revoked_at, requested_reason
        \\FROM core.integration_grants
        \\WHERE zombie_id = $1::uuid
        \\ORDER BY requested_at DESC
    , .{zombie_id}) catch {
        common.internalDbError(hx.res, hx.req_id);
        return;
    });
    defer q.deinit();

    var grants: std.ArrayListUnmanaged(GrantRow) = .{};
    while (q.next() catch null) |row| {
        const grant_id    = hx.alloc.dupe(u8, row.get([]u8, 0) catch continue) catch continue;
        const service     = hx.alloc.dupe(u8, row.get([]u8, 1) catch continue) catch continue;
        const status      = hx.alloc.dupe(u8, row.get([]u8, 2) catch continue) catch continue;
        const requested_at = row.get(i64, 3) catch continue;
        const approved_at  = row.get(i64, 4) catch null;
        const revoked_at   = row.get(i64, 5) catch null;
        const reason      = hx.alloc.dupe(u8, row.get([]u8, 6) catch continue) catch continue;
        grants.append(hx.alloc, .{
            .grant_id     = grant_id,
            .service      = service,
            .status       = status,
            .requested_at = requested_at,
            .approved_at  = approved_at,
            .revoked_at   = revoked_at,
            .reason       = reason,
        }) catch {};
    }

    hx.ok(.ok, .{ .items = grants.items, .total = grants.items.len });
}

// ── innerRevokeGrant ───────────────────────────────────────────────────────
// DELETE /v1/workspaces/{ws}/zombies/{zombie_id}/integration-grants/{grant_id}
// bearer policy — principal set by middleware.

pub fn innerRevokeGrant(hx: hx_mod.Hx, workspace_id: []const u8, zombie_id: []const u8, grant_id: []const u8) void {
    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    if (!common.authorizeWorkspace(conn, hx.principal, workspace_id)) {
        hx.fail(ec.ERR_FORBIDDEN, "Workspace access denied");
        return;
    }
    // M24_001: verify zombie belongs to the path workspace.
    const zombie_ws_id = common.getZombieWorkspaceId(conn, hx.alloc, zombie_id) orelse {
        hx.fail(ec.ERR_ZOMBIE_NOT_FOUND, "Zombie not found");
        return;
    };
    if (!std.mem.eql(u8, zombie_ws_id, workspace_id)) {
        hx.fail(ec.ERR_ZOMBIE_NOT_FOUND, "Zombie not found");
        return;
    }

    const now_ms = std.time.milliTimestamp();
    // Scope UPDATE by workspace_id as defence-in-depth — mirrors the pattern in
    // killZombieOnConn. Even if the app-level zombie-ws match is ever bypassed by
    // a future refactor, the SQL still refuses cross-workspace revocation.
    var rev_q = PgQuery.from(conn.query(
        \\UPDATE core.integration_grants
        \\SET status = 'revoked', revoked_at = $1
        \\WHERE grant_id = $2
        \\  AND zombie_id = $3::uuid
        \\  AND zombie_id IN (SELECT id FROM core.zombies WHERE workspace_id = $4::uuid)
        \\  AND status != 'revoked'
        \\RETURNING grant_id
    , .{ now_ms, grant_id, zombie_id, workspace_id }) catch {
        common.internalDbError(hx.res, hx.req_id);
        return;
    });
    defer rev_q.deinit();

    const revoked = rev_q.next() catch null;
    if (revoked == null) {
        hx.fail(ec.ERR_GRANT_NOT_FOUND, "Grant not found or already revoked");
        return;
    }

    log.info("grant.revoked zombie_id={s} grant_id={s}", .{ zombie_id, grant_id });
    hx.noContent();
}

// ── Defence-in-depth test for the revoke UPDATE SQL ────────────────────────
//
// Even if a future refactor removes the app-layer `zombie_ws_id == workspace_id`
// check in innerRevokeGrant, the SQL `WHERE zombie_id IN (SELECT id FROM
// core.zombies WHERE workspace_id = $4)` clause must still block cross-workspace
// revocation. This test executes the exact production SQL with a foreign
// workspace_id and asserts (a) RETURNING produces zero rows, (b) the grant's
// status remains 'pending' in the DB. If the SQL scope is ever removed from the
// UPDATE, this test fails immediately.

test "integration: revoke UPDATE SQL blocks cross-workspace even without app check" {
    const alloc = std.testing.allocator;
    const db_ctx = (try common.openHandlerTestConn(alloc)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    const conn = db_ctx.conn;

    // Unique IDs to avoid collisions with other integration test fixtures.
    const tenant_id = "0195b4ba-8d3a-7f13-8abc-2b3e1e0ddd01";
    const ws_a = "0195b4ba-8d3a-7f13-8abc-2b3e1e0ddd11";
    const ws_b = "0195b4ba-8d3a-7f13-8abc-2b3e1e0ddd12";
    const zombie_in_b = "0195b4ba-8d3a-7f13-8abc-2b3e1e0ddd21";
    const grant_id = "grant_defindep_001";
    const now: i64 = std.time.milliTimestamp();

    // Seed tenant, two workspaces, a zombie in WS_B, and a pending grant.
    _ = try conn.exec(
        \\INSERT INTO tenants (tenant_id, name, api_key_hash, created_at, updated_at)
        \\VALUES ($1, 'DefIndepTest', 'managed', $2, $2)
        \\ON CONFLICT (tenant_id) DO NOTHING
    , .{ tenant_id, now });
    _ = try conn.exec(
        \\INSERT INTO workspaces (workspace_id, tenant_id, repo_url, default_branch, paused, version, created_at, updated_at)
        \\VALUES ($1, $2, 'https://github.com/test/defindep-a', 'main', false, 1, $3, $3)
        \\ON CONFLICT (workspace_id) DO NOTHING
    , .{ ws_a, tenant_id, now });
    _ = try conn.exec(
        \\INSERT INTO workspaces (workspace_id, tenant_id, repo_url, default_branch, paused, version, created_at, updated_at)
        \\VALUES ($1, $2, 'https://github.com/test/defindep-b', 'main', false, 1, $3, $3)
        \\ON CONFLICT (workspace_id) DO NOTHING
    , .{ ws_b, tenant_id, now });
    _ = try conn.exec(
        \\INSERT INTO core.zombies (id, workspace_id, name, source_markdown, config_json, status, created_at, updated_at)
        \\VALUES ($1, $2, 'defindep-victim', '---\nname: defindep-victim\n---\nx', '{"name":"defindep-victim"}', 'active', 0, 0)
        \\ON CONFLICT DO NOTHING
    , .{ zombie_in_b, ws_b });
    _ = try conn.exec(
        \\INSERT INTO core.integration_grants
        \\  (grant_id, zombie_id, service, status, requested_at, requested_reason)
        \\VALUES ($1, $2::uuid, 'slack', 'pending', $3, 'test defence-in-depth')
        \\ON CONFLICT (grant_id) DO NOTHING
    , .{ grant_id, zombie_in_b, now });

    defer {
        _ = conn.exec("DELETE FROM core.integration_grants WHERE grant_id = $1", .{grant_id}) catch {};
        _ = conn.exec("DELETE FROM core.zombies WHERE id = $1::uuid", .{zombie_in_b}) catch {};
        _ = conn.exec("DELETE FROM workspaces WHERE workspace_id = $1", .{ws_a}) catch {};
        _ = conn.exec("DELETE FROM workspaces WHERE workspace_id = $1", .{ws_b}) catch {};
        _ = conn.exec("DELETE FROM tenants WHERE tenant_id = $1", .{tenant_id}) catch {};
    }

    // Execute the EXACT production UPDATE with a MISMATCHED workspace_id (WS_A,
    // while the zombie lives in WS_B). Must return zero rows.
    var rev_q = PgQuery.from(try conn.query(
        \\UPDATE core.integration_grants
        \\SET status = 'revoked', revoked_at = $1
        \\WHERE grant_id = $2
        \\  AND zombie_id = $3::uuid
        \\  AND zombie_id IN (SELECT id FROM core.zombies WHERE workspace_id = $4::uuid)
        \\  AND status != 'revoked'
        \\RETURNING grant_id
    , .{ now, grant_id, zombie_in_b, ws_a }));
    defer rev_q.deinit();
    try std.testing.expect((try rev_q.next()) == null);

    // Grant status must still be 'pending' — SQL defence blocked the revoke.
    var check_q = PgQuery.from(try conn.query(
        "SELECT status FROM core.integration_grants WHERE grant_id = $1", .{grant_id},
    ));
    defer check_q.deinit();
    const row = (try check_q.next()) orelse return error.TestUnexpectedResult;
    const status_after = try row.get([]const u8, 0);
    try std.testing.expectEqualStrings("pending", status_after);
}
