// HTTP integration tests for M18_001 zombie execution telemetry handlers.
//
// Runs a live httpz test server (same pattern as rbac_http_integration_test.zig).
// Covers T1-T10 per spec dims 4.4, 3.3, 3.4. Each test block spins one server.
//
// T1 — GET /internal/v1/telemetry with .user JWT → 403 (review RBAC gap)
// T2 — GET /internal/v1/telemetry with .operator JWT → 403 (admin required)
// T3 — GET /internal/v1/telemetry with .admin JWT → 200
// T4 — GET /internal/v1/telemetry with no token → 401
// T5 — GET /internal/v1/telemetry with limit=501 → 400
// T6 — Customer endpoint valid JWT, correct workspace → 200
// T7 — Customer endpoint path workspace ≠ JWT workspace → 403
// T8 — Customer endpoint limit=0 → 400
// T9 — Customer endpoint limit=201 → 400
// T10 — Customer endpoint invalid cursor → 400
//
// Skips gracefully if TEST_DATABASE_URL / DATABASE_URL not set.

const std = @import("std");
const pg = @import("pg");
const auth_sessions = @import("../../auth/sessions.zig");
const oidc = @import("../../auth/oidc.zig");
const queue_redis = @import("../../queue/redis.zig");
const common = @import("common.zig");
const handler = @import("../../http/handler.zig");
const http_server = @import("../../http/server.zig");
const telemetry_mod = @import("../../observability/telemetry.zig");
const test_port = @import("../test_port.zig");

const TEST_TENANT_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01";
const TEST_WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11";
const TEST_ZOMBIE_ID = "zombie-m18-http-test";
const TEST_REPO_URL = "https://github.com/usezombie/m18-http-test";
const TEST_JWKS_URL = "https://clerk.dev.usezombie.com/.well-known/jwks.json";
const TEST_ISSUER = "https://clerk.dev.usezombie.com";
const TEST_AUDIENCE = "https://api.usezombie.com";
// RSA pub key; matching tokens below are signed with the corresponding private key.
// Shared with rbac_http_integration_test.zig — kid "rbac-test-kid".
const TEST_JWKS =
    \\{"keys":[{"kty":"RSA","n":"2hg972tpbq8H6kzRZ3oVL4wZ9bO-04gJ6gCig68aluyRBzagx-7XXPCiuX80oBHBVj51kvMjT_QDNXfrwzjy4cPbwiVV4HqOGpeIZkPEopfyzs4G7mjiQmx0YuM_5WQUlUjji6Y_DfeaoH-yOhTWBMBVoI0vW_1n66CFaGuEarj3VasdWYxObJTBAM6Jn4XZDcDsBBPNGO4ku7yILkfi11FqXfBP2V8NT0hAGXVAxlWwv-8up1RDzgACp-8JWoC2-kOUJN82fGenDGKq9hW_sumO-4YPNP4U1smnw5jzLlvKa0LBrYG8IgW-3Dniuq2mojhrD_ZQClUd5rF42OyYqw","e":"AQAB","kid":"rbac-test-kid","use":"sig","alg":"RS256"}]}
;
// .user role, workspace = TEST_WORKSPACE_ID, exp = 4102444800 (year 2100)
const TOKEN_USER =
    "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCIsImtpZCI6InJiYWMtdGVzdC1raWQifQ.eyJzdWIiOiJ1c2VyX3Rlc3QiLCJpc3MiOiJodHRwczovL2NsZXJrLmRldi51c2V6b21iaWUuY29tIiwiYXVkIjoiaHR0cHM6Ly9hcGkudXNlem9tYmllLmNvbSIsImV4cCI6NDEwMjQ0NDgwMCwibWV0YWRhdGEiOnsidGVuYW50X2lkIjoiMDE5NWI0YmEtOGQzYS03ZjEzLThhYmMtMmIzZTFlMGE2ZjAxIiwid29ya3NwYWNlX2lkIjoiMDE5NWI0YmEtOGQzYS03ZjEzLThhYmMtMmIzZTFlMGE2ZjExIiwicm9sZSI6InVzZXIifX0.UEZ3huXtn6bXpa3M1EJZ2QmqLtXewLsHYP5ggTeRg-lgX-Vzp2ECvTsGgzhCSxNNPudRXYgdTsPa1ufIKv_5n1SvuoCRw2eRZfTUp5a_68KbScepnLVx5LaRJmoMyPP8Q_DPYwB0vHm1NCPRIfFqzcBOpLw01Ygkse4mTq19JPE4vcINmaVTWMiN02_ScU0DWhzhzx3_B1_vCBC3wxCpVuM_wqOHDUCnBEPkM-YVQcZrtQIdXPfRzZ2XFRVWFn-E7s0EWBpEP1wSCh31ymki_E1vlnrW4q9ZKNBYnZX0ErvJlcqH2U7nIsFlLYULNP_4mdYrDaWvBSSYZROoK1d8WQ";
