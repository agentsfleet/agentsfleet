//! The production deadline clock: the boot monotonic clock, plus the futex word
//! the scheduler worker parks on between deadlines.
//!
//! Split out of `scheduler.zig` so that file holds one concern — the generic
//! state machine — and this one holds the only part that touches the host clock
//! and the OS wait primitive. `Scheduler(Target, Backend)` names its backend as
//! a type parameter, so a test substitutes a scripted clock without the state
//! machine knowing there is one.

const MonotonicBackend = @This();

epoch: std.atomic.Value(u32) = std.atomic.Value(u32).init(0),

pub fn nowNs(_: *MonotonicBackend) i96 {
    return std.Io.Clock.boot.now(common.globalIo()).toNanoseconds();
}

pub fn snapshotWake(self: *MonotonicBackend) u32 {
    // safe because: the epoch detects wakeups only; scheduler state is mutex-protected.
    return self.epoch.load(.monotonic);
}

pub fn wait(self: *MonotonicBackend, seen: u32, deadline_ns: ?i96) void {
    const io = common.globalIo();
    const timeout: std.Io.Timeout = if (deadline_ns) |deadline| blk: {
        if (deadline <= self.nowNs()) return;
        break :blk .{ .deadline = std.Io.Timestamp.fromNanoseconds(deadline).withClock(.boot) };
    } else .none;
    io.futexWaitTimeout(u32, &self.epoch.raw, seen, timeout) catch |err| switch (err) {
        error.Canceled => {},
    };
}

pub fn wake(self: *MonotonicBackend) void {
    _ = self.epoch.fetchAdd(1, .release); // safe because: waiters only need to observe an epoch change.
    common.globalIo().futexWake(u32, &self.epoch.raw, 1);
}

const std = @import("std");
const common = @import("common");
