// Admin model-caps CRUD + platform-default authz/behaviour over the live HTTP
// surface. Platform-admin-gated: a `platform_admin` JWT passes; a tenant-admin
// JWT is 403. Catalogue mutations repopulate; the platform default is validated
// against the catalogue and kept single-active.
//
// DB-backed: needs TEST_DATABASE_URL — skipped gracefully otherwise (TestHarness
// returns error.SkipZigTest). JWKS + tokens are the shared offline fixtures (same
// keypair as runner_enrollment_integration_test.zig); `exp` is 2100 so they never
// age out.

const std = @import("std");
const scope_fixtures = @import("../../test_scope_tokens.zig");
const clock = @import("common").clock;
const auth_mw = @import("../../../auth/middleware/mod.zig");
const error_registry = @import("../../../errors/error_registry.zig");
const PgQuery = @import("../../../db/pg_query.zig").PgQuery;
const model_rate_cache = @import("../../../state/model_rate_cache.zig");
const harness_mod = @import("../../test_harness.zig");
const TestHarness = harness_mod.TestHarness;

const ALLOC = std.testing.allocator;

const TEST_ISSUER = scope_fixtures.ISSUER;
const TEST_AUDIENCE = scope_fixtures.AUDIENCE;

const TENANT_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01";
const WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11";
// Pre-seeded catalogue rows (known uids so PATCH/DELETE can address them).
const UID_GLM = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a9001";
const UID_OPUS = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a9002";
// id for the direct-insert FK probe (uuidv7 — ck_platform_provider_defaults_uid_uuidv7).
const FK_GHOST_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a9201";

const TEST_JWKS = scope_fixtures.JWKS;
const PLATFORM_ADMIN_TOKEN = scope_fixtures.PLATFORM_ADMIN;
const TENANT_ADMIN_TOKEN = scope_fixtures.TENANT_ADMIN;

fn configureRegistry(_: *auth_mw.MiddlewareRegistry, _: *TestHarness) anyerror!void {}

pub fn startHarness(alloc: std.mem.Allocator) !*TestHarness {
    return TestHarness.start(alloc, .{
        .configureRegistry = configureRegistry,
        .inline_jwks_json = TEST_JWKS,
        .issuer = TEST_ISSUER,
        .audience = TEST_AUDIENCE,
    });
}

fn seedTenantWorkspace(h: *TestHarness) !void {
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    const now = clock.nowMillis();
    _ = try conn.exec(
        "INSERT INTO core.tenants (tenant_id, name, created_at, updated_at) VALUES ($1::uuid, 'M100 Test', $2, $2) ON CONFLICT (tenant_id) DO NOTHING",
        .{ TENANT_ID, now },
    );
    _ = try conn.exec(
        "INSERT INTO core.workspaces (workspace_id, tenant_id, name, created_at) VALUES ($1::uuid, $2::uuid, 'm100-ws', $3) ON CONFLICT (workspace_id) DO NOTHING",
        .{ WORKSPACE_ID, TENANT_ID, now },
    );
}

fn seedModel(h: *TestHarness, uid: []const u8, provider: []const u8, model_id: []const u8) !void {
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    const now = clock.nowMillis();
    _ = try conn.exec(
        \\INSERT INTO core.model_library
        \\  (uid, model_id, provider, context_cap_tokens, input_nanos_per_mtok, cached_input_nanos_per_mtok, output_nanos_per_mtok, created_at_ms, updated_at_ms)
        \\VALUES ($1::uuid, $2, $3, 128000, 1, 0, 2, $4, $4)
        \\ON CONFLICT (provider, model_id) DO NOTHING
    , .{ uid, model_id, provider, now });
}

