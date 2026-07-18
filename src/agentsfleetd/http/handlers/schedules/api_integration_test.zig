//! Schedule management API integration tests.

const std = @import("std");
const common = @import("common");
const pg = @import("pg");

const auth_mw = @import("../../../auth/middleware/mod.zig");
const Credentials = @import("../../../cron/Credentials.zig");
const QStashClient = @import("../../../cron/QStashClient.zig");
const Store = @import("../../../cron/Store.zig");
const fixtures = @import("../../../db/test_fixtures.zig");
const error_codes = @import("../../../errors/error_registry.zig");
const scope_fixtures = @import("../../test_scope_tokens.zig");
const harness_mod = @import("../../test_harness.zig");

const TestHarness = harness_mod.TestHarness;
const testing = std.testing;

const TENANT_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01";
const WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11";
const OTHER_WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-1050000005f0";
const FLEET_ID = "0195b4ba-8d3a-7f13-8abc-105000000501";
const OTHER_FLEET_ID = "0195b4ba-8d3a-7f13-8abc-1050000005f1";
const FLEET_NAME = "m105-schedule-api";
const COLLECTION_PATH = "/v1/workspaces/" ++ WORKSPACE_ID ++ "/fleets/" ++ FLEET_ID ++ "/schedules";
const FOREIGN_FLEET_PATH = "/v1/workspaces/" ++ WORKSPACE_ID ++ "/fleets/" ++ OTHER_FLEET_ID ++ "/schedules";
const FOREIGN_FLEET_DELETE_PATH = "/v1/workspaces/" ++ WORKSPACE_ID ++ "/fleets/" ++ OTHER_FLEET_ID;
const FOREIGN_SCHEDULE_ID = "0195b4ba-8d3a-7f13-8abc-1050000005f2";
const FOREIGN_LEASE_TOKEN = "0195b4ba-8d3a-7f13-8abc-1050000005f3";
const SCHEDULE_CRON = "0 9 * * *";
const SCHEDULE_TIMEZONE = "Asia/Kolkata";
const SCHEDULE_MESSAGE = "summarize";
const CREATED_AT_MS: i64 = 100;
const SYNCED_AT_MS: i64 = 101;
const LEASE_UNTIL_MS: i64 = 200;
const CREATE_BODY = "{\"cron\":\"" ++ SCHEDULE_CRON ++ "\",\"timezone\":\"" ++ SCHEDULE_TIMEZONE ++ "\",\"message\":\"" ++ SCHEDULE_MESSAGE ++ "\"}";
const PATCH_BODY = "{\"cron\":\"15 9 * * *\",\"message\":\"summarize again\"}";

const FakeQStash = struct {
    status: std.atomic.Value(u16) = .init(200),
    calls: std.atomic.Value(u32) = .init(0),

    fn exchange(self: *FakeQStash) QStashClient.Exchange {
        return .{ .ptr = self, .callFn = call };
    }

    fn call(ptr: *anyopaque, alloc: std.mem.Allocator, request: QStashClient.Request) anyerror!QStashClient.Response {
        const self: *FakeQStash = @ptrCast(@alignCast(ptr));
        _ = self.calls.fetchAdd(1, .monotonic);
        const status = self.status.load(.acquire);
        if (status != 200 and status != 204) return .{ .status = status, .body = try alloc.dupe(u8, "{}") };
        if (request.method == .DELETE) return .{ .status = 204, .body = try alloc.dupe(u8, "") };
        const Parsed = struct { schedule_id: []const u8 };
        var parsed = try std.json.parseFromSlice(Parsed, alloc, request.body, .{ .ignore_unknown_fields = true });
        defer parsed.deinit();
        return .{
            .status = 200,
            .body = try std.fmt.allocPrint(alloc, "{{\"scheduleId\":\"{s}\"}}", .{parsed.value.schedule_id}),
        };
    }
};

