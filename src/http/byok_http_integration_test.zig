// HTTP integration tests for M16_004 Default Provider + BYOK.
//
// Requires DATABASE_URL (or TEST_DATABASE_URL) — skipped otherwise via
// `TestHarness.start` returning `error.SkipZigTest`.
// Vault tests (workspace BYOK) also require ENCRYPTION_MASTER_KEY — set
// automatically by setTestEncryptionKey() before the server handles vault calls.
//
// Tiers: T1 (happy path), T2 (edge cases), T3 (auth/role enforcement),
//        T5 (concurrency), T8 (secret safety), T12 (response contract).
//
// Each test starts its own harness on a unique port to avoid cross-test
// state. DB rows are cleaned up in the test body (not deferred) so teardown
// happens before pool.deinit() to avoid connection leaks on exit.
//
// Uses the shared TestHarness (src/http/test_harness.zig) — see that file
// plus docs/ZIG_RULES.md "HTTP Integration Tests — Use TestHarness" for
// the canonical pattern.

const std = @import("std");
const pg = @import("pg");
const PgQuery = @import("../db/pg_query.zig").PgQuery;
const auth_mw = @import("../auth/middleware/mod.zig");
const error_codes = @import("../errors/error_registry.zig");

const harness_mod = @import("test_harness.zig");
const TestHarness = harness_mod.TestHarness;

// ── Test constants ────────────────────────────────────────────────────────────
// Workspace + tenant UUIDs match the role claims in the JWT tokens below.

const TEST_TENANT_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01";
const TEST_WS_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11";
const TEST_ADMIN_WS_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f22"; // second workspace for cross-ws test
const TEST_PROVIDER = "__test_m16004"; // underscore prefix flags test rows for cleanup
const TEST_REPO_URL = "https://github.com/test/m16004";

const TEST_ISSUER = "https://clerk.dev.usezombie.com";
const TEST_AUDIENCE = "https://api.usezombie.com";
const TEST_JWKS =
    \\{"keys":[{"kty":"RSA","n":"2hg972tpbq8H6kzRZ3oVL4wZ9bO-04gJ6gCig68aluyRBzagx-7XXPCiuX80oBHBVj51kvMjT_QDNXfrwzjy4cPbwiVV4HqOGpeIZkPEopfyzs4G7mjiQmx0YuM_5WQUlUjji6Y_DfeaoH-yOhTWBMBVoI0vW_1n66CFaGuEarj3VasdWYxObJTBAM6Jn4XZDcDsBBPNGO4ku7yILkfi11FqXfBP2V8NT0hAGXVAxlWwv-8up1RDzgACp-8JWoC2-kOUJN82fGenDGKq9hW_sumO-4YPNP4U1smnw5jzLlvKa0LBrYG8IgW-3Dniuq2mojhrD_ZQClUd5rF42OyYqw","e":"AQAB","kid":"rbac-test-kid","use":"sig","alg":"RS256"}]}
;

// JWT tokens — role embedded in `metadata.role` claim, signed with the key above.
const TOKEN_USER =
    "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCIsImtpZCI6InJiYWMtdGVzdC1raWQifQ.eyJzdWIiOiJ1c2VyX3Rlc3QiLCJpc3MiOiJodHRwczovL2NsZXJrLmRldi51c2V6b21iaWUuY29tIiwiYXVkIjoiaHR0cHM6Ly9hcGkudXNlem9tYmllLmNvbSIsImV4cCI6NDEwMjQ0NDgwMCwibWV0YWRhdGEiOnsidGVuYW50X2lkIjoiMDE5NWI0YmEtOGQzYS03ZjEzLThhYmMtMmIzZTFlMGE2ZjAxIiwid29ya3NwYWNlX2lkIjoiMDE5NWI0YmEtOGQzYS03ZjEzLThhYmMtMmIzZTFlMGE2ZjExIiwicm9sZSI6InVzZXIifX0.UEZ3huXtn6bXpa3M1EJZ2QmqLtXewLsHYP5ggTeRg-lgX-Vzp2ECvTsGgzhCSxNNPudRXYgdTsPa1ufIKv_5n1SvuoCRw2eRZfTUp5a_68KbScepnLVx5LaRJmoMyPP8Q_DPYwB0vHm1NCPRIfFqzcBOpLw01Ygkse4mTq19JPE4vcINmaVTWMiN02_ScU0DWhzhzx3_B1_vCBC3wxCpVuM_wqOHDUCnBEPkM-YVQcZrtQIdXPfRzZ2XFRVWFn-E7s0EWBpEP1wSCh31ymki_E1vlnrW4q9ZKNBYnZX0ErvJlcqH2U7nIsFlLYULNP_4mdYrDaWvBSSYZROoK1d8WQ";
