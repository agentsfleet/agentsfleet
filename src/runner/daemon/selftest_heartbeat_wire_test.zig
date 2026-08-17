//! The self-test round trip as it crosses the wire.
//!
//! This is the seam the milestone was missing: the dashboard stamped an ask,
//! the daemon put `selftest_requested` on the reply, and the runner read
//! neither — so the control recorded a request nothing ever answered. These
//! tests pin BOTH directions against a real socket, so a future edit that drops
//! the field from the request body or the reply struct fails here instead of
//! shipping a button that spins forever.
//!
//! Real HTTP over loopback, no bubblewrap: the verdict is constructed rather
//! than probed, because what is under test is the transport, not the sandbox.

const std = @import("std");
const testing = std.testing;
const common = @import("common");
const contract = @import("contract");
const protocol = contract.protocol;

const client = @import("control_plane_client.zig");
const dts = @import("deadline_test_support.zig");
const plane_stub = @import("../cmd/plane_stub_test.zig");
const selftest = @import("../selftest.zig");
const selftest_beat = @import("selftest_beat.zig");

/// Reply carrying an operator's ask, so the same exchange proves the DOWN
/// direction: a runner that cannot read this field never probes at all.
const REPLY_ASKING = "{\"status\":\"ok\",\"selftest_requested\":true}";

/// Captures one heartbeat's request body. Mirrors `LeaseBodyStub` in the
/// sibling client test; the buffer is larger because a verdict carries prose
/// details, not three integers.
const HeartbeatBodyStub = struct {
    io: std.Io,
    listener: *std.Io.net.Server,
    body_buf: [4096]u8 = [_]u8{0} ** 4096,
    body_len: usize = 0,

    fn run(self: *HeartbeatBodyStub) void {
        const conn = self.listener.accept(self.io) catch return;
        defer conn.close(self.io);
        var request_buf: [8192]u8 = undefined;
        var total: usize = 0;
        var header_end: usize = 0;
        while (true) {
            if (std.mem.indexOf(u8, request_buf[0..total], "\r\n\r\n")) |index| {
                header_end = index + 4;
                break;
            }
            const read = std.posix.read(conn.socket.handle, request_buf[total..]) catch return;
            if (read == 0) return;
            total += read;
            if (total == request_buf.len) return;
        }
        const content_len = parseContentLength(request_buf[0..header_end]) orelse 0;
        while (total < header_end + content_len) {
            const read = std.posix.read(conn.socket.handle, request_buf[total..]) catch return;
            if (read == 0) return;
            total += read;
        }
        const body = request_buf[header_end .. header_end + content_len];
        if (body.len <= self.body_buf.len) {
            @memcpy(self.body_buf[0..body.len], body);
            self.body_len = body.len;
        }
        var response_buf: [256]u8 = undefined;
        var writer = conn.writer(self.io, &response_buf);
        writer.interface.print(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {d}\r\nConnection: close\r\n\r\n{s}",
            .{ REPLY_ASKING.len, REPLY_ASKING },
        ) catch return;
        writer.interface.flush() catch return;
    }
};

/// Case-insensitive, same shape as the sibling client test's — the client sets
/// the header's case, and a stub that only matched one spelling would silently
/// read a zero-length body and pass on an empty beat.
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

const Exchange = struct {
    body: []const u8,
    reply: std.json.Parsed(@import("AppliedPolicy.zig").HeartbeatReplyRaw),
};

