// RBAC integration tests — role enforcement on billing and
// agent-lifecycle endpoints over the live HTTP surface.
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
    \\{"keys":[{"kty":"RSA","n":"310oH7ahxoKws6fEKmbOP30dQaQhT21HGRxvibeBuqfywkNxJ0xcfhhao1mwbLH7BUOg2GYXDEA6EvcVlKXqGN_Wa_4Q7UenmZqeXYdB_IhAc-SzyoW9hRi01FskVVI8w_N0Pf5SItu7DIqdxbKP8_eGFyrTL1mN-5klkIDCSnhrDLUEgjVo7iod0vsoqUEH-2m1s-2xDh5aQr5rSF6neCTA1-JvKVkJLD6eOdBnEwYBm6-yZ0CNgMfw1uUyw5cGwdaPsCerHctH0EwcI_qQFUUnFjBeN4FJkP_DDoHWTEV9a-5wzomOcoKlyfZvRgplGYYqTWrIAfcZobyzYiSy1w","e":"AQAB","kid":"rbac-test-kid","use":"sig","alg":"RS256"}]}
;
const TEST_USER_TOKEN =
    "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCIsImtpZCI6InJiYWMtdGVzdC1raWQifQ.eyJzdWIiOiJ1c2VyX3Rlc3QiLCJpc3MiOiJodHRwczovL2NsZXJrLmRldi5hZ2VudHNmbGVldC5uZXQiLCJhdWQiOiJodHRwczovL2FwaS5hZ2VudHNmbGVldC5uZXQiLCJleHAiOjQxMDI0NDQ4MDAsIm1ldGFkYXRhIjp7InRlbmFudF9pZCI6IjAxOTViNGJhLThkM2EtN2YxMy04YWJjLTJiM2UxZTBhNmYwMSIsIndvcmtzcGFjZV9pZCI6IjAxOTViNGJhLThkM2EtN2YxMy04YWJjLTJiM2UxZTBhNmYxMSIsInJvbGUiOiJ1c2VyIn19.aSqdpbu-D-1NmzJgcw-7LUJYImlFu-gbrO3fBPlMI6DFvgSGJJg3wAYe5DKJXe5ytCActeAHN8LxGyr1emB4ReHk90B7t_DB301cl5fz6H1EIBnUYkuOYIeCQXvqTmEHduR1KPumEYc6Jfw3kv1tY95k-bugObZ4FihLhWXw4ud8fXRl_CTnD3J3FSx-cn4K8mfy8JjTc1RDmEx5_4-TbBhPyTgj5EAXqB1ddUw7k46UAh_-w2G07SrOxsl1b57Etwp0gvuu4tkpXICYmG423n-RjVvtvuxjSzQyhUZ2Lmfbvi1tLlY7_uzTh_BwwWWYLdJtnmKEblmGReoAu_Qs6A";
const TEST_OPERATOR_TOKEN =
    "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCIsImtpZCI6InJiYWMtdGVzdC1raWQifQ.eyJzdWIiOiJ1c2VyX3Rlc3QiLCJpc3MiOiJodHRwczovL2NsZXJrLmRldi5hZ2VudHNmbGVldC5uZXQiLCJhdWQiOiJodHRwczovL2FwaS5hZ2VudHNmbGVldC5uZXQiLCJleHAiOjQxMDI0NDQ4MDAsIm1ldGFkYXRhIjp7InRlbmFudF9pZCI6IjAxOTViNGJhLThkM2EtN2YxMy04YWJjLTJiM2UxZTBhNmYwMSIsIndvcmtzcGFjZV9pZCI6IjAxOTViNGJhLThkM2EtN2YxMy04YWJjLTJiM2UxZTBhNmYxMSIsInJvbGUiOiJvcGVyYXRvciJ9fQ.eEQp3HyUFsV1bRBDvww3DirCY1R-vrASYT3KXnTeXBa8Owuag8Mc1I_v93XBatf-t-Y0qd6r9uNQuRiRpuXkrC01MJwyPnyvKDYHFAX828PIMdFgZ5FUGU0S6r1B4B8FaVZnfMdwyyQW9tCeFBvvh2hkuodoOlkcaJnR98kMrYjGHVoyDQc5H5JnU5O8Kkb9STE-XR-3b8VdOlGJR-ljX4Vw8Fipo5p7fo_VdhhUXD2C974DrbQWtsXhqUTqOFWAEUcUMM2ODH8pEFWhG8poHVP8LLWCcSFxZDN_Ia3dNR8OK9SEblCPIlfimiMtscqxli-9uC00n62UmLuQtGVlXA";
const TEST_ADMIN_TOKEN =
    "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCIsImtpZCI6InJiYWMtdGVzdC1raWQifQ.eyJzdWIiOiJ1c2VyX3Rlc3QiLCJpc3MiOiJodHRwczovL2NsZXJrLmRldi5hZ2VudHNmbGVldC5uZXQiLCJhdWQiOiJodHRwczovL2FwaS5hZ2VudHNmbGVldC5uZXQiLCJleHAiOjQxMDI0NDQ4MDAsIm1ldGFkYXRhIjp7InRlbmFudF9pZCI6IjAxOTViNGJhLThkM2EtN2YxMy04YWJjLTJiM2UxZTBhNmYwMSIsIndvcmtzcGFjZV9pZCI6IjAxOTViNGJhLThkM2EtN2YxMy04YWJjLTJiM2UxZTBhNmYxMSIsInJvbGUiOiJhZG1pbiJ9fQ.PoaybxCP-Am6iec1ZmRFRnzOuZZtAYfbemZ0CcYbUdUrLgRq8OfQACcT0u5Ads2vBHmQGPtnL-iNo2VnLF013aOhyXxIDdpB8sUWZo_eBl9pNDqmjnGX14yDgVX8nftZ_6h6sFCKe3mzUIITzxZDJAsDfue68iRdAflECLY6RSFEdY-8wHnc9cxlAHrEgiUbscMPVYTsc8zrDkFDZvZMhanUKcoh0o6d3WnRWjCDY-Xoh34V3SkJ3G7-G2CzugMF_iEon9kXeQCzhlIp3rsrLZrRQjnNibtlCga_2-5H0TbKk_6BtBLKeDQ9Kv7g-NA0SrdcAb7GAj9L_mfweKS4TQ";

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

// ── Test: role gates for admin + agent-lifecycle; 404 pins for removed billing ───

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

    // RULE BIL regression — destructive lifecycle (PATCH agent status =
    // stopped/active/killed) fires workspace_guards.enforce(.minimum_role
    // = .operator) BEFORE any agent lookup, so a well-formed-but-
    // nonexistent agent_id yields 403 under TEST_USER_TOKEN.
    const stop_path = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/agents/0195b4ba-8d3a-7f13-8abc-2b3e1e0a71bb", .{TEST_WORKSPACE_ID});
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
        "/v1/workspaces/" ++ TEST_WORKSPACE_ID ++ "/agents/0195b4ba-8d3a-7f13-8abc-2b3e1e0a71bb/billing/summary",
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
