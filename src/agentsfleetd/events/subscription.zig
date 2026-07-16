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
//! Ownership is refcounted (`ref`/`unref`), and that is load-bearing: the hub
//! reader fans a frame out with the channel-map mutex RELEASED, so a racing
//! `unsubscribe` must not free a handle the reader already snapshotted. The
//! stream thread holds one ref for its lifetime; the reader holds one for the
//! duration of a push; the last `unref` frees. `SubscriptionHub.subscribe`
//! hands out the first ref, `unsubscribe` drops it.

const Self = @This();

alloc: std.mem.Allocator,
io: std.Io,
/// Live references to this handle: the owner (stream thread) plus any reader
/// fan-out currently pushing into it. Zero ⇒ freed. Without it, pushing
/// outside the hub's map mutex would be a use-after-free the moment a viewer
/// closed mid-frame.
refs: std.atomic.Value(u32) = .init(1),
/// Channel this subscription is attached to. Owned copy. A shared (tagged)
/// consumer is attached to N channels through the hub; this field then only
/// labels the consumer (log/teardown identity), never a map key.
channel_name: []u8,
/// Shared-consumer marker, set by `SubscriptionHub.createConsumer`. The hub
/// reader routes tagged consumers through `pushTagged` so each queued frame
/// carries its originating channel.
tagged: bool = false,
/// Guards the epoch counter and payload ring; the epoch is read under it before a futex sleep.
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

/// Per-consumer standing buffer: 64 frames × publisher-bounded activity
/// payloads (~1 KiB typical) ≈ 64 KiB worst case for one stalled consumer.
/// A per-fleet stream owns one consumer; a workspace stream's whole fan-in
/// SHARES one tagged consumer, so the budget stays per-CONNECTION —
/// fleet-count-independent — and the overall ceiling is the SSE stream cap
/// times this capacity, never cap × fleets.
pub const QUEUE_CAPACITY: usize = 64;

/// Separates channel name from payload inside a tagged frame. `\n` cannot
/// appear in a Redis channel name, so the consumer splits at the FIRST
/// occurrence unambiguously.
pub const TAGGED_FRAME_DELIMITER: u8 = '\n';

pub const PopResult = union(enum) {
    /// Caller owns the payload; free it with the allocator the hub was
    /// built on (the handler Context allocator).
    message: []u8,
    timeout,
    closed,
};

pub fn create(alloc: std.mem.Allocator, io: std.Io, channel_name: []const u8) error{OutOfMemory}!*Self {
    const self = try alloc.create(Self);
    errdefer alloc.destroy(self);
    const name = try alloc.dupe(u8, channel_name);
    self.* = .{ .alloc = alloc, .io = io, .channel_name = name };
    return self;
}

/// A shared consumer: one queue/epoch fed by N channel attachments, so the
/// stream thread does ONE futex wait for a whole fan-in. `label` names the
/// consumer in logs (a workspace id) — it is never a channel map key.
pub fn createShared(alloc: std.mem.Allocator, io: std.Io, label: []const u8) error{OutOfMemory}!*Self {
    const self = try create(alloc, io, label);
    self.tagged = true;
    return self;
}

/// One frame off a shared consumer: the channel it arrived on plus the
/// publisher's untouched payload. Both borrow the popped buffer — free the
/// buffer, not these.
pub const TaggedFrame = struct {
    channel_name: []const u8,
    payload: []const u8,
};

/// Split a `pushTagged` frame. Null when the delimiter is absent — a frame
/// shape the consumer must drop rather than mis-route.
pub fn splitTagged(frame: []const u8) ?TaggedFrame {
    const cut = std.mem.indexOfScalar(u8, frame, TAGGED_FRAME_DELIMITER) orelse return null;
    return .{ .channel_name = frame[0..cut], .payload = frame[cut + 1 ..] };
}

