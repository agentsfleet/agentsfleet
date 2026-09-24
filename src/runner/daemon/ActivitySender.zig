//! Bounded, ordered live-tail transport for one lease.
//!
//! The child read loop only copies into this queue. One sender thread owns its
//! allocator and HTTP client, so slow activity POSTs cannot stall the model
//! pipe or renewal tick. Four 64 KiB batches bound retained queue memory.

const ActivitySender = @This();

const std = @import("std");
const common = @import("common");
const client_mod = @import("control_plane_client.zig");
const client_errors = @import("../engine/client_errors.zig");
const call_deadline = @import("call_deadline");
const logging = @import("log");

const log = logging.scoped(.fleet_runner);

/// Upper bound for one queued serialized live-tail batch.
pub const MAX_BATCH_BYTES: usize = 64 * 1024;
/// Upper bound for batches retained beside a slow control plane.
pub const MAX_QUEUED_BATCHES: usize = 4;
const SEND_DEADLINE_MS: u31 = 250;

io: std.Io,
sched: *call_deadline.ProcessScheduler,
base_url: []const u8,
runner_token: []const u8,
lease_id: []const u8,
deadline_ms: u31,
mutex: common.Mutex = .{},
cond: common.Condition = .{},
queue: [MAX_QUEUED_BATCHES]?[]u8 = .{null} ** MAX_QUEUED_BATCHES,
head: usize = 0,
queued: usize = 0,
closed: bool = false,
thread: ?std.Thread = null,
test_hook: ?Hook = null,

const Hook = struct {
    ctx: *anyopaque,
    send: *const fn (*anyopaque, []const u8) void,
};

/// Start the sole sender for this lease. The caller joins it after report.
pub fn start(self: *ActivitySender) !void {
    self.thread = try std.Thread.spawn(.{}, run, .{self});
}

/// Copy one complete serialized batch without waiting for network I/O.
/// Oversized or over-capacity batches are lost with the cosmetic tail; stream
/// sequence numbers let the browser detect the missing bytes.
pub fn enqueue(self: *ActivitySender, bytes: []const u8) void {
    if (bytes.len == 0 or bytes.len > MAX_BATCH_BYTES) return;
    const copy = std.heap.page_allocator.dupe(u8, bytes) catch return;
    self.mutex.lock();
    defer self.mutex.unlock();
    if (self.closed or self.queued == MAX_QUEUED_BATCHES) {
        std.heap.page_allocator.free(copy);
        return;
    }
    const slot = (self.head + self.queued) % MAX_QUEUED_BATCHES;
    self.queue[slot] = copy;
    self.queued += 1;
    self.cond.signal();
}

/// Drain and join after the durable report has published the completion marker.
/// Each queued POST has a 250 ms socket deadline; DNS and connect still follow
/// the control-plane client's documented limitations.
pub fn finish(self: *ActivitySender) void {
    self.mutex.lock();
    self.closed = true;
    self.cond.broadcast();
    self.mutex.unlock();
    if (self.thread) |thread| thread.join();
    self.thread = null;
}

fn run(self: *ActivitySender) void {
    var gpa: std.heap.DebugAllocator(.{}) = .{};
    defer if (gpa.deinit() == .leak) log.err("activity_sender_leaked", .{ .error_code = client_errors.ERR_EXEC_TRANSPORT_LOSS });
    const alloc = gpa.allocator();
    var cp = client_mod.init(alloc, self.io, self.sched, self.base_url);
    defer cp.deinit();
    while (true) {
        self.mutex.lock();
        while (self.queued == 0 and !self.closed) self.cond.wait(&self.mutex);
        if (self.queued == 0) {
            self.mutex.unlock();
            return;
        }
        const bytes = self.queue[self.head].?;
        self.queue[self.head] = null;
        self.head = (self.head + 1) % MAX_QUEUED_BATCHES;
        self.queued -= 1;
        self.mutex.unlock();
        defer std.heap.page_allocator.free(bytes);
        if (self.test_hook) |hook| {
            hook.send(hook.ctx, bytes);
        } else {
            cp.activityFramesJson(alloc, self.runner_token, self.lease_id, bytes, @min(self.deadline_ms, SEND_DEADLINE_MS));
        }
    }
}

test {
    _ = @import("ActivitySender_test.zig");
}
