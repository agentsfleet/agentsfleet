//! Process deadline-scheduler boot for `serve.run`.
//!
//! The backend and the scheduler live together because the scheduler borrows a
//! pointer to the backend: they must be initialized in place, in one
//! caller-owned value whose address is stable for the process.

const std = @import("std");
const call_deadline = @import("call_deadline");
const logging = @import("log");
const error_codes = @import("../errors/error_registry.zig");

const log = logging.scoped(.agentsfleetd);

pub const Owned = struct {
    backend: call_deadline.MonotonicBackend = .{},
    /// Null until `start`; optional rather than `undefined` so a teardown on the
    /// failure path has nothing to misread.
    scheduler: ?call_deadline.ProcessScheduler = null,

    /// Start the one process scheduler and return the borrowed handle every
    /// network owner arms against. A scheduler that cannot start means no
    /// outbound call can be bounded, and running unbounded is exactly the
    /// silent hang deadlines exist to prevent — so this exits rather than
    /// degrading into it.
    pub fn start(self: *Owned, alloc: std.mem.Allocator) *call_deadline.ProcessScheduler {
        self.scheduler = call_deadline.ProcessScheduler.init(alloc, &self.backend);
        const scheduler = &self.scheduler.?;
        scheduler.start() catch |err| {
            log.err("startup.deadline_scheduler_failed", .{
                .error_code = error_codes.ERR_INTERNAL_OPERATION_FAILED,
                .err = @errorName(err),
            });
            std.process.exit(1);
        };
        log.info("startup.deadline_scheduler_ok", .{});
        return scheduler;
    }

    pub fn deinit(self: *Owned) void {
        if (self.scheduler) |*scheduler| scheduler.deinit();
        // Poison: the borrowed pointer `start` handed out is dead now, so a
        // use-after-deinit traps instead of reading a freed registration map.
        self.* = undefined;
    }
};
