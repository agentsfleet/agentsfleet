// Cross-workspace IDOR integration tests — M24_001.
//
// Every workspace-scoped handler must reject requests whose path workspace_id
// or zombie_id points to a different tenant's data. `authorizeWorkspace` guards
// the principal→workspace edge; `common.getZombieWorkspaceId` guards the
// workspace→zombie edge. These tests exercise both edges via HTTP.
//
// Coverage matrix (steer endpoint is covered by T1.4 in
// zombie_steer_http_integration_test.zig; not duplicated here):
//
//   | Endpoint                                                | Expected |
//   |---------------------------------------------------------|----------|
//   | GET    /v1/workspaces/{foreign_ws}/zombies              | 403      |
//   | DELETE /v1/workspaces/{my_ws}/zombies/{foreign_zombie}  | 404      |
//   | GET    /v1/workspaces/{my_ws}/zombies/{foreign}/activity| 404      |
//   | GET    /v1/workspaces/{foreign_ws}/credentials          | 403      |
//   | GET    /v1/workspaces/{my_ws}/zombies/{foreign}/ig      | 404      |
//   | DELETE /v1/workspaces/{my_ws}/zombies/{foreign}/ig/{g}  | 404      |
//
// The JWT used is the operator token from zombie_steer_http_integration_test.zig
// — workspace_scope_id = TEST_WORKSPACE_ID. Requests hitting paths under a
// different workspace_id fail at `authorizeWorkspace`; requests hitting
// zombies in a foreign workspace fail at `getZombieWorkspaceId`.
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

const ALLOC = std.testing.allocator;

// IDs — same tenant + workspace as the steer integration test (required by the
// signed JWT), but a UNIQUE OTHER_WS_ID and zombie set to avoid collisions when
// both test files run in the same DB.
const TEST_TENANT_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01";
const TEST_WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11";
const OTHER_WS_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0bbbf0"; // unique for this file
const ZOMBIE_IN_FOREIGN_WS = "0195b4ba-8d3a-7f13-8abc-2b3e1e0bbb01";
const GRANT_ID_PLACEHOLDER = "0195b4ba-8d3a-7f13-8abc-2b3e1e0bbb99";

// Same JWKS + token as the steer test — DO NOT regenerate independently.
const TEST_JWKS_URL = "https://clerk.dev.usezombie.com/.well-known/jwks.json";
const TEST_ISSUER = "https://clerk.dev.usezombie.com";
const TEST_AUDIENCE = "https://api.usezombie.com";
const TEST_JWKS =
    \\{"keys":[{"kty":"RSA","n":"2hg972tpbq8H6kzRZ3oVL4wZ9bO-04gJ6gCig68aluyRBzagx-7XXPCiuX80oBHBVj51kvMjT_QDNXfrwzjy4cPbwiVV4HqOGpeIZkPEopfyzs4G7mjiQmx0YuM_5WQUlUjji6Y_DfeaoH-yOhTWBMBVoI0vW_1n66CFaGuEarj3VasdWYxObJTBAM6Jn4XZDcDsBBPNGO4ku7yILkfi11FqXfBP2V8NT0hAGXVAxlWwv-8up1RDzgACp-8JWoC2-kOUJN82fGenDGKq9hW_sumO-4YPNP4U1smnw5jzLlvKa0LBrYG8IgW-3Dniuq2mojhrD_ZQClUd5rF42OyYqw","e":"AQAB","kid":"rbac-test-kid","use":"sig","alg":"RS256"}]}
