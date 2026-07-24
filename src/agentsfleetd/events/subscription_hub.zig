//! SubscriptionHub — the process's ONE Redis pub/sub connection, fanned out
//! to every live SSE stream.
//!
//! Topology: a mutex-guarded `channel → subscribers` map in front of a single
//! `redis_subscriber` connection read by one dedicated reader thread
//! (`subscription_hub_reader.zig`). Wire SUBSCRIBE/UNSUBSCRIBE happen only on
//! a channel's first-subscriber / last-subscriber edges; everything between
//! is a map edit.
//!
//! Concurrency model — two locks, one thread confinement, one watchdog:
//!   - `mutex` guards `channels` (the map) and NOTHING else. No wire write
//!     ever runs under it, so a stalled peer cannot block dispatch,
//!     subscribe, unsubscribe, channelCount, or stop.
//!   - `wire` guards the `conn` field and serializes every wire WRITE
//!     (subscribe/unsubscribe sends, reconnect install). Never nested with
//!     `mutex` — acquire one at a time.
//!   - The reader thread is confined to the read half (fd poll/read + read
//!     keys) and dereferences `conn` outside `wire`: only the reader (and
//!     post-join `stop()`) swaps the field, and the two transport halves are
//!     disjoint — writers touch only the writer + TLS write keys, under
//!     `wire`.
//!   - Every wire send is bounded by a call_deadline watchdog: a peer that
//!     stops reading gets the socket shut down at the deadline; the send
//!     errors, the reader's next read fails, and the reconnect path heals.
//!
//! Loss semantics: pub/sub is the eyeballs surface, not the audit surface.
//! Frames published while the connection is being re-dialed are lost, exactly
//! as they were when each stream owned the connection that died; clients
//! backfill through the events cursor.

const Self = @This();

pub const Subscription = @import("subscription.zig");

alloc: std.mem.Allocator,
io: std.Io,
/// Resolved Redis config, BORROWED from the queue client's pool — set by
/// `start()`; must outlive the hub (serve.zig and the harness both deinit
/// the hub before the queue client).
cfg: ?redis_config.Config = null,
/// Guards `channels` only. Wire writes NEVER run under it — see the model
/// doc above; fan-out (reader) and map edits (request threads) share it.
mutex: std.Io.Mutex = .init,
/// Guards `conn` (the field) and serializes every wire write. Bounded holds
/// only: each send is watchdog-bounded, so waiters never wait unbounded.
wire: std.Io.Mutex = .init,
channels: std.StringHashMapUnmanaged(*ChannelEntry) = .empty,
conn: ?redis_subscriber = null,
reader_thread: ?std.Thread = null,
stopped: std.atomic.Value(bool) = .init(false),
/// Reader-socket read timeout. Default = prod's `HUB_READ_TIMEOUT_MS`;
/// the test harness lowers it so `stop()`'s join is fast.
read_timeout_ms: u32 = HUB_READ_TIMEOUT_MS,
/// Per-send deadline for wire writes (the watchdog's arm bound). Default =
/// prod's `HUB_SEND_TIMEOUT_MS`; tests lower it to exercise the fire path.
send_timeout_ms: u31 = HUB_SEND_TIMEOUT_MS,
/// `stop()`'s bounded wait for closed streams to detach. Default = prod's
/// `STOP_DRAIN_MAX_MS`; tests lower it to exercise the undrained path.
stop_drain_max_ms: u64 = STOP_DRAIN_MAX_MS,
/// Borrowed process deadline scheduler. Null only in fixtures that never dial;
/// a wire send with no scheduler is refused rather than run unbounded.
sched: ?*call_deadline.ProcessScheduler = null,
/// Generation-guarded ownership of the INSTALLED connection's socket. Stable
/// address (a hub field), so a registration may point at it while `conn` itself
/// is replaced by a reconnect.
wire_owner: call_deadline.SocketOwner = .{},
/// The generation `conn` was installed under. A send armed on an older one is
/// stale, which is what makes a reconnect race harmless.
wire_generation: u64 = 0,
/// One absolute budget for a whole connection attempt (resolve → dial → TLS →
/// AUTH), so a stall in any stage cannot reset the allowance.
setup_timeout_ms: u31 = HUB_SETUP_TIMEOUT_MS,
/// Fan-out snapshot buffer — reader-thread-confined (C5; joined by `stop`
/// before teardown). Retained across frames so a steady-state fan-out allocates
/// nothing; lets the reader copy the subscriber set out from under `mutex` and
/// push with the lock released (C3: no blocking work under a lock the consumer
/// needs).
reader_scratch: std.ArrayList(*Subscription) = .empty,

