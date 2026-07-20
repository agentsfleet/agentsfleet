//! GitHub App callback persistence through the real router and datastore.

const std = @import("std");
const pg = @import("pg");
const common = @import("common");
const auth_mw = @import("../../../../auth/middleware/mod.zig");
const harness_mod = @import("../../../test_harness.zig");
const test_port = @import("../../../test_port.zig");
const fixtures = @import("../../../../db/test_fixtures.zig");
const PgQuery = @import("../../../../db/pg_query.zig").PgQuery;
const ec = @import("../../../../errors/error_registry.zig");
const vault = @import("../../../../state/vault.zig");
const id_format = @import("../../../../types/id_format.zig");
const connector_state = @import("../state.zig");
const spec = @import("spec.zig");
const sql = @import("sql.zig");

const TestHarness = harness_mod.TestHarness;
const testing = std.testing;

const TENANT_ID = "0195c102-0000-7000-8000-f00000000001";
const TENANT_NAME = "m102-github-callback";
const WORKSPACE_ID = "0195c102-0001-7000-8000-000000000001";
const OTHER_WORKSPACE_ID = "0195c102-0002-7000-8000-000000000002";
const ADMIN_WORKSPACE_ID = "0195c102-0003-7000-8000-000000000003";
const SIGNING_SECRET = "m102-github-callback-signing-key";
const FIRST_INSTALL = "42424242";
const NEXT_INSTALL = "43434343";
const FAKE_CODE = "github-user-code";
const TOKEN_PATH = "/login/oauth/access_token";
const CALLBACK_PATH_FMT = "/v1/connectors/github/callback?installation_id={s}&code={s}&state={s}";
const CONTENT_TYPE_JSON = "application/json";
const USER_TOKEN_BODY = "{\"access_token\":\"github-user-token\"}";
const EXPECTED_REDIRECT = "http://127.0.0.1/w/" ++ WORKSPACE_ID ++ "/integrations";
const WAIT_SLICE_NS = 1 * std.time.ns_per_ms;
const WAIT_ATTEMPTS = 2000;

const net = std.Io.net;

const FakeGitHub = struct {
    server: net.Server,
    port: u16,
    accept_thread: std.Thread,
    stop: std.atomic.Value(bool),
    hold_first_ownership: bool,
    release_ownership: std.atomic.Value(bool),
    ownership_waiting: std.atomic.Value(bool),
    ownership_seen: std.atomic.Value(usize),
    ownership_status: std.http.Status,
    calls: std.atomic.Value(usize),

    fn start(self: *FakeGitHub, ownership_status: std.http.Status) !void {
        const io = common.globalIo();
        const listener = try test_port.listenLoopback(io);
        self.server = listener.server;
        self.port = listener.port;
        self.stop = .init(false);
        self.hold_first_ownership = false;
        self.release_ownership = .init(false);
        self.ownership_waiting = .init(false);
        self.ownership_seen = .init(0);
        self.ownership_status = ownership_status;
        self.calls = .init(0);
        self.accept_thread = try std.Thread.spawn(.{}, acceptLoop, .{self});
    }

    fn holdFirstOwnership(self: *FakeGitHub) void {
        self.hold_first_ownership = true;
    }

    fn releaseFirstOwnership(self: *FakeGitHub) void {
        self.release_ownership.store(true, .release);
    }

    fn shutdown(self: *FakeGitHub) void {
        const io = common.globalIo();
        self.stop.store(true, .release);
        var address = net.IpAddress.parseIp4("127.0.0.1", self.port) catch return;
        if (address.connect(io, .{ .mode = .stream })) |stream| stream.close(io) else |_| {}
        self.accept_thread.join();
        self.server.deinit(io);
    }

    fn baseUrl(self: *FakeGitHub, alloc: std.mem.Allocator) ![]const u8 {
        return std.fmt.allocPrint(alloc, "http://127.0.0.1:{d}", .{self.port});
    }

    fn acceptLoop(self: *FakeGitHub) void {
        const io = common.globalIo();
        while (!self.stop.load(.acquire)) {
            const stream = self.server.accept(io) catch return;
            if (self.stop.load(.acquire)) {
                stream.close(io);
                return;
            }
            const thread = std.Thread.spawn(.{}, handleConn, .{ stream, self }) catch {
                stream.close(io);
                continue;
            };
            thread.detach();
        }
    }

    fn handleConn(stream: net.Stream, self: *FakeGitHub) void {
        const io = common.globalIo();
        defer stream.close(io);
        var read_buf: [4096]u8 = undefined;
        var reader = stream.reader(io, &read_buf);
        var write_buf: [4096]u8 = undefined;
        var writer = stream.writer(io, &write_buf);
        var server = std.http.Server.init(&reader.interface, &writer.interface);
        var request = server.receiveHead() catch return;
        _ = self.calls.fetchAdd(1, .acq_rel);
        const target = request.head.target;
        const is_token = std.mem.startsWith(u8, target, TOKEN_PATH);
        const is_ownership = std.mem.startsWith(u8, target, "/user/installations/");
        if (is_ownership and self.hold_first_ownership and self.ownership_seen.fetchAdd(1, .acq_rel) == 0) {
            self.ownership_waiting.store(true, .release);
            while (!self.stop.load(.acquire) and !self.release_ownership.load(.acquire)) common.sleepNanos(WAIT_SLICE_NS);
        }
        request.respond(if (is_token) USER_TOKEN_BODY else "{}", .{
            .status = if (is_token) .ok else if (is_ownership) self.ownership_status else .not_found,
            .keep_alive = false,
            .extra_headers = &.{.{ .name = "content-type", .value = CONTENT_TYPE_JSON }},
        }) catch return;
    }
};