;
// .operator role, workspace_id = TEST_WORKSPACE_ID, tenant_id = TEST_TENANT_ID, exp = year 2100
const TOKEN_OPERATOR =
    "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCIsImtpZCI6InJiYWMtdGVzdC1raWQifQ.eyJzdWIiOiJ1c2VyX3Rlc3QiLCJpc3MiOiJodHRwczovL2NsZXJrLmRldi51c2V6b21iaWUuY29tIiwiYXVkIjoiaHR0cHM6Ly9hcGkudXNlem9tYmllLmNvbSIsImV4cCI6NDEwMjQ0NDgwMCwibWV0YWRhdGEiOnsidGVuYW50X2lkIjoiMDE5NWI0YmEtOGQzYS03ZjEzLThhYmMtMmIzZTFlMGE2ZjAxIiwid29ya3NwYWNlX2lkIjoiMDE5NWI0YmEtOGQzYS03ZjEzLThhYmMtMmIzZTFlMGE2ZjExIiwicm9sZSI6Im9wZXJhdG9yIn19.V84uE69RTLrRef0sogegUcUZeKWx8E68GEruFoS8HegUa3o7bVCfQjlkllNSbtUut919EygbQv1C16BMfNTOAv1Lvl3AeLYPYr4ni6EnzzGllbyxDw1aY68AGWEEvKOUxd5wCGl8BnEqaOKX7KNNbAOV4AzJNWqnV-uxJiZl6oDtqi8bsSF1HAm9qY9MAl6AwoZLGnT_x6ux_3vfKy_9ckZSbgjN7laZOMqQ5nwwcaSpwYNm_3ZpXJLgHYMVxel2M4rT0SIaFh__rE42yGE9FBDRUFoyktGOR3NYPOzogjj3tfOoecC8NEhrwifzXcSNVAiHOMnmXojjAPEUORovPg";

fn stubLookupWebhookSecret(_: *anyopaque, _: []const u8, _: std.mem.Allocator) anyerror!?[]const u8 {
    return null;
}
fn stubTenantApiKeyLookup(_: *anyopaque, _: std.mem.Allocator, _: []const u8) anyerror!?auth_mw.tenant_api_key.LookupResult {
    return null;
}

const HttpResp = struct {
    status: u16,
    body: []u8,
    fn deinit(self: HttpResp, alloc: std.mem.Allocator) void {
        alloc.free(self.body);
    }
};

const TestServer = struct {
    pool: *pg.Pool,
    session_store: auth_sessions.SessionStore,
    verifier: oidc.Verifier,
    queue: queue_redis.Client = undefined,
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
    srv.listen() catch |e| std.debug.panic("idor test server: {s}", .{@errorName(e)});
}

fn seedTestData(conn: *pg.Conn) !void {
    const now: i64 = std.time.milliTimestamp();
    _ = try conn.exec(
        \\INSERT INTO tenants (tenant_id, name, created_at, updated_at)
        \\VALUES ($1, 'IdorTest', $2, $2)
        \\ON CONFLICT (tenant_id) DO NOTHING
    , .{ TEST_TENANT_ID, now });
    _ = try conn.exec(
        \\INSERT INTO workspaces (workspace_id, tenant_id, repo_url, default_branch, paused, version, created_at, updated_at)
        \\VALUES ($1, $2, 'https://github.com/test/idor', 'main', false, 1, $3, $3)
        \\ON CONFLICT (workspace_id) DO NOTHING
    , .{ TEST_WORKSPACE_ID, TEST_TENANT_ID, now });
    _ = try conn.exec(
        \\INSERT INTO workspaces (workspace_id, tenant_id, repo_url, default_branch, paused, version, created_at, updated_at)
        \\VALUES ($1, $2, 'https://github.com/test/idor-foreign', 'main', false, 1, $3, $3)
        \\ON CONFLICT (workspace_id) DO NOTHING
    , .{ OTHER_WS_ID, TEST_TENANT_ID, now });
    // Zombie owned by the FOREIGN workspace. Used to probe IDOR on routes that
    // take (workspace_id, zombie_id) in the path: caller sends TEST_WORKSPACE_ID
    // in the path but this zombie actually belongs to OTHER_WS_ID.
    _ = try conn.exec(
        \\INSERT INTO core.zombies (id, workspace_id, name, source_markdown, config_json, status, created_at, updated_at)
        \\VALUES ($1, $2, 'idor-foreign', '---\nname: idor-foreign\n---\nx', '{"name":"idor-foreign"}', 'active', 0, 0)
        \\ON CONFLICT DO NOTHING
    , .{ ZOMBIE_IN_FOREIGN_WS, OTHER_WS_ID });
}

