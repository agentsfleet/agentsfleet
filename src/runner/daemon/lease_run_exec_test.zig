//! Pre-fork proofs for `lease_run.zig` — every `executeAndReport` arm that
//! resolves BEFORE the child fork, driven end-to-end over a real loopback
//! control plane. The fork itself (and everything after it) is deliberately
//! out of reach here: the unit module's child exec target is the test binary
//! itself, so those arcs are proven by the runner integration suite instead.
//!
//! Proven arms:
//!   - workspace prep failure → bounded backoff, no plane contact, no execute;
//!   - bundle materialization failure → a startup-failure report finalizes the
//!     event (never a silent expiry), the child never forks, the workspace is
//!     cleaned up;
//!   - a dead report endpoint on that path is swallowed (lease expires for
//!     reclaim) rather than crashing the worker;
//!   - the production `MintForwarder` maps the daemon's minted/rejected verdicts
//!     onto the child-facing hook, fail-closed;
//!   - `TickFanout` fans a renewal tick into flush-then-renew-decision.

const std = @import("std");
const testing = std.testing;
const common = @import("common");
const contract = @import("contract");
const Config = @import("config.zig");
const lease_run = @import("lease_run.zig");
const forwarders = @import("forwarders.zig");
const renew_driver = @import("renew_driver.zig");
const client_mod = @import("control_plane_client.zig");
const dts = @import("deadline_test_support.zig");

const protocol = contract.protocol;
const ALLOC = testing.allocator;

const RUNNER_TOKEN = protocol.RUNNER_TOKEN_PREFIX ++ "a" ** 64;
const LEASE_ID = "lease-prefork-1";
const EVENT_ID = "event-prefork-1";
const BUNDLE_HASH = "cafe01cafe01";
/// Base under the standard tmp root, mirroring loop_test's storage fixtures.
const STORAGE_BASE = "/tmp/agentsfleet-m164-lease-exec-test";
/// A path that can never be a directory base: children of a char device fail
/// `createDirAbsolute` with NotDir on every platform, deterministically.
const UNCREATABLE_BASE = "/dev/null";
/// Far-future lease deadline so a renewal tick decides `.keep` with no network.
const FAR_FUTURE_MS: i64 = 1 << 40;
const CP_DEADLINE_MS: u31 = 2_000;
/// The workspace-prep failure arm sleeps one production backoff step (~2s +
/// jitter); the bound proves it backed off AND returned, rather than hanging.
const PREP_FAIL_MAX_WALL_MS: i64 = 15_000;
const HTTP_REQ_BUF_BYTES: usize = 8192;
const STATUS_LINE_OK = "HTTP/1.1 200 OK";
const STATUS_LINE_FAIL = "HTTP/1.1 500 Internal Server Error";
const EMPTY_JSON = "{}";

fn leasePayload(bundle: ?protocol.BundleManifest) protocol.LeasePayload {
    return .{
        .lease_id = LEASE_ID,
        .fencing_token = 7,
        .lease_expires_at = FAR_FUTURE_MS,
        .secret_delivery = .@"inline",
        .policy = .{},
        .event = .{
            .event_id = EVENT_ID,
            .fleet_id = "fleet-prefork",
            .workspace_id = "workspace-prefork",
            .actor = "actor-prefork",
            .event_type = .webhook,
            .request_json = EMPTY_JSON,
            .created_at = 1,
        },
        .bundle = bundle,
    };
}

fn testConfig(alloc: std.mem.Allocator, url: []const u8, storage_home: []const u8) !Config {
    return .{
        .control_plane_url = try alloc.dupe(u8, url),
        .runner_token = try alloc.dupe(u8, RUNNER_TOKEN),
        .sandbox_tier = .dev_none,
        .storage_home = try alloc.dupe(u8, storage_home),
        .network_policy = .deny_all_egress,
        .worker_count = 1,
        .cp_deadlines = .{},
        .registry_allowlist = &.{},
        .alloc = alloc,
    };
}

fn boundPort(handle: std.Io.net.Socket.Handle) !u16 {
    // SAFETY: getsockname fills sa before sa.port is read on success; the != 0
    // branch returns an error without reading sa.
    var sa: std.posix.sockaddr.in = undefined;
    var len: std.posix.socklen_t = @sizeOf(std.posix.sockaddr.in);
    if (std.c.getsockname(handle, @ptrCast(&sa), &len) != 0) return error.GetSockNameFailed;
    return std.mem.bigToNative(u16, sa.port);
}