const Setup = struct {
    h: *TestHarness,
    conn: *pg.Conn,
    creds: *Credentials,
    fake: *FakeQStash,

    fn init() !Setup {
        const h = try TestHarness.start(testing.allocator, .{
            .configureRegistry = noopRegistry,
            .inline_jwks_json = scope_fixtures.JWKS,
            .issuer = scope_fixtures.ISSUER,
            .audience = scope_fixtures.AUDIENCE,
        });
        errdefer h.deinit();
        const conn = try h.acquireConn();
        errdefer h.releaseConn(conn);
        try cleanupDb(conn);
        try fixtures.seedTenantById(conn, TENANT_ID, FLEET_NAME);
        try fixtures.seedWorkspaceWithTenant(conn, WORKSPACE_ID, TENANT_ID);
        try fixtures.seedWorkspaceWithTenant(conn, OTHER_WORKSPACE_ID, TENANT_ID);
        try fixtures.seedFleet(conn, FLEET_ID, WORKSPACE_ID, FLEET_NAME, "{}", "");
        try fixtures.seedFleet(conn, OTHER_FLEET_ID, OTHER_WORKSPACE_ID, FLEET_NAME, "{}", "");
        const creds = try testing.allocator.create(Credentials);
        errdefer testing.allocator.destroy(creds);
        creds.* = try testCredentials();
        errdefer creds.deinit(testing.allocator);
        const fake = try testing.allocator.create(FakeQStash);
        errdefer testing.allocator.destroy(fake);
        fake.* = .{};
        h.ctx.qstash_credentials = creds;
        h.ctx.qstash_exchange_override = fake.exchange();
        return .{ .h = h, .conn = conn, .creds = creds, .fake = fake };
    }

    fn deinit(self: *Setup) void {
        cleanupDb(self.conn) catch |err| std.debug.panic("cleanupDb failed: {}", .{err});
        self.h.releaseConn(self.conn);
        self.h.deinit();
        self.creds.deinit(testing.allocator);
        testing.allocator.destroy(self.creds);
        testing.allocator.destroy(self.fake);
        self.* = undefined;
    }
};

fn noopRegistry(_: *auth_mw.MiddlewareRegistry, _: *TestHarness) anyerror!void {}

fn testCredentials() !Credentials {
    const token = try testing.allocator.dupe(u8, "m105-qstash-token");
    errdefer testing.allocator.free(token);
    const current = try testing.allocator.dupe(u8, "m105-current-signing-key");
    errdefer testing.allocator.free(current);
    const next = try testing.allocator.dupe(u8, "m105-next-signing-key");
    errdefer testing.allocator.free(next);
    return .{
        .token = token,
        .current_signing_key = current,
        .next_signing_key = next,
        .url = try testing.allocator.dupe(u8, "https://qstash.test"),
    };
}

fn cleanupDb(conn: *pg.Conn) !void {
    _ = try conn.exec("DELETE FROM core.fleet_schedules WHERE fleet_id = $1::uuid", .{FLEET_ID});
    _ = try conn.exec("DELETE FROM core.fleet_schedules WHERE fleet_id = $1::uuid", .{OTHER_FLEET_ID});
    fixtures.teardownFleets(conn, OTHER_WORKSPACE_ID);
    fixtures.teardownWorkspace(conn, OTHER_WORKSPACE_ID);
    fixtures.teardownFleets(conn, WORKSPACE_ID);
    fixtures.teardownWorkspace(conn, WORKSPACE_ID);
    fixtures.teardownTenantById(conn, TENANT_ID);
}

fn create(setup: *Setup) !harness_mod.Response {
    return (try (try setup.h.post(COLLECTION_PATH).bearer(scope_fixtures.TENANT_ADMIN)).json(CREATE_BODY)).send();
}

