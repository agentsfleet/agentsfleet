// Integration test — M106 §4 E + Dim 4.3: the ingress recent-thread re-read.
//
// Drives a signed app_mention through POST /v1/connectors/slack/events with a
// loopback FakeSlack standing in for the Slack Web API's conversations.replies.
// Proves two halves at once:
//   • §4 E — the re-read reaches the enqueued event's request_json: the stream
//     entry's recent_thread_msgs[] carries the thread messages FakeSlack served.
//   • Dim 4.3 — thread context is transient: nothing from the re-read lands in
//     memory.memory_entries (ingress writes zero memory rows; durable capture is
//     the runner's job, and the harness runs no runner).
//
// Requires TEST_DATABASE_URL + REDIS_URL_API — skipped gracefully otherwise.

const std = @import("std");
const common = @import("common");
const pg = @import("pg");
const auth_mw = @import("../../../../auth/middleware/mod.zig");
const harness_mod = @import("../../../test_harness.zig");
const test_port = @import("../../../test_port.zig");
const test_fixtures = @import("../../../../db/test_fixtures.zig");
const id_format = @import("../../../../types/id_format.zig");
const redis_fleet = @import("../../../../queue/redis_fleet.zig");
const ec = @import("../../../../errors/error_registry.zig");
const hs = @import("hmac_sig");
const slack_sig = @import("slack_sig.zig");
const spec = @import("spec.zig");

const TestHarness = harness_mod.TestHarness;
const net = std.Io.net;
const testing = std.testing;

const ADMIN_WS = "0195c106-5001-7000-8000-000000000051";
const TARGET_WS = "0195c106-5002-7000-8000-000000000052";
const SIGNING_SECRET = "m106-thread-signing-secret-key!!"; // 32 bytes
const TEAM_ID = "T106THR";
const CHANNEL_ID = "C106THR";
const RESIDENT_NAME = "slack-channel-t106thr-c106thr"; // "slack-channel-" ++ lower(team) ++ "-" ++ lower(channel)
const USER_ID = "U778";
const THREAD_TS = "1700000100.000100";
const MENTION_TS = "1700000100.000200";
const MENTION_TS_429 = "1700000100.000300";
const BOT_TOKEN = "xoxb-m106-thread-tok";
// A distinctive phrase in the FakeSlack thread so the request_json assertion can
// not pass by accident (it is not present anywhere in the mention text).
const THREAD_PHRASE = "prod is called aurora-thread";
const EVENTS_PATH = "/v1/connectors/slack/events";
const TEST_CONSUMER = "m106-thread-test-consumer";

// ── FakeSlack: serves conversations.replies with a two-message thread ─────────
const FakeSlack = struct {
    server: net.Server,
    port: u16,
    accept_thread: std.Thread,
    stop: std.atomic.Value(bool),
    /// HTTP status the fake answers with (200 by default). A test sets this to
    /// 429/5xx to exercise the best-effort degrade path (the re-read must
    /// degrade to an empty thread, never crash the ingress — regression guard
    /// for the errdefer/manual double-deinit bug).
    reply_status: std.atomic.Value(u16),

    const REPLIES_BODY =
        "{\"ok\":true,\"messages\":[" ++
        "{\"type\":\"message\",\"user\":\"" ++ USER_ID ++ "\",\"ts\":\"" ++ THREAD_TS ++ "\",\"text\":\"" ++ THREAD_PHRASE ++ "\"}," ++
        "{\"type\":\"message\",\"user\":\"U779\",\"ts\":\"1700000100.000150\",\"text\":\"got it, thanks\"}" ++
        "]}";

    fn start(self: *FakeSlack) !void {
        const io = common.globalIo();
        const lp = try test_port.listenLoopback(io);
        self.server = lp.server;
        self.port = lp.port;
        self.stop = std.atomic.Value(bool).init(false);
        self.reply_status = std.atomic.Value(u16).init(200);
        self.accept_thread = try std.Thread.spawn(.{}, acceptLoop, .{self});
    }

    fn shutdown(self: *FakeSlack) void {
        const io = common.globalIo();
        self.stop.store(true, .release);
        var addr = net.IpAddress.parseIp4("127.0.0.1", self.port) catch return;
        if (addr.connect(io, .{ .mode = .stream })) |s| s.close(io) else |_| {}
        self.accept_thread.join();
        self.server.deinit(io);
    }

    fn baseUrl(self: *FakeSlack, buf: []u8) ![]const u8 {
        return std.fmt.bufPrint(buf, "http://127.0.0.1:{d}/api", .{self.port});
    }

    fn acceptLoop(self: *FakeSlack) void {
        const io = common.globalIo();
        while (!self.stop.load(.acquire)) {
            const stream = self.server.accept(io) catch return;
            if (self.stop.load(.acquire)) {
                stream.close(io);
                return;
            }
            const t = std.Thread.spawn(.{}, handleConn, .{ self, stream }) catch {
                stream.close(io);
                continue;
            };
            t.detach();
        }
    }

    fn handleConn(self: *FakeSlack, stream: net.Stream) void {
        const io = common.globalIo();
        defer stream.close(io);
        var read_buf: [4096]u8 = undefined;
        var sreader = stream.reader(io, &read_buf);
        var write_buf: [4096]u8 = undefined;
        var swriter = stream.writer(io, &write_buf);
        var http_server = std.http.Server.init(&sreader.interface, &swriter.interface);
        var req = http_server.receiveHead() catch return;
        const status = self.reply_status.load(.acquire);
        // Non-200 → a short error body (Slack sends `retry_after` etc.); the
        // re-read must classify it and degrade to an empty thread.
        const body = if (status == 200) REPLIES_BODY else "{\"ok\":false,\"error\":\"ratelimited\"}";
        req.respond(body, .{
            .status = @enumFromInt(status),
            .keep_alive = false,
            .extra_headers = &.{.{ .name = "content-type", .value = "application/json" }},
        }) catch return;
    }
};

