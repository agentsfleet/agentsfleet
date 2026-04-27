// Test harness — supertest-style fluent API for in-process HTTP integration tests.
//
// Goal: replace the duplicated TestServer boilerplate in byok_http_integration_test.zig
// and rbac_http_integration_test.zig with one shared module. New integration tests
// (webhook, dashboard, telemetry, …) consume it directly.
//
// Contract:
//   * Runs against the LIVE test database. Never creates temp tables.
//   * Caller wires middleware via `configureRegistry` callback so every suite
//     can plug in only the auth policies it needs.
//   * Fixtures are inserted through `conn()` accessor and MUST be cleaned up
//     explicitly in the test body (not via defer) — mirrors the proven
//     byok/rbac pattern where deferred cleanup leaks connections at pool.deinit.
//
// Shape:
//   var h = try TestHarness.start(alloc, .{ .configureRegistry = myWireFn });
//   defer h.deinit();
//   const r = try h.post("/v1/foo").header("x-sig", sig).json(body).send();
//   defer r.deinit();
//   try r.expectStatus(.accepted);

const std = @import("std");
const pg = @import("pg");
const auth_sessions = @import("../auth/sessions.zig");
const oidc = @import("../auth/oidc.zig");
const queue_redis = @import("../queue/redis.zig");
const auth_mw = @import("../auth/middleware/mod.zig");
const common = @import("handlers/common.zig");
const handler = @import("handler.zig");
const http_server = @import("server.zig");
const telemetry_mod = @import("../observability/telemetry.zig");
const test_port = @import("test_port.zig");

const MAX_HEADERS = 16;

pub const Config = struct {
    /// Caller configures the middleware registry. Called AFTER init with pool
    /// already wired into ctx. Caller sets up policies it cares about; unused
    /// policies stay default (stubbed in the helper).
    configureRegistry: *const fn (reg: *auth_mw.MiddlewareRegistry, harness: *TestHarness) anyerror!void,
    /// Optional JWT verification — defaults to a no-op verifier for suites
    /// that don't exercise bearer auth.
    inline_jwks_json: []const u8 = DEFAULT_JWKS,
    issuer: []const u8 = "https://test.invalid",
    audience: []const u8 = "https://test.invalid",
    /// Max time to wait for the in-process server's `/healthz` to return 200
    /// before `start()` returns `error.ServerStartTimeout`. Default 4 s tolerates
    /// CI contention; raise for very slow runners.
    wait_timeout_ms: u32 = 4000,
};

const DEFAULT_JWKS =
    \\{"keys":[{"kty":"RSA","n":"2hg972tpbq8H6kzRZ3oVL4wZ9bO-04gJ6gCig68aluyRBzagx-7XXPCiuX80oBHBVj51kvMjT_QDNXfrwzjy4cPbwiVV4HqOGpeIZkPEopfyzs4G7mjiQmx0YuM_5WQUlUjji6Y_DfeaoH-yOhTWBMBVoI0vW_1n66CFaGuEarj3VasdWYxObJTBAM6Jn4XZDcDsBBPNGO4ku7yILkfi11FqXfBP2V8NT0hAGXVAxlWwv-8up1RDzgACp-8JWoC2-kOUJN82fGenDGKq9hW_sumO-4YPNP4U1smnw5jzLlvKa0LBrYG8IgW-3Dniuq2mojhrD_ZQClUd5rF42OyYqw","e":"AQAB","kid":"test-kid-static","use":"sig","alg":"RS256"}]}
;

