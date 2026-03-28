const std = @import("std");
const metrics = @import("../observability/metrics.zig");
const worker_runtime = @import("worker_runtime.zig");

pub const DrainPhase = enum(u8) {
    running = 0,
    draining = 1,
    drained = 2,
};

pub const WorkerState = struct {
    running: std.atomic.Value(bool),
    drain_phase: std.atomic.Value(u8),
    in_flight_runs: std.atomic.Value(u32),

    pub fn init() WorkerState {
        return .{
            .running = std.atomic.Value(bool).init(true),
            .drain_phase = std.atomic.Value(u8).init(@intFromEnum(DrainPhase.running)),
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

    pub fn getDrainPhase(self: *const WorkerState) DrainPhase {
        return @enumFromInt(self.drain_phase.load(.acquire));
    }

    pub fn isAcceptingWork(self: *const WorkerState) bool {
        return self.getDrainPhase() == .running;
    }

    pub fn startDrain(self: *WorkerState) bool {
        return self.drain_phase.cmpxchgStrong(
            @intFromEnum(DrainPhase.running),
            @intFromEnum(DrainPhase.draining),
            .acq_rel,
            .acquire,
        ) == null;
    }

    pub fn completeDrain(self: *WorkerState) void {
        self.drain_phase.store(@intFromEnum(DrainPhase.drained), .release);
        self.running.store(false, .release);
    }
};

pub fn beginRunIfActive(worker_state: *WorkerState) worker_runtime.WorkerError!void {
    if (!worker_state.isAcceptingWork()) return worker_runtime.WorkerError.ShutdownRequested;
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
    ws.completeDrain();

    try std.testing.expectError(worker_runtime.WorkerError.ShutdownRequested, beginRunIfActive(&ws));
    try std.testing.expectEqual(@as(u32, 0), ws.currentInFlightRuns());
}

test "beginRunIfActive rejects draining worker without incrementing in-flight" {
    var ws = WorkerState.init();
    try std.testing.expect(ws.startDrain());

    try std.testing.expectError(worker_runtime.WorkerError.ShutdownRequested, beginRunIfActive(&ws));
    try std.testing.expectEqual(@as(u32, 0), ws.currentInFlightRuns());
}

test "beginRunIfActive increments in-flight when running" {
    var ws = WorkerState.init();

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

test "WorkerState.init starts running with zero in-flight" {
    const ws = WorkerState.init();
    try std.testing.expect(ws.running.load(.acquire));
    try std.testing.expectEqual(DrainPhase.running, ws.getDrainPhase());
    try std.testing.expectEqual(@as(u32, 0), ws.currentInFlightRuns());
}

test "WorkerState drain phase transitions running to draining to drained" {
    var ws = WorkerState.init();
    try std.testing.expectEqual(DrainPhase.running, ws.getDrainPhase());
    try std.testing.expect(ws.running.load(.acquire));

    try std.testing.expect(ws.startDrain());
    try std.testing.expectEqual(DrainPhase.draining, ws.getDrainPhase());
    try std.testing.expect(ws.running.load(.acquire));

    ws.completeDrain();
    try std.testing.expectEqual(DrainPhase.drained, ws.getDrainPhase());
    try std.testing.expect(!ws.running.load(.acquire));
}

test "WorkerState startDrain is idempotent — second call returns false" {
    var ws = WorkerState.init();
    try std.testing.expect(ws.startDrain());
    try std.testing.expect(!ws.startDrain());
    try std.testing.expectEqual(DrainPhase.draining, ws.getDrainPhase());
}

test "WorkerState isAcceptingWork reflects drain phase" {
    var ws = WorkerState.init();
    try std.testing.expect(ws.isAcceptingWork());

    _ = ws.startDrain();
    try std.testing.expect(!ws.isAcceptingWork());

    ws.completeDrain();
    try std.testing.expect(!ws.isAcceptingWork());
}

test "beginRunIfActive allows multiple concurrent runs while running" {
    var ws = WorkerState.init();
    try beginRunIfActive(&ws);
    try beginRunIfActive(&ws);
    try std.testing.expectEqual(@as(u32, 2), ws.currentInFlightRuns());
    ws.endRun();
    ws.endRun();
}

// ── T1 Happy Path — DrainPhase enum values ─────────────────────────────────

test "DrainPhase enum has correct u8 backing values" {
    try std.testing.expectEqual(@as(u8, 0), @intFromEnum(DrainPhase.running));
    try std.testing.expectEqual(@as(u8, 1), @intFromEnum(DrainPhase.draining));
    try std.testing.expectEqual(@as(u8, 2), @intFromEnum(DrainPhase.drained));
}

test "DrainPhase enum has exactly 3 values" {
    const fields = @typeInfo(DrainPhase).@"enum".fields;
    try std.testing.expectEqual(@as(usize, 3), fields.len);
}

// ── T2 Edge Cases ──────────────────────────────────────────────────────────

test "startDrain on already-drained worker returns false" {
    var ws = WorkerState.init();
    try std.testing.expect(ws.startDrain());
    ws.completeDrain();
    try std.testing.expectEqual(DrainPhase.drained, ws.getDrainPhase());
    // startDrain expects running→draining; drained state should fail
    try std.testing.expect(!ws.startDrain());
}

test "completeDrain is idempotent — calling twice does not panic" {
    var ws = WorkerState.init();
    _ = ws.startDrain();
    ws.completeDrain();
    // Second call: already drained, stores same values again — must not panic
    ws.completeDrain();
    try std.testing.expectEqual(DrainPhase.drained, ws.getDrainPhase());
    try std.testing.expect(!ws.running.load(.acquire));
}

test "beginRunIfActive with in-flight runs during drain still rejects new work" {
    var ws = WorkerState.init();
    // Start a run, then begin draining
    ws.beginRun();
    try std.testing.expectEqual(@as(u32, 1), ws.currentInFlightRuns());
    try std.testing.expect(ws.startDrain());

    // New work must be rejected even though there are in-flight runs
    try std.testing.expectError(worker_runtime.WorkerError.ShutdownRequested, beginRunIfActive(&ws));
    // In-flight count unchanged — rejection did not increment
    try std.testing.expectEqual(@as(u32, 1), ws.currentInFlightRuns());

    // Clean up the in-flight run
    ws.endRun();
    try std.testing.expectEqual(@as(u32, 0), ws.currentInFlightRuns());
}

// ── T5 Concurrency — startDrain race ───────────────────────────────────────

const DrainRaceCtx = struct {
    ws: *WorkerState,
    success_count: *std.atomic.Value(u32),
};

fn drainRaceThread(ctx: DrainRaceCtx) void {
    if (ctx.ws.startDrain()) {
        _ = ctx.success_count.fetchAdd(1, .acq_rel);
    }
}

test "concurrency: multiple threads calling startDrain — exactly one succeeds" {
    var ws = WorkerState.init();
    var success_count = std.atomic.Value(u32).init(0);
    const thread_count: usize = 8;

    const threads = try std.testing.allocator.alloc(std.Thread, thread_count);
    defer std.testing.allocator.free(threads);

    for (threads) |*t| {
        t.* = try std.Thread.spawn(.{}, drainRaceThread, .{DrainRaceCtx{
            .ws = &ws,
            .success_count = &success_count,
        }});
    }
    for (threads) |*t| t.join();

    try std.testing.expectEqual(@as(u32, 1), success_count.load(.acquire));
    try std.testing.expectEqual(DrainPhase.draining, ws.getDrainPhase());
}

// ── T5 Concurrency — beginRunIfActive races with startDrain ────────────────

const ActiveDrainRaceCtx = struct {
    ws: *WorkerState,
    accept_count: *std.atomic.Value(u32),
    reject_count: *std.atomic.Value(u32),
};

fn activeRaceThread(ctx: ActiveDrainRaceCtx) void {
    var i: u32 = 0;
    while (i < 200) : (i += 1) {
        if (beginRunIfActive(ctx.ws)) |_| {
            _ = ctx.accept_count.fetchAdd(1, .acq_rel);
            ctx.ws.endRun();
        } else |_| {
            _ = ctx.reject_count.fetchAdd(1, .acq_rel);
        }
    }
}

test "concurrency: beginRunIfActive vs startDrain — no data corruption" {
    var ws = WorkerState.init();
    var accept_count = std.atomic.Value(u32).init(0);
    var reject_count = std.atomic.Value(u32).init(0);
    const worker_count: usize = 4;

    const workers = try std.testing.allocator.alloc(std.Thread, worker_count);
    defer std.testing.allocator.free(workers);

    for (workers) |*t| {
        t.* = try std.Thread.spawn(.{}, activeRaceThread, .{ActiveDrainRaceCtx{
            .ws = &ws,
            .accept_count = &accept_count,
            .reject_count = &reject_count,
        }});
    }

    // Let workers run a bit, then trigger drain
    std.Thread.sleep(1 * std.time.ns_per_ms);
    _ = ws.startDrain();

    for (workers) |*t| t.join();

    // Counter must be consistent — every accepted run was ended
    try std.testing.expectEqual(@as(u32, 0), ws.currentInFlightRuns());
    // At least some work was accepted and some rejected
    const total = accept_count.load(.acquire) + reject_count.load(.acquire);
    try std.testing.expectEqual(@as(u32, worker_count * 200), total);
}

// ── T5 Concurrency — endRun during drain phase ────────────────────────────

test "concurrency: endRun during drain decrements correctly" {
    var ws = WorkerState.init();
    // Start several in-flight runs
    const run_count: u32 = 5;
    var i: u32 = 0;
    while (i < run_count) : (i += 1) {
        ws.beginRun();
    }
    try std.testing.expectEqual(run_count, ws.currentInFlightRuns());

    // Begin draining
    try std.testing.expect(ws.startDrain());

    // End all runs from separate threads
    const threads = try std.testing.allocator.alloc(std.Thread, run_count);
    defer std.testing.allocator.free(threads);

    for (threads) |*t| {
        t.* = try std.Thread.spawn(.{}, struct {
            fn run(state: *WorkerState) void {
                state.endRun();
            }
        }.run, .{&ws});
    }
    for (threads) |*t| t.join();

    try std.testing.expectEqual(@as(u32, 0), ws.currentInFlightRuns());
}

// ── T7 Regression — DrainPhase size and WorkerState fields ─────────────────

test "regression: DrainPhase enum size is 1 byte" {
    try std.testing.expectEqual(@as(usize, 1), @sizeOf(DrainPhase));
}

test "regression: WorkerState struct has all expected fields" {
    // Compile-time check: accessing these fields must resolve
    const ws = WorkerState.init();
    _ = ws.running;
    _ = ws.drain_phase;
    _ = ws.in_flight_runs;

    // Verify field types at comptime
    const info = @typeInfo(WorkerState).@"struct";
    comptime {
        var found_running = false;
        var found_drain_phase = false;
        var found_in_flight_runs = false;
        for (info.fields) |f| {
            if (std.mem.eql(u8, f.name, "running")) found_running = true;
            if (std.mem.eql(u8, f.name, "drain_phase")) found_drain_phase = true;
            if (std.mem.eql(u8, f.name, "in_flight_runs")) found_in_flight_runs = true;
        }
        if (!found_running) @compileError("WorkerState missing 'running' field");
        if (!found_drain_phase) @compileError("WorkerState missing 'drain_phase' field");
        if (!found_in_flight_runs) @compileError("WorkerState missing 'in_flight_runs' field");
    }
}
