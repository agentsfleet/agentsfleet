// Cross-workspace IDOR integration tests.
//
// Every workspace-scoped handler must reject requests whose path workspace_id
// or fleet_id points to a different tenant's data. `authorizeWorkspace` guards
// the principal→workspace edge; `common.getFleetWorkspaceId` guards the
// workspace→fleet edge. These tests exercise both edges via HTTP.
//
// Coverage matrix (steer endpoint is covered separately in
// fleet_steer_http_integration_test.zig; not duplicated here):
//
//   | Endpoint                                                | Expected |
//   |---------------------------------------------------------|----------|
//   | GET    /v1/workspaces/{foreign_ws}/fleets              | 403      |
//   | DELETE /v1/workspaces/{my_ws}/fleets/{foreign_agent}  | 404      |
//   | GET    /v1/workspaces/{my_ws}/fleets/{foreign}/activity| 404      |
//   | GET    /v1/workspaces/{foreign_ws}/secrets              | 403      |
//   | GET    /v1/workspaces/{my_ws}/fleets/{foreign}/ig      | 404      |
//   | DELETE /v1/workspaces/{my_ws}/fleets/{foreign}/ig/{g}  | 404      |
//
// The JWT used is the operator token from fleet_steer_http_integration_test.zig
// — workspace_scope_id = TEST_WORKSPACE_ID. Requests hitting paths under a
// different workspace_id fail at `authorizeWorkspace`; requests hitting
// fleets in a foreign workspace fail at `getFleetWorkspaceId`.
//
// Skips all tests if TEST_DATABASE_URL / DATABASE_URL is not set.

const std = @import("std");
const scope_fixtures = @import("../test_scope_tokens.zig");
const clock = @import("common").clock;
const pg = @import("pg");
const logging = @import("log");
const session_store_redis = @import("../../session/session_store_redis.zig");
const audit_events = @import("../../auth/audit_events.zig");
const call_deadline = @import("call_deadline");
const oidc = @import("../../auth/oidc.zig");
const queue_redis = @import("../../queue/redis.zig");
const auth_mw = @import("../../auth/middleware/mod.zig");
const common = @import("common.zig");
const handler = @import("../../http/handler.zig");
const http_server = @import("../../http/server.zig");
const subscription_hub = @import("../../events/subscription_hub.zig");
const stream_registry = @import("../../http/stream_registry.zig");
const telemetry_mod = @import("../../observability/telemetry.zig");
const test_port = @import("../test_port.zig");

const TEST_AUDIT_PEPPER: []const u8 = "test-pepper-bytes-32-len--padded";

// This suite asserts workspace-scoping on database reads and makes no outbound
// call, so the scheduler is deliberately never started: arming would fail
// closed, which is the correct answer if a handler ever started dialling here.
var idor_backend: call_deadline.MonotonicBackend = .{};
var idor_scheduler = call_deadline.ProcessScheduler.init(std.heap.page_allocator, &idor_backend);
const TEST_SESSION_PEPPER: []const u8 = "test-pepper-bytes-32-len--padded";

const ALLOC = std.testing.allocator;

// IDs — same tenant + workspace as the steer integration test (required by the
// signed JWT), but a UNIQUE OTHER_WS_ID and fleet set to avoid collisions when
// both test files run in the same DB.
const TEST_TENANT_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01";
const TEST_WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11";
const OTHER_WS_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0bbbf0"; // unique for this file
const AGENTSFLEET_IN_FOREIGN_WS = "0195b4ba-8d3a-7f13-8abc-2b3e1e0bbb01";
const GRANT_ID_PLACEHOLDER = "0195b4ba-8d3a-7f13-8abc-2b3e1e0bbb99";

// Same JWKS + token as the steer test — DO NOT regenerate independently.
const TEST_JWKS_URL = "https://clerk.test.agentsfleet.net/.well-known/jwks.json";
const TEST_ISSUER = scope_fixtures.ISSUER;
const TEST_AUDIENCE = scope_fixtures.AUDIENCE;
const TEST_JWKS = scope_fixtures.JWKS;
// .operator role, workspace_id = TEST_WORKSPACE_ID, tenant_id = TEST_TENANT_ID, exp = year 2100
const TOKEN_OPERATOR = scope_fixtures.TENANT_ADMIN;