const ChannelEntry = struct {
    subscribers: std.ArrayList(*Subscription) = .empty,
};

/// Default reader wake cadence — bounds stop latency, reconnect detection, and
/// pickup delay for wire commands behind a quiet socket. The per-instance
/// `read_timeout_ms` field defaults to this; the test harness overrides it.
const HUB_READ_TIMEOUT_MS: u32 = 1_000;
/// Default per-send bound. A truly dead peer costs one subscriber at most
/// this long, never the daemon: the fired watchdog kills the socket and the
/// reconnect path takes over.
const HUB_SEND_TIMEOUT_MS: u31 = 5_000;
/// Whole-attempt setup budget: resolve + dial + TLS + AUTH share it, so a
/// stalled handshake cannot hold a boot or a reconnect open indefinitely.
const HUB_SETUP_TIMEOUT_MS: u31 = 10_000;
/// `stop()` waits (bounded) for closed streams to detach so a late
/// `unsubscribe` can never touch a deinit'd channel map.
const STOP_DRAIN_MAX_MS: u64 = 5_000;
const STOP_DRAIN_POLL_MS: u64 = 50;

pub fn init(alloc: std.mem.Allocator, io: std.Io) Self {
    return .{ .alloc = alloc, .io = io };
}

/// Dial the shared connection and start the reader thread. Boot path —
/// failure here is a startup failure, mirroring the queue client connect.
/// Single-threaded: the reader does not exist yet, so `conn` is set bare.
pub fn start(self: *Self, cfg: redis_config.Config, sched: *call_deadline.ProcessScheduler) !void {
    self.cfg = cfg;
    self.sched = sched;
    var conn = try hub_wire.connectBounded(self);
    errdefer conn.deinit();
    conn.installReadTimeout();
    self.conn = conn;
    errdefer self.conn = null;
    hub_wire.adoptConnection(self);
    self.reader_thread = try std.Thread.spawn(.{}, reader.readerMain, .{self});
    log.debug("hub_started", .{ .host = cfg.host, .port = cfg.port });
}

/// Stop the reader, close every live subscription so stream threads drain,
/// and drop the connection — the teardown runs under `wire`, so a racing
/// late `unsubscribe` send serializes with it and then observes a null conn
/// instead of a freed one. Idempotent; safe on a never-started hub.
pub fn stop(self: *Self) void {
    if (self.stopped.swap(true, .acq_rel)) return;
    if (self.reader_thread) |t| {
        t.join();
        self.reader_thread = null;
    }
    self.mutex.lockUncancelable(self.io);
    var it = self.channels.valueIterator();
    while (it.next()) |entry| {
        for (entry.*.subscribers.items) |sub| sub.close();
    }
    self.mutex.unlock(self.io);
    // Bounded drain by WALL-CLOCK deadline (re-checked each poll), not a sum of
    // nominal sleep slices — a starved `sleepNanos` overshoots and would overrun it.
    const drain_deadline_ms = clock.nowMillis() + @as(i64, @intCast(self.stop_drain_max_ms));
    while (self.channelCount() > 0 and clock.nowMillis() < drain_deadline_ms) {
        common.sleepNanos(STOP_DRAIN_POLL_MS * std.time.ns_per_ms);
    }
    if (self.channelCount() > 0) {
        log.warn("hub_stop_undrained", .{ .live_channels = self.channelCount() });
    }
    self.wire.lockUncancelable(self.io);
    if (self.conn) |*c| {
        hub_wire.retireConnection(self);
        c.deinit();
        self.conn = null;
    }
    self.wire.unlock(self.io);
}

