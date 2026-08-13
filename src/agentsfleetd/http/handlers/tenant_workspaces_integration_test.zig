//! HTTP integration tests for tenant workspace listing and create authority.
//! Requires TEST_DATABASE_URL — skipped gracefully otherwise.

const std = @import("std");
const scope_fixtures = @import("../test_scope_tokens.zig");
const clock = @import("common").clock;
const pg = @import("pg");
const api_key = @import("../../auth/api_key.zig");
const auth_mw = @import("../../auth/middleware/mod.zig");
const api_key_lookup = @import("../../cmd/api_key_lookup.zig");
const PgQuery = @import("../../db/pg_query.zig").PgQuery;

const harness_mod = @import("../test_harness.zig");
const TestHarness = harness_mod.TestHarness;

const CLAIM_TENANT_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0e6f01";
const CLAIM_WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0e6f11";
const DATABASE_TENANT_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0e6f21";
const DATABASE_USER_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0e6f41";
const API_KEY_ROW_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0e6f51";
const TOKEN_SUBJECT = "user_workspace_reconciliation";
const TOKEN_USER = scope_fixtures.WORKSPACE_RECONCILIATION_ADMIN;
const TENANT_KEY_BODY_CHARACTERS: usize = 48;
const TENANT_API_KEY =
    auth_mw.tenant_api_key.TENANT_KEY_PREFIX ++
    "e" ** TENANT_KEY_BODY_CHARACTERS;
const PAGE_LIMIT = 100;
const WORKSPACE_COUNT = 205;
const SHARED_NAME = "tenant-shared-name";

const WorkspaceResponse = struct {
    items: []const WorkspaceItem,
    tenant_id: []const u8,
    total: ?usize,
    next_cursor: ?[]const u8,
};

const WorkspaceItem = struct {
    id: []const u8,
    name: ?[]const u8,
    created_at: i64,
};

const CreateResponse = struct {
    workspace_id: []const u8,
    tenant_id: []const u8,
    name: []const u8,
};

// SAFETY: populated before TestHarness initializes the middleware chains.
var api_key_ctx: api_key_lookup.Ctx = undefined;

fn configureRegistry(reg: *auth_mw.MiddlewareRegistry, h: *TestHarness) anyerror!void {
    api_key_ctx = .{ .pool = h.pool };
    reg.tenant_api_key_mw = .{
        .host = &api_key_ctx,
        .lookup = api_key_lookup.lookup,
        // Since §6 a tenant key resolves its creator's capabilities; without a
        // resolver the key authenticates and then fails every gate behind it.
        .scope_host = &api_key_ctx,
        .resolveScopes = scope_fixtures.ownerScopes,
    };
}

fn makeHarness(alloc: std.mem.Allocator) !*TestHarness {
    return TestHarness.start(alloc, .{
        .configureRegistry = configureRegistry,
        .inline_jwks_json = scope_fixtures.JWKS,
        .issuer = scope_fixtures.ISSUER,
        .audience = scope_fixtures.AUDIENCE,
    });
}

