//! Tests for the broker's production wiring (`serve_broker.zig`) — extracted so
//! the production file stays under the 350-line FLL cap. Covers the metrics
//! sink, the exchange boundary, and the deadline-armed outbound (finding ②:
//! a broker token exchange must never run unbounded — fail closed on a stall or
//! an un-armable watchdog, exactly like the connector layer's `bounded_fetch`).

const std = @import("std");
const common = @import("common");
const call_deadline = @import("call_deadline");
const serve_broker = @import("serve_broker.zig");

const testing = std.testing;
const HttpClientExchange = serve_broker.HttpClientExchange;

test "metricsSink emits without dereferencing its opaque ptr" {
    const sink = serve_broker.metricsSink();
    // ptr is undefined by contract; onMint must never touch it.
    sink.onMint(.{ .integration = "github", .outcome = "ok", .latency_ms = 12, .cache_hit = false });
}

test "exchange wires a post boundary over the client" {
    var backend: call_deadline.MonotonicBackend = .{};
    var sched = call_deadline.ProcessScheduler.init(testing.allocator, &backend);
    try sched.start();
    defer sched.deinit();
    var ex = HttpClientExchange{ .io = common.globalIo(), .sched = &sched };
    const boundary = ex.exchange();
    // The boundary points back at the exchange struct (no network here).
    try testing.expect(boundary.ptr == @as(*anyopaque, @ptrCast(&ex)));
}

test "exchange refuses an unusable URL fail-closed, never fetches unarmed (finding ②)" {
    var backend: call_deadline.MonotonicBackend = .{};
    var sched = call_deadline.ProcessScheduler.init(testing.allocator, &backend);
    try sched.start();
    defer sched.deinit();
    var ex = HttpClientExchange{ .io = common.globalIo(), .sched = &sched };
    const boundary = ex.exchange();
    // pinHandle can't parse this → the call is refused before any bytes are sent.
    const r = boundary.post(testing.allocator, .{ .url = "not a url", .body = "{}" });
    try testing.expectError(error.HttpExchangeFailed, r);
}

// A vendor that never answers: listening without accept(2) completes the TCP
// handshake via the backlog, so pin + send succeed and the read stalls — the
// exact hung-token-endpoint shape the broker deadline exists for.
const STALL_DEADLINE_MS: u31 = 250;
const ELAPSED_BOUND_MS: i64 = 2_000; // well over the deadline, well under suite timeout

fn boundPort(handle: std.Io.net.Socket.Handle) !u16 {
    // SAFETY: getsockname fills sa before sa.port is read on success.
    var sa: std.posix.sockaddr.in = undefined;
    var len: std.posix.socklen_t = @sizeOf(std.posix.sockaddr.in);
    if (std.c.getsockname(handle, @ptrCast(&sa), &len) != 0) return error.GetSockNameFailed;
    return std.mem.bigToNative(u16, sa.port);
}

test "exchange deadline fires on a stalled vendor and fails closed within the bound (finding ②)" {
    const io = common.globalIo();
    var addr = try std.Io.net.IpAddress.parseIp4("127.0.0.1", 0);
    var listener = addr.listen(io, .{ .reuse_address = true }) catch return error.SkipZigTest;
    defer listener.deinit(io);
    const port = boundPort(listener.socket.handle) catch return error.SkipZigTest;

    var url_buf: [48]u8 = undefined;
    const url = try std.fmt.bufPrint(&url_buf, "http://127.0.0.1:{d}", .{port});

    // Inject the short deadline so the fire returns fast (production is 10 s).
    var backend: call_deadline.MonotonicBackend = .{};
    var sched = call_deadline.ProcessScheduler.init(testing.allocator, &backend);
    try sched.start();
    defer sched.deinit();
    var ex = HttpClientExchange{ .io = io, .sched = &sched, .deadline_ms = STALL_DEADLINE_MS };
    const boundary = ex.exchange();

    const t0 = common.clock.nowMillis();
    const r = boundary.post(testing.allocator, .{ .url = url, .body = "{}" });
    const elapsed = common.clock.nowMillis() - t0;
    // The stalled read is cut by the watchdog → the broker maps it to a failure.
    try testing.expectError(error.HttpExchangeFailed, r);
    // Returned from the fired deadline, not the vendor (which never answers).
    try testing.expect(elapsed < ELAPSED_BOUND_MS);
}