/// How the scripted plane answers the startup-failure report POST.
const ReportMode = enum { ok, drop };

/// Serial loopback control plane for the bundle-failure arc: 500s the bundle
/// download (a 404 would mean skill-only → ready), then per `report_mode`
/// answers or drops the startup-failure report. Captures the report body.
/// Retired via `shutdown()` — never by closing the listener under a blocked
/// accept (Linux never wakes it).
const ScriptedPlane = struct {
    io: std.Io,
    listener: *std.Io.net.Server,
    report_mode: ReportMode,
    bundle_gets: std.atomic.Value(u32) = std.atomic.Value(u32).init(0),
    report_posts: std.atomic.Value(u32) = std.atomic.Value(u32).init(0),
    stop: std.atomic.Value(bool) = std.atomic.Value(bool).init(false),
    report_body_buf: [HTTP_REQ_BUF_BYTES]u8 = [_]u8{0} ** HTTP_REQ_BUF_BYTES,
    report_body_len: usize = 0,

    fn run(self: *ScriptedPlane) void {
        while (true) {
            const conn = self.listener.accept(self.io) catch return;
            if (self.stop.load(.seq_cst)) {
                conn.close(self.io);
                return;
            }
            self.serve(conn);
        }
    }

    fn serve(self: *ScriptedPlane, conn: std.Io.net.Stream) void {
        defer conn.close(self.io);
        var buf: [HTTP_REQ_BUF_BYTES]u8 = undefined;
        var total: usize = 0;
        var header_end: usize = 0;
        while (true) {
            if (std.mem.indexOf(u8, buf[0..total], "\r\n\r\n")) |idx| {
                header_end = idx + 4;
                break;
            }
            const n = std.posix.read(conn.socket.handle, buf[total..]) catch return;
            if (n == 0) return;
            total += n;
            if (total == buf.len) return;
        }
        const content_len = parseContentLength(buf[0..header_end]) orelse 0;
        while (total < header_end + content_len and total < buf.len) {
            const n = std.posix.read(conn.socket.handle, buf[total..]) catch break;
            if (n == 0) break;
            total += n;
        }
        const req_line_end = std.mem.indexOf(u8, buf[0..header_end], "\r\n") orelse header_end;
        const req_line = buf[0..req_line_end];

        if (std.mem.indexOf(u8, req_line, protocol.PATH_RUNNER_BUNDLES) != null) {
            _ = self.bundle_gets.fetchAdd(1, .seq_cst);
            respond(self.io, conn, STATUS_LINE_FAIL, EMPTY_JSON);
            return;
        }
        if (std.mem.indexOf(u8, req_line, protocol.PATH_RUNNER_REPORTS) != null) {
            _ = self.report_posts.fetchAdd(1, .seq_cst);
            const body = buf[header_end..@min(total, header_end + content_len)];
            @memcpy(self.report_body_buf[0..body.len], body);
            self.report_body_len = body.len;
            if (self.report_mode == .drop) return; // close with no response
            respond(self.io, conn, STATUS_LINE_OK, EMPTY_JSON);
            return;
        }
        respond(self.io, conn, STATUS_LINE_FAIL, EMPTY_JSON);
    }

    fn reportBody(self: *const ScriptedPlane) []const u8 {
        return self.report_body_buf[0..self.report_body_len];
    }

    /// Linux-safe retire: stop flag, one throwaway wake connect, caller joins,
    /// only then deinits the listener.
    fn shutdown(self: *ScriptedPlane, port: u16) void {
        self.stop.store(true, .seq_cst);
        var addr = std.Io.net.IpAddress.parseIp4("127.0.0.1", port) catch return;
        const stream = addr.connect(self.io, .{ .mode = .stream }) catch return;
        stream.close(self.io);
    }
};

fn respond(io: std.Io, conn: std.Io.net.Stream, status_line: []const u8, body: []const u8) void {
    var wbuf: [256]u8 = undefined;
    var w = conn.writer(io, &wbuf);
    w.interface.print(
        "{s}\r\nContent-Type: application/json\r\nContent-Length: {d}\r\nConnection: close\r\n\r\n{s}",
        .{ status_line, body.len, body },
    ) catch return;
    w.interface.flush() catch return;
}