// .operator role
const TOKEN_OPERATOR =
    "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCIsImtpZCI6InJiYWMtdGVzdC1raWQifQ.eyJzdWIiOiJ1c2VyX3Rlc3QiLCJpc3MiOiJodHRwczovL2NsZXJrLmRldi51c2V6b21iaWUuY29tIiwiYXVkIjoiaHR0cHM6Ly9hcGkudXNlem9tYmllLmNvbSIsImV4cCI6NDEwMjQ0NDgwMCwibWV0YWRhdGEiOnsidGVuYW50X2lkIjoiMDE5NWI0YmEtOGQzYS03ZjEzLThhYmMtMmIzZTFlMGE2ZjAxIiwid29ya3NwYWNlX2lkIjoiMDE5NWI0YmEtOGQzYS03ZjEzLThhYmMtMmIzZTFlMGE2ZjExIiwicm9sZSI6Im9wZXJhdG9yIn19.V84uE69RTLrRef0sogegUcUZeKWx8E68GEruFoS8HegUa3o7bVCfQjlkllNSbtUut919EygbQv1C16BMfNTOAv1Lvl3AeLYPYr4ni6EnzzGllbyxDw1aY68AGWEEvKOUxd5wCGl8BnEqaOKX7KNNbAOV4AzJNWqnV-uxJiZl6oDtqi8bsSF1HAm9qY9MAl6AwoZLGnT_x6ux_3vfKy_9ckZSbgjN7laZOMqQ5nwwcaSpwYNm_3ZpXJLgHYMVxel2M4rT0SIaFh__rE42yGE9FBDRUFoyktGOR3NYPOzogjj3tfOoecC8NEhrwifzXcSNVAiHOMnmXojjAPEUORovPg";
// .admin role
const TOKEN_ADMIN =
    "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCIsImtpZCI6InJiYWMtdGVzdC1raWQifQ.eyJzdWIiOiJ1c2VyX3Rlc3QiLCJpc3MiOiJodHRwczovL2NsZXJrLmRldi51c2V6b21iaWUuY29tIiwiYXVkIjoiaHR0cHM6Ly9hcGkudXNlem9tYmllLmNvbSIsImV4cCI6NDEwMjQ0NDgwMCwibWV0YWRhdGEiOnsidGVuYW50X2lkIjoiMDE5NWI0YmEtOGQzYS03ZjEzLThhYmMtMmIzZTFlMGE2ZjAxIiwid29ya3NwYWNlX2lkIjoiMDE5NWI0YmEtOGQzYS03ZjEzLThhYmMtMmIzZTFlMGE2ZjExIiwicm9sZSI6ImFkbWluIn19.sTBn0XSWWTLEd5fSEcClUIhMCVeuXjljxYymPdMwahzAhhkg6P3MVhmtiPC_B_nFQQ7WU8cAS7kSvPL3Fcs9feb06C7zosm63ByUdqigATBVILyCDt43em2pG8cGOgj-bhkxIoWsGai5hdzu4vzOEYMMLzvN_V_QPMrjqWnLIiCVXk9_Mcdpx5xbUfA1hAwg_bM8CTlezRQ5ys8oxQDymx6cvuUaW_M69jYEgpFeETNpYWmuvMWIuVlT2wpME9-8l3ytYpE0ZxnGG_HQTY1bXRkg_ZC02uYs90lhOWEs9cPG4Uz0HU6rNSnRK71bAtlgQUlcUZZSK-Gg4GbFM0SVPg";