fn noopRegistry(_: *auth_mw.MiddlewareRegistry, _: *TestHarness) anyerror!void {}

fn cleanup(conn: *pg.Conn) void {
    _ = conn.exec(sql.DELETE_WORKSPACE_INSTALLS, .{ spec.PROVIDER, WORKSPACE_ID }) catch |err| std.log.warn("github callback cleanup ignored: {s}", .{@errorName(err)});
    _ = conn.exec(sql.DELETE_WORKSPACE_INSTALLS, .{ spec.PROVIDER, OTHER_WORKSPACE_ID }) catch |err| std.log.warn("github callback cleanup ignored: {s}", .{@errorName(err)});
    _ = vault.deleteCredential(conn, WORKSPACE_ID, spec.PROVIDER) catch |err| std.log.warn("github callback vault cleanup ignored: {s}", .{@errorName(err)});
}

fn seedAppCreds(alloc: std.mem.Allocator, conn: *pg.Conn) !void {
    var obj: std.json.ObjectMap = .empty;
    defer obj.deinit(alloc);
    try obj.put(alloc, "client_id", .{ .string = "github-client-id" });
    try obj.put(alloc, "client_secret", .{ .string = "github-client-secret" });
    try fixtures.storeVaultJson(alloc, conn, ADMIN_WORKSPACE_ID, "github-app", .{ .object = obj });
}

fn configureGithub(h: *TestHarness, fake: *FakeGitHub, alloc: std.mem.Allocator) ![]const u8 {
    const base = try fake.baseUrl(alloc);
    h.ctx.approval_signing_secret = SIGNING_SECRET;
    h.ctx.platform_admin_workspace_id = ADMIN_WORKSPACE_ID;
    h.ctx.connector_github_api_base_override = base;
    h.ctx.connector_oauth_token_endpoint_override = try std.fmt.allocPrint(alloc, "{s}" ++ TOKEN_PATH, .{base});
    return base;
}

const CallbackResult = struct {
    status: u16 = 0,
    has_state_error: bool = false,
};

fn sendCallback(h: *TestHarness, path: []const u8, result: *CallbackResult) void {
    const response = h.get(path).redirectBehavior(.unhandled).send() catch return;
    defer response.deinit();
    result.status = response.status;
    result.has_state_error = response.bodyContains(ec.ERR_CONNECTOR_STATE_INVALID);
}

fn waitForFirstOwnership(fake: *FakeGitHub) !void {
    for (0..WAIT_ATTEMPTS) |_| {
        if (fake.ownership_waiting.load(.acquire)) return;
        common.sleepNanos(WAIT_SLICE_NS);
    }
    return error.OwnershipRequestDidNotBlock;
}