pub const TestHarness = struct {
    alloc: std.mem.Allocator,
    pool: *pg.Pool,
    session_store: auth_sessions.SessionStore,
    verifier: oidc.Verifier,
    queue: queue_redis.Client,
    has_redis: bool = false,
    telemetry: telemetry_mod.Telemetry,
    registry: auth_mw.MiddlewareRegistry,
    ctx: handler.Context,
    server: *http_server.Server,
    thread: std.Thread,
    port: u16,

    /// Opportunistic Redis connect. Sets `has_redis = true` on success so
    /// tests that need the queue can `if (!h.has_redis) return error.SkipZigTest;`.
    /// Reads REDIS_URL (role .default); returns false silently if unset/unreachable.
    pub fn tryConnectRedis(self: *TestHarness) bool {
        if (self.has_redis) return true;
        if (queue_redis.Client.connectFromEnv(self.alloc, .default)) |client| {
            self.queue = client;
            self.has_redis = true;
            return true;
        } else |_| {
            return false;
        }
    }

    /// Start an in-process server on a free port. Returns `error.SkipZigTest`
    /// when the test DB is not configured (`TEST_DATABASE_URL` unset).
    pub fn start(alloc: std.mem.Allocator, cfg: Config) !*TestHarness {
        const db_ctx = (try common.openHandlerTestConn(alloc)) orelse return error.SkipZigTest;
        db_ctx.pool.release(db_ctx.conn);

        const h = try alloc.create(TestHarness);
        errdefer alloc.destroy(h);
        const port = try test_port.allocFreePort();

        h.* = .{
            .alloc = alloc,
            .pool = db_ctx.pool,
            .session_store = auth_sessions.SessionStore.init(alloc),
            .verifier = oidc.Verifier.init(alloc, .{
                .provider = .clerk,
                .jwks_url = "https://test.invalid/jwks",
                .issuer = cfg.issuer,
                .audience = cfg.audience,
                .inline_jwks_json = cfg.inline_jwks_json,
            }),
            .queue = undefined,
            .telemetry = telemetry_mod.Telemetry.initTest(),
            .registry = undefined,
            .ctx = undefined,
            .server = undefined,
            .thread = undefined,
            .port = port,
        };
        h.ctx = .{
            .pool = h.pool,
            .queue = &h.queue,
            .alloc = alloc,
            .oidc = &h.verifier,
            .auth_sessions = &h.session_store,
            .app_url = "http://127.0.0.1",
            .api_in_flight_requests = std.atomic.Value(u32).init(0),
            .api_max_in_flight_requests = 64,
            .ready_max_queue_depth = null,
            .ready_max_queue_age_ms = null,
            .telemetry = &h.telemetry,
        };
        h.registry = defaultRegistry(h, cfg);
        try cfg.configureRegistry(&h.registry, h);
        h.registry.initChains();
        h.server = try http_server.Server.init(&h.ctx, &h.registry, .{
            .port = port,
            .threads = 2,
            .workers = 2,
            .max_clients = 64,
        });
        h.thread = try std.Thread.spawn(.{}, serverThread, .{h.server});
        errdefer {
            h.server.stop();
            h.thread.join();
            h.server.deinit();
        }
        try waitForServer(alloc, port, cfg.wait_timeout_ms);
        return h;
    }

    pub fn deinit(self: *TestHarness) void {
        self.server.stop();
        self.thread.join();
        self.server.deinit();
        self.verifier.deinit();
        self.session_store.deinit();
        if (self.has_redis) self.queue.deinit();
        self.pool.deinit();
        self.alloc.destroy(self);
    }

    /// Acquire a connection for fixture insertion / cleanup. Caller releases
    /// back via `pool.release(conn)`.
    pub fn acquireConn(self: *TestHarness) !*pg.Conn {
        return self.pool.acquire();
    }
    pub fn releaseConn(self: *TestHarness, conn: *pg.Conn) void {
        self.pool.release(conn);
    }

    // ── Fluent request API ────────────────────────────────────────────────
    pub fn get(self: *TestHarness, path: []const u8) Request {
        return Request.init(self, .GET, path);
    }
    pub fn post(self: *TestHarness, path: []const u8) Request {
        return Request.init(self, .POST, path);
    }
    pub fn put(self: *TestHarness, path: []const u8) Request {
        return Request.init(self, .PUT, path);
    }
    pub fn delete(self: *TestHarness, path: []const u8) Request {
        return Request.init(self, .DELETE, path);
    }
    pub fn request(self: *TestHarness, method: std.http.Method, path: []const u8) Request {
        return Request.init(self, method, path);
    }
};

