//! Unit tests for `renew_driver.onTick` — the window/decision/deadline logic
//! that turns a control-plane renewal outcome into a supervisor decision. A
//! scripted fake client makes every branch deterministic with no HTTP and no
//! wall clock (the decision `now_ms` is injected): outside the window → keep
//! without calling out; renewed → extend + advance the kill deadline; a terminal
//! rejection → terminate; a transport/5xx failure → keep (retry next tick).

const std = @import("std");
const testing = std.testing;
const constants = @import("common");
const client = @import("control_plane_client.zig");
const child_supervisor = @import("../child_supervisor.zig");
const renew_driver = @import("renew_driver.zig");
const ONE_MILLION = 1_000_000;
const MS_PER_SECOND = 1_000;

const RenewDecision = child_supervisor.RenewDecision;
const Driver = renew_driver.RenewDriver(*FakeClient);

// A fixed epoch base so the window math is exact and reproducible.
const NOW_MS: i64 = 1_900_000_000_000;

// A scripted control-plane client: returns one queued renewal outcome and counts
// calls, so a test can assert both the decision AND whether onTick attempted a
// renewal at all (the window short-circuit must make zero calls).
const FakeClient = struct {
    outcome: client.ClientError!client.RenewResult,
    calls: usize = 0,
    pub fn renew(self: *FakeClient, _: std.mem.Allocator, _: []const u8, _: []const u8, _: u31) client.ClientError!client.RenewResult {
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
        .renew_deadline_ms = client.RENEW_DEADLINE_MS,
    };
}

test "onTick outside the renewal window keeps without calling the control plane" {
    var fake = FakeClient{ .outcome = .{ .renewed = NOW_MS + 999_999 } };
    // Deadline well beyond the window → no renewal attempt at all.
    var driver = driverWith(&fake, NOW_MS + constants.RENEWAL_WINDOW_MS + 60_000);
    const h = driver.hook();
    try testing.expectEqual(RenewDecision.keep, h.onTick(h.ctx, NOW_MS));
    try testing.expectEqual(@as(usize, 0), fake.calls);
    try testing.expectEqual(NOW_MS + constants.RENEWAL_WINDOW_MS + 60_000, driver.deadline_ms);
}

test "onTick at exactly the window boundary renews (boundary is inclusive)" {
    var fake = FakeClient{ .outcome = .{ .renewed = NOW_MS + ONE_MILLION } };
    var driver = driverWith(&fake, NOW_MS + constants.RENEWAL_WINDOW_MS);
    const h = driver.hook();
    _ = h.onTick(h.ctx, NOW_MS);
    try testing.expectEqual(@as(usize, 1), fake.calls); // exactly at the window edge → renews
}

test "onTick inside the window renews and advances the kill deadline" {
    const new_deadline = NOW_MS + 1_000_000;
    var fake = FakeClient{ .outcome = .{ .renewed = new_deadline } };
    var driver = driverWith(&fake, NOW_MS + MS_PER_SECOND); // 1s left → inside the window
    const h = driver.hook();
    try testing.expectEqual(RenewDecision{ .extend = new_deadline }, h.onTick(h.ctx, NOW_MS));
    try testing.expectEqual(new_deadline, driver.deadline_ms); // advanced to the renewed value
    try testing.expectEqual(@as(usize, 1), fake.calls);
}

test "onTick inside the window terminates on a terminal renewal and leaves the deadline" {
    var fake = FakeClient{ .outcome = .{ .terminal = 402 } };
    var driver = driverWith(&fake, NOW_MS + MS_PER_SECOND);
    const h = driver.hook();
    try testing.expectEqual(RenewDecision.terminate, h.onTick(h.ctx, NOW_MS));
    try testing.expectEqual(NOW_MS + MS_PER_SECOND, driver.deadline_ms); // unchanged
}

test "onTick keeps (and does not advance) when the renewal call fails transiently" {
    var fake = FakeClient{ .outcome = client.ClientError.RequestFailed };
    var driver = driverWith(&fake, NOW_MS + MS_PER_SECOND);
    const h = driver.hook();
    try testing.expectEqual(RenewDecision.keep, h.onTick(h.ctx, NOW_MS));
    try testing.expectEqual(NOW_MS + MS_PER_SECOND, driver.deadline_ms); // unchanged → next tick retries
    try testing.expectEqual(@as(usize, 1), fake.calls); // it DID attempt
}