const TOKEN_OPERATOR =
    "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCIsImtpZCI6InJiYWMtdGVzdC1raWQifQ.eyJzdWIiOiJ1c2VyX3Rlc3QiLCJpc3MiOiJodHRwczovL2NsZXJrLmRldi51c2V6b21iaWUuY29tIiwiYXVkIjoiaHR0cHM6Ly9hcGkudXNlem9tYmllLmNvbSIsImV4cCI6NDEwMjQ0NDgwMCwibWV0YWRhdGEiOnsidGVuYW50X2lkIjoiMDE5NWI0YmEtOGQzYS03ZjEzLThhYmMtMmIzZTFlMGE2ZjAxIiwid29ya3NwYWNlX2lkIjoiMDE5NWI0YmEtOGQzYS03ZjEzLThhYmMtMmIzZTFlMGE2ZjExIiwicm9sZSI6Im9wZXJhdG9yIn19.V84uE69RTLrRef0sogegUcUZeKWx8E68GEruFoS8HegUa3o7bVCfQjlkllNSbtUut919EygbQv1C16BMfNTOAv1Lvl3AeLYPYr4ni6EnzzGllbyxDw1aY68AGWEEvKOUxd5wCGl8BnEqaOKX7KNNbAOV4AzJNWqnV-uxJiZl6oDtqi8bsSF1HAm9qY9MAl6AwoZLGnT_x6ux_3vfKy_9ckZSbgjN7laZOMqQ5nwwcaSpwYNm_3ZpXJLgHYMVxel2M4rT0SIaFh__rE42yGE9FBDRUFoyktGOR3NYPOzogjj3tfOoecC8NEhrwifzXcSNVAiHOMnmXojjAPEUORovPg";
const TOKEN_ADMIN =
    "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCIsImtpZCI6InJiYWMtdGVzdC1raWQifQ.eyJzdWIiOiJ1c2VyX3Rlc3QiLCJpc3MiOiJodHRwczovL2NsZXJrLmRldi51c2V6b21iaWUuY29tIiwiYXVkIjoiaHR0cHM6Ly9hcGkudXNlem9tYmllLmNvbSIsImV4cCI6NDEwMjQ0NDgwMCwibWV0YWRhdGEiOnsidGVuYW50X2lkIjoiMDE5NWI0YmEtOGQzYS03ZjEzLThhYmMtMmIzZTFlMGE2ZjAxIiwid29ya3NwYWNlX2lkIjoiMDE5NWI0YmEtOGQzYS03ZjEzLThhYmMtMmIzZTFlMGE2ZjExIiwicm9sZSI6ImFkbWluIn19.sTBn0XSWWTLEd5fSEcClUIhMCVeuXjljxYymPdMwahzAhhkg6P3MVhmtiPC_B_nFQQ7WU8cAS7kSvPL3Fcs9feb06C7zosm63ByUdqigATBVILyCDt43em2pG8cGOgj-bhkxIoWsGai5hdzu4vzOEYMMLzvN_V_QPMrjqWnLIiCVXk9_Mcdpx5xbUfA1hAwg_bM8CTlezRQ5ys8oxQDymx6cvuUaW_M69jYEgpFeETNpYWmuvMWIuVlT2wpME9-8l3ytYpE0ZxnGG_HQTY1bXRkg_ZC02uYs90lhOWEs9cPG4Uz0HU6rNSnRK71bAtlgQUlcUZZSK-Gg4GbFM0SVPg";

// Byok uses only default registry policies (bearer_or_api_key + role gates);
// no webhook/svix middleware wiring needed.
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

fn setTestEncryptionKey() void {
    const c = @cImport(@cInclude("stdlib.h"));
    _ = c.setenv("ENCRYPTION_MASTER_KEY", "0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20", 1);
}