fn noopRegistry(_: *auth_mw.MiddlewareRegistry, _: *TestHarness) anyerror!void {}

// ── Fixtures ─────────────────────────────────────────────────────────────────

fn seedSlackApp(alloc: std.mem.Allocator, conn: *pg.Conn) !void {
    var obj: std.json.ObjectMap = .empty;
    defer obj.deinit(alloc);
    try obj.put(alloc, "client_id", .{ .string = "test-client-id" });
    try obj.put(alloc, "client_secret", .{ .string = "test-client-secret" });
    try obj.put(alloc, "signing_secret", .{ .string = SIGNING_SECRET });
    try test_fixtures.storeVaultJson(alloc, conn, ADMIN_WS, "slack-app", .{ .object = obj });
}

fn seedBotToken(alloc: std.mem.Allocator, conn: *pg.Conn) !void {
    var obj: std.json.ObjectMap = .empty;
    defer obj.deinit(alloc);
    try obj.put(alloc, "integration", .{ .string = spec.PROVIDER });
    try obj.put(alloc, "bot_token", .{ .string = BOT_TOKEN });
    try test_fixtures.storeVaultJson(alloc, conn, TARGET_WS, "fleet:slack", .{ .object = obj });
}

fn seedInstall(alloc: std.mem.Allocator, conn: *pg.Conn) !void {
    const uid = try id_format.generateConnectorInstallId(alloc);
    defer alloc.free(uid);
    const scopes: []const []const u8 = &.{ "app_mentions:read", "chat:write", "channels:history" };
    _ = try conn.exec(
        \\INSERT INTO core.connector_installs
        \\  (uid, provider, external_account_id, workspace_id, installed_by, scopes, created_at, updated_at)
        \\VALUES ($1::uuid, $2, $3, $4::uuid, $5, $6::text[], $7, $7)
        \\ON CONFLICT (provider, external_account_id) DO UPDATE SET workspace_id = EXCLUDED.workspace_id
    , .{ uid, spec.PROVIDER, TEAM_ID, TARGET_WS, "UADMIN", scopes, common.clock.nowMillis() });
}

fn preClean(conn: *pg.Conn) void {
    _ = conn.exec("DELETE FROM core.connector_channels WHERE provider = $1 AND external_account_id = $2", .{ spec.PROVIDER, TEAM_ID }) catch |e| std.log.warn("preclean channels: {s}", .{@errorName(e)});
    _ = conn.exec("DELETE FROM core.fleets WHERE workspace_id = $1::uuid AND name = $2", .{ TARGET_WS, RESIDENT_NAME }) catch |e| std.log.warn("preclean fleet: {s}", .{@errorName(e)});
    _ = conn.exec("DELETE FROM core.connector_installs WHERE provider = $1 AND external_account_id = $2", .{ spec.PROVIDER, TEAM_ID }) catch |e| std.log.warn("preclean install: {s}", .{@errorName(e)});
}

