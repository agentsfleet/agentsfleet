//! SubscriptionHub's reader thread: the blocking-read loop, frame fan-out,
//! and the reconnect/redial machinery. Implementation detail of
//! `subscription_hub.zig` — split out by concern (the hub file owns the
//! lifecycle + subscriber-facing surface; this file owns the one thread that
//! reads the shared connection).
//!
//! Thread confinement: every fn here runs on the reader thread only (spawned
//! by `hub.start`, joined by `hub.stop`), so no lock is needed for its own
//! locals; shared state is reached through the hub's two documented locks
//! (`mutex` for the channel map, `wire` for the connection + sends).

const std = @import("std");
const common = @import("common");
const clock = common.clock;
const logging = @import("log");
const call_deadline = @import("call_deadline");
const redis_subscriber = @import("../queue/redis_subscriber.zig");
const wire = @import("subscription_hub_wire.zig");
const metrics = @import("../observability/metrics.zig");
const Hub = @import("subscription_hub.zig");

const log = logging.scoped(.subscription_hub);
const S_RESUBSCRIBE_FAILED = "hub_resubscribe_failed";

/// Redial pacing: one attempt per second, stop-checked every slice so
/// `stop()` never waits out a full backoff.
const RECONNECT_SLICE_MS: u64 = 250;
const RECONNECT_SLICES_PER_ATTEMPT: usize = 4;

/// Reader-thread entrypoint: block on the shared connection, fan each frame
/// out, reconnect on failure, until `hub.stop()` raises the flag.
pub fn readerMain(hub: *Hub) void {
    while (!hub.stopped.load(.acquire)) { // safe because: pairs with stop()'s release via swap.
        const before_ms = clock.nowMillis();
        // The reader dereferences `conn` outside the wire lock: only this
        // thread (and post-join stop()) ever swaps the field, and the read
        // half (fd poll/read + read keys) is disjoint from the write half the
        // wire lock serializes — see the hub's concurrency model doc.
        //
        // Non-null is guaranteed by an invariant spanning three functions
        // (`start` installs, `reconnect` only returns with one installed or the
        // hub stopped, `stop` joins this thread before nulling). Heal instead of
        // unwrapping: a refactor that breaks that invariant should reconnect,
        // not crash the daemon on a null dereference.
        const conn = if (hub.conn) |*c| c else {
            reconnect(hub);
            continue;
        };
        const maybe_msg = conn.nextMessage() catch {
            reconnect(hub);
            continue;
        };
        if (maybe_msg) |msg| {
            var m = msg;
            defer m.deinit(hub.alloc);
            dispatch(hub, m.channel, m.payload);
            continue;
        }
        // null = timeout tick OR closed socket; a null in under half the
        // (instance) timeout is a dead socket.
        if (clock.nowMillis() - before_ms < @divTrunc(@as(i64, hub.read_timeout_ms), 2)) reconnect(hub);
    }
}

/// Fan one frame out to every viewer of `channel`.
///
/// The map mutex covers the LOOKUP and a pointer snapshot — nothing else. The
/// push itself (a payload copy, the subscription's own lock, a futex wake, per
/// viewer) runs with the lock released, so a hot channel with many viewers
/// cannot stall `subscribe`/`unsubscribe`/`channelCount` for the duration of
/// its fan-out (C3: never do blocking work under a lock the consumer needs).
///
/// Pushing unlocked means a racing `unsubscribe` could otherwise free a handle
/// mid-push, so the snapshot takes a ref per subscriber and drops it after —
/// the last ref frees, never the unsubscribe that raced us.
fn dispatch(hub: *Hub, channel: []const u8, payload: []const u8) void {
    hub.reader_scratch.clearRetainingCapacity();
    {
        hub.mutex.lockUncancelable(hub.io);
        defer hub.mutex.unlock(hub.io);
        // a frame racing the last unsubscribe simply has nobody to deliver to
        const entry = hub.channels.get(channel) orelse return;
        hub.reader_scratch.ensureTotalCapacity(hub.alloc, entry.subscribers.items.len) catch {
            // Out of memory for the snapshot: deliver under the lock rather
            // than drop the frame. Briefly contended beats silently lossy.
            for (entry.subscribers.items) |sub| pushOne(sub, channel, payload);
            return;
        };
        for (entry.subscribers.items) |sub| {
            sub.ref();
            hub.reader_scratch.appendAssumeCapacity(sub);
        }
    }
    for (hub.reader_scratch.items) |sub| {
        pushOne(sub, channel, payload);
        sub.unref();
    }
}

/// A shared consumer (one queue fed by N channels) needs to know which channel
/// a frame arrived on; a per-channel consumer already does.
fn pushOne(sub: *Hub.Subscription, channel: []const u8, payload: []const u8) void {
    if (sub.tagged) sub.pushTagged(channel, payload) else sub.push(payload);
}

/// Drop the dead connection, redial with stop-checked pacing, then
/// re-subscribe every channel that still has viewers.
fn reconnect(hub: *Hub) void {
    log.warn("hub_connection_lost", .{ .live_channels = hub.channelCount() });
    dropConn(hub);
    while (!hub.stopped.load(.acquire)) {
        var i: usize = 0;
        while (i < RECONNECT_SLICES_PER_ATTEMPT) : (i += 1) {
            if (hub.stopped.load(.acquire)) return;
            common.sleepNanos(RECONNECT_SLICE_MS * std.time.ns_per_ms);
        }
        var fresh = wire.connectBounded(hub) catch |err| {
            log.warn("hub_redial_failed", .{ .err = @errorName(err) });
            continue;
        };
        fresh.installReadTimeout();
        if (resubscribeAll(hub, fresh)) {
            metrics.incSseHubReconnects();
            log.debug("hub_reconnected", .{ .live_channels = hub.channelCount() });
            return;
        }
        dropConn(hub);
    }
}

