//! Test-only scheduler fixture for the runner graph.
//!
//! Every control-plane client now borrows the root's process scheduler, so a
//! test that builds a client must own one exactly the way `main.zig` does:
//! backend and scheduler in one value with a stable address, started before
//! any verb runs, joined at deinit. Shared here so no test hand-rolls the
//! ownership and accidentally proves something the production root does not do.

const std = @import("std");
const call_deadline = @import("call_deadline");

pub const TestScheduler = struct {
    backend: call_deadline.MonotonicBackend = .{},
    scheduler: ?call_deadline.ProcessScheduler = null,

    /// Start the worker and return the borrowed handle, mirroring
    /// `runner_deadline.Owned.start` minus the process-exit failure policy.
    pub fn start(self: *TestScheduler, alloc: std.mem.Allocator) !*call_deadline.ProcessScheduler {
        self.scheduler = call_deadline.ProcessScheduler.init(alloc, &self.backend);
        try self.scheduler.?.start();
        return &self.scheduler.?;
    }

    pub fn deinit(self: *TestScheduler) void {
        if (self.scheduler) |*scheduler| scheduler.deinit();
        self.* = undefined;
    }
};