fn expectInstall(conn: *pg.Conn, installation_id: []const u8, expected: ?[]const u8) !void {
    var query = PgQuery.from(try conn.query(sql.SELECT_INSTALL, .{ spec.PROVIDER, installation_id }));
    defer query.deinit();
    const row = try query.next();
    if (expected) |workspace_id| {
        const found = row orelse return error.InstallRowMissing;
        try testing.expectEqualStrings(workspace_id, try found.get([]const u8, 0));
        try testing.expectEqualStrings("", try found.get([]const u8, 1));
        try testing.expectEqual(@as(i32, 0), try found.get(i32, 2));
        try testing.expect((try query.next()) == null);
    } else try testing.expect(row == null);
}

fn mintLatestState(h: *TestHarness, workspace_id: []const u8) ![]const u8 {
    const state = try connector_state.mint(testing.allocator, &h.queue, spec.STATE, SIGNING_SECRET, workspace_id, common.clock.nowMillis());
    errdefer testing.allocator.free(state);
    try connector_state.markLatest(&h.queue, spec.STATE, workspace_id, state);
    return state;
}

fn connect(h: *TestHarness, installation_id: []const u8) !void {
    const state = try mintLatestState(h, WORKSPACE_ID);
    defer testing.allocator.free(state);
    const path = try std.fmt.allocPrint(testing.allocator, CALLBACK_PATH_FMT, .{ installation_id, FAKE_CODE, state });
    defer testing.allocator.free(path);
    const response = try h.get(path).redirectBehavior(.unhandled).send();
    defer response.deinit();
    try response.expectStatus(.found);
    try testing.expectEqualStrings(EXPECTED_REDIRECT, response.header("location") orelse return error.RedirectLocationMissing);
}

fn seedInstall(conn: *pg.Conn, installation_id: []const u8, workspace_id: []const u8) !void {
    const uid = try id_format.generateConnectorInstallId(testing.allocator);
    defer testing.allocator.free(uid);
    const no_scopes: []const []const u8 = &.{};
    var query = PgQuery.from(try conn.query(sql.UPSERT_INSTALL, .{
        uid,
        spec.PROVIDER,
        installation_id,
        workspace_id,
        "",
        no_scopes,
        common.clock.nowMillis(),
    }));
    defer query.deinit();
    try testing.expect((try query.next()) != null);
}

test "integration: GitHub callback atomically replaces handle and routing row" {
    const h = TestHarness.start(testing.allocator, .{ .configureRegistry = noopRegistry }) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    fixtures.setTestEncryptionKey();
    try fixtures.seedTenantById(conn, TENANT_ID, TENANT_NAME);
    try fixtures.seedWorkspaceWithTenant(conn, WORKSPACE_ID, TENANT_ID);
    try fixtures.seedWorkspaceWithTenant(conn, ADMIN_WORKSPACE_ID, TENANT_ID);
    cleanup(conn);
    defer cleanup(conn);
    try seedAppCreds(testing.allocator, conn);
    var fake: FakeGitHub = undefined;
    try fake.start(.ok);
    defer fake.shutdown();
    const base = try configureGithub(h, &fake, testing.allocator);
    defer testing.allocator.free(base);
    defer testing.allocator.free(h.ctx.connector_oauth_token_endpoint_override.?);

    try connect(h, FIRST_INSTALL);
    try expectInstall(conn, FIRST_INSTALL, WORKSPACE_ID);
    try connect(h, NEXT_INSTALL);

    var handle = try vault.loadJson(testing.allocator, conn, WORKSPACE_ID, spec.PROVIDER);
    defer handle.deinit();
    try testing.expectEqualStrings(NEXT_INSTALL, handle.value.object.get("installation_id").?.string);
    try expectInstall(conn, FIRST_INSTALL, null);
    try expectInstall(conn, NEXT_INSTALL, WORKSPACE_ID);
    try testing.expectEqual(@as(usize, 4), fake.calls.load(.acquire));
}

