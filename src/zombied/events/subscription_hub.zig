//! SubscriptionHub — the process's ONE Redis pub/sub connection, fanned out
//! to every live SSE stream.
//!
//! Topology: a mutex-guarded `channel → subscribers` map in front of a single
//! `redis_subscriber` connection read by one dedicated reader thread. Wire
//! SUBSCRIBE/UNSUBSCRIBE happen only on a channel's first-subscriber /
//! last-subscriber edges; everything between is a map edit. The reader copies
//! each inbound frame into every subscriber's bounded queue (`Subscription`)
//! and never blocks on a slow consumer.
//!
//! Concurrency model: the reader thread is the only `conn` swapper and does
//! its blocking reads OUTSIDE the hub mutex; subscribe/unsubscribe send their
//! wire commands UNDER the mutex. Reads and writes own disjoint transport
//! state (poll/read on the fd vs the writer + TLS write keys), the same
//! one-reader/one-writer model the request-path client family uses — a
//! pathological interleave surfaces as a read error and heals through the
//! reconnect path.
//!
//! Loss semantics: pub/sub is the eyeballs surface, not the audit surface.
//! Frames published while the connection is being re-dialed are lost, exactly
//! as they were when each stream owned the connection that died; clients
//! backfill through the events cursor.

const SubscriptionHub = @This();

pub const Subscription = @import("subscription.zig");

alloc: std.mem.Allocator,
io: std.Io,
/// Resolved Redis config, BORROWED from the queue client's pool — set by
/// `start()`; must outlive the hub (serve.zig and the harness both deinit
/// the hub before the queue client).
cfg: ?redis_config.Config = null,
/// Guards `channels`, `conn` swaps, and wire writes. The reader's blocking
/// read runs outside it; fan-out and re-subscribe sweeps run under it.
mutex: std.Io.Mutex = .init,
channels: std.StringHashMapUnmanaged(*ChannelEntry) = .empty,
conn: ?redis_subscriber = null,
reader_thread: ?std.Thread = null,
stopped: std.atomic.Value(bool) = .init(false),

const ChannelEntry = struct {
    subscribers: std.ArrayList(*Subscription) = .empty,
};

/// Reader wake cadence — bounds stop latency, reconnect detection, and the
/// pickup delay for wire commands queued behind a quiet socket.
const HUB_READ_TIMEOUT_MS: u32 = 1_000;
/// A `nextMessage` null in under half the read timeout is a dead socket,
/// not an elapsed timeout (the discrimination the SSE loop used pre-hub).
const DISCONNECT_MIN_ELAPSED_MS: i64 = HUB_READ_TIMEOUT_MS / 2;
/// Redial pacing: one attempt per second, stop-checked every slice so
/// `stop()` never waits out a full backoff.
const RECONNECT_SLICE_MS: u64 = 250;
const RECONNECT_SLICES_PER_ATTEMPT: usize = 4;
/// `stop()` waits (bounded) for closed streams to detach so a late
/// `unsubscribe` can never touch a deinit'd channel map.
const STOP_DRAIN_MAX_MS: u64 = 5_000;
const STOP_DRAIN_POLL_MS: u64 = 50;

pub fn init(alloc: std.mem.Allocator, io: std.Io) SubscriptionHub {
    return .{ .alloc = alloc, .io = io };
}

/// Dial the shared connection and start the reader thread. Boot path —
/// failure here is a startup failure, mirroring the queue client connect.
pub fn start(self: *SubscriptionHub, cfg: redis_config.Config) !void {
    self.cfg = cfg;
    var conn = try redis_subscriber.connectFromConfig(self.io, self.alloc, cfg, .{ .read_timeout_ms = HUB_READ_TIMEOUT_MS });
    errdefer conn.deinit();
    conn.installReadTimeout();
    self.conn = conn;
    errdefer self.conn = null;
    self.reader_thread = try std.Thread.spawn(.{}, readerMain, .{self});
    log.debug("hub_started", .{ .host = cfg.host, .port = cfg.port });
}

