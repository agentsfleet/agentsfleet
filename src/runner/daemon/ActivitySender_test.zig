//! Slow-send and queue-bound behavior without a live control plane.

const std = @import("std");
const common = @import("common");
const ActivitySender = @import("ActivitySender.zig");
const dts = @import("deadline_test_support.zig");
const call_deadline = @import("call_deadline");

/// A drain budget far shorter than the production one, so the stall test is
/// quick and still tells the budget apart from the send cap.
const DRAIN_TEST_BUDGET_MS: u32 = 50;
/// Spare lease time just outside the renewal window, under one send cap.
const DRAIN_TEST_SPARE_MS: i64 = 500;

const Probe = struct {
    entered: common.Event = .{},
    release: common.Event = .{},
    mutex: common.Mutex = .{},
    sent: [5]u8 = undefined,
    deadlines: [5]u31 = undefined,
    count: usize = 0,

    fn send(ctx: *anyopaque, bytes: []const u8, deadline_ms: u31) void {
        const self: *Probe = @ptrCast(@alignCast(ctx));
        self.entered.set();
        self.release.timedWait(5 * std.time.ns_per_s) catch return;
        self.mutex.lock();
        defer self.mutex.unlock();
        self.sent[self.count] = bytes[0];
        self.deadlines[self.count] = deadline_ms;
        self.count += 1;
    }
};

test "a slow send does not block the reader and bounded batches stay ordered" {
    var deadlines: dts.TestScheduler = .{};
    defer deadlines.deinit();
    var probe = Probe{};
    defer probe.release.set();
    var sender = ActivitySender{
        .alloc = std.testing.allocator,
        .io = common.globalIo(),
        .sched = try deadlines.start(std.testing.allocator),
        .base_url = "http://127.0.0.1:9",
        .runner_token = "agt_rtest",
        .lease_id = "lease_test",
        .deadline_ms = 5_000,
        .test_hook = .{ .ctx = &probe, .send = Probe.send },
    };
    try sender.start();
    defer sender.finish();
    sender.enqueue("0");
    try probe.entered.timedWait(std.time.ns_per_s);
    for ([_][]const u8{ "1", "2", "3", "4", "5" }) |item| sender.enqueue(item);
    try std.testing.expectEqual(@as(usize, 4), sender.queued);
    probe.release.set();
    sender.finish();
    try std.testing.expectEqual(@as(usize, 5), probe.count);
    try std.testing.expectEqualSlices(u8, "01234", probe.sent[0..probe.count]);
    for (probe.deadlines[0..probe.count]) |deadline_ms| try std.testing.expectEqual(ActivitySender.SEND_DEADLINE_CAP_MS, deadline_ms);
    sender.enqueue("6");
    try std.testing.expectEqual(@as(usize, 5), probe.count);
}

test "an oversized batch never enters the bounded queue and counts as a drop" {
    var deadlines: dts.TestScheduler = .{};
    defer deadlines.deinit();
    var probe = Probe{};
    defer probe.release.set();
    var sender = ActivitySender{
        .alloc = std.testing.allocator,
        .io = common.globalIo(),
        .sched = try deadlines.start(std.testing.allocator),
        .base_url = "http://127.0.0.1:9",
        .runner_token = "agt_rtest",
        .lease_id = "lease_test",
        .deadline_ms = 5_000,
        .test_hook = .{ .ctx = &probe, .send = Probe.send },
    };
    try sender.start();
    defer sender.finish();
    const oversized = try std.testing.allocator.alloc(u8, ActivitySender.MAX_BATCH_BYTES + 1);
    defer std.testing.allocator.free(oversized);
    sender.enqueue(oversized);
    try std.testing.expectEqual(@as(usize, 0), sender.queued);
    // Refused whole, like a full queue: the operator's drop total must see it,
    // or a lost batch leaves no trace anywhere.
    try std.testing.expectEqual(@as(u32, 1), sender.dropped);
}

test "two leases send independently when one control plane call stalls" {
    var deadlines: dts.TestScheduler = .{};
    defer deadlines.deinit();
    const sched = try deadlines.start(std.testing.allocator);
    var blocked = Probe{};
    defer blocked.release.set();
    var ready = Probe{};
    ready.release.set();
    var first = ActivitySender{
        .alloc = std.testing.allocator,
        .io = common.globalIo(),
        .sched = sched,
        .base_url = "http://127.0.0.1:9",
        .runner_token = "agt_rtest",
        .lease_id = "lease_one",
        .deadline_ms = 5_000,
        .test_hook = .{ .ctx = &blocked, .send = Probe.send },
    };
    var second = ActivitySender{
        .alloc = std.testing.allocator,
        .io = common.globalIo(),
        .sched = sched,
        .base_url = "http://127.0.0.1:9",
        .runner_token = "agt_rtest",
        .lease_id = "lease_two",
        .deadline_ms = 5_000,
        .test_hook = .{ .ctx = &ready, .send = Probe.send },
    };
    try first.start();
    defer first.finish();
    try second.start();
    defer second.finish();
    first.enqueue("a");
    try blocked.entered.timedWait(std.time.ns_per_s);
    second.enqueue("b");
    try ready.entered.timedWait(std.time.ns_per_s);
    second.finish();
    try std.testing.expectEqualSlices(u8, "b", ready.sent[0..ready.count]);
    try std.testing.expectEqual(@as(usize, 0), blocked.count);
    blocked.release.set();
    first.finish();
    try std.testing.expectEqualSlices(u8, "a", blocked.sent[0..blocked.count]);
}

