// Integration tests for the non-Slack OAuth provider callbacks added in M108:
// Zoho Desk, Jira, and Linear. Each drives the real Bearer-less callback route
// through TestHarness, with vendor exchanges pointed at a loopback fake.

const std = @import("std");
const common = @import("common");
const pg = @import("pg");
const auth_mw = @import("../../../auth/middleware/mod.zig");
const harness_mod = @import("../../test_harness.zig");
const test_port = @import("../../test_port.zig");
const test_fixtures = @import("../../../db/test_fixtures.zig");
const vault = @import("../../../state/vault.zig");
const credential_key = @import("../../../fleet_runtime/credential_key.zig");
const oauth2 = @import("oauth2.zig");
const ec = @import("../../../errors/error_registry.zig");
const zoho_spec = @import("zoho/spec.zig");
const jira_spec = @import("jira/spec.zig");
const linear_spec = @import("linear/spec.zig");

const TestHarness = harness_mod.TestHarness;
const net = std.Io.net;
const testing = std.testing;

const TENANT_ID = "0195c109-0000-7000-8000-f00000000001";
const TENANT_NAME = "m108-oauth-providers-suite";
const ADMIN_WS = "0195c109-0001-7000-8000-000000000001";
const TARGET_WS = "0195c109-0002-7000-8000-000000000002";
const SIGNING_SECRET = "m108-oauth-providers-signing-secret";
const FIELD_CLIENT_ID = "client_id";
const FIELD_CLIENT_SECRET = "client_secret";
const TOKEN_URL_PATH = "/oauth/token";
const CONTENT_TYPE_JSON = "application/json";

const ZOHO_REFRESH = "zoho-refresh-token";
const JIRA_REFRESH = "jira-refresh-token";
const LINEAR_REFRESH = "linear-refresh-token";
const JIRA_CLOUD_ID = "1324a887-45db-1bf4-1e99-ef0ff456d421";
const STANDARD_EXPIRES_SECONDS: i64 = 3600;
const STANDARD_EXPIRES_SECONDS_TEXT = std.fmt.comptimePrint("{d}", .{STANDARD_EXPIRES_SECONDS});
const LINEAR_EXPIRES_SECONDS: i64 = 86_399;
const LINEAR_EXPIRES_SECONDS_TEXT = std.fmt.comptimePrint("{d}", .{LINEAR_EXPIRES_SECONDS});

const ZOHO_TOKEN_BODY =
    "{\"access_token\":\"zoho-access\",\"refresh_token\":\"" ++ ZOHO_REFRESH ++
    "\",\"expires_in\":" ++ STANDARD_EXPIRES_SECONDS_TEXT ++ ",\"api_domain\":\"https://desk.zoho.com\"}";
const JIRA_TOKEN_BODY =
    "{\"access_token\":\"jira-access\",\"refresh_token\":\"" ++ JIRA_REFRESH ++
    "\",\"expires_in\":" ++ STANDARD_EXPIRES_SECONDS_TEXT ++ "}";
const JIRA_RESOURCES_BODY =
    "[{\"id\":\"" ++ JIRA_CLOUD_ID ++ "\",\"name\":\"Acme Jira\",\"url\":\"https://acme.atlassian.net\"}]";
const LINEAR_TOKEN_BODY =
    "{\"access_token\":\"linear-access\",\"refresh_token\":\"" ++ LINEAR_REFRESH ++
    "\",\"expires_in\":" ++ LINEAR_EXPIRES_SECONDS_TEXT ++ "}";

