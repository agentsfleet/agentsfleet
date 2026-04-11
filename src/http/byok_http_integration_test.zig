// HTTP integration tests for M16_004 Default Provider + BYOK.
//
// Requires DATABASE_URL (or TEST_DATABASE_URL) — skipped otherwise.
// Vault tests (workspace BYOK) also require ENCRYPTION_MASTER_KEY — set
// automatically by setTestEncryptionKey() before the server handles vault calls.
//
// Tiers: T1 (happy path), T2 (edge cases), T3 (auth/role enforcement),
//        T5 (concurrency), T8 (secret safety), T12 (response contract).
//
// Each test starts its own HTTP server on a unique port to avoid cross-test
// state. DB rows are cleaned up in the test body (not deferred) so teardown
// happens before pool.deinit() to avoid connection leaks on exit.

const std = @import("std");
const pg = @import("pg");
const PgQuery = @import("../db/pg_query.zig").PgQuery;
const auth_sessions = @import("../auth/sessions.zig");
const oidc = @import("../auth/oidc.zig");
const queue_redis = @import("../queue/redis.zig");
const common = @import("handlers/common.zig");
const handler = @import("handler.zig");
const http_server = @import("server.zig");
const error_codes = @import("../errors/error_registry.zig");
const telemetry_mod = @import("../observability/telemetry.zig");

// ── Test constants ────────────────────────────────────────────────────────────
// Workspace + tenant UUIDs match the role claims in the JWT tokens below.

const TEST_TENANT_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01";
const TEST_WS_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11";
const TEST_ADMIN_WS_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f22"; // second workspace for cross-ws test
const TEST_PROVIDER = "__test_m16004"; // underscore prefix flags test rows for cleanup
const TEST_REPO_URL = "https://github.com/test/m16004";

const TEST_JWKS_URL = "https://clerk.dev.usezombie.com/.well-known/jwks.json";
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

var next_port = std.atomic.Value(u16).init(39200);

// ── Infrastructure helpers ─────────────────────────────────────────────────────

const HttpResp = struct {
    status: u16,
    body: []u8,
    fn deinit(self: HttpResp, a: std.mem.Allocator) void {
        a.free(self.body);
    }
};

const TestServer = struct {
    pool: *pg.Pool,
    session_store: auth_sessions.SessionStore,
    verifier: oidc.Verifier,
    queue: queue_redis.Client = undefined,
    telemetry: telemetry_mod.Telemetry,
    ctx: handler.Context,
    thread: std.Thread,
    port: u16,

    fn deinit(self: *TestServer) void {
        http_server.stop();
        self.thread.join();
        self.verifier.deinit();
        self.session_store.deinit();
        self.pool.deinit();
    }
};

fn serverThread(ctx: *handler.Context, port: u16) void {
    http_server.serve(ctx, .{ .port = port, .threads = 2, .workers = 2, .max_clients = 64 }) catch |err|
        std.debug.panic("m16004 test server: {s}", .{@errorName(err)});
}

