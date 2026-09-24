//! Slow-send and queue-bound behavior without a live control plane.

const std = @import("std");
const common = @import("common");
const ActivitySender = @import("ActivitySender.zig");
const dts = @import("deadline_test_support.zig");

const Probe = struct {
    entered: common.Event = .{},
    release: common.Event = .{},
    mutex: common.Mutex = .{},
    sent: [5]u8 = undefined,
    count: usize = 0,

    fn send(ctx: *anyopaque, bytes: []const u8) void {
        const self: *Probe = @ptrCast(@alignCast(ctx));
        self.entered.set();
        self.release.timedWait(5 * std.time.ns_per_s) catch return;
        self.mutex.lock();
        defer self.mutex.unlock();
        self.sent[self.count] = bytes[0];
        self.count += 1;
    }
};

test "a slow send does not block the reader and bounded batches stay ordered" {
    var deadlines: dts.TestScheduler = .{};
    defer deadlines.deinit();
    var probe = Probe{};
    defer probe.release.set();
    var sender = ActivitySender{
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
    sender.enqueue("6");
    try std.testing.expectEqual(@as(usize, 5), probe.count);
}

test "an oversized batch never enters the bounded queue" {
    var deadlines: dts.TestScheduler = .{};
    defer deadlines.deinit();
    var sender = ActivitySender{
        .io = common.globalIo(),
        .sched = try deadlines.start(std.testing.allocator),
        .base_url = "http://127.0.0.1:9",
        .runner_token = "agt_rtest",
        .lease_id = "lease_test",
        .deadline_ms = 5_000,
    };
    const oversized = try std.testing.allocator.alloc(u8, ActivitySender.MAX_BATCH_BYTES + 1);
    defer std.testing.allocator.free(oversized);
    sender.enqueue(oversized);
    try std.testing.expectEqual(@as(usize, 0), sender.queued);
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
        .io = common.globalIo(),
        .sched = sched,
        .base_url = "http://127.0.0.1:9",
        .runner_token = "agt_rtest",
        .lease_id = "lease_one",
        .deadline_ms = 5_000,
        .test_hook = .{ .ctx = &blocked, .send = Probe.send },
    };
    var second = ActivitySender{
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