const FakeVendor = struct {
    server: net.Server,
    port: u16,
    accept_thread: std.Thread,
    stop: std.atomic.Value(bool),
    bodies: []const []const u8,
    next: std.atomic.Value(usize),

    fn start(self: *FakeVendor, bodies: []const []const u8) !void {
        const io = common.globalIo();
        const lp = try test_port.listenLoopback(io);
        self.server = lp.server;
        self.port = lp.port;
        self.stop = std.atomic.Value(bool).init(false);
        self.bodies = bodies;
        self.next = std.atomic.Value(usize).init(0);
        self.accept_thread = try std.Thread.spawn(.{}, acceptLoop, .{self});
    }

    fn shutdown(self: *FakeVendor) void {
        const io = common.globalIo();
        self.stop.store(true, .release);
        var addr = net.IpAddress.parseIp4("127.0.0.1", self.port) catch return;
        if (addr.connect(io, .{ .mode = .stream })) |s| s.close(io) else |_| {}
        self.accept_thread.join();
        self.server.deinit(io);
    }

    fn tokenUrl(self: *FakeVendor, alloc: std.mem.Allocator) ![]const u8 {
        return std.fmt.allocPrint(alloc, "http://127.0.0.1:{d}" ++ TOKEN_URL_PATH, .{self.port});
    }

    fn acceptLoop(self: *FakeVendor) void {
        const io = common.globalIo();
        while (!self.stop.load(.acquire)) {
            const stream = self.server.accept(io) catch return;
            if (self.stop.load(.acquire)) {
                stream.close(io);
                return;
            }
            handleConn(stream, self);
        }
    }

    fn handleConn(stream: net.Stream, self: *FakeVendor) void {
        const io = common.globalIo();
        defer stream.close(io);
        var read_buf: [4096]u8 = undefined;
        var sreader = stream.reader(io, &read_buf);
        var write_buf: [4096]u8 = undefined;
        var swriter = stream.writer(io, &write_buf);
        var http_server = std.http.Server.init(&sreader.interface, &swriter.interface);
        var req = http_server.receiveHead() catch return;
        const i = self.next.fetchAdd(1, .acq_rel);
        const body = if (i < self.bodies.len) self.bodies[i] else "{}";
        req.respond(body, .{
            .keep_alive = false,
            .extra_headers = &.{.{ .name = "content-type", .value = CONTENT_TYPE_JSON }},
        }) catch return;
    }
};

fn noopRegistry(_: *auth_mw.MiddlewareRegistry, _: *TestHarness) anyerror!void {}

fn seedFixtures(alloc: std.mem.Allocator, conn: *pg.Conn, provider: []const u8) !void {
    test_fixtures.setTestEncryptionKey();
    try test_fixtures.seedTenantById(conn, TENANT_ID, TENANT_NAME);
    try test_fixtures.seedWorkspaceWithTenant(conn, ADMIN_WS, TENANT_ID);
    try test_fixtures.seedWorkspaceWithTenant(conn, TARGET_WS, TENANT_ID);
    deleteFleetHandle(alloc, conn, TARGET_WS, provider);
    try seedAppCreds(alloc, conn, provider);
}

fn seedAppCreds(alloc: std.mem.Allocator, conn: *pg.Conn, provider: []const u8) !void {
    var obj: std.json.ObjectMap = .empty;
    defer obj.deinit(alloc);
    try obj.put(alloc, FIELD_CLIENT_ID, .{ .string = "m108-client-id" });
    try obj.put(alloc, FIELD_CLIENT_SECRET, .{ .string = "m108-client-secret" });
    const key = try std.fmt.allocPrint(alloc, "{s}-app", .{provider});
    defer alloc.free(key);
    try test_fixtures.storeVaultJson(alloc, conn, ADMIN_WS, key, .{ .object = obj });
}

fn deleteFleetHandle(alloc: std.mem.Allocator, conn: *pg.Conn, ws: []const u8, provider: []const u8) void {
    const key = credential_key.allocKeyName(alloc, provider) catch return;
    defer alloc.free(key);
    _ = vault.deleteCredential(conn, ws, key) catch |e| std.log.warn("oauth provider cleanup ignored: {s}", .{@errorName(e)});
}

fn driveCallback(h: *TestHarness, alloc: std.mem.Allocator, flow: oauth2.Spec, provider: []const u8, token_url: []const u8) !void {
    h.ctx.approval_signing_secret = SIGNING_SECRET;
    h.ctx.platform_admin_workspace_id = ADMIN_WS;
    h.ctx.connector_oauth_token_endpoint_override = token_url;
    const state = try oauth2.mintState(alloc, &h.queue, flow, SIGNING_SECRET, TARGET_WS, common.clock.nowMillis());
    defer alloc.free(state);
    const path = try std.fmt.allocPrint(alloc, "/v1/connectors/{s}/callback?code=fake-code&state={s}", .{ provider, state });
    defer alloc.free(path);
    const r = try h.get(path).redirectBehavior(.unhandled).send();
    defer r.deinit();
    try r.expectStatus(.found);
}

fn loadHandle(alloc: std.mem.Allocator, conn: *pg.Conn, provider: []const u8) !std.json.Parsed(std.json.Value) {
    const key = try credential_key.allocKeyName(alloc, provider);
    defer alloc.free(key);
    return vault.loadJson(alloc, conn, TARGET_WS, key);
}

