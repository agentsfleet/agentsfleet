//! Declarative cron lifecycle integration tests for Fleet writes.

const std = @import("std");
const pg = @import("pg");

const auth_mw = @import("../../../auth/middleware/mod.zig");
const Credentials = @import("../../../cron/Credentials.zig");
const QStashClient = @import("../../../cron/QStashClient.zig");
const fixtures = @import("../../../db/test_fixtures.zig");
const error_codes = @import("../../../errors/error_registry.zig");
const id_format = @import("../../../types/id_format.zig");
const PgQuery = @import("../../../db/pg_query.zig").PgQuery;
const scope_fixtures = @import("../../test_scope_tokens.zig");
const harness_mod = @import("../../test_harness.zig");

const TestHarness = harness_mod.TestHarness;
const testing = std.testing;

const TENANT_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01";
const WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11";
const FLEET_NAME = "zoho-sprint-daily-summarizer";
const FLEETS_PATH = "/v1/workspaces/" ++ WORKSPACE_ID ++ "/fleets";

const SKILL_MD =
    \\---
    \\name: zoho-sprint-daily-summarizer
    \\description: Daily Zoho Sprints summary
    \\version: 0.1.0
    \\---
    \\Summarize yesterday's Zoho Sprints work.
;

const CRON_TRIGGER_MD =
    \\---
    \\name: zoho-sprint-daily-summarizer
    \\x-agentsfleet:
    \\  triggers:
    \\    - type: cron
    \\      schedule: "0 9 * * *"
    \\      timezone: "Asia/Kolkata"
    \\      message: "summarize Zoho Sprints"
    \\  tools: ["http_request"]
    \\  budget:
    \\    daily_dollars: 1.0
    \\---
;

const PATCH_TRIGGER_MD =
    \\---
    \\name: zoho-sprint-daily-summarizer
    \\x-agentsfleet:
    \\  triggers:
    \\    - type: cron
    \\      schedule: "15 9 * * *"
    \\      timezone: "Asia/Kolkata"
    \\      message: "summarize Zoho Sprints again"
    \\  tools: ["http_request"]
    \\  budget:
    \\    daily_dollars: 1.0
    \\---
;

const API_TRIGGER_MD =
    \\---
    \\name: zoho-sprint-daily-summarizer
    \\x-agentsfleet:
    \\  triggers:
    \\    - type: api
    \\  tools: ["http_request"]
    \\  budget:
    \\    daily_dollars: 1.0
    \\---
;

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
        return .{ .status = 200, .body = try std.fmt.allocPrint(alloc, "{{\"scheduleId\":\"{s}\"}}", .{parsed.value.schedule_id}) };
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
        if (!h.tryConnectRedis()) return error.SkipZigTest;
        const conn = try h.acquireConn();
        errdefer h.releaseConn(conn);
        try cleanupDb(conn);
        try fixtures.seedTenantById(conn, TENANT_ID, FLEET_NAME);
        try fixtures.seedWorkspaceWithTenant(conn, WORKSPACE_ID, TENANT_ID);
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
    const token = try testing.allocator.dupe(u8, "m105-fleet-cron-token");
    errdefer testing.allocator.free(token);
    const current = try testing.allocator.dupe(u8, "m105-fleet-cron-current");
    errdefer testing.allocator.free(current);
    const next = try testing.allocator.dupe(u8, "m105-fleet-cron-next");
    errdefer testing.allocator.free(next);
    return .{
        .token = token,
        .current_signing_key = current,
        .next_signing_key = next,
        .url = try testing.allocator.dupe(u8, "https://qstash.test"),
    };
}

fn cleanupDb(conn: *pg.Conn) !void {
    _ = try conn.exec("DELETE FROM core.fleet_schedules WHERE fleet_id IN (SELECT id FROM core.fleets WHERE workspace_id = $1::uuid)", .{WORKSPACE_ID});
    fixtures.teardownFleets(conn, WORKSPACE_ID);
    _ = try conn.exec("DELETE FROM core.tenant_fleet_library WHERE workspace_id = $1::uuid", .{WORKSPACE_ID});
    fixtures.teardownWorkspace(conn, WORKSPACE_ID);
    fixtures.teardownTenantById(conn, TENANT_ID);
}