fn cleanupTestData(conn: *pg.Conn) void {
    _ = conn.exec("DELETE FROM core.integration_grants WHERE zombie_id = $1::uuid", .{ZOMBIE_IN_FOREIGN_WS}) catch {};
    _ = conn.exec("DELETE FROM core.zombies WHERE id = $1::uuid", .{ZOMBIE_IN_FOREIGN_WS}) catch {};
    // Delete OTHER_WS_ID only — TEST_WORKSPACE_ID is shared with other test files.
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
        .ctx = .{ .pool = db_ctx.pool, .queue = undefined, .alloc = alloc, .oidc = undefined, .auth_sessions = undefined, .app_url = "http://127.0.0.1", .api_in_flight_requests = std.atomic.Value(u32).init(0), .api_max_in_flight_requests = 64, .ready_max_queue_depth = null, .ready_max_queue_age_ms = null, .telemetry = undefined },
        .telemetry = undefined,
        .server = undefined,
        .thread = undefined,
        .port = port,
    };
    srv.telemetry = telemetry_mod.Telemetry.initTest();
    srv.ctx.telemetry = &srv.telemetry;
    if (queue_redis.Client.connectFromEnv(alloc, .default)) |client| {
        srv.queue = client;
        srv.has_redis = true;
    } else |_| {}
    srv.ctx.queue = &srv.queue;
    srv.ctx.oidc = &srv.verifier;
    srv.ctx.auth_sessions = &srv.session_store;
    srv.registry = .{
        .bearer_or_api_key = .{ .verifier = &srv.verifier },
        .tenant_api_key_mw = .{ .host = undefined, .lookup = stubTenantApiKeyLookup },
        .require_role_admin = .{ .required = .admin },
        .require_role_operator = .{ .required = .operator },
        .webhook_hmac_mw = .{ .secret = "" },
        .webhook_url_secret_mw = .{ .lookup_ctx = &srv.queue, .lookup_fn = stubLookupWebhookSecret },
    };
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

fn urlJoin(alloc: std.mem.Allocator, port: u16, comptime path_fmt: []const u8, args: anytype) ![]u8 {
    var parts: std.ArrayList(u8) = .{};
    defer parts.deinit(alloc);
    try parts.writer(alloc).print("http://127.0.0.1:{d}", .{port});
    try parts.writer(alloc).print(path_fmt, args);
    return try parts.toOwnedSlice(alloc);
}

// ── IDOR Tests ────────────────────────────────────────────────────────────

test "M24_001 IDOR: GET /workspaces/{foreign}/zombies returns 403" {
    const srv = try startTestServer(ALLOC);
    defer {
        if (srv.pool.acquire()) |c| { cleanupTestData(c); srv.pool.release(c); } else |_| {}
        srv.deinit();
        ALLOC.destroy(srv);
    }

    // Principal is scoped to TEST_WORKSPACE_ID; requesting OTHER_WS_ID must 403.
    const url = try urlJoin(ALLOC, srv.port, "/v1/workspaces/{s}/zombies", .{OTHER_WS_ID});
    defer ALLOC.free(url);

    const r = try sendReq(ALLOC, url, .GET, TOKEN_OPERATOR, null);
    defer r.deinit(ALLOC);
    try std.testing.expectEqual(@as(u16, 403), r.status);
}