fn stubTenantApiKeyLookup(_: *anyopaque, _: std.mem.Allocator, _: []const u8) anyerror!?auth_mw.tenant_api_key.LookupResult {
    return null;
}

fn stubRunnerLookup(_: *anyopaque, _: std.mem.Allocator, _: []const u8) anyerror!?auth_mw.runner_bearer.LookupResult {
    return null;
}

const HttpResp = struct {
    status: u16,
    body: []u8,
    fn deinit(self: HttpResp, alloc: std.mem.Allocator) void {
        alloc.free(self.body);
    }
};

// Listen-thread status. Set by serverThread on listen() error so the main
// thread can distinguish a recoverable port race from a real failure.
//   0 = running (or not started)
//   1 = listen failed with AddressInUse — retry with a new port
//   2 = listen failed with some other error — surface to the caller
const LISTEN_OK: u8 = 0;
const LISTEN_ADDRESS_IN_USE: u8 = 1;
const LISTEN_OTHER_ERR: u8 = 2;

const TestServer = struct {
    pool: *pg.Pool,
    /// Cold shared-SSE hub (never started): IDOR tests exercise no SSE
    /// route, but Context.hub must point at a valid hub.
    hub: subscription_hub,
    streams: stream_registry,
    session_store: session_store_redis.SessionStore,
    verifier: oidc.Verifier,
    // SAFETY: test fixture; field is populated by the surrounding builder before any read.
    queue: queue_redis.Client = undefined,
    has_redis: bool = false,
    telemetry: telemetry_mod.Telemetry,
    registry: auth_mw.MiddlewareRegistry,
    ctx: handler.Context,
    server: *http_server.Server,
    thread: std.Thread,
    port: u16,
    listen_status: std.atomic.Value(u8) = std.atomic.Value(u8).init(LISTEN_OK),

    fn deinit(self: *TestServer) void {
        self.server.stop();
        self.thread.join();
        self.server.deinit();
        self.hub.stop();
        self.hub.deinit();
        self.streams.deinit();
        self.verifier.deinit();
        // Redis-backed SessionStore is a pure facade — no per-instance teardown.
        if (self.has_redis) self.queue.deinit();
        self.pool.deinit();
    }
};

fn serverThread(srv: *TestServer) void {
    const log = logging.scoped(.idor_test);
    srv.server.listen() catch |e| {
        const code: u8 = if (e == error.AddressInUse) LISTEN_ADDRESS_IN_USE else LISTEN_OTHER_ERR;
        srv.listen_status.store(code, .seq_cst);
        // audit-error-codes: intentional-fake
        log.warn("listen_failed", .{ .error_code = "UZ-TEST-001", .err = @errorName(e) });
    };
}

fn seedTestData(conn: *pg.Conn) !void {
    const now: i64 = clock.nowMillis();
    _ = try conn.exec(
        \\INSERT INTO tenants (tenant_id, name, created_at, updated_at)
        \\VALUES ($1, 'IdorTest', $2, $2)
        \\ON CONFLICT (tenant_id) DO NOTHING
    , .{ TEST_TENANT_ID, now });
    _ = try conn.exec(
        \\INSERT INTO workspaces (workspace_id, tenant_id, created_at)
        \\VALUES ($1, $2, $3)
        \\ON CONFLICT (workspace_id) DO NOTHING
    , .{ TEST_WORKSPACE_ID, TEST_TENANT_ID, now });
    _ = try conn.exec(
        \\INSERT INTO workspaces (workspace_id, tenant_id, created_at)
        \\VALUES ($1, $2, $3)
        \\ON CONFLICT (workspace_id) DO NOTHING
    , .{ OTHER_WS_ID, TEST_TENANT_ID, now });
    // Fleet owned by the FOREIGN workspace. Used to probe IDOR on routes that
    // take (workspace_id, fleet_id) in the path: caller sends TEST_WORKSPACE_ID
    // in the path but this fleet actually belongs to OTHER_WS_ID.
    _ = try conn.exec(
        \\INSERT INTO core.fleets (id, workspace_id, name, source_markdown, config_json, status, created_at, updated_at)
        \\VALUES ($1, $2, 'idor-foreign', '---\nname: idor-foreign\n---\nx', '{"name":"idor-foreign"}', 'active', 0, 0)
        \\ON CONFLICT DO NOTHING
    , .{ AGENTSFLEET_IN_FOREIGN_WS, OTHER_WS_ID });
}