fn startTestServer(alloc: std.mem.Allocator) !*TestServer {
    const db_ctx = (try common.openHandlerTestConn(alloc)) orelse return error.SkipZigTest;
    try setupSeedData(db_ctx.conn);
    db_ctx.pool.release(db_ctx.conn);

    var session_store = auth_sessions.SessionStore.init(alloc);
    var verifier = oidc.Verifier.init(alloc, .{
        .provider = .clerk,
        .jwks_url = TEST_JWKS_URL,
        .issuer = TEST_ISSUER,
        .audience = TEST_AUDIENCE,
        .inline_jwks_json = TEST_JWKS,
    });
    const port = next_port.fetchAdd(1, .acq_rel);

    const srv = try alloc.create(TestServer);
    srv.* = TestServer{
        .pool = db_ctx.pool,
        .session_store = session_store,
        .verifier = verifier,
        .ctx = .{
            .pool = db_ctx.pool,
            .queue = &undefined,
            .alloc = alloc,
            .api_keys = "",
            .oidc = &verifier,
            .auth_sessions = &session_store,
            .app_url = "http://127.0.0.1",
            .api_in_flight_requests = std.atomic.Value(u32).init(0),
            .api_max_in_flight_requests = 64,
            .ready_max_queue_depth = null,
            .ready_max_queue_age_ms = null,
            .telemetry = undefined,
        },
        .thread = undefined,
        .port = port,
    };
    srv.telemetry = telemetry_mod.Telemetry.initTest();
    srv.ctx.telemetry = &srv.telemetry;
    srv.ctx.queue = &srv.queue;
    srv.ctx.oidc = &srv.verifier;
    srv.ctx.auth_sessions = &srv.session_store;
    srv.thread = try std.Thread.spawn(.{}, serverThread, .{ &srv.ctx, port });
    errdefer {
        http_server.stop();
        srv.thread.join();
    }
    try waitForServer(alloc, port);
    return srv;
}

fn waitForServer(alloc: std.mem.Allocator, port: u16) !void {
    const url = try std.fmt.allocPrint(alloc, "http://127.0.0.1:{d}/healthz", .{port});
    defer alloc.free(url);
    var i: usize = 0;
    while (i < 40) : (i += 1) {
        const r = sendReq(alloc, url, .GET, null, null) catch {
            std.Thread.sleep(25 * std.time.ns_per_ms);
            continue;
        };
        defer r.deinit(alloc);
        if (r.status == 200) return;
        std.Thread.sleep(25 * std.time.ns_per_ms);
    }
    return error.ServerStartTimeout;
}

fn sendReq(alloc: std.mem.Allocator, url: []const u8, method: std.http.Method, token: ?[]const u8, body: ?[]const u8) !HttpResp {
    var client: std.http.Client = .{ .allocator = alloc };
    defer client.deinit();
    var auth_val: ?[]u8 = null;
    defer if (auth_val) |v| alloc.free(v);
    var hdrs: [2]std.http.Header = undefined;
    var hc: usize = 0;
    if (token) |t| {
        auth_val = try std.fmt.allocPrint(alloc, "Bearer {s}", .{t});
        hdrs[hc] = .{ .name = "authorization", .value = auth_val.? };
        hc += 1;
    }
    if (body != null) {
        hdrs[hc] = .{ .name = "content-type", .value = "application/json" };
        hc += 1;
    }
    var resp_buf: std.ArrayList(u8) = .{};
    var writer: std.Io.Writer.Allocating = .fromArrayList(alloc, &resp_buf);
    const result = try client.fetch(.{
        .location = .{ .url = url },
        .method = method,
        .payload = body,
        .extra_headers = hdrs[0..hc],
        .response_writer = &writer.writer,
    });
    return .{ .status = @intFromEnum(result.status), .body = try writer.toOwnedSlice() };
}

// ── DB setup / teardown ───────────────────────────────────────────────────────

fn setTestEncryptionKey() void {
    const c = @cImport(@cInclude("stdlib.h"));
    _ = c.setenv("ENCRYPTION_MASTER_KEY", "0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20", 1);
}

