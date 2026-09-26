//! A full queue folds new batches into its newest slot instead of shedding
//! them. Reproduces the live loss: one send stalled ~1.5 s while the child kept
//! streaming 16-frame batches every ~180 ms, overflowing four slots.

const std = @import("std");
const common = @import("common");
const ActivitySender = @import("ActivitySender.zig");
const dts = @import("deadline_test_support.zig");
const call_deadline = @import("call_deadline");

/// Batches enqueued behind one stalled send: several seconds of streaming at
/// the observed cadence, far past what four separate slots can hold.
const STALLED_BATCHES: usize = 40;

/// Records every posted body whole, so a test can compare the wire stream.
const Recorder = struct {
    entered: common.Event = .{},
    release: common.Event = .{},
    mutex: common.Mutex = .{},
    bodies: std.ArrayList([]u8) = .empty,

    fn send(ctx: *anyopaque, bytes: []const u8, _: u31) void {
        const self: *Recorder = @ptrCast(@alignCast(ctx));
        self.entered.set();
        self.release.timedWait(5 * std.time.ns_per_s) catch return;
        const copy = std.testing.allocator.dupe(u8, bytes) catch return;
        self.mutex.lock();
        defer self.mutex.unlock();
        self.bodies.append(std.testing.allocator, copy) catch std.testing.allocator.free(copy);
    }

    fn deinit(self: *Recorder) void {
        for (self.bodies.items) |body| std.testing.allocator.free(body);
        self.bodies.deinit(std.testing.allocator);
    }

    /// Every posted body joined the way the wire joins frames.
    fn joined(self: *Recorder) ![]u8 {
        return std.mem.join(std.testing.allocator, ",", self.bodies.items);
    }
};

fn recordingSender(sched: *call_deadline.ProcessScheduler, recorder: *Recorder) ActivitySender {
    return .{
        .alloc = std.testing.allocator,
        .io = common.globalIo(),
        .sched = sched,
        .base_url = "http://127.0.0.1:9",
        .runner_token = "agt_rtest",
        .lease_id = "lease_fold",
        .deadline_ms = 5_000,
        .test_hook = .{ .ctx = recorder, .send = Recorder.send },
    };
}

test "a stalled send loses no frames: overflow folds into the newest batch in order" {
    var deadlines: dts.TestScheduler = .{};
    defer deadlines.deinit();
    var recorder = Recorder{};
    defer recorder.deinit();
    defer recorder.release.set();
    var sender = recordingSender(try deadlines.start(std.testing.allocator), &recorder);
    try sender.start();
    defer sender.finish();

    var expected: std.ArrayList(u8) = .empty;
    defer expected.deinit(std.testing.allocator);
    var name: [8]u8 = undefined;
    sender.enqueue("in-flight");
    try expected.appendSlice(std.testing.allocator, "in-flight");
    try recorder.entered.timedWait(std.time.ns_per_s);
    for (0..STALLED_BATCHES) |at| {
        const frame = try std.fmt.bufPrint(&name, "f{d:0>2}", .{at});
        sender.enqueue(frame);
        try expected.print(std.testing.allocator, ",{s}", .{frame});
    }
    try std.testing.expectEqual(ActivitySender.MAX_QUEUED_BATCHES, sender.queued);
    try std.testing.expectEqual(@as(u32, 0), sender.dropped);

    recorder.release.set();
    sender.finish();
    // One POST per slot, not per batch: the fold costs latency, never text.
    try std.testing.expectEqual(ActivitySender.MAX_QUEUED_BATCHES + 1, recorder.bodies.items.len);
    const wire = try recorder.joined();
    defer std.testing.allocator.free(wire);
    try std.testing.expectEqualStrings(expected.items, wire);
}

test "a fold that would overflow the newest slot is dropped and counted, an exact fit is kept" {
    var deadlines: dts.TestScheduler = .{};
    defer deadlines.deinit();
    var recorder = Recorder{};
    defer recorder.deinit();
    defer recorder.release.set();
    var sender = recordingSender(try deadlines.start(std.testing.allocator), &recorder);
    try sender.start();
    defer sender.finish();

    const tail_room: usize = 8;
    const big = try std.testing.allocator.alloc(u8, ActivitySender.MAX_BATCH_BYTES - tail_room);
    defer std.testing.allocator.free(big);
    @memset(big, 'b');
    sender.enqueue("in-flight");
    try recorder.entered.timedWait(std.time.ns_per_s);
    for (0..ActivitySender.MAX_QUEUED_BATCHES) |_| sender.enqueue(big);

    // Comma plus eight bytes needs nine: one over the slot, so it is shed.
    sender.enqueue("12345678");
    try std.testing.expectEqual(@as(u32, 1), sender.dropped);
    // Comma plus seven fills the slot to the byte, so it is kept.
    sender.enqueue("1234567");
    try std.testing.expectEqual(@as(u32, 1), sender.dropped);

    recorder.release.set();
    sender.finish();
    const newest = recorder.bodies.items[recorder.bodies.items.len - 1];
    try std.testing.expectEqual(ActivitySender.MAX_BATCH_BYTES, newest.len);
    try std.testing.expect(std.mem.endsWith(u8, newest, ",1234567"));
}

test "a fold never writes into the batch the sender is posting" {
    var deadlines: dts.TestScheduler = .{};
    defer deadlines.deinit();
    var recorder = Recorder{};
    defer recorder.deinit();
    defer recorder.release.set();
    var sender = recordingSender(try deadlines.start(std.testing.allocator), &recorder);
    try sender.start();
    defer sender.finish();

    sender.enqueue("posting");
    try recorder.entered.timedWait(std.time.ns_per_s);
    for ([_][]const u8{ "a", "b", "c", "d", "e", "f" }) |item| sender.enqueue(item);
    recorder.release.set();
    sender.finish();
    try std.testing.expectEqualStrings("posting", recorder.bodies.items[0]);
    try std.testing.expectEqualStrings("d,e,f", recorder.bodies.items[recorder.bodies.items.len - 1]);
}
