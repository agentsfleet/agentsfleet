//! End-to-end QStash fire proofs through HTTP, PostgreSQL, and Redis.

const std = @import("std");
const common = @import("common");
const hmac_sig = @import("hmac_sig");
const pg = @import("pg");

const auth_mw = @import("../../../auth/middleware/mod.zig");
const cron_constants = @import("../../../cron/constants.zig");
const Credentials = @import("../../../cron/Credentials.zig");
const QStashVerifier = @import("../../../cron/QStashVerifier.zig");
const Store = @import("../../../cron/Store.zig");
const fixtures = @import("../../../db/test_fixtures.zig");
const error_codes = @import("../../../errors/error_registry.zig");
const harness_mod = @import("../../test_harness.zig");

const TestHarness = harness_mod.TestHarness;
const testing = std.testing;
const PATH = cron_constants.ingress_path;
const CURRENT_KEY = "m105-current-signing-key";
const NEXT_KEY = "m105-next-signing-key";
const CONTENDERS: usize = 100;
const GENERATION: i64 = 1;

const Ids = struct {
    tenant: []const u8,
    workspace: []const u8,
    fleet: []const u8,
    schedule: []const u8,
};

const ID_SETS = [_]Ids{
    .{ .tenant = "0195b4ba-8d3a-7f13-8abc-105000000410", .workspace = "0195b4ba-8d3a-7f13-8abc-105000000411", .fleet = "0195b4ba-8d3a-7f13-8abc-105000000412", .schedule = "0195b4ba-8d3a-7f13-8abc-105000000413" },
    .{ .tenant = "0195b4ba-8d3a-7f13-8abc-105000000420", .workspace = "0195b4ba-8d3a-7f13-8abc-105000000421", .fleet = "0195b4ba-8d3a-7f13-8abc-105000000422", .schedule = "0195b4ba-8d3a-7f13-8abc-105000000423" },
    .{ .tenant = "0195b4ba-8d3a-7f13-8abc-105000000430", .workspace = "0195b4ba-8d3a-7f13-8abc-105000000431", .fleet = "0195b4ba-8d3a-7f13-8abc-105000000432", .schedule = "0195b4ba-8d3a-7f13-8abc-105000000433" },
    .{ .tenant = "0195b4ba-8d3a-7f13-8abc-105000000440", .workspace = "0195b4ba-8d3a-7f13-8abc-105000000441", .fleet = "0195b4ba-8d3a-7f13-8abc-105000000442", .schedule = "0195b4ba-8d3a-7f13-8abc-105000000443" },
    .{ .tenant = "0195b4ba-8d3a-7f13-8abc-105000000450", .workspace = "0195b4ba-8d3a-7f13-8abc-105000000451", .fleet = "0195b4ba-8d3a-7f13-8abc-105000000452", .schedule = "0195b4ba-8d3a-7f13-8abc-105000000453" },
};

const Setup = struct {
    harness: *TestHarness,
    conn: *pg.Conn,
    credentials: *Credentials,
    ids: Ids,

    fn init(index: usize) !Setup {
        const h = TestHarness.start(testing.allocator, .{ .configureRegistry = noopRegistry }) catch |err| switch (err) {
            error.SkipZigTest => return error.SkipZigTest,
            else => return err,
        };
        errdefer h.deinit();
        const conn = try h.acquireConn();
        errdefer h.releaseConn(conn);
        const ids = ID_SETS[index];
        try cleanupDb(conn, ids);
        try fixtures.seedTenantById(conn, ids.tenant, "m105-qstash-ingress");
        try fixtures.seedWorkspaceWithTenant(conn, ids.workspace, ids.tenant);
        try fixtures.seedFleet(conn, ids.fleet, ids.workspace, "m105-qstash", "{}", "");
        try seedSchedule(h, ids);
        const credentials = try testing.allocator.create(Credentials);
        errdefer testing.allocator.destroy(credentials);
        credentials.* = try testCredentials(testing.allocator);
        errdefer credentials.deinit(testing.allocator);
        h.ctx.qstash_credentials = credentials;
        h.ctx.api_max_in_flight_requests = 128;
        try cleanupRedis(h, ids.fleet);
        return .{ .harness = h, .conn = conn, .credentials = credentials, .ids = ids };
    }

    fn deinit(self: *Setup) void {
        cleanupRedis(self.harness, self.ids.fleet) catch |err| std.debug.panic("cleanupRedis failed: {}", .{err});
        cleanupDb(self.conn, self.ids) catch |err| std.debug.panic("cleanupDb failed: {}", .{err});
        self.harness.releaseConn(self.conn);
        self.harness.deinit();
        self.credentials.deinit(testing.allocator);
        testing.allocator.destroy(self.credentials);
        self.* = undefined;
    }
};