fn setupSeedData(conn: *pg.Conn) !void {
    const now_ms = std.time.milliTimestamp();
    _ = try conn.exec("DELETE FROM platform_llm_keys WHERE provider LIKE '\\__test\\_%' ESCAPE '\\'", .{});
    _ = try conn.exec("DELETE FROM vault.secrets WHERE workspace_id IN ($1, $2)", .{ TEST_WS_ID, TEST_ADMIN_WS_ID });
    _ = try conn.exec("DELETE FROM workspaces WHERE workspace_id IN ($1, $2)", .{ TEST_WS_ID, TEST_ADMIN_WS_ID });
    _ = try conn.exec("DELETE FROM tenants WHERE tenant_id = $1", .{TEST_TENANT_ID});
    _ = try conn.exec(
        "INSERT INTO tenants (tenant_id, name, api_key_hash, created_at, updated_at) VALUES ($1, 'M16_004 Test', 'x', $2, $2)",
        .{ TEST_TENANT_ID, now_ms },
    );
    _ = try conn.exec(
        \\INSERT INTO workspaces (workspace_id, tenant_id, repo_url, default_branch, paused, version, created_at, updated_at)
        \\VALUES ($1, $2, $3, 'main', false, 1, $4, $4)
    , .{ TEST_WS_ID, TEST_TENANT_ID, TEST_REPO_URL, now_ms });
    _ = try conn.exec(
        \\INSERT INTO workspaces (workspace_id, tenant_id, repo_url, default_branch, paused, version, created_at, updated_at)
        \\VALUES ($1, $2, $3, 'main', false, 1, $4, $4)
    , .{ TEST_ADMIN_WS_ID, TEST_TENANT_ID, TEST_REPO_URL, now_ms });
}

fn cleanupSeedData(conn: *pg.Conn) void {
    _ = conn.exec("DELETE FROM platform_llm_keys WHERE provider LIKE '\\__test\\_%' ESCAPE '\\'", .{}) catch {};
    _ = conn.exec("DELETE FROM vault.secrets WHERE workspace_id IN ($1, $2)", .{ TEST_WS_ID, TEST_ADMIN_WS_ID }) catch {};
    _ = conn.exec("DELETE FROM workspaces WHERE workspace_id IN ($1, $2)", .{ TEST_WS_ID, TEST_ADMIN_WS_ID }) catch {};
    _ = conn.exec("DELETE FROM tenants WHERE tenant_id = $1", .{TEST_TENANT_ID}) catch {};
}

// ── T1 + T12: Admin platform key lifecycle ────────────────────────────────────

test "integration: M16_004 admin platform key PUT→GET→DELETE lifecycle" {
    const alloc = std.testing.allocator;
    const srv = try startTestServer(alloc);
    defer {
        srv.deinit();
        alloc.destroy(srv);
    }

    const base = try std.fmt.allocPrint(alloc, "http://127.0.0.1:{d}/v1/admin/platform-keys", .{srv.port});
    defer alloc.free(base);

    // PUT: upsert platform key pointing to admin workspace — T1 happy path
    const put_body = try std.fmt.allocPrint(alloc, "{{\"provider\":\"{s}\",\"source_workspace_id\":\"{s}\"}}", .{ TEST_PROVIDER, TEST_WS_ID });
    defer alloc.free(put_body);
    {
        const r = try sendReq(alloc, base, .PUT, TOKEN_ADMIN, put_body);
        defer r.deinit(alloc);
        try std.testing.expectEqual(@as(u16, 200), r.status);
        // T12: response must contain provider and active=true
        try std.testing.expect(std.mem.indexOf(u8, r.body, TEST_PROVIDER) != null);
        try std.testing.expect(std.mem.indexOf(u8, r.body, "true") != null);
        // T8: response must NOT contain key material
        try std.testing.expect(std.mem.indexOf(u8, r.body, "sk-") == null);
    }

    // GET: list must include the row just upserted — T1 + T12
    {
        const r = try sendReq(alloc, base, .GET, TOKEN_ADMIN, null);
        defer r.deinit(alloc);
        try std.testing.expectEqual(@as(u16, 200), r.status);
        try std.testing.expect(std.mem.indexOf(u8, r.body, TEST_PROVIDER) != null);
        // T8: response must NOT contain key material in list output
        try std.testing.expect(std.mem.indexOf(u8, r.body, "api_key") == null);
    }

    // DELETE: deactivate — T1
    const del_url = try std.fmt.allocPrint(alloc, "{s}/{s}", .{ base, TEST_PROVIDER });
    defer alloc.free(del_url);
    {
        const r = try sendReq(alloc, del_url, .DELETE, TOKEN_ADMIN, null);
        defer r.deinit(alloc);
        try std.testing.expectEqual(@as(u16, 200), r.status);
        try std.testing.expect(std.mem.indexOf(u8, r.body, "false") != null); // active=false
    }

    const conn = try srv.pool.acquire();
    defer srv.pool.release(conn);
    cleanupSeedData(conn);
}