/// Beat once against a capturing stub and hand back both directions.
fn beat(alloc: std.mem.Allocator, stub: *HeartbeatBodyStub, report: ?protocol.SelftestReport) !Exchange {
    const io = common.globalIo();
    var address = try std.Io.net.IpAddress.parseIp4("127.0.0.1", 0);
    var listener = address.listen(io, .{ .reuse_address = true }) catch return error.SkipZigTest;
    defer listener.deinit(io);
    const port = plane_stub.boundPort(listener.socket.handle) catch return error.SkipZigTest;

    stub.* = .{ .io = io, .listener = &listener };
    const responder = std.Thread.spawn(.{}, HeartbeatBodyStub.run, .{stub}) catch return error.SkipZigTest;

    var url_buf: [48]u8 = undefined;
    const url = try std.fmt.bufPrint(&url_buf, "http://127.0.0.1:{d}", .{port});
    var deadlines: dts.TestScheduler = .{};
    defer deadlines.deinit();
    var c = client.init(alloc, io, try deadlines.start(alloc), url);
    defer c.deinit();

    const reply = try c.heartbeat(alloc, "agt_rtest", 5_000, null, report);
    responder.join();
    return .{ .body = stub.body_buf[0..stub.body_len], .reply = reply };
}

test "test_startup_probe_reports_on_first_heartbeat" {
    // Dimension 2.6's wire half: a verdict the daemon holds at boot rides the
    // very next beat. Before this existed the runner had no way to send one at
    // all — `heartbeat` took only a capability report.
    const alloc = testing.allocator;
    const checks = try alloc.alloc(selftest.Check, 1);
    defer alloc.free(checks);
    checks[0] = .{
        .name = selftest.CHECK_RESOLVER,
        .ok = false,
        .detail = selftest.DETAIL_RESOLVER_DANGLING,
    };
    var pending = selftest_beat.Pending.init(alloc);
    pending.result = .{
        .checks = checks,
        .network_policy = .allow_all,
        .sandbox_tier = .landlock_full,
    };
    // Ownership stays with `checks` above — clear the holder without freeing.
    defer pending.result = null;

    var stub: HeartbeatBodyStub = undefined;
    const x = try beat(alloc, &stub, pending.report());
    defer x.reply.deinit();

    try testing.expect(x.body.len > 0);
    const sent = try std.json.parseFromSlice(protocol.HeartbeatRequest, alloc, x.body, .{});
    defer sent.deinit();
    const verdict = sent.value.selftest orelse return error.SelftestMissingFromBeat;
    try testing.expect(!verdict.all_ok);
    try testing.expectEqual(@as(usize, 1), verdict.checks.len);
    try testing.expectEqualStrings(selftest.CHECK_RESOLVER, verdict.checks[0].name);
    // The mechanism reaches the operator, not just a red dot.
    try testing.expectEqualStrings(selftest.DETAIL_RESOLVER_DANGLING, verdict.checks[0].detail);
    // The policy travels with it so the page can call a stale result stale.
    try testing.expectEqualStrings("allow_all", verdict.network_policy);

    // Down direction: the ask the operator recorded reaches the runner.
    try testing.expect(x.reply.value.selftest_requested);
}

test "a beat with no verdict carries no self-test at all" {
    // Sending an empty report every tick would overwrite the stored verdict and
    // wipe the last real one — the panel would blank between probes.
    const alloc = testing.allocator;
    var stub: HeartbeatBodyStub = undefined;
    const x = try beat(alloc, &stub, null);
    defer x.reply.deinit();

    const sent = try std.json.parseFromSlice(protocol.HeartbeatRequest, alloc, x.body, .{});
    defer sent.deinit();
    try testing.expectEqual(@as(?protocol.SelftestReport, null), sent.value.selftest);
}

test "a control plane that never sends the field simply never asks" {
    // An older daemon omits `selftest_requested` entirely. It must decode false
    // rather than fail the beat — a runner that cannot parse a reply stops
    // heartbeating, which is a far worse outage than a missing self-test.
    const alloc = testing.allocator;
    const parsed = try std.json.parseFromSlice(
        @import("AppliedPolicy.zig").HeartbeatReplyRaw,
        alloc,
        "{\"status\":\"ok\"}",
        .{ .ignore_unknown_fields = true },
    );
    defer parsed.deinit();
    try testing.expect(!parsed.value.selftest_requested);
}