pub fn cleanup(h: *TestHarness) void {
    const conn = h.acquireConn() catch return;
    defer h.releaseConn(conn);
    _ = conn.exec("DELETE FROM core.platform_provider_defaults WHERE source_workspace_id = $1::uuid", .{WORKSPACE_ID}) catch |err| std.log.warn("cleanup ignored: {s}", .{@errorName(err)});
    _ = conn.exec("DELETE FROM core.model_library WHERE provider IN ('fireworks','anthropic','m100test')", .{}) catch |err| std.log.warn("cleanup ignored: {s}", .{@errorName(err)});
    _ = conn.exec("DELETE FROM core.workspaces WHERE workspace_id = $1::uuid", .{WORKSPACE_ID}) catch |err| std.log.warn("cleanup ignored: {s}", .{@errorName(err)});
}

fn countActivePlatformKeys(h: *TestHarness) !struct { total: i64, provider: []const u8 } {
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    var q = PgQuery.from(try conn.query("SELECT count(*)::bigint, coalesce(max(provider), '') FROM core.platform_provider_defaults WHERE active = true", .{}));
    defer q.deinit();
    const row = (try q.next()) orelse return error.TestUnexpectedResult;
    const total = try row.get(i64, 0);
    const provider = try ALLOC.dupe(u8, try row.get([]const u8, 1));
    return .{ .total = total, .provider = provider };
}

const CREATE_BODY =
    \\{"provider":"m100test","model_id":"alpha-1","context_cap_tokens":128000,"input_nanos_per_mtok":550000000,"cached_input_nanos_per_mtok":140000000,"output_nanos_per_mtok":2190000000}
;

test "admin models: platform_admin POST creates a priced row, GET lists it" {
    const h = try startHarness(ALLOC);
    defer h.deinit();
    defer cleanup(h);

    const created = try (try (try h.post("/v1/admin/models").bearer(PLATFORM_ADMIN_TOKEN)).json(CREATE_BODY)).send();
    defer created.deinit();
    try created.expectStatus(.created);

    const list = try (try h.get("/v1/admin/models").bearer(PLATFORM_ADMIN_TOKEN)).send();
    defer list.deinit();
    try list.expectStatus(.ok);
    try std.testing.expect(list.bodyContains("alpha-1"));
    try std.testing.expect(list.bodyContains("m100test"));
}

test "admin models: a duplicate (provider, model_id) POST is rejected 409" {
    const h = try startHarness(ALLOC);
    defer h.deinit();
    defer cleanup(h);

    const first = try (try (try h.post("/v1/admin/models").bearer(PLATFORM_ADMIN_TOKEN)).json(CREATE_BODY)).send();
    defer first.deinit();
    try first.expectStatus(.created);

    const dup = try (try (try h.post("/v1/admin/models").bearer(PLATFORM_ADMIN_TOKEN)).json(CREATE_BODY)).send();
    defer dup.deinit();
    try dup.expectStatus(.conflict);
    try dup.expectErrorCode(error_registry.ERR_MODEL_CAP_EXISTS);
}

test "admin models: a tenant-admin JWT without platform_admin is rejected 403" {
    const h = try startHarness(ALLOC);
    defer h.deinit();
    defer cleanup(h);

    const r = try (try (try h.post("/v1/admin/models").bearer(TENANT_ADMIN_TOKEN)).json(CREATE_BODY)).send();
    defer r.deinit();
    try r.expectStatus(.forbidden);
    try r.expectErrorCode(error_registry.ERR_INSUFFICIENT_SCOPE);
}

test "admin models: PATCH updates rates; DELETE removes the row" {
    const h = try startHarness(ALLOC);
    defer h.deinit();
    defer cleanup(h);
    try seedModel(h, UID_GLM, "fireworks", "glm-5.2");

    const patch = try (try (try h.request(.PATCH, "/v1/admin/models/" ++ UID_GLM).bearer(PLATFORM_ADMIN_TOKEN))
        .json("{\"context_cap_tokens\":200000,\"input_nanos_per_mtok\":2,\"cached_input_nanos_per_mtok\":0,\"output_nanos_per_mtok\":3}")).send();
    defer patch.deinit();
    try patch.expectStatus(.ok);

    const del = try (try h.delete("/v1/admin/models/" ++ UID_GLM).bearer(PLATFORM_ADMIN_TOKEN)).send();
    defer del.deinit();
    try del.expectStatus(.no_content);
}

