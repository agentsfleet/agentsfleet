// Integration tests for POST /v1/zombies/{id}/steer (M23_001 §1).
//
// T1.1 — valid bearer, idle zombie         → 200 {message_queued:true, execution_active:false}
// T1.2 — valid bearer, zombie with exec_id → 200 {execution_active:true, execution_id:non-null}
// T1.3 — no Bearer token                   → 401
// T1.4 — zombie owned by different ws      → 404
// T1.5 — empty message body                → 400
// T1.6 — message > 8192 bytes              → 400
//
// T1.1 and T1.2 require REDIS_URL in env; they skip gracefully when Redis is unavailable.
// T1.3-T1.6 use queue=undefined (requests fail before Redis is reached).
//
// Skips all tests if TEST_DATABASE_URL / DATABASE_URL is not set.

const std = @import("std");
const pg = @import("pg");
const auth_sessions = @import("../../auth/sessions.zig");
const oidc = @import("../../auth/oidc.zig");
const queue_redis = @import("../../queue/redis.zig");
const auth_mw = @import("../../auth/middleware/mod.zig");
const common = @import("common.zig");
const handler = @import("../../http/handler.zig");
const http_server = @import("../../http/server.zig");
const telemetry_mod = @import("../../observability/telemetry.zig");
const test_port = @import("../test_port.zig");
const PgQuery = @import("../../db/pg_query.zig").PgQuery;

const ALLOC = std.testing.allocator;

// Workspace + zombie IDs — unique segment `0aaa` avoids collisions with other test files.
const TEST_TENANT_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01";
const TEST_WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11";
const OTHER_WS_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0aff01";
const ZOMBIE_IDLE = "0195b4ba-8d3a-7f13-8abc-2b3e1e0aaa01";
const ZOMBIE_ACTIVE = "0195b4ba-8d3a-7f13-8abc-2b3e1e0aaa02";
const ZOMBIE_OTHER_WS = "0195b4ba-8d3a-7f13-8abc-2b3e1e0aaa03";
const SESSION_ACTIVE = "0195b4ba-8d3a-7f13-8abc-2b3e1e0aaa10";
const ACTIVE_EXEC_ID = "test-exec-steer-001";

// Shared JWKS + tokens (RSA, kid="rbac-test-kid").
// Same key pair as rbac_http_integration_test.zig — do not regenerate independently.
const TEST_JWKS_URL = "https://clerk.dev.usezombie.com/.well-known/jwks.json";
const TEST_ISSUER = "https://clerk.dev.usezombie.com";
const TEST_AUDIENCE = "https://api.usezombie.com";
const TEST_JWKS =
    \\{"keys":[{"kty":"RSA","n":"2hg972tpbq8H6kzRZ3oVL4wZ9bO-04gJ6gCig68aluyRBzagx-7XXPCiuX80oBHBVj51kvMjT_QDNXfrwzjy4cPbwiVV4HqOGpeIZkPEopfyzs4G7mjiQmx0YuM_5WQUlUjji6Y_DfeaoH-yOhTWBMBVoI0vW_1n66CFaGuEarj3VasdWYxObJTBAM6Jn4XZDcDsBBPNGO4ku7yILkfi11FqXfBP2V8NT0hAGXVAxlWwv-8up1RDzgACp-8JWoC2-kOUJN82fGenDGKq9hW_sumO-4YPNP4U1smnw5jzLlvKa0LBrYG8IgW-3Dniuq2mojhrD_ZQClUd5rF42OyYqw","e":"AQAB","kid":"rbac-test-kid","use":"sig","alg":"RS256"}]}
