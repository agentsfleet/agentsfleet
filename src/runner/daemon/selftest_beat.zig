//! selftest_beat.zig — the verdict a runner carries between heartbeats.
//!
//! Closes the loop the operator control opens. The dashboard stamps an ask, the
//! reply carries `selftest_requested`, this runs the probe, and the NEXT beat
//! carries the verdict up. One interval of latency, no second endpoint, and no
//! synchronous wait on a host that may be offline — which is exactly the host an
//! operator most wants to test.
//!
//! It also runs once at daemon start (Dimension 2.6), so a freshly deployed
//! runner whose sandbox cannot resolve a name says so on its first beat instead
//! of waiting for someone to suspect it and click.
//!
//! Ownership is the subtle part. `grade` names each operator-bind check after
//! the bind's PATH, and that path is borrowed from the policy snapshot — so the
//! snapshot must outlive the verdict built from it. `Pending` holds both and
//! frees them together rather than deep-copying the check names.

const std = @import("std");
const contract = @import("contract");
const logging = @import("log");
const protocol = contract.protocol;

const AppliedPolicy = @import("AppliedPolicy.zig");
const Config = @import("config.zig");
const selftest = @import("../selftest.zig");
const selftest_exec = @import("../selftest_exec.zig");

const log = logging.scoped(.runner_selftest);

/// Scratch workspace for the probe, under the runner's storage root. The probe
/// writes nothing, but the sandbox argv binds a workspace read-write and chdirs
/// into it, so it needs a real directory — its own, not a lease's, so a probe
/// can never touch tenant scratch.
pub const WORKSPACE_DIR_NAME = ".selftest";

/// Does this beat run a probe? Pure so the control loop's decision is testable
/// without a heartbeat, a socket, or a sandbox.
///
/// An assignment is required for both arms, not just the startup one: a probe
/// under the bootstrap config would report a verdict against a policy the page
/// is not showing. Absent one the runner is already refusing to lease, and the
/// row reads degraded for that reason — so there is nothing a self-test would
/// add that the operator cannot already see.
pub fn shouldCapture(asked: bool, startup_done: bool, has_assignment: bool) bool {
    if (!has_assignment) return false;
    return asked or !startup_done;
}

/// The verdict awaiting its ride up, plus the policy snapshot its check names
/// borrow from. Never more than one: a second probe supersedes an unsent
/// verdict rather than queueing, because an operator asking again wants the
/// CURRENT state of the host, not a backlog.
pub const Pending = struct {
    alloc: std.mem.Allocator,
    result: ?selftest.Result = null,
    policy: ?protocol.AssignedPolicy = null,

    pub fn init(alloc: std.mem.Allocator) Pending {
        return .{ .alloc = alloc };
    }

    pub fn deinit(self: *Pending) void {
        self.clear();
    }

    /// Drop the held verdict. Called once the control plane has taken it, and
    /// again before a fresh probe overwrites it.
    pub fn clear(self: *Pending) void {
        if (self.result) |r| r.deinit(self.alloc);
        self.result = null;
        if (self.policy) |p| AppliedPolicy.freePolicy(self.alloc, p);
        self.policy = null;
    }

    /// The report to attach to this beat, or null when there is no verdict.
    /// Borrows from the held result — valid until the next `clear`/`capture`.
    pub fn report(self: *const Pending) ?protocol.SelftestReport {
        const r = self.result orelse return null;
        return .{
            .checks = r.checks,
            .all_ok = r.allOk(),
            .sandbox_tier = @tagName(r.sandbox_tier),
            .network_policy = @tagName(r.network_policy),
        };
    }

    /// Run one probe under the CURRENTLY ASSIGNED policy and hold the verdict.
    ///
    /// Under the assignment, not the bootstrap config: a verdict produced under
    /// the wrong policy is worse than none, because the page would render it
    /// against the assignment the operator is actually looking at. With no
    /// assignment yet there is nothing to probe under — the runner is already
    /// refusing to lease, and the row reads degraded for that reason.
    /// Returns whether a verdict was actually produced. The caller uses that to
    /// decide whether the startup proof is done: a transient workspace or
    /// allocation failure must not count as "probed", or one bad boot suppresses
    /// the startup self-test for the life of the daemon.
    pub fn capture(self: *Pending, io: std.Io, applied: *AppliedPolicy, cfg: Config) bool {
        self.clear();
        const pol = applied.snapshot(self.alloc) orelse {
            log.debug("selftest_skipped_no_assignment", .{});
            return false;
        };
        var eff = cfg;
        eff.sandbox_tier = pol.sandbox_tier;
        eff.network_policy = pol.network_policy;
        eff.registry_allowlist = pol.registry_allowlist;
        eff.extra_binds = pol.extra_binds;

        const workspace = probeWorkspace(io, self.alloc, cfg) catch |err| {
            AppliedPolicy.freePolicy(self.alloc, pol);
            log.warn("selftest_workspace_failed", .{ .err = @errorName(err) });
            return false;
        };
        defer self.alloc.free(workspace);

        const r = selftest_exec.run(io, self.alloc, eff, workspace) catch |err| {
            AppliedPolicy.freePolicy(self.alloc, pol);
            log.warn("selftest_probe_failed", .{ .err = @errorName(err) });
            return false;
        };
        // The snapshot is retained, not freed: `r`'s operator-bind check names
        // point into `pol.extra_binds`.
        self.result = r;
        self.policy = pol;
        log.info("selftest_completed", .{ .all_ok = r.allOk(), .checks = r.checks.len });
        return true;
    }
};

/// Ensure `{storage_home}/.selftest` exists and return its path (caller frees).
fn probeWorkspace(io: std.Io, alloc: std.mem.Allocator, cfg: Config) ![]u8 {
    const path = try std.fs.path.join(alloc, &.{ cfg.storage_home, WORKSPACE_DIR_NAME });
    errdefer alloc.free(path);
    std.Io.Dir.createDirAbsolute(io, path, .default_dir) catch |err| switch (err) {
        error.PathAlreadyExists => {},
        else => return err,
    };
    return path;
}

test {
    _ = @import("selftest_beat_test.zig");
}
