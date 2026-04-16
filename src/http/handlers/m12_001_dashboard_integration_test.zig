// M12_001: HTTP integration tests for the three dashboard endpoints.
//
//   T1  — GET  /v1/workspaces/{ws}/activity → 200 with seeded events
//   T2  — same, no token → 401
//   T3  — same, wrong workspace in path → 403
//   T4  — same, invalid cursor → 400
//   T5  — POST /v1/workspaces/{ws}/zombies/{id}:stop → 200, status=stopped
//   T6  — same, second call → 409 UZ-ZMB-010
//   T7  — same, zombie not in workspace → 404 UZ-ZMB-009
//   T8  — GET  /v1/workspaces/{ws}/zombies/{id}/billing/summary → 200 with data
//   T9  — same, zombie with zero telemetry → 200 (counts zeroed, not 404)
//   T10 — same, zombie not in workspace → 404 UZ-ZMB-009
//   T11 — GET  /v1/workspaces/{ws}/billing/summary after seeding → total_cents
//         reflects the sum of seeded telemetry rows (proves the stub upgrade).
//
// Skips gracefully if TEST_DATABASE_URL / DATABASE_URL not set.

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

fn stubConsumeNonce(_: *anyopaque, _: []const u8) anyerror!bool { return false; }
fn stubLookupWebhookSecret(_: *anyopaque, _: []const u8, _: std.mem.Allocator) anyerror!?[]const u8 { return null; }

const TEST_TENANT_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01";
const TEST_WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11";
const TEST_WORKSPACE_OTHER = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f99";
const TEST_ZOMBIE_ACTIVE = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a7001";
const TEST_ZOMBIE_EMPTY = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a7002";
const TEST_ZOMBIE_NONEXISTENT = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a7999";
const TEST_REPO_URL = "https://github.com/usezombie/m12-http-test";
const TEST_JWKS_URL = "https://clerk.dev.usezombie.com/.well-known/jwks.json";
const TEST_ISSUER = "https://clerk.dev.usezombie.com";
const TEST_AUDIENCE = "https://api.usezombie.com";
const TEST_JWKS =
    \\{"keys":[{"kty":"RSA","n":"2hg972tpbq8H6kzRZ3oVL4wZ9bO-04gJ6gCig68aluyRBzagx-7XXPCiuX80oBHBVj51kvMjT_QDNXfrwzjy4cPbwiVV4HqOGpeIZkPEopfyzs4G7mjiQmx0YuM_5WQUlUjji6Y_DfeaoH-yOhTWBMBVoI0vW_1n66CFaGuEarj3VasdWYxObJTBAM6Jn4XZDcDsBBPNGO4ku7yILkfi11FqXfBP2V8NT0hAGXVAxlWwv-8up1RDzgACp-8JWoC2-kOUJN82fGenDGKq9hW_sumO-4YPNP4U1smnw5jzLlvKa0LBrYG8IgW-3Dniuq2mojhrD_ZQClUd5rF42OyYqw","e":"AQAB","kid":"rbac-test-kid","use":"sig","alg":"RS256"}]}