fn cleanupTestData(conn: *pg.Conn) void {
    _ = conn.exec("DELETE FROM core.integration_grants WHERE fleet_id = $1::uuid", .{AGENTSFLEET_IN_FOREIGN_WS}) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
    _ = conn.exec("DELETE FROM core.fleets WHERE id = $1::uuid", .{AGENTSFLEET_IN_FOREIGN_WS}) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
    // Delete OTHER_WS_ID only — TEST_WORKSPACE_ID is shared with other test files.
    _ = conn.exec("DELETE FROM workspaces WHERE workspace_id = $1", .{OTHER_WS_ID}) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
}

// Bind/spawn/healthz attempts before we give up. The TOCTOU window between
// `test_port.allocFreePort` (which closes the probe socket) and httpz's bind
// can lose the port to another process. SO_REUSEADDR is set on both ends but
// doesn't help when a different socket is already actively bound. We just
// pick a fresh port and try again.
const PORT_RETRY_ATTEMPTS: usize = 5;

fn bindAndWaitReady(alloc: std.mem.Allocator, srv: *TestServer) !void {
    var attempt: usize = 0;
    while (attempt < PORT_RETRY_ATTEMPTS) : (attempt += 1) {
        srv.port = try test_port.allocFreePort();
        srv.listen_status = std.atomic.Value(u8).init(LISTEN_OK);
        srv.server = try http_server.Server.init(srv.ctx.io, &srv.ctx, &srv.registry, .{ .port = srv.port, .threads = 1, .workers = 1, .max_clients = 64 });
        srv.thread = try std.Thread.spawn(.{}, serverThread, .{srv});

        const health_url = try std.fmt.allocPrint(alloc, "http://127.0.0.1:{d}/healthz", .{srv.port});
        defer alloc.free(health_url);
        var i: usize = 0;
        while (i < 40) : (i += 1) {
            switch (srv.listen_status.load(.seq_cst)) {
                LISTEN_ADDRESS_IN_USE => break, // retry with a fresh port
                LISTEN_OTHER_ERR => return error.ListenFailed,
                else => {},
            }
            const r = sendReq(alloc, health_url, .GET, null, null) catch {
                @import("common").sleepNanos(25_000_000);
                continue;
            };
            defer r.deinit(alloc);
            if (r.status == 200) return;
            @import("common").sleepNanos(25_000_000);
        }

        // Either the port was lost to a race, or the server never came up.
        // Tear down this attempt's server+thread cleanly. listen() returns once
        // stop() is invoked, so the join completes regardless of which path
        // fired (AddressInUse early-exit, real OtherErr, or healthz timeout).
        srv.server.stop();
        srv.thread.join();
        srv.server.deinit();

        if (srv.listen_status.load(.seq_cst) != LISTEN_ADDRESS_IN_USE) {
            // Healthz never returned 200 but listen didn't fail with
            // AddressInUse — surface the timeout, don't loop on a real bug.
            return error.ConnectionTimedOut;
        }
        // AddressInUse — fall through and retry with a new port.
    }
    return error.PortRetriesExhausted;
}