/// Post-test teardown, keyed by fleet_id so it cleans whichever channel fleet a
/// test materialized. The mention MATERIALIZES an active resident fleet with a
/// pending event on its Redis stream; without this, that leftover active fleet
/// leaks into other suites' lease scans (e.g. control_plane's "assigns across
/// active fleets" asserts an exact fleet count). Drops the fleet_events + the
/// Redis stream (which also removes the consumer group + PEL) + the binding +
/// the fleet row. The shared install is re-seeded per test, so it is left alone.
fn teardownMaterialized(h: *TestHarness, conn: *pg.Conn, fleet_id: []const u8) void {
    _ = conn.exec("DELETE FROM core.fleet_events WHERE fleet_id = $1::uuid", .{fleet_id}) catch |e| std.log.warn("teardown events: {s}", .{@errorName(e)});
    var key_buf: [128]u8 = undefined;
    if (std.fmt.bufPrint(&key_buf, "fleet:{s}:events", .{fleet_id})) |stream_key| {
        h.queue.del(stream_key) catch |e| std.log.warn("teardown stream: {s}", .{@errorName(e)});
    } else |e| std.log.warn("teardown stream key: {s}", .{@errorName(e)});
    _ = conn.exec("DELETE FROM core.connector_channels WHERE fleet_id = $1::uuid", .{fleet_id}) catch |e| std.log.warn("teardown binding: {s}", .{@errorName(e)});
    _ = conn.exec("DELETE FROM core.fleets WHERE id = $1::uuid", .{fleet_id}) catch |e| std.log.warn("teardown fleet: {s}", .{@errorName(e)});
}

fn mentionBody(alloc: std.mem.Allocator, mention_ts: []const u8) ![]const u8 {
    return std.fmt.allocPrint(
        alloc,
        "{{\"type\":\"event_callback\",\"team_id\":\"{s}\"," ++
            "\"event\":{{\"type\":\"app_mention\",\"channel\":\"{s}\",\"user\":\"{s}\"," ++
            "\"text\":\"<@U0BOT> what is prod called?\",\"ts\":\"{s}\",\"thread_ts\":\"{s}\"}}}}",
        .{ TEAM_ID, CHANNEL_ID, USER_ID, mention_ts, THREAD_TS },
    );
}

fn postSigned(h: *TestHarness, now_s: i64, body: []const u8) !harness_mod.Response {
    var ts_buf: [24]u8 = undefined;
    const ts = try std.fmt.bufPrint(&ts_buf, "{d}", .{now_s});
    const mac = hs.computeMac(SIGNING_SECRET, &.{ slack_sig.CONFIG.hmac_version, ":", ts, ":", body });
    var sig_buf: [slack_sig.CONFIG.prefix.len + hs.MAC_LEN * 2]u8 = undefined;
    const sig = hs.encodeMacHex(&sig_buf, slack_sig.CONFIG.prefix, mac);
    var rq = h.post(EVENTS_PATH);
    rq = try rq.header(ec.SLACK_SIG_HEADER, sig);
    rq = try rq.header(ec.SLACK_TS_HEADER, ts);
    rq = rq.rawBody(body);
    return rq.send();
}

fn boundFleetId(alloc: std.mem.Allocator, conn: *pg.Conn) ![]const u8 {
    const PgQuery = @import("../../../../db/pg_query.zig").PgQuery;
    var q = PgQuery.from(try conn.query(
        "SELECT fleet_id::text FROM core.connector_channels WHERE provider = $1 AND external_account_id = $2 AND external_channel_id = $3",
        .{ spec.PROVIDER, TEAM_ID, CHANNEL_ID },
    ));
    defer q.deinit();
    const row = try q.next() orelse return error.NoBinding;
    return alloc.dupe(u8, try row.get([]const u8, 0));
}

fn resetRole(conn: *pg.Conn) void {
    _ = conn.exec("RESET ROLE", .{}) catch |err| std.log.warn("reset role: {s}", .{@errorName(err)});
}

fn memoryRowCount(conn: *pg.Conn, fleet_id: []const u8) !i64 {
    const PgQuery = @import("../../../../db/pg_query.zig").PgQuery;
    _ = try conn.exec("SET ROLE memory_runtime", .{});
    defer resetRole(conn);
    var q = PgQuery.from(try conn.query("SELECT count(*) FROM memory.memory_entries WHERE fleet_id = $1::uuid", .{fleet_id}));
    defer q.deinit();
    const row = try q.next() orelse return 0;
    return row.get(i64, 0);
}

fn startHarness(alloc: std.mem.Allocator) !*TestHarness {
    return TestHarness.start(alloc, .{ .configureRegistry = noopRegistry }) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
}

// ── Test ─────────────────────────────────────────────────────────────────────

