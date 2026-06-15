const std = @import("std");
const pg = @import("pg");
const events_bus = @import("../events/bus.zig");
const queue_redis = @import("../queue/redis_client.zig");
const approval_gate_sweeper = @import("../agent/approval_gate_sweeper.zig");
const liveness_sweeper = @import("../fleet/liveness_sweeper.zig");
const reclaim_sweeper = @import("../fleet/reclaim_sweeper.zig");
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
    ) !void {
        events_bus.install(&self.event_bus);
        self.installed = true;
        errdefer self.stop();

        self.signal_thread = try std.Thread.spawn(.{}, serve_shutdown.signalWatcher, .{});
        self.event_thread = try std.Thread.spawn(.{}, events_bus.runThread, .{&self.event_bus});
        self.approval_sweeper_thread = try std.Thread.spawn(.{}, approval_gate_sweeper.run, .{ pool, queue, alloc, serve_shutdown.flag() });
        self.liveness_sweeper_thread = try std.Thread.spawn(.{}, liveness_sweeper.run, .{ pool, alloc, serve_shutdown.flag() });
        self.reclaim_sweeper_thread = try std.Thread.spawn(.{}, reclaim_sweeper.run, .{ pool, queue, alloc, serve_shutdown.flag() });
    }

    pub fn stop(self: *Self) void {
        if (self.stopped) return;
        self.stopped = true;
        serve_shutdown.request();
        self.event_bus.stop();
        join(&self.signal_thread);
        join(&self.event_thread);
        join(&self.approval_sweeper_thread);
        join(&self.liveness_sweeper_thread);
        join(&self.reclaim_sweeper_thread);
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