fn seedIdentityFixtures(conn: *pg.Conn) !void {
    const now_ms = clock.nowMillis();
    _ = try conn.exec(
        \\INSERT INTO core.tenants (id, name, created_at, updated_at)
        \\VALUES ($1::uuid, 'TenantWsClaim', $3, $3), ($2, 'TenantWsMapped', $3, $3)
        \\ON CONFLICT (id) DO NOTHING
    , .{ CLAIM_TENANT_ID, DATABASE_TENANT_ID, now_ms });
    _ = try conn.exec(
        \\INSERT INTO core.users (id, tenant_id, oidc_subject, email, created_at, updated_at)
        \\VALUES ($1::uuid, $2, $3, 'tenant-workspaces@test.agentsfleet', $4, $4)
        \\ON CONFLICT (oidc_subject) DO UPDATE
        \\SET tenant_id = EXCLUDED.tenant_id, updated_at = EXCLUDED.updated_at
    , .{ DATABASE_USER_ID, DATABASE_TENANT_ID, TOKEN_SUBJECT, now_ms });
    const key_hash = api_key.sha256Hex(TENANT_API_KEY);
    _ = try conn.exec(
        \\INSERT INTO core.api_keys
        \\  (id, tenant_id, key_name, description, key_hash, created_by,
        \\   active, created_at, updated_at)
        \\VALUES ($1, $2, 'workspace-reconciliation', '', $3, $4, TRUE, $5, $5)
        \\ON CONFLICT (key_hash) DO UPDATE
        \\SET tenant_id = EXCLUDED.tenant_id,
        \\    created_by = EXCLUDED.created_by,
        \\    active = TRUE,
        \\    updated_at = EXCLUDED.updated_at
    , .{
        API_KEY_ROW_ID,
        CLAIM_TENANT_ID,
        key_hash[0..],
        TOKEN_SUBJECT,
        now_ms,
    });
}

fn cleanupFixtures(conn: *pg.Conn) void {
    _ = conn.exec(
        "DELETE FROM core.api_keys WHERE id = $1::uuid",
        .{API_KEY_ROW_ID},
    ) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
    _ = conn.exec(
        "DELETE FROM core.workspaces WHERE tenant_id IN ($1::uuid, $2::uuid)",
        .{ CLAIM_TENANT_ID, DATABASE_TENANT_ID },
    ) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
    _ = conn.exec(
        "DELETE FROM core.memberships WHERE user_id = $1::uuid",
        .{DATABASE_USER_ID},
    ) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
    _ = conn.exec(
        "DELETE FROM core.users WHERE id = $1::uuid",
        .{DATABASE_USER_ID},
    ) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
    _ = conn.exec(
        "DELETE FROM core.tenants WHERE id IN ($1::uuid, $2::uuid)",
        .{ CLAIM_TENANT_ID, DATABASE_TENANT_ID },
    ) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
}

fn countNamedRows(conn: *pg.Conn, tenant_id: []const u8, name: []const u8) !i64 {
    var query = PgQuery.from(try conn.query(
        \\SELECT COUNT(*)::BIGINT FROM core.workspaces
        \\WHERE tenant_id = $1::uuid AND name = $2
    , .{ tenant_id, name }));
    defer query.deinit();
    const row = (try query.next()) orelse return error.MissingCount;
    return row.get(i64, 0);
}

fn expectCreateUsesMappedTenant(
    h: *TestHarness,
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
) !void {
    const name = try std.fmt.allocPrint(alloc, "mapped-create-{d}", .{clock.nowMillis()});
    defer alloc.free(name);
    const body = try std.fmt.allocPrint(alloc, "{{\"name\":\"{s}\"}}", .{name});
    defer alloc.free(body);
    const response = try (try (try h.post("/v1/workspaces").bearer(TOKEN_USER)).json(body)).send();
    defer response.deinit();
    try response.expectStatus(.created);
    const parsed = try std.json.parseFromSlice(CreateResponse, alloc, response.body, .{ .ignore_unknown_fields = true });
    defer parsed.deinit();
    try std.testing.expectEqualStrings(DATABASE_TENANT_ID, parsed.value.tenant_id);
    try std.testing.expectEqualStrings(name, parsed.value.name);
    try std.testing.expectEqual(@as(i64, 0), try countNamedRows(conn, CLAIM_TENANT_ID, name));
    try std.testing.expectEqual(@as(i64, 1), try countNamedRows(conn, DATABASE_TENANT_ID, name));
}

