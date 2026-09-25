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
/// Ring capacity: the four WAITING batches plus the one the sender is posting.
/// `queued` never counts the in-flight slot, so the producer's index
/// `(head + queued) % RING_SLOTS` cannot reach it while `queued < RING_SLOTS - 1`.
/// That inequality is the whole protection; it holds because `enqueue` drops
/// at `MAX_QUEUED_BATCHES`.
const RING_SLOTS: usize = MAX_QUEUED_BATCHES + 1;
/// Give a slow control plane room for several round trips without holding a
/// finished lease for the full configured deadline on every queued batch.
pub const SEND_DEADLINE_CAP_MS: u31 = 1_000;

/// Owns the ring. Claimed in `start`, released in `finish`, both on the
/// caller's thread; the sender thread never allocates from it.
alloc: std.mem.Allocator,
io: std.Io,
sched: *call_deadline.ProcessScheduler,
base_url: []const u8,
runner_token: []const u8,
lease_id: []const u8,
deadline_ms: u31,
mutex: common.Mutex = .{},
cond: common.Condition = .{},
/// Slot storage, allocated once by the sender thread and reused for every
/// batch. The child reader only ever `@memcpy`s into it, so the latency path
/// asks the kernel for nothing.
slots: ?*[RING_SLOTS][MAX_BATCH_BYTES]u8 = null,
/// Bytes live in each slot; `null` where the slot is free.
lens: [RING_SLOTS]?usize = .{null} ** RING_SLOTS,
head: usize = 0,
queued: usize = 0,
closed: bool = false,
/// Batches the queue could not take, reported once when the lease ends. The
/// browser recovers the text from `stream_seq`; this is how an OPERATOR finds
/// out the tail was shed, and how a saturated host is told apart from a quiet
/// one.
dropped: u32 = 0,
/// Set once the first drop has been reported, so the sender says so ONCE
/// while the lease runs and the final tally arrives at `finish`.
drop_noticed: bool = false,
thread: ?std.Thread = null,
test_hook: ?Hook = null,

const Hook = struct {
    ctx: *anyopaque,
    send: *const fn (*anyopaque, []const u8, u31) void,
};

/// Start the sole sender for this lease. The caller joins it after report.
///
/// The slot storage is claimed HERE, before the thread exists, so the child
/// reader never races an unallocated queue and never allocates itself. One
/// allocation serves the whole lease; a failure leaves the caller's transport off.
pub fn start(self: *ActivitySender) !void {
    self.slots = try self.alloc.create([RING_SLOTS][MAX_BATCH_BYTES]u8);
    errdefer {
        self.alloc.destroy(self.slots.?);
        self.slots = null;
    }
    self.thread = try std.Thread.spawn(.{}, run, .{self});
}

/// Copy one complete serialized batch without waiting for network I/O.
/// Oversized or over-capacity batches are lost with the cosmetic tail; stream
/// sequence numbers let the browser detect the missing bytes, and `dropped`
/// tells the operator it happened.
pub fn enqueue(self: *ActivitySender, bytes: []const u8) void {
    if (bytes.len == 0 or bytes.len > MAX_BATCH_BYTES) return;
    // Claim under the lock, copy OUTSIDE it, publish under it again. The lock
    // is held for a handful of stores either side of a 64 KiB copy, never for
    // the copy itself, so the sender freeing a slot is never made to wait on
    // the reader's memcpy.
    //
    // Sound because there is ONE producer. The claimed index is
    // `(head + queued) % RING_SLOTS`; the consumer only ever advances `head`
    // and decrements `queued` together, which leaves their sum — and so this
    // slot — exactly where it was. Nothing else can claim it, and the slot the
    // consumer is posting from sits at `head - 1`, which this index reaches
    // only at `queued == RING_SLOTS - 1`, one past the cap that drops first.
    self.mutex.lock();
    const slots = self.slots orelse {
        self.mutex.unlock();
        return;
    };
    if (self.closed or self.queued == MAX_QUEUED_BATCHES) {
        self.dropped +|= 1;
        self.mutex.unlock();
        return;
    }
    const slot = (self.head + self.queued) % RING_SLOTS;
    self.mutex.unlock();

    @memcpy(slots[slot][0..bytes.len], bytes);

    self.mutex.lock();
    self.lens[slot] = bytes.len;
    self.queued += 1;
    self.cond.signal();
    self.mutex.unlock();
}

/// Drain and join after the durable report has published the completion marker.
/// Each queued POST uses a bounded activity deadline. Slow sends cannot
/// block the child reader because this thread owns the network call.
pub fn finish(self: *ActivitySender) void {
    self.mutex.lock();
    self.closed = true;
    self.cond.broadcast();
    const dropped = self.dropped;
    self.mutex.unlock();
    if (self.thread) |thread| thread.join();
    self.thread = null;
    // After the join, so the count is final. Logged once per lease rather than
    // per drop: a saturated control plane sheds batches in bursts, and one line
    // per batch would bury the reason under the symptom.
    if (dropped > 0) log.warn("activity_batches_dropped", .{
        .error_code = client_errors.ERR_EXEC_TRANSPORT_LOSS,
        .lease_id = self.lease_id,
        .dropped = dropped,
        .capacity = MAX_QUEUED_BATCHES,
    });
    self.dropped = 0;
    self.drop_noticed = false;
    if (self.slots) |slots| {
        self.alloc.destroy(slots);
        self.slots = null;
    }
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
        // Pop under the lock, send outside it. Once `head` moves past this
        // slot the producer's next index is `(head + queued) % RING_SLOTS`;
        // with `queued` capped one below `RING_SLOTS` that expression never
        // lands back on the slot being posted, so the spare slot alone keeps
        // the in-flight body intact — no flag, no second bookkeeping path.
        const slot = self.head;
        const len = self.lens[slot].?;
        const slots = self.slots.?;
        self.head = (slot + 1) % RING_SLOTS;
        self.queued -= 1;
        // The reader must not log — that is I/O on the pipe thread — so the
        // first drop is announced from HERE, the next time the sender wakes.
        const first_drop = self.dropped > 0 and !self.drop_noticed;
        if (first_drop) self.drop_noticed = true;
        self.mutex.unlock();
        if (first_drop) log.warn("activity_batch_dropped", .{
            .error_code = client_errors.ERR_EXEC_TRANSPORT_LOSS,
            .lease_id = self.lease_id,
            .capacity = MAX_QUEUED_BATCHES,
        });
        const bytes = slots[slot][0..len];
        defer {
            self.mutex.lock();
            self.lens[slot] = null;
            self.mutex.unlock();
        }
        const deadline_ms = @min(self.deadline_ms, SEND_DEADLINE_CAP_MS);
        if (self.test_hook) |hook| {
            hook.send(hook.ctx, bytes, deadline_ms);
        } else {
            cp.activityFramesJson(alloc, self.runner_token, self.lease_id, bytes, deadline_ms);
        }
    }
}

test {
    _ = @import("ActivitySender_test.zig");
}