const HttpResponse = struct {
    status: u16,
    body: []u8,
    fn deinit(self: HttpResponse, alloc: std.mem.Allocator) void { alloc.free(self.body); }
};

// Heap-allocated running server — same pattern as rbac_http_integration_test.zig
// and byok_http_integration_test.zig. Heap allocation guarantees that self-referential
// pointers (ctx.oidc = &verifier, etc.) remain valid after startTestServer returns.
const TestServer = struct {
    pool: *pg.Pool,
    session_store: auth_sessions.SessionStore,
    verifier: oidc.Verifier,
    queue: queue_redis.Client = undefined,
    telemetry: telemetry_mod.Telemetry,
    ctx: handler.Context,
    thread: std.Thread,
    port: u16,

    // Correct teardown order: stop server first, then release pool.
    // Pool must outlive the server thread so handlers can complete in-flight requests.
    // Caller must call alloc.destroy(self) after deinit() — matches rbac_http_integration_test pattern.
    fn deinit(self: *TestServer) void {
        http_server.stop();
        self.thread.join();
        self.verifier.deinit();
        self.session_store.deinit();
        self.pool.deinit();
    }
};

fn serverThread(ctx: *handler.Context, port: u16) void {
    http_server.serve(ctx, .{ .port = port, .threads = 1, .workers = 1, .max_clients = 64 }) catch |e|
        std.debug.panic("m18 test server: {s}", .{@errorName(e)});
}

fn seedTestData(conn: *pg.Conn) !void {
    const now_ms = std.time.milliTimestamp();
    // Idempotent seed — use upserts instead of DELETE so we don't trip FK constraints
    // from sibling tables (platform_llm_keys, activity_events, etc.) that other tests
    // may have populated against this test workspace.
    _ = try conn.exec("DELETE FROM zombie_execution_telemetry WHERE workspace_id = $1", .{TEST_WORKSPACE_ID});
    _ = try conn.exec(
        \\INSERT INTO tenants (tenant_id, name, api_key_hash, created_at, updated_at)
        \\VALUES ($1, 'M18Test', 'managed', $2, $2)
        \\ON CONFLICT (tenant_id) DO NOTHING
    , .{ TEST_TENANT_ID, now_ms });
    _ = try conn.exec(
        \\INSERT INTO workspaces (workspace_id, tenant_id, repo_url, default_branch, paused, version, created_at, updated_at)
        \\VALUES ($1, $2, $3, 'main', false, 1, $4, $4)
        \\ON CONFLICT (workspace_id) DO NOTHING
    , .{ TEST_WORKSPACE_ID, TEST_TENANT_ID, TEST_REPO_URL, now_ms });
    _ = try conn.exec(
        \\INSERT INTO workspace_entitlements
        \\  (entitlement_id, workspace_id, plan_tier, max_stages, max_distinct_skills,
        \\   allow_custom_skills, enable_agent_scoring, agent_scoring_weights_json,
        \\   scoring_context_max_tokens, created_at, updated_at)
        \\VALUES ('0195b4ba-8d3a-7f13-8abc-2b3e1e0a6ff1', $1, 'FREE', 2, 8,
        \\        false, false, '{"completion":0.4,"error_rate":0.3,"latency":0.2,"resource":0.1}',
        \\        2048, $2, $2)
        \\ON CONFLICT (workspace_id) DO UPDATE SET plan_tier=EXCLUDED.plan_tier, updated_at=EXCLUDED.updated_at
    , .{ TEST_WORKSPACE_ID, now_ms });
    _ = try conn.exec(
        \\INSERT INTO workspace_billing_state
        \\  (billing_id, workspace_id, plan_tier, plan_sku, billing_status, adapter, subscription_id, created_at, updated_at)
        \\VALUES ('0195b4ba-8d3a-7f13-8abc-2b3e1e0a6ff2', $1, 'FREE', 'free', 'ACTIVE', 'noop', 'sub-m18', $2, $2)
        \\ON CONFLICT (workspace_id) DO UPDATE SET plan_tier=EXCLUDED.plan_tier, updated_at=EXCLUDED.updated_at
    , .{ TEST_WORKSPACE_ID, now_ms });
}

