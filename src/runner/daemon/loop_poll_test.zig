//! `pollAndProcess` arm proofs (loop.zig) — the worker-side lease poll short of
//! the execute path. Each refuse arm must idle WITHOUT touching the control
//! plane (a degraded or policy-less worker hammering a plane is the regression
//! these pin), the empty-lease reply must honour the server's retry hint, and a
//! lease transport error must back off bounded instead of crashing the worker.
//! The zeroed `backoff_ms` seam keeps every idle at milliseconds.

const std = @import("std");
const testing = std.testing;
const common = @import("common");
const contract = @import("contract");
const Config = @import("config.zig");
const loop = @import("loop.zig");
const AppliedPolicy = @import("AppliedPolicy.zig");
const client_mod = @import("control_plane_client.zig");
const dts = @import("deadline_test_support.zig");

const protocol = contract.protocol;
const ALLOC = testing.allocator;

const RUNNER_TOKEN = protocol.RUNNER_TOKEN_PREFIX ++ "a" ** 64;
const STORAGE_BASE = "/tmp/agentsfleet-m164-loop-poll-test";
/// One-worker assignment in the exact wire shape `AppliedPolicy.apply` decodes.
const POLICY_JSON =
    \\{"sandbox_tier":"dev_none","network_policy":"deny_all_egress","registry_allowlist":[],"worker_count":1}
;
const EMPTY_LEASE_BODY = "{\"lease\":null,\"retry_after_ms\":2}";
const HTTP_REQ_BUF_BYTES: usize = 4096;

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

const StubMode = enum { empty_lease, drop };

/// Serial loopback plane: counts accepts, then per mode answers the lease poll
/// with an empty lease + retry hint, or drops the connection (transport error).
/// Retired via `shutdown()` (stop flag + wake connect), never a bare deinit.
const PollStub = struct {
    io: std.Io,
    listener: *std.Io.net.Server,
    mode: StubMode,
    accepts: std.atomic.Value(u32) = std.atomic.Value(u32).init(0),
    stop: std.atomic.Value(bool) = std.atomic.Value(bool).init(false),

    fn run(self: *PollStub) void {
        while (true) {
            const conn = self.listener.accept(self.io) catch return;
            if (self.stop.load(.seq_cst)) {
                conn.close(self.io);
                return;
            }
            defer conn.close(self.io);
            _ = self.accepts.fetchAdd(1, .seq_cst);
            var buf: [HTTP_REQ_BUF_BYTES]u8 = undefined;
            var total: usize = 0;
            while (total < buf.len) {
                const n = std.posix.read(conn.socket.handle, buf[total..]) catch break;
                if (n == 0) break;
                total += n;
                if (std.mem.indexOf(u8, buf[0..total], "\r\n\r\n") != null) break;
            }
            if (self.mode == .drop) continue; // close with no response
            var wbuf: [256]u8 = undefined;
            var w = conn.writer(self.io, &wbuf);
            w.interface.print(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {d}\r\nConnection: close\r\n\r\n{s}",
                .{ EMPTY_LEASE_BODY.len, EMPTY_LEASE_BODY },
            ) catch return;
            w.interface.flush() catch return;
        }
    }

    fn shutdown(self: *PollStub, port: u16) void {
        self.stop.store(true, .seq_cst);
        var addr = std.Io.net.IpAddress.parseIp4("127.0.0.1", port) catch return;
        const stream = addr.connect(self.io, .{ .mode = .stream }) catch return;
        stream.close(self.io);
    }
};

fn applyOnePolicy(applied: *AppliedPolicy) !void {
    const pol = try std.json.parseFromSlice(std.json.Value, ALLOC, POLICY_JSON, .{});
    defer pol.deinit();
    try testing.expectEqual(AppliedPolicy.ApplyOutcome.applied, applied.apply(pol.value));
}

