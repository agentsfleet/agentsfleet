const std = @import("std");
const pg = @import("pg");
const events_bus = @import("../events/bus.zig");
const queue_redis = @import("../queue/redis_client.zig");
const approval_gate_sweeper = @import("../fleet_runtime/approval_gate_sweeper.zig");
const liveness_sweeper = @import("../fleet/liveness_sweeper.zig");
const reclaim_sweeper = @import("../fleet/reclaim_sweeper.zig");
const retention_sweeper = @import("../fleet/retention_sweeper.zig");
const repair_verification_dispatcher = @import("../fleet/repair_verification_dispatcher.zig");
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
    retention_sweeper_thread: ?std.Thread = null,
    repair_verification_dispatcher_thread: ?std.Thread = null,
    outbound_consumer_thread: ?std.Thread = null,
    stop_flag: *std.atomic.Value(bool) = serve_shutdown.flag(),
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
        self.approval_sweeper_thread = try std.Thread.spawn(.{}, approval_gate_sweeper.run, .{ pool, queue, alloc, self.stop_flag });
        self.liveness_sweeper_thread = try std.Thread.spawn(.{}, liveness_sweeper.run, .{ pool, alloc, self.stop_flag });
        self.reclaim_sweeper_thread = try std.Thread.spawn(.{}, reclaim_sweeper.run, .{ pool, queue, alloc, self.stop_flag });
        self.retention_sweeper_thread = try std.Thread.spawn(.{}, retention_sweeper.run, .{ pool, self.stop_flag });
        try self.startRepairVerificationDispatcher(pool, queue, alloc);
        // §4 connector:outbound answer-delivery consumer — provider-routed; uses
        // the real Slack API base in production (a test drives the worker directly
        // with a FakeSlack loopback base instead of going through boot).
        self.outbound_consumer_thread = try std.Thread.spawn(.{}, outbound_worker.run, .{ pool, queue, alloc, self.stop_flag, slack_post.SLACK_API_BASE_DEFAULT, sched });
    }

    fn startRepairVerificationDispatcher(
        self: *Self,
        pool: *pg.Pool,
        queue: *queue_redis.Client,
        alloc: std.mem.Allocator,
    ) !void {
        self.repair_verification_dispatcher_thread = try std.Thread.spawn(
            .{},
            repair_verification_dispatcher.run,
            .{ pool, queue, alloc, self.stop_flag },
        );
    }

    pub fn stop(self: *Self) void {
        if (self.stopped) return;
        self.stopped = true;
        serve_shutdown.request();
        // The watcher only retires after stopping a live server; at teardown
        // the server is already down (or never came up), so disarm it before
        // the join or a boot-failure path would hang here.
        serve_shutdown.disarmWatcher();
        self.stop_flag.store(true, .release);
        self.event_bus.stop();
        join(&self.signal_thread);
        join(&self.event_thread);
        join(&self.approval_sweeper_thread);
        join(&self.liveness_sweeper_thread);
        join(&self.reclaim_sweeper_thread);
        join(&self.retention_sweeper_thread);
        join(&self.repair_verification_dispatcher_thread);
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

test "integration: repair dispatcher thread is installed and joined" {
    const harness_mod = @import("../http/test_harness.zig");
    const auth_mw = @import("../auth/middleware/mod.zig");
    const Configure = struct {
        fn registry(_: *auth_mw.MiddlewareRegistry, _: *harness_mod.TestHarness) anyerror!void {}
    };
    const h = harness_mod.TestHarness.start(std.testing.allocator, .{
        .configureRegistry = Configure.registry,
    }) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    serve_shutdown.reset();
    defer serve_shutdown.reset();
    var stop_flag = std.atomic.Value(bool).init(false);
    var threads = Threads.init();
    threads.stop_flag = &stop_flag;
    try threads.startRepairVerificationDispatcher(h.pool, &h.queue, std.testing.allocator);
    try std.testing.expect(threads.repair_verification_dispatcher_thread != null);
    threads.stop();
    try std.testing.expect(stop_flag.load(.acquire));
    try std.testing.expect(threads.repair_verification_dispatcher_thread == null);
}
