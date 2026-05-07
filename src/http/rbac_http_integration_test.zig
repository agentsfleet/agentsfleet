// RBAC integration tests — role enforcement on billing and
// zombie-lifecycle endpoints over the live HTTP surface.
//
// Requires TEST_DATABASE_URL — skipped gracefully otherwise via
// `TestHarness.start` returning `error.SkipZigTest`.
//
// Uses the shared TestHarness (src/http/test_harness.zig) — see that file
// plus docs/ZIG_RULES.md "HTTP Integration Tests — Use TestHarness" for
// the canonical pattern.

const std = @import("std");
const pg = @import("pg");
const auth_mw = @import("../auth/middleware/mod.zig");
const error_codes = @import("../errors/error_registry.zig");

const harness_mod = @import("test_harness.zig");
const TestHarness = harness_mod.TestHarness;

const TEST_TENANT_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01";
const TEST_WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11";
const TEST_ISSUER = "https://clerk.dev.usezombie.com";
const TEST_AUDIENCE = "https://api.usezombie.com";
const TEST_JWKS =
    \\{"keys":[{"kty":"RSA","n":"2hg972tpbq8H6kzRZ3oVL4wZ9bO-04gJ6gCig68aluyRBzagx-7XXPCiuX80oBHBVj51kvMjT_QDNXfrwzjy4cPbwiVV4HqOGpeIZkPEopfyzs4G7mjiQmx0YuM_5WQUlUjji6Y_DfeaoH-yOhTWBMBVoI0vW_1n66CFaGuEarj3VasdWYxObJTBAM6Jn4XZDcDsBBPNGO4ku7yILkfi11FqXfBP2V8NT0hAGXVAxlWwv-8up1RDzgACp-8JWoC2-kOUJN82fGenDGKq9hW_sumO-4YPNP4U1smnw5jzLlvKa0LBrYG8IgW-3Dniuq2mojhrD_ZQClUd5rF42OyYqw","e":"AQAB","kid":"rbac-test-kid","use":"sig","alg":"RS256"}]}
;
const TEST_USER_TOKEN =
    "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCIsImtpZCI6InJiYWMtdGVzdC1raWQifQ.eyJzdWIiOiJ1c2VyX3Rlc3QiLCJpc3MiOiJodHRwczovL2NsZXJrLmRldi51c2V6b21iaWUuY29tIiwiYXVkIjoiaHR0cHM6Ly9hcGkudXNlem9tYmllLmNvbSIsImV4cCI6NDEwMjQ0NDgwMCwibWV0YWRhdGEiOnsidGVuYW50X2lkIjoiMDE5NWI0YmEtOGQzYS03ZjEzLThhYmMtMmIzZTFlMGE2ZjAxIiwid29ya3NwYWNlX2lkIjoiMDE5NWI0YmEtOGQzYS03ZjEzLThhYmMtMmIzZTFlMGE2ZjExIiwicm9sZSI6InVzZXIifX0.UEZ3huXtn6bXpa3M1EJZ2QmqLtXewLsHYP5ggTeRg-lgX-Vzp2ECvTsGgzhCSxNNPudRXYgdTsPa1ufIKv_5n1SvuoCRw2eRZfTUp5a_68KbScepnLVx5LaRJmoMyPP8Q_DPYwB0vHm1NCPRIfFqzcBOpLw01Ygkse4mTq19JPE4vcINmaVTWMiN02_ScU0DWhzhzx3_B1_vCBC3wxCpVuM_wqOHDUCnBEPkM-YVQcZrtQIdXPfRzZ2XFRVWFn-E7s0EWBpEP1wSCh31ymki_E1vlnrW4q9ZKNBYnZX0ErvJlcqH2U7nIsFlLYULNP_4mdYrDaWvBSSYZROoK1d8WQ";
const TEST_OPERATOR_TOKEN =
    "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCIsImtpZCI6InJiYWMtdGVzdC1raWQifQ.eyJzdWIiOiJ1c2VyX3Rlc3QiLCJpc3MiOiJodHRwczovL2NsZXJrLmRldi51c2V6b21iaWUuY29tIiwiYXVkIjoiaHR0cHM6Ly9hcGkudXNlem9tYmllLmNvbSIsImV4cCI6NDEwMjQ0NDgwMCwibWV0YWRhdGEiOnsidGVuYW50X2lkIjoiMDE5NWI0YmEtOGQzYS03ZjEzLThhYmMtMmIzZTFlMGE2ZjAxIiwid29ya3NwYWNlX2lkIjoiMDE5NWI0YmEtOGQzYS03ZjEzLThhYmMtMmIzZTFlMGE2ZjExIiwicm9sZSI6Im9wZXJhdG9yIn19.V84uE69RTLrRef0sogegUcUZeKWx8E68GEruFoS8HegUa3o7bVCfQjlkllNSbtUut919EygbQv1C16BMfNTOAv1Lvl3AeLYPYr4ni6EnzzGllbyxDw1aY68AGWEEvKOUxd5wCGl8BnEqaOKX7KNNbAOV4AzJNWqnV-uxJiZl6oDtqi8bsSF1HAm9qY9MAl6AwoZLGnT_x6ux_3vfKy_9ckZSbgjN7laZOMqQ5nwwcaSpwYNm_3ZpXJLgHYMVxel2M4rT0SIaFh__rE42yGE9FBDRUFoyktGOR3NYPOzogjj3tfOoecC8NEhrwifzXcSNVAiHOMnmXojjAPEUORovPg";