;
// .user role, workspace = TEST_WORKSPACE_ID; same tokens minted in telemetry_http_integration_test.zig.
const TOKEN_USER =
    "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCIsImtpZCI6InJiYWMtdGVzdC1raWQifQ.eyJzdWIiOiJ1c2VyX3Rlc3QiLCJpc3MiOiJodHRwczovL2NsZXJrLmRldi51c2V6b21iaWUuY29tIiwiYXVkIjoiaHR0cHM6Ly9hcGkudXNlem9tYmllLmNvbSIsImV4cCI6NDEwMjQ0NDgwMCwibWV0YWRhdGEiOnsidGVuYW50X2lkIjoiMDE5NWI0YmEtOGQzYS03ZjEzLThhYmMtMmIzZTFlMGE2ZjAxIiwid29ya3NwYWNlX2lkIjoiMDE5NWI0YmEtOGQzYS03ZjEzLThhYmMtMmIzZTFlMGE2ZjExIiwicm9sZSI6InVzZXIifX0.UEZ3huXtn6bXpa3M1EJZ2QmqLtXewLsHYP5ggTeRg-lgX-Vzp2ECvTsGgzhCSxNNPudRXYgdTsPa1ufIKv_5n1SvuoCRw2eRZfTUp5a_68KbScepnLVx5LaRJmoMyPP8Q_DPYwB0vHm1NCPRIfFqzcBOpLw01Ygkse4mTq19JPE4vcINmaVTWMiN02_ScU0DWhzhzx3_B1_vCBC3wxCpVuM_wqOHDUCnBEPkM-YVQcZrtQIdXPfRzZ2XFRVWFn-E7s0EWBpEP1wSCh31ymki_E1vlnrW4q9ZKNBYnZX0ErvJlcqH2U7nIsFlLYULNP_4mdYrDaWvBSSYZROoK1d8WQ";
// .operator role — required for :stop, billing summary, credentials (RULE BIL + kill-switch guard).
const TOKEN_OPERATOR =
    "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCIsImtpZCI6InJiYWMtdGVzdC1raWQifQ.eyJzdWIiOiJ1c2VyX3Rlc3QiLCJpc3MiOiJodHRwczovL2NsZXJrLmRldi51c2V6b21iaWUuY29tIiwiYXVkIjoiaHR0cHM6Ly9hcGkudXNlem9tYmllLmNvbSIsImV4cCI6NDEwMjQ0NDgwMCwibWV0YWRhdGEiOnsidGVuYW50X2lkIjoiMDE5NWI0YmEtOGQzYS03ZjEzLThhYmMtMmIzZTFlMGE2ZjAxIiwid29ya3NwYWNlX2lkIjoiMDE5NWI0YmEtOGQzYS03ZjEzLThhYmMtMmIzZTFlMGE2ZjExIiwicm9sZSI6Im9wZXJhdG9yIn19.V84uE69RTLrRef0sogegUcUZeKWx8E68GEruFoS8HegUa3o7bVCfQjlkllNSbtUut919EygbQv1C16BMfNTOAv1Lvl3AeLYPYr4ni6EnzzGllbyxDw1aY68AGWEEvKOUxd5wCGl8BnEqaOKX7KNNbAOV4AzJNWqnV-uxJiZl6oDtqi8bsSF1HAm9qY9MAl6AwoZLGnT_x6ux_3vfKy_9ckZSbgjN7laZOMqQ5nwwcaSpwYNm_3ZpXJLgHYMVxel2M4rT0SIaFh__rE42yGE9FBDRUFoyktGOR3NYPOzogjj3tfOoecC8NEhrwifzXcSNVAiHOMnmXojjAPEUORovPg";

const HttpResponse = struct {
    status: u16,
    body: []u8,
    fn deinit(self: HttpResponse, alloc: std.mem.Allocator) void { alloc.free(self.body); }
};

const TestServer = struct {
    pool: *pg.Pool,
    session_store: auth_sessions.SessionStore,
    verifier: oidc.Verifier,
    queue: queue_redis.Client = undefined,
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
        self.pool.deinit();
    }
};

fn serverThread(srv: *http_server.Server) void {
    srv.listen() catch |e| std.debug.panic("m12 test server: {s}", .{@errorName(e)});
}

