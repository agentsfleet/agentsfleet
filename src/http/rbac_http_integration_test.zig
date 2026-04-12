const std = @import("std");
const pg = @import("pg");
const auth_sessions = @import("../auth/sessions.zig");
const oidc = @import("../auth/oidc.zig");
const queue_redis = @import("../queue/redis.zig");
const common = @import("handlers/common.zig");
const handler = @import("handler.zig");
const http_server = @import("server.zig");
const error_codes = @import("../errors/error_registry.zig");
const telemetry_mod = @import("../observability/telemetry.zig");

const TEST_TENANT_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01";
const TEST_WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11";
const TEST_SKILL_REF_ENCODED = "clawhub%3A%2F%2Fopenclaw%2Freviewer%401.2.0";
const TEST_SKILL_SECRET_KEY = "API_KEY";
const TEST_SUBSCRIPTION_ID = "sub_rbac_test";
const TEST_REPO_URL = "https://github.com/usezombie/rbac-http-test";
const TEST_JWKS_URL = "https://clerk.dev.usezombie.com/.well-known/jwks.json";
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

var next_test_port = std.atomic.Value(u16).init(38100);

const HttpResponse = struct {
    status: u16,
    body: []u8,

    fn deinit(self: HttpResponse, alloc: std.mem.Allocator) void {
        alloc.free(self.body);
    }
};

const RunningServer = struct {
    pool: *pg.Pool,
    session_store: auth_sessions.SessionStore,
    verifier: oidc.Verifier,
    // Queue intentionally uninitialized — do not add tests that touch ctx.queue
    // without initializing this field first.
    queue: queue_redis.Client = undefined,
    telemetry: telemetry_mod.Telemetry,
    ctx: handler.Context,
    thread: std.Thread,
    port: u16,

    fn deinit(self: *RunningServer) void {
        http_server.stop();
        self.thread.join();
        self.verifier.deinit();
        self.session_store.deinit();
        self.pool.deinit();
    }
};

const ConcurrentRequestCtx = struct {
    url: []const u8,
    token: []const u8,
    status: *u16,

    fn run(self: ConcurrentRequestCtx) void {
        const response = sendRequest(std.heap.page_allocator, self.url, .GET, self.token, null, null) catch {
            self.status.* = 0;
            return;
        };
        defer response.deinit(std.heap.page_allocator);
        self.status.* = response.status;
    }
};

fn allocTestPort() u16 {
    return next_test_port.fetchAdd(1, .acq_rel);
}