test "IDOR: PATCH /workspaces/{my}/zombies/{foreign} status=killed returns 404" {
    const srv = try startTestServer(ALLOC);
    defer {
        if (srv.pool.acquire()) |c| { cleanupTestData(c); srv.pool.release(c); } else |_| {}
        srv.deinit();
        ALLOC.destroy(srv);
    }

    // Caller's ws in path, foreign zombie in path. Must 404 — the patch
    // handler scopes the UPDATE by both ids and returns 404 when no row
    // matches. The kill flow now rides on PATCH .../zombies/{id} with
    // body {status:"killed"} (folded from the retired POST /kill).
    const url = try urlJoin(ALLOC, srv.port, "/v1/workspaces/{s}/zombies/{s}", .{ TEST_WORKSPACE_ID, ZOMBIE_IN_FOREIGN_WS });
    defer ALLOC.free(url);

    const r = try sendReq(ALLOC, url, .PATCH, TOKEN_OPERATOR, "{\"status\":\"killed\"}");
    defer r.deinit(ALLOC);
    try std.testing.expectEqual(@as(u16, 404), r.status);
}

test "M24_001 IDOR: GET /workspaces/{my}/zombies/{foreign}/activity returns 404" {
    const srv = try startTestServer(ALLOC);
    defer {
        if (srv.pool.acquire()) |c| { cleanupTestData(c); srv.pool.release(c); } else |_| {}
        srv.deinit();
        ALLOC.destroy(srv);
    }

    // Caller's ws in path, foreign zombie in path. Must 404 — greptile P1 regression guard.
    const url = try urlJoin(ALLOC, srv.port, "/v1/workspaces/{s}/zombies/{s}/activity", .{ TEST_WORKSPACE_ID, ZOMBIE_IN_FOREIGN_WS });
    defer ALLOC.free(url);

    const r = try sendReq(ALLOC, url, .GET, TOKEN_OPERATOR, null);
    defer r.deinit(ALLOC);
    try std.testing.expectEqual(@as(u16, 404), r.status);
}

test "M24_001 IDOR: GET /workspaces/{foreign}/credentials returns 403" {
    const srv = try startTestServer(ALLOC);
    defer {
        if (srv.pool.acquire()) |c| { cleanupTestData(c); srv.pool.release(c); } else |_| {}
        srv.deinit();
        ALLOC.destroy(srv);
    }

    const url = try urlJoin(ALLOC, srv.port, "/v1/workspaces/{s}/credentials", .{OTHER_WS_ID});
    defer ALLOC.free(url);

    const r = try sendReq(ALLOC, url, .GET, TOKEN_OPERATOR, null);
    defer r.deinit(ALLOC);
    try std.testing.expectEqual(@as(u16, 403), r.status);
}

test "M24_001 IDOR: GET /workspaces/{my}/zombies/{foreign}/integration-grants returns 404" {
    const srv = try startTestServer(ALLOC);
    defer {
        if (srv.pool.acquire()) |c| { cleanupTestData(c); srv.pool.release(c); } else |_| {}
        srv.deinit();
        ALLOC.destroy(srv);
    }

    const url = try urlJoin(ALLOC, srv.port, "/v1/workspaces/{s}/zombies/{s}/integration-grants", .{ TEST_WORKSPACE_ID, ZOMBIE_IN_FOREIGN_WS });
    defer ALLOC.free(url);

    const r = try sendReq(ALLOC, url, .GET, TOKEN_OPERATOR, null);
    defer r.deinit(ALLOC);
    try std.testing.expectEqual(@as(u16, 404), r.status);
}

test "M24_001 IDOR: DELETE /workspaces/{my}/zombies/{foreign}/integration-grants/{g} returns 404" {
    const srv = try startTestServer(ALLOC);
    defer {
        if (srv.pool.acquire()) |c| { cleanupTestData(c); srv.pool.release(c); } else |_| {}
        srv.deinit();
        ALLOC.destroy(srv);
    }

    const url = try urlJoin(ALLOC, srv.port, "/v1/workspaces/{s}/zombies/{s}/integration-grants/{s}", .{ TEST_WORKSPACE_ID, ZOMBIE_IN_FOREIGN_WS, GRANT_ID_PLACEHOLDER });
    defer ALLOC.free(url);

    const r = try sendReq(ALLOC, url, .DELETE, TOKEN_OPERATOR, null);
    defer r.deinit(ALLOC);
    try std.testing.expectEqual(@as(u16, 404), r.status);
}