fn defaultRegistry(h: *TestHarness, cfg: Config) auth_mw.MiddlewareRegistry {
    _ = cfg;
    return .{
        .bearer_or_api_key = .{ .verifier = &h.verifier },
        .tenant_api_key_mw = .{ .host = undefined, .lookup = stubTenantApiKey },
        .require_role_admin = .{ .required = .admin },
        .require_role_operator = .{ .required = .operator },
        .slack_sig = .{ .secret = "" },
        .webhook_hmac_mw = .{ .secret = "" },
        .oauth_state_mw = .{ .signing_secret = "", .consume_ctx = &h.queue, .consume_nonce = stubConsumeNonce },
        .webhook_url_secret_mw = .{ .lookup_ctx = &h.queue, .lookup_fn = stubWebhookUrlSecret },
    };
}

fn stubConsumeNonce(_: *anyopaque, _: []const u8) anyerror!bool {
    return false;
}
fn stubWebhookUrlSecret(_: *anyopaque, _: []const u8, _: std.mem.Allocator) anyerror!?[]const u8 {
    return null;
}
fn stubTenantApiKey(_: *anyopaque, _: std.mem.Allocator, _: []const u8) anyerror!?auth_mw.tenant_api_key.LookupResult {
    return null;
}

fn serverThread(srv: *http_server.Server) void {
    srv.listen() catch |err| std.debug.panic("harness server: {s}", .{@errorName(err)});
}

fn waitForServer(alloc: std.mem.Allocator, port: u16, timeout_ms: u32) !void {
    const url = try std.fmt.allocPrint(alloc, "http://127.0.0.1:{d}/healthz", .{port});
    defer alloc.free(url);
    const poll_interval_ms: u32 = 25;
    const max_attempts: u32 = (timeout_ms + poll_interval_ms - 1) / poll_interval_ms; // ceil div
    var i: u32 = 0;
    while (i < max_attempts) : (i += 1) {
        var client: std.http.Client = .{ .allocator = alloc };
        defer client.deinit();
        var buf: std.ArrayList(u8) = .{};
        var writer: std.Io.Writer.Allocating = .fromArrayList(alloc, &buf);
        const result = client.fetch(.{
            .location = .{ .url = url },
            .method = .GET,
            .response_writer = &writer.writer,
        }) catch {
            std.Thread.sleep(poll_interval_ms * std.time.ns_per_ms);
            continue;
        };
        const body = writer.toOwnedSlice() catch &.{};
        alloc.free(body);
        if (@intFromEnum(result.status) == 200) return;
        std.Thread.sleep(poll_interval_ms * std.time.ns_per_ms);
    }
    return error.ServerStartTimeout;
}

