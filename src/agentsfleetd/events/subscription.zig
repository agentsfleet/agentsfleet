//! Per-stream handle to the SubscriptionHub's fan-out: a bounded ring of
//! owned payload copies with a timed-wait pop.
//!
//! One producer (the hub reader thread, via `push`), one consumer (the SSE
//! stream thread, via `pop`); `close` may additionally arrive from the hub's
//! stop path. The producer NEVER blocks: a full ring drops the oldest frame
//! and counts it — a stalled consumer must cost frames, not stall the hub.
//!
//! Timed wait: `Io.Condition` exposes no timeout (vendor/pg documented the
//! same gap), so the consumer waits on an epoch counter with
//! `futexWaitTimeout` — the epoch is read under the mutex before sleeping,
//! so a producer's bump-then-wake can never be lost between the predicate
//! check and the wait (the same registered-waiter shape `Io.Condition` uses
//! internally, plus a deadline).
//!
//! Created by `SubscriptionHub.subscribe`, destroyed by
//! `SubscriptionHub.unsubscribe`; the stream thread only borrows it between
//! those two calls.

const Subscription = @This();

alloc: std.mem.Allocator,
io: std.Io,
/// Channel this subscription is attached to. Owned copy.
channel_name: []u8,
mutex: std.Io.Mutex = .init,
/// Bumped (release) + futex-woken on every push/close; pop reads it under
/// the mutex and sleeps on that observed value.
epoch: std.atomic.Value(u32) = .init(0),
/// Ring of owned payload copies; oldest at `tail`.
// SAFETY: slots are written by push before count admits them to any reader;
// only indices inside [tail, tail+count) are ever read or freed.
ring: [QUEUE_CAPACITY][]u8 = undefined,
tail: usize = 0,
count: usize = 0,
/// Frames dropped against this consumer: ring-full evictions + copy failures.
drops: u64 = 0,
closed: bool = false,

/// Per-stream standing buffer: 64 frames × publisher-bounded activity
/// payloads (~1 KiB typical) ≈ 64 KiB worst case for one stalled consumer,
/// bounded overall by the SSE stream cap.
pub const QUEUE_CAPACITY: usize = 64;

pub const PopResult = union(enum) {
    /// Caller owns the payload; free it with the allocator the hub was
    /// built on (the handler Context allocator).
    message: []u8,
    timeout,
    closed,
};

pub fn create(alloc: std.mem.Allocator, io: std.Io, channel_name: []const u8) error{OutOfMemory}!*Subscription {
    const self = try alloc.create(Subscription);
    errdefer alloc.destroy(self);
    const name = try alloc.dupe(u8, channel_name);
    self.* = .{ .alloc = alloc, .io = io, .channel_name = name };
    return self;
}

/// Hub-only: by the time the hub calls this it has unhooked the subscription
/// from its map, so no producer can race the teardown; the consumer's part
/// of the deal is that the stream thread never touches the handle after
/// unsubscribe.
pub fn destroy(self: *Subscription) void {
    while (self.count > 0) : (self.count -= 1) {
        self.alloc.free(self.ring[self.tail]);
        self.tail = (self.tail + 1) % QUEUE_CAPACITY;
    }
    self.alloc.free(self.channel_name);
    const alloc = self.alloc;
    alloc.destroy(self);
}

/// Producer side (hub reader thread). Copies `payload` in; a full ring
/// evicts the oldest frame and a failed copy drops the new one — both
/// counted, neither blocking.
pub fn push(self: *Subscription, payload: []const u8) void {
    const copy = self.alloc.dupe(u8, payload) catch {
        self.noteDrop();
        return;
    };
    var evicted: ?[]u8 = null;
    {
        self.mutex.lockUncancelable(self.io);
        defer self.mutex.unlock(self.io);
        if (self.closed) {
            evicted = copy;
        } else {
            if (self.count == QUEUE_CAPACITY) {
                evicted = self.ring[self.tail];
                self.tail = (self.tail + 1) % QUEUE_CAPACITY;
                self.count -= 1;
                self.drops += 1;
                metrics.incSseDroppedFrames();
            }
            self.ring[(self.tail + self.count) % QUEUE_CAPACITY] = copy;
            self.count += 1;
        }
    }
    self.wake();
    if (evicted) |old| self.alloc.free(old);
}

fn noteDrop(self: *Subscription) void {
    self.mutex.lockUncancelable(self.io);
    self.drops += 1;
    self.mutex.unlock(self.io);
    metrics.incSseDroppedFrames();
}

fn wake(self: *Subscription) void {
    // safe because: the release bump pairs with pop's read of the epoch
    // under the mutex; bump-then-wake means a consumer that saw the old
    // epoch either gets this wake or observes the new value before sleeping.
    _ = self.epoch.fetchAdd(1, .release);
    self.io.futexWake(u32, &self.epoch.raw, 1);
}

/// Consumer side (stream thread). Waits up to `timeout_ms` for the next
/// frame. Remaining frames are delivered before `closed` is reported, so a
/// closing hub still drains what was queued.
pub fn pop(self: *Subscription, timeout_ms: u64) PopResult {
    const io = self.io;
    const deadline_ms = clock.nowMillis() + @as(i64, @intCast(timeout_ms));
    while (true) {
        // SAFETY: assigned under the mutex below before any read.
        var seen: u32 = undefined;
        {
            self.mutex.lockUncancelable(io);
            defer self.mutex.unlock(io);
            if (self.count > 0) {
                const payload = self.ring[self.tail];
                self.tail = (self.tail + 1) % QUEUE_CAPACITY;
                self.count -= 1;
                return .{ .message = payload };
            }
            if (self.closed) return .closed;
            // safe because: read under the mutex, so any push that beat us
            // here already bumped it and the futex wait returns immediately.
            seen = self.epoch.load(.monotonic);
        }
        const now_ms = clock.nowMillis();
        if (now_ms >= deadline_ms) return .timeout;
        const remaining: i64 = deadline_ms - now_ms;
        // timeout expiry and spurious wakes both just re-run the checks
        io.futexWaitTimeout(u32, &self.epoch.raw, seen, .{
            .duration = .{ .raw = .fromMilliseconds(remaining), .clock = .awake },
        }) catch |err| switch (err) {
            // never expected on a plain stream thread; re-running the
            // predicate/deadline checks is the correct response anyway
            error.Canceled => {},
        };
    }
}

/// Hub stop/drain path: wake the consumer permanently.
pub fn close(self: *Subscription) void {
    {
        self.mutex.lockUncancelable(self.io);
        defer self.mutex.unlock(self.io);
        self.closed = true;
    }
    self.wake();
}

/// Snapshot of the drop counter (admin/diagnostic surface).
pub fn dropCount(self: *Subscription) u64 {
    self.mutex.lockUncancelable(self.io);
    defer self.mutex.unlock(self.io);
    return self.drops;
}

const std = @import("std");
const common = @import("common");
const clock = common.clock;
const metrics = @import("../observability/metrics.zig");
