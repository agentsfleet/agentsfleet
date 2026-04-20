//! Integration tests for signup_bootstrap.zig.
//!
//! All DB-backed tests skip when TEST_DATABASE_URL / DATABASE_URL is unset.
//! Each test uses a unique oidc_subject so parallel runs don't collide. The
//! cleanup helper walks user → memberships → tenant → workspaces (and through
//! them, credit_state / credit_audit via ON DELETE CASCADE) so reruns start
//! from a clean slate.

const std = @import("std");
const pg = @import("pg");
const PgQuery = @import("../db/pg_query.zig").PgQuery;
const bootstrap = @import("signup_bootstrap.zig");
const store = @import("signup_bootstrap_store.zig");
const base = @import("../db/test_fixtures.zig");

const COLLISION_TENANT_ID = "0195b4ba-8d3a-7f13-8abc-b00000000003";
const COLLISION_WORKSPACE_EXISTING = "0195b4ba-8d3a-7f13-8abc-b00000000004";
const COLLISION_WORKSPACE_NEW = "0195b4ba-8d3a-7f13-8abc-b00000000005";

fn cleanupBootstrappedAccount(conn: *pg.Conn, oidc_subject: []const u8) void {
    // Workspaces cascade credit_state + credit_audit; deleting tenants
    // cascades memberships on the tenant side. Users delete directly.
    _ = conn.exec(
        \\DELETE FROM core.workspaces
        \\WHERE tenant_id IN (
        \\  SELECT u.tenant_id FROM core.users u WHERE u.oidc_subject = $1
        \\)
    , .{oidc_subject}) catch {};
    _ = conn.exec(
        \\DELETE FROM core.memberships
        \\WHERE user_id IN (SELECT user_id FROM core.users WHERE oidc_subject = $1)
    , .{oidc_subject}) catch {};
    _ = conn.exec(
        \\DELETE FROM core.tenants
        \\WHERE tenant_id IN (SELECT tenant_id FROM core.users WHERE oidc_subject = $1)
    , .{oidc_subject}) catch {};
    _ = conn.exec("DELETE FROM core.users WHERE oidc_subject = $1", .{oidc_subject}) catch {};
}

fn cleanupCollisionFixture(conn: *pg.Conn) void {
    _ = conn.exec("DELETE FROM core.workspaces WHERE tenant_id = $1::uuid", .{COLLISION_TENANT_ID}) catch {};
    _ = conn.exec("DELETE FROM core.tenants WHERE tenant_id = $1::uuid", .{COLLISION_TENANT_ID}) catch {};
}

fn countTenantsForOidc(conn: *pg.Conn, oidc_subject: []const u8) !i64 {
    var q = PgQuery.from(try conn.query(
        \\SELECT COUNT(*)::BIGINT FROM core.tenants t
        \\JOIN core.users u ON u.tenant_id = t.tenant_id
        \\WHERE u.oidc_subject = $1
    , .{oidc_subject}));
    defer q.deinit();
    const row = (try q.next()) orelse return 0;
    return try row.get(i64, 0);
}

fn countUsersForOidc(conn: *pg.Conn, oidc_subject: []const u8) !i64 {
    var q = PgQuery.from(try conn.query(
        "SELECT COUNT(*)::BIGINT FROM core.users WHERE oidc_subject = $1",
        .{oidc_subject},
    ));
    defer q.deinit();
    const row = (try q.next()) orelse return 0;
    return try row.get(i64, 0);
}

test "derivePersonalTenantName: email local-part wins; empty falls back to personal" {
    // Mirror of the private helper — kept in sync with signup_bootstrap.zig.
    const cases = [_]struct { email: []const u8, want: []const u8 }{
        .{ .email = "jane@acme.com", .want = "jane" },
        .{ .email = "@dangling.example", .want = "personal" },
        .{ .email = "plainaddress", .want = "plainaddress" },
    };
    for (cases) |c| {
        const got = try derivePersonalTenantNameForTest(std.testing.allocator, c.email);
        defer std.testing.allocator.free(got);
        try std.testing.expectEqualSlices(u8, c.want, got);
    }
}

fn derivePersonalTenantNameForTest(alloc: std.mem.Allocator, email: []const u8) ![]u8 {
    const at = std.mem.indexOfScalar(u8, email, '@') orelse email.len;
    const local = email[0..at];
    if (local.len == 0) return alloc.dupe(u8, "personal");
    return alloc.dupe(u8, local);
}

