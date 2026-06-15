//! Multi-tick / repeated-call tests for `renew_driver.onTick`: the driver holds
//! a single `deadline_ms` and re-derives its decision on every tick, so these
//! exercise the cumulative effect of consecutive renewals and the behaviour of a
//! tick that arrives AFTER a terminal decision. A scripted fake client makes
//! each tick's outcome deterministic with no HTTP and an injected `now_ms`.

const std = @import("std");
const testing = std.testing;
const constants = @import("common");
const client = @import("control_plane_client.zig");
const child_supervisor = @import("../child_supervisor.zig");
const renew_driver = @import("renew_driver.zig");
const contract = @import("contract");
const MS_PER_SECOND = 1_000;

const RenewDecision = child_supervisor.RenewDecision;
const Driver = renew_driver.RenewDriver(*FakeClient);

const NOW_MS: i64 = 1_900_000_000_000;

// A scripted client that walks a queue of outcomes (one per renewal attempt) and
// counts calls, so a test can assert both the cumulative deadline AND exactly how
// many times the driver reached out.
const FakeClient = struct {
    outcomes: []const (client.ClientError!client.RenewResult),
    idx: usize = 0,
    calls: usize = 0,
    pub fn renew(self: *FakeClient, _: std.mem.Allocator, _: []const u8, _: []const u8, _: contract.protocol.RenewRequest, _: u31) client.ClientError!client.RenewResult {
        self.calls += 1;
        if (self.idx >= self.outcomes.len) return self.outcomes[self.outcomes.len - 1];
        const o = self.outcomes[self.idx];
        self.idx += 1;
        return o;
    }
};

fn driverWith(fake: *FakeClient, deadline_ms: i64) Driver {
    return .{
        .alloc = testing.allocator,
        .cp = fake,
        .runner_token = "agt_rtest",
        .lease_id = "lease_test",
        .deadline_ms = deadline_ms,
        .renew_deadline_ms = client.RENEW_DEADLINE_MS,
    };
}

test "onTick should accumulate deadline extensions across multiple consecutive renewal ticks" {
    // Three ticks, each inside the window, each returning a fresh extended
    // deadline. The driver must advance through every renewal and finish on the
    // last value — never stop at the first.
    const d1 = NOW_MS + 100_000;
    const d2 = NOW_MS + 200_000;
    const d3 = NOW_MS + 300_000;
    var fake = FakeClient{ .outcomes = &.{
        .{ .renewed = d1 },
        .{ .renewed = d2 },
        .{ .renewed = d3 },
    } };
    var driver = driverWith(&fake, NOW_MS + MS_PER_SECOND); // 1s left → inside the window
    const h = driver.hook();

    try testing.expectEqual(RenewDecision{ .extend = d1 }, h.onTick(h.ctx, NOW_MS, .{}));
    try testing.expectEqual(d1, driver.deadline_ms);
    try testing.expectEqual(RenewDecision{ .extend = d2 }, h.onTick(h.ctx, d1 - MS_PER_SECOND, .{}));
    try testing.expectEqual(d2, driver.deadline_ms);
    try testing.expectEqual(RenewDecision{ .extend = d3 }, h.onTick(h.ctx, d2 - MS_PER_SECOND, .{}));

    try testing.expectEqual(d3, driver.deadline_ms); // final deadline is the last renewal
    try testing.expectEqual(@as(usize, 3), fake.calls);
}

test "onTick should remain safe after a terminal renewal even when called again" {
    // First tick (inside the window) returns terminal → .terminate. A late/extra
    // tick must stay fail-safe: the driver has no terminal latch, so a second
    // tick OUTSIDE the window short-circuits to .keep without re-calling the
    // client — proving a late tick after termination cannot corrupt state or
    // re-enter the client when the deadline is comfortably far.
    var fake = FakeClient{ .outcomes = &.{.{ .terminal = 402 }} };
    var driver = driverWith(&fake, NOW_MS + MS_PER_SECOND);
    const h = driver.hook();

    try testing.expectEqual(RenewDecision.terminate, h.onTick(h.ctx, NOW_MS, .{}));
    try testing.expectEqual(@as(usize, 1), fake.calls);
    try testing.expectEqual(NOW_MS + MS_PER_SECOND, driver.deadline_ms); // terminal left it unchanged

    // A late tick well outside the renewal window: short-circuits, no second call.
    const far_now = NOW_MS + 1_000 - constants.RENEWAL_WINDOW_MS - 60_000;
    try testing.expectEqual(RenewDecision.keep, h.onTick(h.ctx, far_now, .{}));
    try testing.expectEqual(@as(usize, 1), fake.calls); // no double-call to fake.renew
}
