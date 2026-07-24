//! Process deadline-scheduler boot for `agentsfleet-runner`.
//!
//! The runner-side twin of `agentsfleetd`'s `cmd/serve_deadline.zig`: the
//! backend and the scheduler live together because the scheduler borrows a
//! pointer to the backend, so both must be initialized in place inside one
//! caller-owned value whose address is stable for the process. Each root keeps
//! its own copy because the failure policy it encodes — which log scope, which
//! error registry, exit versus degrade — is root policy, not library behaviour.

const std = @import("std");
const call_deadline = @import("call_deadline");
const logging = @import("log");
const client_errors = @import("../engine/client_errors.zig");

const log = logging.scoped(.fleet_runner);

pub const Owned = struct {
    backend: call_deadline.MonotonicBackend = .{},
    /// Null until `start`; optional rather than `undefined` so a teardown on the
    /// failure path has nothing to misread.
    scheduler: ?call_deadline.ProcessScheduler = null,

    /// Start the one process scheduler and return the borrowed handle every
    /// control-plane client arms against. A scheduler that cannot start means
    /// no outbound call can be bounded, and running unbounded is exactly the
    /// silent hang deadlines exist to prevent — so this exits rather than
    /// degrading into it.
    pub fn start(self: *Owned, alloc: std.mem.Allocator) *call_deadline.ProcessScheduler {
        self.scheduler = call_deadline.ProcessScheduler.init(alloc, &self.backend);
        const scheduler = &self.scheduler.?;
        scheduler.start() catch |err| {
            log.err("startup.deadline_scheduler_failed", .{
                .error_code = client_errors.ERR_EXEC_RUNNER_FLEET_INIT,
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