// ── getZombieWorkspaceId orelse branch — nonexistent zombie ────────────────

test "M24_001 IDOR: GET activity for nonexistent zombie returns 404 (getZombieWorkspaceId orelse branch)" {
    const srv = try startTestServer(ALLOC);
    defer {
        if (srv.pool.acquire()) |c| { cleanupTestData(c); srv.pool.release(c); } else |_| {}
        srv.deinit();
        ALLOC.destroy(srv);
    }

    // UUIDv7 shape but nothing in core.zombies matches — exercises the `orelse`
    // branch in common.getZombieWorkspaceId rather than the !eql path.
    const nonexistent_zombie = "0195b4ba-8d3a-7f13-8abc-2b3e1edead01";
    const url = try urlJoin(ALLOC, srv.port, "/v1/workspaces/{s}/zombies/{s}/activity", .{ TEST_WORKSPACE_ID, nonexistent_zombie });
    defer ALLOC.free(url);

    const r = try sendReq(ALLOC, url, .GET, TOKEN_OPERATOR, null);
    defer r.deinit(ALLOC);
    try std.testing.expectEqual(@as(u16, 404), r.status);
}

// ─────────────────────────────────────────────────────────────────────────────
// M26_001: REST conventions — envelope shape, method enforcement, 204 body.
// Added to this file because it shares TestServer + operator JWT + cleanup.
// ─────────────────────────────────────────────────────────────────────────────

// Tiny JSON probe that asserts a top-level string key exists in a JSON object
// body, without pulling in a full parser. Good enough for the 1-level envelope
// keys we assert here (`items`, `total`, `zombies`, `agents`, etc.).
// OOM is a hard failure in tests — never silently "prove" absence of a key
// because the probe failed to allocate (a `false` return would make a negated
// assertion `expect(!bodyHasTopLevelKey(...))` wrongly pass).
fn bodyHasTopLevelKey(body: []const u8, key: []const u8) bool {
    // Matches `"key":` with optional whitespace. Not hardened against quoted-in-
    // string pathologies; sufficient for server-generated response shapes.
    const alloc = std.testing.allocator;
    const needle = std.fmt.allocPrint(alloc, "\"{s}\":", .{key}) catch
        @panic("bodyHasTopLevelKey: OOM allocating needle — cannot infer presence safely");
    defer alloc.free(needle);
    return std.mem.indexOf(u8, body, needle) != null;
}

test "M26_001 envelope: GET /workspaces/{my}/zombies body has items+total, no zombies key" {
    const srv = try startTestServer(ALLOC);
    defer {
        if (srv.pool.acquire()) |c| { cleanupTestData(c); srv.pool.release(c); } else |_| {}
        srv.deinit();
        ALLOC.destroy(srv);
    }

    const url = try urlJoin(ALLOC, srv.port, "/v1/workspaces/{s}/zombies", .{TEST_WORKSPACE_ID});
    defer ALLOC.free(url);

    const r = try sendReq(ALLOC, url, .GET, TOKEN_OPERATOR, null);
    defer r.deinit(ALLOC);
    try std.testing.expectEqual(@as(u16, 200), r.status);
    try std.testing.expect(bodyHasTopLevelKey(r.body, "items"));
    try std.testing.expect(bodyHasTopLevelKey(r.body, "total"));
    // Old collection-keyed envelope must be gone.
    try std.testing.expect(!bodyHasTopLevelKey(r.body, "zombies"));
}