fn seedForeignSchedule(setup: *Setup) !void {
    const store = Store.init(setup.h.ctx.pool);
    var created = switch (try store.create(testing.allocator, .{
        .fleet_id = OTHER_FLEET_ID,
        .source = .api,
        .source_key = FOREIGN_SCHEDULE_ID,
        .cron = SCHEDULE_CRON,
        .timezone = SCHEDULE_TIMEZONE,
        .message = SCHEDULE_MESSAGE,
    }, FOREIGN_SCHEDULE_ID, FOREIGN_LEASE_TOKEN, CREATED_AT_MS, LEASE_UNTIL_MS)) {
        .created => |schedule| schedule,
        else => return error.ForeignScheduleCreateFailed,
    };
    defer created.deinit(testing.allocator);
    var synced = (try store.finalizeSuccess(testing.allocator, FOREIGN_SCHEDULE_ID, created.generation, FOREIGN_LEASE_TOKEN, SYNCED_AT_MS)) orelse
        return error.ForeignScheduleFinalizeFailed;
    synced.deinit(testing.allocator);
}

fn scheduleIdFrom(response: harness_mod.Response) ![]u8 {
    const Parsed = struct { schedule_id: []const u8 };
    var parsed = try std.json.parseFromSlice(Parsed, testing.allocator, response.body, .{ .ignore_unknown_fields = true });
    defer parsed.deinit();
    return testing.allocator.dupe(u8, parsed.value.schedule_id);
}

fn itemPath(alloc: std.mem.Allocator, schedule_id: []const u8) ![]u8 {
    return std.fmt.allocPrint(alloc, "{s}/{s}", .{ COLLECTION_PATH, schedule_id });
}

test "test_create_schedule_provider_outcomes" {
    var setup = try Setup.init();
    defer setup.deinit();
    var ok = try create(&setup);
    defer ok.deinit();
    try ok.expectStatus(.created);
    try testing.expect(ok.bodyContains("\"sync_status\":\"synced\""));
    try testing.expectEqual(@as(u32, 1), setup.fake.calls.load(.acquire));

    setup.fake.status.store(500, .release);
    var failed = try create(&setup);
    defer failed.deinit();
    try failed.expectStatus(.bad_gateway);
    try failed.expectErrorCode(error_codes.ERR_SCHEDULE_PROVIDER_UNAVAILABLE);
}

test "test_patch_schedule_serialization" {
    var setup = try Setup.init();
    defer setup.deinit();
    var ok = try create(&setup);
    defer ok.deinit();
    const sid = try scheduleIdFrom(ok);
    defer testing.allocator.free(sid);
    const path = try itemPath(testing.allocator, sid);
    defer testing.allocator.free(path);

    var patch = try (try (try setup.h.request(.PATCH, path).bearer(scope_fixtures.TENANT_ADMIN)).json(PATCH_BODY)).send();
    defer patch.deinit();
    try patch.expectStatus(.ok);
    try testing.expect(patch.bodyContains("\"cron\":\"15 9 * * *\""));

    _ = try setup.conn.exec(
        "UPDATE core.fleet_schedules SET sync_token = $1::uuid, sync_lease_until = $2 WHERE uid = $3::uuid",
        .{ "0195b4ba-8d3a-7f13-8abc-105000000599", common.clock.nowMillis() + 60000, sid },
    );
    var busy = try (try (try setup.h.request(.PATCH, path).bearer(scope_fixtures.TENANT_ADMIN)).json(PATCH_BODY)).send();
    defer busy.deinit();
    try busy.expectStatus(.conflict);
    try busy.expectErrorCode(error_codes.ERR_SCHEDULE_UPDATE_BUSY);
}

test "test_delete_schedule_provider_outcomes" {
    var setup = try Setup.init();
    defer setup.deinit();
    var ok = try create(&setup);
    defer ok.deinit();
    const sid = try scheduleIdFrom(ok);
    defer testing.allocator.free(sid);
    const path = try itemPath(testing.allocator, sid);
    defer testing.allocator.free(path);
    var deleted = try (try setup.h.delete(path).bearer(scope_fixtures.TENANT_ADMIN)).send();
    defer deleted.deinit();
    try deleted.expectStatus(.no_content);

    setup.fake.status.store(500, .release);
    var failed_create = try create(&setup);
    defer failed_create.deinit();
    try failed_create.expectStatus(.bad_gateway);
    const failed_sid = try scheduleIdFromList(&setup);
    defer testing.allocator.free(failed_sid);
    const failed_path = try itemPath(testing.allocator, failed_sid);
    defer testing.allocator.free(failed_path);
    var failed_delete = try (try setup.h.delete(failed_path).bearer(scope_fixtures.TENANT_ADMIN)).send();
    defer failed_delete.deinit();
    try failed_delete.expectStatus(.bad_gateway);
}

