// RBAC integration tests — role enforcement on billing and
// fleet-lifecycle endpoints over the live HTTP surface.
//
// Requires TEST_DATABASE_URL — skipped gracefully otherwise via
// `TestHarness.start` returning `error.SkipZigTest`.
//
// Uses the shared TestHarness (src/http/test_harness.zig) — see that file
// plus docs/ZIG_RULES.md "HTTP Integration Tests — Use TestHarness" for
// the canonical pattern.

const std = @import("std");
const clock = @import("common").clock;
const pg = @import("pg");
const auth_mw = @import("../auth/middleware/mod.zig");
const error_codes = @import("../errors/error_registry.zig");

const harness_mod = @import("test_harness.zig");
const TestHarness = harness_mod.TestHarness;

const TEST_BALANCE_NANOS: i64 = 1000000;

const TEST_TENANT_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01";
const TEST_WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11";
const TEST_ISSUER = "https://clerk.dev.agentsfleet.net";
const TEST_AUDIENCE = "https://api.agentsfleet.net";
const TEST_JWKS =
    \\{"keys":[{"kty":"RSA","n":"055gLVwlsa0-zUCqQAYqyfiAob0IBQZgtHfhGUSIyRz-Czj-q0-2ota3rtZVla0drH56Zjzs80rSt9sDFQzsl0EfBrErbfhaUiEyiSwJnR69BxkbEtxTgdi-gtTe6Y5dbkxjO126xAaBcAkFYrfABFKaUZiEZjECtVjjRIu-LihcBbVx_LiLQr2beTb8A7sGeOY0JUjIcKMCDofdcRUoVuqvPBRjNk0pwDp7EOccQVthDOysJi6k5JdY3seExqNtthtWOcnUTD1uYKtx9YGoDLbdu5N3HkPieQHDXd8-Ah1uxbmHUr94CVzYlTxyIfp_vPvJa9pbYUy2PdoDuK_I9w","e":"AQAB","kid":"test-kid-static","use":"sig","alg":"RS256"}]}
;
const TEST_USER_TOKEN =
    "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCIsImtpZCI6InRlc3Qta2lkLXN0YXRpYyJ9.eyJzdWIiOiJ1c2VyX3Rlc3QiLCJpc3MiOiJodHRwczovL2NsZXJrLmRldi5hZ2VudHNmbGVldC5uZXQiLCJhdWQiOiJodHRwczovL2FwaS5hZ2VudHNmbGVldC5uZXQiLCJleHAiOjQxMDI0NDQ4MDAsInNjb3BlcyI6ImZsZWV0OnJlYWQiLCJtZXRhZGF0YSI6eyJ0ZW5hbnRfaWQiOiIwMTk1YjRiYS04ZDNhLTdmMTMtOGFiYy0yYjNlMWUwYTZmMDEiLCJ3b3Jrc3BhY2VfaWQiOiIwMTk1YjRiYS04ZDNhLTdmMTMtOGFiYy0yYjNlMWUwYTZmMTEifX0.u6CS-YuOMeBdKkJirBlOmA15U4a5b7H6F-xATiTIpQMy-9t5U1VP7ivMQOmX6TrIfDAtt5y4YZxYkgfruNdefZAdlnYqZ1mXq9R1EQbGh8zlWfFLIq1pJih5lt5Bc8MD7TQH6GUshzcRig0Vy9_dzEwERBvllk2KJo24th8zrRB39UHMzVzVTyA_I6yanpHhuTwcbD6UtFjrgaJLq68NJlZG05iRb-cbsIo-5i8tnlupVrO9NfTPkYWTTwexzFwktxcHMNtBwZ7leMMx6H9pFo2xWdzykduKJI6d1doCd_SLjDyxU3sgdYTcrRU09JDpASTDseDH9n8kSXO0Q0Y5sg";
const TEST_OPERATOR_TOKEN =
    "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCIsImtpZCI6InRlc3Qta2lkLXN0YXRpYyJ9.eyJzdWIiOiJ1c2VyX3Rlc3QiLCJpc3MiOiJodHRwczovL2NsZXJrLmRldi5hZ2VudHNmbGVldC5uZXQiLCJhdWQiOiJodHRwczovL2FwaS5hZ2VudHNmbGVldC5uZXQiLCJleHAiOjQxMDI0NDQ4MDAsInNjb3BlcyI6ImZsZWV0OndyaXRlIGNyZWRlbnRpYWw6cmVhZCIsIm1ldGFkYXRhIjp7InRlbmFudF9pZCI6IjAxOTViNGJhLThkM2EtN2YxMy04YWJjLTJiM2UxZTBhNmYwMSIsIndvcmtzcGFjZV9pZCI6IjAxOTViNGJhLThkM2EtN2YxMy04YWJjLTJiM2UxZTBhNmYxMSJ9fQ.Vf7b0wnWFRmIEOtd1jq01MRha9MHeTUybxubZhO5ctyROlswnGezKs4V_4Mn8avKz-vfCHwqId1ejWNuTxQvlkJnfkwHOH9kh3wdi41yeEGekLBzx5UYk01brVmyRksq52kdjRVnPTZKd2qoNhKM3un0lvjl669EGK0iPheGWj581r1EfAs31FrcWN6WPTuGw_N39mnofyx0JoEhLXMmlCIZcrhq7UeOALf_NBQR-Fm8C5d2hRpnth-_Ryd3PzFvuRJPd6uR73f_n9Cl66TLCIoNygIzrhMPqszmOyA6Kk-GRHCawU4OaxK50fCqohxSfimc26BK26A8N872RZtcRQ";