fn noopRegistry(_: *auth_mw.MiddlewareRegistry, _: *TestHarness) anyerror!void {}

fn testCredentials(alloc: std.mem.Allocator) !Credentials {
    const token = try alloc.dupe(u8, "m105-qstash-token");
    errdefer alloc.free(token);
    const current = try alloc.dupe(u8, CURRENT_KEY);
    errdefer alloc.free(current);
    const next = try alloc.dupe(u8, NEXT_KEY);
    errdefer alloc.free(next);
    return .{
        .token = token,
        .current_signing_key = current,
        .next_signing_key = next,
        .url = try alloc.dupe(u8, "https://qstash.test"),
    };
}

fn seedSchedule(h: *TestHarness, ids: Ids) !void {
    const store = Store.init(h.pool);
    var created = switch (try store.create(testing.allocator, .{
        .fleet_id = ids.fleet,
        .source = .api,
        .source_key = ids.schedule,
        .cron = "0 9 * * *",
        .timezone = "Asia/Kolkata",
        .message = "summarize today's Zoho Sprints",
    }, ids.schedule, "0195b4ba-8d3a-7f13-8abc-105000000499", 100, 200)) {
        .created => |schedule| schedule,
        else => return error.ScheduleSeedFailed,
    };
    defer created.deinit(testing.allocator);
    var synced = (try store.finalizeSuccess(testing.allocator, ids.schedule, GENERATION, "0195b4ba-8d3a-7f13-8abc-105000000499", 101)) orelse
        return error.ScheduleFinalizeFailed;
    synced.deinit(testing.allocator);
}

fn cleanupDb(conn: *pg.Conn, ids: Ids) !void {
    _ = try conn.exec("DELETE FROM core.fleet_schedules WHERE uid = $1::uuid", .{ids.schedule});
    fixtures.teardownFleets(conn, ids.workspace);
    fixtures.teardownWorkspace(conn, ids.workspace);
    fixtures.teardownTenantById(conn, ids.tenant);
}

fn cleanupRedis(h: *TestHarness, fleet_id: []const u8) !void {
    var pattern_buffer: [160]u8 = undefined;
    const pattern = try std.fmt.bufPrint(&pattern_buffer, "cron:dedup:{s}:*", .{fleet_id});
    var found = try h.queue.command(&.{ "KEYS", pattern });
    defer found.deinit(h.queue.alloc);
    if (found == .array) if (found.array) |keys| {
        for (keys) |key_value| {
            const key = switch (key_value) {
                .bulk => |value| value orelse continue,
                else => continue,
            };
            try h.queue.del(key);
        }
    };
    var stream_buffer: [128]u8 = undefined;
    const stream = try streamKey(&stream_buffer, fleet_id);
    try h.queue.del(stream);
}

fn streamKey(buffer: *[128]u8, fleet_id: []const u8) ![]const u8 {
    return std.fmt.bufPrint(buffer, "fleet:{s}:events", .{fleet_id});
}

fn streamLen(h: *TestHarness, fleet_id: []const u8) !i64 {
    var buffer: [128]u8 = undefined;
    const stream = try streamKey(&buffer, fleet_id);
    var response = try h.queue.command(&.{ "XLEN", stream });
    defer response.deinit(h.queue.alloc);
    return switch (response) {
        .integer => |value| value,
        else => error.UnexpectedRedisResponse,
    };
}

fn fieldValue(fields: []const @import("../../../queue/redis_protocol.zig").RespValue, name: []const u8) ![]const u8 {
    var index: usize = 0;
    while (index + 1 < fields.len) : (index += 2) {
        const key = fields[index].bulk orelse continue;
        if (std.mem.eql(u8, key, name)) return fields[index + 1].bulk orelse error.UnexpectedRedisResponse;
    }
    return error.RedisFieldMissing;
}

fn expectCronEvent(h: *TestHarness, ids: Ids) !void {
    var buffer: [128]u8 = undefined;
    const stream = try streamKey(&buffer, ids.fleet);
    var response = try h.queue.command(&.{ "XRANGE", stream, "-", "+", "COUNT", "1" });
    defer response.deinit(h.queue.alloc);
    const entries = response.array orelse return error.UnexpectedRedisResponse;
    try testing.expectEqual(@as(usize, 1), entries.len);
    const entry = entries[0].array orelse return error.UnexpectedRedisResponse;
    if (entry.len != 2) return error.UnexpectedRedisResponse;
    const fields = entry[1].array orelse return error.UnexpectedRedisResponse;
    try testing.expectEqualStrings("cron", try fieldValue(fields, "type"));
    var actor_buffer: [64]u8 = undefined;
    const actor = try std.fmt.bufPrint(&actor_buffer, "cron:{s}", .{ids.schedule});
    try testing.expectEqualStrings(actor, try fieldValue(fields, "actor"));
    try testing.expectEqualStrings(ids.workspace, try fieldValue(fields, "workspace_id"));
    const request_json = try fieldValue(fields, "request");
    var parsed = try std.json.parseFromSlice(std.json.Value, testing.allocator, request_json, .{});
    defer parsed.deinit();
    const object = parsed.value.object;
    try testing.expectEqualStrings("summarize today's Zoho Sprints", object.get("message").?.string);
    try testing.expectEqualStrings(ids.schedule, object.get("schedule_id").?.string);
    try testing.expectEqual(GENERATION, object.get("generation").?.integer);
    try testing.expect(object.get("fired_at").?.integer > 0);
}