fn startTestServer(alloc: std.mem.Allocator) !*TestServer {
    const db_ctx = (try common.openHandlerTestConn(alloc)) orelse return error.SkipZigTest;
    try seedTestData(db_ctx.conn);
    db_ctx.pool.release(db_ctx.conn);
    const srv = try alloc.create(TestServer);
    srv.* = TestServer{
        .pool = db_ctx.pool,
        .hub = subscription_hub.init(alloc, @import("common").globalIo()),
        .streams = stream_registry.init(alloc, @import("common").globalIo()),
        // SAFETY: session_store is populated in-place after the queue connects below.
        // IDOR tests never hit /v1/auth/* so leaving it undefined when Redis
        // is absent does not crash anything; the field is only read by the
        // auth-session handlers.
        .session_store = undefined,
        .verifier = oidc.Verifier.init(alloc, .{ .provider = .clerk, .jwks_url = TEST_JWKS_URL, .issuer = TEST_ISSUER, .audience = TEST_AUDIENCE, .inline_jwks_json = TEST_JWKS }),
        // SAFETY: test fixture; field is populated by the surrounding builder before any read.
        .registry = undefined,
        // SAFETY: test fixture; field is populated by the surrounding builder before any read.
        // SAFETY: test fixture; field is populated by the surrounding builder before any read.
        // SAFETY: test fixture; field is populated by the surrounding builder before any read.
        // SAFETY: test fixture; field is populated by the surrounding builder before any read.
        .ctx = .{ .pool = db_ctx.pool, .queue = undefined, .alloc = alloc, .io = @import("common").globalIo(), .deadline_scheduler = &idor_scheduler, .clerk_webhook_secret = null, .approval_signing_secret = null, .clerk_secret_key = null, .oidc = undefined, .auth_sessions = undefined, .audit_ctx = audit_events.AuditCtx.init(TEST_AUDIT_PEPPER), .app_url = "http://127.0.0.1", .api_url = "http://127.0.0.1", .api_in_flight_requests = std.atomic.Value(u32).init(0), .api_max_in_flight_requests = 64, .hub = undefined, .stream_registry = undefined, .fleet_sets = undefined, .ready_max_queue_depth = null, .ready_max_queue_age_ms = null, .telemetry = undefined },
        // SAFETY: test fixture; field is populated by the surrounding builder before any read.
        .telemetry = undefined,
        // SAFETY: test fixture; field is populated by the surrounding builder before any read.
        .server = undefined,
        // SAFETY: test fixture; field is populated by the surrounding builder before any read.
        .thread = undefined,
        .port = 0,
    };
    srv.telemetry = telemetry_mod.Telemetry.initTest();
    srv.ctx.telemetry = &srv.telemetry;
    srv.ctx.hub = &srv.hub;
    srv.ctx.stream_registry = &srv.streams;
    var idor_env = try @import("common").env.testLiveSnapshot(alloc);
    defer idor_env.deinit();
    if (queue_redis.Client.connectFromEnv(@import("common").globalIo(), &idor_env, alloc, .default)) |client| {
        srv.queue = client;
        srv.has_redis = true;
        srv.session_store = session_store_redis.SessionStore.init(
            alloc,
            &srv.queue,
            TEST_SESSION_PEPPER,
            TEST_AUDIT_PEPPER,
        );
    } else |_| {}
    srv.ctx.queue = &srv.queue;
    srv.ctx.oidc = &srv.verifier;
    srv.ctx.auth_sessions = &srv.session_store;
    srv.registry = .{
        .bearer_or_api_key = .{ .verifier = &srv.verifier },
        // SAFETY: test fixture; field is populated by the surrounding builder before any read.
        .tenant_api_key_mw = .{ .host = undefined, .lookup = stubTenantApiKeyLookup },
        // SAFETY: stubRunnerLookup ignores host and returns null; .host unread.
        .runner_bearer_mw = .{ .host = undefined, .lookup = stubRunnerLookup },
        .require_scope_mw = .{},
        .webhook_hmac_mw = .{ .secret = "" },
    };
    srv.registry.initChains();
    try bindAndWaitReady(alloc, srv);
    return srv;
}

fn sendReq(alloc: std.mem.Allocator, url: []const u8, method: std.http.Method, token: ?[]const u8, body: ?[]const u8) !HttpResp {
    var client: std.http.Client = .{ .allocator = alloc, .io = @import("common").globalIo() };
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
    var resp_buf: std.ArrayList(u8) = .empty;
    var w: std.Io.Writer.Allocating = .fromArrayList(alloc, &resp_buf);
    const result = try client.fetch(.{ .location = .{ .url = url }, .method = method, .payload = body, .extra_headers = hdrs[0..hc], .response_writer = &w.writer });
    return .{ .status = @intFromEnum(result.status), .body = try w.toOwnedSlice() };
}

fn urlJoin(alloc: std.mem.Allocator, port: u16, comptime path_fmt: []const u8, args: anytype) ![]u8 {
    var aw: std.Io.Writer.Allocating = .init(alloc);
    defer aw.deinit();
    try aw.writer.print("http://127.0.0.1:{d}", .{port});
    try aw.writer.print(path_fmt, args);
    return aw.toOwnedSlice();
}