test "admin models: PATCH/DELETE of an unknown uid is 404" {
    const h = try startHarness(ALLOC);
    defer h.deinit();
    defer cleanup(h);

    const del = try (try h.delete("/v1/admin/models/" ++ UID_GLM).bearer(PLATFORM_ADMIN_TOKEN)).send();
    defer del.deinit();
    try del.expectStatus(.not_found);
    try del.expectErrorCode(error_registry.ERR_MODEL_CAP_NOT_FOUND);
}

test "platform default: an uncatalogued model is rejected 400" {
    const h = try startHarness(ALLOC);
    defer h.deinit();
    defer cleanup(h);
    try seedTenantWorkspace(h);

    const body = "{\"provider\":\"fireworks\",\"source_workspace_id\":\"" ++ WORKSPACE_ID ++ "\",\"model\":\"not-in-catalogue\"}";
    const r = try (try (try h.put("/v1/admin/platform-keys").bearer(PLATFORM_ADMIN_TOKEN)).json(body)).send();
    defer r.deinit();
    try r.expectStatus(.bad_request);
    try r.expectErrorCode(error_registry.ERR_PROVIDER_MODEL_NOT_IN_CATALOGUE);
}

test "platform default: setting a second provider leaves exactly one active row" {
    const h = try startHarness(ALLOC);
    defer h.deinit();
    defer cleanup(h);
    try seedTenantWorkspace(h);
    try seedModel(h, UID_GLM, "fireworks", "glm-5.2");
    try seedModel(h, UID_OPUS, "anthropic", "claude-opus-4-8");

    const a = try (try (try h.put("/v1/admin/platform-keys").bearer(PLATFORM_ADMIN_TOKEN))
        .json("{\"provider\":\"fireworks\",\"source_workspace_id\":\"" ++ WORKSPACE_ID ++ "\",\"model\":\"glm-5.2\"}")).send();
    defer a.deinit();
    try a.expectStatus(.ok);

    const b = try (try (try h.put("/v1/admin/platform-keys").bearer(PLATFORM_ADMIN_TOKEN))
        .json("{\"provider\":\"anthropic\",\"source_workspace_id\":\"" ++ WORKSPACE_ID ++ "\",\"model\":\"claude-opus-4-8\"}")).send();
    defer b.deinit();
    try b.expectStatus(.ok);

    const active = try countActivePlatformKeys(h);
    defer ALLOC.free(active.provider);
    try std.testing.expectEqual(@as(i64, 1), active.total);
    try std.testing.expectEqualStrings("anthropic", active.provider);
}

test "platform default: GET returns the active row's model for the Edit-default pre-fill" {
    const h = try startHarness(ALLOC);
    defer h.deinit();
    defer cleanup(h);
    try seedTenantWorkspace(h);
    try seedModel(h, UID_GLM, "fireworks", "glm-5.2");

    const set = try (try (try h.put("/v1/admin/platform-keys").bearer(PLATFORM_ADMIN_TOKEN))
        .json("{\"provider\":\"fireworks\",\"source_workspace_id\":\"" ++ WORKSPACE_ID ++ "\",\"model\":\"glm-5.2\"}")).send();
    defer set.deinit();
    try set.expectStatus(.ok);

    // The active row now carries its priced model in the GET body — the field the
    // admin UI reads to pre-fill the "Edit default" dialog. Before SELECT_KEYS
    // carried `model`, the GET exposed only provider/active and the pre-fill was
    // unreachable.
    const list = try (try h.get("/v1/admin/platform-keys").bearer(PLATFORM_ADMIN_TOKEN)).send();
    defer list.deinit();
    try list.expectStatus(.ok);
    try std.testing.expect(list.bodyContains("\"model\""));
    try std.testing.expect(list.bodyContains("glm-5.2"));
}

