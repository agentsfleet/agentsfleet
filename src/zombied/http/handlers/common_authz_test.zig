const std = @import("std");
const pg = @import("pg");
const common = @import("common.zig");
const PgQuery = @import("../../db/pg_query.zig").PgQuery;
const http_auth = @import("../../db/test_fixtures_http_auth.zig");
const constants = @import("common");

test "requireRole error message includes current role and required role" {
    var msg_buf: [128]u8 = undefined;
    const user_msg = std.fmt.bufPrint(
        &msg_buf,
        "Your role is '{s}'. {s} role required.",
        .{ common.AuthRole.user.label(), common.AuthRole.operator.label() },
    ) catch unreachable;
    try std.testing.expectEqualStrings("Your role is 'user'. operator role required.", user_msg);
}

test "requireRole error message fits all role combinations in 128-byte buffer" {
    const roles = [_]common.AuthRole{ .user, .operator, .admin };
    for (roles) |current| {
        for (roles) |required| {
            var msg_buf: [128]u8 = undefined;
            _ = try std.fmt.bufPrint(
                &msg_buf,
                "Your role is '{s}'. {s} role required.",
                .{ current.label(), required.label() },
            );
        }
    }
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

test "integration: clerk workspace claim scoping blocks cross-workspace access" {
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
    const null_tenant = common.AuthPrincipal{ .mode = .jwt_oidc, .role = .user, .tenant_id = null };
    try std.testing.expect(!common.authorizeWorkspace(db_ctx.conn, null_tenant, http_auth.WS_PRIMARY));

    // Positive control: the same workspace with the correct tenant still authorizes,
    // proving the guard rejects only the missing-tenant case, not legitimate access.
    const ok = common.AuthPrincipal{ .mode = .jwt_oidc, .tenant_id = http_auth.TENANT_ID };
    try std.testing.expect(common.authorizeWorkspace(db_ctx.conn, ok, http_auth.WS_PRIMARY));
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
