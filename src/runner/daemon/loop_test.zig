//! Unit + boot tests for the runner's parent event-leasing loop (`loop.zig`):
//! proves boot goes straight to heartbeat → lease (never register, Option B), the
//! SIGTERM/SIGINT handler flips the drain flag, and the exit → outcome mapping.
//! The boot test spins a one-shot loopback control plane on an ephemeral port with
//! a watchdog, so a non-responding stub fails fast instead of hanging.

const std = @import("std");
const testing = std.testing;
const constants = @import("common");
const contract = @import("contract");
const Config = @import("config.zig");
const loop = @import("loop.zig");
const dts = @import("deadline_test_support.zig");

const protocol = contract.protocol;

/// Lease identity a report carries beside the verdict — irrelevant to the
/// projections asserted here, so one fixture serves every case.
const REPORT_CTX = contract.report_mapping.ReportContext{
    .lease_id = "lease-1",
    .event_id = "event-1",
    .fencing_token = 1,
    .wall_ms = 0,
};

/// Scratch buffer for reading the stub control plane's one request line.
const HEARTBEAT_REQ_BUF_BYTES: usize = 1024;

// Records the first request line a one-shot loopback control plane observes, so
// the boot test can prove the daemon's first contact is a heartbeat (lease-loop
// entry), never a register call.
const BootProbe = struct {
    // SAFETY: written by serveOneStopHeartbeat before line_len is set; only
    // line_buf[0..line_len] is ever read.
    line_buf: [256]u8 = undefined,
    line_len: usize = 0,
};

// Read the kernel-assigned local port off a bound listener handle. Zig 0.16's
// std.Io.net.Server exposes no getsockname; go through libc on the raw fd. (The
// runner can't share agentsfleetd's test_port helper — separate module/binary.)
fn boundPort(handle: std.Io.net.Socket.Handle) !u16 {
    // SAFETY: getsockname fills sa before sa.port is read on success; the !=0
    // branch returns an error without reading sa.
    var sa: std.posix.sockaddr.in = undefined;
    var len: std.posix.socklen_t = @sizeOf(std.posix.sockaddr.in);
    if (std.c.getsockname(handle, @ptrCast(&sa), &len) != 0) return error.GetSockNameFailed;
    return std.mem.bigToNative(u16, sa.port);
}

// Accept one connection, capture its request line, reply `stop` so `runLoop`
// exits after a single heartbeat. The `stop` body must parse cleanly or the loop
// would back off and retry — hence a well-formed fixed HTTP/1.1 response.
fn serveOneStopHeartbeat(listener: *std.Io.net.Server, io: std.Io, probe: *BootProbe) void {
    const conn = listener.accept(io) catch return;
    defer conn.close(io);

    var buf: [HEARTBEAT_REQ_BUF_BYTES]u8 = undefined;
    var total: usize = 0;
    while (total < buf.len) {
        // SO_RCVTIMEO not set here; raw posix.read mirrors the prior one-recv loop.
        const n = std.posix.read(conn.socket.handle, buf[total..]) catch break;
        if (n == 0) break;
        total += n;
        if (std.mem.indexOf(u8, buf[0..total], "\r\n\r\n") != null) break;
    }
    const line_end = std.mem.indexOf(u8, buf[0..total], "\r\n") orelse total;
    probe.line_len = @min(line_end, probe.line_buf.len);
    @memcpy(probe.line_buf[0..probe.line_len], buf[0..probe.line_len]);

    var wbuf: [256]u8 = undefined;
    var w = conn.writer(io, &wbuf);
    w.interface.writeAll(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n" ++
            "Content-Length: 17\r\nConnection: close\r\n\r\n{\"status\":\"stop\"}",
    ) catch return;
    w.interface.flush() catch return;
}

/// Upper bound the boot test can wait for the stub to respond. The happy path
/// completes in well under a second; this only fires if the stub never responds.
const BOOT_TEST_WATCHDOG_MS: u64 = 5_000;