/// Fluent request builder. Non-chaining — each method mutates and returns
/// by value. Keep on the caller's stack; `send` consumes.
pub const Request = struct {
    harness: *TestHarness,
    method: std.http.Method,
    path: []const u8,
    hdr_names: [MAX_HEADERS][]const u8 = undefined,
    hdr_values: [MAX_HEADERS][]const u8 = undefined,
    hdr_count: usize = 0,
    body: ?[]const u8 = null,
    bearer_owned: ?[]u8 = null, // allocated by bearer(); freed in send()'s defer

    fn init(h: *TestHarness, method: std.http.Method, path: []const u8) Request {
        return .{ .harness = h, .method = method, .path = path };
    }

    pub fn header(self: Request, name: []const u8, value: []const u8) !Request {
        var r = self;
        if (r.hdr_count >= MAX_HEADERS) return error.TooManyHeaders;
        r.hdr_names[r.hdr_count] = name;
        r.hdr_values[r.hdr_count] = value;
        r.hdr_count += 1;
        return r;
    }

    pub fn bearer(self: Request, token: []const u8) !Request {
        std.debug.assert(self.bearer_owned == null); // double-bearer would leak the first allocation
        const val = try std.fmt.allocPrint(self.harness.alloc, "Bearer {s}", .{token});
        errdefer self.harness.alloc.free(val);
        var r = try self.header("authorization", val);
        r.bearer_owned = val;
        return r;
    }

    /// Adds `Content-Type: application/json` and sets body. Returns
    /// `error.TooManyHeaders` on slot overflow, matching `header()`'s contract —
    /// mixed assert/error is a footgun (Greptile #233 3106330937).
    pub fn json(self: Request, body: []const u8) !Request {
        var r = try self.header("content-type", "application/json");
        r.body = body;
        return r;
    }

    /// Raw body without content-type — caller sets it via `header`.
    fn rawBody(self: Request, body: []const u8) Request {
        var r = self;
        r.body = body;
        return r;
    }

    pub fn send(self: Request) !Response {
        const alloc = self.harness.alloc;
        defer if (self.bearer_owned) |v| alloc.free(v);

        const url = try std.fmt.allocPrint(alloc, "http://127.0.0.1:{d}{s}", .{ self.harness.port, self.path });
        defer alloc.free(url);

        var hdrs: [MAX_HEADERS]std.http.Header = undefined;
        var i: usize = 0;
        while (i < self.hdr_count) : (i += 1) {
            hdrs[i] = .{ .name = self.hdr_names[i], .value = self.hdr_values[i] };
        }

        var client: std.http.Client = .{ .allocator = alloc };
        defer client.deinit();
        var buf: std.ArrayList(u8) = .{};
        var writer: std.Io.Writer.Allocating = .fromArrayList(alloc, &buf);
        const result = try client.fetch(.{
            .location = .{ .url = url },
            .method = self.method,
            .payload = self.body,
            .extra_headers = hdrs[0..self.hdr_count],
            .response_writer = &writer.writer,
        });
        return .{
            .status = @intFromEnum(result.status),
            .body = try writer.toOwnedSlice(),
            .alloc = alloc,
        };
    }
};

pub const Response = struct {
    status: u16,
    body: []u8,
    alloc: std.mem.Allocator,

    pub fn deinit(self: Response) void {
        self.alloc.free(self.body);
    }

    pub fn expectStatus(self: Response, expected: std.http.Status) !void {
        const got = self.status;
        const want: u16 = @intFromEnum(expected);
        if (got != want) {
            std.debug.print("expectStatus: want {d}, got {d}; body={s}\n", .{ want, got, self.body });
            return error.UnexpectedStatus;
        }
    }

    /// Assert the RFC7807 problem+json error code matches. Tolerant of
    /// surrounding whitespace and field ordering — does a substring match
    /// on "\"error_code\":\"<code>\"" (the field name used in this repo's
    /// error envelope; see src/http/handlers/common.zig errorResponse).
    pub fn expectErrorCode(self: Response, code: []const u8) !void {
        const needle = try std.fmt.allocPrint(self.alloc, "\"error_code\":\"{s}\"", .{code});
        defer self.alloc.free(needle);
        if (std.mem.indexOf(u8, self.body, needle) == null) {
            std.debug.print("expectErrorCode: {s} not in body={s}\n", .{ code, self.body });
            return error.ErrorCodeMismatch;
        }
    }

    pub fn bodyContains(self: Response, needle: []const u8) bool {
        return std.mem.indexOf(u8, self.body, needle) != null;
    }
};

// ── Unit tests ─────────────────────────────────────────────────────────────
//
// Scope: Request builder state + Response assertion helpers. Tests that touch
// only in-memory state — no server, no DB, no network. `.send()` and
// `TestHarness.start/deinit` are covered by the integration suites (byok,
// rbac, telemetry, dashboard, zombie_steer, tenant_api_keys, webhook).
//
// Request.init takes *TestHarness but only reads `harness.alloc` unless
// `.send()` is called. Tests build a partial harness with only `alloc` set.