/// Frees map storage. Call after `stop()` AND after every stream thread has
/// deregistered (registry `awaitEmpty`) — a late `unsubscribe` would touch
/// freed map storage. serve.zig's defer chain and the harness encode this
/// ordering.
pub fn deinit(self: *Self) void {
    var it = self.channels.iterator();
    while (it.next()) |kv| {
        kv.value_ptr.*.subscribers.deinit(self.alloc);
        self.alloc.destroy(kv.value_ptr.*);
        self.alloc.free(kv.key_ptr.*);
    }
    self.channels.deinit(self.alloc);
    // Reader-owned; the reader is joined by stop() before any caller reaches here.
    self.reader_scratch.deinit(self.alloc);
    self.* = undefined;
}

pub const SubscribeError = error{ OutOfMemory, HubStopped };

/// Attach a new subscriber to `channel_name`. The first subscriber on a
/// channel sends the wire SUBSCRIBE — outside the map mutex, bounded by the
/// send watchdog; during a reconnect gap the send is skipped and the
/// post-redial sweep re-subscribes from the map.
pub fn subscribe(self: *Self, channel_name: []const u8) SubscribeError!*Subscription {
    const sub = try Subscription.create(self.alloc, self.io, channel_name);
    errdefer sub.unref();
    try self.attachChannel(sub, channel_name);
    return sub;
}

/// A consumer with no channel of its own: ONE queue/epoch fed by N channel
/// attachments, so a fan-in stream does one futex wait for the whole set and
/// its memory budget stays fleet-count-independent. The caller `attachChannel`s
/// each channel, `detachChannel`s every one, then `unref()`s. Refuses on a
/// draining hub (like `subscribe`), so a workspace stream against a stopped hub
/// is refused at connect rather than 200-ing into a consumer that never attaches.
pub fn createSharedConsumer(self: *Self, label: []const u8) SubscribeError!*Subscription {
    if (self.stopped.load(.acquire)) return error.HubStopped;
    return Subscription.createShared(self.alloc, self.io, label);
}

/// Attach `sub` to one channel: map insert, then the first-subscriber wire
/// SUBSCRIBE outside the map mutex. Per-fleet attaches one; a fan-in attaches
/// one shared consumer to many.
pub fn attachChannel(self: *Self, sub: *Subscription, channel_name: []const u8) SubscribeError!void {
    if (try self.attach(sub, channel_name)) self.wireSendSubscribe(channel_name);
}

/// Map half of subscribe: insert under `mutex` only. Returns true when `sub`
/// is the channel's first subscriber (the caller then does the wire send).
fn attach(self: *Self, sub: *Subscription, channel_name: []const u8) SubscribeError!bool {
    // Everything fallible is allocated before the lock; `consumed` routes the
    // spares to the map or back to the allocator on the way out. The explicit
    // catch covers the window before the consumed-defer is registered.
    const spare_key = try self.alloc.dupe(u8, channel_name);
    const spare_entry = self.alloc.create(ChannelEntry) catch |err| {
        self.alloc.free(spare_key);
        return err;
    };
    spare_entry.* = .{};
    var consumed = false;
    defer if (!consumed) {
        self.alloc.free(spare_key);
        self.alloc.destroy(spare_entry);
    };

    self.mutex.lockUncancelable(self.io);
    defer self.mutex.unlock(self.io);
    // checked under the mutex: stop()'s close-sweep holds it, so a stream
    // admitted here is guaranteed to be seen (and closed) by that sweep
    if (self.stopped.load(.acquire)) return error.HubStopped;
    const gop = try self.channels.getOrPut(self.alloc, spare_key);
    if (gop.found_existing) {
        try gop.value_ptr.*.subscribers.append(self.alloc, sub);
        return false;
    }
    gop.value_ptr.* = spare_entry;
    spare_entry.subscribers.append(self.alloc, sub) catch |err| {
        // roll the fresh map slot back out; the defer frees the spares
        _ = self.channels.remove(spare_key);
        return err;
    };
    consumed = true;
    return true;
}