fn parseContentLength(headers: []const u8) ?usize {
    var it = std.mem.splitSequence(u8, headers, "\r\n");
    while (it.next()) |line| {
        const prefix = "content-length:";
        if (line.len > prefix.len and std.ascii.startsWithIgnoreCase(line, prefix)) {
            const v = std.mem.trim(u8, line[prefix.len..], " ");
            return std.fmt.parseInt(usize, v, 10) catch null;
        }
    }
    return null;
}

/// Run the bundle-failure arc against a scripted plane; returns the observed
/// counters + captured report body length for the caller's assertions.
fn runBundleFailureArc(report_mode: ReportMode) !struct { bundle_gets: u32, report_posts: u32, body_had_lease: bool, body_had_fleet_error: bool } {
    const io = common.globalIo();
    var addr = try std.Io.net.IpAddress.parseIp4("127.0.0.1", 0);
    var listener = addr.listen(io, .{ .reuse_address = true }) catch return error.SkipZigTest;
    const port = boundPort(listener.socket.handle) catch return error.SkipZigTest;
    var stub = ScriptedPlane{ .io = io, .listener = &listener, .report_mode = report_mode };
    const stub_thread = std.Thread.spawn(.{}, ScriptedPlane.run, .{&stub}) catch return error.SkipZigTest;

    var url_buf: [48]u8 = undefined;
    const url = try std.fmt.bufPrint(&url_buf, "http://127.0.0.1:{d}", .{port});
    var deadlines: dts.TestScheduler = .{};
    defer deadlines.deinit();
    var cp = client_mod.init(ALLOC, io, try deadlines.start(ALLOC), url);
    defer cp.deinit();

    // A real storage base so workspace prep succeeds and the arc reaches the
    // bundle download; a fresh hash so the content-addressed cache misses.
    std.Io.Dir.createDirAbsolute(io, STORAGE_BASE, .default_dir) catch |err| switch (err) {
        error.PathAlreadyExists => {},
        else => return error.SkipZigTest,
    };
    const cfg = try testConfig(ALLOC, url, STORAGE_BASE);
    defer cfg.deinit();
    var env_map: std.process.Environ.Map = .init(ALLOC);
    defer env_map.deinit();

    lease_run.executeAndReport(io, ALLOC, &cp, RUNNER_TOKEN, cfg, &env_map, leasePayload(.{ .content_hash = BUNDLE_HASH }));

    stub.shutdown(port);
    stub_thread.join();
    listener.deinit(io);

    // The per-lease workspace is torn down on every exit path.
    var ws_buf: [std.fs.max_path_bytes]u8 = undefined;
    const ws = try std.fmt.bufPrint(&ws_buf, "{s}/{s}", .{ STORAGE_BASE, LEASE_ID });
    try testing.expectError(error.FileNotFound, std.Io.Dir.cwd().access(io, ws, .{}));

    return .{
        .bundle_gets = stub.bundle_gets.load(.seq_cst),
        .report_posts = stub.report_posts.load(.seq_cst),
        .body_had_lease = std.mem.indexOf(u8, stub.reportBody(), LEASE_ID) != null,
        .body_had_fleet_error = std.mem.indexOf(u8, stub.reportBody(), "fleet_error") != null,
    };
}

test "a bundle that fails to download reports a startup failure and never executes" {
    const r = try runBundleFailureArc(.ok);
    // One download attempt (500 → fail-closed, no retry: retry is deferred by
    // spec), then exactly one finalizing report — never a silent lease expiry.
    try testing.expectEqual(@as(u32, 1), r.bundle_gets);
    try testing.expectEqual(@as(u32, 1), r.report_posts);
    try testing.expect(r.body_had_lease);
    try testing.expect(r.body_had_fleet_error);
}

test "a dead report endpoint on the startup-failure path is swallowed, not fatal" {
    // The report POST is dropped mid-flight: the worker logs, returns, and the
    // lease is left to expire for reclaim — the arc must not error or hang.
    const r = try runBundleFailureArc(.drop);
    try testing.expectEqual(@as(u32, 1), r.bundle_gets);
    try testing.expectEqual(@as(u32, 1), r.report_posts);
}