/// Build a minimally-initialized TestHarness suitable for Request/Response
/// unit tests. The caller MUST NOT invoke `.send()` or harness lifecycle
/// methods on the returned value — only `alloc` is safely initialized.
fn fakeHarness(alloc: std.mem.Allocator) TestHarness {
    return TestHarness{
        .alloc = alloc,
        .pool = undefined,
        .session_store = undefined,
        .verifier = undefined,
        .queue = undefined,
        .telemetry = undefined,
        .registry = undefined,
        .ctx = undefined,
        .server = undefined,
        .thread = undefined,
        .port = 0,
    };
}

fn makeResponse(alloc: std.mem.Allocator, status: u16, body: []const u8) !Response {
    return Response{
        .status = status,
        .body = try alloc.dupe(u8, body),
        .alloc = alloc,
    };
}

// ── T1: Request builder happy paths ────────────────────────────────────────

test "Request.header stores name and value at current slot" {
    var h = fakeHarness(std.testing.allocator);
    const r = try Request.init(&h, .GET, "/foo").header("x-test", "hello");
    try std.testing.expectEqual(@as(usize, 1), r.hdr_count);
    try std.testing.expectEqualStrings("x-test", r.hdr_names[0]);
    try std.testing.expectEqualStrings("hello", r.hdr_values[0]);
}

test "Request.header chained — both headers recorded in order" {
    var h = fakeHarness(std.testing.allocator);
    const r = try (try Request.init(&h, .POST, "/bar").header("a", "1")).header("b", "2");
    try std.testing.expectEqual(@as(usize, 2), r.hdr_count);
    try std.testing.expectEqualStrings("a", r.hdr_names[0]);
    try std.testing.expectEqualStrings("b", r.hdr_names[1]);
    try std.testing.expectEqualStrings("2", r.hdr_values[1]);
}

test "Request.bearer sets authorization header and owns the allocation" {
    const alloc = std.testing.allocator;
    var h = fakeHarness(alloc);
    const r = try Request.init(&h, .GET, "/x").bearer("tok123");
    defer if (r.bearer_owned) |v| alloc.free(v);

    try std.testing.expectEqual(@as(usize, 1), r.hdr_count);
    try std.testing.expectEqualStrings("authorization", r.hdr_names[0]);
    try std.testing.expectEqualStrings("Bearer tok123", r.hdr_values[0]);
    try std.testing.expect(r.bearer_owned != null);
    try std.testing.expectEqualStrings("Bearer tok123", r.bearer_owned.?);
}

test "Request.json adds content-type header and sets body" {
    var h = fakeHarness(std.testing.allocator);
    const body = "{\"k\":1}";
    const r = try Request.init(&h, .POST, "/y").json(body);
    try std.testing.expectEqual(@as(usize, 1), r.hdr_count);
    try std.testing.expectEqualStrings("content-type", r.hdr_names[0]);
    try std.testing.expectEqualStrings("application/json", r.hdr_values[0]);
    try std.testing.expect(r.body != null);
    try std.testing.expectEqualStrings(body, r.body.?);
}

test "Request.rawBody sets body without content-type" {
    var h = fakeHarness(std.testing.allocator);
    const r = Request.init(&h, .PUT, "/z").rawBody("plain text");
    try std.testing.expectEqual(@as(usize, 0), r.hdr_count);
    try std.testing.expect(r.body != null);
    try std.testing.expectEqualStrings("plain text", r.body.?);
}

test "Request.header + json preserves prior headers" {
    var h = fakeHarness(std.testing.allocator);
    const r = try (try Request.init(&h, .POST, "/q").header("x-trace", "abc")).json("{}");
    try std.testing.expectEqual(@as(usize, 2), r.hdr_count);
    try std.testing.expectEqualStrings("x-trace", r.hdr_names[0]);
    try std.testing.expectEqualStrings("content-type", r.hdr_names[1]);
}

// ── T2: Edge cases ─────────────────────────────────────────────────────────