fn setupSeedData(conn: *pg.Conn) !void {
    const now_ms = std.time.milliTimestamp();
    _ = try conn.exec("DELETE FROM workspace_billing_audit WHERE workspace_id = $1", .{TEST_WORKSPACE_ID});
    _ = try conn.exec("DELETE FROM workspace_billing_state WHERE workspace_id = $1", .{TEST_WORKSPACE_ID});
    _ = try conn.exec("DELETE FROM workspace_entitlements WHERE workspace_id = $1", .{TEST_WORKSPACE_ID});
    _ = try conn.exec("DELETE FROM vault.workspace_skill_secrets WHERE workspace_id = $1", .{TEST_WORKSPACE_ID});
    _ = try conn.exec("DELETE FROM workspaces WHERE workspace_id = $1", .{TEST_WORKSPACE_ID});
    _ = try conn.exec("DELETE FROM tenants WHERE tenant_id = $1", .{TEST_TENANT_ID});

    _ = try conn.exec(
        \\INSERT INTO tenants (tenant_id, name, api_key_hash, created_at, updated_at)
        \\VALUES ($1, 'RBAC Test Tenant', 'managed', $2, $2)
    , .{ TEST_TENANT_ID, now_ms });
    _ = try conn.exec(
        \\INSERT INTO workspaces
        \\  (workspace_id, tenant_id, repo_url, default_branch, paused, version, created_at, updated_at)
        \\VALUES ($1, $2, $3, 'main', false, 1, $4, $4)
    , .{ TEST_WORKSPACE_ID, TEST_TENANT_ID, TEST_REPO_URL, now_ms });
    _ = try conn.exec(
        \\INSERT INTO workspace_entitlements
        \\  (entitlement_id, workspace_id, plan_tier, max_stages, max_distinct_skills,
        \\   allow_custom_skills, enable_agent_scoring, agent_scoring_weights_json, scoring_context_max_tokens, created_at, updated_at)
        \\VALUES ('0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f71', $1, 'SCALE', 8, 16,
        \\        true, false, '{"completion":0.4,"error_rate":0.3,"latency":0.2,"resource":0.1}', 2048, $2, $2)
        \\ON CONFLICT (workspace_id) DO UPDATE
        \\SET plan_tier = EXCLUDED.plan_tier,
        \\    max_stages = EXCLUDED.max_stages,
        \\    max_distinct_skills = EXCLUDED.max_distinct_skills,
        \\    allow_custom_skills = EXCLUDED.allow_custom_skills,
        \\    enable_agent_scoring = EXCLUDED.enable_agent_scoring,
        \\    agent_scoring_weights_json = EXCLUDED.agent_scoring_weights_json,
        \\    scoring_context_max_tokens = EXCLUDED.scoring_context_max_tokens,
        \\    updated_at = EXCLUDED.updated_at
    , .{ TEST_WORKSPACE_ID, now_ms });
    _ = try conn.exec(
        \\INSERT INTO workspace_billing_state
        \\  (billing_id, workspace_id, plan_tier, plan_sku, billing_status, adapter, subscription_id, created_at, updated_at)
        \\VALUES ('0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f72', $1, 'SCALE', 'scale', 'ACTIVE', 'noop', $2, $3, $3)
        \\ON CONFLICT (workspace_id) DO UPDATE
        \\SET plan_tier = EXCLUDED.plan_tier,
        \\    plan_sku = EXCLUDED.plan_sku,
        \\    billing_status = EXCLUDED.billing_status,
        \\    adapter = EXCLUDED.adapter,
        \\    subscription_id = EXCLUDED.subscription_id,
        \\    updated_at = EXCLUDED.updated_at
    , .{ TEST_WORKSPACE_ID, TEST_SUBSCRIPTION_ID, now_ms });
}

fn cleanupSeedData(conn: *pg.Conn) !void {
    _ = try conn.exec("DELETE FROM workspace_billing_audit WHERE workspace_id = $1", .{TEST_WORKSPACE_ID});
    _ = try conn.exec("DELETE FROM workspace_billing_state WHERE workspace_id = $1", .{TEST_WORKSPACE_ID});
    _ = try conn.exec("DELETE FROM workspace_entitlements WHERE workspace_id = $1", .{TEST_WORKSPACE_ID});
    _ = try conn.exec("DELETE FROM vault.workspace_skill_secrets WHERE workspace_id = $1", .{TEST_WORKSPACE_ID});
    _ = try conn.exec("DELETE FROM workspaces WHERE workspace_id = $1", .{TEST_WORKSPACE_ID});
    _ = try conn.exec("DELETE FROM tenants WHERE tenant_id = $1", .{TEST_TENANT_ID});
}

fn serverThread(ctx: *handler.Context, port: u16) void {
    http_server.serve(ctx, .{
        .port = port,
        .threads = 1,
        .workers = 1,
        .max_clients = 64,
    }) catch |err| {
        std.debug.panic("rbac test server failed: {s}", .{@errorName(err)});
    };
}

