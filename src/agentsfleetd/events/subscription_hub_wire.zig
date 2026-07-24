//! Bounded connection setup and bounded wire writes for the subscription hub.
//! Implementation detail of `subscription_hub.zig`; split out to keep that file
//! within the length cap and to keep all deadline handling in one place.

const std = @import("std");
const call_deadline = @import("call_deadline");
const logging = @import("log");
const redis_subscriber = @import("../queue/redis_subscriber.zig");
const Hub = @import("subscription_hub.zig");

const log = logging.scoped(.subscription_hub);

const EV_SEND_REFUSED = "hub_wire_send_refused";
const EV_SEND_FAILED = "hub_wire_send_failed";
const EV_SEND_SLOWPATH = "hub_wire_write_slowpath";

pub const WireVerb = enum { subscribe, unsubscribe };

/// Dial one attempt under a single absolute budget covering resolve, dial, TLS,
/// and AUTH. The setup guard is finished before the connection value is
/// returned, so the caller may move it into the hub: a Subscriber is returned
/// by value, and an interrupt target must never point at storage that moves.
pub fn connectBounded(hub: *Hub) !redis_subscriber {
    const sched = hub.sched orelse return error.DeadlineSchedulerUnavailable;
    const cfg = hub.cfg orelse return error.RedisConfigMissing;

    var owner: call_deadline.SocketOwner = .{};
    const generation = owner.beginAttempt();
    const deadline_ns = std.Io.Clock.boot.now(hub.io).toNanoseconds() +
        @as(i96, hub.setup_timeout_ms) * std.time.ns_per_ms;

    var guard = sched.arm(owner.target(generation), hub.setup_timeout_ms) catch
        return error.DeadlineSchedulerUnavailable;
    // Quiescence barrier before the value escapes: after `finish` returns, no
    // interrupt callback is running or can start against `owner`.
    defer {
        owner.endAttempt();
        _ = guard.finish();
    }

    return redis_subscriber.connectFromConfig(
        hub.io,
        hub.alloc,
        cfg,
        .{ .read_timeout_ms = hub.read_timeout_ms },
        .{ .owner = &owner, .generation = generation, .deadline_ns = deadline_ns },
    );
}

/// Publish a freshly installed connection as the hub's current wire generation.
/// Call with `wire` held, immediately after `hub.conn` is set.
pub fn adoptConnection(hub: *Hub) void {
    hub.wire_generation = hub.wire_owner.beginAttempt();
    const conn = if (hub.conn) |*c| c else return;
    _ = conn.attachTo(&hub.wire_owner, hub.wire_generation);
}

/// Retire the current wire generation so a deadline that fires afterwards is
/// stale. Call with `wire` held, before the connection is deinit'd.
pub fn retireConnection(hub: *Hub) void {
    hub.wire_owner.endAttempt();
}

/// One deadline-bounded wire write under `wire`. Skips silently when no
/// connection is installed (reconnect gap — the post-redial sweep covers it).
pub fn wireSend(hub: *Hub, verb: WireVerb, channel_name: []const u8) void {
    hub.wire.lockUncancelable(hub.io);
    defer hub.wire.unlock(hub.io);
    const conn_ptr = if (hub.conn) |*c| c else return;
    const sched = hub.sched orelse {
        log.warn(EV_SEND_REFUSED, .{ .channel = channel_name, .reason = "scheduler_absent" });
        return;
    };
    var guard = sched.arm(hub.wire_owner.target(hub.wire_generation), hub.send_timeout_ms) catch {
        // No bound available → an unbounded send is refused outright; the
        // reconnect sweep re-subscribes from the map once the reader replaces
        // the connection.
        log.warn(EV_SEND_REFUSED, .{ .channel = channel_name, .reason = "scheduler_unavailable" });
        return;
    };

    const send_result = switch (verb) {
        .subscribe => conn_ptr.sendSubscribe(channel_name),
        .unsubscribe => conn_ptr.sendUnsubscribe(channel_name),
    };
    send_result catch |err| {
        // A failed write means the socket is dead (or was shut down by the
        // deadline); the reader's next read fails too and the reconnect sweep
        // re-subscribes from the map.
        log.warn(EV_SEND_FAILED, .{ .verb = @tagName(verb), .channel = channel_name, .err = @errorName(err) });
    };

    // Per-send precision: `fired` means THIS send's registration fired, which a
    // shared interrupted flag could not distinguish from an earlier send's.
    if (guard.finish() == .fired) {
        log.warn(EV_SEND_SLOWPATH, .{
            .verb = @tagName(verb),
            .channel = channel_name,
            .deadline_ms = hub.send_timeout_ms,
        });
    }
}