/// Take a reference. The hub reader calls this under the channel-map mutex
/// while snapshotting subscribers, so the handle cannot be freed by a racing
/// `unsubscribe` between the snapshot and the push.
pub fn ref(self: *Self) void {
    // safe because: the map mutex (or an existing ref) already keeps the
    // handle alive at the call site — this bump only publishes the new count
    // to whoever later does the releasing decrement.
    _ = self.refs.fetchAdd(1, .monotonic);
}

/// Drop a reference; the last one frees. `unref` is the ONLY way a handle is
/// released — a caller that still holds a ref cannot be use-after-freed by
/// another thread's unsubscribe.
pub fn unref(self: *Self) void {
    // safe because: acq_rel makes every prior push's writes visible to the
    // thread that observes the final decrement and runs the free.
    if (self.refs.fetchSub(1, .acq_rel) != 1) return;
    self.destroy();
}

/// Free the handle. Private: the last `unref` calls it, so no caller can free
/// a subscription another thread is still pushing into.
fn destroy(self: *Self) void {
    while (self.count > 0) : (self.count -= 1) {
        self.alloc.free(self.ring[self.tail]);
        self.tail = (self.tail + 1) % QUEUE_CAPACITY;
    }
    self.alloc.free(self.channel_name);
    const alloc = self.alloc;
    self.* = undefined;
    alloc.destroy(self);
}

/// Producer side (hub reader thread). Copies `payload` in; a full ring
/// evicts the oldest frame and a failed copy drops the new one — both
/// counted, neither blocking.
pub fn push(self: *Self, payload: []const u8) void {
    const copy = self.alloc.dupe(u8, payload) catch {
        self.noteDrop();
        return;
    };
    self.enqueueOwned(copy);
}

/// Producer side for a shared (tagged) consumer: the frame is stored as
/// `channel ++ TAGGED_FRAME_DELIMITER ++ payload` in one owned buffer, so
/// the consumer can splice the originating fleet id from the channel name
/// without re-parsing the payload. Same drop semantics as `push`.
pub fn pushTagged(self: *Self, channel_name: []const u8, payload: []const u8) void {
    const copy = self.alloc.alloc(u8, channel_name.len + 1 + payload.len) catch {
        self.noteDrop();
        return;
    };
    @memcpy(copy[0..channel_name.len], channel_name);
    copy[channel_name.len] = TAGGED_FRAME_DELIMITER;
    @memcpy(copy[channel_name.len + 1 ..], payload);
    self.enqueueOwned(copy);
}

/// Ring insert shared by `push`/`pushTagged`. Takes ownership of `copy`.
fn enqueueOwned(self: *Self, copy: []u8) void {
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

fn noteDrop(self: *Self) void {
    self.mutex.lockUncancelable(self.io);
    self.drops += 1;
    self.mutex.unlock(self.io);
    metrics.incSseDroppedFrames();
}

fn wake(self: *Self) void {
    // safe because: the release bump pairs with pop's read of the epoch
    // under the mutex; bump-then-wake means a consumer that saw the old
    // epoch either gets this wake or observes the new value before sleeping.
    _ = self.epoch.fetchAdd(1, .release);
    self.io.futexWake(u32, &self.epoch.raw, 1);
}

/// Consumer side (stream thread). Waits up to `timeout_ms` for the next
/// frame. Remaining frames are delivered before `closed` is reported, so a
/// closing hub still drains what was queued.
pub fn pop(self: *Self, timeout_ms: u64) PopResult {
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
pub fn close(self: *Self) void {
    {
        self.mutex.lockUncancelable(self.io);
        defer self.mutex.unlock(self.io);
        self.closed = true;
    }
    self.wake();
}

/// Snapshot of the drop counter (admin/diagnostic surface).
pub fn dropCount(self: *Self) u64 {
    self.mutex.lockUncancelable(self.io);
    defer self.mutex.unlock(self.io);
    return self.drops;
}

const std = @import("std");
const common = @import("common");
const clock = common.clock;
const metrics = @import("../observability/metrics.zig");
