//! Integration tests for API-key connectors: the generic connect route validates
//! against a loopback vendor before writing `fleet:<provider>` to the vault.

const std = @import("std");
const common = @import("common");
const pg = @import("pg");
const auth_mw = @import("../../../auth/middleware/mod.zig");
const harness_mod = @import("../../test_harness.zig");
const scope_tokens = @import("../../test_scope_tokens.zig");
const test_fixtures = @import("../../../db/test_fixtures.zig");
const vault = @import("../../../state/vault.zig");
const credential_key = @import("../../../fleet_runtime/credential_key.zig");
const ec = @import("../../../errors/error_registry.zig");

const TestHarness = harness_mod.TestHarness;
const testing = std.testing;
const net = std.Io.net;

const AUTHED_TENANT = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01";
const AUTHED_WS = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11";
const FOREIGN_TENANT = "0195c108-0100-7000-8000-f00000000001";
const FOREIGN_WS = "0195c108-0101-7000-8000-000000000001";
const STALL_PROBE_DEADLINE_MS: u31 = 250;

fn noopRegistry(_: *auth_mw.MiddlewareRegistry, _: *TestHarness) anyerror!void {}

fn startAuthedHarness(alloc: std.mem.Allocator) !*TestHarness {
    return TestHarness.start(alloc, .{
        .configureRegistry = noopRegistry,
        .inline_jwks_json = scope_tokens.JWKS,
        .issuer = scope_tokens.ISSUER,
        .audience = scope_tokens.AUDIENCE,
    });
}

fn seedAuthedFixtures(conn: *pg.Conn) !void {
    const now_ms = common.clock.nowMillis();
    _ = try conn.exec(
        "INSERT INTO tenants (tenant_id, name, created_at, updated_at) VALUES ($1, 'M108 API Key Tenant', $2, $2) ON CONFLICT (tenant_id) DO NOTHING",
        .{ AUTHED_TENANT, now_ms },
    );
    _ = try conn.exec(
        "INSERT INTO workspaces (workspace_id, tenant_id, created_at) VALUES ($1, $2, $3) ON CONFLICT (workspace_id) DO NOTHING",
        .{ AUTHED_WS, AUTHED_TENANT, now_ms },
    );
    try test_fixtures.seedTenantById(conn, FOREIGN_TENANT, "M108 Foreign Tenant");
    try test_fixtures.seedWorkspaceWithTenant(conn, FOREIGN_WS, FOREIGN_TENANT);
}

fn deleteFleetHandle(alloc: std.mem.Allocator, conn: *pg.Conn, provider: []const u8) void {
    const key = credential_key.allocKeyName(alloc, provider) catch return;
    defer alloc.free(key);
    _ = vault.deleteCredential(conn, AUTHED_WS, key) catch |e| std.log.warn("api key cleanup ignored: {s}", .{@errorName(e)});
}

const FakeProbe = struct {
    server: net.Server,
    port: u16,
    status: std.http.Status,
    accept_thread: std.Thread,
    stop: std.atomic.Value(bool),

    fn start(self: *FakeProbe, status: std.http.Status) !void {
        const io = common.globalIo();
        var addr = try net.IpAddress.parseIp4("127.0.0.1", 0);
        self.server = try addr.listen(io, .{ .reuse_address = true });
        self.port = try boundPort(self.server.socket.handle);
        self.status = status;
        self.stop = std.atomic.Value(bool).init(false);
        self.accept_thread = try std.Thread.spawn(.{}, acceptLoop, .{self});
    }

    fn baseUrl(self: *FakeProbe, buf: []u8) ![]const u8 {
        return std.fmt.bufPrint(buf, "http://127.0.0.1:{d}", .{self.port});
    }

    fn shutdown(self: *FakeProbe) void {
        const io = common.globalIo();
        self.stop.store(true, .release);
        var addr = net.IpAddress.parseIp4("127.0.0.1", self.port) catch return;
        if (addr.connect(io, .{ .mode = .stream })) |s| s.close(io) else |_| {}
        self.accept_thread.join();
        self.server.deinit(io);
    }

    fn acceptLoop(self: *FakeProbe) void {
        const io = common.globalIo();
        while (!self.stop.load(.acquire)) {
            const stream = self.server.accept(io) catch return;
            if (self.stop.load(.acquire)) {
                stream.close(io);
                return;
            }
            handleConn(stream, self.status);
        }
    }

    fn handleConn(stream: net.Stream, status: std.http.Status) void {
        const io = common.globalIo();
        defer stream.close(io);
        var read_buf: [4096]u8 = undefined;
        var sreader = stream.reader(io, &read_buf);
        var write_buf: [4096]u8 = undefined;
        var swriter = stream.writer(io, &write_buf);
        var http_server = std.http.Server.init(&sreader.interface, &swriter.interface);
        var req = http_server.receiveHead() catch return;
        req.respond("{}", .{ .status = status, .keep_alive = false }) catch return;
    }
};

fn boundPort(handle: std.Io.net.Socket.Handle) !u16 {
    // SAFETY: getsockname initializes `sa` before `sa.port` is read on success.
    var sa: std.posix.sockaddr.in = undefined;
    var len: std.posix.socklen_t = @sizeOf(std.posix.sockaddr.in);
    if (std.c.getsockname(handle, @ptrCast(&sa), &len) != 0) return error.GetSockNameFailed;
    return std.mem.bigToNative(u16, sa.port);
}