const TEST_ADMIN_TOKEN =
    "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCIsImtpZCI6InJiYWMtdGVzdC1raWQifQ.eyJzdWIiOiJ1c2VyX3Rlc3QiLCJpc3MiOiJodHRwczovL2NsZXJrLmRldi51c2V6b21iaWUuY29tIiwiYXVkIjoiaHR0cHM6Ly9hcGkudXNlem9tYmllLmNvbSIsImV4cCI6NDEwMjQ0NDgwMCwibWV0YWRhdGEiOnsidGVuYW50X2lkIjoiMDE5NWI0YmEtOGQzYS03ZjEzLThhYmMtMmIzZTFlMGE2ZjAxIiwid29ya3NwYWNlX2lkIjoiMDE5NWI0YmEtOGQzYS03ZjEzLThhYmMtMmIzZTFlMGE2ZjExIiwicm9sZSI6ImFkbWluIn19.sTBn0XSWWTLEd5fSEcClUIhMCVeuXjljxYymPdMwahzAhhkg6P3MVhmtiPC_B_nFQQ7WU8cAS7kSvPL3Fcs9feb06C7zosm63ByUdqigATBVILyCDt43em2pG8cGOgj-bhkxIoWsGai5hdzu4vzOEYMMLzvN_V_QPMrjqWnLIiCVXk9_Mcdpx5xbUfA1hAwg_bM8CTlezRQ5ys8oxQDymx6cvuUaW_M69jYEgpFeETNpYWmuvMWIuVlT2wpME9-8l3ytYpE0ZxnGG_HQTY1bXRkg_ZC02uYs90lhOWEs9cPG4Uz0HU6rNSnRK71bAtlgQUlcUZZSK-Gg4GbFM0SVPg";

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
    const now_ms = std.time.milliTimestamp();
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
        \\  (tenant_id, plan_tier, plan_sku, balance_cents, grant_source, created_at, updated_at)
        \\VALUES ($1, 'scale', 'scale_default', 1000000, 'rbac_test_seed', $2, $2)
        \\ON CONFLICT (tenant_id) DO NOTHING
    , .{ TEST_TENANT_ID, now_ms });
}

fn cleanupSeedData(conn: *pg.Conn) !void {
    _ = conn;
    // Tenants/workspaces are shared across the integration suite; nothing to
    // narrow-clean now that workspace-scoped billing audit tables are gone.
}

// ── Test: role gates for admin + zombie-lifecycle; 404 pins for removed billing ───

test "integration: RBAC endpoints enforce operator and admin roles over live HTTP" {
    const alloc = std.testing.allocator;
    const h = seedAndHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    // Admin-gated endpoint that survived the billing teardown.
    const admin_keys_path = "/v1/admin/platform-keys";

    { // No token → 401
        const r = try h.get(admin_keys_path).send();
        defer r.deinit();
        try r.expectStatus(.unauthorized);
        try r.expectErrorCode(error_codes.ERR_UNAUTHORIZED);
    }
    { // User role → 403
        const r = try (try h.get(admin_keys_path).bearer(TEST_USER_TOKEN)).send();
        defer r.deinit();
        try r.expectStatus(.forbidden);
        try r.expectErrorCode(error_codes.ERR_INSUFFICIENT_ROLE);
    }
    { // Operator rejected for admin-only endpoint → 403
        const r = try (try h.get(admin_keys_path).bearer(TEST_OPERATOR_TOKEN)).send();
        defer r.deinit();
        try r.expectStatus(.forbidden);
        try r.expectErrorCode(error_codes.ERR_INSUFFICIENT_ROLE);
    }
    { // Admin → 200
        const r = try (try h.get(admin_keys_path).bearer(TEST_ADMIN_TOKEN)).send();
        defer r.deinit();
        try r.expectStatus(.ok);
    }

    // RULE BIL regression — destructive lifecycle (PATCH zombie status =
    // stopped/active/killed) fires workspace_guards.enforce(.minimum_role
    // = .operator) BEFORE any zombie lookup, so a well-formed-but-
    // nonexistent zombie_id yields 403 under TEST_USER_TOKEN.
    const stop_path = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/zombies/0195b4ba-8d3a-7f13-8abc-2b3e1e0a71bb", .{TEST_WORKSPACE_ID});
    defer alloc.free(stop_path);
    {
        var req = h.request(.PATCH, stop_path);
        req = try req.bearer(TEST_USER_TOKEN);
        req = try req.json("{\"status\":\"stopped\"}");
        const r = try req.send();
        defer r.deinit();
        try r.expectStatus(.forbidden);
        try r.expectErrorCode(error_codes.ERR_INSUFFICIENT_ROLE);
    }

    // M11_005: removed workspace-scoped billing endpoints must 404 regardless
    // of role — pre-v2.0 bare 404s per RULE EP4.
    const removed_paths = [_][]const u8{
        "/v1/workspaces/" ++ TEST_WORKSPACE_ID ++ "/billing/events",
        "/v1/workspaces/" ++ TEST_WORKSPACE_ID ++ "/billing/scale",
        "/v1/workspaces/" ++ TEST_WORKSPACE_ID ++ "/billing/summary",
        "/v1/workspaces/" ++ TEST_WORKSPACE_ID ++ "/zombies/0195b4ba-8d3a-7f13-8abc-2b3e1e0a71bb/billing/summary",
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
