// Integration tests — M106 §4 outbound answer delivery (plumbing half; the
// mention→LLM→answer behavioural half is a staging eval, per the locked
// decision — the harness runs no runner). A capturing FakeSlack stands in for
// the Slack Web API: it reads the chat.postMessage body (so we assert the answer
// went to the right channel+thread) and answers `{ok:true}`.
//
// Covers Dim 4.1 (the answer is posted in-thread) at two levels: slack_post.deliver
// directly, and end-to-end through connector_outbound.enqueue → the worker.
//
// Requires TEST_DATABASE_URL + REDIS_URL_API — skipped gracefully otherwise.

const std = @import("std");
const common = @import("common");
const pg = @import("pg");
const auth_mw = @import("../../../../auth/middleware/mod.zig");
const harness_mod = @import("../../../test_harness.zig");
const test_port = @import("../../../test_port.zig");
const test_fixtures = @import("../../../../db/test_fixtures.zig");
const connector_outbound = @import("../../../../queue/connector_outbound.zig");
const spec = @import("spec.zig");
const call_deadline = @import("call_deadline");
const post = @import("post.zig");
const worker = @import("../outbound/worker.zig");

const TestHarness = harness_mod.TestHarness;
const net = std.Io.net;
const testing = std.testing;

const TENANT_ID = "0195c106-4000-7000-8000-f00000000042"; // per-suite tenant — keeps this suite's workspace off the shared tenant's FK chain
const TENANT_NAME = "slack-outbound-suite";
const WS = "0195c106-4002-7000-8000-000000000042";
const FLEET_ID = "0195c106-4003-7000-8000-000000000043";
const FLEET_NAME = "slack-channel-t106out-c106out";
// v7-shaped uid (position-15 nibble '7') — core.fleet_events has a uuidv7 CHECK.
const EVENT_UID = "0195c106-4004-7000-8000-000000000044";
const EVENT_ID = "1700000009000-0";
const CHANNEL = "C106OUT";
const THREAD_TS = "1700000009.000100";
const ANSWER = "Aurora is healthy — no alerts in 24h.";
const BOT_TOKEN = "xoxb-m106-out-tok";
const REQUEST_JSON =
    "{\"text\":\"<@U0BOT> is aurora healthy?\",\"reply_thread_ts\":\"" ++ THREAD_TS ++
    "\",\"channel_id\":\"" ++ CHANNEL ++ "\",\"recent_thread_msgs\":[]}";