// Fresh signup inserts six rows across core.* and billing.*.
test "bootstrapPersonalAccount: fresh signup provisions tenant/user/membership/workspace/credit" {
    const db_ctx = (try base.openTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);
    const oidc = "oidc-test-happy-path-01";
    cleanupBootstrappedAccount(db_ctx.conn, oidc);
    defer cleanupBootstrappedAccount(db_ctx.conn, oidc);

    var result = try bootstrap.bootstrapPersonalAccount(db_ctx.conn, std.testing.allocator, .{
        .oidc_subject = oidc,
        .email = "happy@test.zombie",
        .display_name = "Happy Path",
    });
    defer result.deinit(std.testing.allocator);

    try std.testing.expect(result.created);
    try std.testing.expect(result.user_id.len == 36);
    try std.testing.expect(result.tenant_id.len == 36);
    try std.testing.expect(result.workspace_id.len == 36);
    try std.testing.expect(result.workspace_name.len > 0);

    // Tenant name is email local-part.
    {
        var q = PgQuery.from(try db_ctx.conn.query(
            "SELECT name FROM core.tenants WHERE tenant_id = $1::uuid",
            .{result.tenant_id},
        ));
        defer q.deinit();
        const row = (try q.next()) orelse return error.TestUnexpectedResult;
        try std.testing.expectEqualSlices(u8, "happy", try row.get([]const u8, 0));
    }

    // User row has display_name + correct tenant binding.
    {
        var q = PgQuery.from(try db_ctx.conn.query(
            \\SELECT tenant_id::text, email, display_name FROM core.users
            \\WHERE oidc_subject = $1
        , .{oidc}));
        defer q.deinit();
        const row = (try q.next()) orelse return error.TestUnexpectedResult;
        try std.testing.expectEqualSlices(u8, result.tenant_id, try row.get([]const u8, 0));
        try std.testing.expectEqualSlices(u8, "happy@test.zombie", try row.get([]const u8, 1));
        try std.testing.expectEqualSlices(u8, "Happy Path", try row.get([]const u8, 2));
    }

    // Owner membership present.
    {
        var q = PgQuery.from(try db_ctx.conn.query(
            \\SELECT role FROM core.memberships
            \\WHERE tenant_id = $1::uuid AND user_id = $2::uuid
        , .{ result.tenant_id, result.user_id }));
        defer q.deinit();
        const row = (try q.next()) orelse return error.TestUnexpectedResult;
        try std.testing.expectEqualSlices(u8, "owner", try row.get([]const u8, 0));
    }

    // Credit state initialized at 0 and an audit row was written.
    {
        var q = PgQuery.from(try db_ctx.conn.query(
            \\SELECT initial_credit_cents, remaining_credit_cents, currency
            \\FROM billing.workspace_credit_state WHERE workspace_id = $1::uuid
        , .{result.workspace_id}));
        defer q.deinit();
        const row = (try q.next()) orelse return error.TestUnexpectedResult;
        try std.testing.expectEqual(@as(i64, 0), try row.get(i64, 0));
        try std.testing.expectEqual(@as(i64, 0), try row.get(i64, 1));
        try std.testing.expectEqualSlices(u8, "USD", try row.get([]const u8, 2));
    }
    {
        var q = PgQuery.from(try db_ctx.conn.query(
            \\SELECT actor, delta_credit_cents FROM billing.workspace_credit_audit
            \\WHERE workspace_id = $1::uuid
        , .{result.workspace_id}));
        defer q.deinit();
        const row = (try q.next()) orelse return error.TestUnexpectedResult;
        try std.testing.expectEqualSlices(u8, "signup_bootstrap", try row.get([]const u8, 0));
        try std.testing.expectEqual(@as(i64, 0), try row.get(i64, 1));
    }
}