// ── IDOR Tests ────────────────────────────────────────────────────────────

test "IDOR: GET /workspaces/{foreign}/fleets returns 403" {
    const srv = try startTestServer(ALLOC);
    defer {
        if (srv.pool.acquire()) |c| {
            cleanupTestData(c);
            srv.pool.release(c);
        } else |_| {}
        srv.deinit();
        ALLOC.destroy(srv);
    }

    // Principal is scoped to TEST_WORKSPACE_ID; requesting OTHER_WS_ID must 403.
    const url = try urlJoin(ALLOC, srv.port, "/v1/workspaces/{s}/fleets", .{OTHER_WS_ID});
    defer ALLOC.free(url);

    const r = try sendReq(ALLOC, url, .GET, TOKEN_OPERATOR, null);
    defer r.deinit(ALLOC);
    try std.testing.expectEqual(@as(u16, 403), r.status);
}

test "IDOR: PATCH /workspaces/{my}/fleets/{foreign} status=killed returns 404" {
    const srv = try startTestServer(ALLOC);
    defer {
        if (srv.pool.acquire()) |c| {
            cleanupTestData(c);
            srv.pool.release(c);
        } else |_| {}
        srv.deinit();
        ALLOC.destroy(srv);
    }

    // Caller's ws in path, foreign fleet in path. Must 404 — the patch
    // handler scopes the UPDATE by both ids and returns 404 when no row
    // matches. The kill flow now rides on PATCH .../fleets/{id} with
    // body {status:"killed"} (folded from the retired POST /kill).
    const url = try urlJoin(ALLOC, srv.port, "/v1/workspaces/{s}/fleets/{s}", .{ TEST_WORKSPACE_ID, AGENTSFLEET_IN_FOREIGN_WS });
    defer ALLOC.free(url);

    const r = try sendReq(ALLOC, url, .PATCH, TOKEN_OPERATOR, "{\"status\":\"killed\"}");
    defer r.deinit(ALLOC);
    try std.testing.expectEqual(@as(u16, 404), r.status);
}

test "IDOR: GET /workspaces/{my}/fleets/{foreign}/activity returns 404" {
    const srv = try startTestServer(ALLOC);
    defer {
        if (srv.pool.acquire()) |c| {
            cleanupTestData(c);
            srv.pool.release(c);
        } else |_| {}
        srv.deinit();
        ALLOC.destroy(srv);
    }

    // Caller's ws in path, foreign fleet in path. Must 404 — greptile P1 regression guard.
    const url = try urlJoin(ALLOC, srv.port, "/v1/workspaces/{s}/fleets/{s}/activity", .{ TEST_WORKSPACE_ID, AGENTSFLEET_IN_FOREIGN_WS });
    defer ALLOC.free(url);

    const r = try sendReq(ALLOC, url, .GET, TOKEN_OPERATOR, null);
    defer r.deinit(ALLOC);
    try std.testing.expectEqual(@as(u16, 404), r.status);
}

test "IDOR: GET /workspaces/{foreign}/secrets returns 403" {
    const srv = try startTestServer(ALLOC);
    defer {
        if (srv.pool.acquire()) |c| {
            cleanupTestData(c);
            srv.pool.release(c);
        } else |_| {}
        srv.deinit();
        ALLOC.destroy(srv);
    }

    const url = try urlJoin(ALLOC, srv.port, "/v1/workspaces/{s}/secrets", .{OTHER_WS_ID});
    defer ALLOC.free(url);

    const r = try sendReq(ALLOC, url, .GET, TOKEN_OPERATOR, null);
    defer r.deinit(ALLOC);
    try std.testing.expectEqual(@as(u16, 403), r.status);
}