/// Guarantees the boot test cannot hang. `runLoop`'s control-plane client has no
/// read timeout (`std.http.Client.fetch`), so if the stub never responds — its
/// thread exits early, or a sandbox blocks loopback TCP — the heartbeat fetch
/// would block forever and `join()` would never be reached. On timeout this
/// requests drain and closes the listener: the blocked client read gets a reset,
/// the fetch errors, and `runLoop` falls through its heartbeat-error path to the
/// drain check and exits. A hang becomes a fast, loud failure (empty probe), not
/// an indefinite stall.
const BootWatchdog = struct {
    io: std.Io,
    listener: *std.Io.net.Server,
    done: std.atomic.Value(bool) = std.atomic.Value(bool).init(false),
    fired: std.atomic.Value(bool) = std.atomic.Value(bool).init(false),

    fn run(self: *BootWatchdog) void {
        var waited_ms: u64 = 0;
        while (!self.done.load(.seq_cst) and waited_ms < BOOT_TEST_WATCHDOG_MS) {
            constants.sleepNanos(50 * std.time.ns_per_ms);
            waited_ms += 50;
        }
        if (self.done.load(.seq_cst)) return;
        self.fired.store(true, .seq_cst);
        loop.drain_requested.store(true, .seq_cst);
        // Unblock runLoop's timeout-less read. The stub never accepted, so the
        // client's connection is queued on the listener; the client is blocked
        // reading a response that will never come. Closing the *listener* does
        // NOT reset an established-but-unaccepted connection on macOS/BSD — so
        // accept the queued connection and close *that* fd, which sends the peer
        // FIN/RST. The client read returns EOF, the heartbeat fetch errors, and
        // runLoop falls through to the drain check and exits.
        if (self.listener.accept(self.io)) |conn| conn.close(self.io) else |_| {}
        self.listener.deinit(self.io);
    }
};

test "runner boots from a agt_r token straight into the lease loop with no register call" {
    const alloc = testing.allocator;
    loop.drain_requested.store(false, .seq_cst);
    defer loop.drain_requested.store(false, .seq_cst);

    const io = constants.globalIo();
    var addr = try std.Io.net.IpAddress.parseIp4("127.0.0.1", 0);
    var listener = try addr.listen(io, .{ .reuse_address = true });
    const port = try boundPort(listener.socket.handle);

    var probe: BootProbe = .{};
    var server_thread = try std.Thread.spawn(.{}, serveOneStopHeartbeat, .{ &listener, io, &probe });
    var wd = BootWatchdog{ .io = io, .listener = &listener };
    var wd_thread = try std.Thread.spawn(.{}, BootWatchdog.run, .{&wd});

    const url = try std.fmt.allocPrint(alloc, "http://127.0.0.1:{d}", .{port});
    defer alloc.free(url);
    // Identity is the pre-minted agt_r — Config is built directly here; the
    // env → Config parse (incl. the agt_r prefix gate) is covered in config.zig.
    const cfg = Config{
        .control_plane_url = try alloc.dupe(u8, url),
        .runner_token = try alloc.dupe(u8, contract.protocol.RUNNER_TOKEN_PREFIX ++ "a" ** 64),
        .host_id = try alloc.dupe(u8, "boot-test-host"),
        .sandbox_tier = .dev_none,
        .workspace_base = try alloc.dupe(u8, "/tmp/agentsfleet-runner-boot-test"),
        .network_policy = .deny_all_egress,
        .worker_count = 1,
        .cp_deadlines = .{},
        .registry_allowlist = &.{},
        .alloc = alloc,
    };
    defer cfg.deinit();

    // dev_none never forks a child, so the env block is unused here — an empty
    // map satisfies the threaded `runLoop` signature.
    var env_map: std.process.Environ.Map = .init(alloc);
    defer env_map.deinit();
    // Returns on the `stop` heartbeat (or on drain if the watchdog fires) — a
    // clean exit either way, never token_rejected/worker_pool_failed here.
    var deadlines: dts.TestScheduler = .{};
    defer deadlines.deinit();
    const exit_reason = loop.runLoop(io, alloc, try deadlines.start(alloc), cfg, &env_map);
    try testing.expect(exit_reason == .fleet_stop or exit_reason == .drained);
    wd.done.store(true, .seq_cst);
    server_thread.join();
    wd_thread.join();
    if (!wd.fired.load(.seq_cst)) listener.deinit(io); // watchdog already closed it if it fired

    // First (and only) control-plane contact is the heartbeat — not register.
    // If the watchdog fired (stub never responded), the probe is empty and this
    // fails fast rather than hanging.
    const observed = probe.line_buf[0..probe.line_len];
    const expected = "POST " ++ protocol.PATH_RUNNER_HEARTBEATS ++ " ";
    try testing.expect(std.mem.startsWith(u8, observed, expected));
    // The enrollment route is never touched on boot (Option B).
    try testing.expect(std.mem.indexOf(u8, observed, "POST " ++ protocol.PATH_RUNNERS ++ " ") == null);
}