// ── Capturing FakeSlack ──────────────────────────────────────────────────────
// Reads the request body into a buffer + answers {ok:true}. Single-writer (one
// connection per delivery), so a release-store of `captured_len` after the buffer
// write + an acquire-load on read gives the caller the fully-written capture.
const FakeSlack = struct {
    server: net.Server,
    port: u16,
    accept_thread: std.Thread,
    stop: std.atomic.Value(bool),
    captured: [4096]u8,
    captured_len: std.atomic.Value(usize),

    fn start(self: *FakeSlack) !void {
        const io = common.globalIo();
        const lp = try test_port.listenLoopback(io);
        self.server = lp.server;
        self.port = lp.port;
        self.stop = std.atomic.Value(bool).init(false);
        self.captured_len = std.atomic.Value(usize).init(0);
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

    /// Copy the last captured request body out. Acquire-load pairs with the
    /// handler's release-store so the buffer bytes are visible.
    fn capturedBody(self: *FakeSlack, out: []u8) []const u8 {
        const len = self.captured_len.load(.acquire);
        const n = @min(out.len, len);
        @memcpy(out[0..n], self.captured[0..n]);
        return out[0..n];
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

        // Capture the request body (the chat.postMessage JSON) into the buffer,
        // then release-store the length so a reader sees the completed write.
        var body_buf: [4096]u8 = undefined;
        const body_reader = req.readerExpectNone(&body_buf);
        var total: usize = 0;
        while (total < self.captured.len) {
            const n = body_reader.readSliceShort(self.captured[total..]) catch break;
            if (n == 0) break;
            total += n;
        }
        self.captured_len.store(total, .release);

        req.respond("{\"ok\":true,\"ts\":\"1700000009.000200\"}", .{
            .keep_alive = false,
            .extra_headers = &.{.{ .name = "content-type", .value = "application/json" }},
        }) catch return;
    }
};

fn noopRegistry(_: *auth_mw.MiddlewareRegistry, _: *TestHarness) anyerror!void {}

// ── Fixtures ─────────────────────────────────────────────────────────────────

fn seedFleetRow(conn: *pg.Conn) !void {
    const now = common.clock.nowMillis();
    _ = try conn.exec(
        \\INSERT INTO core.fleets
        \\  (id, workspace_id, name, source_markdown, trigger_markdown, config_json,
        \\   status, created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, $3, '# skill', '# trigger', '{}'::jsonb, 'active', $4, $4)
        \\ON CONFLICT (id) DO UPDATE SET status = 'active', updated_at = EXCLUDED.updated_at
    , .{ FLEET_ID, WS, FLEET_NAME, now });
}

fn seedEventRow(conn: *pg.Conn) !void {
    const now = common.clock.nowMillis();
    _ = try conn.exec(
        \\INSERT INTO core.fleet_events
        \\  (uid, fleet_id, event_id, workspace_id, actor, event_type, status,
        \\   request_json, created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, $3, $4::uuid, 'slack:U1', 'chat', 'received',
        \\        $5::jsonb, $6, $6)
        \\ON CONFLICT DO NOTHING
    , .{ EVENT_UID, FLEET_ID, EVENT_ID, WS, REQUEST_JSON, now });
}

/// Vault the per-install bot token under the (WS, fleet:slack) handle callback.zig
/// writes — the shape post.zig's loadBotToken reads.
fn seedBotToken(alloc: std.mem.Allocator, conn: *pg.Conn) !void {
    var obj: std.json.ObjectMap = .empty;
    defer obj.deinit(alloc);
    try obj.put(alloc, "integration", .{ .string = spec.PROVIDER });
    try obj.put(alloc, "bot_token", .{ .string = BOT_TOKEN });
    try obj.put(alloc, "bot_user_id", .{ .string = "U0BOT" });
    try obj.put(alloc, "team_id", .{ .string = "T106OUT" });
    try test_fixtures.storeVaultJson(alloc, conn, WS, spec.PROVIDER, .{ .object = obj });
}

fn preClean(conn: *pg.Conn) void {
    _ = conn.exec("DELETE FROM core.fleet_events WHERE fleet_id = $1::uuid", .{FLEET_ID}) catch |e| std.log.warn("preclean events: {s}", .{@errorName(e)});
    _ = conn.exec("DELETE FROM core.connector_channels WHERE fleet_id = $1::uuid", .{FLEET_ID}) catch |e| std.log.warn("preclean channels: {s}", .{@errorName(e)});
    _ = conn.exec("DELETE FROM core.fleets WHERE id = $1::uuid", .{FLEET_ID}) catch |e| std.log.warn("preclean fleet: {s}", .{@errorName(e)});
}

fn seedAll(alloc: std.mem.Allocator, conn: *pg.Conn) !void {
    test_fixtures.setTestEncryptionKey();
    try test_fixtures.seedTenantById(conn, TENANT_ID, TENANT_NAME);
    try test_fixtures.seedWorkspaceWithTenant(conn, WS, TENANT_ID);
    preClean(conn);
    try seedFleetRow(conn);
    try seedEventRow(conn);
    try seedBotToken(alloc, conn);
}

fn startHarness(alloc: std.mem.Allocator) !*TestHarness {
    return TestHarness.start(alloc, .{ .configureRegistry = noopRegistry }) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
}

// ── Tests ────────────────────────────────────────────────────────────────────

test "integration: slack_post.deliver posts the answer to the mention's thread (Dim 4.1)" {
    const alloc = testing.allocator;
    const h = try startHarness(alloc);
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try seedAll(alloc, conn);
    defer preClean(conn); // don't leak the seeded active fleet into other suites' lease scans

    var fake: FakeSlack = undefined;
    try fake.start();
    defer fake.shutdown();
    var base_buf: [64]u8 = undefined;
    const base = try fake.baseUrl(&base_buf);

    // One process scheduler, as the daemon root owns.
    var backend: call_deadline.MonotonicBackend = .{};
    var sched = call_deadline.ProcessScheduler.init(alloc, &backend);
    try sched.start();
    defer sched.deinit();
    const verdict = post.deliver(alloc, common.globalIo(), &sched, h.pool, base, WS, FLEET_ID, EVENT_ID, ANSWER);
    try testing.expectEqual(post.Outcome.delivered, verdict);

    // The captured chat.postMessage body carries the originating channel + thread
    // + the model's answer — the reply is threaded to the mention (Dim 4.1).
    var body_buf: [4096]u8 = undefined;
    const body = fake.capturedBody(&body_buf);
    try testing.expect(std.mem.indexOf(u8, body, CHANNEL) != null);
    try testing.expect(std.mem.indexOf(u8, body, THREAD_TS) != null);
    try testing.expect(std.mem.indexOf(u8, body, "Aurora is healthy") != null);
}

test "integration: enqueue → worker delivers the answer end-to-end + acks (Dim 4.1)" {
    const alloc = testing.allocator;
    const h = try startHarness(alloc);
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try seedAll(alloc, conn);
    defer preClean(conn); // don't leak the seeded active fleet into other suites' lease scans
    try connector_outbound.ensureGroup(&h.queue);

    var fake: FakeSlack = undefined;
    try fake.start();
    defer fake.shutdown();
    var base_buf: [64]u8 = undefined;
    const base = try fake.baseUrl(&base_buf);

    // Enqueue a job the way service_report.finalize does.
    const entry = try connector_outbound.enqueue(&h.queue, .{
        .provider = spec.PROVIDER,
        .workspace_id = WS,
        .fleet_id = FLEET_ID,
        .event_id = EVENT_ID,
        .answer = ANSWER,
    });
    defer alloc.free(entry);

    // Run the real worker until it drains the job (bounded wait), then stop it.
    var shutdown = std.atomic.Value(bool).init(false);
    const t = try std.Thread.spawn(.{}, worker.run, .{ h.pool, &h.queue, alloc, &shutdown, base, &h.deadline_scheduler });
    defer {
        shutdown.store(true, .release);
        t.join();
    }

    // Poll for the FakeSlack to capture the post (worker delivers async).
    var waited: usize = 0;
    while (waited < 200) : (waited += 1) {
        var body_buf: [4096]u8 = undefined;
        if (fake.capturedBody(&body_buf).len > 0) break;
        common.sleepNanos(25 * std.time.ns_per_ms);
    }
    var body_buf: [4096]u8 = undefined;
    const body = fake.capturedBody(&body_buf);
    try testing.expect(std.mem.indexOf(u8, body, THREAD_TS) != null);
    try testing.expect(std.mem.indexOf(u8, body, "Aurora is healthy") != null);
}