/// Detach and release `sub` — the single-channel (per-fleet) path. The handle
/// is freed by the last `unref`, which may be an in-flight reader fan-out
/// rather than this call.
pub fn unsubscribe(self: *Self, sub: *Subscription) void {
    self.detachChannel(sub, sub.channel_name);
    sub.unref();
}

/// Detach `sub` from ONE channel, leaving the handle alive (a shared consumer
/// is detached once per attached channel, then destroyed by its owner). The
/// last subscriber off a channel sends the wire UNSUBSCRIBE outside the map
/// mutex, then repairs the one benign race: a new first-subscriber whose
/// SUBSCRIBE landed before our UNSUBSCRIBE would be silently muted, so if the
/// channel is live again we re-SUBSCRIBE (a double SUBSCRIBE is a Redis no-op).
pub fn detachChannel(self: *Self, sub: *Subscription, channel_name: []const u8) void {
    var freed_key: ?[]const u8 = null;
    var freed_entry: ?*ChannelEntry = null;
    {
        self.mutex.lockUncancelable(self.io);
        defer self.mutex.unlock(self.io);
        if (self.channels.getEntry(channel_name)) |entry| {
            const subs = &entry.value_ptr.*.subscribers;
            for (subs.items, 0..) |candidate, i| {
                if (candidate == sub) {
                    _ = subs.swapRemove(i);
                    break;
                }
            }
            if (subs.items.len == 0) {
                freed_key = entry.key_ptr.*;
                freed_entry = entry.value_ptr.*;
                self.channels.removeByPtr(entry.key_ptr);
            }
        }
    }
    if (freed_entry != null) {
        self.wireSendUnsubscribe(channel_name);
        if (self.channelLive(channel_name)) self.wireSendSubscribe(channel_name);
    }
    if (freed_entry) |entry| {
        entry.subscribers.deinit(self.alloc);
        self.alloc.destroy(entry);
    }
    if (freed_key) |key| self.alloc.free(key);
}

/// Live channel count (wire SUBSCRIBE cardinality). Test + admin surface.
pub fn channelCount(self: *Self) usize {
    self.mutex.lockUncancelable(self.io);
    defer self.mutex.unlock(self.io);
    return self.channels.count();
}

fn channelLive(self: *Self, channel_name: []const u8) bool {
    self.mutex.lockUncancelable(self.io);
    defer self.mutex.unlock(self.io);
    return self.channels.contains(channel_name);
}

/// Watchdog-bounded wire SUBSCRIBE under `wire`. Skips silently when no
/// connection is installed (reconnect gap — the post-redial sweep covers it).
/// pub for the reader's post-install delta pass.
pub fn wireSendSubscribe(self: *Self, channel_name: []const u8) void {
    hub_wire.wireSend(self, .subscribe, channel_name);
}

fn wireSendUnsubscribe(self: *Self, channel_name: []const u8) void {
    hub_wire.wireSend(self, .unsubscribe, channel_name);
}

pub fn testDisconnectConnection(self: *Self) bool {
    self.wire.lockUncancelable(self.io);
    defer self.wire.unlock(self.io);
    if (self.conn == null) return false;
    // Interrupt through the owner, exactly as a fired deadline would — the test
    // seam cannot reach a descriptor the production path no longer exposes.
    return self.wire_owner.target(self.wire_generation).interrupt() == .interrupted;
}
pub fn testHoldWire(self: *Self) void {
    self.wire.lockUncancelable(self.io);
}
pub fn testReleaseWire(self: *Self) void {
    self.wire.unlock(self.io);
}
const std = @import("std");
const common = @import("common");
const clock = common.clock;
const logging = @import("log");
const call_deadline = @import("call_deadline");
const redis_config = @import("../queue/redis_config.zig");
const redis_subscriber = @import("../queue/redis_subscriber.zig");
const reader = @import("subscription_hub_reader.zig");
const hub_wire = @import("subscription_hub_wire.zig");
const log = logging.scoped(.subscription_hub);
test {
    _ = @import("subscription_hub_test.zig");
}
