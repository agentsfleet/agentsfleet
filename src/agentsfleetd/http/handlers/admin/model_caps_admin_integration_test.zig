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
const clock = @import("common").clock;
const auth_mw = @import("../../../auth/middleware/mod.zig");
const error_registry = @import("../../../errors/error_registry.zig");
const PgQuery = @import("../../../db/pg_query.zig").PgQuery;
const model_rate_cache = @import("../../../state/model_rate_cache.zig");
const harness_mod = @import("../../test_harness.zig");
const TestHarness = harness_mod.TestHarness;

const ALLOC = std.testing.allocator;

const TEST_ISSUER = "https://clerk.test.agentsfleet.net";
const TEST_AUDIENCE = "https://api.agentsfleet.net";

const TENANT_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01";
const WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11";
// Pre-seeded catalogue rows (known uids so PATCH/DELETE can address them).
const UID_GLM = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a9001";
const UID_OPUS = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a9002";

const TEST_JWKS =
    \\{"keys":[{"kty":"RSA","n":"7ZUw6J4OYDXLJPGWADVw2-IgBawVd55H1Xh4R_FFFFYVNdG2O7EcTvBlFZhRzxDW9uL-SvxCt6slRDXDlZo9fmSI9yki7z8RAJZokcekxdP8za5w7g4QAoFeSieDhWWChkzHJ-vDGkrr0SAn8n4lIwpya-vCbO1eXmmz4Ay0pjenWyyGB1j371Zk2JGkAEJB347oJcVDMqVDt3d-TR0fyyspVw0nNxdDkZgNuB0EXOuEV4WvWgj0dtzwURhTI82AfpgheV23Kz7np9EoPxAhkfuslAjpRfqlRCXOOfmik-T6nvCe-fFPmHRwIY_zc1VrtwjKF0TjeALm4CCj_0pjRQ","e":"AQAB","kid":"test-kid-static","use":"sig","alg":"RS256"}]}
;
const PLATFORM_ADMIN_TOKEN =
    "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCIsImtpZCI6InRlc3Qta2lkLXN0YXRpYyJ9.eyJzdWIiOiJ1c2VyX204MDAwNSIsImlzcyI6Imh0dHBzOi8vY2xlcmsudGVzdC5hZ2VudHNmbGVldC5uZXQiLCJhdWQiOiJodHRwczovL2FwaS5hZ2VudHNmbGVldC5uZXQiLCJleHAiOjQxMDI0NDQ4MDAsIm1ldGFkYXRhIjp7InRlbmFudF9pZCI6IjAxOTViNGJhLThkM2EtN2YxMy04YWJjLTJiM2UxZTBhNmYwMSIsIndvcmtzcGFjZV9pZCI6IjAxOTViNGJhLThkM2EtN2YxMy04YWJjLTJiM2UxZTBhNmYxMSIsInJvbGUiOiJhZG1pbiIsInBsYXRmb3JtX2FkbWluIjp0cnVlfX0.Jz-CQ6v1iiI5g1neq9zAwuNa99k33WzEJYCrazuizcFXaxGTmcRzb20iWmo2eIPBcwERzrOXmSM1iw5NdlAJSsamtds2WCQntNdpkOG3Xp4_xp0faUZmNUeD4viISG1kfMr2hKKR1XPEbydTdbKEvcQoNVVmGFdDnba9fV-9WiXlSLgHuGOKHWWgZCUV8akZImjNhbGM3l0y-_v3V8skx1BaUxkTg-WInhagaDOXvGOOAEoPThmGj2bhDT4F3ZXlAbEvLyJnoQz7pkWUwv4jTQVE4jqyBs19Fx-pGppDU_1tM8h5GRN0GegzuM98bgWgfBAX2uvrIT_a5XoMRhFxQg";
const TENANT_ADMIN_TOKEN =
    "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCIsImtpZCI6InRlc3Qta2lkLXN0YXRpYyJ9.eyJzdWIiOiJ1c2VyX204MDAwNSIsImlzcyI6Imh0dHBzOi8vY2xlcmsudGVzdC5hZ2VudHNmbGVldC5uZXQiLCJhdWQiOiJodHRwczovL2FwaS5hZ2VudHNmbGVldC5uZXQiLCJleHAiOjQxMDI0NDQ4MDAsIm1ldGFkYXRhIjp7InRlbmFudF9pZCI6IjAxOTViNGJhLThkM2EtN2YxMy04YWJjLTJiM2UxZTBhNmYwMSIsIndvcmtzcGFjZV9pZCI6IjAxOTViNGJhLThkM2EtN2YxMy04YWJjLTJiM2UxZTBhNmYxMSIsInJvbGUiOiJhZG1pbiJ9fQ.jBmYsg5xN1HFcENmp24xn3RwWCKkX-jF1uffnnCpot_iYJfNv_yOYzGocigF62rsHlOAqRJF0ZQ-C3te8oOzPAd8yKZcaXJiC9SU_Rj59CpNri5pk3PjdovN9UL-2oPLkOEkoiwG-36ubpBieunFP3VuyfIwWcpXbmXsXVy68WIr9bfCemW1XZa4rCTOcKwg6Q8ccU2McscPhZ_hwgJI2jA8uygL3wgaC2CIMKsH6aUII5IO9zMNKkC_lK_t9OAHNkBCqxXNTQOXXLSyddbvwvmQ2Vjcy_ZftGaYtTZlWurXfY9pOX4tno_WWVvy2R_kOWEaAeSK_dfHOIRvv3YVsw";

fn configureRegistry(_: *auth_mw.MiddlewareRegistry, _: *TestHarness) anyerror!void {}

fn startHarness(alloc: std.mem.Allocator) !*TestHarness {
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
        \\INSERT INTO core.model_caps
        \\  (uid, model_id, provider, context_cap_tokens, input_nanos_per_mtok, cached_input_nanos_per_mtok, output_nanos_per_mtok, created_at_ms, updated_at_ms)
        \\VALUES ($1::uuid, $2, $3, 128000, 1, 0, 2, $4, $4)
        \\ON CONFLICT (provider, model_id) DO NOTHING
    , .{ uid, model_id, provider, now });
}

fn cleanup(h: *TestHarness) void {
    const conn = h.acquireConn() catch return;
    defer h.releaseConn(conn);
    _ = conn.exec("DELETE FROM core.platform_llm_keys WHERE source_workspace_id = $1::uuid", .{WORKSPACE_ID}) catch |err| std.log.warn("cleanup ignored: {s}", .{@errorName(err)});
    _ = conn.exec("DELETE FROM core.model_caps WHERE provider IN ('fireworks','anthropic','m100test')", .{}) catch |err| std.log.warn("cleanup ignored: {s}", .{@errorName(err)});
    _ = conn.exec("DELETE FROM core.workspaces WHERE workspace_id = $1::uuid", .{WORKSPACE_ID}) catch |err| std.log.warn("cleanup ignored: {s}", .{@errorName(err)});
}

fn countActivePlatformKeys(h: *TestHarness) !struct { total: i64, provider: []const u8 } {
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    var q = PgQuery.from(try conn.query("SELECT count(*)::bigint, coalesce(max(provider), '') FROM core.platform_llm_keys WHERE active = true", .{}));
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
    try r.expectErrorCode(error_registry.ERR_PLATFORM_ADMIN_REQUIRED);
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