fn cleanupTestData(conn: *pg.Conn) void {
    // Only clean up what this test owns. tenants/workspaces/entitlements/billing_state
    // are shared fixtures — leaving them in place is safe and avoids FK violations from
    // sibling tables that may reference them.
    _ = conn.exec("DELETE FROM zombie_execution_telemetry WHERE workspace_id = $1", .{TEST_WORKSPACE_ID}) catch {};
}

fn startTestServer(alloc: std.mem.Allocator) !*TestServer {
    const db_ctx = (try common.openHandlerTestConn(alloc)) orelse return error.SkipZigTest;
    try seedTestData(db_ctx.conn);
    db_ctx.pool.release(db_ctx.conn);

    const port = try test_port.allocFreePort();

    const srv = try alloc.create(TestServer);
    srv.* = TestServer{
        .pool = db_ctx.pool,
        .session_store = auth_sessions.SessionStore.init(alloc),
        .verifier = oidc.Verifier.init(alloc, .{
            .provider = .clerk,
            .jwks_url = TEST_JWKS_URL,
            .issuer = TEST_ISSUER,
            .audience = TEST_AUDIENCE,
            .inline_jwks_json = TEST_JWKS,
        }),
        .ctx = .{
            .pool = db_ctx.pool,
            .queue = undefined,
            .alloc = alloc,
            .api_keys = "",
            .oidc = undefined,
            .auth_sessions = undefined,
            .app_url = "http://127.0.0.1",
            .api_in_flight_requests = std.atomic.Value(u32).init(0),
            .api_max_in_flight_requests = 64,
            .ready_max_queue_depth = null,
            .ready_max_queue_age_ms = null,
            .telemetry = undefined,
        },
        .telemetry = undefined,
        .thread = undefined,
        .port = port,
    };
    // Fix up self-referential pointers after heap placement.
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

    // Wait for /healthz to confirm the server is accepting connections.
    const url = try std.fmt.allocPrint(alloc, "http://127.0.0.1:{d}/healthz", .{port});
    defer alloc.free(url);
    var attempts: usize = 0;
    while (attempts < 40) : (attempts += 1) {
        const r = get(alloc, url, null) catch { std.Thread.sleep(25_000_000); continue; };
        defer r.deinit(alloc);
        if (r.status == 200) return srv;
        std.Thread.sleep(25_000_000);
    }
    return error.ConnectionTimedOut;
}

fn get(alloc: std.mem.Allocator, url: []const u8, token: ?[]const u8) !HttpResponse {
    var client: std.http.Client = .{ .allocator = alloc };
    defer client.deinit();
    var auth_buf: [1024]u8 = undefined;
    var headers: [1]std.http.Header = undefined;
    var hlen: usize = 0;
    if (token) |t| {
        headers[0] = .{ .name = "authorization", .value = try std.fmt.bufPrint(&auth_buf, "Bearer {s}", .{t}) };
        hlen = 1;
    }
    var body: std.ArrayList(u8) = .{};
    var w: std.Io.Writer.Allocating = .fromArrayList(alloc, &body);
    const res = try client.fetch(.{ .location = .{ .url = url }, .method = .GET, .extra_headers = headers[0..hlen], .response_writer = &w.writer });
    return .{ .status = @intFromEnum(res.status), .body = try w.toOwnedSlice() };
}

// ── T1-T5: Operator endpoint RBAC and limit enforcement ──────────────────────