test "Request.header fills up to exactly MAX_HEADERS without error" {
    var h = fakeHarness(std.testing.allocator);
    var r = Request.init(&h, .GET, "/");
    var i: usize = 0;
    while (i < MAX_HEADERS) : (i += 1) {
        r = try r.header("x-fill", "v");
    }
    try std.testing.expectEqual(@as(usize, MAX_HEADERS), r.hdr_count);
}

test "Request.rawBody accepts empty string" {
    var h = fakeHarness(std.testing.allocator);
    const r = Request.init(&h, .POST, "/").rawBody("");
    try std.testing.expect(r.body != null);
    try std.testing.expectEqual(@as(usize, 0), r.body.?.len);
}

// ── T3: Negative / error paths ─────────────────────────────────────────────

test "Request.header returns error.TooManyHeaders when full" {
    var h = fakeHarness(std.testing.allocator);
    var r = Request.init(&h, .GET, "/");
    var i: usize = 0;
    while (i < MAX_HEADERS) : (i += 1) {
        r = try r.header("x", "v");
    }
    try std.testing.expectError(error.TooManyHeaders, r.header("one-too-many", "v"));
}

test "Response.expectStatus returns error.UnexpectedStatus on mismatch" {
    const alloc = std.testing.allocator;
    const r = try makeResponse(alloc, 500, "{}");
    defer r.deinit();
    try std.testing.expectError(error.UnexpectedStatus, r.expectStatus(.ok));
}

test "Response.expectErrorCode returns error.ErrorCodeMismatch when code absent" {
    const alloc = std.testing.allocator;
    const r = try makeResponse(alloc, 401, "{\"detail\":\"no code field\"}");
    defer r.deinit();
    try std.testing.expectError(error.ErrorCodeMismatch, r.expectErrorCode("UZ-WH-010"));
}

// ── T7: Regression — pins review-fix behavior so it can't silently revert ──

test "regression: bearer does not leak when header overflow triggers TooManyHeaders" {
    // Fix 014b1327: bearer() gained `errdefer self.harness.alloc.free(val)` before
    // `try self.header(...)`. Without the errdefer, TooManyHeaders leaks the
    // Bearer string. std.testing.allocator catches the leak if the errdefer
    // is removed.
    const alloc = std.testing.allocator;
    var h = fakeHarness(alloc);
    var r = Request.init(&h, .GET, "/");
    var i: usize = 0;
    while (i < MAX_HEADERS) : (i += 1) {
        r = try r.header("x", "v");
    }
    try std.testing.expectError(error.TooManyHeaders, r.bearer("tok"));
    // Implicit: std.testing.allocator asserts zero leaks at test exit.
}

test "regression: expectErrorCode matches repo's error_code field (not code)" {
    // Fix 494ad1bc: this repo's RFC7807 envelope uses "error_code":"..." not
    // "code":"...". A prior version of expectErrorCode searched for "code":"…"
    // and produced false-positive failures against real 401 responses. This
    // test pins the field name so a naive refactor back to "code":"…" fails.
    const alloc = std.testing.allocator;
    const body =
        \\{"docs_uri":"https://x/y","title":"Unauthorized","detail":"...","error_code":"UZ-AUTH-002","request_id":"req_1"}
    ;
    const r = try makeResponse(alloc, 401, body);
    defer r.deinit();
    try r.expectErrorCode("UZ-AUTH-002");
}

test "Response.bodyContains true when substring present, false otherwise" {
    const alloc = std.testing.allocator;
    const r = try makeResponse(alloc, 200, "{\"status\":\"accepted\",\"event_id\":\"evt_1\"}");
    defer r.deinit();
    try std.testing.expect(r.bodyContains("accepted"));
    try std.testing.expect(r.bodyContains("evt_1"));
    try std.testing.expect(!r.bodyContains("rejected"));
}

test "Response.expectStatus returns void on exact match" {
    const alloc = std.testing.allocator;
    const r = try makeResponse(alloc, 202, "{}");
    defer r.deinit();
    try r.expectStatus(.accepted);
}