test "integration: GitHub callback rejects a stale app-install state after a newer start" {
    const h = TestHarness.start(testing.allocator, .{ .configureRegistry = noopRegistry }) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    fixtures.setTestEncryptionKey();
    try fixtures.seedTenantById(conn, TENANT_ID, TENANT_NAME);
    try fixtures.seedWorkspaceWithTenant(conn, WORKSPACE_ID, TENANT_ID);
    try fixtures.seedWorkspaceWithTenant(conn, ADMIN_WORKSPACE_ID, TENANT_ID);
    cleanup(conn);
    defer cleanup(conn);
    try seedAppCreds(testing.allocator, conn);
    var fake: FakeGitHub = undefined;
    try fake.start(.ok);
    defer fake.shutdown();
    const base = try configureGithub(h, &fake, testing.allocator);
    defer testing.allocator.free(base);
    defer testing.allocator.free(h.ctx.connector_oauth_token_endpoint_override.?);

    const stale_state = try mintLatestState(h, WORKSPACE_ID);
    defer testing.allocator.free(stale_state);
    const current_state = try mintLatestState(h, WORKSPACE_ID);
    defer testing.allocator.free(current_state);

    const current_path = try std.fmt.allocPrint(testing.allocator, CALLBACK_PATH_FMT, .{ NEXT_INSTALL, FAKE_CODE, current_state });
    defer testing.allocator.free(current_path);
    const current = try h.get(current_path).redirectBehavior(.unhandled).send();
    defer current.deinit();
    try current.expectStatus(.found);

    const stale_path = try std.fmt.allocPrint(testing.allocator, CALLBACK_PATH_FMT, .{ FIRST_INSTALL, FAKE_CODE, stale_state });
    defer testing.allocator.free(stale_path);
    const stale = try h.get(stale_path).redirectBehavior(.unhandled).send();
    defer stale.deinit();
    try stale.expectStatus(.bad_request);
    try stale.expectErrorCode(ec.ERR_CONNECTOR_STATE_INVALID);
    try expectInstall(conn, FIRST_INSTALL, null);
    try expectInstall(conn, NEXT_INSTALL, WORKSPACE_ID);
}

test "integration: GitHub callback keeps state freshness through final persistence" {
    const h = TestHarness.start(testing.allocator, .{ .configureRegistry = noopRegistry }) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    fixtures.setTestEncryptionKey();
    try fixtures.seedTenantById(conn, TENANT_ID, TENANT_NAME);
    try fixtures.seedWorkspaceWithTenant(conn, WORKSPACE_ID, TENANT_ID);
    try fixtures.seedWorkspaceWithTenant(conn, ADMIN_WORKSPACE_ID, TENANT_ID);
    cleanup(conn);
    defer cleanup(conn);
    try seedAppCreds(testing.allocator, conn);
    var fake: FakeGitHub = undefined;
    try fake.start(.ok);
    fake.holdFirstOwnership();
    defer fake.shutdown();
    const base = try configureGithub(h, &fake, testing.allocator);
    defer testing.allocator.free(base);
    defer testing.allocator.free(h.ctx.connector_oauth_token_endpoint_override.?);

    const stale_state = try mintLatestState(h, WORKSPACE_ID);
    defer testing.allocator.free(stale_state);
    const stale_path = try std.fmt.allocPrint(testing.allocator, CALLBACK_PATH_FMT, .{ FIRST_INSTALL, FAKE_CODE, stale_state });
    defer testing.allocator.free(stale_path);
    var stale_result: CallbackResult = .{};
    const stale_thread = try std.Thread.spawn(.{}, sendCallback, .{ h, stale_path, &stale_result });
    errdefer {
        fake.releaseFirstOwnership();
        stale_thread.join();
    }
    try waitForFirstOwnership(&fake);

    const current_state = try mintLatestState(h, WORKSPACE_ID);
    defer testing.allocator.free(current_state);
    const current_path = try std.fmt.allocPrint(testing.allocator, CALLBACK_PATH_FMT, .{ NEXT_INSTALL, FAKE_CODE, current_state });
    defer testing.allocator.free(current_path);
    const current = try h.get(current_path).redirectBehavior(.unhandled).send();
    defer current.deinit();
    try current.expectStatus(.found);

    fake.releaseFirstOwnership();
    stale_thread.join();
    try testing.expectEqual(@as(u16, 400), stale_result.status);
    try testing.expect(stale_result.has_state_error);
    try expectInstall(conn, FIRST_INSTALL, null);
    try expectInstall(conn, NEXT_INSTALL, WORKSPACE_ID);
    var handle = try vault.loadJson(testing.allocator, conn, WORKSPACE_ID, spec.PROVIDER);
    defer handle.deinit();
    try testing.expectEqualStrings(NEXT_INSTALL, handle.value.object.get("installation_id").?.string);
}