test "M18_001: internal telemetry RBAC and limit enforcement" {
    const alloc = std.testing.allocator;
    const srv = try startTestServer(alloc);
    defer {
        if (srv.pool.acquire()) |c| { cleanupTestData(c); srv.pool.release(c); } else |_| {}
        srv.deinit();
        alloc.destroy(srv);
    }

    const base = try std.fmt.allocPrint(alloc, "http://127.0.0.1:{d}/internal/v1/telemetry", .{srv.port});
    defer alloc.free(base);

    // T1: .user role → 403
    { const r = try get(alloc, base, TOKEN_USER); defer r.deinit(alloc); try std.testing.expectEqual(@as(u16, 403), r.status); }
    // T2: .operator role → 403 (admin required; operator.allows(.admin) = false)
    { const r = try get(alloc, base, TOKEN_OPERATOR); defer r.deinit(alloc); try std.testing.expectEqual(@as(u16, 403), r.status); }
    // T3: .admin role → 200
    { const r = try get(alloc, base, TOKEN_ADMIN); defer r.deinit(alloc); try std.testing.expectEqual(@as(u16, 200), r.status); }
    // T4: no token → 401
    { const r = try get(alloc, base, null); defer r.deinit(alloc); try std.testing.expectEqual(@as(u16, 401), r.status); }
    // T5: limit=501 → 400
    const url_limit501 = try std.fmt.allocPrint(alloc, "{s}?limit=501", .{base});
    defer alloc.free(url_limit501);
    { const r = try get(alloc, url_limit501, TOKEN_ADMIN); defer r.deinit(alloc); try std.testing.expectEqual(@as(u16, 400), r.status); }
}

// ── T6-T10: Customer endpoint auth, limit, and cursor enforcement ─────────────

test "M18_001: customer telemetry endpoint auth, limit and cursor enforcement" {
    const alloc = std.testing.allocator;
    const srv = try startTestServer(alloc);
    defer {
        if (srv.pool.acquire()) |c| { cleanupTestData(c); srv.pool.release(c); } else |_| {}
        srv.deinit();
        alloc.destroy(srv);
    }

    const ws_base = try std.fmt.allocPrint(alloc, "http://127.0.0.1:{d}/v1/workspaces/{s}/zombies/{s}/telemetry", .{ srv.port, TEST_WORKSPACE_ID, TEST_ZOMBIE_ID });
    defer alloc.free(ws_base);

    // T6: valid JWT, correct workspace → 200
    { const r = try get(alloc, ws_base, TOKEN_USER); defer r.deinit(alloc); try std.testing.expectEqual(@as(u16, 200), r.status); }
    // T7: JWT workspace ≠ path workspace → 403
    const wrong_ws = try std.fmt.allocPrint(alloc, "http://127.0.0.1:{d}/v1/workspaces/0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f99/zombies/{s}/telemetry", .{ srv.port, TEST_ZOMBIE_ID });
    defer alloc.free(wrong_ws);
    { const r = try get(alloc, wrong_ws, TOKEN_USER); defer r.deinit(alloc); try std.testing.expectEqual(@as(u16, 403), r.status); }
    // T8: limit=0 → 400
    const url_limit0 = try std.fmt.allocPrint(alloc, "{s}?limit=0", .{ws_base});
    defer alloc.free(url_limit0);
    { const r = try get(alloc, url_limit0, TOKEN_USER); defer r.deinit(alloc); try std.testing.expectEqual(@as(u16, 400), r.status); }
    // T9: limit=201 → 400
    const url_limit201 = try std.fmt.allocPrint(alloc, "{s}?limit=201", .{ws_base});
    defer alloc.free(url_limit201);
    { const r = try get(alloc, url_limit201, TOKEN_USER); defer r.deinit(alloc); try std.testing.expectEqual(@as(u16, 400), r.status); }
    // T10: invalid base64 cursor → 400
    const ubad = try std.fmt.allocPrint(alloc, "{s}?cursor=!!invalid!!", .{ws_base});
    defer alloc.free(ubad);
    { const r = try get(alloc, ubad, TOKEN_USER); defer r.deinit(alloc); try std.testing.expectEqual(@as(u16, 400), r.status); }
}