fn seedTemplate(conn: *pg.Conn, alloc: std.mem.Allocator, trigger_md: []const u8) ![]const u8 {
    const id = try id_format.generateFleetLibraryId(alloc);
    errdefer alloc.free(id);
    var digest: [32]u8 = undefined;
    std.crypto.hash.sha2.Sha256.hash(SKILL_MD, &digest, .{});
    const content_hash = std.fmt.bytesToHex(digest, .lower);
    _ = try conn.exec(
        \\INSERT INTO core.tenant_fleet_library
        \\  (id, workspace_id, name, description, source_kind, source_ref, visibility,
        \\   content_hash, skill_markdown, trigger_markdown, support_files_json,
        \\   requirements_json, created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, $3, 'cron lifecycle fixture', 'upload', 'unit', 'tenant',
        \\        $4, $5, $6, '[]'::jsonb, '{}'::jsonb, 0, 0)
    , .{ id, WORKSPACE_ID, FLEET_NAME, &content_hash, SKILL_MD, trigger_md });
    return id;
}

fn installBody(alloc: std.mem.Allocator, template_id: []const u8) ![]u8 {
    return std.fmt.allocPrint(alloc, "{{\"tenant_library_id\":\"{s}\"}}", .{template_id});
}

fn createdFleetId(response: harness_mod.Response) ![]u8 {
    const Parsed = struct { fleet_id: []const u8 };
    var parsed = try std.json.parseFromSlice(Parsed, testing.allocator, response.body, .{ .ignore_unknown_fields = true });
    defer parsed.deinit();
    return testing.allocator.dupe(u8, parsed.value.fleet_id);
}

fn patchTriggerBody(alloc: std.mem.Allocator, trigger_md: []const u8) ![]u8 {
    const escaped = try std.json.Stringify.valueAlloc(alloc, trigger_md, .{});
    defer alloc.free(escaped);
    return std.fmt.allocPrint(alloc, "{{\"trigger_markdown\":{s}}}", .{escaped});
}

fn fleetPath(alloc: std.mem.Allocator, fleet_id: []const u8) ![]u8 {
    return std.fmt.allocPrint(alloc, "{s}/{s}", .{ FLEETS_PATH, fleet_id });
}

const ScheduleRow = struct {
    cron: []u8,
    desired: []u8,
    sync_status: []u8,
    generation: i64,
};

fn readSchedule(conn: *pg.Conn, alloc: std.mem.Allocator, fleet_id: []const u8) !?ScheduleRow {
    var query = PgQuery.from(try conn.query(
        \\SELECT cron_expression, desired_status, sync_status, generation
        \\FROM core.fleet_schedules
        \\WHERE fleet_id = $1::uuid AND source_key = 'trigger:cron'
    , .{fleet_id}));
    defer query.deinit();
    const row = (try query.next()) orelse return null;
    return .{
        .cron = try alloc.dupe(u8, try row.get([]const u8, 0)),
        .desired = try alloc.dupe(u8, try row.get([]const u8, 1)),
        .sync_status = try alloc.dupe(u8, try row.get([]const u8, 2)),
        .generation = try row.get(i64, 3),
    };
}

fn freeSchedule(alloc: std.mem.Allocator, row: *ScheduleRow) void {
    alloc.free(row.cron);
    alloc.free(row.desired);
    alloc.free(row.sync_status);
}

fn forceActive(conn: *pg.Conn, fleet_id: []const u8) !void {
    _ = try conn.exec("UPDATE core.fleets SET status = 'active' WHERE id = $1::uuid", .{fleet_id});
}