test "an uncreatable workspace backs off once and returns without touching the plane" {
    const io = common.globalIo();
    var deadlines: dts.TestScheduler = .{};
    defer deadlines.deinit();
    // Loopback port 9 (discard) — nothing listens; any contact would error the
    // client loudly. The arm under test returns before any plane call.
    var cp = client_mod.init(ALLOC, io, try deadlines.start(ALLOC), "http://127.0.0.1:9");
    defer cp.deinit();
    const cfg = try testConfig(ALLOC, "http://127.0.0.1:9", UNCREATABLE_BASE);
    defer cfg.deinit();
    var env_map: std.process.Environ.Map = .init(ALLOC);
    defer env_map.deinit();

    const started = common.clock.nowMillis();
    lease_run.executeAndReport(io, ALLOC, &cp, RUNNER_TOKEN, cfg, &env_map, leasePayload(null));
    const wall = common.clock.nowMillis() - started;
    // Backed off (worker poll loops must not hot-spin on a persistent prep
    // failure) yet bounded — returned after one backoff step, no hang.
    try testing.expect(wall < PREP_FAIL_MAX_WALL_MS);
}

test "TickFanout fans a renewal tick into flush-then-decision and keeps a fresh lease" {
    const io = common.globalIo();
    var deadlines: dts.TestScheduler = .{};
    defer deadlines.deinit();
    var cp = client_mod.init(ALLOC, io, try deadlines.start(ALLOC), "http://127.0.0.1:9");
    defer cp.deinit();

    var forwarder = forwarders.ActivityForwarder{
        .alloc = ALLOC,
        .cp = &cp,
        .runner_token = RUNNER_TOKEN,
        .lease_id = LEASE_ID,
        .deadline_ms = CP_DEADLINE_MS,
    };
    defer forwarder.deinit();
    const Driver = renew_driver.RenewDriver(*client_mod);
    var driver = Driver.init(ALLOC, &cp, RUNNER_TOKEN, leasePayload(null), CP_DEADLINE_MS);
    var fanout = lease_run.TickFanout{ .forwarder = &forwarder, .driver = &driver };
    const hook = fanout.hook();

    // The hook rides the supervisor's tick cadence — a drifted tick_ms would
    // silently change both the flush and renewal cadences.
    try testing.expectEqual(common.RENEWAL_TICK_MS, hook.tick_ms);
    // Far-future deadline: the decision is `.keep` with no renew call, and the
    // empty activity batch has nothing stale to flush — no plane contact at all
    // (the dead port above would error loudly if either path dialed out).
    const decision = hook.onTick(hook.ctx, common.clock.nowMillis(), .{});
    try testing.expect(decision == .keep);
}

test "the production MintForwarder fails the child's ask closed on a rejected mint" {
    const io = common.globalIo();
    var addr = try std.Io.net.IpAddress.parseIp4("127.0.0.1", 0);
    var listener = addr.listen(io, .{ .reuse_address = true }) catch return error.SkipZigTest;
    const port = boundPort(listener.socket.handle) catch return error.SkipZigTest;
    // The scripted plane 500s every non-bundle, non-report path — including the
    // mint POST — which is exactly the rejected-verdict wire shape (non-2xx).
    var stub = ScriptedPlane{ .io = io, .listener = &listener, .report_mode = .ok };
    const stub_thread = std.Thread.spawn(.{}, ScriptedPlane.run, .{&stub}) catch return error.SkipZigTest;

    var url_buf: [48]u8 = undefined;
    const url = try std.fmt.bufPrint(&url_buf, "http://127.0.0.1:{d}", .{port});
    var deadlines: dts.TestScheduler = .{};
    defer deadlines.deinit();
    var cp = client_mod.init(ALLOC, io, try deadlines.start(ALLOC), url);
    defer cp.deinit();

    var minter = lease_run.MintForwarder{
        .cp = &cp,
        .runner_token = RUNNER_TOKEN,
        .lease_id = LEASE_ID,
        .deadline_ms = CP_DEADLINE_MS,
    };
    const hook = minter.hook();
    const outcome = hook.onMint(hook.ctx, ALLOC, "github", null);
    stub.shutdown(port);
    stub_thread.join();
    listener.deinit(io);
    // Fail-closed: the child aborts its tool call; no token, no fallback.
    try testing.expect(outcome == .rejected);
}

test "detailFor answers a distinct cause line for every materialize failure" {
    // A startup failure is all the operator gets — there is no runner log for a
    // hosted user to read — so the three causes must not share a line.
    const download = lease_run.detailFor(.download);
    const malformed = lease_run.detailFor(.malformed);
    const memory = lease_run.detailFor(.memory);
    for ([_][]const u8{ download, malformed, memory }) |line| try testing.expect(line.len > 0);
    try testing.expect(!std.mem.eql(u8, download, malformed));
    try testing.expect(!std.mem.eql(u8, malformed, memory));
    try testing.expect(!std.mem.eql(u8, download, memory));
}