// ── rejected-token streak (loop.zig fail-loud exit) ─────────────────────────

/// Zeroes the backoff seam so a ten-reject streak runs in milliseconds.
fn zeroBackoff(_: u32) u64 {
    return 0;
}

const AUTH_REJECT_RESPONSE = "HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";

/// Serial loopback control plane that 401s every heartbeat. `drop_at` (1-based
/// accept index) closes that connection without a response — a transport error
/// that must RESET the consecutive-reject streak, not count toward it. Retired
/// via `shutdown()` — NEVER by closing the listener under it: on Linux,
/// `listener.deinit` does not wake a blocked `accept`, so a join after it hangs
/// forever (it happens to wake on macOS, which is exactly how that hang ships).
const RejectingStub = struct {
    listener: *std.Io.net.Server,
    io: std.Io,
    drop_at: u32 = 0,
    accepts: std.atomic.Value(u32) = std.atomic.Value(u32).init(0),
    stop: std.atomic.Value(bool) = std.atomic.Value(bool).init(false),

    fn run(self: *RejectingStub) void {
        while (true) {
            const conn = self.listener.accept(self.io) catch return;
            if (self.stop.load(.seq_cst)) { // shutdown()'s wake connect, not a heartbeat
                conn.close(self.io);
                return;
            }
            defer conn.close(self.io);
            var buf: [HEARTBEAT_REQ_BUF_BYTES]u8 = undefined;
            var total: usize = 0;
            while (total < buf.len) {
                const n = std.posix.read(conn.socket.handle, buf[total..]) catch break;
                if (n == 0) break;
                total += n;
                if (std.mem.indexOf(u8, buf[0..total], "\r\n\r\n") != null) break;
            }
            const seq = self.accepts.fetchAdd(1, .seq_cst) + 1;
            if (seq == self.drop_at) continue; // close with no response: transport error
            var wbuf: [128]u8 = undefined;
            var w = conn.writer(self.io, &wbuf);
            w.interface.writeAll(AUTH_REJECT_RESPONSE) catch return;
            w.interface.flush() catch return;
        }
    }

    /// Linux-safe retire: set the stop flag, then wake the blocked accept with
    /// one throwaway loopback connect that run() swallows. The caller joins the
    /// stub thread after this and only THEN deinits the listener — no thread
    /// may still sit in accept at deinit.
    fn shutdown(self: *RejectingStub, port: u16) void {
        self.stop.store(true, .seq_cst);
        var addr = std.Io.net.IpAddress.parseIp4("127.0.0.1", port) catch return;
        const stream = addr.connect(self.io, .{ .mode = .stream }) catch return;
        stream.close(self.io);
    }
};