fn signBody(alloc: std.mem.Allocator, body: []const u8, message_id: []const u8) ![]u8 {
    const header = try encode(alloc, "{\"alg\":\"HS256\",\"typ\":\"JWT\"}");
    defer alloc.free(header);
    var digest: [std.crypto.hash.sha2.Sha256.digest_length]u8 = undefined;
    std.crypto.hash.sha2.Sha256.hash(body, &digest, .{});
    const hash = try encode(alloc, &digest);
    defer alloc.free(hash);
    const claims = try std.json.Stringify.valueAlloc(alloc, .{
        .iss = "Upstash",
        .sub = "http://127.0.0.1" ++ cron_constants.ingress_path,
        .exp = common.clock.nowSeconds() + 60,
        .nbf = common.clock.nowSeconds() - 1,
        .iat = common.clock.nowSeconds() - 1,
        .jti = message_id,
        .body = hash,
    }, .{});
    defer alloc.free(claims);
    const payload = try encode(alloc, claims);
    defer alloc.free(payload);
    const mac = hmac_sig.computeMac(CURRENT_KEY, &.{ header, ".", payload });
    const signature = try encode(alloc, &mac);
    defer alloc.free(signature);
    return std.fmt.allocPrint(alloc, "{s}.{s}.{s}", .{ header, payload, signature });
}

fn encode(alloc: std.mem.Allocator, raw: []const u8) ![]u8 {
    const encoded = try alloc.alloc(u8, std.base64.url_safe_no_pad.Encoder.calcSize(raw.len));
    _ = std.base64.url_safe_no_pad.Encoder.encode(encoded, raw);
    return encoded;
}

fn postFire(h: *TestHarness, schedule_id: []const u8, generation: i64, message_id: []const u8) !harness_mod.Response {
    const body = try std.fmt.allocPrint(testing.allocator, "{{\"schedule_id\":\"{s}\",\"generation\":{d}}}", .{ schedule_id, generation });
    defer testing.allocator.free(body);
    const signature = try signBody(testing.allocator, body, message_id);
    defer testing.allocator.free(signature);
    var destination_buffer: [cron_constants.max_destination_url_bytes]u8 = undefined;
    const destination = try cron_constants.destinationUrl(&destination_buffer, h.ctx.api_url);
    const verifier = QStashVerifier.init(destination, CURRENT_KEY, NEXT_KEY);
    var verified = try verifier.verify(testing.allocator, signature, body);
    defer verified.deinit(testing.allocator);
    var request = h.post(PATH);
    request = try request.header(cron_constants.signature_header, signature);
    request = try request.header(cron_constants.schedule_id_header, schedule_id);
    request = try request.header(cron_constants.message_id_header, message_id);
    return request.rawBody(body).send();
}

test "test_fire_enqueues_cron_event" {
    var setup = try Setup.init(0);
    defer setup.deinit();
    const response = try postFire(setup.harness, setup.ids.schedule, GENERATION, "jwt-fire-enqueue");
    defer response.deinit();
    try response.expectStatus(.ok);
    try testing.expect(response.bodyContains("\"accepted\":true"));
    try testing.expectEqual(@as(i64, 1), try streamLen(setup.harness, setup.ids.fleet));
    try expectCronEvent(setup.harness, setup.ids);
}

test "test_fire_requires_qstash_credentials" {
    const h = try TestHarness.start(testing.allocator, .{ .configureRegistry = noopRegistry });
    defer h.deinit();
    const response = try h.post(PATH).rawBody("{}").send();
    defer response.deinit();
    try response.expectStatus(.service_unavailable);
    try response.expectErrorCode(error_codes.ERR_SCHEDULE_NOT_CONFIGURED);
}