fn setupSeedData(conn: *pg.Conn) !void {
    const now_ms = std.time.milliTimestamp();
    // Idempotent — other integration suites share TEST_TENANT_ID / TEST_WS_ID.
    // Only wipe rows THIS suite owns; tenants/workspaces use ON CONFLICT DO NOTHING
    // so a prior test's seed survives intact (and FK-referencing rows in sibling
    // suites don't get cascaded out from under them).
    _ = try conn.exec("DELETE FROM platform_llm_keys WHERE provider LIKE '\\__test\\_%' ESCAPE '\\'", .{});
    _ = try conn.exec("DELETE FROM vault.secrets WHERE workspace_id IN ($1, $2)", .{ TEST_WS_ID, TEST_ADMIN_WS_ID });
    _ = try conn.exec(
        \\INSERT INTO tenants (tenant_id, name, created_at, updated_at)
        \\VALUES ($1, 'M16_004 Test', $2, $2)
        \\ON CONFLICT (tenant_id) DO NOTHING
    , .{ TEST_TENANT_ID, now_ms });
    _ = try conn.exec(
        \\INSERT INTO workspaces (workspace_id, tenant_id, repo_url, default_branch, paused, version, created_at, updated_at)
        \\VALUES ($1, $2, $3, 'main', false, 1, $4, $4)
        \\ON CONFLICT (workspace_id) DO NOTHING
    , .{ TEST_WS_ID, TEST_TENANT_ID, TEST_REPO_URL, now_ms });
    _ = try conn.exec(
        \\INSERT INTO workspaces (workspace_id, tenant_id, repo_url, default_branch, paused, version, created_at, updated_at)
        \\VALUES ($1, $2, $3, 'main', false, 1, $4, $4)
        \\ON CONFLICT (workspace_id) DO NOTHING
    , .{ TEST_ADMIN_WS_ID, TEST_TENANT_ID, TEST_REPO_URL, now_ms });
}

fn cleanupSeedData(conn: *pg.Conn) void {
    // Narrow scope — only delete rows THIS suite owns (platform keys + workspace BYOK
    // secrets). Tenants/workspaces are shared across the integration suite via the
    // baked-in JWT claims; wiping them here breaks sibling tests and forces a
    // `make test-integration` reset. See docs/ZIG_RULES.md "HTTP Integration Tests".
    _ = conn.exec("DELETE FROM platform_llm_keys WHERE provider LIKE '\\__test\\_%' ESCAPE '\\'", .{}) catch {};
    _ = conn.exec("DELETE FROM vault.secrets WHERE workspace_id IN ($1, $2)", .{ TEST_WS_ID, TEST_ADMIN_WS_ID }) catch {};
}

// ── T1 + T12: Admin platform key lifecycle ────────────────────────────────────

test "integration: admin platform key PUT-GET-DELETE lifecycle" {
    const alloc = std.testing.allocator;
    const h = seedAndHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const path = "/v1/admin/platform-keys";
    const put_body = try std.fmt.allocPrint(alloc, "{{\"provider\":\"{s}\",\"source_workspace_id\":\"{s}\"}}", .{ TEST_PROVIDER, TEST_WS_ID });
    defer alloc.free(put_body);

    { // PUT: upsert — T1 + T12 + T8 (no key material leak)
        const r = try (try (try h.put(path).bearer(TOKEN_ADMIN)).json(put_body)).send();
        defer r.deinit();
        try r.expectStatus(.ok);
        try std.testing.expect(r.bodyContains(TEST_PROVIDER));
        try std.testing.expect(r.bodyContains("true"));
        try std.testing.expect(!r.bodyContains("sk-"));
    }
    { // GET: list contains row — T1 + T12 + T8
        const r = try (try h.get(path).bearer(TOKEN_ADMIN)).send();
        defer r.deinit();
        try r.expectStatus(.ok);
        try std.testing.expect(r.bodyContains(TEST_PROVIDER));
        try std.testing.expect(!r.bodyContains("api_key"));
    }
    { // DELETE: deactivate — T1
        const del_path = try std.fmt.allocPrint(alloc, "{s}/{s}", .{ path, TEST_PROVIDER });
        defer alloc.free(del_path);
        const r = try (try h.delete(del_path).bearer(TOKEN_ADMIN)).send();
        defer r.deinit();
        try r.expectStatus(.ok);
        try std.testing.expect(r.bodyContains("false"));
    }

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    cleanupSeedData(conn);
}

// ── T3: Admin platform key enforces admin-only access ────────────────────────

