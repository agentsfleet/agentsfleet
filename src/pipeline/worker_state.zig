const std = @import("std");
const metrics = @import("../observability/metrics.zig");
const worker_runtime = @import("worker_runtime.zig");

pub const WorkerState = struct {
    running: std.atomic.Value(bool),
    in_flight_runs: std.atomic.Value(u32),

    pub fn init() WorkerState {
        return .{
            .running = std.atomic.Value(bool).init(true),
            .in_flight_runs = std.atomic.Value(u32).init(0),
        };
    }

    pub fn beginRun(self: *WorkerState) void {
        _ = self.in_flight_runs.fetchAdd(1, .acq_rel);
        metrics.setWorkerInFlightRuns(self.currentInFlightRuns());
    }

    pub fn endRun(self: *WorkerState) void {
        const prev = self.in_flight_runs.fetchSub(1, .acq_rel);
        std.debug.assert(prev > 0);
        metrics.setWorkerInFlightRuns(self.currentInFlightRuns());
    }

    pub fn currentInFlightRuns(self: *const WorkerState) u32 {
        return self.in_flight_runs.load(.acquire);
    }
};

pub fn beginRunIfActive(worker_state: *WorkerState) worker_runtime.WorkerError!void {
    if (!worker_state.running.load(.acquire)) return worker_runtime.WorkerError.ShutdownRequested;
    worker_state.beginRun();
}

test "worker state in-flight run counter tracks begin/end safely" {
    var ws = WorkerState.init();
    try std.testing.expectEqual(@as(u32, 0), ws.currentInFlightRuns());
    ws.beginRun();
    ws.beginRun();
    try std.testing.expectEqual(@as(u32, 2), ws.currentInFlightRuns());
    ws.endRun();
    try std.testing.expectEqual(@as(u32, 1), ws.currentInFlightRuns());
    ws.endRun();
    try std.testing.expectEqual(@as(u32, 0), ws.currentInFlightRuns());
}

test "beginRunIfActive rejects stopped worker without incrementing in-flight" {
    var ws = WorkerState.init();
    ws.running.store(false, .release);

    try std.testing.expectError(worker_runtime.WorkerError.ShutdownRequested, beginRunIfActive(&ws));
    try std.testing.expectEqual(@as(u32, 0), ws.currentInFlightRuns());
}

test "beginRunIfActive increments in-flight when running" {
    var ws = WorkerState.init();
    ws.running.store(true, .release);

    try beginRunIfActive(&ws);
    try std.testing.expectEqual(@as(u32, 1), ws.currentInFlightRuns());
    ws.endRun();
    try std.testing.expectEqual(@as(u32, 0), ws.currentInFlightRuns());
}

const CounterRaceCtx = struct {
    ws: *WorkerState,
    iterations: u32,
};

fn runBalancedCounterLoop(ctx: CounterRaceCtx) void {
    var i: u32 = 0;
    while (i < ctx.iterations) : (i += 1) {
        ctx.ws.beginRun();
        ctx.ws.endRun();
    }
}

test "integration: worker state in-flight run counter is balanced across threads" {
    var ws = WorkerState.init();
    const thread_count: usize = 6;
    const iterations: u32 = 500;

    const threads = try std.testing.allocator.alloc(std.Thread, thread_count);
    defer std.testing.allocator.free(threads);

    for (threads) |*thread| {
        thread.* = try std.Thread.spawn(.{}, runBalancedCounterLoop, .{CounterRaceCtx{
            .ws = &ws,
            .iterations = iterations,
        }});
    }
    for (threads) |*thread| thread.join();

    try std.testing.expectEqual(@as(u32, 0), ws.currentInFlightRuns());
}