test "test_api_key_connect_probe_success_writes_handle" {
    const alloc = testing.allocator;
    const h = startAuthedHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    test_fixtures.setTestEncryptionKey();
    try seedAuthedFixtures(conn);
    deleteFleetHandle(alloc, conn, common.PROVIDER_DATADOG);

    var fake: FakeProbe = undefined;
    try fake.start(.ok);
    defer fake.shutdown();
    var base_buf: [64]u8 = undefined;
    h.ctx.connector_api_key_probe_base_override = try fake.baseUrl(&base_buf);

    const body = "{\"api_key\":\"dd_api_test\",\"app_key\":\"dd_app_test\",\"site\":\"us1\"}";
    const r = try (try (try h.post("/v1/workspaces/" ++ AUTHED_WS ++ "/connectors/datadog/connect").bearer(scope_tokens.TENANT_ADMIN)).json(body)).send();
    defer r.deinit();
    try r.expectStatus(.ok);
    try testing.expect(r.bodyContains("connected"));

    const key = try credential_key.allocKeyName(alloc, common.PROVIDER_DATADOG);
    defer alloc.free(key);
    var parsed = try vault.loadJson(alloc, conn, AUTHED_WS, key);
    defer parsed.deinit();
    try testing.expect(parsed.value.object.get("api_key") != null);
    try testing.expect(parsed.value.object.get("app_key") != null);
    deleteFleetHandle(alloc, conn, common.PROVIDER_DATADOG);
}

test "test_api_key_probe_rejects_no_write" {
    const alloc = testing.allocator;
    const h = startAuthedHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    test_fixtures.setTestEncryptionKey();
    try seedAuthedFixtures(conn);
    deleteFleetHandle(alloc, conn, common.PROVIDER_DATADOG);

    var fake: FakeProbe = undefined;
    try fake.start(.unauthorized);
    defer fake.shutdown();
    var base_buf: [64]u8 = undefined;
    h.ctx.connector_api_key_probe_base_override = try fake.baseUrl(&base_buf);

    const body = "{\"api_key\":\"bad\",\"app_key\":\"bad\",\"site\":\"us1\"}";
    const r = try (try (try h.post("/v1/workspaces/" ++ AUTHED_WS ++ "/connectors/datadog/connect").bearer(scope_tokens.TENANT_ADMIN)).json(body)).send();
    defer r.deinit();
    try r.expectStatus(.bad_request);
    try r.expectErrorCode(ec.ERR_CONNECTOR_PROBE_REJECTED);

    const key = try credential_key.allocKeyName(alloc, common.PROVIDER_DATADOG);
    defer alloc.free(key);
    if (vault.loadJson(alloc, conn, AUTHED_WS, key)) |parsed| {
        var p = parsed;
        p.deinit();
        return error.HandleUnexpectedlyWritten;
    } else |_| {}
}

test "test_api_key_probe_deadline_no_write" {
    const alloc = testing.allocator;
    const h = startAuthedHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    test_fixtures.setTestEncryptionKey();
    try seedAuthedFixtures(conn);
    deleteFleetHandle(alloc, conn, common.PROVIDER_DATADOG);

    const io = common.globalIo();
    var addr = try net.IpAddress.parseIp4("127.0.0.1", 0);
    var listener = try addr.listen(io, .{ .reuse_address = true });
    defer listener.deinit(io);
    const port = try boundPort(listener.socket.handle);
    var base_buf: [64]u8 = undefined;
    h.ctx.connector_api_key_probe_base_override = try std.fmt.bufPrint(&base_buf, "http://127.0.0.1:{d}", .{port});
    h.ctx.connector_api_key_probe_deadline_ms_override = STALL_PROBE_DEADLINE_MS;

    const body = "{\"api_key\":\"dd_api_test\",\"app_key\":\"dd_app_test\",\"site\":\"us1\"}";
    const r = try (try (try h.post("/v1/workspaces/" ++ AUTHED_WS ++ "/connectors/datadog/connect").bearer(scope_tokens.TENANT_ADMIN)).json(body)).send();
    defer r.deinit();
    try r.expectStatus(.bad_gateway);
    try r.expectErrorCode(ec.ERR_CONNECTOR_VENDOR_DEADLINE);

    const key = try credential_key.allocKeyName(alloc, common.PROVIDER_DATADOG);
    defer alloc.free(key);
    if (vault.loadJson(alloc, conn, AUTHED_WS, key)) |parsed| {
        var p = parsed;
        p.deinit();
        return error.HandleUnexpectedlyWritten;
    } else |_| {}
}

test "test_api_key_connect_workspace_scoped" {
    const alloc = testing.allocator;
    const h = startAuthedHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try seedAuthedFixtures(conn);

    const body = "{\"api_key\":\"dd_api_test\",\"app_key\":\"dd_app_test\",\"site\":\"us1\"}";
    {
        const r = try (try (try h.post("/v1/workspaces/" ++ AUTHED_WS ++ "/connectors/datadog/connect").bearer(scope_tokens.VIEWER)).json(body)).send();
        defer r.deinit();
        try r.expectStatus(.forbidden);
    }
    {
        const r = try (try (try h.post("/v1/workspaces/" ++ FOREIGN_WS ++ "/connectors/datadog/connect").bearer(scope_tokens.TENANT_ADMIN)).json(body)).send();
        defer r.deinit();
        try r.expectStatus(.forbidden);
    }
}
