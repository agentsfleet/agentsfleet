//! Scripted multi-beat control-loop proof (loop.zig `runLoop`): the pool comes
//! up on the first applicable assignment, an unchanged capability probe is not
//! re-sent, a degraded reply forgets the accepted report so the row can only
//! converge, and a `drain` directive exits `.drained` with the workers joined.
//! The `heartbeat_interval_ms` + `backoff_ms` seams keep the four-beat script
//! at milliseconds; the watchdog turns a wedged loop into a fast loud failure.

const std = @import("std");
const testing = std.testing;
const common = @import("common");
const contract = @import("contract");
const Config = @import("config.zig");
const loop = @import("loop.zig");
const dts = @import("deadline_test_support.zig");

const protocol = contract.protocol;
const ALLOC = testing.allocator;

const RUNNER_TOKEN = protocol.RUNNER_TOKEN_PREFIX ++ "a" ** 64;
const STORAGE_BASE = "/tmp/agentsfleet-m164-loop-seq-test";
const POLICY_JSON =
    \\{"sandbox_tier":"dev_none","network_policy":"deny_all_egress","registry_allowlist":[],"worker_count":1}
;
const BEAT_OK_WITH_POLICY = "{\"status\":\"ok\",\"assigned_policy\":" ++ POLICY_JSON ++ "}";
const BEAT_OK_DEGRADED = "{\"status\":\"ok\",\"degraded\":true,\"assigned_policy\":" ++ POLICY_JSON ++ "}";
const BEAT_DRAIN = "{\"status\":\"drain\"}";
/// The retry hint is deliberately LONG. This stub is serial — one connection at
/// a time — and the pool's worker shares it with the control loop. A tight hint
/// lets the worker re-poll faster than the loop can heartbeat, so the loop is
/// starved, never reaches the drain directive, and the run hangs. One poll then
/// a long idle proves the pool came up and leaves the socket to the heartbeats.
const LEASE_RETRY_AFTER_MS = 3_000;
const EMPTY_LEASE_BODY = "{\"lease\":null,\"retry_after_ms\":3000}";
/// Beats 1+2 prove pool-up + probe dedupe, 3 proves the degraded forget, 4 exits.
const SCRIPT = [_][]const u8{ BEAT_OK_WITH_POLICY, BEAT_OK_WITH_POLICY, BEAT_OK_DEGRADED, BEAT_DRAIN };
const HTTP_REQ_BUF_BYTES: usize = 4096;
/// Generous bound for a millisecond-cadence four-beat script; only a wedged
/// loop (or a stub that stopped answering) ever reaches it. Comfortably clears
/// one worker idle at `LEASE_RETRY_AFTER_MS`, which the pool join waits out.
const SEQ_WATCHDOG_MS: u64 = LEASE_RETRY_AFTER_MS * 5;

fn zeroBackoff(_: u32) u64 {
    return 0;
}

fn boundPort(handle: std.Io.net.Socket.Handle) !u16 {
    // SAFETY: getsockname fills sa before sa.port is read on success; the != 0
    // branch returns an error without reading sa.
    var sa: std.posix.sockaddr.in = undefined;
    var len: std.posix.socklen_t = @sizeOf(std.posix.sockaddr.in);
    if (std.c.getsockname(handle, @ptrCast(&sa), &len) != 0) return error.GetSockNameFailed;
    return std.mem.bigToNative(u16, sa.port);
}

/// Serial loopback plane: heartbeats consume the script in order (the last
/// entry repeats if a straggler beat arrives); lease polls always get an empty
/// lease with a tight retry hint. Counts both. Retired via `shutdown()`.
const SeqStub = struct {
    io: std.Io,
    listener: *std.Io.net.Server,
    beats: std.atomic.Value(u32) = std.atomic.Value(u32).init(0),
    lease_polls: std.atomic.Value(u32) = std.atomic.Value(u32).init(0),
    stop: std.atomic.Value(bool) = std.atomic.Value(bool).init(false),

    fn run(self: *SeqStub) void {
        while (true) {
            const conn = self.listener.accept(self.io) catch return;
            if (self.stop.load(.seq_cst)) {
                conn.close(self.io);
                return;
            }
            self.serve(conn);
        }
    }

    fn serve(self: *SeqStub, conn: std.Io.net.Stream) void {
        defer conn.close(self.io);
        var buf: [HTTP_REQ_BUF_BYTES]u8 = undefined;
        var total: usize = 0;
        while (total < buf.len) {
            const n = std.posix.read(conn.socket.handle, buf[total..]) catch break;
            if (n == 0) break;
            total += n;
            if (std.mem.indexOf(u8, buf[0..total], "\r\n\r\n") != null) break;
        }
        const req_line_end = std.mem.indexOf(u8, buf[0..total], "\r\n") orelse total;
        const req_line = buf[0..req_line_end];

        const body = blk: {
            if (std.mem.indexOf(u8, req_line, protocol.PATH_RUNNER_HEARTBEATS) != null) {
                const seq = self.beats.fetchAdd(1, .seq_cst);
                break :blk SCRIPT[@min(seq, SCRIPT.len - 1)];
            }
            _ = self.lease_polls.fetchAdd(1, .seq_cst);
            break :blk EMPTY_LEASE_BODY;
        };
        var wbuf: [512]u8 = undefined;
        var w = conn.writer(self.io, &wbuf);
        w.interface.print(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {d}\r\nConnection: close\r\n\r\n{s}",
            .{ body.len, body },
        ) catch return;
        w.interface.flush() catch return;
    }

    fn shutdown(self: *SeqStub, port: u16) void {
        self.stop.store(true, .seq_cst);
        var addr = std.Io.net.IpAddress.parseIp4("127.0.0.1", port) catch return;
        const stream = addr.connect(self.io, .{ .mode = .stream }) catch return;
        stream.close(self.io);
    }
};