test "M26_001 envelope: GET /workspaces/{my}/agent-keys body has items+total, no agents key" {
    const srv = try startTestServer(ALLOC);
    defer {
        if (srv.pool.acquire()) |c| { cleanupTestData(c); srv.pool.release(c); } else |_| {}
        srv.deinit();
        ALLOC.destroy(srv);
    }

    const url = try urlJoin(ALLOC, srv.port, "/v1/workspaces/{s}/agent-keys", .{TEST_WORKSPACE_ID});
    defer ALLOC.free(url);

    const r = try sendReq(ALLOC, url, .GET, TOKEN_OPERATOR, null);
    defer r.deinit(ALLOC);
    try std.testing.expectEqual(@as(u16, 200), r.status);
    try std.testing.expect(bodyHasTopLevelKey(r.body, "items"));
    try std.testing.expect(bodyHasTopLevelKey(r.body, "total"));
    try std.testing.expect(!bodyHasTopLevelKey(r.body, "agents"));
}

test "memories: GET with malformed zombie_id in path returns 400" {
    const srv = try startTestServer(ALLOC);
    defer {
        if (srv.pool.acquire()) |c| { cleanupTestData(c); srv.pool.release(c); } else |_| {}
        srv.deinit();
        ALLOC.destroy(srv);
    }

    // Path-segment zombie_id fails UUIDv7 format check in handler — 400 from
    // resolveZombieInWorkspace before any DB access.
    const url = try urlJoin(ALLOC, srv.port, "/v1/workspaces/{s}/zombies/not-a-uuid/memories", .{TEST_WORKSPACE_ID});
    defer ALLOC.free(url);

    const r = try sendReq(ALLOC, url, .GET, TOKEN_OPERATOR, null);
    defer r.deinit(ALLOC);
    try std.testing.expectEqual(@as(u16, 400), r.status);
}

test "M26_001 no-content: DELETE agent-key returns 204 with empty body" {
    const srv = try startTestServer(ALLOC);
    defer {
        if (srv.pool.acquire()) |c| { cleanupTestData(c); srv.pool.release(c); } else |_| {}
        srv.deinit();
        ALLOC.destroy(srv);
    }

    // Seed an agent key in TEST_WORKSPACE_ID for this test only. A zombie
    // record is also required because agent_keys.zombie_id has a FK.
    const agent_id = "agent_m26_204_test";
    const zombie_for_agent = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f99";
    const conn = try srv.pool.acquire();
    _ = try conn.exec(
        \\INSERT INTO core.zombies (id, workspace_id, name, source_markdown, config_json, status, created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, 'm26-204-test', '---\nname: m26-204\n---\nx', '{"name":"m26-204"}', 'active', 0, 0)
        \\ON CONFLICT DO NOTHING
    , .{ zombie_for_agent, TEST_WORKSPACE_ID });
    _ = try conn.exec(
        \\INSERT INTO core.agent_keys (agent_id, workspace_id, zombie_id, name, description, key_hash, created_at)
        \\VALUES ($1, $2::uuid, $3::uuid, 'm26-204-test', '', 'stub-hash', 0)
        \\ON CONFLICT (agent_id) DO NOTHING
    , .{ agent_id, TEST_WORKSPACE_ID, zombie_for_agent });
    srv.pool.release(conn);
    defer {
        if (srv.pool.acquire()) |c| {
            _ = c.exec("DELETE FROM core.agent_keys WHERE agent_id = $1", .{agent_id}) catch {};
            _ = c.exec("DELETE FROM core.zombies WHERE id = $1::uuid", .{zombie_for_agent}) catch {};
            srv.pool.release(c);
        } else |_| {}
    }

    const url = try urlJoin(ALLOC, srv.port, "/v1/workspaces/{s}/agent-keys/{s}", .{ TEST_WORKSPACE_ID, agent_id });
    defer ALLOC.free(url);

    const r = try sendReq(ALLOC, url, .DELETE, TOKEN_OPERATOR, null);
    defer r.deinit(ALLOC);
    try std.testing.expectEqual(@as(u16, 204), r.status);
    // RFC 9110 §6.4.5: 204 responses MUST NOT include a message body.
    try std.testing.expectEqual(@as(usize, 0), r.body.len);
}