// ── T3: Admin platform key enforces admin-only access ────────────────────────

test "integration: M16_004 admin platform key enforces admin role and validates input" {
    const alloc = std.testing.allocator;
    const srv = try startTestServer(alloc);
    defer {
        srv.deinit();
        alloc.destroy(srv);
    }

    const url = try std.fmt.allocPrint(alloc, "http://127.0.0.1:{d}/v1/admin/platform-keys", .{srv.port});
    defer alloc.free(url);
    const valid_body = try std.fmt.allocPrint(alloc, "{{\"provider\":\"{s}\",\"source_workspace_id\":\"{s}\"}}", .{ TEST_PROVIDER, TEST_WS_ID });
    defer alloc.free(valid_body);

    // T3: no token → 401
    {
        const r = try sendReq(alloc, url, .PUT, null, valid_body);
        defer r.deinit(alloc);
        try std.testing.expectEqual(@as(u16, 401), r.status);
    }
    // T3: user role → 403
    {
        const r = try sendReq(alloc, url, .PUT, TOKEN_USER, valid_body);
        defer r.deinit(alloc);
        try std.testing.expectEqual(@as(u16, 403), r.status);
        try std.testing.expect(std.mem.indexOf(u8, r.body, error_codes.ERR_INSUFFICIENT_ROLE) != null);
    }
    // T3: operator role → 403 (admin-only endpoint)
    {
        const r = try sendReq(alloc, url, .PUT, TOKEN_OPERATOR, valid_body);
        defer r.deinit(alloc);
        try std.testing.expectEqual(@as(u16, 403), r.status);
    }
    // T2: empty provider → 400
    {
        const r = try sendReq(alloc, url, .PUT, TOKEN_ADMIN, "{\"provider\":\"\",\"source_workspace_id\":\"0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11\"}");
        defer r.deinit(alloc);
        try std.testing.expectEqual(@as(u16, 400), r.status);
    }
    // T2: 33-char provider (over limit) → 400
    {
        const long = "a" ** 33;
        const b = try std.fmt.allocPrint(alloc, "{{\"provider\":\"{s}\",\"source_workspace_id\":\"{s}\"}}", .{ long, TEST_WS_ID });
        defer alloc.free(b);
        const r = try sendReq(alloc, url, .PUT, TOKEN_ADMIN, b);
        defer r.deinit(alloc);
        try std.testing.expectEqual(@as(u16, 400), r.status);
    }
    // T2: non-UUIDv7 source_workspace_id → 400
    {
        const r = try sendReq(alloc, url, .PUT, TOKEN_ADMIN, "{\"provider\":\"kimi\",\"source_workspace_id\":\"not-a-uuid\"}");
        defer r.deinit(alloc);
        try std.testing.expectEqual(@as(u16, 400), r.status);
    }
    // T3: malformed JSON → 400
    {
        const r = try sendReq(alloc, url, .PUT, TOKEN_ADMIN, "{bad json");
        defer r.deinit(alloc);
        try std.testing.expectEqual(@as(u16, 400), r.status);
    }
    // T3: GET also enforces admin-only
    {
        const r = try sendReq(alloc, url, .GET, TOKEN_OPERATOR, null);
        defer r.deinit(alloc);
        try std.testing.expectEqual(@as(u16, 403), r.status);
    }

    const conn = try srv.pool.acquire();
    defer srv.pool.release(conn);
    cleanupSeedData(conn);
}