;
// .operator role, workspace_id = TEST_WORKSPACE_ID, exp = 4102444800 (year 2100)
const TOKEN_OPERATOR =
    "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCIsImtpZCI6InJiYWMtdGVzdC1raWQifQ.eyJzdWIiOiJ1c2VyX3Rlc3QiLCJpc3MiOiJodHRwczovL2NsZXJrLmRldi51c2V6b21iaWUuY29tIiwiYXVkIjoiaHR0cHM6Ly9hcGkudXNlem9tYmllLmNvbSIsImV4cCI6NDEwMjQ0NDgwMCwibWV0YWRhdGEiOnsidGVuYW50X2lkIjoiMDE5NWI0YmEtOGQzYS03ZjEzLThhYmMtMmIzZTFlMGE2ZjAxIiwid29ya3NwYWNlX2lkIjoiMDE5NWI0YmEtOGQzYS03ZjEzLThhYmMtMmIzZTFlMGE2ZjExIiwicm9sZSI6Im9wZXJhdG9yIn19.V84uE69RTLrRef0sogegUcUZeKWx8E68GEruFoS8HegUa3o7bVCfQjlkllNSbtUut919EygbQv1C16BMfNTOAv1Lvl3AeLYPYr4ni6EnzzGllbyxDw1aY68AGWEEvKOUxd5wCGl8BnEqaOKX7KNNbAOV4AzJNWqnV-uxJiZl6oDtqi8bsSF1HAm9qY9MAl6AwoZLGnT_x6ux_3vfKy_9ckZSbgjN7laZOMqQ5nwwcaSpwYNm_3ZpXJLgHYMVxel2M4rT0SIaFh__rE42yGE9FBDRUFoyktGOR3NYPOzogjj3tfOoecC8NEhrwifzXcSNVAiHOMnmXojjAPEUORovPg";

fn stubConsumeNonce(_: *anyopaque, _: []const u8) anyerror!bool {
    return false;
}
fn stubLookupWebhookSecret(_: *anyopaque, _: []const u8, _: std.mem.Allocator) anyerror!?[]const u8 {
    return null;
}

const HttpResp = struct {
    status: u16,
    body: []u8,
    fn deinit(self: HttpResp, alloc: std.mem.Allocator) void {
        alloc.free(self.body);
    }
};

// Heap-allocated test server. queue.has_redis tracks whether Redis is connected.
// Self-referential pointers (ctx.oidc, etc.) are valid only after heap placement.
const TestServer = struct {
    pool: *pg.Pool,
    session_store: auth_sessions.SessionStore,
    verifier: oidc.Verifier,
    queue: queue_redis.Client = undefined, // valid only when has_redis=true
    has_redis: bool = false,
    telemetry: telemetry_mod.Telemetry,
    registry: auth_mw.MiddlewareRegistry,
    ctx: handler.Context,
    server: *http_server.Server,
    thread: std.Thread,
    port: u16,

    fn deinit(self: *TestServer) void {
        self.server.stop();
        self.thread.join();
        self.server.deinit();
        self.verifier.deinit();
        self.session_store.deinit();
        if (self.has_redis) self.queue.deinit();
        self.pool.deinit();
    }
};

fn serverThread(srv: *http_server.Server) void {
    srv.listen() catch |e| std.debug.panic("steer test server: {s}", .{@errorName(e)});
}