test "admin models: deleting the active default's model is blocked 409" {
    const h = try startHarness(ALLOC);
    defer h.deinit();
    defer cleanup(h);
    try seedTenantWorkspace(h);
    try seedModel(h, UID_GLM, "fireworks", "glm-5.2");

    const set = try (try (try h.put("/v1/admin/platform-keys").bearer(PLATFORM_ADMIN_TOKEN))
        .json("{\"provider\":\"fireworks\",\"source_workspace_id\":\"" ++ WORKSPACE_ID ++ "\",\"model\":\"glm-5.2\"}")).send();
    defer set.deinit();
    try set.expectStatus(.ok);

    const del = try (try h.delete("/v1/admin/models/" ++ UID_GLM).bearer(PLATFORM_ADMIN_TOKEN)).send();
    defer del.deinit();
    try del.expectStatus(.conflict);
    try del.expectErrorCode(error_registry.ERR_MODEL_CAP_IN_USE);
}

test "platform default FK: a platform_provider_defaults row cannot reference an uncatalogued model" {
    const h = try startHarness(ALLOC);
    defer h.deinit();
    defer cleanup(h);
    try seedTenantWorkspace(h);

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    const now = clock.nowMillis();
    // Direct insert, bypassing the handler's capFor pre-check: (ghostprov,
    // ghost-model) is not a core.model_library row, so fk_platform_provider_defaults_model
    // must reject it. This is the DB-level guarantee that makes the model-delete
    // vs default-set race unwinnable — the app guard alone is not race-tight.
    if (conn.exec(
        \\INSERT INTO core.platform_provider_defaults
        \\  (id, provider, source_workspace_id, model, context_cap_tokens, active, created_at, updated_at)
        \\VALUES ($1::uuid, 'ghostprov', $2::uuid, 'ghost-model', 128000, true, $3, $3)
    , .{ FK_GHOST_ID, WORKSPACE_ID, now })) |_| {
        return error.FkShouldHaveRejectedUncataloguedModel;
    } else |_| {
        // Must be a Postgres foreign_key_violation (SQLSTATE 23503), not an
        // incidental failure — a malformed uid or the workspace FK would also
        // throw and falsely "prove" enforcement. The driver carries the sqlstate
        // on conn.err.?.code (see signup_bootstrap.zig / fleet_memory_role_test).
        const pg_err = conn.err orelse return error.ExpectedPgError;
        try std.testing.expectEqualStrings("23503", pg_err.code);
    }
}

test "platform default: standing a provider down NULLs its model, freeing its catalogue row to delete" {
    const h = try startHarness(ALLOC);
    defer h.deinit();
    defer cleanup(h);
    try seedTenantWorkspace(h);
    try seedModel(h, UID_GLM, "fireworks", "glm-5.2");
    try seedModel(h, UID_OPUS, "anthropic", "claude-opus-4-8");

    // fireworks becomes the default — its row references glm-5.2.
    const a = try (try (try h.put("/v1/admin/platform-keys").bearer(PLATFORM_ADMIN_TOKEN))
        .json("{\"provider\":\"fireworks\",\"source_workspace_id\":\"" ++ WORKSPACE_ID ++ "\",\"model\":\"glm-5.2\"}")).send();
    defer a.deinit();
    try a.expectStatus(.ok);

    // anthropic takes over, standing fireworks down. The stand-down NULLs
    // fireworks' model, releasing fk_platform_provider_defaults_model — so glm-5.2 is now
    // deletable. WITHOUT the NULL, this DELETE would hit the FK and 500.
    const b = try (try (try h.put("/v1/admin/platform-keys").bearer(PLATFORM_ADMIN_TOKEN))
        .json("{\"provider\":\"anthropic\",\"source_workspace_id\":\"" ++ WORKSPACE_ID ++ "\",\"model\":\"claude-opus-4-8\"}")).send();
    defer b.deinit();
    try b.expectStatus(.ok);

    const del = try (try h.delete("/v1/admin/models/" ++ UID_GLM).bearer(PLATFORM_ADMIN_TOKEN)).send();
    defer del.deinit();
    try del.expectStatus(.no_content);
}

// Catalogue uid for the cache-repopulation probe — a private (provider, model)
// pair so a parallel sibling test never mutates the row this one asserts on.
const UID_CACHE = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a9101";