test "integration: M16_004 workspace BYOK credential lifecycle and key never in response" {
    setTestEncryptionKey();
    const alloc = std.testing.allocator;
    const srv = try startTestServer(alloc);
    defer {
        srv.deinit();
        alloc.destroy(srv);
    }
    const url = try std.fmt.allocPrint(alloc, "http://127.0.0.1:{d}/v1/workspaces/{s}/credentials/llm", .{ srv.port, TEST_WS_ID });
    defer alloc.free(url);
    const secret_key = "sk-ant-SUPER-SECRET-DO-NOT-LEAK-1234";
    // PUT: store key — T1
    {
        const b = try std.fmt.allocPrint(alloc, "{{\"provider\":\"anthropic\",\"api_key\":\"{s}\"}}", .{secret_key});
        defer alloc.free(b);
        const r = try sendReq(alloc, url, .PUT, TOKEN_OPERATOR, b);
        defer r.deinit(alloc);
        try std.testing.expectEqual(@as(u16, 204), r.status);
        // T12: 204 means empty body
        try std.testing.expectEqual(@as(usize, 0), r.body.len);
    }

    { // GET: has_key=true, provider correct, key never in response (T1+T8+T12)
        const r = try sendReq(alloc, url, .GET, TOKEN_OPERATOR, null);
        defer r.deinit(alloc);
        try std.testing.expectEqual(@as(u16, 200), r.status);
        try std.testing.expect(std.mem.indexOf(u8, r.body, "true") != null);
        try std.testing.expect(std.mem.indexOf(u8, r.body, "anthropic") != null);
        try std.testing.expect(std.mem.indexOf(u8, r.body, secret_key) == null);
        try std.testing.expect(std.mem.indexOf(u8, r.body, "sk-ant-") == null);
    }

    { // DELETE: remove key — T1
        const r = try sendReq(alloc, url, .DELETE, TOKEN_OPERATOR, null);
        defer r.deinit(alloc);
        try std.testing.expectEqual(@as(u16, 204), r.status);
    }
    { // GET after DELETE: has_key=false — T1
        const r = try sendReq(alloc, url, .GET, TOKEN_OPERATOR, null);
        defer r.deinit(alloc);
        try std.testing.expectEqual(@as(u16, 200), r.status);
        try std.testing.expect(std.mem.indexOf(u8, r.body, "false") != null);
        try std.testing.expect(std.mem.indexOf(u8, r.body, secret_key) == null);
    }

    const conn = try srv.pool.acquire();
    defer srv.pool.release(conn);
    cleanupSeedData(conn);
}

// ── T3 + T8: Workspace BYOK enforces operator role and workspace scope ─────────