test "test_zoho_callback_vaults_refresh_handle" {
    const alloc = testing.allocator;
    const h = TestHarness.start(alloc, .{ .configureRegistry = noopRegistry }) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try seedFixtures(alloc, conn, common.PROVIDER_ZOHO);
    var fake: FakeVendor = undefined;
    try fake.start(&.{ZOHO_TOKEN_BODY});
    defer fake.shutdown();
    const token_url = try fake.tokenUrl(alloc);
    defer alloc.free(token_url);
    try driveCallback(h, alloc, zoho_spec.SPEC, common.PROVIDER_ZOHO, token_url);

    var parsed = try loadHandle(alloc, conn, common.PROVIDER_ZOHO);
    defer parsed.deinit();
    const obj = parsed.value.object;
    try testing.expectEqualStrings(common.PROVIDER_ZOHO, obj.get("integration").?.string);
    try testing.expectEqualStrings(ZOHO_REFRESH, obj.get("refresh_token").?.string);
    try testing.expectEqualStrings("https://accounts.zoho.com", obj.get("accounts_base").?.string);
}

test "test_jira_callback_resolves_cloud_id" {
    const alloc = testing.allocator;
    const h = TestHarness.start(alloc, .{ .configureRegistry = noopRegistry }) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try seedFixtures(alloc, conn, common.PROVIDER_JIRA);
    var fake: FakeVendor = undefined;
    try fake.start(&.{ JIRA_TOKEN_BODY, JIRA_RESOURCES_BODY });
    defer fake.shutdown();
    const token_url = try fake.tokenUrl(alloc);
    defer alloc.free(token_url);
    try driveCallback(h, alloc, jira_spec.SPEC, common.PROVIDER_JIRA, token_url);

    var parsed = try loadHandle(alloc, conn, common.PROVIDER_JIRA);
    defer parsed.deinit();
    const obj = parsed.value.object;
    try testing.expectEqualStrings(JIRA_REFRESH, obj.get("refresh_token").?.string);
    try testing.expectEqualStrings(JIRA_CLOUD_ID, obj.get("cloud_id").?.string);
    try testing.expectEqualStrings("https://acme.atlassian.net", obj.get("site_url").?.string);
}

test "test_linear_callback_vaults_refresh_handle" {
    const alloc = testing.allocator;
    const h = TestHarness.start(alloc, .{ .configureRegistry = noopRegistry }) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try seedFixtures(alloc, conn, common.PROVIDER_LINEAR);
    var fake: FakeVendor = undefined;
    try fake.start(&.{LINEAR_TOKEN_BODY});
    defer fake.shutdown();
    const token_url = try fake.tokenUrl(alloc);
    defer alloc.free(token_url);
    try driveCallback(h, alloc, linear_spec.SPEC, common.PROVIDER_LINEAR, token_url);

    var parsed = try loadHandle(alloc, conn, common.PROVIDER_LINEAR);
    defer parsed.deinit();
    const obj = parsed.value.object;
    try testing.expectEqualStrings(common.PROVIDER_LINEAR, obj.get("integration").?.string);
    try testing.expectEqualStrings(LINEAR_REFRESH, obj.get("refresh_token").?.string);
    try testing.expect(obj.get("access_token") != null);
}

test "test_new_provider_state_forgery_rejected" {
    const alloc = testing.allocator;
    const h = TestHarness.start(alloc, .{ .configureRegistry = noopRegistry }) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try seedFixtures(alloc, conn, common.PROVIDER_ZOHO);
    h.ctx.approval_signing_secret = SIGNING_SECRET;
    h.ctx.platform_admin_workspace_id = ADMIN_WS;
    const good = try oauth2.mintState(alloc, &h.queue, zoho_spec.SPEC, SIGNING_SECRET, TARGET_WS, common.clock.nowMillis());
    defer alloc.free(good);
    const forged = try alloc.dupe(u8, good);
    defer alloc.free(forged);
    forged[forged.len - 1] = if (forged[forged.len - 1] == 'A') 'B' else 'A';
    const path = try std.fmt.allocPrint(alloc, "/v1/connectors/zoho/callback?code=fake-code&state={s}", .{forged});
    defer alloc.free(path);
    const r = try h.get(path).redirectBehavior(.unhandled).send();
    defer r.deinit();
    try r.expectStatus(.bad_request);
    try r.expectErrorCode(ec.ERR_CONNECTOR_STATE_INVALID);
    if (loadHandle(alloc, conn, common.PROVIDER_ZOHO)) |parsed| {
        var p = parsed;
        p.deinit();
        return error.HandleUnexpectedlyWritten;
    } else |_| {}
}
