//! Worker drain primitive.
//!
//! Tracks in-flight events and a three-phase drain (running → draining → drained)
//! so the process can hold SIGTERM long enough for current events to finish,
//! then force-exit on timeout. Ported from pre-M10 src/pipeline/worker_state.zig
//! with event-oriented naming (runs → events) to fit the zombie model.
//!
//! Lock-free: all state is std.atomic. Safe under concurrent beginEvent/endEvent
//! from N zombie threads + startDrain from the signal handler thread.

const std = @import("std");

pub const WorkerError = error{ShutdownRequested};

pub const DrainPhase = enum(u8) {
    running = 0,
    draining = 1,
    drained = 2,
};

pub const WorkerState = struct {
    running: std.atomic.Value(bool),
    drain_phase: std.atomic.Value(u8),
    in_flight_events: std.atomic.Value(u32),

    pub fn init() WorkerState {
        return .{
            .running = std.atomic.Value(bool).init(true),
            .drain_phase = std.atomic.Value(u8).init(@intFromEnum(DrainPhase.running)),
            .in_flight_events = std.atomic.Value(u32).init(0),
        };
    }

    pub fn beginEvent(self: *WorkerState) void {
        _ = self.in_flight_events.fetchAdd(1, .acq_rel);
    }

    pub fn endEvent(self: *WorkerState) void {
        const prev = self.in_flight_events.fetchSub(1, .acq_rel);
        // `unreachable` is explicit about intent and still fires in ReleaseSafe.
        // Without this, unbalanced endEvent would wrap u32 and stall drain forever.
        if (prev == 0) unreachable;
    }

    pub fn currentInFlightEvents(self: *const WorkerState) u32 {
        return self.in_flight_events.load(.acquire);
    }

    pub fn getDrainPhase(self: *const WorkerState) DrainPhase {
        return @enumFromInt(self.drain_phase.load(.acquire));
    }

    pub fn isAcceptingWork(self: *const WorkerState) bool {
        return self.getDrainPhase() == .running;
    }

    /// Transition running → draining. Returns true on the first call, false
    /// on subsequent calls (idempotent for repeated SIGTERM).
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

/// Gate at the top of each event claim. Returns WorkerError.ShutdownRequested
/// if the process is draining or drained.
///
/// Increments the in-flight counter FIRST, then checks drain phase, rolling
/// back on shutdown. This order closes a TOCTOU race where a concurrent
/// startDrain() could observe in_flight == 0 between the phase check and the
/// increment, letting completeDrain() race ahead of real work. With the flip,
/// any drain wait loop that sees in_flight == 0 is guaranteed no new claim is
/// in progress.
pub fn beginEventIfActive(worker_state: *WorkerState) WorkerError!void {
    worker_state.beginEvent();
    if (!worker_state.isAcceptingWork()) {
        worker_state.endEvent();
        return WorkerError.ShutdownRequested;
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

test "in-flight counter tracks begin/end safely" {
    var ws = WorkerState.init();
    try std.testing.expectEqual(@as(u32, 0), ws.currentInFlightEvents());
    ws.beginEvent();
    ws.beginEvent();
    try std.testing.expectEqual(@as(u32, 2), ws.currentInFlightEvents());
    ws.endEvent();
    try std.testing.expectEqual(@as(u32, 1), ws.currentInFlightEvents());
    ws.endEvent();
    try std.testing.expectEqual(@as(u32, 0), ws.currentInFlightEvents());
}

test "beginEventIfActive rejects drained worker without incrementing" {
    var ws = WorkerState.init();
    ws.completeDrain();
    try std.testing.expectError(WorkerError.ShutdownRequested, beginEventIfActive(&ws));
    try std.testing.expectEqual(@as(u32, 0), ws.currentInFlightEvents());
}

test "beginEventIfActive rejects draining worker without incrementing" {
    var ws = WorkerState.init();
    try std.testing.expect(ws.startDrain());
    try std.testing.expectError(WorkerError.ShutdownRequested, beginEventIfActive(&ws));
    try std.testing.expectEqual(@as(u32, 0), ws.currentInFlightEvents());
}

test "beginEventIfActive increments in-flight when running" {
    var ws = WorkerState.init();
    try beginEventIfActive(&ws);
    try std.testing.expectEqual(@as(u32, 1), ws.currentInFlightEvents());
    ws.endEvent();
    try std.testing.expectEqual(@as(u32, 0), ws.currentInFlightEvents());
}

test "init starts running with zero in-flight" {
    const ws = WorkerState.init();
    try std.testing.expect(ws.running.load(.acquire));
    try std.testing.expectEqual(DrainPhase.running, ws.getDrainPhase());
    try std.testing.expectEqual(@as(u32, 0), ws.currentInFlightEvents());
}

test "drain phase transitions running to draining to drained" {
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

test "startDrain is idempotent on second call" {
    var ws = WorkerState.init();
    try std.testing.expect(ws.startDrain());
    try std.testing.expect(!ws.startDrain());
    try std.testing.expectEqual(DrainPhase.draining, ws.getDrainPhase());
}

test "isAcceptingWork reflects drain phase" {
    var ws = WorkerState.init();
    try std.testing.expect(ws.isAcceptingWork());
    _ = ws.startDrain();
    try std.testing.expect(!ws.isAcceptingWork());
    ws.completeDrain();
    try std.testing.expect(!ws.isAcceptingWork());
}

test "DrainPhase enum backing values are 0/1/2" {
    try std.testing.expectEqual(@as(u8, 0), @intFromEnum(DrainPhase.running));
    try std.testing.expectEqual(@as(u8, 1), @intFromEnum(DrainPhase.draining));
    try std.testing.expectEqual(@as(u8, 2), @intFromEnum(DrainPhase.drained));
}

test "DrainPhase enum has exactly 3 values" {
    const fields = @typeInfo(DrainPhase).@"enum".fields;
    try std.testing.expectEqual(@as(usize, 3), fields.len);
}

test "startDrain on already-drained worker returns false" {
    var ws = WorkerState.init();
    try std.testing.expect(ws.startDrain());
    ws.completeDrain();
    try std.testing.expect(!ws.startDrain());
    try std.testing.expectEqual(DrainPhase.drained, ws.getDrainPhase());
}

const CounterRaceCtx = struct {
    ws: *WorkerState,
    iterations: u32,
};

fn runBalancedCounterLoop(ctx: CounterRaceCtx) void {
    var i: u32 = 0;
    while (i < ctx.iterations) : (i += 1) {
        ctx.ws.beginEvent();
        ctx.ws.endEvent();
    }
}

test "integration: in-flight counter is balanced across threads" {
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

    try std.testing.expectEqual(@as(u32, 0), ws.currentInFlightEvents());
}