fn seedTestData(conn: *pg.Conn) !void {
    const now: i64 = std.time.milliTimestamp();
    _ = try conn.exec(
        \\INSERT INTO tenants (tenant_id, name, api_key_hash, created_at, updated_at)
        \\VALUES ($1, 'SteerTest', 'managed', $2, $2)
        \\ON CONFLICT (tenant_id) DO NOTHING
    , .{ TEST_TENANT_ID, now });
    _ = try conn.exec(
        \\INSERT INTO workspaces (workspace_id, tenant_id, repo_url, default_branch, paused, version, created_at, updated_at)
        \\VALUES ($1, $2, 'https://github.com/test/steer', 'main', false, 1, $3, $3)
        \\ON CONFLICT (workspace_id) DO NOTHING
    , .{ TEST_WORKSPACE_ID, TEST_TENANT_ID, now });
    _ = try conn.exec(
        \\INSERT INTO workspaces (workspace_id, tenant_id, repo_url, default_branch, paused, version, created_at, updated_at)
        \\VALUES ($1, $2, 'https://github.com/test/other', 'main', false, 1, $3, $3)
        \\ON CONFLICT (workspace_id) DO NOTHING
    , .{ OTHER_WS_ID, TEST_TENANT_ID, now });
    // Idle zombie in TEST_WORKSPACE_ID (no session row → execution_id is null)
    _ = try conn.exec(
        \\INSERT INTO core.zombies (id, workspace_id, name, source_markdown, config_json, status, created_at, updated_at)
        \\VALUES ($1, $2, 'steer-idle', '---\nname: steer-idle\n---\ntest', '{"name":"steer-idle"}', 'active', 0, 0)
        \\ON CONFLICT DO NOTHING
    , .{ ZOMBIE_IDLE, TEST_WORKSPACE_ID });
    // Active zombie: has a session row with execution_id set
    _ = try conn.exec(
        \\INSERT INTO core.zombies (id, workspace_id, name, source_markdown, config_json, status, created_at, updated_at)
        \\VALUES ($1, $2, 'steer-active', '---\nname: steer-active\n---\ntest', '{"name":"steer-active"}', 'active', 0, 0)
        \\ON CONFLICT DO NOTHING
    , .{ ZOMBIE_ACTIVE, TEST_WORKSPACE_ID });
    _ = try conn.exec(
        \\INSERT INTO core.zombie_sessions (id, zombie_id, context_json, execution_id, execution_started_at, checkpoint_at, created_at, updated_at)
        \\VALUES ($1, $2, '{}', $3, 1000, 0, 0, 0)
        \\ON CONFLICT (zombie_id) DO UPDATE SET execution_id=EXCLUDED.execution_id, execution_started_at=EXCLUDED.execution_started_at
    , .{ SESSION_ACTIVE, ZOMBIE_ACTIVE, ACTIVE_EXEC_ID });
    // Zombie in OTHER_WS_ID — different workspace, used to verify 404
    _ = try conn.exec(
        \\INSERT INTO core.zombies (id, workspace_id, name, source_markdown, config_json, status, created_at, updated_at)
        \\VALUES ($1, $2, 'steer-otherws', '---\nname: steer-otherws\n---\ntest', '{"name":"steer-otherws"}', 'active', 0, 0)
        \\ON CONFLICT DO NOTHING
    , .{ ZOMBIE_OTHER_WS, OTHER_WS_ID });
}