fn seedTestData(conn: *pg.Conn) !void {
    const now_ms = std.time.milliTimestamp();
    // Clean up anything this test owns from a prior run.
    _ = try conn.exec("DELETE FROM core.activity_events WHERE workspace_id = $1::uuid", .{TEST_WORKSPACE_ID});
    _ = try conn.exec("DELETE FROM zombie_execution_telemetry WHERE workspace_id = $1", .{TEST_WORKSPACE_ID});
    _ = try conn.exec("DELETE FROM core.zombies WHERE workspace_id = $1::uuid", .{TEST_WORKSPACE_ID});

    // Shared fixtures (idempotent) — tenant, workspace, entitlements, billing state.
    _ = try conn.exec(
        \\INSERT INTO tenants (tenant_id, name, api_key_hash, created_at, updated_at)
        \\VALUES ($1, 'M12Test', 'managed', $2, $2)
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
        \\VALUES ('0195b4ba-8d3a-7f13-8abc-2b3e1e0a6ff3', $1, 'FREE', 2, 8,
        \\        false, false, '{"completion":0.4,"error_rate":0.3,"latency":0.2,"resource":0.1}',
        \\        2048, $2, $2)
        \\ON CONFLICT (workspace_id) DO UPDATE SET plan_tier=EXCLUDED.plan_tier, updated_at=EXCLUDED.updated_at
    , .{ TEST_WORKSPACE_ID, now_ms });
    _ = try conn.exec(
        \\INSERT INTO workspace_billing_state
        \\  (billing_id, workspace_id, plan_tier, plan_sku, billing_status, adapter, subscription_id, created_at, updated_at)
        \\VALUES ('0195b4ba-8d3a-7f13-8abc-2b3e1e0a6ff4', $1, 'FREE', 'free', 'ACTIVE', 'noop', 'sub-m12', $2, $2)
        \\ON CONFLICT (workspace_id) DO UPDATE SET plan_tier=EXCLUDED.plan_tier, updated_at=EXCLUDED.updated_at
    , .{ TEST_WORKSPACE_ID, now_ms });

    // Two zombies: one with data, one empty (for zero-state billing test).
    const zombie_specs = [_]struct { id: []const u8, name: []const u8 }{
        .{ .id = TEST_ZOMBIE_ACTIVE, .name = "zombie-m12-active" },
        .{ .id = TEST_ZOMBIE_EMPTY, .name = "zombie-m12-empty" },
    };
    for (zombie_specs) |z| {
        _ = try conn.exec(
            \\INSERT INTO core.zombies
            \\  (id, workspace_id, name, source_markdown, trigger_markdown, config_json,
            \\   status, created_at, updated_at)
            \\VALUES ($1::uuid, $2::uuid, $3, 'seed', null, '{}'::jsonb, 'active', $4, $4)
        , .{ z.id, TEST_WORKSPACE_ID, z.name, now_ms });
    }

    // Activity events (3 on the active zombie) — exercises the pagination feed.
    const activity_ids = [_][]const u8{
        "0195b4ba-8d3a-7f13-8abc-2b3e1e0a7100",
        "0195b4ba-8d3a-7f13-8abc-2b3e1e0a7101",
        "0195b4ba-8d3a-7f13-8abc-2b3e1e0a7102",
    };
    for (activity_ids, 0..) |aid, i| {
        _ = try conn.exec(
            \\INSERT INTO core.activity_events
            \\  (id, zombie_id, workspace_id, event_type, detail, created_at)
            \\VALUES ($1::uuid, $2::uuid, $3::uuid, 'event_received', 'seed', $4)
        , .{ aid, TEST_ZOMBIE_ACTIVE, TEST_WORKSPACE_ID, now_ms - @as(i64, @intCast(i * 1000)) });
    }

    // Telemetry: 2 billable + 1 non-billable on active zombie; empty zombie has none.
    const telemetry_rows = [_]struct { tid: []const u8, eid: []const u8, cents: i64 }{
        .{ .tid = "tel-m12-0", .eid = "ev-m12-0", .cents = 500 },
        .{ .tid = "tel-m12-1", .eid = "ev-m12-1", .cents = 500 },
        .{ .tid = "tel-m12-2", .eid = "ev-m12-2", .cents = 0 },
    };
    for (telemetry_rows) |row| {
        _ = try conn.exec(
            \\INSERT INTO zombie_execution_telemetry
            \\  (id, zombie_id, workspace_id, event_id, token_count, time_to_first_token_ms,
            \\   epoch_wall_time_ms, wall_seconds, plan_tier, credit_deducted_cents, recorded_at)
            \\VALUES ($1, $2, $3, $4, 100, 42, $5, 3, 'free', $6, $5)
        , .{ row.tid, TEST_ZOMBIE_ACTIVE, TEST_WORKSPACE_ID, row.eid, now_ms, row.cents });
    }
}