fn seedSameNameAcrossTenants(conn: *pg.Conn) !void {
    _ = try conn.exec(
        \\INSERT INTO core.workspaces (id, tenant_id, name, created_at)
        \\VALUES ($1::uuid, $2, $5, 1), ($3, $4, $5, 2)
        \\ON CONFLICT (id) DO UPDATE
        \\SET tenant_id = EXCLUDED.tenant_id, name = EXCLUDED.name,
        \\    created_at = EXCLUDED.created_at
    , .{
        CLAIM_WORKSPACE_ID,
        CLAIM_TENANT_ID,
        "0195b4ba-8d3a-7f13-8abc-2b3e1e0e6f22",
        DATABASE_TENANT_ID,
        SHARED_NAME,
    });
}

fn expectExactNameIsolation(h: *TestHarness, alloc: std.mem.Allocator) !void {
    const response = try (try h.get(
        "/v1/tenants/me/workspaces?name=tenant-shared-name&limit=1",
    ).bearer(TOKEN_USER)).send();
    defer response.deinit();
    try response.expectStatus(.ok);
    const parsed = try std.json.parseFromSlice(WorkspaceResponse, alloc, response.body, .{ .ignore_unknown_fields = true });
    defer parsed.deinit();
    try std.testing.expectEqualStrings(DATABASE_TENANT_ID, parsed.value.tenant_id);
    try std.testing.expectEqual(@as(?usize, null), parsed.value.total);
    try std.testing.expectEqual(@as(usize, 1), parsed.value.items.len);
    try std.testing.expectEqualStrings(SHARED_NAME, parsed.value.items[0].name.?);
    try std.testing.expect(!std.mem.eql(u8, CLAIM_WORKSPACE_ID, parsed.value.items[0].id));
    try std.testing.expect(parsed.value.next_cursor == null);
}

fn expectApiKeyUsesIssuingTenant(
    h: *TestHarness,
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
) !void {
    const list_response = try (try h.get(
        "/v1/tenants/me/workspaces?name=tenant-shared-name&limit=1",
    ).bearer(TENANT_API_KEY)).send();
    defer list_response.deinit();
    try list_response.expectStatus(.ok);
    const listed = try std.json.parseFromSlice(
        WorkspaceResponse,
        alloc,
        list_response.body,
        .{ .ignore_unknown_fields = true },
    );
    defer listed.deinit();
    try std.testing.expectEqualStrings(CLAIM_TENANT_ID, listed.value.tenant_id);
    try std.testing.expectEqual(@as(usize, 1), listed.value.items.len);
    try std.testing.expectEqualStrings(CLAIM_WORKSPACE_ID, listed.value.items[0].id);

    const name = try std.fmt.allocPrint(
        alloc,
        "api-key-bound-{d}",
        .{clock.nowMillis()},
    );
    defer alloc.free(name);
    const body = try std.fmt.allocPrint(alloc, "{{\"name\":\"{s}\"}}", .{name});
    defer alloc.free(body);
    const create_response = try (try (try h.post(
        "/v1/workspaces",
    ).bearer(TENANT_API_KEY)).json(body)).send();
    defer create_response.deinit();
    try create_response.expectStatus(.created);
    const created = try std.json.parseFromSlice(
        CreateResponse,
        alloc,
        create_response.body,
        .{ .ignore_unknown_fields = true },
    );
    defer created.deinit();
    try std.testing.expectEqualStrings(CLAIM_TENANT_ID, created.value.tenant_id);
    try std.testing.expectEqual(@as(i64, 1), try countNamedRows(
        conn,
        CLAIM_TENANT_ID,
        name,
    ));
    try std.testing.expectEqual(@as(i64, 0), try countNamedRows(
        conn,
        DATABASE_TENANT_ID,
        name,
    ));
}

