//! A frame that would carry a batch past the queued sender's size limit must
//! not take the frames before it down with it. The sender refuses an oversized
//! batch whole, so the forwarder ships what it has first.

const std = @import("std");
const testing = std.testing;
const common = @import("common");
const client_mod = @import("control_plane_client.zig");
const dts = @import("deadline_test_support.zig");
const call_deadline = @import("call_deadline");
const forwarders = @import("forwarders.zig");
const ActivitySender = @import("ActivitySender.zig");

const DEAD_URL = "http://127.0.0.1:9";
/// Two of these fit under one batch only apart: each is well under the limit,
/// together they are over it.
const HALF_BATCH_ARGS_BYTES: usize = 40 * 1024;

/// Records the size of every batch the sender thread posts.
const Capture = struct {
    mutex: common.Mutex = .{},
    lens: [4]usize = undefined,
    count: usize = 0,

    fn send(ctx: *anyopaque, bytes: []const u8, _: u31) void {
        const self: *Capture = @ptrCast(@alignCast(ctx));
        self.mutex.lock();
        defer self.mutex.unlock();
        if (self.count < self.lens.len) {
            self.lens[self.count] = bytes.len;
            self.count += 1;
        }
    }
};

fn startedSender(sched: *call_deadline.ProcessScheduler, capture: *Capture) ActivitySender {
    return .{
        .alloc = testing.allocator,
        .io = common.globalIo(),
        .sched = sched,
        .base_url = DEAD_URL,
        .runner_token = "agt_rtest",
        .lease_id = "lease_test",
        .deadline_ms = call_deadline.ACTIVITY_DEADLINE_MS,
        .test_hook = .{ .ctx = capture, .send = Capture.send },
    };
}

fn forwardToolCall(fwd: *forwarders.ActivityForwarder, args: []const u8) void {
    forwarders.ActivityForwarder.forward(@ptrCast(fwd), .{
        .tool_call_started = .{ .name = "probe", .args_redacted = args },
    });
}

test "a frame that would overflow the batch ships the frames before it, then starts the next batch" {
    var deadlines: dts.TestScheduler = .{};
    defer deadlines.deinit();
    const sched = try deadlines.start(testing.allocator);
    var c = client_mod.init(testing.allocator, common.globalIo(), sched, DEAD_URL);
    defer c.deinit();
    var capture = Capture{};
    var sender = startedSender(sched, &capture);
    try sender.start();
    defer sender.finish();

    var fwd = forwarders.ActivityForwarder{ .alloc = testing.allocator, .cp = &c, .runner_token = "agt_rtest", .lease_id = "lease_test", .deadline_ms = call_deadline.ACTIVITY_DEADLINE_MS, .transport = .{ .queued = &sender } };
    defer fwd.deinit();
    fwd.eager_first_frame_done = true;
    fwd.eager_first_chunk_done = true;

    const args = try testing.allocator.alloc(u8, HALF_BATCH_ARGS_BYTES);
    defer testing.allocator.free(args);
    @memset(args, 'x');

    forwardToolCall(&fwd, args);
    try testing.expectEqual(@as(usize, 1), fwd.count);
    forwardToolCall(&fwd, args);
    // The first frame shipped on its own; the second leads the next batch,
    // with no separator in front of it.
    try testing.expectEqual(@as(usize, 1), fwd.count);
    try testing.expectEqual(@as(u8, '{'), fwd.buf.items[0]);
    fwd.flush();
    // `finish` totals and resets the drop count, so read it first.
    try testing.expectEqual(@as(u32, 0), sender.dropped);
    sender.finish();

    try testing.expectEqual(@as(usize, 2), capture.count);
    for (capture.lens[0..capture.count]) |len| try testing.expect(len <= ActivitySender.MAX_BATCH_BYTES);
}

test "a single frame larger than a batch is refused and counted, never posted" {
    var deadlines: dts.TestScheduler = .{};
    defer deadlines.deinit();
    const sched = try deadlines.start(testing.allocator);
    var c = client_mod.init(testing.allocator, common.globalIo(), sched, DEAD_URL);
    defer c.deinit();
    var capture = Capture{};
    var sender = startedSender(sched, &capture);
    try sender.start();
    defer sender.finish();

    var fwd = forwarders.ActivityForwarder{ .alloc = testing.allocator, .cp = &c, .runner_token = "agt_rtest", .lease_id = "lease_test", .deadline_ms = call_deadline.ACTIVITY_DEADLINE_MS, .transport = .{ .queued = &sender } };
    defer fwd.deinit();
    fwd.eager_first_frame_done = true;
    fwd.eager_first_chunk_done = true;

    const args = try testing.allocator.alloc(u8, ActivitySender.MAX_BATCH_BYTES + 1);
    defer testing.allocator.free(args);
    @memset(args, 'x');

    forwardToolCall(&fwd, args);
    // The byte cap flushed it at once, so nothing lingers to poison a later batch.
    try testing.expectEqual(@as(usize, 0), fwd.count);
    // `finish` totals and resets the drop count, so read it first.
    try testing.expectEqual(@as(u32, 1), sender.dropped);
    sender.finish();

    try testing.expectEqual(@as(usize, 0), capture.count);
}