test "IDOR: GET /workspaces/{my}/fleets/{foreign}/integration-grants returns 404" {
    const srv = try startTestServer(ALLOC);
    defer {
        if (srv.pool.acquire()) |c| {
            cleanupTestData(c);
            srv.pool.release(c);
        } else |_| {}
        srv.deinit();
        ALLOC.destroy(srv);
    }

    const url = try urlJoin(ALLOC, srv.port, "/v1/workspaces/{s}/fleets/{s}/integration-grants", .{ TEST_WORKSPACE_ID, AGENTSFLEET_IN_FOREIGN_WS });
    defer ALLOC.free(url);

    const r = try sendReq(ALLOC, url, .GET, TOKEN_OPERATOR, null);
    defer r.deinit(ALLOC);
    try std.testing.expectEqual(@as(u16, 404), r.status);
}

test "IDOR: DELETE /workspaces/{my}/fleets/{foreign}/integration-grants/{g} returns 404" {
    const srv = try startTestServer(ALLOC);
    defer {
        if (srv.pool.acquire()) |c| {
            cleanupTestData(c);
            srv.pool.release(c);
        } else |_| {}
        srv.deinit();
        ALLOC.destroy(srv);
    }

    const url = try urlJoin(ALLOC, srv.port, "/v1/workspaces/{s}/fleets/{s}/integration-grants/{s}", .{ TEST_WORKSPACE_ID, AGENTSFLEET_IN_FOREIGN_WS, GRANT_ID_PLACEHOLDER });
    defer ALLOC.free(url);

    const r = try sendReq(ALLOC, url, .DELETE, TOKEN_OPERATOR, null);
    defer r.deinit(ALLOC);
    try std.testing.expectEqual(@as(u16, 404), r.status);
}

// ── getFleetWorkspaceId orelse branch — nonexistent fleet ────────────────

test "IDOR: GET activity for nonexistent fleet returns 404 (getFleetWorkspaceId orelse branch)" {
    const srv = try startTestServer(ALLOC);
    defer {
        if (srv.pool.acquire()) |c| {
            cleanupTestData(c);
            srv.pool.release(c);
        } else |_| {}
        srv.deinit();
        ALLOC.destroy(srv);
    }

    // UUIDv7 shape but nothing in core.fleets matches — exercises the `orelse`
    // branch in common.getFleetWorkspaceId rather than the !eql path.
    const nonexistent_agent = "0195b4ba-8d3a-7f13-8abc-2b3e1edead01";
    const url = try urlJoin(ALLOC, srv.port, "/v1/workspaces/{s}/fleets/{s}/activity", .{ TEST_WORKSPACE_ID, nonexistent_agent });
    defer ALLOC.free(url);

    const r = try sendReq(ALLOC, url, .GET, TOKEN_OPERATOR, null);
    defer r.deinit(ALLOC);
    try std.testing.expectEqual(@as(u16, 404), r.status);
}

// ─────────────────────────────────────────────────────────────────────────────
// REST conventions — envelope shape, method enforcement, 204 body.
// Added to this file because it shares TestServer + operator JWT + cleanup.
// ─────────────────────────────────────────────────────────────────────────────

// Tiny JSON probe that asserts a top-level string key exists in a JSON object
// body, without pulling in a full parser. Good enough for the 1-level envelope
// keys we assert here (`items`, `total`, `fleets`, `fleets`, etc.).
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

test "envelope: GET /workspaces/{my}/fleets body has items+total, no fleets key" {
    const srv = try startTestServer(ALLOC);
    defer {
        if (srv.pool.acquire()) |c| {
            cleanupTestData(c);
            srv.pool.release(c);
        } else |_| {}
        srv.deinit();
        ALLOC.destroy(srv);
    }

    const url = try urlJoin(ALLOC, srv.port, "/v1/workspaces/{s}/fleets", .{TEST_WORKSPACE_ID});
    defer ALLOC.free(url);

    const r = try sendReq(ALLOC, url, .GET, TOKEN_OPERATOR, null);
    defer r.deinit(ALLOC);
    try std.testing.expectEqual(@as(u16, 200), r.status);
    try std.testing.expect(bodyHasTopLevelKey(r.body, "items"));
    try std.testing.expect(bodyHasTopLevelKey(r.body, "total"));
    // Old collection-keyed envelope must be gone.
    try std.testing.expect(!bodyHasTopLevelKey(r.body, "fleets"));
}

