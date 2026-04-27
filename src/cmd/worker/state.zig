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

const WorkerError = error{ShutdownRequested};

const DrainPhase = enum(u8) {
    running = 0,
    draining = 1,
    drained = 2,
};

pub const WorkerState = struct {
    drain_phase: std.atomic.Value(u8),
    in_flight_events: std.atomic.Value(u32),

    pub fn init() WorkerState {
        return .{
            .drain_phase = std.atomic.Value(u8).init(@intFromEnum(DrainPhase.running)),
            .in_flight_events = std.atomic.Value(u32).init(0),
        };
    }

    fn beginEvent(self: *WorkerState) void {
        _ = self.in_flight_events.fetchAdd(1, .acq_rel);
    }

    fn endEvent(self: *WorkerState) void {
        const prev = self.in_flight_events.fetchSub(1, .acq_rel);
        // `unreachable` is explicit about intent and still fires in ReleaseSafe.
        // Without this, unbalanced endEvent would wrap u32 and stall drain forever.
        if (prev == 0) unreachable;
    }

    fn currentInFlightEvents(self: *const WorkerState) u32 {
        return self.in_flight_events.load(.acquire);
    }

    pub fn getDrainPhase(self: *const WorkerState) DrainPhase {
        return @enumFromInt(self.drain_phase.load(.acquire));
    }

    /// True while new claims are allowed. False as soon as startDrain() fires.
    /// Worker loops should use this as their top-of-iteration exit condition —
    /// it gives prompt response to SIGTERM without stranding in-flight events.
    pub fn isAcceptingWork(self: *const WorkerState) bool {
        return self.getDrainPhase() == .running;
    }

    /// True once completeDrain() has run and no in-flight events remain.
    /// This is the "fully stopped" flag — use it when you need to know the
    /// process is safe to exit, as distinct from "stopped accepting new work".
    fn isStopped(self: *const WorkerState) bool {
        return self.getDrainPhase() == .drained;
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

    /// Transition draining → drained. Asserts the phase was draining — reaching
    /// drained without first calling startDrain() is a programmer error (the
    /// `draining` phase is the signal that holds new claims at the gate while
    /// in-flight events finish). CAS makes the invariant self-enforcing.
    pub fn completeDrain(self: *WorkerState) void {
        const prev = self.drain_phase.cmpxchgStrong(
            @intFromEnum(DrainPhase.draining),
            @intFromEnum(DrainPhase.drained),
            .acq_rel,
            .acquire,
        );
        if (prev != null) unreachable; // must have been in draining phase
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
    try std.testing.expect(ws.startDrain());
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
    try std.testing.expect(ws.isAcceptingWork());
    try std.testing.expect(!ws.isStopped());
    try std.testing.expectEqual(DrainPhase.running, ws.getDrainPhase());
    try std.testing.expectEqual(@as(u32, 0), ws.currentInFlightEvents());
}

test "drain phase transitions running to draining to drained" {
    var ws = WorkerState.init();
    try std.testing.expectEqual(DrainPhase.running, ws.getDrainPhase());
    try std.testing.expect(ws.isAcceptingWork());
    try std.testing.expect(!ws.isStopped());

    try std.testing.expect(ws.startDrain());
    try std.testing.expectEqual(DrainPhase.draining, ws.getDrainPhase());
    try std.testing.expect(!ws.isAcceptingWork()); // new work rejected during drain
    try std.testing.expect(!ws.isStopped()); // but not fully stopped yet

    ws.completeDrain();
    try std.testing.expectEqual(DrainPhase.drained, ws.getDrainPhase());
    try std.testing.expect(!ws.isAcceptingWork());
    try std.testing.expect(ws.isStopped());
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

// The drain-race scenario that motivated the increment-first ordering in
// beginEventIfActive. N worker threads hammer the gate while a drain thread
// runs the full cycle: startDrain → wait for in_flight == 0 → completeDrain.
// Invariant: every claim that got past the gate also reached endEvent.
// If the TOCTOU race were reintroduced, the drain thread could observe
// in_flight == 0 while a worker is between increment and endEvent — begins
// and ends would diverge by at least one, or currentInFlightEvents would
// be non-zero when the drain thread finishes.

const DrainRaceCtx = struct {
    ws: *WorkerState,
    begins: std.atomic.Value(u64),
    ends: std.atomic.Value(u64),
    iterations: u32,
};

fn drainRaceWorker(ctx: *DrainRaceCtx) void {
    var i: u32 = 0;
    while (i < ctx.iterations) : (i += 1) {
        beginEventIfActive(ctx.ws) catch return;
        _ = ctx.begins.fetchAdd(1, .monotonic);
        // Minimal "work" — memory fence is enough to widen the window between
        // increment and endEvent so the race, if present, would manifest.
        std.atomic.spinLoopHint();
        ctx.ws.endEvent();
        _ = ctx.ends.fetchAdd(1, .monotonic);
    }
}

fn drainRaceDrain(ws: *WorkerState) void {
    // Give workers a moment to start hammering the gate.
    std.Thread.sleep(1 * std.time.ns_per_ms);
    _ = ws.startDrain();
    while (ws.currentInFlightEvents() > 0) {
        std.atomic.spinLoopHint();
    }
    ws.completeDrain();
}

test "integration: all claims balance after drain completes" {
    // What this actually proves: every claim that got past the gate also
    // reached endEvent, and no claim is "in flight" once all threads have
    // joined. The narrower guarantee "drain never completes while a claim
    // is in flight" does NOT hold — with increment-first ordering, a worker
    // can transiently increment between the drain thread's wait-loop exit
    // and its completeDrain call; that claim rolls back via endEvent when
    // it sees phase=draining/drained. The post-state invariants hold either
    // way, which is what callers actually care about.
    var ws = WorkerState.init();
    var ctx = DrainRaceCtx{
        .ws = &ws,
        .begins = std.atomic.Value(u64).init(0),
        .ends = std.atomic.Value(u64).init(0),
        .iterations = 2000,
    };

    const worker_count: usize = 4;
    const workers = try std.testing.allocator.alloc(std.Thread, worker_count);
    defer std.testing.allocator.free(workers);

    for (workers) |*t| {
        t.* = try std.Thread.spawn(.{}, drainRaceWorker, .{&ctx});
    }
    const drain_thread = try std.Thread.spawn(.{}, drainRaceDrain, .{&ws});

    drain_thread.join();
    for (workers) |*t| t.join();

    // Post-conditions after all threads have joined:
    //  - No event in flight.
    //  - Every claim that got past the gate also reached endEvent.
    //  - Phase is drained (the CAS succeeded).
    //  - isStopped() reflects that.
    try std.testing.expectEqual(@as(u32, 0), ws.currentInFlightEvents());
    try std.testing.expectEqual(ctx.begins.load(.monotonic), ctx.ends.load(.monotonic));
    try std.testing.expectEqual(DrainPhase.drained, ws.getDrainPhase());
    try std.testing.expect(ws.isStopped());
    try std.testing.expect(!ws.isAcceptingWork());
}

test "completeDrain requires draining phase (running → drained is a programmer error)" {
    // Positive path verified in "drain phase transitions running to draining to drained".
    // Direct running → drained is forbidden by the CAS in completeDrain;
    // that behavior is enforced via `unreachable`, not a recoverable error,
    // so we can't test the failure case in a normal unit test. The guarantee
    // is documented; the CAS is load-bearing.
    var ws = WorkerState.init();
    try std.testing.expect(ws.startDrain());
    ws.completeDrain();
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