// Replaying the same oidc_subject returns existing rows with created=false
// and does not insert anything new.
test "bootstrapPersonalAccount: replay returns existing rows without re-inserting" {
    const db_ctx = (try base.openTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);
    const oidc = "oidc-test-replay-02";
    cleanupBootstrappedAccount(db_ctx.conn, oidc);
    defer cleanupBootstrappedAccount(db_ctx.conn, oidc);

    var first = try bootstrap.bootstrapPersonalAccount(db_ctx.conn, std.testing.allocator, .{
        .oidc_subject = oidc,
        .email = "replay@test.zombie",
    });
    defer first.deinit(std.testing.allocator);
    try std.testing.expect(first.created);

    var second = try bootstrap.bootstrapPersonalAccount(db_ctx.conn, std.testing.allocator, .{
        .oidc_subject = oidc,
        .email = "replay@test.zombie",
    });
    defer second.deinit(std.testing.allocator);
    try std.testing.expect(!second.created);
    try std.testing.expectEqualSlices(u8, first.user_id, second.user_id);
    try std.testing.expectEqualSlices(u8, first.tenant_id, second.tenant_id);
    try std.testing.expectEqualSlices(u8, first.workspace_id, second.workspace_id);
    try std.testing.expectEqualSlices(u8, first.workspace_name, second.workspace_name);

    // Exactly one user + one tenant + one workspace after the replay.
    try std.testing.expectEqual(@as(i64, 1), try countUsersForOidc(db_ctx.conn, oidc));
    try std.testing.expectEqual(@as(i64, 1), try countTenantsForOidc(db_ctx.conn, oidc));
    var q = PgQuery.from(try db_ctx.conn.query(
        "SELECT COUNT(*)::BIGINT FROM core.workspaces WHERE tenant_id = $1::uuid",
        .{first.tenant_id},
    ));
    defer q.deinit();
    const row = (try q.next()) orelse return error.TestUnexpectedResult;
    try std.testing.expectEqual(@as(i64, 1), try row.get(i64, 0));
}

// Collision retry. Pre-seed a workspace with the name the injected
// generator yields first; verify the helper retries to the second candidate.
var collision_gen_state: usize = 0;
const COLLISION_NAMES = [_][]const u8{ "preserve-me-042", "fresh-name-999" };

fn collisionSequenceGen(alloc: std.mem.Allocator) anyerror![]u8 {
    const idx = @min(collision_gen_state, COLLISION_NAMES.len - 1);
    collision_gen_state += 1;
    return alloc.dupe(u8, COLLISION_NAMES[idx]);
}

test "pickUniqueWorkspaceName: retries past an existing (tenant_id, name) collision" {
    const db_ctx = (try base.openTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);
    cleanupCollisionFixture(db_ctx.conn);
    defer cleanupCollisionFixture(db_ctx.conn);

    _ = try db_ctx.conn.exec(
        \\INSERT INTO core.tenants (tenant_id, name, created_at, updated_at)
        \\VALUES ($1::uuid, 'collision-tenant', 0, 0)
    , .{COLLISION_TENANT_ID});
    _ = try db_ctx.conn.exec(
        \\INSERT INTO core.workspaces
        \\  (workspace_id, tenant_id, name, repo_url, default_branch, created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, 'preserve-me-042', '', 'main', 0, 0)
    , .{ COLLISION_WORKSPACE_EXISTING, COLLISION_TENANT_ID });

    collision_gen_state = 0;
    const chosen = try bootstrap.pickUniqueWorkspaceName(
        db_ctx.conn,
        std.testing.allocator,
        COLLISION_TENANT_ID,
        COLLISION_WORKSPACE_NEW,
        "test",
        123,
        collisionSequenceGen,
    );
    defer std.testing.allocator.free(chosen);
    try std.testing.expectEqualSlices(u8, "fresh-name-999", chosen);

    // Both workspaces exist, the new row has the second name.
    var q = PgQuery.from(try db_ctx.conn.query(
        \\SELECT name FROM core.workspaces
        \\WHERE tenant_id = $1::uuid ORDER BY name
    , .{COLLISION_TENANT_ID}));
    defer q.deinit();
    const row1 = (try q.next()) orelse return error.TestUnexpectedResult;
    try std.testing.expectEqualSlices(u8, "fresh-name-999", try row1.get([]const u8, 0));
    const row2 = (try q.next()) orelse return error.TestUnexpectedResult;
    try std.testing.expectEqualSlices(u8, "preserve-me-042", try row2.get([]const u8, 0));
}

