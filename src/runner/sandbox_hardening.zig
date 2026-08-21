//! sandbox_hardening.zig — the in-child hardening a sandboxed tier requires,
//! and the argv wire that carries the operator binds it must admit.
//!
//! Shared by `__execute` (the lease child) and `__selftest_probe` so the probe
//! proves the SAME constraint set a lease runs under: a probe that skipped the
//! landlock layer graded the M136 resolver fault as healthy, because only the
//! constrained child could not read the resolver target. Split from
//! `child_exec.zig` on the 350-line bound (RULE FLL).

const std = @import("std");
const builtin = @import("builtin");
const contract = @import("contract");

const landlock = @import("engine/landlock.zig");

/// Re-exported so `selftest_probe` reaches the policy layer's own writable
/// device set through the module that already carries the lease's hardening,
/// rather than reaching past it into `engine/`.
pub const FLOOR_RW_FILES = landlock.LANDLOCK_FLOOR_RW_FILES;
const seccomp = @import("engine/seccomp.zig");

/// Repeatable operator-bind flags the parent forwards so the child's landlock
/// ruleset admits the same mounts bwrap made. Mode-explicit, one prefix per
/// mode, so an unstated mode can never widen access (the bind contract's
/// default-closed posture).
pub const BIND_RO_FLAG_PREFIX = "--bind-ro=";
pub const BIND_RW_FLAG_PREFIX = "--bind-rw=";

/// Apply the mandatory hardening, in order: no_new_privs → landlock
/// (workspace + baseline + operator binds) → seccomp. A runtime seccomp trap
/// exits SECCOMP_VIOLATION_EXIT → landlock_deny (see seccomp.zig). Callers
/// fail closed on any error — a sandbox that cannot be established never runs
/// work and never reports a verdict.
pub fn applySandboxHardening(workspace: []const u8, extra_binds: []const contract.protocol.ExtraBind) !void {
    try applyNoNewPrivs();
    try landlock.applyPolicy(workspace, extra_binds);
    try seccomp.applyFilter();
}

/// Collect the parent's repeatable bind flags into `buf`, borrowed from argv.
/// Overflow errors rather than truncates: a landlock ruleset missing an
/// assigned mount would fail leases confusingly, and the daemon-side validator
/// already caps the list at this same bound.
pub fn collectBindFlags(argv: []const [:0]const u8, buf: []contract.protocol.ExtraBind) error{TooManyBinds}![]const contract.protocol.ExtraBind {
    var n: usize = 0;
    for (argv) |a| {
        const entry: ?contract.protocol.ExtraBind = if (std.mem.startsWith(u8, a, BIND_RO_FLAG_PREFIX))
            .{ .path = a[BIND_RO_FLAG_PREFIX.len..], .mode = .read_only }
        else if (std.mem.startsWith(u8, a, BIND_RW_FLAG_PREFIX))
            .{ .path = a[BIND_RW_FLAG_PREFIX.len..], .mode = .read_write }
        else
            null;
        if (entry) |e| {
            if (n == buf.len) return error.TooManyBinds;
            buf[n] = e;
            n += 1;
        }
    }
    return buf[0..n];
}

/// Set `PR_SET_NO_NEW_PRIVS`: after this, no exec in this child or any
/// descendant can gain privilege via a setuid/setgid binary — the setuid helpers
/// in the RO-bound /usr,/bin,/sbin become inert. It is additive (does NOT remove
/// the userns CAP_SYS_ADMIN that Landlock rides today, so Landlock keeps working)
/// and must run BEFORE landlock_restrict_self. Linux-only: the `--sandboxed` flag
/// is set only on Linux tiers (the parent's establishSandbox fail-closes any
/// other host), so the non-Linux branch is just compile-portability.
fn applyNoNewPrivs() error{NoNewPrivsFailed}!void {
    if (builtin.os.tag != .linux) return;
    // prctl(PR_SET_NO_NEW_PRIVS=38, 1=set, 0,0,0) → 0 on success.
    if (std.os.linux.prctl(@intFromEnum(std.os.linux.PR.SET_NO_NEW_PRIVS), 1, 0, 0, 0) != 0)
        return error.NoNewPrivsFailed;
}

test "collectBindFlags maps each mode prefix and fails closed past the cap" {
    var buf: [contract.protocol.MAX_EXTRA_BINDS]contract.protocol.ExtraBind = undefined;
    const argv = [_][:0]const u8{ "exe", "__x", "--bind-ro=/srv/models", "--bind-rw=/srv/cache", "--other" };
    const got = try collectBindFlags(&argv, &buf);
    try std.testing.expectEqual(@as(usize, 2), got.len);
    try std.testing.expectEqualStrings("/srv/models", got[0].path);
    try std.testing.expectEqual(contract.protocol.BindMode.read_only, got[0].mode);
    try std.testing.expectEqualStrings("/srv/cache", got[1].path);
    try std.testing.expectEqual(contract.protocol.BindMode.read_write, got[1].mode);

    // One slot, two flags → the whole parse refuses rather than truncating: a
    // ruleset silently missing an assigned mount is the confusing failure.
    var one: [1]contract.protocol.ExtraBind = undefined;
    try std.testing.expectError(error.TooManyBinds, collectBindFlags(&argv, &one));
}