/// Stop the reader, close every live subscription so stream threads drain,
/// and drop the connection. Idempotent; safe on a never-started hub.
pub fn stop(self: *SubscriptionHub) void {
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
    // Bounded drain: every closed stream detaches through unsubscribe(),
    // which touches the channel map — deinit() must not race that.
    var waited_ms: u64 = 0;
    while (self.channelCount() > 0 and waited_ms < STOP_DRAIN_MAX_MS) : (waited_ms += STOP_DRAIN_POLL_MS) {
        common.sleepNanos(STOP_DRAIN_POLL_MS * std.time.ns_per_ms);
    }
    if (self.channelCount() > 0) {
        log.warn("hub_stop_undrained", .{ .live_channels = self.channelCount() });
    }
    if (self.conn) |*c| {
        c.deinit();
        self.conn = null;
    }
}

/// Frees map storage. Call after `stop()` AND after every stream thread has
/// deregistered (registry `awaitEmpty`) — a late `unsubscribe` would touch
/// freed map storage. serve.zig's defer chain and the harness encode this
/// ordering.
pub fn deinit(self: *SubscriptionHub) void {
    var it = self.channels.iterator();
    while (it.next()) |kv| {
        kv.value_ptr.*.subscribers.deinit(self.alloc);
        self.alloc.destroy(kv.value_ptr.*);
        self.alloc.free(kv.key_ptr.*);
    }
    self.channels.deinit(self.alloc);
}

pub const SubscribeError = error{ OutOfMemory, HubStopped };

/// Attach a new subscriber to `channel_name`. First subscriber on a channel
/// sends the wire SUBSCRIBE; during a reconnect gap the wire send is skipped
/// and the post-redial sweep re-subscribes from the map.
pub fn subscribe(self: *SubscriptionHub, channel_name: []const u8) SubscribeError!*Subscription {
    const sub = try Subscription.create(self.alloc, self.io, channel_name);
    errdefer sub.destroy();
    try self.attach(sub);
    return sub;
}

fn attach(self: *SubscriptionHub, sub: *Subscription) SubscribeError!void {
    // Everything fallible is allocated before the lock; `consumed` routes the
    // spares to the map or back to the allocator on the way out. The explicit
    // catch covers the window before the consumed-defer is registered.
    const spare_key = try self.alloc.dupe(u8, sub.channel_name);
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
        return;
    }
    gop.value_ptr.* = spare_entry;
    spare_entry.subscribers.append(self.alloc, sub) catch |err| {
        // roll the fresh map slot back out; the defer frees the spares
        _ = self.channels.remove(spare_key);
        return err;
    };
    consumed = true;
    if (self.conn) |*c| c.sendSubscribe(sub.channel_name) catch |err| {
        // a failed write means the socket is dead; the reader's next read
        // fails too and the reconnect sweep re-subscribes from the map
        log.warn("hub_subscribe_send_failed", .{ .channel = sub.channel_name, .err = @errorName(err) });
    };
}

/// Detach and destroy `sub`. Last subscriber off a channel sends the wire
/// UNSUBSCRIBE (skipped during a reconnect gap — the fresh connection never
/// re-subscribes a channel that left the map).
pub fn unsubscribe(self: *SubscriptionHub, sub: *Subscription) void {
    var freed_key: ?[]const u8 = null;
    var freed_entry: ?*ChannelEntry = null;
    {
        self.mutex.lockUncancelable(self.io);
        defer self.mutex.unlock(self.io);
        if (self.channels.getEntry(sub.channel_name)) |entry| {
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
                if (self.conn) |*c| c.sendUnsubscribe(sub.channel_name) catch |err| {
                    log.debug("hub_unsubscribe_send_failed", .{ .err = @errorName(err) });
                };
            }
        }
    }
    if (freed_entry) |entry| {
        entry.subscribers.deinit(self.alloc);
        self.alloc.destroy(entry);
    }
    if (freed_key) |key| self.alloc.free(key);
    sub.destroy();
}