test "integration: admin platform key enforces admin role and validates input" {
    const alloc = std.testing.allocator;
    const h = seedAndHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const path = "/v1/admin/platform-keys";
    const valid_body = try std.fmt.allocPrint(alloc, "{{\"provider\":\"{s}\",\"source_workspace_id\":\"{s}\"}}", .{ TEST_PROVIDER, TEST_WS_ID });
    defer alloc.free(valid_body);

    { // T3: no token → 401
        const r = try (try h.put(path).json(valid_body)).send();
        defer r.deinit();
        try r.expectStatus(.unauthorized);
    }
    { // T3: user role → 403
        const r = try (try (try h.put(path).bearer(TOKEN_USER)).json(valid_body)).send();
        defer r.deinit();
        try r.expectStatus(.forbidden);
        try r.expectErrorCode(error_codes.ERR_INSUFFICIENT_ROLE);
    }
    { // T3: operator role → 403
        const r = try (try (try h.put(path).bearer(TOKEN_OPERATOR)).json(valid_body)).send();
        defer r.deinit();
        try r.expectStatus(.forbidden);
    }
    { // T2: empty provider → 400
        const r = try (try (try h.put(path).bearer(TOKEN_ADMIN))
            .json("{\"provider\":\"\",\"source_workspace_id\":\"0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11\"}")).send();
        defer r.deinit();
        try r.expectStatus(.bad_request);
    }
    { // T2: 33-char provider → 400
        const long = "a" ** 33;
        const b = try std.fmt.allocPrint(alloc, "{{\"provider\":\"{s}\",\"source_workspace_id\":\"{s}\"}}", .{ long, TEST_WS_ID });
        defer alloc.free(b);
        const r = try (try (try h.put(path).bearer(TOKEN_ADMIN)).json(b)).send();
        defer r.deinit();
        try r.expectStatus(.bad_request);
    }
    { // T2: non-UUIDv7 source_workspace_id → 400
        const r = try (try (try h.put(path).bearer(TOKEN_ADMIN))
            .json("{\"provider\":\"kimi\",\"source_workspace_id\":\"not-a-uuid\"}")).send();
        defer r.deinit();
        try r.expectStatus(.bad_request);
    }
    { // T3: malformed JSON → 400
        const r = try (try (try h.put(path).bearer(TOKEN_ADMIN)).json("{bad json")).send();
        defer r.deinit();
        try r.expectStatus(.bad_request);
    }
    { // T3: GET enforces admin-only
        const r = try (try h.get(path).bearer(TOKEN_OPERATOR)).send();
        defer r.deinit();
        try r.expectStatus(.forbidden);
    }

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    cleanupSeedData(conn);
}

// ── T1 + T8 + T12: Workspace BYOK credential lifecycle ────────────────────────

test "integration: workspace BYOK credential lifecycle and key never in response" {
    setTestEncryptionKey();
    const alloc = std.testing.allocator;
    const h = seedAndHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const path = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/credentials/llm", .{TEST_WS_ID});
    defer alloc.free(path);
    const secret_key = "sk-ant-SUPER-SECRET-DO-NOT-LEAK-1234";

    { // PUT: store key — T1 + T12
        const b = try std.fmt.allocPrint(alloc, "{{\"provider\":\"anthropic\",\"api_key\":\"{s}\"}}", .{secret_key});
        defer alloc.free(b);
        const r = try (try (try h.put(path).bearer(TOKEN_OPERATOR)).json(b)).send();
        defer r.deinit();
        try r.expectStatus(.no_content);
        try std.testing.expectEqual(@as(usize, 0), r.body.len);
    }
    { // GET: has_key=true, no key in response — T1 + T8 + T12
        const r = try (try h.get(path).bearer(TOKEN_OPERATOR)).send();
        defer r.deinit();
        try r.expectStatus(.ok);
        try std.testing.expect(r.bodyContains("true"));
        try std.testing.expect(r.bodyContains("anthropic"));
        try std.testing.expect(!r.bodyContains(secret_key));
        try std.testing.expect(!r.bodyContains("sk-ant-"));
    }
    { // DELETE: remove key — T1
        const r = try (try h.delete(path).bearer(TOKEN_OPERATOR)).send();
        defer r.deinit();
        try r.expectStatus(.no_content);
    }
    { // GET after DELETE: has_key=false — T1
        const r = try (try h.get(path).bearer(TOKEN_OPERATOR)).send();
        defer r.deinit();
        try r.expectStatus(.ok);
        try std.testing.expect(r.bodyContains("false"));
        try std.testing.expect(!r.bodyContains(secret_key));
    }

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    cleanupSeedData(conn);
}

// ── T3 + T8: Workspace BYOK enforces operator role and workspace scope ─────────

