const std = @import("std");
const pg = @import("pg");
const events_bus = @import("../events/bus.zig");
const queue_redis = @import("../queue/redis_client.zig");
const approval_gate_sweeper = @import("../fleet_runtime/approval_gate_sweeper.zig");
const liveness_sweeper = @import("../fleet/liveness_sweeper.zig");
const reclaim_sweeper = @import("../fleet/reclaim_sweeper.zig");
const outbound_worker = @import("../http/handlers/connectors/outbound/worker.zig");
const slack_post = @import("../http/handlers/connectors/slack/post.zig");
const bounded_fetch = @import("../http/handlers/connectors/bounded_fetch.zig");
const serve_shutdown = @import("serve_shutdown.zig");

/// Background threads owned by `serve.zig`.
pub const Threads = struct {
    const Self = @This();

    event_bus: events_bus.Bus = events_bus.Bus.init(),
    signal_thread: ?std.Thread = null,
    event_thread: ?std.Thread = null,
    approval_sweeper_thread: ?std.Thread = null,
    liveness_sweeper_thread: ?std.Thread = null,
    reclaim_sweeper_thread: ?std.Thread = null,
    outbound_consumer_thread: ?std.Thread = null,
    installed: bool = false,
    stopped: bool = false,

    pub fn init() Threads {
        return .{};
    }

    pub fn start(
        self: *Self,
        pool: *pg.Pool,
        queue: *queue_redis.Client,
        alloc: std.mem.Allocator,
        sched: *bounded_fetch.Scheduler,
    ) !void {
        events_bus.install(&self.event_bus);
        self.installed = true;
        errdefer self.stop();

        self.signal_thread = try std.Thread.spawn(.{}, serve_shutdown.signalWatcher, .{});
        self.event_thread = try std.Thread.spawn(.{}, events_bus.runThread, .{&self.event_bus});
        self.approval_sweeper_thread = try std.Thread.spawn(.{}, approval_gate_sweeper.run, .{ pool, queue, alloc, serve_shutdown.flag() });
        self.liveness_sweeper_thread = try std.Thread.spawn(.{}, liveness_sweeper.run, .{ pool, alloc, serve_shutdown.flag() });
        self.reclaim_sweeper_thread = try std.Thread.spawn(.{}, reclaim_sweeper.run, .{ pool, queue, alloc, serve_shutdown.flag() });
        // §4 connector:outbound answer-delivery consumer — provider-routed; uses
        // the real Slack API base in production (a test drives the worker directly
        // with a FakeSlack loopback base instead of going through boot).
        self.outbound_consumer_thread = try std.Thread.spawn(.{}, outbound_worker.run, .{ pool, queue, alloc, serve_shutdown.flag(), slack_post.SLACK_API_BASE_DEFAULT, sched });
    }

    pub fn stop(self: *Self) void {
        if (self.stopped) return;
        self.stopped = true;
        serve_shutdown.request();
        // The watcher only retires after stopping a live server; at teardown
        // the server is already down (or never came up), so disarm it before
        // the join or a boot-failure path would hang here.
        serve_shutdown.disarmWatcher();
        self.event_bus.stop();
        join(&self.signal_thread);
        join(&self.event_thread);
        join(&self.approval_sweeper_thread);
        join(&self.liveness_sweeper_thread);
        join(&self.reclaim_sweeper_thread);
        join(&self.outbound_consumer_thread);
        if (self.installed) {
            events_bus.uninstall();
            self.installed = false;
        }
    }
};

fn join(thread: *?std.Thread) void {
    if (thread.*) |*t| {
        t.join();
        thread.* = null;
    }
}