/// Live channel count (wire SUBSCRIBE cardinality). Test + admin surface.
pub fn channelCount(self: *SubscriptionHub) usize {
    self.mutex.lockUncancelable(self.io);
    defer self.mutex.unlock(self.io);
    return self.channels.count();
}

fn readerMain(self: *SubscriptionHub) void {
    while (!self.stopped.load(.acquire)) {
        const before_ms = clock.nowMillis();
        // safe because: this thread is the only conn swapper, so reading the
        // optional outside the mutex is single-writer; writers only touch the
        // conn's write half, under the mutex.
        const maybe_msg = self.conn.?.nextMessage() catch {
            self.reconnect();
            continue;
        };
        if (maybe_msg) |msg| {
            var m = msg;
            defer m.deinit(self.alloc);
            self.dispatch(m.channel, m.payload);
            continue;
        }
        // null = read-timeout tick OR closed socket; block time tells them apart
        if (clock.nowMillis() - before_ms < DISCONNECT_MIN_ELAPSED_MS) self.reconnect();
    }
}

fn dispatch(self: *SubscriptionHub, channel: []const u8, payload: []const u8) void {
    self.mutex.lockUncancelable(self.io);
    defer self.mutex.unlock(self.io);
    // a frame racing the last unsubscribe simply has nobody to deliver to
    const entry = self.channels.get(channel) orelse return;
    for (entry.subscribers.items) |sub| sub.push(payload);
}

/// Reader-thread only: drop the dead connection, redial with stop-checked
/// pacing, then re-subscribe every channel that still has viewers.
fn reconnect(self: *SubscriptionHub) void {
    log.warn("hub_connection_lost", .{ .live_channels = self.channelCount() });
    self.dropConn();
    while (!self.stopped.load(.acquire)) {
        var i: usize = 0;
        while (i < RECONNECT_SLICES_PER_ATTEMPT) : (i += 1) {
            if (self.stopped.load(.acquire)) return;
            common.sleepNanos(RECONNECT_SLICE_MS * std.time.ns_per_ms);
        }
        var fresh = redis_subscriber.connectFromConfig(self.io, self.alloc, self.cfg.?, .{ .read_timeout_ms = HUB_READ_TIMEOUT_MS }) catch |err| {
            log.warn("hub_redial_failed", .{ .err = @errorName(err) });
            continue;
        };
        fresh.installReadTimeout();
        if (self.resubscribeAll(fresh)) {
            metrics.incSseHubReconnects();
            log.debug("hub_reconnected", .{ .live_channels = self.channelCount() });
            return;
        }
        self.dropConn();
    }
}

fn dropConn(self: *SubscriptionHub) void {
    self.mutex.lockUncancelable(self.io);
    var dead = self.conn;
    self.conn = null;
    self.mutex.unlock(self.io);
    if (dead) |*c| c.deinit();
}

/// Install `fresh` as the live connection and replay SUBSCRIBE for every
/// mapped channel. False = a send failed (socket already dead again).
fn resubscribeAll(self: *SubscriptionHub, fresh: redis_subscriber) bool {
    self.mutex.lockUncancelable(self.io);
    defer self.mutex.unlock(self.io);
    self.conn = fresh;
    var it = self.channels.keyIterator();
    while (it.next()) |key| {
        self.conn.?.sendSubscribe(key.*) catch |err| {
            log.warn("hub_resubscribe_failed", .{ .channel = key.*, .err = @errorName(err) });
            return false;
        };
    }
    return true;
}

const std = @import("std");
const common = @import("common");
const clock = common.clock;
const logging = @import("log");
const redis_config = @import("../queue/redis_config.zig");
const redis_subscriber = @import("../queue/redis_subscriber.zig");
const metrics = @import("../observability/metrics.zig");
const log = logging.scoped(.subscription_hub);

test {
    _ = @import("subscription_hub_test.zig");
}
