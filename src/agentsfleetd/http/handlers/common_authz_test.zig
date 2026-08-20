const std = @import("std");
const pg = @import("pg");
const common = @import("common.zig");
const scopes = @import("../../auth/scopes.zig");
const PgQuery = @import("../../db/pg_query.zig").PgQuery;
const http_auth = @import("../../db/test_fixtures_http_auth.zig");
const constants = @import("common");

const MAPPED_USER_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f41";
const MAPPED_SUBJECT = "common-authz-stale-tenant-subject";
const MAPPED_EMAIL = "common-authz-stale-tenant@test.agentsfleet";
const MAPPED_CREATED_AT: i64 = 1_700_000_000_000;

fn cleanupMappedUser(conn: *pg.Conn) !void {
    _ = try conn.exec(
        "DELETE FROM core.users WHERE id = $1::uuid OR oidc_subject = $2",
        .{ MAPPED_USER_ID, MAPPED_SUBJECT },
    );
}

test "integration: workspace:any bypasses the tenant match; a non-holder is tenant-bound" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    http_auth.cleanup(db_ctx.conn);
    defer http_auth.cleanup(db_ctx.conn);

    try http_auth.seedTenant(db_ctx.conn);
    try http_auth.seedScopeWorkspace(db_ctx.conn, http_auth.WS_PRIMARY); // owned by TENANT_ID

    // A platform operator authenticated in a DIFFERENT tenant, holding the
    // audited cross-tenant override, reaches TENANT_ID's workspace.
    const operator = common.AuthPrincipal{
        .mode = .jwt_oidc,
        .tenant_id = http_auth.TENANT_UNRELATED,
        .scopes = scopes.parseClaim("workspace:any"),
    };
    try std.testing.expect(common.authorizeWorkspace(db_ctx.conn, operator, http_auth.WS_PRIMARY));

    // Capability ≠ ownership (Dimension 2.3): even holding fleet:admin, a
    // cross-tenant principal WITHOUT workspace:any is denied — the capability
    // axis (scopes) does not grant the ownership axis. Tenant isolation is
    // otherwise unchanged.
    const stranger = common.AuthPrincipal{
        .mode = .jwt_oidc,
        .tenant_id = http_auth.TENANT_UNRELATED,
        .scopes = scopes.parseClaim("fleet:admin"),
    };
    try std.testing.expect(!common.authorizeWorkspace(db_ctx.conn, stranger, http_auth.WS_PRIMARY));
}

test "integration: authoritative OIDC tenant authorizes workspace when token tenant is stale" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    http_auth.cleanup(db_ctx.conn);
    try cleanupMappedUser(db_ctx.conn);
    defer http_auth.cleanup(db_ctx.conn);
    defer cleanupMappedUser(db_ctx.conn) catch unreachable;

    try http_auth.seedTenant(db_ctx.conn);
    try http_auth.seedScopeWorkspace(db_ctx.conn, http_auth.WS_PRIMARY);
    _ = try db_ctx.conn.exec(
        \\INSERT INTO core.users
        \\  (id, tenant_id, oidc_subject, email, created_at, updated_at)
        \\VALUES ($1::uuid, $2, $3, $4, $5, $5)
    , .{
        MAPPED_USER_ID,
        http_auth.TENANT_ID,
        MAPPED_SUBJECT,
        MAPPED_EMAIL,
        MAPPED_CREATED_AT,
    });

    const principal = common.AuthPrincipal{
        .mode = .jwt_oidc,
        .user_id = MAPPED_SUBJECT,
        .tenant_id = http_auth.TENANT_UNRELATED,
    };
    try std.testing.expect(common.authorizeWorkspace(db_ctx.conn, principal, http_auth.WS_PRIMARY));
    try std.testing.expect(common.authorizeWorkspaceAndSetTenantContext(
        db_ctx.conn,
        principal,
        http_auth.WS_PRIMARY,
    ));

    var q = PgQuery.from(try db_ctx.conn.query(
        "SELECT current_setting('app.current_tenant_id', true)",
        .{},
    ));
    defer q.deinit();
    const row = (try q.next()) orelse return error.TestUnexpectedResult;
    const current_tenant = try row.get(?[]const u8, 0);
    try std.testing.expectEqualStrings(http_auth.TENANT_ID, current_tenant.?);
}

test "integration: oidc workspace scoping blocks cross-workspace access" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    http_auth.cleanup(db_ctx.conn);
    defer http_auth.cleanup(db_ctx.conn);

    try http_auth.seedTenant(db_ctx.conn);
    try http_auth.seedScopeWorkspace(db_ctx.conn, http_auth.WS_PRIMARY);
    try http_auth.seedScopeWorkspace(db_ctx.conn, http_auth.WS_SECONDARY);

    const principal = common.AuthPrincipal{
        .mode = .jwt_oidc,
        .tenant_id = http_auth.TENANT_ID,
        .workspace_scope_id = http_auth.WS_PRIMARY,
    };
    try std.testing.expect(common.authorizeWorkspace(db_ctx.conn, principal, http_auth.WS_PRIMARY));
    try std.testing.expect(!common.authorizeWorkspace(db_ctx.conn, principal, http_auth.WS_SECONDARY));
}