// A vendor that dies mid-body: valid head promising 4096 bytes, 128 sent, then
// close. The stalled-vendor test above never writes a body byte, which is why
// the accumulator leak survived it — this one puts real bytes in the writer
// before the failure so testing.allocator proves the error path frees them.
const PartialVendor = struct {
    fn run(listener: *std.Io.net.Server, io: std.Io) void {
        const conn = listener.accept(io) catch return;
        defer conn.close(io);
        var buf: [2048]u8 = undefined;
        _ = std.posix.read(conn.socket.handle, &buf) catch return;
        // Truncated CHUNKED framing: one full 0x80-byte chunk lands in the
        // client's accumulator, then the terminal chunk never arrives — a
        // guaranteed hard read error (a short content-length body is tolerated
        // by std's fetch, chunked truncation is not).
        const head: []const u8 = "HTTP/1.1 200 OK\r\ntransfer-encoding: chunked\r\n\r\n80\r\n" ++ ("x" ** 128) ++ "\r\n";
        var sent: usize = 0;
        while (sent < head.len) {
            const rc = std.posix.system.write(conn.socket.handle, head[sent..].ptr, head.len - sent);
            if (std.posix.errno(rc) != .SUCCESS) return;
            sent += @intCast(rc);
        }
    }
};

test "Built.deinit zeroizes and frees every platform secret, leak-free (Dimension 3.1)" {
    const alloc = testing.allocator;
    // Field-by-field with an errdefer ladder (A2): a later dupe failure must
    // not leak the earlier ones even inside the test.
    const app_id = try alloc.dupe(u8, "12345");
    errdefer alloc.free(app_id);
    const pem = try alloc.dupe(u8, "fake-pem-material");
    errdefer alloc.free(pem);
    const slug = try alloc.dupe(u8, "my-app");
    errdefer alloc.free(slug);
    const cid = try alloc.dupe(u8, "client-id");
    errdefer alloc.free(cid);
    const csec = try alloc.dupe(u8, "client-secret-material");
    var built = serve_broker.Built{
        // deinit never reads deps — it frees only the owned secret bytes.
        .deps = undefined,
        .github_app = .{ .app_id = app_id, .private_key_pem = pem, .app_slug = slug },
        .zoho_app = .{ .client_id = cid, .client_secret = csec },
    };
    // Teardown routes the pem + client secret through secure_memory.freeBytes;
    // std.testing.allocator proves the zeroizing path frees everything.
    built.deinit(alloc);
}

test "exchange frees the partial response body when the vendor dies mid-stream" {
    const io = common.globalIo();
    var addr = try std.Io.net.IpAddress.parseIp4("127.0.0.1", 0);
    var listener = addr.listen(io, .{ .reuse_address = true }) catch return error.SkipZigTest;
    defer listener.deinit(io);
    const port = boundPort(listener.socket.handle) catch return error.SkipZigTest;
    const vendor = std.Thread.spawn(.{}, PartialVendor.run, .{ &listener, io }) catch return error.SkipZigTest;

    var url_buf: [48]u8 = undefined;
    const url = try std.fmt.bufPrint(&url_buf, "http://127.0.0.1:{d}", .{port});

    var backend: call_deadline.MonotonicBackend = .{};
    var sched = call_deadline.ProcessScheduler.init(testing.allocator, &backend);
    try sched.start();
    defer sched.deinit();
    var ex = HttpClientExchange{ .io = io, .sched = &sched };
    const boundary = ex.exchange();
    const r = boundary.post(testing.allocator, .{ .url = url, .body = "{}" });
    vendor.join();
    if (r) |resp| {
        // Unexpected success must not ALSO leak the body and drown the signal.
        testing.allocator.free(resp.body);
        return error.TestUnexpectedResult;
    } else |err| try testing.expectEqual(error.HttpExchangeFailed, err);
}

// ---------------------------------------------------------------------------
// Vault-backed platform-app loads — the half of this file only a real boot
// runs. `loadGithubApp` and (through `buildDeps`) `loadOauthApp` decide whether
// each provider can mint at all; every miss must degrade to null so an
// unconfigured platform still boots. Live-database cases guard on
// TEST_DATABASE_URL like pool_test: skipped in the plain lane, executed in the
// coverage lane.
// ---------------------------------------------------------------------------

const pg = @import("pg");
const db = @import("../db/pool.zig");
const base = @import("../db/test_fixtures.zig");
const vault = @import("../state/vault.zig");
const crypto_primitives = @import("../secrets/crypto_primitives.zig");

