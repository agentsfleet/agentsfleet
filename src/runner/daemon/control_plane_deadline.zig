//! The armed-attempt half of the runner control-plane client (RULE FLL — the
//! client file sits at the line cap, like the `renew` and `mint` splits).
//!
//! One `Attempt` bounds one `fetch`. It is a STACK LOCAL of the calling verb:
//! the scheduler is armed against the attempt's connection generation, so a
//! fire can only ever reach the exact call that armed it. A pooled descriptor
//! recycled into a later call is therefore safe, which a raw descriptor number
//! could never prove.
//!
//! Drive it as `begin` → `arm` → fetch → `release`. `release` retires the
//! generation and then finishes the guard; `finish` is the quiescence barrier,
//! so once it returns no interrupt callback is running or can start and the
//! socket is safe to hand back to the connection pool.

const std = @import("std");
const logging = @import("log");
const call_deadline = @import("call_deadline");
const client_errors = @import("../engine/client_errors.zig");

const log = logging.scoped(.fleet_runner);
const EV_ARM_REFUSED = "cp_deadline_arm_refused";

pub const Scheduler = call_deadline.ProcessScheduler;

/// Why a verb could not be armed — every branch is a refusal, never a fallback
/// to an unbounded run. The caller maps each to its own `ClientError`.
pub const ArmOutcome = enum {
    armed,
    /// No pooled socket to arm: the connect itself failed. A transport loss the
    /// control loop already retries.
    pin_failed,
    /// The process scheduler is stopping or out of identifiers.
    scheduler_unavailable,
};

pub const Attempt = struct {
    owner: call_deadline.SocketOwner = .{},
    generation: u64 = 0,
    guard: ?Scheduler.Guard = null,

    /// Open a fresh generation. Any registration naming the previous one is now
    /// stale. Called BEFORE the pin, so the control block outlives every stage.
    pub fn begin(self: *Attempt) void {
        self.generation = self.owner.beginAttempt();
    }

    /// Publish the pinned socket and arm the deadline against THIS generation.
    /// Fails closed: `handle == null` (the pin failed) is a refusal, not a
    /// licence to fetch unarmed. Logs the refusal class here, beside the
    /// mechanism, so both are greppable from one event name.
    pub fn armPinned(self: *Attempt, sched: *Scheduler, handle: ?std.posix.fd_t, deadline_ms: u31) ArmOutcome {
        const socket = handle orelse {
            log.warn(EV_ARM_REFUSED, .{ .error_code = client_errors.ERR_EXEC_TRANSPORT_LOSS, .reason = "pin_failed" });
            return .pin_failed;
        };
        _ = self.owner.attachSocket(self.generation, socket);
        self.guard = sched.arm(self.owner.target(self.generation), deadline_ms) catch {
            log.err(EV_ARM_REFUSED, .{ .error_code = client_errors.ERR_EXEC_TRANSPORT_LOSS, .reason = "scheduler_unavailable" });
            return .scheduler_unavailable;
        };
        return .armed;
    }

    /// True when the deadline fired against this attempt — what distinguishes a
    /// cancellation from an ordinary transport failure.
    pub fn wasInterrupted(self: *Attempt) bool {
        return self.owner.wasInterrupted();
    }

    /// Retire the generation, then quiesce the guard. Idempotent: a second call
    /// finds no guard and does nothing.
    pub fn release(self: *Attempt) void {
        self.owner.endAttempt();
        if (self.guard) |guard| {
            _ = guard.finish();
            self.guard = null;
        }
    }
};

test {
    _ = @import("control_plane_deadline_test.zig");
}