/// Boot a runLoop against a RejectingStub; `drained` is the drain flag as the
/// loop exited (read before the helper's cleanup defer resets it).
fn runRejectedTokenLoop(drop_at: u32) !struct { exit: loop.LoopExit, accepts: u32, drained: bool } {
    const alloc = testing.allocator;
    const saved_backoff = loop.backoff_ms;
    loop.backoff_ms = zeroBackoff;
    defer loop.backoff_ms = saved_backoff;
    loop.drain_requested.store(false, .seq_cst);
    defer loop.drain_requested.store(false, .seq_cst);

    const io = constants.globalIo();
    var addr = try std.Io.net.IpAddress.parseIp4("127.0.0.1", 0);
    var listener = try addr.listen(io, .{ .reuse_address = true });
    const port = try boundPort(listener.socket.handle);

    var stub = RejectingStub{ .listener = &listener, .io = io, .drop_at = drop_at };
    var stub_thread = try std.Thread.spawn(.{}, RejectingStub.run, .{&stub});
    var wd = BootWatchdog{ .io = io, .listener = &listener };
    var wd_thread = try std.Thread.spawn(.{}, BootWatchdog.run, .{&wd});

    const url = try std.fmt.allocPrint(alloc, "http://127.0.0.1:{d}", .{port});
    defer alloc.free(url);
    const cfg = Config{
        .control_plane_url = try alloc.dupe(u8, url),
        .runner_token = try alloc.dupe(u8, contract.protocol.RUNNER_TOKEN_PREFIX ++ "a" ** 64),
        .host_id = try alloc.dupe(u8, "reject-test-host"),
        .sandbox_tier = .dev_none,
        .workspace_base = try alloc.dupe(u8, "/tmp/agentsfleet-runner-reject-test"),
        .network_policy = .deny_all_egress,
        .worker_count = 1,
        .cp_deadlines = .{},
        .registry_allowlist = &.{},
        .alloc = alloc,
    };
    defer cfg.deinit();
    var env_map: std.process.Environ.Map = .init(alloc);
    defer env_map.deinit();

    var deadlines: dts.TestScheduler = .{};
    defer deadlines.deinit();
    const exit_reason = loop.runLoop(io, alloc, try deadlines.start(alloc), cfg, &env_map);
    // Read before this helper's own defer clears the flag for the next test.
    const drained = loop.drain_requested.load(.seq_cst);
    wd.done.store(true, .seq_cst);
    wd_thread.join();
    if (!wd.fired.load(.seq_cst)) {
        stub.shutdown(port); // wake the blocked accept (Linux-safe), stub retires
        stub_thread.join();
        listener.deinit(io); // only after the join — nothing sits in accept now
    } else {
        stub_thread.join(); // the watchdog already closed the listener; accept errored out
    }
    return .{ .exit = exit_reason, .accepts = stub.accepts.load(.seq_cst), .drained = drained };
}

test "ten consecutive rejected heartbeats exit token_rejected and request drain" {
    const r = try runRejectedTokenLoop(0);
    try testing.expectEqual(loop.LoopExit.token_rejected, r.exit);
    // The exit also requests drain so any sibling loops observe the stop.
    try testing.expect(r.drained);
    // Exactly the streak cap, no more — mirrors loop.zig's
    // MAX_CONSECUTIVE_AUTH_REJECTS; a drift here means the fail-loud
    // threshold silently moved.
    try testing.expectEqual(@as(u32, 10), r.accepts);
}

test "a transport error between rejects resets the consecutive-reject streak" {
    // Accept #6 drops with no response after five 401s: the streak restarts,
    // so the exit needs ten NEW consecutive rejects — 16 accepts total
    // (5 rejected + 1 dropped + 10 rejected), not 11.
    const r = try runRejectedTokenLoop(6);
    try testing.expectEqual(loop.LoopExit.token_rejected, r.exit);
    try testing.expectEqual(@as(u32, 16), r.accepts);
}