const TEST_ADMIN_TOKEN =
    "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCIsImtpZCI6InRlc3Qta2lkLXN0YXRpYyJ9.eyJzdWIiOiJ1c2VyX3Rlc3QiLCJpc3MiOiJodHRwczovL2NsZXJrLmRldi5hZ2VudHNmbGVldC5uZXQiLCJhdWQiOiJodHRwczovL2FwaS5hZ2VudHNmbGVldC5uZXQiLCJleHAiOjQxMDI0NDQ4MDAsInNjb3BlcyI6ImZsZWV0OmFkbWluIGNyZWRlbnRpYWw6d3JpdGUgYXBpa2V5OmFkbWluIGZsZWV0a2V5OndyaXRlIGdyYW50OndyaXRlIGNvbm5lY3Rvcjp3cml0ZSBiaWxsaW5nOnJlYWQgYXBwcm92YWw6cmVzb2x2ZSB3b3Jrc3BhY2U6YWRtaW4gdGVtcGxhdGU6d3JpdGUiLCJtZXRhZGF0YSI6eyJ0ZW5hbnRfaWQiOiIwMTk1YjRiYS04ZDNhLTdmMTMtOGFiYy0yYjNlMWUwYTZmMDEiLCJ3b3Jrc3BhY2VfaWQiOiIwMTk1YjRiYS04ZDNhLTdmMTMtOGFiYy0yYjNlMWUwYTZmMTEifX0.wIBu6pV0SEgUx-dWvIQS6qXYXDT3licQxSYiC2-n7ijivJCikIRnO96t7HD71TQnSeJWR2uBbuK0oMDfUotxO-bkIDZ8CnEXMrV3u0W3o5ChssaBCjrU4zPOFTEt4kxICm0BRItzSAI7U8LDSsqa90OYeN2teZ0rtjrc8KlwqOFRoJJ17hDjbxUDqx1sbL_kKgn2mN8Vrwgksyze7z9buSI2f0be555BaR9bgE6rMeBBSqQSlRiNP_7_CITCciZ9cenAzJL7oytaA0XHj73Qdadq4_0L5fPKg5WCVFlgqEofuP5pQo8cHK-jxn1PHYlzVisMWObFcI4ki_A6DZFlqg";

fn configureRegistry(_: *auth_mw.MiddlewareRegistry, _: *TestHarness) anyerror!void {}

fn startHarness(alloc: std.mem.Allocator) !*TestHarness {
    return TestHarness.start(alloc, .{
        .configureRegistry = configureRegistry,
        .inline_jwks_json = TEST_JWKS,
        .issuer = TEST_ISSUER,
        .audience = TEST_AUDIENCE,
    });
}

fn seedAndHarness(alloc: std.mem.Allocator) !*TestHarness {
    const h = try startHarness(alloc);
    errdefer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try setupSeedData(conn);
    return h;
}

fn setupSeedData(conn: *pg.Conn) !void {
    const now_ms = clock.nowMillis();
    _ = try conn.exec(
        \\INSERT INTO tenants (tenant_id, name, created_at, updated_at)
        \\VALUES ($1, 'RBAC Test Tenant', $2, $2)
        \\ON CONFLICT (tenant_id) DO NOTHING
    , .{ TEST_TENANT_ID, now_ms });
    _ = try conn.exec(
        \\INSERT INTO workspaces
        \\  (workspace_id, tenant_id, created_at)
        \\VALUES ($1, $2, $3)
        \\ON CONFLICT (workspace_id) DO NOTHING
    , .{ TEST_WORKSPACE_ID, TEST_TENANT_ID, now_ms });
    _ = try conn.exec(
        \\INSERT INTO billing.tenant_billing
        \\  (tenant_id, balance_nanos, grant_source, created_at, updated_at)
        \\VALUES ($1, $3, 'rbac_test_seed', $2, $2)
        \\ON CONFLICT (tenant_id) DO NOTHING
    , .{ TEST_TENANT_ID, now_ms, TEST_BALANCE_NANOS });
}

fn cleanupSeedData(conn: *pg.Conn) !void {
    _ = conn;
    // Tenants/workspaces are shared across the integration suite; nothing to
    // narrow-clean now that workspace-scoped billing audit tables are gone.
}

// ── Test: role gates for admin + fleet-lifecycle; 404 pins for removed billing ───