test "test_fleet_cron_syncs_schedule_and_lifecycle" {
    var setup = try Setup.init();
    defer setup.deinit();
    const template_id = try seedTemplate(setup.conn, testing.allocator, CRON_TRIGGER_MD);
    defer testing.allocator.free(template_id);
    const body = try installBody(testing.allocator, template_id);
    defer testing.allocator.free(body);

    var created = try (try (try setup.h.post(FLEETS_PATH).bearer(scope_fixtures.TENANT_ADMIN)).json(body)).send();
    defer created.deinit();
    try created.expectStatus(.created);
    try testing.expect(created.bodyContains("\"webhook_urls\":{}"));
    const fleet_id = try createdFleetId(created);
    defer testing.allocator.free(fleet_id);
    try testing.expectEqual(@as(u32, 1), setup.fake.calls.load(.acquire));

    var first = (try readSchedule(setup.conn, testing.allocator, fleet_id)) orelse return error.ScheduleMissing;
    defer freeSchedule(testing.allocator, &first);
    try testing.expectEqualStrings("0 9 * * *", first.cron);
    try testing.expectEqualStrings("active", first.desired);
    try testing.expectEqualStrings("synced", first.sync_status);

    try forceActive(setup.conn, fleet_id);
    const path = try fleetPath(testing.allocator, fleet_id);
    defer testing.allocator.free(path);
    const patch_body = try patchTriggerBody(testing.allocator, PATCH_TRIGGER_MD);
    defer testing.allocator.free(patch_body);
    var patched = try (try (try setup.h.request(.PATCH, path).bearer(scope_fixtures.TENANT_ADMIN)).json(patch_body)).send();
    defer patched.deinit();
    try patched.expectStatus(.ok);
    try testing.expectEqual(@as(u32, 2), setup.fake.calls.load(.acquire));
    var second = (try readSchedule(setup.conn, testing.allocator, fleet_id)) orelse return error.ScheduleMissing;
    defer freeSchedule(testing.allocator, &second);
    try testing.expectEqualStrings("15 9 * * *", second.cron);
    try testing.expect(second.generation > first.generation);

    var stopped = try (try (try setup.h.request(.PATCH, path).bearer(scope_fixtures.TENANT_ADMIN)).json("{\"status\":\"stopped\"}")).send();
    defer stopped.deinit();
    try stopped.expectStatus(.ok);
    var paused = (try readSchedule(setup.conn, testing.allocator, fleet_id)) orelse return error.ScheduleMissing;
    defer freeSchedule(testing.allocator, &paused);
    try testing.expectEqualStrings("paused", paused.desired);
    try testing.expectEqual(@as(u32, 3), setup.fake.calls.load(.acquire));

    var resumed = try (try (try setup.h.request(.PATCH, path).bearer(scope_fixtures.TENANT_ADMIN)).json("{\"status\":\"active\"}")).send();
    defer resumed.deinit();
    try resumed.expectStatus(.ok);
    var active = (try readSchedule(setup.conn, testing.allocator, fleet_id)) orelse return error.ScheduleMissing;
    defer freeSchedule(testing.allocator, &active);
    try testing.expectEqualStrings("active", active.desired);
    try testing.expectEqual(@as(u32, 4), setup.fake.calls.load(.acquire));

    var killed = try (try (try setup.h.request(.PATCH, path).bearer(scope_fixtures.TENANT_ADMIN)).json("{\"status\":\"killed\"}")).send();
    defer killed.deinit();
    try killed.expectStatus(.ok);
    var terminal = (try readSchedule(setup.conn, testing.allocator, fleet_id)) orelse return error.ScheduleMissing;
    defer freeSchedule(testing.allocator, &terminal);
    try testing.expectEqualStrings("paused", terminal.desired);
    try testing.expectEqual(@as(u32, 5), setup.fake.calls.load(.acquire));

    var deleted = try (try setup.h.delete(path).bearer(scope_fixtures.TENANT_ADMIN)).send();
    defer deleted.deinit();
    try deleted.expectStatus(.no_content);
    try testing.expect((try readSchedule(setup.conn, testing.allocator, fleet_id)) == null);
    try testing.expectEqual(@as(u32, 6), setup.fake.calls.load(.acquire));
}

test "test_install_no_cron_no_schedule" {
    var setup = try Setup.init();
    defer setup.deinit();
    setup.h.ctx.qstash_credentials = null;
    const template_id = try seedTemplate(setup.conn, testing.allocator, API_TRIGGER_MD);
    defer testing.allocator.free(template_id);
    const body = try installBody(testing.allocator, template_id);
    defer testing.allocator.free(body);
    var created = try (try (try setup.h.post(FLEETS_PATH).bearer(scope_fixtures.TENANT_ADMIN)).json(body)).send();
    defer created.deinit();
    try created.expectStatus(.created);
    const fleet_id = try createdFleetId(created);
    defer testing.allocator.free(fleet_id);
    try testing.expect((try readSchedule(setup.conn, testing.allocator, fleet_id)) == null);
    try testing.expectEqual(@as(u32, 0), setup.fake.calls.load(.acquire));
}

test "test_fleet_cron_install_fails_closed_without_qstash" {
    var setup = try Setup.init();
    defer setup.deinit();
    setup.h.ctx.qstash_credentials = null;
    const template_id = try seedTemplate(setup.conn, testing.allocator, CRON_TRIGGER_MD);
    defer testing.allocator.free(template_id);
    const body = try installBody(testing.allocator, template_id);
    defer testing.allocator.free(body);
    var created = try (try (try setup.h.post(FLEETS_PATH).bearer(scope_fixtures.TENANT_ADMIN)).json(body)).send();
    defer created.deinit();
    try created.expectStatus(.service_unavailable);
    try created.expectErrorCode(error_codes.ERR_SCHEDULE_NOT_CONFIGURED);
}