test "envelope: GET /workspaces/{my}/fleet-keys body has items+total, no fleets key" {
    const srv = try startTestServer(ALLOC);
    defer {
        if (srv.pool.acquire()) |c| {
            cleanupTestData(c);
            srv.pool.release(c);
        } else |_| {}
        srv.deinit();
        ALLOC.destroy(srv);
    }

    const url = try urlJoin(ALLOC, srv.port, "/v1/workspaces/{s}/fleet-keys", .{TEST_WORKSPACE_ID});
    defer ALLOC.free(url);

    const r = try sendReq(ALLOC, url, .GET, TOKEN_OPERATOR, null);
    defer r.deinit(ALLOC);
    try std.testing.expectEqual(@as(u16, 200), r.status);
    try std.testing.expect(bodyHasTopLevelKey(r.body, "items"));
    try std.testing.expect(bodyHasTopLevelKey(r.body, "total"));
    try std.testing.expect(!bodyHasTopLevelKey(r.body, "fleets"));
}

test "memories: GET with malformed fleet_id in path returns 400" {
    const srv = try startTestServer(ALLOC);
    defer {
        if (srv.pool.acquire()) |c| {
            cleanupTestData(c);
            srv.pool.release(c);
        } else |_| {}
        srv.deinit();
        ALLOC.destroy(srv);
    }

    // Path-segment fleet_id fails UUIDv7 format check in handler — 400 from
    // resolveFleetInWorkspace before any DB access.
    const url = try urlJoin(ALLOC, srv.port, "/v1/workspaces/{s}/fleets/not-a-uuid/memories", .{TEST_WORKSPACE_ID});
    defer ALLOC.free(url);

    const r = try sendReq(ALLOC, url, .GET, TOKEN_OPERATOR, null);
    defer r.deinit(ALLOC);
    try std.testing.expectEqual(@as(u16, 400), r.status);
}

test "no-content: DELETE fleet-key returns 204 with empty body" {
    const srv = try startTestServer(ALLOC);
    defer {
        if (srv.pool.acquire()) |c| {
            cleanupTestData(c);
            srv.pool.release(c);
        } else |_| {}
        srv.deinit();
        ALLOC.destroy(srv);
    }

    // Seed a Fleet key in TEST_WORKSPACE_ID for this test only. A fleet
    // record is also required because fleet_keys.fleet_id has a FK.
    const fleet_key_id = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6204";
    const fleet_for_agent = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f99";
    const conn = try srv.pool.acquire();
    _ = try conn.exec(
        \\INSERT INTO core.fleets (id, workspace_id, name, source_markdown, config_json, status, created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, 'm26-204-test', '---\nname: m26-204\n---\nx', '{"name":"m26-204"}', 'active', 0, 0)
        \\ON CONFLICT DO NOTHING
    , .{ fleet_for_agent, TEST_WORKSPACE_ID });
    _ = try conn.exec(
        \\INSERT INTO core.fleet_keys (uid, fleet_key_id, workspace_id, fleet_id, name, description, key_hash, created_at)
        \\VALUES ($1::uuid, $1, $2::uuid, $3::uuid, 'm26-204-test', '', 'stub-hash', 0)
        \\ON CONFLICT (fleet_key_id) DO NOTHING
    , .{ fleet_key_id, TEST_WORKSPACE_ID, fleet_for_agent });
    srv.pool.release(conn);
    defer {
        if (srv.pool.acquire()) |c| {
            _ = c.exec("DELETE FROM core.fleet_keys WHERE fleet_key_id = $1", .{fleet_key_id}) catch {};
            _ = c.exec("DELETE FROM core.fleets WHERE id = $1::uuid", .{fleet_for_agent}) catch {};
            srv.pool.release(c);
        } else |_| {}
    }

    const url = try urlJoin(ALLOC, srv.port, "/v1/workspaces/{s}/fleet-keys/{s}", .{ TEST_WORKSPACE_ID, fleet_key_id });
    defer ALLOC.free(url);

    const r = try sendReq(ALLOC, url, .DELETE, TOKEN_OPERATOR, null);
    defer r.deinit(ALLOC);
    try std.testing.expectEqual(@as(u16, 204), r.status);
    // RFC 9110 section 6.4.5: 204 responses MUST NOT include a message body.
    try std.testing.expectEqual(@as(usize, 0), r.body.len);
}