fn cleanupTestData(conn: *pg.Conn) void {
    _ = conn.exec("DELETE FROM core.activity_events WHERE workspace_id = $1::uuid", .{TEST_WORKSPACE_ID}) catch {};
    _ = conn.exec("DELETE FROM zombie_execution_telemetry WHERE workspace_id = $1", .{TEST_WORKSPACE_ID}) catch {};
    _ = conn.exec("DELETE FROM core.zombies WHERE workspace_id = $1::uuid", .{TEST_WORKSPACE_ID}) catch {};
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
        .registry = undefined,
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
        .server = undefined,
        .thread = undefined,
        .port = port,
    };
    srv.telemetry = telemetry_mod.Telemetry.initTest();
    srv.ctx.telemetry = &srv.telemetry;
    srv.ctx.queue = &srv.queue;
    srv.ctx.oidc = &srv.verifier;
    srv.ctx.auth_sessions = &srv.session_store;
    srv.registry = .{
        .bearer_or_api_key = .{ .api_keys = "", .verifier = &srv.verifier },
        .admin_api_key_mw = .{ .api_keys = "" },
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

    const url = try std.fmt.allocPrint(alloc, "http://127.0.0.1:{d}/healthz", .{port});
    defer alloc.free(url);
    var attempts: usize = 0;
    while (attempts < 40) : (attempts += 1) {
        const r = http(alloc, .GET, url, null, null) catch { std.Thread.sleep(25_000_000); continue; };
        defer r.deinit(alloc);
        if (r.status == 200) return srv;
        std.Thread.sleep(25_000_000);
    }
    return error.ConnectionTimedOut;
}

fn http(
    alloc: std.mem.Allocator,
    method: std.http.Method,
    url: []const u8,
    token: ?[]const u8,
    body: ?[]const u8,
) !HttpResponse {
    var client: std.http.Client = .{ .allocator = alloc };
    defer client.deinit();
    var auth_buf: [1024]u8 = undefined;
    var headers: [1]std.http.Header = undefined;
    var hlen: usize = 0;
    if (token) |t| {
        headers[0] = .{ .name = "authorization", .value = try std.fmt.bufPrint(&auth_buf, "Bearer {s}", .{t}) };
        hlen = 1;
    }
    var buf: std.ArrayList(u8) = .{};
    var w: std.Io.Writer.Allocating = .fromArrayList(alloc, &buf);
    const res = try client.fetch(.{
        .location = .{ .url = url },
        .method = method,
        .extra_headers = headers[0..hlen],
        .payload = body,
        .response_writer = &w.writer,
    });
    return .{ .status = @intFromEnum(res.status), .body = try w.toOwnedSlice() };
}