/// Drive one `pollAndProcess` against a live counting stub with the backoff
/// seam zeroed; returns the number of plane contacts the arm made.
fn runPollAgainstStub(mode: StubMode, applied: *AppliedPolicy, worker_index: u32) !u32 {
    const saved_backoff = loop.backoff_ms;
    loop.backoff_ms = zeroBackoff;
    defer loop.backoff_ms = saved_backoff;

    const io = common.globalIo();
    var addr = try std.Io.net.IpAddress.parseIp4("127.0.0.1", 0);
    var listener = addr.listen(io, .{ .reuse_address = true }) catch return error.SkipZigTest;
    const port = boundPort(listener.socket.handle) catch return error.SkipZigTest;
    var stub = PollStub{ .io = io, .listener = &listener, .mode = mode };
    const stub_thread = std.Thread.spawn(.{}, PollStub.run, .{&stub}) catch return error.SkipZigTest;

    var url_buf: [48]u8 = undefined;
    const url = try std.fmt.bufPrint(&url_buf, "http://127.0.0.1:{d}", .{port});
    var deadlines: dts.TestScheduler = .{};
    defer deadlines.deinit();
    var cp = client_mod.init(ALLOC, io, try deadlines.start(ALLOC), url);
    defer cp.deinit();

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

    loop.pollAndProcess(io, ALLOC, &cp, RUNNER_TOKEN, cfg, &env_map, applied, worker_index);

    stub.shutdown(port);
    stub_thread.join();
    listener.deinit(io);
    return stub.accepts.load(.seq_cst);
}

test "a degraded worker leases nothing — zero plane contact" {
    var applied = AppliedPolicy.init(ALLOC);
    defer applied.deinit();
    try applyOnePolicy(&applied);
    applied.setDegraded(true);
    // Invariant 2, runner half: an unmet assignment refuses even while a policy
    // is held — degraded wins over everything, and the plane never hears it.
    try testing.expectEqual(@as(u32, 0), try runPollAgainstStub(.empty_lease, &applied, 0));
}

test "a policy-less worker leases nothing — zero plane contact" {
    var applied = AppliedPolicy.init(ALLOC);
    defer applied.deinit();
    try testing.expectEqual(@as(u32, 0), try runPollAgainstStub(.empty_lease, &applied, 0));
}

test "a worker at or above the assigned count idles without polling (soft-shrink)" {
    var applied = AppliedPolicy.init(ALLOC);
    defer applied.deinit();
    try applyOnePolicy(&applied);
    // worker_count=1 → index 1 is above the assignment: the shrink half of a
    // worker-count change must idle, never race the still-assigned worker.
    try testing.expectEqual(@as(u32, 0), try runPollAgainstStub(.empty_lease, &applied, 1));
}

// NOT TESTED HERE — the snapshot-copy-failure arm (`applied.snapshot(alloc)`
// returning null while a policy IS held). Driving it needs an allocator that
// fails inside the deep copy, and a `FailingAllocator` on this path wedges the
// suite rather than returning: the arm is reached, but the run never completes.
// The neighbouring fail-closed behaviour it shares an idle branch with is
// covered by the no-policy case above, so the uncovered remainder is the copy
// itself. Worth revisiting with a purpose-built failing allocator that leaves
// the logging path alone.

test "an empty lease reply honours the server retry hint and returns" {
    var applied = AppliedPolicy.init(ALLOC);
    defer applied.deinit();
    try applyOnePolicy(&applied);
    // One poll, one reply, one bounded idle (retry_after_ms=2) — the worker
    // returns to its loop instead of spinning on an idle queue.
    try testing.expectEqual(@as(u32, 1), try runPollAgainstStub(.empty_lease, &applied, 0));
}

test "a lease transport error backs off bounded and returns to the worker loop" {
    var applied = AppliedPolicy.init(ALLOC);
    defer applied.deinit();
    try applyOnePolicy(&applied);
    // The stub accepts then drops the connection: a transport loss (never the
    // Unauthorized path) — the worker logs, backs off once, and returns.
    try testing.expectEqual(@as(u32, 1), try runPollAgainstStub(.drop, &applied, 0));
}