test "two four and sixteen leases can all send while every control plane call stalls" {
    for ([_]usize{ 2, 4, 16 }) |active| {
        var deadlines: dts.TestScheduler = .{};
        defer deadlines.deinit();
        const sched = try deadlines.start(std.testing.allocator);
        var probes: [16]Probe = undefined;
        var senders: [16]ActivitySender = undefined;
        var started: usize = 0;
        defer {
            for (probes[0..started]) |*probe| probe.release.set();
            for (senders[0..started]) |*sender| sender.finish();
        }
        for (0..active) |at| {
            probes[at] = .{};
            senders[at] = .{
                .alloc = std.testing.allocator,
                .io = common.globalIo(),
                .sched = sched,
                .base_url = "http://127.0.0.1:9",
                .runner_token = "agt_rtest",
                .lease_id = "lease_test",
                .deadline_ms = 5_000,
                .test_hook = .{ .ctx = &probes[at], .send = Probe.send },
            };
            try senders[at].start();
            started += 1;
        }
        for (senders[0..started]) |*sender| sender.enqueue("x");
        // Every sender must enter its own blocked POST before any is released.
        // A shared send mutex would leave the second waiter timed out here.
        for (probes[0..started]) |*probe| try probe.entered.timedWait(2 * std.time.ns_per_s);
        for (probes[0..started]) |*probe| probe.release.set();
        for (senders[0..started]) |*sender| sender.finish();
        for (probes[0..started]) |*probe| {
            try std.testing.expectEqual(@as(usize, 1), probe.count);
            try std.testing.expectEqual(@as(u8, 'x'), probe.sent[0]);
        }
    }
}

fn testSender(sched: *call_deadline.ProcessScheduler, probe: *Probe) ActivitySender {
    return .{
        .alloc = std.testing.allocator,
        .io = common.globalIo(),
        .sched = sched,
        .base_url = "http://127.0.0.1:9",
        .runner_token = "agt_rtest",
        .lease_id = "lease_test",
        .deadline_ms = 5_000,
        .test_hook = .{ .ctx = probe, .send = Probe.send },
    };
}

test "drainFor returns once every queued batch is posted" {
    var deadlines: dts.TestScheduler = .{};
    defer deadlines.deinit();
    var probe = Probe{};
    probe.release.set();
    var sender = testSender(try deadlines.start(std.testing.allocator), &probe);
    try sender.start();
    defer sender.finish();
    for ([_][]const u8{ "a", "b", "c" }) |item| sender.enqueue(item);
    sender.drainFor(ActivitySender.DRAIN_BEFORE_REPORT_MS);
    // Posted before the report would go out: that is the whole point of draining.
    probe.mutex.lock();
    defer probe.mutex.unlock();
    try std.testing.expectEqualSlices(u8, "abc", probe.sent[0..probe.count]);
}

test "drainFor gives up at its budget when a send stalls" {
    var deadlines: dts.TestScheduler = .{};
    defer deadlines.deinit();
    var probe = Probe{};
    defer probe.release.set();
    var sender = testSender(try deadlines.start(std.testing.allocator), &probe);
    try sender.start();
    defer sender.finish();
    sender.enqueue("0");
    try probe.entered.timedWait(std.time.ns_per_s);
    const started_ms = common.clock.nowMonotonicMillis();
    sender.drainFor(DRAIN_TEST_BUDGET_MS);
    const waited_ms = common.clock.nowMonotonicMillis() - started_ms;
    // A stalled control plane holds the report for the budget, not the send cap.
    try std.testing.expect(waited_ms >= DRAIN_TEST_BUDGET_MS);
    try std.testing.expect(waited_ms < ActivitySender.DRAIN_BEFORE_REPORT_MS);
    try std.testing.expectEqual(@as(usize, 0), probe.count);
}

test "drainFor on a sender that never started returns at once" {
    var probe = Probe{};
    var deadlines: dts.TestScheduler = .{};
    defer deadlines.deinit();
    var sender = testSender(try deadlines.start(std.testing.allocator), &probe);
    const started_ms = common.clock.nowMonotonicMillis();
    sender.drainFor(ActivitySender.DRAIN_BEFORE_REPORT_MS);
    try std.testing.expect(common.clock.nowMonotonicMillis() - started_ms < DRAIN_TEST_BUDGET_MS);
}

test "the report drain never reaches into the lease's renewal window" {
    const now_ms: i64 = 1_800_000_000_000;
    const window_ms = common.RENEWAL_WINDOW_MS;
    // A healthy lease has far more than the window left: the full send cap.
    try std.testing.expectEqual(ActivitySender.DRAIN_BEFORE_REPORT_MS, ActivitySender.drainBudgetMs(now_ms + 3 * window_ms, now_ms));
    // Half a second of spare time outside the window: only that half second.
    try std.testing.expectEqual(@as(u32, DRAIN_TEST_SPARE_MS), ActivitySender.drainBudgetMs(now_ms + window_ms + DRAIN_TEST_SPARE_MS, now_ms));
    // Inside the window, or already expired: report at once.
    try std.testing.expectEqual(@as(u32, 0), ActivitySender.drainBudgetMs(now_ms + window_ms, now_ms));
    try std.testing.expectEqual(@as(u32, 0), ActivitySender.drainBudgetMs(now_ms - window_ms, now_ms));
    // A garbage deadline from the wire saturates instead of trapping.
    try std.testing.expectEqual(ActivitySender.DRAIN_BEFORE_REPORT_MS, ActivitySender.drainBudgetMs(std.math.maxInt(i64), now_ms));
    try std.testing.expectEqual(@as(u32, 0), ActivitySender.drainBudgetMs(std.math.minInt(i64), now_ms));
}