test "test_fire_rejects_bad_signature" {
    var setup = try Setup.init(1);
    defer setup.deinit();
    var request = setup.harness.post(PATH);
    request = try request.header(cron_constants.signature_header, "not-a-valid-token");
    const response = try request.rawBody("{}").send();
    defer response.deinit();
    try response.expectStatus(.unauthorized);
    try response.expectErrorCode(error_codes.ERR_SCHEDULE_SIGNATURE_INVALID);
    try testing.expectEqual(@as(i64, 0), try streamLen(setup.harness, setup.ids.fleet));
    const missing = try setup.harness.post(PATH).rawBody("{}").send();
    defer missing.deinit();
    try missing.expectStatus(.unauthorized);
    try missing.expectErrorCode(error_codes.ERR_SCHEDULE_SIGNATURE_INVALID);
}

test "test_fire_dedupes_replay" {
    var setup = try Setup.init(2);
    defer setup.deinit();
    const first = try postFire(setup.harness, setup.ids.schedule, GENERATION, "jwt-fire-replay");
    defer first.deinit();
    try first.expectStatus(.ok);
    const replay = try postFire(setup.harness, setup.ids.schedule, GENERATION, "jwt-fire-replay");
    defer replay.deinit();
    try replay.expectStatus(.ok);
    try testing.expectEqual(@as(i64, 1), try streamLen(setup.harness, setup.ids.fleet));
}

test "test_fire_skips_inactive" {
    var setup = try Setup.init(3);
    defer setup.deinit();
    const missing_id = "0195b4ba-8d3a-7f13-8abc-105000000498";
    const missing = try postFire(setup.harness, missing_id, GENERATION, "missing");
    defer missing.deinit();
    try missing.expectStatus(.ok);
    const states = [_]struct { sql: []const u8, id: []const u8 }{
        .{ .sql = "UPDATE core.fleet_schedules SET desired_status = 'paused' WHERE uid = $1::uuid", .id = "paused" },
        .{ .sql = "UPDATE core.fleet_schedules SET desired_status = 'deleting' WHERE uid = $1::uuid", .id = "deleting" },
        .{ .sql = "UPDATE core.fleet_schedules SET desired_status = 'active', sync_status = 'failed' WHERE uid = $1::uuid", .id = "failed" },
        .{ .sql = "UPDATE core.fleets SET status = 'stopped' WHERE id = $1::uuid", .id = "stopped" },
        .{ .sql = "UPDATE core.fleets SET status = 'killed' WHERE id = $1::uuid", .id = "killed" },
    };
    for (states) |state| {
        const target_id = if (std.mem.indexOf(u8, state.sql, "fleet_schedules") != null) setup.ids.schedule else setup.ids.fleet;
        _ = try setup.conn.exec(state.sql, .{target_id});
        const response = try postFire(setup.harness, setup.ids.schedule, GENERATION, state.id);
        defer response.deinit();
        try response.expectStatus(.ok);
    }
    try testing.expectEqual(@as(i64, 0), try streamLen(setup.harness, setup.ids.fleet));
}

test "test_fire_100_way_exactly_once" {
    var setup = try Setup.init(4);
    defer setup.deinit();
    var threads: [CONTENDERS]std.Thread = undefined;
    var statuses: [CONTENDERS]u16 = .{0} ** CONTENDERS;
    var ready = std.atomic.Value(u32).init(0);
    var gate = std.atomic.Value(bool).init(false);
    var peak = std.atomic.Value(u32).init(0);
    setup.harness.ctx.api_peak_in_flight_probe = &peak;
    defer setup.harness.ctx.api_peak_in_flight_probe = null;
    const Worker = struct {
        fn run(h: *TestHarness, schedule_id: []const u8, status: *u16, ready_count: *std.atomic.Value(u32), start_gate: *std.atomic.Value(bool)) void {
            _ = ready_count.fetchAdd(1, .acq_rel);
            while (!start_gate.load(.acquire)) std.atomic.spinLoopHint();
            const response = postFire(h, schedule_id, GENERATION, "jwt-fire-concurrent") catch return;
            defer response.deinit();
            status.* = response.status;
        }
    };
    var spawned: usize = 0;
    errdefer {
        gate.store(true, .release);
        for (threads[0..spawned]) |thread| thread.join();
    }
    for (&threads, 0..) |*thread, index| {
        thread.* = try std.Thread.spawn(.{}, Worker.run, .{ setup.harness, setup.ids.schedule, &statuses[index], &ready, &gate });
        spawned += 1;
    }
    while (ready.load(.acquire) != CONTENDERS) std.atomic.spinLoopHint();
    gate.store(true, .release);
    for (threads) |thread| thread.join();
    spawned = 0;
    for (statuses) |status| try testing.expectEqual(@as(u16, 200), status);
    try testing.expect(peak.load(.acquire) >= 2);
    try testing.expectEqual(@as(i64, 1), try streamLen(setup.harness, setup.ids.fleet));
}
