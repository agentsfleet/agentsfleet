//! Edge/boundary tests for `renew_driver.onTick`: a server-issued deadline at or
//! before `now_ms`, a pathological starting deadline near `i64` min (window math
//! must not overflow/panic), and the supervisor-level timeout that protects a
//! lease when renewal fails transiently on every tick. A scripted fake client
//! and an injected `now_ms` keep every branch deterministic — no HTTP, no clock.

const std = @import("std");
const clock = @import("common").clock;
const testing = std.testing;
const client = @import("control_plane_client.zig");
const child_supervisor = @import("../child_supervisor.zig");
const pipe_proto = @import("../pipe_proto.zig");
const renew_driver = @import("renew_driver.zig");
const contract = @import("contract");

const RenewDecision = child_supervisor.RenewDecision;
const ActivitySink = child_supervisor.ActivitySink;
const ActivityFrame = contract.activity.ActivityFrame;
const Driver = renew_driver.RenewDriver(*FakeClient);

const NOW_MS: i64 = 1_900_000_000_000;

const FakeClient = struct {
    outcome: client.ClientError!client.RenewResult,
    calls: usize = 0,
    pub fn renew(self: *FakeClient, _: std.mem.Allocator, _: []const u8, _: []const u8) client.ClientError!client.RenewResult {
        self.calls += 1;
        return self.outcome;
    }
};

fn driverWith(fake: *FakeClient, deadline_ms: i64) Driver {
    return .{
        .alloc = testing.allocator,
        .cp = fake,
        .runner_token = "zrn_test",
        .lease_id = "lease_test",
        .deadline_ms = deadline_ms,
    };
}

test "onTick should treat a server deadline at or before now_ms as fail-safe at the supervisor boundary" {
    // The driver trusts the control plane's deadline verbatim (the server is
    // authoritative), so a returned deadline == now_ms is applied as-is. The
    // fail-safe is structural: the supervisor's readResult compares now >=
    // deadline next tick and times out rather than running forever. Here we pin
    // the contract that the decision carries exactly the server value and never
    // silently advances PAST it — an at/before-now deadline can only shrink the
    // window, never extend a lease the server is trying to end.
    var fake = FakeClient{ .outcome = .{ .renewed = NOW_MS } };
    var driver = driverWith(&fake, NOW_MS + 1_000); // inside the window
    const h = driver.hook();

    const decision = h.onTick(h.ctx, NOW_MS);
    try testing.expectEqual(RenewDecision{ .extend = NOW_MS }, decision);
    try testing.expectEqual(NOW_MS, driver.deadline_ms); // never advanced past the server value
    // The applied deadline is not in the future → next supervisor tick times out.
    try testing.expect(driver.deadline_ms <= NOW_MS);
}

test "onTick should handle a near-minimum deadline_ms without arithmetic panic" {
    // A pathological starting deadline near i64 min with now_ms far in the
    // future: `deadline_ms - now_ms` must not underflow/panic. The huge negative
    // difference is well below the window, so the driver renews (it is far past
    // due) and applies the server's fresh deadline.
    const fresh = NOW_MS + 100_000;
    var fake = FakeClient{ .outcome = .{ .renewed = fresh } };
    var driver = driverWith(&fake, std.math.minInt(i64) + 1);
    const h = driver.hook();

    const decision = h.onTick(h.ctx, NOW_MS);
    try testing.expectEqual(RenewDecision{ .extend = fresh }, decision);
    try testing.expectEqual(fresh, driver.deadline_ms);
    try testing.expectEqual(@as(usize, 1), fake.calls);
}

test "readResult should time out when renewal fails transiently on every tick" {
    // The lease deadline is 500ms out; renewal ticks fire on a small cadence and
    // every attempt fails transiently (RequestFailed → .keep), so the deadline is
    // never advanced. Once it elapses the supervisor must report timed_out — the
    // fail-safe contract: renewal that never succeeds lets the lease expire, it is
    // never silently kept alive. No writer ever sends a frame, the read blocks,
    // ticks fire, and the deadline wins.
    var fake = FakeClient{ .outcome = client.ClientError.RequestFailed };
    var driver = driverWith(&fake, 0); // overwritten below to a live near deadline
    const near_dl = clock.nowMillis() + 500;
    driver.deadline_ms = near_dl;
    var hook = driver.hook();
    hook.tick_ms = 100; // 4–5 ticks before the deadline

    const fds = try pipe_proto.osPipe();
    defer pipe_proto.osClose(fds[0]);
    defer pipe_proto.osClose(fds[1]); // write end open → no EOF; the read blocks to the deadline

    const Noop = struct {
        fn forward(_: *anyopaque, _: ActivityFrame) void {}
    };
    var dummy: u8 = 0;
    const sink = ActivitySink{ .ctx = &dummy, .forward = Noop.forward };

    const outcome = try child_supervisor.readResult(testing.allocator, fds[0], near_dl, sink, hook);
    defer testing.allocator.free(outcome.bytes);

    try testing.expect(outcome.timed_out);
    try testing.expect(!outcome.terminated);
    try testing.expect(fake.calls >= 1); // it DID attempt to renew, repeatedly
}