test "should reject foreign fleet deletion before schedule removal" {
    var setup = try Setup.init();
    defer setup.deinit();
    try seedForeignSchedule(&setup);

    var denied = try (try setup.h.delete(FOREIGN_FLEET_DELETE_PATH).bearer(scope_fixtures.TENANT_ADMIN)).send();
    defer denied.deinit();
    try denied.expectStatus(.not_found);
    try denied.expectErrorCode(error_codes.ERR_AGENTSFLEET_NOT_FOUND);
    try testing.expectEqual(@as(u32, 0), setup.fake.calls.load(.acquire));

    var preserved = (try Store.init(setup.h.ctx.pool).get(testing.allocator, OTHER_FLEET_ID, FOREIGN_SCHEDULE_ID)) orelse
        return error.ForeignScheduleRemoved;
    defer preserved.deinit(testing.allocator);
}

test "test_schedule_authz" {
    var setup = try Setup.init();
    defer setup.deinit();
    var denied = try (try (try setup.h.post(COLLECTION_PATH).bearer(scope_fixtures.VIEWER)).json(CREATE_BODY)).send();
    defer denied.deinit();
    try denied.expectStatus(.forbidden);
    try denied.expectErrorCode(error_codes.ERR_INSUFFICIENT_SCOPE);
}

test "test_schedule_routes_bind_fleet_to_workspace" {
    var setup = try Setup.init();
    defer setup.deinit();
    var foreign = try (try setup.h.get(FOREIGN_FLEET_PATH).bearer(scope_fixtures.TENANT_ADMIN)).send();
    defer foreign.deinit();
    try foreign.expectStatus(.not_found);
    try foreign.expectErrorCode(error_codes.ERR_AGENTSFLEET_NOT_FOUND);
    try testing.expectEqual(@as(u32, 0), setup.fake.calls.load(.acquire));
}

test "test_schedule_sync_route" {
    var setup = try Setup.init();
    defer setup.deinit();
    setup.fake.status.store(500, .release);
    var failed = try create(&setup);
    defer failed.deinit();
    try failed.expectStatus(.bad_gateway);
    const sid = try scheduleIdFromList(&setup);
    defer testing.allocator.free(sid);
    const path = try std.fmt.allocPrint(testing.allocator, "{s}/{s}:sync", .{ COLLECTION_PATH, sid });
    defer testing.allocator.free(path);
    setup.fake.status.store(200, .release);
    var synced = try (try (try setup.h.post(path).bearer(scope_fixtures.TENANT_ADMIN)).json("{}")).send();
    defer synced.deinit();
    try synced.expectStatus(.ok);
    try testing.expect(synced.bodyContains("\"sync_status\":\"synced\""));
}

fn scheduleIdFromList(setup: *Setup) ![]u8 {
    var list = try (try setup.h.get(COLLECTION_PATH).bearer(scope_fixtures.TENANT_ADMIN)).send();
    defer list.deinit();
    try list.expectStatus(.ok);
    const ParsedItem = struct { schedule_id: []const u8 };
    const Parsed = struct { items: []ParsedItem };
    var parsed = try std.json.parseFromSlice(Parsed, testing.allocator, list.body, .{ .ignore_unknown_fields = true });
    defer parsed.deinit();
    try testing.expect(parsed.value.items.len >= 1);
    return testing.allocator.dupe(u8, parsed.value.items[0].schedule_id);
}