test "integration: tenant-wide principal without a workspace claim authorizes any workspace in its tenant" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    http_auth.cleanup(db_ctx.conn);
    defer http_auth.cleanup(db_ctx.conn);

    try http_auth.seedTenant(db_ctx.conn);
    try http_auth.seedScopeWorkspace(db_ctx.conn, http_auth.WS_PRIMARY);
    try http_auth.seedScopeWorkspace(db_ctx.conn, http_auth.WS_SECONDARY);

    // A principal whose token carries tenant_id but NO workspace claim is
    // tenant-scoped, not workspace-scoped: it authorizes every workspace under
    // its tenant (the workspace_scope_id == null branch — distinct from the
    // scoped oidc case above, which this used to duplicate). Cross-tenant denial
    // is covered by the null-tenant and tenant-mismatch tests.
    const principal = common.AuthPrincipal{
        .mode = .jwt_oidc,
        .tenant_id = http_auth.TENANT_ID,
    };
    try std.testing.expect(common.authorizeWorkspace(db_ctx.conn, principal, http_auth.WS_PRIMARY));
    try std.testing.expect(common.authorizeWorkspace(db_ctx.conn, principal, http_auth.WS_SECONDARY));
}

test "integration: null-tenant principal is denied workspace authorization (IDOR fail-closed)" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    http_auth.cleanup(db_ctx.conn);
    defer http_auth.cleanup(db_ctx.conn);

    try http_auth.seedTenant(db_ctx.conn);
    try http_auth.seedScopeWorkspace(db_ctx.conn, http_auth.WS_PRIMARY);

    // A principal with no tenant (unprovisioned Clerk session window) must be
    // denied even against a workspace that exists. Pre-fix, the null-tenant
    // branch ran an unscoped existence check and returned true — cross-tenant IDOR.
    const null_tenant = common.AuthPrincipal{ .mode = .jwt_oidc, .tenant_id = null };
    try std.testing.expect(!common.authorizeWorkspace(db_ctx.conn, null_tenant, http_auth.WS_PRIMARY));

    // Positive control: the same workspace with the correct tenant still authorizes,
    // proving the guard rejects only the missing-tenant case, not legitimate access.
    const ok = common.AuthPrincipal{ .mode = .jwt_oidc, .tenant_id = http_auth.TENANT_ID };
    try std.testing.expect(common.authorizeWorkspace(db_ctx.conn, ok, http_auth.WS_PRIMARY));
}

test "integration: a denied authorize-with-context leaves app.current_tenant_id untouched" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    http_auth.cleanup(db_ctx.conn);
    defer http_auth.cleanup(db_ctx.conn);

    try http_auth.seedTenant(db_ctx.conn);
    try http_auth.seedScopeWorkspace(db_ctx.conn, http_auth.WS_PRIMARY);

    // Seed a sentinel context, then have a cross-tenant stranger (no bypass
    // scope) fail the context-writing authorize. The sentinel must survive:
    // set_config lives inside the WHERE-gated row, so a deny writes nothing.
    const sentinel = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f99";
    try std.testing.expect(common.setTenantSessionContext(db_ctx.conn, sentinel));

    const stranger = common.AuthPrincipal{
        .mode = .jwt_oidc,
        .tenant_id = http_auth.TENANT_UNRELATED,
    };
    try std.testing.expect(!common.authorizeWorkspaceAndSetTenantContext(
        db_ctx.conn,
        stranger,
        http_auth.WS_PRIMARY,
    ));

    var q = PgQuery.from(try db_ctx.conn.query(
        "SELECT current_setting('app.current_tenant_id', true)",
        .{},
    ));
    defer q.deinit();
    const row = (try q.next()) orelse return error.TestUnexpectedResult;
    const current_tenant = try row.get(?[]const u8, 0);
    try std.testing.expectEqualStrings(sentinel, current_tenant.?);
}

test "integration: malformed tenant claim degrades to absent, never a statement error" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    http_auth.cleanup(db_ctx.conn);
    try cleanupMappedUser(db_ctx.conn);
    defer http_auth.cleanup(db_ctx.conn);
    defer cleanupMappedUser(db_ctx.conn) catch unreachable;

    try http_auth.seedTenant(db_ctx.conn);
    try http_auth.seedScopeWorkspace(db_ctx.conn, http_auth.WS_PRIMARY);

    // No user row + a claim that is not a UUID: the claim can only ever deny,
    // so it is treated as absent — a clean deny, not a cast error surfacing 500.
    const claim_only = common.AuthPrincipal{
        .mode = .jwt_oidc,
        .tenant_id = "not-a-uuid",
    };
    try std.testing.expect(!common.authorizeWorkspace(db_ctx.conn, claim_only, http_auth.WS_PRIMARY));

    // With an authoritative user row present, the malformed claim is irrelevant:
    // the user-row arm decides and the request authorizes.
    _ = try db_ctx.conn.exec(
        \\INSERT INTO core.users
        \\  (id, tenant_id, oidc_subject, email, created_at, updated_at)
        \\VALUES ($1::uuid, $2, $3, $4, $5, $5)
    , .{
        MAPPED_USER_ID,
        http_auth.TENANT_ID,
        MAPPED_SUBJECT,
        MAPPED_EMAIL,
        MAPPED_CREATED_AT,
    });
    const mapped_with_bad_claim = common.AuthPrincipal{
        .mode = .jwt_oidc,
        .user_id = MAPPED_SUBJECT,
        .tenant_id = "not-a-uuid",
    };
    try std.testing.expect(common.authorizeWorkspace(db_ctx.conn, mapped_with_bad_claim, http_auth.WS_PRIMARY));
}

