// Integration tests for /v1/api-keys (M28_002 §3, §4).
//
// Covers:
//   - POST creates: 201, zmb_t_ prefix, SHA-256 hex persisted in core.api_keys.
//   - Duplicate key_name within a tenant: 409 UZ-APIKEY-005.
//   - Round-trip auth: a minted zmb_t_ key authenticates a subsequent GET.
//   - PATCH {active:false} revokes; the same key can no longer authenticate.
//   - DELETE on an active key is 409; DELETE on a revoked key is 204.
//   - Tenant isolation: GET as tenant A does not return tenant B's rows.
//
// The operator JWT and JWKS mirror cross_workspace_idor_test.zig so any future
// key rotation in that test updates both at once — do NOT regenerate here.
// Skips if TEST_DATABASE_URL / DATABASE_URL is not set.

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
const api_key_lookup = @import("../../cmd/api_key_lookup.zig");

const ALLOC = std.testing.allocator;

const TEST_TENANT_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01";
const TEST_WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11";
const OTHER_TENANT_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0ccc01";
const FOREIGN_KEY_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0ccc02";

const TEST_JWKS_URL = "https://clerk.dev.usezombie.com/.well-known/jwks.json";
const TEST_ISSUER = "https://clerk.dev.usezombie.com";
const TEST_AUDIENCE = "https://api.usezombie.com";
const TEST_JWKS =
    \\{"keys":[{"kty":"RSA","n":"2hg972tpbq8H6kzRZ3oVL4wZ9bO-04gJ6gCig68aluyRBzagx-7XXPCiuX80oBHBVj51kvMjT_QDNXfrwzjy4cPbwiVV4HqOGpeIZkPEopfyzs4G7mjiQmx0YuM_5WQUlUjji6Y_DfeaoH-yOhTWBMBVoI0vW_1n66CFaGuEarj3VasdWYxObJTBAM6Jn4XZDcDsBBPNGO4ku7yILkfi11FqXfBP2V8NT0hAGXVAxlWwv-8up1RDzgACp-8JWoC2-kOUJN82fGenDGKq9hW_sumO-4YPNP4U1smnw5jzLlvKa0LBrYG8IgW-3Dniuq2mojhrD_ZQClUd5rF42OyYqw","e":"AQAB","kid":"rbac-test-kid","use":"sig","alg":"RS256"}]}
;
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

const TestServer = struct {
    pool: *pg.Pool,
    session_store: auth_sessions.SessionStore,
    verifier: oidc.Verifier,
    queue: queue_redis.Client = undefined,
    has_redis: bool = false,
    telemetry: telemetry_mod.Telemetry,
    api_key_ctx: api_key_lookup.Ctx,
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
    srv.listen() catch |e| std.debug.panic("api_keys test server: {s}", .{@errorName(e)});
}

fn seedTestData(conn: *pg.Conn) !void {
    const now: i64 = std.time.milliTimestamp();
    _ = try conn.exec(
        \\INSERT INTO tenants (tenant_id, name, api_key_hash, created_at, updated_at)
        \\VALUES ($1, 'ApiKeysTest', 'managed', $2, $2)
        \\ON CONFLICT (tenant_id) DO NOTHING
    , .{ TEST_TENANT_ID, now });
    _ = try conn.exec(
        \\INSERT INTO tenants (tenant_id, name, api_key_hash, created_at, updated_at)
        \\VALUES ($1, 'ApiKeysOtherTest', 'managed', $2, $2)
        \\ON CONFLICT (tenant_id) DO NOTHING
    , .{ OTHER_TENANT_ID, now });
    _ = try conn.exec(
        \\INSERT INTO workspaces (workspace_id, tenant_id, repo_url, default_branch, paused, version, created_at, updated_at)
        \\VALUES ($1, $2, 'https://github.com/test/api-keys', 'main', false, 1, $3, $3)
        \\ON CONFLICT (workspace_id) DO NOTHING
    , .{ TEST_WORKSPACE_ID, TEST_TENANT_ID, now });
}