fn startServer(alloc: std.mem.Allocator) !*RunningServer {
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
    const port = allocTestPort();

    const running = try alloc.create(RunningServer);
    running.* = RunningServer{
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
    running.telemetry = telemetry_mod.Telemetry.initTest();
    running.ctx.telemetry = &running.telemetry;
    running.ctx.queue = &running.queue;
    running.ctx.oidc = &running.verifier;
    running.ctx.auth_sessions = &running.session_store;
    running.thread = try std.Thread.spawn(.{}, serverThread, .{ &running.ctx, port });
    errdefer {
        http_server.stop();
        running.thread.join();
    }
    try waitForServer(alloc, port);
    return running;
}

fn waitForServer(alloc: std.mem.Allocator, port: u16) !void {
    const url = try std.fmt.allocPrint(alloc, "http://127.0.0.1:{d}/healthz", .{port});
    defer alloc.free(url);

    var attempt: usize = 0;
    while (attempt < 40) : (attempt += 1) {
        const response = sendRequest(alloc, url, .GET, null, null, null) catch {
            std.Thread.sleep(25 * std.time.ns_per_ms);
            continue;
        };
        defer response.deinit(alloc);
        if (response.status == 200) return;
        std.Thread.sleep(25 * std.time.ns_per_ms);
    }
    return error.ConnectionTimedOut;
}

fn sendRequest(
    alloc: std.mem.Allocator,
    url: []const u8,
    method: std.http.Method,
    token: ?[]const u8,
    payload: ?[]const u8,
    content_type: ?[]const u8,
) !HttpResponse {
    var client: std.http.Client = .{ .allocator = alloc };
    defer client.deinit();

    var auth_header: ?[]u8 = null;
    defer if (auth_header) |value| alloc.free(value);

    var headers_buf: [2]std.http.Header = undefined;
    var header_count: usize = 0;
    if (token) |value| {
        auth_header = try std.fmt.allocPrint(alloc, "Bearer {s}", .{value});
        headers_buf[header_count] = .{ .name = "authorization", .value = auth_header.? };
        header_count += 1;
    }
    if (content_type) |value| {
        headers_buf[header_count] = .{ .name = "content-type", .value = value };
        header_count += 1;
    }

    var response_body: std.ArrayList(u8) = .{};
    var writer: std.Io.Writer.Allocating = .fromArrayList(alloc, &response_body);
    const result = try client.fetch(.{
        .location = .{ .url = url },
        .method = method,
        .payload = payload,
        .extra_headers = headers_buf[0..header_count],
        .response_writer = &writer.writer,
    });
    return .{
        .status = @intFromEnum(result.status),
        .body = try writer.toOwnedSlice(),
    };
}

test "integration: RBAC endpoints enforce operator and admin roles over live HTTP" {
    const server = try startServer(std.testing.allocator);
    defer {
        server.deinit();
        std.testing.allocator.destroy(server);
    }

    const harness_url = try std.fmt.allocPrint(std.testing.allocator, "http://127.0.0.1:{d}/v1/workspaces/{s}/harness/active", .{
        server.port,
        TEST_WORKSPACE_ID,
    });
    defer std.testing.allocator.free(harness_url);
    const invalid_harness_url = try std.fmt.allocPrint(std.testing.allocator, "http://127.0.0.1:{d}/v1/workspaces/not-a-uuid/harness/active", .{server.port});
    defer std.testing.allocator.free(invalid_harness_url);
    const skill_secret_url = try std.fmt.allocPrint(std.testing.allocator, "http://127.0.0.1:{d}/v1/workspaces/{s}/skills/{s}/secrets/{s}", .{
        server.port,
        TEST_WORKSPACE_ID,
        TEST_SKILL_REF_ENCODED,
        TEST_SKILL_SECRET_KEY,
    });
    defer std.testing.allocator.free(skill_secret_url);
    const billing_event_url = try std.fmt.allocPrint(std.testing.allocator, "http://127.0.0.1:{d}/v1/workspaces/{s}/billing/events", .{
        server.port,
        TEST_WORKSPACE_ID,
    });
    defer std.testing.allocator.free(billing_event_url);

    {
        const response = try sendRequest(std.testing.allocator, harness_url, .GET, null, null, null);
        defer response.deinit(std.testing.allocator);
        try std.testing.expectEqual(@as(u16, 401), response.status);
        try std.testing.expect(std.mem.indexOf(u8, response.body, error_codes.ERR_UNAUTHORIZED) != null);
    }
    {
        const response = try sendRequest(std.testing.allocator, invalid_harness_url, .GET, TEST_OPERATOR_TOKEN, null, null);
        defer response.deinit(std.testing.allocator);
        try std.testing.expectEqual(@as(u16, 400), response.status);
        try std.testing.expect(std.mem.indexOf(u8, response.body, error_codes.ERR_UUIDV7_INVALID_ID_SHAPE) != null);
    }
    {
        const response = try sendRequest(std.testing.allocator, harness_url, .GET, TEST_USER_TOKEN, null, null);
        defer response.deinit(std.testing.allocator);
        try std.testing.expectEqual(@as(u16, 403), response.status);
        try std.testing.expect(std.mem.indexOf(u8, response.body, error_codes.ERR_INSUFFICIENT_ROLE) != null);
    }
    {
        const response = try sendRequest(std.testing.allocator, skill_secret_url, .DELETE, TEST_USER_TOKEN, null, null);
        defer response.deinit(std.testing.allocator);
        try std.testing.expectEqual(@as(u16, 403), response.status);
        try std.testing.expect(std.mem.indexOf(u8, response.body, error_codes.ERR_INSUFFICIENT_ROLE) != null);
    }
    {
        const response = try sendRequest(
            std.testing.allocator,
            billing_event_url,
            .POST,
            TEST_USER_TOKEN,
            "{\"event_type\":\"PAYMENT_FAILED\",\"reason\":\"rbac-test\"}",
            "application/json",
        );
        defer response.deinit(std.testing.allocator);
        try std.testing.expectEqual(@as(u16, 403), response.status);
        try std.testing.expect(std.mem.indexOf(u8, response.body, error_codes.ERR_INSUFFICIENT_ROLE) != null);
    }
    {
        // Operator token must be rejected for admin-only billing-events endpoint.
        const response = try sendRequest(
            std.testing.allocator,
            billing_event_url,
            .POST,
            TEST_OPERATOR_TOKEN,
            "{\"event_type\":\"PAYMENT_FAILED\",\"reason\":\"rbac-test\"}",
            "application/json",
        );
        defer response.deinit(std.testing.allocator);
        try std.testing.expectEqual(@as(u16, 403), response.status);
        try std.testing.expect(std.mem.indexOf(u8, response.body, error_codes.ERR_INSUFFICIENT_ROLE) != null);
    }
    {
        const response = try sendRequest(std.testing.allocator, harness_url, .GET, TEST_OPERATOR_TOKEN, null, null);
        defer response.deinit(std.testing.allocator);
        try std.testing.expectEqual(@as(u16, 200), response.status);
        try std.testing.expect(std.mem.indexOf(u8, response.body, "\"source\":\"default-v1\"") != null);
    }
    {
        const response = try sendRequest(std.testing.allocator, skill_secret_url, .DELETE, TEST_OPERATOR_TOKEN, null, null);
        defer response.deinit(std.testing.allocator);
        try std.testing.expectEqual(@as(u16, 200), response.status);
        try std.testing.expect(std.mem.indexOf(u8, response.body, "\"deleted\":true") != null);
    }
    {
        const response = try sendRequest(
            std.testing.allocator,
            billing_event_url,
            .POST,
            TEST_ADMIN_TOKEN,
            "{\"event_type\":\"PAYMENT_FAILED\",\"reason\":\"rbac-test\"}",
            "application/json",
        );
        defer response.deinit(std.testing.allocator);
        try std.testing.expectEqual(@as(u16, 200), response.status);
        try std.testing.expect(std.mem.indexOf(u8, response.body, "\"billing_status\":\"GRACE\"") != null);
    }

    const cleanup_conn = try server.pool.acquire();
    defer server.pool.release(cleanup_conn);
    try cleanupSeedData(cleanup_conn);
}

test "integration: RBAC user-role rejection stays deterministic under concurrency" {
    const server = try startServer(std.testing.allocator);
    defer {
        server.deinit();
        std.testing.allocator.destroy(server);
    }

    const harness_url = try std.fmt.allocPrint(std.testing.allocator, "http://127.0.0.1:{d}/v1/workspaces/{s}/harness/active", .{
        server.port,
        TEST_WORKSPACE_ID,
    });
    defer std.testing.allocator.free(harness_url);

    var statuses = [_]u16{0} ** 5;
    var threads: [5]std.Thread = undefined;
    for (&threads, 0..) |*thread, idx| {
        thread.* = try std.Thread.spawn(.{}, ConcurrentRequestCtx.run, .{ConcurrentRequestCtx{
            .url = harness_url,
            .token = TEST_USER_TOKEN,
            .status = &statuses[idx],
        }});
    }
    for (&threads) |*thread| thread.join();
    for (statuses) |status| try std.testing.expectEqual(@as(u16, 403), status);

    const cleanup_conn = try server.pool.acquire();
    defer server.pool.release(cleanup_conn);
    try cleanupSeedData(cleanup_conn);
}