test "a rejected lease returns to the worker loop after one bounded idle" {
    const alloc = testing.allocator;
    const saved_backoff = loop.backoff_ms;
    loop.backoff_ms = zeroBackoff;
    defer loop.backoff_ms = saved_backoff;

    const io = constants.globalIo();
    var addr = try std.Io.net.IpAddress.parseIp4("127.0.0.1", 0);
    var listener = try addr.listen(io, .{ .reuse_address = true });
    const port = try boundPort(listener.socket.handle);
    var stub = RejectingStub{ .listener = &listener, .io = io };
    var stub_thread = try std.Thread.spawn(.{}, RejectingStub.run, .{&stub});

    const url = try std.fmt.allocPrint(alloc, "http://127.0.0.1:{d}", .{port});
    defer alloc.free(url);
    const cfg = Config{
        .control_plane_url = try alloc.dupe(u8, url),
        .runner_token = try alloc.dupe(u8, contract.protocol.RUNNER_TOKEN_PREFIX ++ "a" ** 64),
        .host_id = try alloc.dupe(u8, "lease-reject-host"),
        .sandbox_tier = .dev_none,
        .workspace_base = try alloc.dupe(u8, "/tmp/agentsfleet-runner-lease-reject"),
        .network_policy = .deny_all_egress,
        .worker_count = 1,
        .cp_deadlines = .{},
        .registry_allowlist = &.{},
        .alloc = alloc,
    };
    defer cfg.deinit();
    var env_map: std.process.Environ.Map = .init(alloc);
    defer env_map.deinit();

    var deadlines: dts.TestScheduler = .{};
    defer deadlines.deinit();
    var cp = @import("control_plane_client.zig").init(alloc, io, try deadlines.start(alloc), cfg.control_plane_url);
    defer cp.deinit();
    // A 401 lease must come back to the worker loop after ONE bounded idle —
    // the heartbeat loop owns the process exit; a worker that crashed, spun,
    // or retried inline here would hammer a known-rejected control plane.
    loop.pollAndProcess(io, alloc, &cp, cfg.runner_token, cfg, &env_map);
    try testing.expectEqual(@as(u32, 1), stub.accepts.load(.seq_cst));

    stub.shutdown(port); // Linux-safe: wake the blocked accept, then join, THEN deinit
    stub_thread.join();
    listener.deinit(io);
}

test "drain signal handler requests a graceful drain" {
    defer loop.drain_requested.store(false, .seq_cst);
    loop.drain_requested.store(false, .seq_cst);
    try testing.expect(!loop.drain_requested.load(.seq_cst));
    loop.requestDrain(std.posix.SIG.TERM);
    try testing.expect(loop.drain_requested.load(.seq_cst));
}

test "a failed execution reports fleet_error; a clean one reports processed" {
    // The verdict→wire projection moved onto the conversion pair, which owns
    // its own round-trip tests; loop keeps only the token-split width policy.
    try testing.expectEqual(protocol.Outcome.fleet_error, contract.report_mapping.toReport(.{}, REPORT_CTX).outcome);
    try testing.expectEqual(protocol.Outcome.processed, contract.report_mapping.toReport(contract.execution_result.ExecutionResult.completed(""), REPORT_CTX).outcome);
}

test "splitFields carries the final result's splits onto the report verbatim, beside the legacy total" {
    const result = contract.execution_result.ExecutionResult{
        .input_tokens = 10,
        .cached_input_tokens = 2,
        .output_tokens = 5,
        .token_count = 17,
    };
    const splits = loop.splitFields(result);
    try testing.expectEqual(@as(u32, 10), splits.input_tokens);
    try testing.expectEqual(@as(u32, 2), splits.cached_input_tokens);
    try testing.expectEqual(@as(u32, 5), splits.output_tokens);
    // The legacy total is untouched by the mapping — both ride the report.
    try testing.expectEqual(@as(u64, 17), result.token_count);
}

test "splitFields saturates the wire-frozen u32 fields instead of wrapping" {
    const result = contract.execution_result.ExecutionResult{
        .input_tokens = std.math.maxInt(u64),
        .output_tokens = @as(u64, std.math.maxInt(u32)) + 1,
    };
    const splits = loop.splitFields(result);
    try testing.expectEqual(@as(u32, std.math.maxInt(u32)), splits.input_tokens);
    try testing.expectEqual(@as(u32, 0), splits.cached_input_tokens);
    try testing.expectEqual(@as(u32, std.math.maxInt(u32)), splits.output_tokens);
}