test "M12_001: GET /workspaces/{ws}/activity — auth, seed, invalid cursor" {
    const alloc = std.testing.allocator;
    const srv = try startTestServer(alloc);
    defer {
        if (srv.pool.acquire()) |c| { cleanupTestData(c); srv.pool.release(c); } else |_| {}
        srv.deinit();
        alloc.destroy(srv);
    }

    const url = try std.fmt.allocPrint(alloc, "http://127.0.0.1:{d}/v1/workspaces/{s}/activity", .{ srv.port, TEST_WORKSPACE_ID });
    defer alloc.free(url);

    // T1: happy path
    { const r = try http(alloc, .GET, url, TOKEN_USER, null); defer r.deinit(alloc); try std.testing.expectEqual(@as(u16, 200), r.status); try std.testing.expect(std.mem.indexOf(u8, r.body, "events") != null); }
    // T2: no token → 401
    { const r = try http(alloc, .GET, url, null, null); defer r.deinit(alloc); try std.testing.expectEqual(@as(u16, 401), r.status); }
    // T3: wrong workspace → 403
    const wrong = try std.fmt.allocPrint(alloc, "http://127.0.0.1:{d}/v1/workspaces/{s}/activity", .{ srv.port, TEST_WORKSPACE_OTHER });
    defer alloc.free(wrong);
    { const r = try http(alloc, .GET, wrong, TOKEN_USER, null); defer r.deinit(alloc); try std.testing.expectEqual(@as(u16, 403), r.status); }
    // T4: invalid cursor → 400
    const bad = try std.fmt.allocPrint(alloc, "{s}?cursor=!!not-base64!!", .{url});
    defer alloc.free(bad);
    { const r = try http(alloc, .GET, bad, TOKEN_USER, null); defer r.deinit(alloc); try std.testing.expectEqual(@as(u16, 400), r.status); }
}

test "M12_001: POST /workspaces/{ws}/zombies/{id}:stop — transitions, 409, 404" {
    const alloc = std.testing.allocator;
    const srv = try startTestServer(alloc);
    defer {
        if (srv.pool.acquire()) |c| { cleanupTestData(c); srv.pool.release(c); } else |_| {}
        srv.deinit();
        alloc.destroy(srv);
    }

    const stop_url = try std.fmt.allocPrint(alloc, "http://127.0.0.1:{d}/v1/workspaces/{s}/zombies/{s}:stop", .{ srv.port, TEST_WORKSPACE_ID, TEST_ZOMBIE_ACTIVE });
    defer alloc.free(stop_url);

    // T5-pre: user role → 403 (kill switch requires operator, greptile PR #221 3095061278).
    { const r = try http(alloc, .POST, stop_url, TOKEN_USER, null); defer r.deinit(alloc); try std.testing.expectEqual(@as(u16, 403), r.status); }
    // T5: operator active → stopped, 200 with status=stopped in body.
    { const r = try http(alloc, .POST, stop_url, TOKEN_OPERATOR, null); defer r.deinit(alloc); try std.testing.expectEqual(@as(u16, 200), r.status); try std.testing.expect(std.mem.indexOf(u8, r.body, "\"status\":\"stopped\"") != null); }
    // T6: re-call → 409 UZ-ZMB-010.
    { const r = try http(alloc, .POST, stop_url, TOKEN_OPERATOR, null); defer r.deinit(alloc); try std.testing.expectEqual(@as(u16, 409), r.status); try std.testing.expect(std.mem.indexOf(u8, r.body, "UZ-ZMB-010") != null); }
    // T7: nonexistent zombie → 404 UZ-ZMB-009.
    const missing = try std.fmt.allocPrint(alloc, "http://127.0.0.1:{d}/v1/workspaces/{s}/zombies/{s}:stop", .{ srv.port, TEST_WORKSPACE_ID, TEST_ZOMBIE_NONEXISTENT });
    defer alloc.free(missing);
    { const r = try http(alloc, .POST, missing, TOKEN_OPERATOR, null); defer r.deinit(alloc); try std.testing.expectEqual(@as(u16, 404), r.status); try std.testing.expect(std.mem.indexOf(u8, r.body, "UZ-ZMB-009") != null); }
}