test "integration: claim-bound api-key principal authorizes its own workspace and no other" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    http_auth.cleanup(db_ctx.conn);
    defer http_auth.cleanup(db_ctx.conn);

    try http_auth.seedTenant(db_ctx.conn);
    try http_auth.seedScopeWorkspace(db_ctx.conn, http_auth.WS_PRIMARY);

    const key_principal = common.AuthPrincipal{
        .mode = .api_key,
        .tenant_id = http_auth.TENANT_ID,
    };
    try std.testing.expect(common.authorizeWorkspace(db_ctx.conn, key_principal, http_auth.WS_PRIMARY));

    const foreign_key = common.AuthPrincipal{
        .mode = .api_key,
        .tenant_id = http_auth.TENANT_UNRELATED,
    };
    try std.testing.expect(!common.authorizeWorkspace(db_ctx.conn, foreign_key, http_auth.WS_PRIMARY));
}

test "integration: runner principal is denied workspace authorization outright" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    http_auth.cleanup(db_ctx.conn);
    defer http_auth.cleanup(db_ctx.conn);

    try http_auth.seedTenant(db_ctx.conn);
    try http_auth.seedScopeWorkspace(db_ctx.conn, http_auth.WS_PRIMARY);

    // A runner token names a machine, not a tenant — even one whose row was
    // seeded with a tenant claim must never satisfy a workspace route.
    const runner = common.AuthPrincipal{
        .mode = .runner,
        .tenant_id = http_auth.TENANT_ID,
    };
    try std.testing.expect(!common.authorizeWorkspace(db_ctx.conn, runner, http_auth.WS_PRIMARY));
}

test "integration: tenant context helper writes app.current_tenant_id" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try std.testing.expect(common.setTenantSessionContext(db_ctx.conn, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f21"));
    var q = PgQuery.from(try db_ctx.conn.query(
        "SELECT current_setting('app.current_tenant_id', true)",
        .{},
    ));
    defer q.deinit();
    const row = (try q.next()) orelse return error.TestUnexpectedResult;
    const current_tenant = try row.get(?[]const u8, 0);
    try std.testing.expect(current_tenant != null);
    try std.testing.expectEqualStrings("0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f21", current_tenant.?);
}

test "integration: RLS policy enforces tenant session isolation" {
    if (constants.env.testLiveValue("HANDLER_DB_TEST_NONSUPERUSER") == null) return error.SkipZigTest;
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try createRlsProbe(db_ctx.conn);
    try std.testing.expect(common.setTenantSessionContext(db_ctx.conn, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f31"));
    _ = try db_ctx.conn.exec("INSERT INTO rls_probe (tenant_id, value) VALUES ('0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f31', 'a1')", .{});
    try std.testing.expect(common.setTenantSessionContext(db_ctx.conn, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f32"));
    _ = try db_ctx.conn.exec("INSERT INTO rls_probe (tenant_id, value) VALUES ('0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f32', 'b1')", .{});

    try std.testing.expect(common.setTenantSessionContext(db_ctx.conn, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f31"));
    var count_q = PgQuery.from(try db_ctx.conn.query("SELECT COUNT(*)::BIGINT FROM rls_probe", .{}));
    defer count_q.deinit();
    const row = (try count_q.next()) orelse return error.TestUnexpectedResult;
    const visible_rows = try row.get(i64, 0);
    try std.testing.expectEqual(@as(i64, 1), visible_rows);
}

fn createRlsProbe(conn: *pg.Conn) !void {
    _ = try conn.exec(
        \\CREATE TEMP TABLE rls_probe (
        \\  tenant_id UUID NOT NULL,
        \\  value TEXT NOT NULL
        \\)
    , .{});
    _ = try conn.exec("ALTER TABLE rls_probe ENABLE ROW LEVEL SECURITY", .{});
    _ = try conn.exec("ALTER TABLE rls_probe FORCE ROW LEVEL SECURITY", .{});
    _ = try conn.exec(
        \\CREATE POLICY rls_probe_select_tenant ON rls_probe
        \\FOR SELECT USING (tenant_id::text = current_setting('app.current_tenant_id', true))
    , .{});
    _ = try conn.exec(
        \\CREATE POLICY rls_probe_insert_tenant ON rls_probe
        \\FOR INSERT WITH CHECK (tenant_id::text = current_setting('app.current_tenant_id', true))
    , .{});
}
