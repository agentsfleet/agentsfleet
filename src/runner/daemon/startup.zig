//! Startup steps the daemon runs before it enters the control loop.
//!
//! Split from `main.zig` so each step is drivable in a test rather than only
//! reachable by booting the process. Today that is one step: enabling the
//! delegated cgroup controllers.

const std = @import("std");
const builtin = @import("builtin");
const logging = @import("log");

const CgroupScope = @import("../engine/CgroupScope.zig");
const client_errors = @import("../engine/client_errors.zig");

const log = logging.scoped(.fleet_runner);
const ERR_EXEC_RUNNER_FLEET_INIT = client_errors.ERR_EXEC_RUNNER_FLEET_INIT;

/// Enable the delegated cgroup controllers for this host.
///
/// Which controllers the subtree carries is a HOST fact, settled before any
/// policy assignment exists — so it belongs at startup rather than on the first
/// cage-building heartbeat. systemd's `Delegate=` only makes the controllers
/// AVAILABLE in the unit cgroup; writing `cgroup.subtree_control` is the
/// delegatee's job and systemd never does it. Doing it here is what makes a
/// populated subtree a post-condition of the daemon being up — which is what
/// `engine/capability_probe.zig` reports upward and what the runner bootstrap
/// playbook's readiness gate asserts after a deploy.
///
/// Non-fatal by design: a `dev_none` host builds no cage and must still boot,
/// and it is the control plane's reconciliation of assigned-against-reported
/// capability — not this call — that refuses leases on a host which cannot
/// deliver its assigned isolation.
pub fn enableResourceControl(io: std.Io, alloc: std.mem.Allocator) void {
    CgroupScope.enableDelegatedControllers(io, alloc) catch |err| {
        if (!shouldReport(err)) return;
        log.err("cgroup_controllers_unavailable", .{ .error_code = ERR_EXEC_RUNNER_FLEET_INIT, .err = @errorName(err) });
    };
}

/// Whether an enablement error is a real fault worth reporting.
/// `UnsupportedPlatform` is the off-Linux no-op — there is no cgroup hierarchy
/// to write, which is expected rather than broken. Everything else names a host
/// that cannot cage a lease and must reach the journal. Pure, so the matrix is
/// unit-testable without a kernel.
///
/// Takes `anyerror` rather than `CgroupScope.CgroupError`: on Linux the
/// enablement path allocates, so its inferred error set is wider than the
/// declared cgroup set. That extra arm is comptime-dead on a macOS target, so
/// only a Linux build sees it — narrowing here compiles natively and breaks the
/// cross-compile.
pub fn shouldReport(err: anyerror) bool {
    return err != error.UnsupportedPlatform;
}

test "test_startup_enables_delegated_controllers: a non-Linux host is a silent no-op" {
    // The step runs unconditionally at startup, so it must be inert — not noisy,
    // not fatal — on a platform with no cgroup hierarchy at all.
    try std.testing.expect(!shouldReport(CgroupScope.CgroupError.UnsupportedPlatform));

    if (builtin.os.tag != .linux) {
        // Drives the real startup step off-Linux: it must return normally.
        // `undefined` io is never dereferenced because the platform check in
        // `enableDelegatedControllers` short-circuits ahead of any file access.
        enableResourceControl(undefined, std.testing.allocator);
    }
}

test "every real enablement failure is reported, including a lost delegation" {
    // A unit that lost `Delegate=`/`DelegateSubgroup=` resolves no base. That is
    // the fault the operator has to see — it means this host will run leases
    // with no limits — so it must never be swallowed like the off-Linux no-op.
    try std.testing.expect(shouldReport(CgroupScope.CgroupError.CgroupNotDelegated));
    try std.testing.expect(shouldReport(CgroupScope.CgroupError.CgroupWriteFailed));
    try std.testing.expect(shouldReport(CgroupScope.CgroupError.CgroupReadFailed));
    try std.testing.expect(shouldReport(CgroupScope.CgroupError.CgroupCreateFailed));
    try std.testing.expect(shouldReport(CgroupScope.CgroupError.CgroupMoveFailed));
}