test "M12_001: GET /workspaces/{ws}/zombies/{id}/billing/summary — populated, zeros, IDOR" {
    const alloc = std.testing.allocator;
    const srv = try startTestServer(alloc);
    defer {
        if (srv.pool.acquire()) |c| { cleanupTestData(c); srv.pool.release(c); } else |_| {}
        srv.deinit();
        alloc.destroy(srv);
    }

    // T8-pre: user role → 403 (RULE BIL).
    const url_active = try std.fmt.allocPrint(alloc, "http://127.0.0.1:{d}/v1/workspaces/{s}/zombies/{s}/billing/summary?period_days=30", .{ srv.port, TEST_WORKSPACE_ID, TEST_ZOMBIE_ACTIVE });
    defer alloc.free(url_active);
    { const r = try http(alloc, .GET, url_active, TOKEN_USER, null); defer r.deinit(alloc); try std.testing.expectEqual(@as(u16, 403), r.status); }
    // T8: operator — zombie with 2 billable + 1 non-billable → total_runs=3, total_cents=1000.
    { const r = try http(alloc, .GET, url_active, TOKEN_OPERATOR, null); defer r.deinit(alloc); try std.testing.expectEqual(@as(u16, 200), r.status); try std.testing.expect(std.mem.indexOf(u8, r.body, "\"total_runs\":3") != null); try std.testing.expect(std.mem.indexOf(u8, r.body, "\"total_cents\":1000") != null); }
    // T9: zombie with no telemetry → 200 with zeros (not 404).
    const url_empty = try std.fmt.allocPrint(alloc, "http://127.0.0.1:{d}/v1/workspaces/{s}/zombies/{s}/billing/summary?period_days=7", .{ srv.port, TEST_WORKSPACE_ID, TEST_ZOMBIE_EMPTY });
    defer alloc.free(url_empty);
    { const r = try http(alloc, .GET, url_empty, TOKEN_OPERATOR, null); defer r.deinit(alloc); try std.testing.expectEqual(@as(u16, 200), r.status); try std.testing.expect(std.mem.indexOf(u8, r.body, "\"total_runs\":0") != null); try std.testing.expect(std.mem.indexOf(u8, r.body, "\"total_cents\":0") != null); }
    // T10: zombie not in workspace → 404 UZ-ZMB-009.
    const url_missing = try std.fmt.allocPrint(alloc, "http://127.0.0.1:{d}/v1/workspaces/{s}/zombies/{s}/billing/summary", .{ srv.port, TEST_WORKSPACE_ID, TEST_ZOMBIE_NONEXISTENT });
    defer alloc.free(url_missing);
    { const r = try http(alloc, .GET, url_missing, TOKEN_OPERATOR, null); defer r.deinit(alloc); try std.testing.expectEqual(@as(u16, 404), r.status); try std.testing.expect(std.mem.indexOf(u8, r.body, "UZ-ZMB-009") != null); }
}

test "M12_001: GET /workspaces/{ws}/billing/summary surfaces real telemetry after stub upgrade" {
    const alloc = std.testing.allocator;
    const srv = try startTestServer(alloc);
    defer {
        if (srv.pool.acquire()) |c| { cleanupTestData(c); srv.pool.release(c); } else |_| {}
        srv.deinit();
        alloc.destroy(srv);
    }

    // T11: workspace summary should now aggregate over the seeded telemetry
    // (3 rows, 1000 cents total). This pins the zero-stub → real-data upgrade.
    // Uses TOKEN_OPERATOR — workspaces_billing_summary.zig has always enforced
    // operator-minimum role.
    const url = try std.fmt.allocPrint(alloc, "http://127.0.0.1:{d}/v1/workspaces/{s}/billing/summary?period_days=30", .{ srv.port, TEST_WORKSPACE_ID });
    defer alloc.free(url);
    const r = try http(alloc, .GET, url, TOKEN_OPERATOR, null);
    defer r.deinit(alloc);
    try std.testing.expectEqual(@as(u16, 200), r.status);
    try std.testing.expect(std.mem.indexOf(u8, r.body, "\"total_runs\":3") != null);
    try std.testing.expect(std.mem.indexOf(u8, r.body, "\"total_cents\":1000") != null);
}
