//! Drives lease renewal from the child-supervisor's read-loop ticks.
//!
//! Holds the control-plane client and the lease's current kill deadline; once
//! inside the renewal window it calls `/renew` and maps the result to a
//! supervisor `RenewDecision`. The supervisor stays HTTP-agnostic — it only
//! knows `onTick(now) → keep|extend|terminate`; all the renewal/window/HTTP
//! logic lives here.
//!
//! Fail-safe: a transient/5xx renewal failure returns `keep` so the next tick
//! retries — if renewal never succeeds the lease simply expires and is reclaimed
//! (never double-run). A definitive 4xx (lost / capped / no-credits) returns
//! `terminate` and the run ends.
//!
//! Generic over the control-plane `Client` so production binds the real
//! `LoopbackClient` and tests inject a scripted fake — deterministic, no HTTP
//! and no wall clock (ZIG_RULES §Deterministic testing: inject the client, the
//! decision `now_ms` is passed in). `Client` must expose
//! `renew(alloc, runner_token, lease_id) !RenewResult`.

const std = @import("std");
const logging = @import("log");
const contract = @import("contract");
const constants = @import("common");
const child_supervisor = @import("../child_supervisor.zig");

const LeasePayload = contract.protocol.LeasePayload;
const log = logging.scoped(.zombie_runner);

/// Build the renewal-driver type bound to a control-plane client type `Client`.
pub fn RenewDriver(comptime Client: type) type {
    return struct {
        const Self = @This();

        alloc: std.mem.Allocator,
        cp: Client,
        runner_token: []const u8,
        lease_id: []const u8,
        deadline_ms: i64,

        /// Build a driver seeded with the lease's initial kill deadline.
        pub fn init(alloc: std.mem.Allocator, cp: Client, runner_token: []const u8, lease: LeasePayload) Self {
            return .{
                .alloc = alloc,
                .cp = cp,
                .runner_token = runner_token,
                .lease_id = lease.lease_id,
                .deadline_ms = lease.lease_expires_at,
            };
        }

        /// Build the supervisor hook bound to this driver (production tick cadence).
        pub fn hook(self: *Self) child_supervisor.RenewHook {
            return .{ .ctx = self, .onTick = onTick, .tick_ms = constants.RENEWAL_TICK_MS };
        }

        /// Supervisor calls this on each tick / progress frame. Renew only inside
        /// the window; map the renewal result to a decision, advancing the kill
        /// deadline on success.
        fn onTick(ctx: *anyopaque, now_ms: i64) child_supervisor.RenewDecision {
            const self: *Self = @ptrCast(@alignCast(ctx));
            // Equivalent to `deadline_ms - now_ms > WINDOW` but overflow-safe: a
            // garbage/extreme deadline from the wire must not panic the tick loop.
            if (self.deadline_ms > now_ms +| constants.RENEWAL_WINDOW_MS) return .keep;
            const res = self.cp.renew(self.alloc, self.runner_token, self.lease_id) catch |err| {
                log.warn("renew_failed_retry", .{ .lease_id = self.lease_id, .err = @errorName(err) });
                return .keep;
            };
            switch (res) {
                .renewed => |new_deadline| {
                    self.deadline_ms = new_deadline;
                    log.debug("lease_renewed", .{ .lease_id = self.lease_id, .lease_expires_at = new_deadline });
                    return .{ .extend = new_deadline };
                },
                .terminal => |status| {
                    log.info("lease_renew_terminal", .{ .lease_id = self.lease_id, .status = status });
                    return .terminate;
                },
            }
        }
    };
}