test "admin models: catalogue mutations repopulate the rate cache (patch then delete)" {
    const h = try startHarness(ALLOC);
    defer h.deinit();
    defer cleanup(h);
    // Direct seed (no cache touch); the HTTP mutations below drive the repopulate
    // path. This is the exact path that use-after-freed the process-global cache
    // before populate() began owning its own page_allocator memory — exercising it
    // across two mutations is the regression guard.
    try seedModel(h, UID_CACHE, "m100test", "cache-probe-1");

    const patch = try (try (try h.request(.PATCH, "/v1/admin/models/" ++ UID_CACHE).bearer(PLATFORM_ADMIN_TOKEN))
        .json("{\"context_cap_tokens\":321000,\"input_nanos_per_mtok\":424242,\"cached_input_nanos_per_mtok\":0,\"output_nanos_per_mtok\":7}")).send();
    defer patch.deinit();
    try patch.expectStatus(.ok);

    // The PATCH repopulated the global cache from the mutated row. The lookup is
    // race-robust under the parallel runner: any concurrent repopulate rebuilds
    // from the same committed row, so the rate is stable.
    const after_patch = model_rate_cache.lookup_model_rate("m100test", "cache-probe-1") orelse return error.TestUnexpectedResult;
    try std.testing.expectEqual(@as(i64, 424242), after_patch.input_nanos_per_mtok);
    try std.testing.expectEqual(@as(u32, 321000), after_patch.context_cap_tokens);

    const del = try (try h.delete("/v1/admin/models/" ++ UID_CACHE).bearer(PLATFORM_ADMIN_TOKEN)).send();
    defer del.deinit();
    try del.expectStatus(.no_content);
    // DELETE repopulated again; the pair must fall out of the cache.
    try std.testing.expect(model_rate_cache.lookup_model_rate("m100test", "cache-probe-1") == null);
}

test "admin models: POST rejects invalid rates, non-positive cap, and malformed JSON 400" {
    const h = try startHarness(ALLOC);
    defer h.deinit();
    defer cleanup(h);

    const bad_bodies = [_][]const u8{
        // negative input rate → S_RATES_NONNEG
        // pin test: literal is the contract (a valid cap, so the rate is the only invalid field)
        "{\"provider\":\"m100test\",\"model_id\":\"neg-rate\",\"context_cap_tokens\":1000,\"input_nanos_per_mtok\":-1,\"cached_input_nanos_per_mtok\":0,\"output_nanos_per_mtok\":0}",
        // non-positive cap → S_CAP_POSITIVE
        "{\"provider\":\"m100test\",\"model_id\":\"zero-cap\",\"context_cap_tokens\":0,\"input_nanos_per_mtok\":1,\"cached_input_nanos_per_mtok\":0,\"output_nanos_per_mtok\":0}",
        // malformed JSON
        "{not valid json",
    };
    for (bad_bodies) |body| {
        const r = try (try (try h.post("/v1/admin/models").bearer(PLATFORM_ADMIN_TOKEN)).json(body)).send();
        defer r.deinit();
        try r.expectStatus(.bad_request);
        try r.expectErrorCode(error_registry.ERR_INVALID_REQUEST);
    }
}

test "admin models: PATCH rejects a negative rate 400" {
    const h = try startHarness(ALLOC);
    defer h.deinit();
    defer cleanup(h);
    try seedModel(h, UID_OPUS, "m100test", "patch-neg");

    const r = try (try (try h.request(.PATCH, "/v1/admin/models/" ++ UID_OPUS).bearer(PLATFORM_ADMIN_TOKEN))
        // pin test: literal is the contract (a valid cap, so the rate is the only invalid field)
        .json("{\"context_cap_tokens\":1000,\"input_nanos_per_mtok\":-5,\"cached_input_nanos_per_mtok\":0,\"output_nanos_per_mtok\":0}")).send();
    defer r.deinit();
    try r.expectStatus(.bad_request);
    try r.expectErrorCode(error_registry.ERR_INVALID_REQUEST);
}