fn seedPaginatedWorkspaces(conn: *pg.Conn) !void {
    _ = try conn.exec("DELETE FROM core.workspaces WHERE tenant_id = $1::uuid", .{DATABASE_TENANT_ID});
    _ = try conn.exec(
        \\INSERT INTO core.workspaces (id, tenant_id, name, created_at)
        \\SELECT
        \\  ('0195b4ba-8d3a-7f13-8abc-2b3f' || lpad(to_hex(n), 8, '0'))::uuid,
        \\  $1::uuid,
        \\  'reconcile-' || n,
        \\  CASE WHEN n = 101 THEN 100 ELSE n END
        \\FROM generate_series(1, $2::integer) AS n
    , .{ DATABASE_TENANT_ID, WORKSPACE_COUNT });
}

fn expectCompletePagination(h: *TestHarness, alloc: std.mem.Allocator) !void {
    var cursor: ?[]u8 = null;
    defer if (cursor) |value| alloc.free(value);
    var expected: usize = 1;
    while (true) {
        const url = if (cursor) |value|
            try std.fmt.allocPrint(alloc, "/v1/tenants/me/workspaces?limit={d}&starting_after={s}", .{ PAGE_LIMIT, value })
        else
            try std.fmt.allocPrint(alloc, "/v1/tenants/me/workspaces?limit={d}", .{PAGE_LIMIT});
        defer alloc.free(url);
        const response = try (try h.get(url).bearer(TOKEN_USER)).send();
        defer response.deinit();
        try response.expectStatus(.ok);
        const parsed = try std.json.parseFromSlice(WorkspaceResponse, alloc, response.body, .{ .ignore_unknown_fields = true });
        defer parsed.deinit();
        try std.testing.expectEqualStrings(DATABASE_TENANT_ID, parsed.value.tenant_id);
        try std.testing.expectEqual(@as(?usize, null), parsed.value.total);
        for (parsed.value.items) |item| {
            const expected_name = try std.fmt.allocPrint(alloc, "reconcile-{d}", .{expected});
            defer alloc.free(expected_name);
            try std.testing.expectEqualStrings(expected_name, item.name.?);
            expected += 1;
        }
        if (cursor) |value| alloc.free(value);
        cursor = if (parsed.value.next_cursor) |value| try alloc.dupe(u8, value) else null;
        if (cursor == null) break;
    }
    try std.testing.expectEqual(@as(usize, WORKSPACE_COUNT + 1), expected);
}

fn expectRequestFailures(h: *TestHarness) !void {
    const viewer = try (try h.get("/v1/tenants/me/workspaces").bearer(scope_fixtures.VIEWER)).send();
    defer viewer.deinit();
    try viewer.expectStatus(.forbidden);
    const unauthenticated = try h.get("/v1/tenants/me/workspaces").send();
    defer unauthenticated.deinit();
    try unauthenticated.expectStatus(.unauthorized);
    const bad_limit = try (try h.get("/v1/tenants/me/workspaces?limit=0").bearer(TOKEN_USER)).send();
    defer bad_limit.deinit();
    try bad_limit.expectStatus(.bad_request);
    const bad_cursor = try (try h.get("/v1/tenants/me/workspaces?starting_after=broken").bearer(TOKEN_USER)).send();
    defer bad_cursor.deinit();
    try bad_cursor.expectStatus(.bad_request);
    const zero_byte_name = try (try h.get("/v1/tenants/me/workspaces?name=%00").bearer(TOKEN_USER)).send();
    defer zero_byte_name.deinit();
    try zero_byte_name.expectStatus(.bad_request);
}

test "integration: tenant workspaces reconcile create and paginate without crossing tenants" {
    const alloc = std.testing.allocator;
    const h = makeHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    cleanupFixtures(conn);
    errdefer cleanupFixtures(conn);
    try seedIdentityFixtures(conn);
    try expectCreateUsesMappedTenant(h, conn, alloc);
    try seedSameNameAcrossTenants(conn);
    try expectExactNameIsolation(h, alloc);
    try expectApiKeyUsesIssuingTenant(h, conn, alloc);
    try seedPaginatedWorkspaces(conn);
    try expectCompletePagination(h, alloc);
    try expectRequestFailures(h);
    cleanupFixtures(conn);
}