test "integration: GitHub callback rejects an installation owned by another workspace and rolls back" {
    const h = TestHarness.start(testing.allocator, .{ .configureRegistry = noopRegistry }) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    fixtures.setTestEncryptionKey();
    try fixtures.seedTenantById(conn, TENANT_ID, TENANT_NAME);
    try fixtures.seedWorkspaceWithTenant(conn, WORKSPACE_ID, TENANT_ID);
    try fixtures.seedWorkspaceWithTenant(conn, OTHER_WORKSPACE_ID, TENANT_ID);
    try fixtures.seedWorkspaceWithTenant(conn, ADMIN_WORKSPACE_ID, TENANT_ID);
    cleanup(conn);
    defer cleanup(conn);
    try seedAppCreds(testing.allocator, conn);
    var fake: FakeGitHub = undefined;
    try fake.start(.ok);
    defer fake.shutdown();
    const base = try configureGithub(h, &fake, testing.allocator);
    defer testing.allocator.free(base);
    defer testing.allocator.free(h.ctx.connector_oauth_token_endpoint_override.?);

    try connect(h, FIRST_INSTALL);
    try seedInstall(conn, NEXT_INSTALL, OTHER_WORKSPACE_ID);
    const state = try mintLatestState(h, WORKSPACE_ID);
    defer testing.allocator.free(state);
    const path = try std.fmt.allocPrint(testing.allocator, CALLBACK_PATH_FMT, .{ NEXT_INSTALL, FAKE_CODE, state });
    defer testing.allocator.free(path);
    const response = try h.get(path).redirectBehavior(.unhandled).send();
    defer response.deinit();
    try response.expectStatus(.forbidden);
    try response.expectErrorCode("UZ-CONN-008");

    var handle = try vault.loadJson(testing.allocator, conn, WORKSPACE_ID, spec.PROVIDER);
    defer handle.deinit();
    try testing.expectEqualStrings(FIRST_INSTALL, handle.value.object.get("installation_id").?.string);
    try expectInstall(conn, FIRST_INSTALL, WORKSPACE_ID);
    try expectInstall(conn, NEXT_INSTALL, OTHER_WORKSPACE_ID);
}

test "integration: GitHub callback rejects an installation absent from the authorized user's account" {
    const h = TestHarness.start(testing.allocator, .{ .configureRegistry = noopRegistry }) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    fixtures.setTestEncryptionKey();
    try fixtures.seedTenantById(conn, TENANT_ID, TENANT_NAME);
    try fixtures.seedWorkspaceWithTenant(conn, WORKSPACE_ID, TENANT_ID);
    try fixtures.seedWorkspaceWithTenant(conn, ADMIN_WORKSPACE_ID, TENANT_ID);
    cleanup(conn);
    defer cleanup(conn);
    try seedAppCreds(testing.allocator, conn);
    var fake: FakeGitHub = undefined;
    try fake.start(.not_found);
    defer fake.shutdown();
    const base = try configureGithub(h, &fake, testing.allocator);
    defer testing.allocator.free(base);
    defer testing.allocator.free(h.ctx.connector_oauth_token_endpoint_override.?);

    const state = try mintLatestState(h, WORKSPACE_ID);
    defer testing.allocator.free(state);
    const path = try std.fmt.allocPrint(testing.allocator, CALLBACK_PATH_FMT, .{ FIRST_INSTALL, FAKE_CODE, state });
    defer testing.allocator.free(path);
    const response = try h.get(path).redirectBehavior(.unhandled).send();
    defer response.deinit();
    try response.expectStatus(.forbidden);
    try response.expectErrorCode("UZ-CONN-008");
    try expectInstall(conn, FIRST_INSTALL, null);
    try testing.expectError(error.NotFound, vault.loadJson(testing.allocator, conn, WORKSPACE_ID, spec.PROVIDER));
}
