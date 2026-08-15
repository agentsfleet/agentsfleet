//! The four control-plane verbs no test ever drove end-to-end: `report`,
//! `memoryHydrate`, `memoryCapture`, and the best-effort `activityFramesJson`.
//!
//! `heartbeat`/`getSelf` already ride the deadline and keep-alive suites; these
//! four carried the run's DURABLE outcomes — the terminal report and the
//! fleet's memory — with zero executed lines. Each is pinned against a scripted
//! local plane: the 2xx path parses/settles, and a refusal maps to the typed
//! error the caller's retry logic branches on.

const std = @import("std");
const testing = std.testing;
const common = @import("common");
const protocol = @import("contract").protocol;

const client_mod = @import("control_plane_client.zig");
const dts = @import("deadline_test_support.zig");
const plane_stub = @import("../cmd/plane_stub_test.zig");

const ALLOC = testing.allocator;
const DEADLINE_MS: u31 = 2_000;

const Plane = struct {
    listener: std.Io.net.Server,
    stub: plane_stub.OneShotPlane,
    thread: std.Thread,
    url_buf: [48]u8,

    fn start(status: plane_stub.StubStatus) !*Plane {
        const io = common.globalIo();
        const self = try ALLOC.create(Plane);
        errdefer ALLOC.destroy(self);
        // SAFETY: url() overwrites the prefix it reads before any use.
        self.url_buf = undefined;
        var addr = try std.Io.net.IpAddress.parseIp4("127.0.0.1", 0);
        self.listener = addr.listen(io, .{ .reuse_address = true }) catch return error.SkipZigTest;
        self.stub = .{ .io = io, .listener = &self.listener, .status = status };
        self.thread = std.Thread.spawn(.{}, plane_stub.OneShotPlane.serve, .{&self.stub}) catch return error.SkipZigTest;
        return self;
    }

    fn url(self: *Plane) ![]const u8 {
        const port = try plane_stub.boundPort(self.listener.socket.handle);
        return std.fmt.bufPrint(&self.url_buf, "http://127.0.0.1:{d}", .{port});
    }

    fn stop(self: *Plane) void {
        self.thread.join();
        self.listener.deinit(common.globalIo());
        ALLOC.destroy(self);
    }
};

fn sampleReport() protocol.ReportRequest {
    return .{
        .lease_id = "lease-verb-probe",
        .event_id = "evt-verb-probe",
        .fencing_token = 5,
        .outcome = .processed,
        .response_text = "done",
        .tokens = 10,
        .telemetry = .{ .time_to_first_token_ms = 1, .wall_ms = 2 },
        .checkpoint = .{ .last_event_id = "evt-verb-probe", .last_response = "done" },
    };
}

test "report settles on a 2xx and surfaces a refusal as BadStatus" {
    {
        var plane = try Plane.start(.{ .line = "200 OK", .body = "{\"ok\":true}" });
        var deadlines: dts.TestScheduler = .{};
        defer deadlines.deinit();
        var c = client_mod.init(ALLOC, common.globalIo(), try deadlines.start(ALLOC), try plane.url());
        try c.report(ALLOC, "agt_rtest", sampleReport(), DEADLINE_MS);
        c.deinit();
        plane.stop();
    }
    {
        // A 500 is retryable-transient to the caller — BadStatus, not a settle.
        var plane = try Plane.start(.{ .line = "500 Internal Server Error", .body = "{}" });
        var deadlines: dts.TestScheduler = .{};
        defer deadlines.deinit();
        var c = client_mod.init(ALLOC, common.globalIo(), try deadlines.start(ALLOC), try plane.url());
        try testing.expectError(error.BadStatus, c.report(ALLOC, "agt_rtest", sampleReport(), DEADLINE_MS));
        c.deinit();
        plane.stop();
    }
}

test "memoryHydrate parses the window and owns its strings past the wire body" {
    var plane = try Plane.start(.{
        .line = "200 OK",
        .body = "{\"memory\":[{\"key\":\"learned\",\"content\":\"a fact\",\"category\":\"core\"}]}",
    });
    var deadlines: dts.TestScheduler = .{};
    defer deadlines.deinit();
    var c = client_mod.init(ALLOC, common.globalIo(), try deadlines.start(ALLOC), try plane.url());

    const parsed = try c.memoryHydrate(ALLOC, "agt_rtest", "fleet-1", DEADLINE_MS);
    defer parsed.deinit();
    try testing.expectEqual(@as(usize, 1), parsed.value.memory.len);
    try testing.expectEqualStrings("learned", parsed.value.memory[0].key);
    try testing.expectEqualStrings("core", parsed.value.memory[0].category);
    c.deinit();
    plane.stop();
}

test "memoryHydrate refuses a body that is not the hydrate shape" {
    var plane = try Plane.start(.{ .line = "200 OK", .body = "{\"unexpected\":true}" });
    var deadlines: dts.TestScheduler = .{};
    defer deadlines.deinit();
    var c = client_mod.init(ALLOC, common.globalIo(), try deadlines.start(ALLOC), try plane.url());

    try testing.expectError(error.MalformedResponse, c.memoryHydrate(ALLOC, "agt_rtest", "fleet-1", DEADLINE_MS));
    c.deinit();
    plane.stop();
}

test "memoryCapture fences the write body and settles on 2xx" {
    var plane = try Plane.start(.{ .line = "200 OK", .body = "{\"ok\":true}" });
    var deadlines: dts.TestScheduler = .{};
    defer deadlines.deinit();
    var c = client_mod.init(ALLOC, common.globalIo(), try deadlines.start(ALLOC), try plane.url());

    const deltas = [_]protocol.MemoryDelta{
        .{ .key = "learned", .content = "a fact", .category = "core" },
    };
    try c.memoryCapture(ALLOC, "agt_rtest", "fleet-1", .{
        .lease_id = "lease-verb-probe",
        .fencing_token = 5,
        .memory = &deltas,
    }, DEADLINE_MS);
    c.deinit();
    plane.stop();
}

test "activityFramesJson is best-effort: a 2xx forwards, a dead plane is silent" {
    {
        var plane = try Plane.start(.{ .line = "202 Accepted", .body = "" });
        var deadlines: dts.TestScheduler = .{};
        defer deadlines.deinit();
        var c = client_mod.init(ALLOC, common.globalIo(), try deadlines.start(ALLOC), try plane.url());
        c.activityFramesJson(ALLOC, "agt_rtest", "lease-verb-probe", "{\"fleet_response_chunk\":{\"text\":\"hi\"}}", DEADLINE_MS);
        c.deinit();
        plane.stop();
    }
    {
        // No listener at all: the verb must swallow the transport failure —
        // activity is cosmetic and a down plane must never disturb execution.
        var deadlines: dts.TestScheduler = .{};
        defer deadlines.deinit();
        var c = client_mod.init(ALLOC, common.globalIo(), try deadlines.start(ALLOC), "http://127.0.0.1:1");
        defer c.deinit();
        c.activityFramesJson(ALLOC, "agt_rtest", "lease-verb-probe", "{\"fleet_response_chunk\":{\"text\":\"hi\"}}", DEADLINE_MS);
    }
}