test "M26_001 no-content: DELETE integration-grant returns 204 with empty body" {
    const srv = try startTestServer(ALLOC);
    defer {
        if (srv.pool.acquire()) |c| { cleanupTestData(c); srv.pool.release(c); } else |_| {}
        srv.deinit();
        ALLOC.destroy(srv);
    }

    // Seed zombie + pending grant. Revoke path requires the grant.status != 'revoked'.
    const zombie_for_grant = "0195b4ba-8d3a-7f13-8abc-2b3e1ecafe02";
    const grant_id = "grant_m26_204";
    const conn = try srv.pool.acquire();
    _ = try conn.exec(
        \\INSERT INTO core.zombies (id, workspace_id, name, source_markdown, config_json, status, created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, 'm26-grant-test', '---\nname: m26-grant\n---\nx', '{"name":"m26-grant"}', 'active', 0, 0)
        \\ON CONFLICT DO NOTHING
    , .{ zombie_for_grant, TEST_WORKSPACE_ID });
    _ = try conn.exec(
        \\INSERT INTO core.integration_grants
        \\  (grant_id, zombie_id, service, status, requested_at, requested_reason)
        \\VALUES ($1, $2::uuid, 'slack', 'pending', 0, 'm26 test')
        \\ON CONFLICT (grant_id) DO NOTHING
    , .{ grant_id, zombie_for_grant });
    srv.pool.release(conn);
    defer {
        if (srv.pool.acquire()) |c| {
            _ = c.exec("DELETE FROM core.integration_grants WHERE grant_id = $1", .{grant_id}) catch {};
            _ = c.exec("DELETE FROM core.zombies WHERE id = $1::uuid", .{zombie_for_grant}) catch {};
            srv.pool.release(c);
        } else |_| {}
    }

    const url = try urlJoin(ALLOC, srv.port, "/v1/workspaces/{s}/zombies/{s}/integration-grants/{s}", .{ TEST_WORKSPACE_ID, zombie_for_grant, grant_id });
    defer ALLOC.free(url);

    const r = try sendReq(ALLOC, url, .DELETE, TOKEN_OPERATOR, null);
    defer r.deinit(ALLOC);
    try std.testing.expectEqual(@as(u16, 204), r.status);
    // RFC 9110 §6.4.5: 204 MUST NOT include a message body.
    try std.testing.expectEqual(@as(usize, 0), r.body.len);
}

test "memories: GET with limit=0 returns 400" {
    const srv = try startTestServer(ALLOC);
    defer {
        if (srv.pool.acquire()) |c| { cleanupTestData(c); srv.pool.release(c); } else |_| {}
        srv.deinit();
        ALLOC.destroy(srv);
    }

    // parseLimitQs returns OutOfRange → 400 before any DB access.
    const valid_zid = "0195b4ba-8d3a-7f13-8abc-2b3e1e0cafe2";
    const url = try urlJoin(ALLOC, srv.port, "/v1/workspaces/{s}/zombies/{s}/memories?limit=0", .{ TEST_WORKSPACE_ID, valid_zid });
    defer ALLOC.free(url);

    const r = try sendReq(ALLOC, url, .GET, TOKEN_OPERATOR, null);
    defer r.deinit(ALLOC);
    try std.testing.expectEqual(@as(u16, 400), r.status);
}

test "memories: GET with non-numeric limit returns 400" {
    const srv = try startTestServer(ALLOC);
    defer {
        if (srv.pool.acquire()) |c| { cleanupTestData(c); srv.pool.release(c); } else |_| {}
        srv.deinit();
        ALLOC.destroy(srv);
    }

    const valid_zid = "0195b4ba-8d3a-7f13-8abc-2b3e1e0cafe3";
    const url = try urlJoin(ALLOC, srv.port, "/v1/workspaces/{s}/zombies/{s}/memories?query=x&limit=abc", .{ TEST_WORKSPACE_ID, valid_zid });
    defer ALLOC.free(url);

    const r = try sendReq(ALLOC, url, .GET, TOKEN_OPERATOR, null);
    defer r.deinit(ALLOC);
    try std.testing.expectEqual(@as(u16, 400), r.status);
}