fn cleanupTestData(conn: *pg.Conn) void {
    _ = conn.exec("DELETE FROM core.zombie_sessions WHERE zombie_id IN ($1, $2, $3)", .{ ZOMBIE_IDLE, ZOMBIE_ACTIVE, ZOMBIE_OTHER_WS }) catch {};
    _ = conn.exec("DELETE FROM core.zombies WHERE workspace_id IN ($1, $2)", .{ TEST_WORKSPACE_ID, OTHER_WS_ID }) catch {};
    // Delete OTHER_WS_ID workspace so its FK reference to TEST_TENANT_ID doesn't block
    // rbac/byok test teardown (which does DELETE FROM tenants WHERE tenant_id = TEST_TENANT_ID).
    _ = conn.exec("DELETE FROM workspaces WHERE workspace_id = $1", .{OTHER_WS_ID}) catch {};
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
        .verifier = oidc.Verifier.init(alloc, .{ .provider = .clerk, .jwks_url = TEST_JWKS_URL, .issuer = TEST_ISSUER, .audience = TEST_AUDIENCE, .inline_jwks_json = TEST_JWKS }),
        .registry = undefined,
        .ctx = .{ .pool = db_ctx.pool, .queue = undefined, .alloc = alloc, .api_keys = "", .oidc = undefined, .auth_sessions = undefined, .app_url = "http://127.0.0.1", .api_in_flight_requests = std.atomic.Value(u32).init(0), .api_max_in_flight_requests = 64, .ready_max_queue_depth = null, .ready_max_queue_age_ms = null, .telemetry = undefined },
        .telemetry = undefined,
        .server = undefined,
        .thread = undefined,
        .port = port,
    };
    srv.telemetry = telemetry_mod.Telemetry.initTest();
    srv.ctx.telemetry = &srv.telemetry;
    // Try Redis — success path tests (T1.1, T1.2) will skip if unavailable.
    if (queue_redis.Client.connectFromEnv(alloc, .default)) |client| {
        srv.queue = client;
        srv.has_redis = true;
    } else |_| {}
    srv.ctx.queue = &srv.queue;
    srv.ctx.oidc = &srv.verifier;
    srv.ctx.auth_sessions = &srv.session_store;
    srv.registry = .{ .bearer_or_api_key = .{ .api_keys = "", .verifier = &srv.verifier }, .admin_api_key_mw = .{ .api_keys = "" }, .require_role_admin = .{ .required = .admin }, .require_role_operator = .{ .required = .operator }, .slack_sig = .{ .secret = "" }, .webhook_hmac_mw = .{ .secret = "" }, .oauth_state_mw = .{ .signing_secret = "", .consume_ctx = &srv.queue, .consume_nonce = stubConsumeNonce }, .webhook_url_secret_mw = .{ .lookup_ctx = &srv.queue, .lookup_fn = stubLookupWebhookSecret } };
    srv.registry.initChains();
    srv.server = try http_server.Server.init(&srv.ctx, &srv.registry, .{ .port = port, .threads = 1, .workers = 1, .max_clients = 64 });
    srv.thread = try std.Thread.spawn(.{}, serverThread, .{srv.server});
    errdefer { srv.server.stop(); srv.thread.join(); srv.server.deinit(); }
    const health_url = try std.fmt.allocPrint(alloc, "http://127.0.0.1:{d}/healthz", .{port});
    defer alloc.free(health_url);
    var i: usize = 0;
    while (i < 40) : (i += 1) {
        const r = sendReq(alloc, health_url, .GET, null, null) catch { std.Thread.sleep(25_000_000); continue; };
        defer r.deinit(alloc);
        if (r.status == 200) return srv;
        std.Thread.sleep(25_000_000);
    }
    return error.ConnectionTimedOut;
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
    var w: std.Io.Writer.Allocating = .fromArrayList(alloc, &resp_buf);
    const result = try client.fetch(.{ .location = .{ .url = url }, .method = method, .payload = body, .extra_headers = hdrs[0..hc], .response_writer = &w.writer });
    return .{ .status = @intFromEnum(result.status), .body = try w.toOwnedSlice() };
}

// ── T1.3 / T1.4 / T1.5 / T1.6 — auth + body validation (no Redis needed) ──

test "M23_001 §1: steer endpoint auth and body validation" {
    const srv = try startTestServer(ALLOC);
    defer {
        if (srv.pool.acquire()) |c| { cleanupTestData(c); srv.pool.release(c); } else |_| {}
        srv.deinit();
        ALLOC.destroy(srv);
    }

    // M24_001: /v1/workspaces/{ws}/zombies/{id}/steer
    const steer_idle = try std.fmt.allocPrint(ALLOC, "http://127.0.0.1:{d}/v1/workspaces/{s}/zombies/{s}/steer", .{ srv.port, TEST_WORKSPACE_ID, ZOMBIE_IDLE });
    defer ALLOC.free(steer_idle);
    // steer_other: caller's workspace in URL path, but zombie actually belongs to OTHER_WS — handler returns 404.
    const steer_other = try std.fmt.allocPrint(ALLOC, "http://127.0.0.1:{d}/v1/workspaces/{s}/zombies/{s}/steer", .{ srv.port, TEST_WORKSPACE_ID, ZOMBIE_OTHER_WS });
    defer ALLOC.free(steer_other);
    const body_valid = "{\"message\":\"redirect to phase 2\"}";
    const body_empty = "{\"message\":\"\"}";
    const body_toolong = "{\"message\":\"" ++ "x" ** 8193 ++ "\"}";

    // T1.3: no bearer token → 401
    { const r = try sendReq(ALLOC, steer_idle, .POST, null, body_valid); defer r.deinit(ALLOC); try std.testing.expectEqual(@as(u16, 401), r.status); }
    // T1.4: zombie in different workspace → 404
    { const r = try sendReq(ALLOC, steer_other, .POST, TOKEN_OPERATOR, body_valid); defer r.deinit(ALLOC); try std.testing.expectEqual(@as(u16, 404), r.status); }
    // T1.5: empty message → 400
    { const r = try sendReq(ALLOC, steer_idle, .POST, TOKEN_OPERATOR, body_empty); defer r.deinit(ALLOC); try std.testing.expectEqual(@as(u16, 400), r.status); }
    // T1.6: message > 8192 bytes → 400
    { const r = try sendReq(ALLOC, steer_idle, .POST, TOKEN_OPERATOR, body_toolong); defer r.deinit(ALLOC); try std.testing.expectEqual(@as(u16, 400), r.status); }
}