fn cleanupApiKeys(conn: *pg.Conn) void {
    _ = conn.exec("DELETE FROM core.api_keys WHERE tenant_id IN ($1::uuid, $2::uuid)", .{ TEST_TENANT_ID, OTHER_TENANT_ID }) catch {};
    // Intentionally do NOT delete OTHER_TENANT_ID here — seedTestData runs
    // before cleanupApiKeys in startTestServer; removing the tenant row
    // would leave subsequent INSERTs into core.api_keys (FK → core.tenants)
    // failing with foreign_key_violation.
}

fn startTestServer(alloc: std.mem.Allocator) !*TestServer {
    const db_ctx = (try common.openHandlerTestConn(alloc)) orelse return error.SkipZigTest;
    try seedTestData(db_ctx.conn);
    cleanupApiKeys(db_ctx.conn); // start clean
    db_ctx.pool.release(db_ctx.conn);
    const port = try test_port.allocFreePort();
    const srv = try alloc.create(TestServer);
    srv.* = TestServer{
        .pool = db_ctx.pool,
        .session_store = auth_sessions.SessionStore.init(alloc),
        .verifier = oidc.Verifier.init(alloc, .{ .provider = .clerk, .jwks_url = TEST_JWKS_URL, .issuer = TEST_ISSUER, .audience = TEST_AUDIENCE, .inline_jwks_json = TEST_JWKS }),
        .api_key_ctx = .{ .pool = db_ctx.pool },
        .registry = undefined,
        .ctx = .{ .pool = db_ctx.pool, .queue = undefined, .alloc = alloc, .api_keys = "", .oidc = undefined, .auth_sessions = undefined, .app_url = "http://127.0.0.1", .api_in_flight_requests = std.atomic.Value(u32).init(0), .api_max_in_flight_requests = 64, .ready_max_queue_depth = null, .ready_max_queue_age_ms = null, .telemetry = undefined },
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
        .bearer_or_api_key = .{ .api_keys = "", .verifier = &srv.verifier },
        .admin_api_key_mw = .{ .api_keys = "" },
        // Real DB-backed lookup — required so minted zmb_t_ keys actually authenticate.
        .tenant_api_key_mw = .{ .host = &srv.api_key_ctx, .lookup = api_key_lookup.lookup },
        .require_role_admin = .{ .required = .admin },
        .require_role_operator = .{ .required = .operator },
        .slack_sig = .{ .secret = "" },
        .webhook_hmac_mw = .{ .secret = "" },
        .oauth_state_mw = .{ .signing_secret = "", .consume_ctx = &srv.queue, .consume_nonce = stubConsumeNonce },
        .webhook_url_secret_mw = .{ .lookup_ctx = &srv.queue, .lookup_fn = stubLookupWebhookSecret },
    };
    srv.registry.initChains();
    srv.server = try http_server.Server.init(&srv.ctx, &srv.registry, .{ .port = port, .threads = 1, .workers = 1, .max_clients = 64 });
    srv.thread = try std.Thread.spawn(.{}, serverThread, .{srv.server});
    errdefer {
        srv.server.stop();
        srv.thread.join();
        srv.server.deinit();
    }
    const health_url = try std.fmt.allocPrint(alloc, "http://127.0.0.1:{d}/healthz", .{port});
    defer alloc.free(health_url);
    var i: usize = 0;
    while (i < 40) : (i += 1) {
        const r = sendReq(alloc, health_url, .GET, null, null) catch {
            std.Thread.sleep(25_000_000);
            continue;
        };
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

fn urlFor(alloc: std.mem.Allocator, port: u16, comptime fmt: []const u8, args: anytype) ![]u8 {
    var parts: std.ArrayList(u8) = .{};
    defer parts.deinit(alloc);
    try parts.writer(alloc).print("http://127.0.0.1:{d}", .{port});
    try parts.writer(alloc).print(fmt, args);
    return try parts.toOwnedSlice(alloc);
}

fn parseJsonString(alloc: std.mem.Allocator, body: []const u8, field: []const u8) !?[]const u8 {
    const parsed = try std.json.parseFromSlice(std.json.Value, alloc, body, .{});
    defer parsed.deinit();
    const obj = parsed.value.object.get(field) orelse return null;
    if (obj != .string) return null;
    return try alloc.dupe(u8, obj.string);
}

fn expectStatus(resp: HttpResp, want: u16) !void {
    if (resp.status != want) {
        std.debug.print("expected status {d}, got {d}; body={s}\n", .{ want, resp.status, resp.body });
        return error.TestExpectedEqual;
    }
}

fn deinitSrv(srv: *TestServer) void {
    if (srv.pool.acquire()) |c| {
        cleanupApiKeys(c);
        srv.pool.release(c);
    } else |_| {}
    srv.deinit();
    ALLOC.destroy(srv);
}

// ── Tests ─────────────────────────────────────────────────────────────────

test "POST /v1/api-keys returns 201 with zmb_t_ key and persists SHA-256 hash" {
    const srv = try startTestServer(ALLOC);
    defer deinitSrv(srv);

    const u = try urlFor(ALLOC, srv.port, "/v1/api-keys", .{});
    defer ALLOC.free(u);
    const resp = try sendReq(ALLOC, u, .POST, TOKEN_OPERATOR,
        \\{"key_name":"ci-pipeline","description":"GH Actions"}
    );
    defer resp.deinit(ALLOC);
    try expectStatus(resp, 201);

    const raw_key = (try parseJsonString(ALLOC, resp.body, "key")) orelse return error.TestExpectedEqual;
    defer ALLOC.free(raw_key);
    try std.testing.expect(std.mem.startsWith(u8, raw_key, "zmb_t_"));
    try std.testing.expectEqual(@as(usize, 70), raw_key.len);

    // Verify persisted hash matches SHA-256(raw_key).
    var digest: [32]u8 = undefined;
    std.crypto.hash.sha2.Sha256.hash(raw_key, &digest, .{});
    const expected_hex = std.fmt.bytesToHex(digest, .lower);

    const conn = try srv.pool.acquire();
    defer srv.pool.release(conn);
    var q = @import("../../db/pg_query.zig").PgQuery.from(try conn.query(
        \\SELECT key_hash FROM core.api_keys WHERE tenant_id = $1::uuid AND key_name = $2
    , .{ TEST_TENANT_ID, "ci-pipeline" }));
    defer q.deinit();
    const row = (try q.next()) orelse return error.TestExpectedEqual;
    const stored_hash = try row.get([]u8, 0);
    try std.testing.expectEqualStrings(expected_hex[0..], stored_hash);
}

test "POST /v1/api-keys with duplicate key_name within a tenant returns 409 UZ-APIKEY-005" {
    const srv = try startTestServer(ALLOC);
    defer deinitSrv(srv);

    const u = try urlFor(ALLOC, srv.port, "/v1/api-keys", .{});
    defer ALLOC.free(u);
    const first = try sendReq(ALLOC, u, .POST, TOKEN_OPERATOR,
        \\{"key_name":"duplicate-name"}
    );
    defer first.deinit(ALLOC);
    try expectStatus(first, 201);

    const second = try sendReq(ALLOC, u, .POST, TOKEN_OPERATOR,
        \\{"key_name":"duplicate-name"}
    );
    defer second.deinit(ALLOC);
    try expectStatus(second, 409);
    try std.testing.expect(std.mem.indexOf(u8, second.body, "UZ-APIKEY-005") != null);
}

test "A minted zmb_t_ key authenticates GET /v1/api-keys and is revoked by PATCH {active:false}" {
    const srv = try startTestServer(ALLOC);
    defer deinitSrv(srv);

    const create_url = try urlFor(ALLOC, srv.port, "/v1/api-keys", .{});
    defer ALLOC.free(create_url);
    const create_resp = try sendReq(ALLOC, create_url, .POST, TOKEN_OPERATOR,
        \\{"key_name":"round-trip"}
    );
    defer create_resp.deinit(ALLOC);
    try expectStatus(create_resp, 201);
    const raw_key = (try parseJsonString(ALLOC, create_resp.body, "key")) orelse return error.TestExpectedEqual;
    defer ALLOC.free(raw_key);
    const id = (try parseJsonString(ALLOC, create_resp.body, "id")) orelse return error.TestExpectedEqual;
    defer ALLOC.free(id);

    // Authenticate GET using the minted raw key.
    const list_url = try urlFor(ALLOC, srv.port, "/v1/api-keys", .{});
    defer ALLOC.free(list_url);
    const list_before = try sendReq(ALLOC, list_url, .GET, raw_key, null);
    defer list_before.deinit(ALLOC);
    try expectStatus(list_before, 200);
    try std.testing.expect(std.mem.indexOf(u8, list_before.body, "key_hash") == null);

    // Revoke via PATCH.
    const patch_url = try urlFor(ALLOC, srv.port, "/v1/api-keys/{s}", .{id});
    defer ALLOC.free(patch_url);
    const patch_resp = try sendReq(ALLOC, patch_url, .PATCH, TOKEN_OPERATOR,
        \\{"active":false}
    );
    defer patch_resp.deinit(ALLOC);
    try expectStatus(patch_resp, 200);

    // The revoked key no longer authenticates.
    const list_after = try sendReq(ALLOC, list_url, .GET, raw_key, null);
    defer list_after.deinit(ALLOC);
    try expectStatus(list_after, 401);
    try std.testing.expect(std.mem.indexOf(u8, list_after.body, "UZ-APIKEY-004") != null);
}

test "DELETE rejects an active key with 409 and succeeds on a revoked key with 204" {
    const srv = try startTestServer(ALLOC);
    defer deinitSrv(srv);

    const create_url = try urlFor(ALLOC, srv.port, "/v1/api-keys", .{});
    defer ALLOC.free(create_url);
    const create_resp = try sendReq(ALLOC, create_url, .POST, TOKEN_OPERATOR,
        \\{"key_name":"delete-flow"}
    );
    defer create_resp.deinit(ALLOC);
    try expectStatus(create_resp, 201);
    const id = (try parseJsonString(ALLOC, create_resp.body, "id")) orelse return error.TestExpectedEqual;
    defer ALLOC.free(id);

    const del_url = try urlFor(ALLOC, srv.port, "/v1/api-keys/{s}", .{id});
    defer ALLOC.free(del_url);
    const del_active = try sendReq(ALLOC, del_url, .DELETE, TOKEN_OPERATOR, null);
    defer del_active.deinit(ALLOC);
    try expectStatus(del_active, 409);

    // Revoke, then delete.
    const patch_resp = try sendReq(ALLOC, del_url, .PATCH, TOKEN_OPERATOR,
        \\{"active":false}
    );
    defer patch_resp.deinit(ALLOC);
    try expectStatus(patch_resp, 200);

    const del_revoked = try sendReq(ALLOC, del_url, .DELETE, TOKEN_OPERATOR, null);
    defer del_revoked.deinit(ALLOC);
    try expectStatus(del_revoked, 204);
}

test "GET /v1/api-keys returns only the calling tenant's rows (cross-tenant isolation)" {
    const srv = try startTestServer(ALLOC);
    defer deinitSrv(srv);

    // Seed a key directly into OTHER_TENANT_ID (no JWT exists for that tenant).
    {
        const conn = try srv.pool.acquire();
        defer srv.pool.release(conn);
        _ = try conn.exec(
            \\INSERT INTO core.api_keys (id, tenant_id, key_name, key_hash, created_by, active)
            \\VALUES ($1::uuid, $2::uuid, 'other-tenant-key', 'deadbeef' , 'user_other', TRUE)
        , .{ FOREIGN_KEY_ID, OTHER_TENANT_ID });
    }

    // Operator for TEST_TENANT_ID mints one key of their own.
    const create_url = try urlFor(ALLOC, srv.port, "/v1/api-keys", .{});
    defer ALLOC.free(create_url);
    const create_resp = try sendReq(ALLOC, create_url, .POST, TOKEN_OPERATOR,
        \\{"key_name":"own-tenant-key"}
    );
    defer create_resp.deinit(ALLOC);
    try expectStatus(create_resp, 201);

    // Listing as TEST_TENANT_ID must NOT reveal OTHER_TENANT_ID's row.
    const list_resp = try sendReq(ALLOC, create_url, .GET, TOKEN_OPERATOR, null);
    defer list_resp.deinit(ALLOC);
    try expectStatus(list_resp, 200);
    try std.testing.expect(std.mem.indexOf(u8, list_resp.body, "own-tenant-key") != null);
    try std.testing.expect(std.mem.indexOf(u8, list_resp.body, "other-tenant-key") == null);
    try std.testing.expect(std.mem.indexOf(u8, list_resp.body, FOREIGN_KEY_ID) == null);
}
