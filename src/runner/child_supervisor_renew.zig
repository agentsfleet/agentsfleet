//! The lease-renewal surface the supervisor's read loop drives.
//!
//! Split from `child_supervisor_read.zig` for the file-length budget (RULE FLL)
//! when the fetch's renewal pump landed. Deliberately depends on nothing in the
//! read loop — only the usage snapshot and the failure vocabulary — so the loop
//! imports these types rather than the other way round.

const pipe_proto = @import("pipe_proto.zig");
const types = @import("engine/types.zig");

/// What the read loop should do after a renewal tick or a progress frame.
/// `extend` carries the new absolute kill deadline (epoch ms).
/// `terminate` carries the class the run is reported under, so a fleet-budget
/// stop reaches the durable `failure_label` instead of collapsing into the
/// generic `renewal_terminate` every renewal stop used to share.
pub const RenewDecision = union(enum) { keep, extend: i64, terminate: types.FailureClass };

/// Hook the daemon installs so the supervisor can drive lease renewal during a
/// long execution without the supervisor knowing any HTTP. `onTick` fires in
/// the idle gap between frames (renewal-tick cadence) and after each progress
/// frame, carrying the current epoch ms and the latest cumulative usage
/// snapshot (zeros until the child's first usage frame); the daemon renews
/// inside the window and returns a decision. A live child that emits no
/// frames still ticks, so a long run renews and is never falsely reclaimed.
pub const RenewHook = struct {
    ctx: *anyopaque,
    onTick: *const fn (ctx: *anyopaque, now_ms: i64, usage: pipe_proto.UsageSnapshot) RenewDecision,
    /// How often (ms) the read loop wakes between frames to consider renewal.
    /// Production sets `constants.RENEWAL_TICK_MS`; tests inject a small value.
    tick_ms: i64,
};

/// Lets a long-running fetch keep the lease alive from inside its own poll loop.
///
/// The fetch is serviced ON the read loop, and that loop is the ONLY driver of
/// lease renewal — `applyTick` fires between frames. A minutes-scale fetch
/// therefore starved renewal completely: the lease lapsed, the control plane
/// reclaimed it, and a second runner began the same event while this one was
/// still fetching. The renew hook and the live usage snapshot are locals of that
/// loop, so the tick crosses from there into the fetch rather than the reverse.
pub const RenewTick = struct {
    ctx: *anyopaque,
    /// Returns false when the lease can no longer be held; the fetch stops.
    onTick: *const fn (ctx: *anyopaque, now_ms: i64) bool,
};

/// Binds the read loop's renew hook and live usage into a `RenewTick`. Stack-
/// owned by the frame that services one fetch, so it cannot outlive either
/// borrow.
pub const RenewPump = struct {
    hook: ?RenewHook,
    usage: *const pipe_proto.UsageSnapshot,
    /// The read loop's own deadline. An `.extend` that fired during the fetch
    /// must land here too, exactly as `applyTick` lands it — otherwise the loop
    /// resumes after a multi-minute fetch holding the deadline it started with
    /// and terminates a run whose lease was in fact renewed.
    deadline: *i64,

    fn onTick(ctx: *anyopaque, now_ms: i64) bool {
        const self: *RenewPump = @ptrCast(@alignCast(ctx));
        // No renew hook wired means nothing is tracking this lease's expiry, so
        // there is no renewal to lose — the fetch's own deadline still bounds it.
        const h = self.hook orelse return true;
        switch (h.onTick(h.ctx, now_ms, self.usage.*)) {
            .keep => {},
            .extend => |new_deadline| self.deadline.* = new_deadline,
            .terminate => return false,
        }
        return true;
    }

    pub fn tick(self: *RenewPump) RenewTick {
        return .{ .ctx = self, .onTick = onTick };
    }
};