const BROKER_WS = "0195b4ba-8d3a-7f13-8abc-2b3e1e0bbbb1";

fn liveBrokerPool() !?*db.Pool {
    const url = common.env.testLiveValue("TEST_DATABASE_URL") orelse return null;
    var env = try common.env.fromPairs(testing.allocator, &.{.{ "DATABASE_URL_API", url }});
    defer env.deinit();
    return try db.initFromEnvForRole(common.globalIo(), &env, testing.allocator, .api);
}

fn seedBrokerWorkspace(conn: *pg.Conn) !void {
    try base.seedTenant(conn);
    try base.seedWorkspace(conn, BROKER_WS);
}

fn cleanupBrokerWorkspace(conn: *pg.Conn) void {
    _ = conn.exec("DELETE FROM vault.secrets WHERE workspace_id = $1", .{BROKER_WS}) catch |err|
        std.log.warn("cleanup ignored: {s}", .{@errorName(err)});
    base.teardownWorkspace(conn, BROKER_WS);
}

test "loadGithubApp returns the configured App and null on a missing field" {
    crypto_primitives.setTestKek();
    const pool = (try liveBrokerPool()) orelse return error.SkipZigTest;
    defer pool.deinit();
    const conn = try pool.acquire();
    defer pool.release(conn);
    try seedBrokerWorkspace(conn);
    defer cleanupBrokerWorkspace(conn);

    // (1) Fully configured: id + PEM + slug all come back duped.
    try vault.storeJsonPlaintext(testing.allocator, conn, BROKER_WS, serve_broker.GITHUB_APP_VAULT_KEY, "{\"app_id\":\"12345\",\"private_key_pem\":\"-----BEGIN RSA-----probe\",\"app_slug\":\"agentsfleet-app\"}");
    const app = serve_broker.loadGithubApp(testing.allocator, conn, BROKER_WS) orelse
        return error.TestUnexpectedResult;
    defer {
        testing.allocator.free(app.app_id);
        testing.allocator.free(app.private_key_pem);
        if (app.app_slug) |s| testing.allocator.free(s);
    }
    try testing.expectEqualStrings("12345", app.app_id);
    try testing.expect(app.app_slug != null);

    // (2) Missing private_key_pem → unconfigured, never a half-built App.
    try vault.storeJsonPlaintext(testing.allocator, conn, BROKER_WS, serve_broker.GITHUB_APP_VAULT_KEY, "{\"app_id\":\"12345\"}");
    try testing.expect(serve_broker.loadGithubApp(testing.allocator, conn, BROKER_WS) == null);

    // (3) No row at all → the vault-miss arm.
    cleanupBrokerWorkspace(conn);
    try seedBrokerWorkspace(conn);
    try testing.expect(serve_broker.loadGithubApp(testing.allocator, conn, BROKER_WS) == null);
}

test "buildDeps loads every configured OAuth provider app from the vault" {
    crypto_primitives.setTestKek();
    const pool = (try liveBrokerPool()) orelse return error.SkipZigTest;
    defer pool.deinit();
    {
        const conn = try pool.acquire();
        defer pool.release(conn);
        try seedBrokerWorkspace(conn);
        for ([_][]const u8{ "zoho-app", "jira-app", "linear-app" }) |key| {
            try vault.storeJsonPlaintext(testing.allocator, conn, BROKER_WS, key, "{\"client_id\":\"cid_probe\",\"client_secret\":\"csec_probe\"}");
        }
    }
    defer {
        const conn = pool.acquire() catch null;
        if (conn) |c| {
            cleanupBrokerWorkspace(c);
            pool.release(c);
        }
    }

    var backend: call_deadline.MonotonicBackend = .{};
    var sched = call_deadline.ProcessScheduler.init(testing.allocator, &backend);
    try sched.start();
    defer sched.deinit();
    var ex = HttpClientExchange{ .io = common.globalIo(), .sched = &sched };

    var built = serve_broker.buildDeps(testing.allocator, pool, &ex, BROKER_WS);
    defer built.deinit(testing.allocator);

    // Each provider's app came off its own vault key; a wrong-key read would
    // leave that provider unconfigured and its mints reconnect_required.
    try testing.expect(built.zoho_app != null);
    try testing.expect(built.jira_app != null);
    try testing.expect(built.linear_app != null);
    try testing.expectEqualStrings("cid_probe", built.zoho_app.?.client_id);
}
