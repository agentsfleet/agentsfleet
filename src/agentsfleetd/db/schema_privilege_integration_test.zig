//! Integration proof of the privilege boundary against a live PostgreSQL —
//! the catalogue, the refusals, and the elevated paths end to end.
//!
//! The unit tier (`schema_privilege_test.zig`) pins the DECLARED posture in
//! the slot text; this tier proves the live database enforces it. The test
//! user is a superuser in local dev, so `SET ROLE api_runtime` succeeds and
//! from that point the session holds exactly what the handlers hold — which
//! is what makes the refusal assertions honest.
//!
//! Requires LIVE_DB=1 + TEST_DATABASE_URL (set by `make test-integration`).
//! Each refusal test opens its own pool: a 42501 leaves the connection in a
//! pg-error state the next assertion must not inherit.

const std = @import("std");
const common = @import("common");
const pg = @import("pg");
const base = @import("test_fixtures.zig");
const PgQuery = @import("pg_query.zig").PgQuery;
const pool_elevation = @import("pool_elevation.zig");
const vault = @import("../state/vault.zig");
const account_teardown = @import("../state/account_teardown.zig");
const crypto_primitives = @import("../secrets/crypto_primitives.zig");

const SQLSTATE_INSUFFICIENT_PRIVILEGE = "42501";

// UUIDv7-shaped fixture identity (14th hex digit '7' satisfies the slot
// CHECKs), disjoint from every other suite's constants.
const PRIV_TENANT_ID = "01917000-aaaa-7000-8000-00000000a154";
const PRIV_WORKSPACE_ID = "01917000-aaaa-7000-8000-00000000b154";
const PRIV_USER_ID = "01917000-aaaa-7000-8000-00000000c154";
const PRIV_OIDC_SUBJECT = "priv-boundary-user-m154";
const PRIV_SECRET_NAME = "priv-boundary-secret";
const PRIV_SECRET_BODY = "{\"api_token\":\"round-trip-proof\"}";

fn skipUnlessLive() !void {
    if (common.env.testLiveValue("LIVE_DB") == null) return error.SkipZigTest;
}

fn scalarI64(conn: *pg.Conn, sql: []const u8, args: anytype) !i64 {
    var q = PgQuery.from(try conn.query(sql, args));
    defer q.deinit();
    const row = (try q.next()) orelse return error.TestUnexpectedResult;
    return row.get(i64, 0);
}

fn currentRoleOwned(alloc: std.mem.Allocator, conn: *pg.Conn) ![]u8 {
    var q = PgQuery.from(try conn.query("SELECT current_role::text", .{}));
    defer q.deinit();
    const row = (try q.next()) orelse return error.TestUnexpectedResult;
    return alloc.dupe(u8, try row.get([]const u8, 0));
}

fn teardownPrivFixtures(conn: *pg.Conn) void {
    // Superuser sweep, child-before-parent; each arm tolerant of absence.
    _ = conn.exec("RESET ROLE", .{}) catch |err| std.log.warn("priv fixture cleanup ignored: {s}", .{@errorName(err)});
    _ = conn.exec("DELETE FROM vault.secrets WHERE workspace_id = $1::uuid", .{PRIV_WORKSPACE_ID}) catch |err| std.log.warn("priv fixture cleanup ignored: {s}", .{@errorName(err)});
    _ = conn.exec("DELETE FROM billing.usage_ledger WHERE tenant_id = $1::uuid", .{PRIV_TENANT_ID}) catch |err| std.log.warn("priv fixture cleanup ignored: {s}", .{@errorName(err)});
    _ = conn.exec("DELETE FROM billing.tenant_wallet WHERE tenant_id = $1::uuid", .{PRIV_TENANT_ID}) catch |err| std.log.warn("priv fixture cleanup ignored: {s}", .{@errorName(err)});
    _ = conn.exec("DELETE FROM core.users WHERE tenant_id = $1::uuid", .{PRIV_TENANT_ID}) catch |err| std.log.warn("priv fixture cleanup ignored: {s}", .{@errorName(err)});
    _ = conn.exec("DELETE FROM core.workspaces WHERE tenant_id = $1::uuid", .{PRIV_TENANT_ID}) catch |err| std.log.warn("priv fixture cleanup ignored: {s}", .{@errorName(err)});
    _ = conn.exec("DELETE FROM core.tenants WHERE id = $1::uuid", .{PRIV_TENANT_ID}) catch |err| std.log.warn("priv fixture cleanup ignored: {s}", .{@errorName(err)});
}

