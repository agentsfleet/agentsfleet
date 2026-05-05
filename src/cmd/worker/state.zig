//! Worker drain primitive.
//!
//! Three-phase drain (running → draining → drained) so the process can hold
//! SIGTERM long enough for current events to finish, then force-exit on
//! timeout. Lock-free: drain_phase is std.atomic. Worker loops poll
//! isAcceptingWork() at the top of each iteration; the signal handler thread
//! flips the phase via startDrain(); completeDrain() seals it.

const std = @import("std");

const DrainPhase = enum(u8) {
    running = 0,
    draining = 1,
    drained = 2,
};

pub const WorkerState = struct {
    drain_phase: std.atomic.Value(u8),

    pub fn init() WorkerState {
        return .{
            .drain_phase = std.atomic.Value(u8).init(@intFromEnum(DrainPhase.running)),
        };
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

// ── Tests ────────────────────────────────────────────────────────────────────

test "init starts in running phase" {
    const ws = WorkerState.init();
    try std.testing.expect(ws.isAcceptingWork());
    try std.testing.expectEqual(DrainPhase.running, ws.getDrainPhase());
}

test "drain phase transitions running to draining to drained" {
    var ws = WorkerState.init();
    try std.testing.expectEqual(DrainPhase.running, ws.getDrainPhase());
    try std.testing.expect(ws.isAcceptingWork());

    try std.testing.expect(ws.startDrain());
    try std.testing.expectEqual(DrainPhase.draining, ws.getDrainPhase());
    try std.testing.expect(!ws.isAcceptingWork()); // new work rejected during drain

    ws.completeDrain();
    try std.testing.expectEqual(DrainPhase.drained, ws.getDrainPhase());
    try std.testing.expect(!ws.isAcceptingWork());
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