// ── T1.1 — idle zombie → 200 {message_queued:true, execution_active:false} ──

test "M23_001 §1.1: steer idle zombie returns message_queued=true execution_active=false" {
    const srv = try startTestServer(ALLOC);
    defer {
        if (srv.pool.acquire()) |c| { cleanupTestData(c); srv.pool.release(c); } else |_| {}
        srv.deinit();
        ALLOC.destroy(srv);
    }
    if (!srv.has_redis) return error.SkipZigTest;

    const url = try std.fmt.allocPrint(ALLOC, "http://127.0.0.1:{d}/v1/workspaces/{s}/zombies/{s}/steer", .{ srv.port, TEST_WORKSPACE_ID, ZOMBIE_IDLE });
    defer ALLOC.free(url);

    const r = try sendReq(ALLOC, url, .POST, TOKEN_OPERATOR, "{\"message\":\"proceed to phase 2\"}");
    defer r.deinit(ALLOC);

    try std.testing.expectEqual(@as(u16, 200), r.status);
    try std.testing.expect(std.mem.indexOf(u8, r.body, "\"message_queued\":true") != null);
    try std.testing.expect(std.mem.indexOf(u8, r.body, "\"execution_active\":false") != null);

    // Cleanup: delete the Redis steer key that was written
    _ = srv.queue.getDel("zombie:" ++ ZOMBIE_IDLE ++ ":steer") catch {};
}

// ── T1.2 — active zombie → 200 {execution_active:true, execution_id:non-null} ──

test "M23_001 §1.2: steer active zombie returns execution_active=true and execution_id" {
    const srv = try startTestServer(ALLOC);
    defer {
        if (srv.pool.acquire()) |c| { cleanupTestData(c); srv.pool.release(c); } else |_| {}
        srv.deinit();
        ALLOC.destroy(srv);
    }
    if (!srv.has_redis) return error.SkipZigTest;

    const url = try std.fmt.allocPrint(ALLOC, "http://127.0.0.1:{d}/v1/workspaces/{s}/zombies/{s}/steer", .{ srv.port, TEST_WORKSPACE_ID, ZOMBIE_ACTIVE });
    defer ALLOC.free(url);

    const r = try sendReq(ALLOC, url, .POST, TOKEN_OPERATOR, "{\"message\":\"new objective\"}");
    defer r.deinit(ALLOC);

    try std.testing.expectEqual(@as(u16, 200), r.status);
    try std.testing.expect(std.mem.indexOf(u8, r.body, "\"message_queued\":true") != null);
    try std.testing.expect(std.mem.indexOf(u8, r.body, "\"execution_active\":true") != null);
    try std.testing.expect(std.mem.indexOf(u8, r.body, ACTIVE_EXEC_ID) != null);

    // Cleanup: delete the Redis steer key that was written
    _ = srv.queue.getDel("zombie:" ++ ZOMBIE_ACTIVE ++ ":steer") catch {};
}