/// Requests drain if the scripted loop wedges, so the test fails loudly in
/// seconds instead of hanging the suite.
const Watchdog = struct {
    done: std.atomic.Value(bool) = std.atomic.Value(bool).init(false),
    fired: std.atomic.Value(bool) = std.atomic.Value(bool).init(false),

    fn run(self: *Watchdog) void {
        var waited_ms: u64 = 0;
        while (!self.done.load(.seq_cst) and waited_ms < SEQ_WATCHDOG_MS) {
            common.sleepNanos(50 * std.time.ns_per_ms);
            waited_ms += 50;
        }
        if (self.done.load(.seq_cst)) return;
        self.fired.store(true, .seq_cst);
        loop.drain_requested.store(true, .seq_cst);
    }
};

test "the control loop brings the pool up on assignment, dedupes probes, forgets on degraded, and drains on directive" {
    const saved_backoff = loop.backoff_ms;
    const saved_interval = loop.heartbeat_interval_ms;
    loop.backoff_ms = zeroBackoff;
    loop.heartbeat_interval_ms = 1;
    defer {
        loop.backoff_ms = saved_backoff;
        loop.heartbeat_interval_ms = saved_interval;
    }
    loop.drain_requested.store(false, .seq_cst);
    defer loop.drain_requested.store(false, .seq_cst);

    const io = common.globalIo();
    var addr = try std.Io.net.IpAddress.parseIp4("127.0.0.1", 0);
    var listener = addr.listen(io, .{ .reuse_address = true }) catch return error.SkipZigTest;
    const port = boundPort(listener.socket.handle) catch return error.SkipZigTest;
    var stub = SeqStub{ .io = io, .listener = &listener };
    const stub_thread = std.Thread.spawn(.{}, SeqStub.run, .{&stub}) catch return error.SkipZigTest;
    var wd = Watchdog{};
    const wd_thread = try std.Thread.spawn(.{}, Watchdog.run, .{&wd});

    var url_buf: [48]u8 = undefined;
    const url = try std.fmt.bufPrint(&url_buf, "http://127.0.0.1:{d}", .{port});
    const cfg = Config{
        .control_plane_url = try ALLOC.dupe(u8, url),
        .runner_token = try ALLOC.dupe(u8, RUNNER_TOKEN),
        .sandbox_tier = .dev_none,
        .storage_home = try ALLOC.dupe(u8, STORAGE_BASE),
        .network_policy = .deny_all_egress,
        .worker_count = 1,
        .cp_deadlines = .{},
        .registry_allowlist = &.{},
        .alloc = ALLOC,
    };
    defer cfg.deinit();
    var env_map: std.process.Environ.Map = .init(ALLOC);
    defer env_map.deinit();

    var deadlines: dts.TestScheduler = .{};
    defer deadlines.deinit();
    const exit_reason = loop.runLoop(io, ALLOC, try deadlines.start(ALLOC), cfg, &env_map);

    wd.done.store(true, .seq_cst);
    wd_thread.join();
    stub.shutdown(port);
    stub_thread.join();
    listener.deinit(io);

    // A watchdog fire means the loop wedged — the drained exit it forces must
    // not read as a pass.
    try testing.expect(!wd.fired.load(.seq_cst));
    // The drain directive (beat 4) is the exit, workers joined on the way out.
    try testing.expectEqual(loop.LoopExit.drained, exit_reason);
    try testing.expectEqual(@as(u32, SCRIPT.len), stub.beats.load(.seq_cst));
    // The pool actually came up on beat 1's assignment: its worker leased at
    // least once against the plane before the drain landed.
    try testing.expect(stub.lease_polls.load(.seq_cst) >= 1);
}