test "integration: RBAC endpoints enforce operator and admin roles over live HTTP" {
    const alloc = std.testing.allocator;
    const h = seedAndHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    // Platform-plane endpoint: the route requires a platform scope
    // (`platform-key:read`), so a tenant-scoped principal — whatever tenant
    // capabilities it carries — is insufficient and is rejected `403 UZ-AUTH-022`.
    // The platform success path (200) is covered in
    // model_caps_admin_integration_test, which carries a platform-scoped token.
    const admin_keys_path = "/v1/admin/platform-keys";

    { // No token → 401
        const r = try h.get(admin_keys_path).send();
        defer r.deinit();
        try r.expectStatus(.unauthorized);
        try r.expectErrorCode(error_codes.ERR_UNAUTHORIZED);
    }
    { // Tenant principal without a platform scope → 403 UZ-AUTH-022
        const r = try (try h.get(admin_keys_path).bearer(TEST_USER_TOKEN)).send();
        defer r.deinit();
        try r.expectStatus(.forbidden);
        try r.expectErrorCode(error_codes.ERR_INSUFFICIENT_SCOPE);
    }
    { // Another tenant principal without a platform scope → 403 UZ-AUTH-022
        const r = try (try h.get(admin_keys_path).bearer(TEST_OPERATOR_TOKEN)).send();
        defer r.deinit();
        try r.expectStatus(.forbidden);
        try r.expectErrorCode(error_codes.ERR_INSUFFICIENT_SCOPE);
    }
    { // A full tenant-capability principal is ALSO rejected (no platform scope) → 403
        const r = try (try h.get(admin_keys_path).bearer(TEST_ADMIN_TOKEN)).send();
        defer r.deinit();
        try r.expectStatus(.forbidden);
        try r.expectErrorCode(error_codes.ERR_INSUFFICIENT_SCOPE);
    }

    // RULE BIL regression — destructive lifecycle (PATCH fleet status =
    // stopped/active/killed) fires workspace_guards.enforce(.minimum_role
    // = .operator) BEFORE any fleet lookup, so a well-formed-but-
    // nonexistent fleet_id yields 403 under TEST_USER_TOKEN.
    const stop_path = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/fleets/0195b4ba-8d3a-7f13-8abc-2b3e1e0a71bb", .{TEST_WORKSPACE_ID});
    defer alloc.free(stop_path);
    {
        var req = h.request(.PATCH, stop_path);
        req = try req.bearer(TEST_USER_TOKEN);
        req = try req.json("{\"status\":\"stopped\"}");
        const r = try req.send();
        defer r.deinit();
        try r.expectStatus(.forbidden);
        try r.expectErrorCode(error_codes.ERR_INSUFFICIENT_SCOPE);
    }

    // M11_005: removed workspace-scoped billing endpoints must 404 regardless
    // of role — pre-v2.0 bare 404s per RULE EP4.
    const removed_paths = [_][]const u8{
        "/v1/workspaces/" ++ TEST_WORKSPACE_ID ++ "/billing/events",
        "/v1/workspaces/" ++ TEST_WORKSPACE_ID ++ "/billing/scale",
        "/v1/workspaces/" ++ TEST_WORKSPACE_ID ++ "/billing/summary",
        "/v1/workspaces/" ++ TEST_WORKSPACE_ID ++ "/fleets/0195b4ba-8d3a-7f13-8abc-2b3e1e0a71bb/billing/summary",
        "/v1/workspaces/" ++ TEST_WORKSPACE_ID ++ "/scoring/config",
    };
    for (removed_paths) |path| {
        const r = try (try h.get(path).bearer(TEST_ADMIN_TOKEN)).send();
        defer r.deinit();
        try r.expectStatus(.not_found);
    }

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try cleanupSeedData(conn);
}

// ── Test: deterministic rejection under concurrency ───────────────────────────

const ConcurrentCtx = struct {
    h: *TestHarness,
    path: []const u8,
    token: []const u8,
    status: *u16,

    fn run(self: ConcurrentCtx) void {
        const r = (self.h.get(self.path).bearer(self.token) catch {
            self.status.* = 0;
            return;
        }).send() catch {
            self.status.* = 0;
            return;
        };
        defer r.deinit();
        self.status.* = r.status;
    }
};

test "integration: RBAC user-role rejection stays deterministic under concurrency" {
    const alloc = std.testing.allocator;
    const h = seedAndHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    // Admin-only endpoint that survived the billing teardown — a user-role
    // token is deterministically 403'd here. We don't care which endpoint; we
    // care that the rejection is stable across 5 concurrent callers.
    const admin_keys_path: []const u8 = "/v1/admin/platform-keys";

    var statuses = [_]u16{0} ** 5;
    var threads: [5]std.Thread = undefined;
    for (&threads, 0..) |*thread, idx| {
        thread.* = try std.Thread.spawn(.{}, ConcurrentCtx.run, .{ConcurrentCtx{
            .h = h,
            .path = admin_keys_path,
            .token = TEST_USER_TOKEN,
            .status = &statuses[idx],
        }});
    }
    for (&threads) |*thread| thread.join();
    for (statuses) |status| try std.testing.expectEqual(@as(u16, 403), status);

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try cleanupSeedData(conn);
}