test "integration: workspace BYOK enforces operator role and workspace boundary" {
    const alloc = std.testing.allocator;
    const h = seedAndHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const path = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/credentials/llm", .{TEST_WS_ID});
    defer alloc.free(path);
    const other_path = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/credentials/llm", .{TEST_ADMIN_WS_ID});
    defer alloc.free(other_path);
    const valid_body = "{\"provider\":\"anthropic\",\"api_key\":\"sk-test-1234\"}";

    { // T3: no token → 401
        const r = try (try h.put(path).json(valid_body)).send();
        defer r.deinit();
        try r.expectStatus(.unauthorized);
    }
    { // T3: user role → 403
        const r = try (try (try h.put(path).bearer(TOKEN_USER)).json(valid_body)).send();
        defer r.deinit();
        try r.expectStatus(.forbidden);
    }
    { // T8: cross-workspace → 403 or 400
        const r = try (try (try h.put(other_path).bearer(TOKEN_OPERATOR)).json(valid_body)).send();
        defer r.deinit();
        try std.testing.expect(r.status == 403 or r.status == 400);
    }
    { // T2: provider too long → 400
        const long = "a" ** 33;
        const b = try std.fmt.allocPrint(alloc, "{{\"provider\":\"{s}\",\"api_key\":\"sk-x\"}}", .{long});
        defer alloc.free(b);
        const r = try (try (try h.put(path).bearer(TOKEN_OPERATOR)).json(b)).send();
        defer r.deinit();
        try r.expectStatus(.bad_request);
    }
    { // T2: api_key too long → 400
        const long_key = "sk-" ++ ("a" ** 254);
        const b = try std.fmt.allocPrint(alloc, "{{\"provider\":\"anthropic\",\"api_key\":\"{s}\"}}", .{long_key});
        defer alloc.free(b);
        const r = try (try (try h.put(path).bearer(TOKEN_OPERATOR)).json(b)).send();
        defer r.deinit();
        try r.expectStatus(.bad_request);
    }
    { // T2: empty api_key → 400
        const r = try (try (try h.put(path).bearer(TOKEN_OPERATOR))
            .json("{\"provider\":\"anthropic\",\"api_key\":\"\"}")).send();
        defer r.deinit();
        try r.expectStatus(.bad_request);
    }
    { // T3: GET enforces operator role
        const r = try (try h.get(path).bearer(TOKEN_USER)).send();
        defer r.deinit();
        try r.expectStatus(.forbidden);
    }

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    cleanupSeedData(conn);
}

// ── T5: Concurrent upserts are idempotent ─────────────────────────────────────

const ConcurrentPutCtx = struct {
    h: *TestHarness,
    body: []const u8,
    result: *u16,
    fn run(self: ConcurrentPutCtx) void {
        const bearer_req = self.h.put("/v1/admin/platform-keys").bearer(TOKEN_ADMIN) catch {
            self.result.* = 0;
            return;
        };
        const json_req = bearer_req.json(self.body) catch {
            self.result.* = 0;
            return;
        };
        const r = json_req.send() catch {
            self.result.* = 0;
            return;
        };
        defer r.deinit();
        self.result.* = r.status;
    }
};

test "integration: concurrent platform key upserts are idempotent" {
    const alloc = std.testing.allocator;
    const h = seedAndHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const body = try std.fmt.allocPrint(alloc, "{{\"provider\":\"{s}\",\"source_workspace_id\":\"{s}\"}}", .{ TEST_PROVIDER, TEST_WS_ID });
    defer alloc.free(body);
    var results = [_]u16{0} ** 5;
    var threads: [5]std.Thread = undefined;
    for (&threads, 0..) |*t, i| {
        t.* = try std.Thread.spawn(.{}, ConcurrentPutCtx.run, .{ConcurrentPutCtx{ .h = h, .body = body, .result = &results[i] }});
    }
    for (&threads) |*t| t.join();
    for (results) |status| try std.testing.expectEqual(@as(u16, 200), status);

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    var q = PgQuery.from(try conn.query("SELECT COUNT(*) FROM platform_llm_keys WHERE provider = $1 AND active = true", .{TEST_PROVIDER}));
    defer q.deinit();
    const row = (try q.next()).?;
    const count = try row.get(i64, 0);
    try std.testing.expectEqual(@as(i64, 1), count);
    cleanupSeedData(conn);
}

test {
    _ = @import("handlers/byok_handlers_unit_test.zig");
}