test "integration: ingress re-reads the thread into request_json, stores nothing in memory (§4 E / Dim 4.3)" {
    const alloc = testing.allocator;
    const h = try startHarness(alloc);
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);

    test_fixtures.setTestEncryptionKey();
    try test_fixtures.seedTenant(conn);
    try test_fixtures.seedWorkspace(conn, ADMIN_WS);
    try test_fixtures.seedWorkspace(conn, TARGET_WS);
    preClean(conn);
    try seedSlackApp(alloc, conn);
    try seedBotToken(alloc, conn);
    try seedInstall(alloc, conn);
    h.ctx.platform_admin_workspace_id = ADMIN_WS;

    var fake: FakeSlack = undefined;
    try fake.start();
    defer fake.shutdown();
    var base_buf: [64]u8 = undefined;
    h.ctx.connector_slack_api_base_override = try fake.baseUrl(&base_buf);

    const body = try mentionBody(alloc, MENTION_TS);
    defer alloc.free(body);
    const r = try postSigned(h, common.clock.nowSeconds(), body);
    defer r.deinit();
    try r.expectStatus(.ok);

    const fleet_id = try boundFleetId(alloc, conn);
    defer alloc.free(fleet_id);
    // Drop the materialized fleet + its stream so it can't leak into other
    // suites' lease scans (LIFO: runs before `alloc.free(fleet_id)` above).
    defer teardownMaterialized(h, conn, fleet_id);

    // §4 E — the enqueued event's request_json carries the re-read thread. Read
    // the stream entry back (create the group at 0 so the existing entry is
    // delivered) and assert the FakeSlack thread phrase rode into
    // recent_thread_msgs[] (it appears nowhere in the mention text).
    try redis_fleet.ensureFleetConsumerGroup(&h.queue, fleet_id);
    var ev = (try redis_fleet.xreadgroupFleetOnce(&h.queue, fleet_id, TEST_CONSUMER)) orelse return error.NoStreamEntry;
    defer ev.deinit(alloc);
    try testing.expect(std.mem.indexOf(u8, ev.request_json, "recent_thread_msgs") != null);
    try testing.expect(std.mem.indexOf(u8, ev.request_json, THREAD_PHRASE) != null);

    // Dim 4.3 — thread context is transient: ingress wrote nothing to durable
    // memory (the runner, absent here, is the only writer of memory_entries).
    try testing.expectEqual(@as(i64, 0), try memoryRowCount(conn, fleet_id));
}

test "integration: a Slack 429 on the thread re-read degrades to an empty thread + still acks (§4 E best-effort)" {
    const alloc = testing.allocator;
    const h = try startHarness(alloc);
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);

    test_fixtures.setTestEncryptionKey();
    try test_fixtures.seedTenant(conn);
    try test_fixtures.seedWorkspace(conn, ADMIN_WS);
    try test_fixtures.seedWorkspace(conn, TARGET_WS);
    preClean(conn);
    try seedSlackApp(alloc, conn);
    try seedBotToken(alloc, conn);
    try seedInstall(alloc, conn);
    h.ctx.platform_admin_workspace_id = ADMIN_WS;

    var fake: FakeSlack = undefined;
    try fake.start();
    defer fake.shutdown();
    // conversations.replies rate-limited — the pre-fix code double-freed the
    // Allocating writer here (errdefer + manual deinit) and crashed the ingress.
    fake.reply_status.store(429, .release);
    var base_buf: [64]u8 = undefined;
    h.ctx.connector_slack_api_base_override = try fake.baseUrl(&base_buf);

    const body = try mentionBody(alloc, MENTION_TS_429);
    defer alloc.free(body);
    const r = try postSigned(h, common.clock.nowSeconds(), body);
    defer r.deinit();
    try r.expectStatus(.ok); // ingress does NOT crash — it degrades + 200-acks

    const fleet_id = try boundFleetId(alloc, conn);
    defer alloc.free(fleet_id);
    defer teardownMaterialized(h, conn, fleet_id);

    // The event still enqueued, but the re-read degraded to an EMPTY thread:
    // recent_thread_msgs is `[]` and the FakeSlack phrase never made it in.
    try redis_fleet.ensureFleetConsumerGroup(&h.queue, fleet_id);
    var ev = (try redis_fleet.xreadgroupFleetOnce(&h.queue, fleet_id, TEST_CONSUMER)) orelse return error.NoStreamEntry;
    defer ev.deinit(alloc);
    try testing.expect(std.mem.indexOf(u8, ev.request_json, "\"recent_thread_msgs\":[]") != null);
    try testing.expect(std.mem.indexOf(u8, ev.request_json, THREAD_PHRASE) == null);
}