test "integration: M16_004 workspace BYOK enforces operator role and workspace boundary" {
    const alloc = std.testing.allocator;
    const srv = try startTestServer(alloc);
    defer {
        srv.deinit();
        alloc.destroy(srv);
    }

    const url = try std.fmt.allocPrint(alloc, "http://127.0.0.1:{d}/v1/workspaces/{s}/credentials/llm", .{ srv.port, TEST_WS_ID });
    defer alloc.free(url);
    // A URL for a different workspace — the operator token is scoped to TEST_WS_ID
    const other_url = try std.fmt.allocPrint(alloc, "http://127.0.0.1:{d}/v1/workspaces/{s}/credentials/llm", .{ srv.port, TEST_ADMIN_WS_ID });
    defer alloc.free(other_url);
    const valid_body = "{\"provider\":\"anthropic\",\"api_key\":\"sk-test-1234\"}";

    // T3: no token → 401
    {
        const r = try sendReq(alloc, url, .PUT, null, valid_body);
        defer r.deinit(alloc);
        try std.testing.expectEqual(@as(u16, 401), r.status);
    }
    { // T3: user role → 403
        const r = try sendReq(alloc, url, .PUT, TOKEN_USER, valid_body);
        defer r.deinit(alloc);
        try std.testing.expectEqual(@as(u16, 403), r.status);
    }
    { // T8: cross-workspace → 403 or 400
        const r = try sendReq(alloc, other_url, .PUT, TOKEN_OPERATOR, valid_body);
        defer r.deinit(alloc);
        try std.testing.expect(r.status == 403 or r.status == 400);
    }
    // T2: provider too long → 400
    {
        const long = "a" ** 33;
        const b = try std.fmt.allocPrint(alloc, "{{\"provider\":\"{s}\",\"api_key\":\"sk-x\"}}", .{long});
        defer alloc.free(b);
        const r = try sendReq(alloc, url, .PUT, TOKEN_OPERATOR, b);
        defer r.deinit(alloc);
        try std.testing.expectEqual(@as(u16, 400), r.status);
    }
    { // T2: api_key too long (257 chars) → 400
        const long_key = "sk-" ++ ("a" ** 254);
        const b = try std.fmt.allocPrint(alloc, "{{\"provider\":\"anthropic\",\"api_key\":\"{s}\"}}", .{long_key});
        defer alloc.free(b);
        const r = try sendReq(alloc, url, .PUT, TOKEN_OPERATOR, b);
        defer r.deinit(alloc);
        try std.testing.expectEqual(@as(u16, 400), r.status);
    }
    { // T2: empty api_key → 400
        const r = try sendReq(alloc, url, .PUT, TOKEN_OPERATOR, "{\"provider\":\"anthropic\",\"api_key\":\"\"}");
        defer r.deinit(alloc);
        try std.testing.expectEqual(@as(u16, 400), r.status);
    }
    { // T3: GET enforces operator role
        const r = try sendReq(alloc, url, .GET, TOKEN_USER, null);
        defer r.deinit(alloc);
        try std.testing.expectEqual(@as(u16, 403), r.status);
    }

    const conn = try srv.pool.acquire();
    defer srv.pool.release(conn);
    cleanupSeedData(conn);
}
const ConcurrentPutCtx = struct {
    url: []const u8,
    body: []const u8,
    result: *u16,
    fn run(self: ConcurrentPutCtx) void {
        const r = sendReq(std.heap.page_allocator, self.url, .PUT, TOKEN_ADMIN, self.body) catch {
            self.result.* = 0;
            return;
        };
        defer r.deinit(std.heap.page_allocator);
        self.result.* = r.status;
    }
};

test "integration: M16_004 concurrent platform key upserts are idempotent" {
    const alloc = std.testing.allocator;
    const srv = try startTestServer(alloc);
    defer {
        srv.deinit();
        alloc.destroy(srv);
    }
    const url = try std.fmt.allocPrint(alloc, "http://127.0.0.1:{d}/v1/admin/platform-keys", .{srv.port});
    defer alloc.free(url);
    const body = try std.fmt.allocPrint(alloc, "{{\"provider\":\"{s}\",\"source_workspace_id\":\"{s}\"}}", .{ TEST_PROVIDER, TEST_WS_ID });
    defer alloc.free(body);
    var results = [_]u16{0} ** 5;
    var threads: [5]std.Thread = undefined;
    for (&threads, 0..) |*t, i| {
        t.* = try std.Thread.spawn(.{}, ConcurrentPutCtx.run, .{ConcurrentPutCtx{ .url = url, .body = body, .result = &results[i] }});
    }
    for (&threads) |*t| t.join();
    for (results) |status| try std.testing.expectEqual(@as(u16, 200), status);
    const conn = try srv.pool.acquire();
    defer srv.pool.release(conn);
    var q = PgQuery.from(try conn.query("SELECT COUNT(*) FROM platform_llm_keys WHERE provider = $1 AND active = true", .{TEST_PROVIDER}));
    defer q.deinit();
    const row = (try q.next()).?;
    const count = try row.get(i64, 0);
    try std.testing.expectEqual(@as(i64, 1), count);
    cleanupSeedData(conn);
}
test {
    _ = @import("handlers/m16_004_handler_unit_test.zig");
}