// Transactional rollback. The handler's fast-path replay check is bypassed
// here by pre-seeding a `core.users` row with the target oidc_subject BEFORE
// calling bootstrapTransaction, so the UNIQUE(oidc_subject) constraint fires
// on the user INSERT inside the transaction. errdefer must roll back the
// already-inserted tenant row — proving any mid-transaction failure unwinds
// the entire bootstrap.
test "bootstrapTransaction: unique-violation mid-tx rolls back every preceding INSERT" {
    const db_ctx = (try base.openTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);
    const oidc = "oidc-test-rollback-04";
    cleanupBootstrappedAccount(db_ctx.conn, oidc);
    defer cleanupBootstrappedAccount(db_ctx.conn, oidc);

    // Pre-seed user with the target oidc_subject under a throwaway tenant.
    // This is the poison pill — the bootstrap's user INSERT will collide.
    const poison_tenant = "0195b4ba-8d3a-7f13-8abc-b00000000099";
    const poison_user = "0195b4ba-8d3a-7f13-8abc-b00000000098";
    defer {
        _ = db_ctx.conn.exec("DELETE FROM core.memberships WHERE user_id = $1::uuid", .{poison_user}) catch {};
        _ = db_ctx.conn.exec("DELETE FROM core.users WHERE user_id = $1::uuid", .{poison_user}) catch {};
        _ = db_ctx.conn.exec("DELETE FROM core.tenants WHERE tenant_id = $1::uuid", .{poison_tenant}) catch {};
    }
    _ = try db_ctx.conn.exec(
        \\INSERT INTO core.tenants (tenant_id, name, created_at, updated_at)
        \\VALUES ($1::uuid, 'poison-tenant', 0, 0)
    , .{poison_tenant});
    _ = try db_ctx.conn.exec(
        \\INSERT INTO core.users (user_id, tenant_id, oidc_subject, email, created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, $3, 'poison@test.zombie', 0, 0)
    , .{ poison_user, poison_tenant, oidc });

    // Snapshot tenant count globally so we can prove the bootstrap's tenant
    // was rolled back (not just absent via some filter quirk).
    const tenant_count_before = blk: {
        var q = PgQuery.from(try db_ctx.conn.query("SELECT COUNT(*)::BIGINT FROM core.tenants", .{}));
        defer q.deinit();
        const row = (try q.next()) orelse return error.TestUnexpectedResult;
        break :blk try row.get(i64, 0);
    };

    // Call bootstrapTransaction directly so the fast-path replay check is
    // not consulted. The user INSERT will raise unique_violation and
    // errdefer should ROLLBACK the whole tx.
    const err = bootstrap.bootstrapTransaction(
        db_ctx.conn,
        std.testing.allocator,
        .{ .oidc_subject = oidc, .email = "rollback@test.zombie" },
        bootstrap.defaultHerokuNameGen,
    );
    try std.testing.expect(std.meta.isError(err));

    // Post-rollback invariants.
    const tenant_count_after = blk: {
        var q = PgQuery.from(try db_ctx.conn.query("SELECT COUNT(*)::BIGINT FROM core.tenants", .{}));
        defer q.deinit();
        const row = (try q.next()) orelse return error.TestUnexpectedResult;
        break :blk try row.get(i64, 0);
    };
    try std.testing.expectEqual(tenant_count_before, tenant_count_after);

    // Only the poison user exists for this oidc_subject; no bootstrap user.
    try std.testing.expectEqual(@as(i64, 1), try countUsersForOidc(db_ctx.conn, oidc));

    // No workspaces created for the poison tenant (the bootstrap's workspace
    // would have gone under a fresh tenant, which itself was rolled back).
    var q = PgQuery.from(try db_ctx.conn.query(
        "SELECT COUNT(*)::BIGINT FROM core.workspaces WHERE tenant_id = $1::uuid",
        .{poison_tenant},
    ));
    defer q.deinit();
    const row = (try q.next()) orelse return error.TestUnexpectedResult;
    try std.testing.expectEqual(@as(i64, 0), try row.get(i64, 0));
}

// Compile-time coverage of the store surface so it's exercised even when the
// DB tests are skipped.
test "signup_bootstrap_store: public API resolves at compile time" {
    _ = store.findExistingByOidcSubject;
    _ = store.insertTenant;
    _ = store.insertUser;
    _ = store.insertMembership;
    _ = store.tryInsertWorkspace;
}
