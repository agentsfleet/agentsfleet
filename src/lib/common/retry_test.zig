//! The shared retry loop, driven with a scripted call and a fake clock: no
//! socket and no real pause, so the budget arithmetic is exact.

const std = @import("std");
const testing = std.testing;

const retry = @import("retry.zig");

const FetchError = error{ Transient, Refused };
const VALUE = "payload";
/// Stand-in for `backoff.ms(attempt)`'s first step, un-jittered.
const DELAY_MS: u64 = 2_000;
const BUDGET_MS: u64 = 5_000;
const MAX_ATTEMPTS: u32 = 3;
/// An attempt that ran out a 10 s deadline before failing.
const SLOW_FAILURE_MS: u64 = 10_000;

fn onlyTransient(err: anyerror) bool {
    return err == error.Transient;
}

const POLICY = retry.Policy{ .max_attempts = MAX_ATTEMPTS, .budget_ms = BUDGET_MS, .retryable = onlyTransient };

const Clock = struct {
    now_ms: u64 = 0,
    slept_ms: u64 = 0,

    pub fn nowMs(self: *Clock) u64 {
        return self.now_ms;
    }
    pub fn delayMs(_: *Clock, _: u32) u64 {
        return DELAY_MS;
    }
    pub fn sleepMs(self: *Clock, ms: u64) void {
        self.now_ms += ms;
        self.slept_ms += ms;
    }
};

/// Plays back one outcome per attempt and advances the shared clock by each
/// attempt's cost, so a slow failure is visible to the budget.
const Script = struct {
    outcomes: []const ?FetchError,
    costs_ms: []const u64,
    clock: *Clock,
    calls: usize = 0,
    retries: usize = 0,

    pub fn fetch(self: *Script) FetchError![]const u8 {
        const at = self.calls;
        self.calls += 1;
        self.clock.now_ms += self.costs_ms[at];
        if (self.outcomes[at]) |err| return err;
        return VALUE;
    }

    pub fn onRetry(self: *Script, _: u32, _: anyerror, _: u64) void {
        self.retries += 1;
    }
};

test "a transient failure is retried and the second attempt's value is returned" {
    var clock: Clock = .{};
    var script = Script{ .outcomes = &.{ error.Transient, null }, .costs_ms = &.{ 0, 0 }, .clock = &clock };
    try testing.expectEqualStrings(VALUE, try retry.run(POLICY, &script, &clock));
    try testing.expectEqual(@as(usize, 2), script.calls);
    try testing.expectEqual(@as(usize, 1), script.retries);
    try testing.expectEqual(DELAY_MS, clock.slept_ms);
}

test "a first-try success never sleeps" {
    var clock: Clock = .{};
    var script = Script{ .outcomes = &.{null}, .costs_ms = &.{0}, .clock = &clock };
    _ = try retry.run(POLICY, &script, &clock);
    try testing.expectEqual(@as(usize, 1), script.calls);
    try testing.expectEqual(@as(u64, 0), clock.slept_ms);
}

test "an error the policy does not call retryable returns at once" {
    var clock: Clock = .{};
    var script = Script{ .outcomes = &.{ error.Refused, null }, .costs_ms = &.{ 0, 0 }, .clock = &clock };
    try testing.expectError(error.Refused, retry.run(POLICY, &script, &clock));
    try testing.expectEqual(@as(usize, 1), script.calls);
    try testing.expectEqual(@as(usize, 0), script.retries);
}

test "a failure that already spent the budget is not repeated" {
    var clock: Clock = .{};
    var script = Script{ .outcomes = &.{ error.Transient, null }, .costs_ms = &.{ SLOW_FAILURE_MS, 0 }, .clock = &clock };
    try testing.expectError(error.Transient, retry.run(POLICY, &script, &clock));
    try testing.expectEqual(@as(usize, 1), script.calls);
    try testing.expectEqual(@as(u64, 0), clock.slept_ms);
}

test "fast failures stop at the attempt cap" {
    // 0 ms fail, pause to 2 s, 0 ms fail, pause to 4 s — still inside 5 s — so
    // the third attempt runs; after it the cap refuses a fourth.
    var clock: Clock = .{};
    const fail = error.Transient;
    var script = Script{ .outcomes = &.{ fail, fail, fail, null }, .costs_ms = &.{ 0, 0, 0, 0 }, .clock = &clock };
    try testing.expectError(error.Transient, retry.run(POLICY, &script, &clock));
    try testing.expectEqual(@as(usize, MAX_ATTEMPTS), script.calls);
    try testing.expectEqual(@as(usize, MAX_ATTEMPTS - 1), script.retries);
}

test "Policy.allows: the budget refuses a pause that would end past it" {
    try testing.expect(POLICY.allows(error.Transient, 0, 0, DELAY_MS));
    try testing.expect(!POLICY.allows(error.Transient, 0, BUDGET_MS - DELAY_MS, DELAY_MS));
    try testing.expect(!POLICY.allows(error.Transient, MAX_ATTEMPTS - 1, 0, 0));
    // Saturating: an absurd elapsed value or attempt cannot wrap into a retry.
    try testing.expect(!POLICY.allows(error.Transient, 0, std.math.maxInt(u64), DELAY_MS));
    try testing.expect(!POLICY.allows(error.Transient, std.math.maxInt(u32), 0, 0));
}

test "RealPacer reads a non-zero clock, a bounded backoff step, and sleeps" {
    const pacer = retry.RealPacer{ .io = std.testing.io };
    try testing.expect(pacer.nowMs() > 0);
    try testing.expect(pacer.delayMs(0) > 0);
    const before = pacer.nowMs();
    pacer.sleepMs(1);
    try testing.expect(pacer.nowMs() >= before);
}