/// Swap the connection out under the wire lock (serializing against in-flight
/// sends — bounded by the send watchdog), then close it unlocked.
fn dropConn(hub: *Hub) void {
    hub.wire.lockUncancelable(hub.io);
    // Retire the wire generation BEFORE the socket dies: any armed send
    // registration is now stale, so a late fire cannot reach the descriptor
    // number the kernel is about to recycle.
    wire.retireConnection(hub);
    var dead = hub.conn;
    hub.conn = null;
    hub.wire.unlock(hub.io);
    if (dead) |*c| c.deinit();
}

/// Replay SUBSCRIBE for every mapped channel on `fresh`, then install it.
/// The O(N) sends run on the NOT-YET-INSTALLED conn — single-owner, no lock,
/// and a full send buffer cannot stall dispatch/subscribe/unsubscribe (the
/// names are a duped snapshot: a racing last-unsubscribe frees the map key).
/// After install, a delta pass covers channels subscribed during the window
/// (attach skips its wire send while conn is null); a channel whose attach
/// raced the install double-SUBSCRIBEs, which Redis treats as a no-op.
/// False = send failure or hub stopped; `fresh` is consumed either way.
fn resubscribeAll(hub: *Hub, fresh: redis_subscriber) bool {
    var conn = fresh;
    const before = snapshotChannelNames(hub) catch {
        conn.deinit();
        return false; // OOM → treat as a failed attempt; the redial loop retries
    };
    defer freeNames(hub, before);
    // The sweep's own control block: `conn` is not installed yet, so the hub's
    // wire owner cannot bound these sends. Every guard is finished and the
    // generation retired BEFORE `conn` moves into the hub — a Subscriber is
    // moved by value, so its pre-move storage is never a safe interrupt target.
    var sweep_owner: call_deadline.SocketOwner = .{};
    const sweep_generation = sweep_owner.beginAttempt();
    _ = conn.attachTo(&sweep_owner, sweep_generation);
    const sched = hub.sched orelse {
        conn.deinit();
        return false; // no scheduler → no bounded send; fail the attempt, never send unbounded
    };
    for (before) |name| {
        // Stop is checked BETWEEN sends: each send is individually bounded, so
        // without this a shutdown racing a large sweep would wait out the
        // deadline once per channel — multiplying stop latency by channel count.
        if (hub.stopped.load(.acquire)) {
            sweep_owner.endAttempt();
            conn.deinit();
            return false;
        }
        var guard = sched.arm(sweep_owner.target(sweep_generation), hub.send_timeout_ms) catch {
            sweep_owner.endAttempt();
            conn.deinit();
            return false; // arming refused → the attempt fails closed; the redial loop retries
        };
        const sent = conn.sendSubscribe(name);
        _ = guard.finish();
        sent catch |err| {
            log.warn(S_RESUBSCRIBE_FAILED, .{ .channel = name, .err = @errorName(err) });
            sweep_owner.endAttempt();
            conn.deinit();
            return false;
        };
    }
    // Quiescent before the move: every guard above is finished, and retiring
    // the generation makes any copied target inert.
    sweep_owner.endAttempt();
    hub.wire.lockUncancelable(hub.io);
    if (hub.stopped.load(.acquire)) { // safe because: pairs with stop()'s release via swap.
        hub.wire.unlock(hub.io);
        conn.deinit();
        return false;
    }
    hub.conn = conn;
    // Rebind the wire owner to the connection that now exists: fresh
    // generation, fresh socket. Without this, post-reconnect sends would arm
    // against the RETIRED descriptor — and a fire could shut down whatever
    // connection the kernel recycled that number onto.
    wire.adoptConnection(hub);
    hub.wire.unlock(hub.io);

    const after = snapshotChannelNames(hub) catch return true; // installed; the next reconnect sweeps
    defer freeNames(hub, after);
    for (after) |name| {
        if (containsName(before, name)) continue;
        hub.wireSendSubscribe(name); // send failure logs + heals via the reader's next read
    }
    return true;
}

/// Duped channel names under the map mutex; caller owns the slice + strings.
fn snapshotChannelNames(hub: *Hub) error{OutOfMemory}![]const []const u8 {
    hub.mutex.lockUncancelable(hub.io);
    defer hub.mutex.unlock(hub.io);
    var names = try hub.alloc.alloc([]const u8, hub.channels.count());
    errdefer hub.alloc.free(names);
    var i: usize = 0;
    errdefer for (names[0..i]) |n| hub.alloc.free(n);
    var it = hub.channels.keyIterator();
    while (it.next()) |key| : (i += 1) names[i] = try hub.alloc.dupe(u8, key.*);
    return names;
}

fn freeNames(hub: *Hub, names: []const []const u8) void {
    for (names) |n| hub.alloc.free(n);
    hub.alloc.free(names);
}

fn containsName(names: []const []const u8, key: []const u8) bool {
    for (names) |n| if (std.mem.eql(u8, n, key)) return true;
    return false;
}