test "no-content: DELETE integration-grant returns 204 with empty body" {
    const srv = try startTestServer(ALLOC);
    defer {
        if (srv.pool.acquire()) |c| {
            cleanupTestData(c);
            srv.pool.release(c);
        } else |_| {}
        srv.deinit();
        ALLOC.destroy(srv);
    }

    // Seed fleet + pending grant. Revoke path requires the grant.status != 'revoked'.
    const fleet_for_grant = "0195b4ba-8d3a-7f13-8abc-2b3e1ecafe02";
    const grant_id = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6205";
    const conn = try srv.pool.acquire();
    _ = try conn.exec(
        \\INSERT INTO core.fleets (id, workspace_id, name, source_markdown, config_json, status, created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, 'm26-grant-test', '---\nname: m26-grant\n---\nx', '{"name":"m26-grant"}', 'active', 0, 0)
        \\ON CONFLICT DO NOTHING
    , .{ fleet_for_grant, TEST_WORKSPACE_ID });
    _ = try conn.exec(
        \\INSERT INTO core.integration_grants
        \\  (uid, grant_id, fleet_id, service, status, requested_at, requested_reason)
        \\VALUES ($1::uuid, $1, $2::uuid, 'slack', 'pending', 0, 'm26 test')
        \\ON CONFLICT (grant_id) DO NOTHING
    , .{ grant_id, fleet_for_grant });
    srv.pool.release(conn);
    defer {
        if (srv.pool.acquire()) |c| {
            _ = c.exec("DELETE FROM core.integration_grants WHERE grant_id = $1", .{grant_id}) catch {};
            _ = c.exec("DELETE FROM core.fleets WHERE id = $1::uuid", .{fleet_for_grant}) catch {};
            srv.pool.release(c);
        } else |_| {}
    }

    const url = try urlJoin(ALLOC, srv.port, "/v1/workspaces/{s}/fleets/{s}/integration-grants/{s}", .{ TEST_WORKSPACE_ID, fleet_for_grant, grant_id });
    defer ALLOC.free(url);

    const r = try sendReq(ALLOC, url, .DELETE, TOKEN_OPERATOR, null);
    defer r.deinit(ALLOC);
    try std.testing.expectEqual(@as(u16, 204), r.status);
    // RFC 9110 section 6.4.5: 204 MUST NOT include a message body.
    try std.testing.expectEqual(@as(usize, 0), r.body.len);
}

test "memories: GET with limit=0 returns 400" {
    const srv = try startTestServer(ALLOC);
    defer {
        if (srv.pool.acquire()) |c| {
            cleanupTestData(c);
            srv.pool.release(c);
        } else |_| {}
        srv.deinit();
        ALLOC.destroy(srv);
    }

    // parseLimitQs returns OutOfRange → 400 before any DB access.
    const valid_zid = "0195b4ba-8d3a-7f13-8abc-2b3e1e0cafe2";
    const url = try urlJoin(ALLOC, srv.port, "/v1/workspaces/{s}/fleets/{s}/memories?limit=0", .{ TEST_WORKSPACE_ID, valid_zid });
    defer ALLOC.free(url);

    const r = try sendReq(ALLOC, url, .GET, TOKEN_OPERATOR, null);
    defer r.deinit(ALLOC);
    try std.testing.expectEqual(@as(u16, 400), r.status);
}

test "memories: GET with non-numeric limit returns 400" {
    const srv = try startTestServer(ALLOC);
    defer {
        if (srv.pool.acquire()) |c| {
            cleanupTestData(c);
            srv.pool.release(c);
        } else |_| {}
        srv.deinit();
        ALLOC.destroy(srv);
    }

    const valid_zid = "0195b4ba-8d3a-7f13-8abc-2b3e1e0cafe3";
    const url = try urlJoin(ALLOC, srv.port, "/v1/workspaces/{s}/fleets/{s}/memories?query=x&limit=abc", .{ TEST_WORKSPACE_ID, valid_zid });
    defer ALLOC.free(url);

    const r = try sendReq(ALLOC, url, .GET, TOKEN_OPERATOR, null);
    defer r.deinit(ALLOC);
    try std.testing.expectEqual(@as(u16, 400), r.status);
}