fn seedPrivTenantAndWorkspace(conn: *pg.Conn) !void {
    try base.seedTenantById(conn, PRIV_TENANT_ID, "priv-boundary-tenant");
    try base.seedWorkspaceWithTenant(conn, PRIV_WORKSPACE_ID, PRIV_TENANT_ID);
}

// ── §1 — the grants ─────────────────────────────────────────────────────────

test "integration: api_runtime holds zero catalogue grants on the secret store and the wallet" {
    try skipUnlessLive();
    const alloc = std.testing.allocator;
    const db_ctx = (try base.openTestConn(alloc)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    // Table-level grants: zero rows (Dimension 1.1). Column-level grants are
    // swept too — a column grant would satisfy the table query while still
    // widening the role.
    const table_grants = try scalarI64(db_ctx.conn,
        \\SELECT count(*)::bigint FROM information_schema.role_table_grants
        \\WHERE grantee = 'api_runtime'
        \\  AND ((table_schema = 'vault' AND table_name = 'secrets')
        \\    OR (table_schema = 'billing' AND table_name = 'tenant_wallet'))
    , .{});
    try std.testing.expectEqual(@as(i64, 0), table_grants);

    const column_grants = try scalarI64(db_ctx.conn,
        \\SELECT count(*)::bigint FROM information_schema.role_column_grants
        \\WHERE grantee = 'api_runtime'
        \\  AND ((table_schema = 'vault' AND table_name = 'secrets')
        \\    OR (table_schema = 'billing' AND table_name = 'tenant_wallet'))
    , .{});
    try std.testing.expectEqual(@as(i64, 0), column_grants);
}

test "integration: an unelevated read or write of either table is refused by PostgreSQL" {
    try skipUnlessLive();
    const alloc = std.testing.allocator;

    // Dimension 1.2 + the live half of Dimension 1.4: api_runtime IS a member
    // of vault_runtime and billing_runtime, so a refusal here proves the
    // membership is non-inheriting — dormant until SET ROLE — not merely that
    // no grant exists.
    const cases = [_][]const u8{
        "SELECT count(*) FROM vault.secrets",
        "UPDATE vault.secrets SET updated_at = 0 WHERE key_name = 'no-such-key'",
        "SELECT count(*) FROM billing.tenant_wallet",
        "UPDATE billing.tenant_wallet SET updated_at = 0 WHERE tenant_id = '01917000-aaaa-7000-8000-00000000ffff'::uuid",
    };
    for (cases) |case_sql| {
        // Fresh pool per case: the refusal leaves the connection erroring.
        const db_ctx = (try base.openTestConn(alloc)) orelse return error.SkipZigTest;
        defer db_ctx.pool.deinit();
        defer db_ctx.pool.release(db_ctx.conn);
        const conn = db_ctx.conn;
        _ = try conn.exec("SET ROLE api_runtime", .{});

        const result = conn.query(case_sql, .{}); // check-pg-drain: ok — the statement is refused (42501), so no Result ever exists to drain
        try std.testing.expectError(error.PG, result);
        const pg_err = conn.err orelse return error.ExpectedPgError;
        try std.testing.expectEqualStrings(SQLSTATE_INSUFFICIENT_PRIVILEGE, pg_err.code);
    }
}

test "integration: the migration role retains full authority on both tables" {
    try skipUnlessLive();
    const alloc = std.testing.allocator;
    const db_ctx = (try base.openTestConn(alloc)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    // Dimension 1.3. A rebuild from empty re-authors these tables as
    // db_migrator; its schema-level ALL (schema/110) is what makes that
    // possible. Pinned via has_schema_privilege — PostgreSQL's
    // information_schema carries no view of schema ACLs (role_usage_grants
    // covers sequences and domains only), so the catalogue functions are the
    // one readable surface for this grant.
    const migrator_schema_grants = try scalarI64(db_ctx.conn,
        \\SELECT (has_schema_privilege('db_migrator', 'vault', 'USAGE')::int
        \\      + has_schema_privilege('db_migrator', 'vault', 'CREATE')::int
        \\      + has_schema_privilege('db_migrator', 'billing', 'USAGE')::int
        \\      + has_schema_privilege('db_migrator', 'billing', 'CREATE')::int)::bigint
    , .{});
    try std.testing.expectEqual(@as(i64, 4), migrator_schema_grants);
}

// ── §2 — the elevated paths ─────────────────────────────────────────────────

test "integration: secret store, read and delete work end to end under elevation" {
    try skipUnlessLive();
    // The real store path seals with the process KEK; seed it the way boot
    // does (secrets_json_integration_test convention).
    crypto_primitives.setTestKek();
    const alloc = std.testing.allocator;
    const db_ctx = (try base.openTestConn(alloc)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);
    const conn = db_ctx.conn;

    defer teardownPrivFixtures(conn);
    try seedPrivTenantAndWorkspace(conn);

    // Dimension 2.1: the production entry points, on a session holding exactly
    // the handler role. Store and read elevate internally; nothing here names
    // a role.
    _ = try conn.exec("SET ROLE api_runtime", .{});

    try vault.storeJsonPlaintext(alloc, conn, PRIV_WORKSPACE_ID, PRIV_SECRET_NAME, PRIV_SECRET_BODY);

    var parsed = try vault.loadJson(alloc, conn, PRIV_WORKSPACE_ID, PRIV_SECRET_NAME);
    defer parsed.deinit();
    const token = parsed.value.object.get("api_token") orelse return error.TestUnexpectedResult;
    try std.testing.expectEqualStrings("round-trip-proof", token.string);

    try std.testing.expect(try vault.deleteCredential(conn, PRIV_WORKSPACE_ID, PRIV_SECRET_NAME));
    // Idempotent delete: second call reports nothing removed, still no error.
    try std.testing.expect(!try vault.deleteCredential(conn, PRIV_WORKSPACE_ID, PRIV_SECRET_NAME));
}

test "integration: a failed statement inside an elevated callback rolls back and clears the role" {
    try skipUnlessLive();
    const alloc = std.testing.allocator;
    const db_ctx = (try base.openTestConn(alloc)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);
    const conn = db_ctx.conn;
    defer {
        _ = conn.exec("RESET ROLE", .{}) catch |err| std.log.warn("priv fixture cleanup ignored: {s}", .{@errorName(err)});
    }

    _ = try conn.exec("SET ROLE api_runtime", .{});

    // Dimension 2.3: a statement fails mid-scope; `deinit` rolls the owned
    // transaction back, and the server reverts SET LOCAL with it. Nothing in
    // this block issues a reset — the defer is the whole cleanup.
    const failed = blk: {
        var scope = pool_elevation.begin(conn, .vault) catch |err| break :blk err;
        defer scope.deinit();
        // A statement that cannot succeed even elevated: relation absent.
        _ = scope.conn.exec("SELECT no_such_column FROM vault.secrets", .{}) catch |err| break :blk err;
        scope.commit() catch |err| break :blk err;
        break :blk {};
    };
    try std.testing.expectError(error.PG, failed);

    // The connection reports the base role and serves an unelevated read —
    // no manual cleanup happened between the failure and this assertion.
    const role = try currentRoleOwned(alloc, conn);
    defer alloc.free(role);
    try std.testing.expectEqualStrings("api_runtime", role);
    _ = try scalarI64(conn, "SELECT 1::bigint", .{});
}

test "integration: erasure removes secrets and the wallet row under elevation" {
    try skipUnlessLive();
    // Seeding a secret below rides the real store path, which needs the KEK.
    crypto_primitives.setTestKek();
    const alloc = std.testing.allocator;
    const db_ctx = (try base.openTestConn(alloc)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);
    const conn = db_ctx.conn;

    defer teardownPrivFixtures(conn);
    try seedPrivTenantAndWorkspace(conn);
    _ = try conn.exec(
        \\INSERT INTO core.users (id, tenant_id, oidc_subject, email, display_name, created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, $3, 'priv@example.test', 'priv', 0, 0)
        \\ON CONFLICT DO NOTHING
    , .{ PRIV_USER_ID, PRIV_TENANT_ID, PRIV_OIDC_SUBJECT });
    _ = try conn.exec(
        \\INSERT INTO billing.tenant_wallet (tenant_id, balance_nanos, grant_source, created_at, updated_at)
        \\VALUES ($1::uuid, 42, 'priv-test', 0, 0)
        \\ON CONFLICT DO NOTHING
    , .{PRIV_TENANT_ID});
    try vault.storeJsonPlaintext(alloc, conn, PRIV_WORKSPACE_ID, PRIV_SECRET_NAME, PRIV_SECRET_BODY);

    // Dimension 2.4: the purge runs on the handler's role; its vault and
    // memory statements elevate per statement inside the one transaction.
    _ = try conn.exec("SET ROLE api_runtime", .{});
    const result = try account_teardown.purgeByOidcSubject(conn, alloc, PRIV_OIDC_SUBJECT, &.{});
    try std.testing.expect(result.purged);
    _ = try conn.exec("RESET ROLE", .{});

    try std.testing.expectEqual(@as(i64, 0), try scalarI64(
        conn,
        "SELECT count(*)::bigint FROM vault.secrets WHERE workspace_id = $1::uuid",
        .{PRIV_WORKSPACE_ID},
    ));
    try std.testing.expectEqual(@as(i64, 0), try scalarI64(
        conn,
        "SELECT count(*)::bigint FROM billing.tenant_wallet WHERE tenant_id = $1::uuid",
        .{PRIV_TENANT_ID},
    ));
}

// ── §3 — the pool backstop ──────────────────────────────────────────────────

test "integration: a failed connection is refused elevation before any SET LOCAL" {
    try skipUnlessLive();
    const alloc = std.testing.allocator;
    const db_ctx = (try base.openTestConn(alloc)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);
    const conn = db_ctx.conn;

    // Abort an explicit transaction: the failed statement leaves the driver in
    // its fail state, which the elevation gate must refuse outright — a
    // mid-query or failed connection never gets a SET LOCAL issued on it.
    try conn.begin();
    try std.testing.expectError(error.PG, conn.exec("SELECT no_such_thing", .{}));
    try std.testing.expectError(pool_elevation.Error.ElevationRefused, pool_elevation.begin(conn, .vault));
    try conn.rollback();
    _ = try scalarI64(conn, "SELECT 1::bigint", .{});
}

test "integration: api_runtime elevates to metering_runtime and returns to base" {
    try skipUnlessLive();
    const alloc = std.testing.allocator;
    const db_ctx = (try base.openTestConn(alloc)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);
    const conn = db_ctx.conn;
    defer {
        _ = conn.exec("RESET ROLE", .{}) catch |err| std.log.warn("priv fixture cleanup ignored: {s}", .{@errorName(err)});
    }

    // The composite role fences every renewal and settle charge, yet the
    // metering suites connect as superuser — which may SET any role. This is
    // the one live proof that the api_runtime -> metering_runtime SET path
    // (slot 120's membership) actually holds in the catalogue.
    _ = try conn.exec("SET ROLE api_runtime", .{});
    var scope = try pool_elevation.begin(conn, .metering);
    defer scope.deinit();
    const inside_role = try currentRoleOwned(alloc, scope.conn);
    defer alloc.free(inside_role);
    try scope.commit();
    try std.testing.expectEqualStrings("metering_runtime", inside_role);

    const after_role = try currentRoleOwned(alloc, conn);
    defer alloc.free(after_role);
    try std.testing.expectEqualStrings("api_runtime", after_role);
}

test "integration: a connection returns to the base role and is reusable after an elevated commit" {
    try skipUnlessLive();
    const alloc = std.testing.allocator;
    const db_ctx = (try base.openTestConn(alloc)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);
    const conn = db_ctx.conn;
    defer {
        _ = conn.exec("RESET ROLE", .{}) catch |err| std.log.warn("priv fixture cleanup ignored: {s}", .{@errorName(err)});
    }

    _ = try conn.exec("SET ROLE api_runtime", .{});

    // Dimension 3.2: inside the scope the role is elevated; after the commit
    // the SAME connection reports the base role and serves an unelevated read,
    // with no reset call anywhere in this test.
    var scope = try pool_elevation.begin(conn, .billing);
    defer scope.deinit();
    const inside_role = try currentRoleOwned(alloc, scope.conn);
    defer alloc.free(inside_role);
    try scope.commit();
    try std.testing.expectEqualStrings("billing_runtime", inside_role);

    const after_role = try currentRoleOwned(alloc, conn);
    defer alloc.free(after_role);
    try std.testing.expectEqualStrings("api_runtime", after_role);
    _ = try scalarI64(conn, "SELECT 1::bigint", .{});

    // And the registry sees nothing left to audit: this release will pool the
    // connection normally rather than counting a refusal.
    try std.testing.expect(pool_elevation.auditRelease(conn) == null);
}
