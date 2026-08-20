//! Unit tier for the probe child arm.
//!
//! The probe's checks are real syscalls, so their success paths belong to the
//! integration lane. What is asserted here is the wire shape between child and
//! parent, and the property that keeps the arm out of the operator surface —
//! both of which are pure and must hold on every platform.

const std = @import("std");
const selftest_probe = @import("selftest_probe.zig");
const contract = @import("contract");
const child_exec = @import("child_exec.zig");
const sandbox_hardening = @import("sandbox_hardening.zig");
const selftest_exec = @import("selftest_exec.zig");

test "the probe arm is hidden, like __execute" {
    // The spec pins `UNCHANGED agentsfleet-runner command surface`. The `__`
    // prefix is what keeps this out of the operator registry and out of
    // `--help`; a plain name here would be a second host entrypoint alongside
    // `doctor`, which §1 rejected by design.
    try std.testing.expect(std.mem.startsWith(u8, selftest_probe.SUBCOMMAND, "__"));
}

test "the child and the parent agree on the verdict alphabet" {
    // The child writes these characters; the parent switches on them. They live
    // in one enum so the two halves cannot drift (RULE UFS) — this asserts the
    // encoding itself, which is the part a careless edit would silently change.
    try std.testing.expectEqual(@as(u8, '1'), @intFromEnum(selftest_probe.Verdict.passed));
    try std.testing.expectEqual(@as(u8, '0'), @intFromEnum(selftest_probe.Verdict.failed));
    try std.testing.expectEqual(@as(u8, 'x'), @intFromEnum(selftest_probe.Verdict.untested));
}

test "a line built from the child's own keys parses back to the same verdicts" {
    // Round-trip: compose a line the way the child does, using the child's
    // exported keys, and read it with the parent's parser. Catches a key
    // renamed on one side only — which would silently degrade every check to
    // "failed" and red-flag every healthy runner.
    var buf: [128]u8 = undefined;
    const line = try std.fmt.bufPrint(&buf, "{s}1 {s}1 {s}1 {s}0 {s}x {s}1 {s}1", .{
        selftest_probe.KEY_RESOLVER,
        selftest_probe.KEY_SCRATCH,
        selftest_probe.KEY_HOME,
        selftest_probe.KEY_DNS,
        selftest_probe.KEY_EGRESS,
        selftest_probe.KEY_TRANSPORT,
        selftest_probe.KEY_BINDS,
    });
    const o = selftest_exec.outcomeFrom(line, false);
    try std.testing.expect(o.resolver_readable);
    try std.testing.expect(o.scratch_writable);
    try std.testing.expect(o.home_writable);
    try std.testing.expect(!o.dns_resolved);
    try std.testing.expect(o.dns_testable);
    try std.testing.expect(!o.egress_reachable);
    try std.testing.expect(o.transport_execs);
    try std.testing.expect(o.transport_testable);
    try std.testing.expect(o.extra_binds_present);
}

test "an untested transport is not read as a failed one" {
    // The two are different operator instructions — "install curl" versus "fix
    // the bind set" — so the parser must keep them apart. Collapsing them is
    // how `grade` would tell an operator to repair a sandbox that is fine.
    var buf: [128]u8 = undefined;
    const line = try std.fmt.bufPrint(&buf, "{s}1 {s}1 {s}1 {s}1 {s}1 {s}x {s}1", .{
        selftest_probe.KEY_RESOLVER,
        selftest_probe.KEY_SCRATCH,
        selftest_probe.KEY_HOME,
        selftest_probe.KEY_DNS,
        selftest_probe.KEY_EGRESS,
        selftest_probe.KEY_TRANSPORT,
        selftest_probe.KEY_BINDS,
    });
    const o = selftest_exec.outcomeFrom(line, false);
    try std.testing.expect(!o.transport_execs);
    try std.testing.expect(!o.transport_testable);
}

test "a probe that never reported a transport key does not certify one" {
    // Fail-closed, the same reading `scratch` and `home` take: an older probe
    // paired with this parser attempted no spawn, and a spawn nobody attempted
    // is not a pass. `testable` stays true so `grade` reports the exec fault
    // rather than "no curl on this host", which would be a false instruction.
    const o = selftest_exec.outcomeFrom("resolver=1 scratch=1 home=1 dns=1 egress=1 binds=1", false);
    try std.testing.expect(!o.transport_execs);
    try std.testing.expect(o.transport_testable);
}

test "the flag prefixes are distinct and each ends at its value" {
    // `--dial=` must not prefix-match `--bind-ro=` (or vice versa) or one
    // target would be parsed as the other.
    const flags = [_][]const u8{
        selftest_probe.RESOLVE_FLAG_PREFIX,
        selftest_probe.DIAL_FLAG_PREFIX,
        selftest_probe.TRANSPORT_FLAG_PREFIX,
        sandbox_hardening.BIND_RO_FLAG_PREFIX,
        sandbox_hardening.BIND_RW_FLAG_PREFIX,
        child_exec.WORKSPACE_FLAG_PREFIX,
    };
    for (flags, 0..) |a, i| {
        try std.testing.expect(std.mem.endsWith(u8, a, "="));
        for (flags[i + 1 ..]) |b| try std.testing.expect(!std.mem.startsWith(u8, a, b));
    }
}

test "the resolver path is the one the incident dangled" {
    // Pinned deliberately: the M167 outage was `/etc/resolv.conf` symlinking
    // into an unbound `/run/systemd/resolve`. A probe checking any other path
    // would not have caught it.
    try std.testing.expectEqualStrings("/etc/resolv.conf", selftest_probe.RESOLV_PATH);
}

test "a home the sandbox cannot provide is graded unusable before any write" {
    // The absent case is the one that shipped: HOME pointed at a host path the
    // sandbox never mounted, and nothing graded it. Each of these must fail
    // WITHOUT attempting I/O — a probe that tries to write to a relative path
    // resolves it against the workspace and reports a healthy home that isn't.
    try std.testing.expect(!selftest_probe.homePathUsable(null));
    try std.testing.expect(!selftest_probe.homePathUsable(""));
    try std.testing.expect(!selftest_probe.homePathUsable("relative/home"));
    try std.testing.expect(!selftest_probe.homePathUsable("."));
    try std.testing.expect(!selftest_probe.homePathUsable(".."));
}

test "the home a lease actually receives is usable" {
    // The positive arm, pinned against the real constant rather than a literal:
    // if CHILD_HOME ever stops being absolute the probe must stop passing it.
    try std.testing.expect(selftest_probe.homePathUsable(contract.protocol.CHILD_HOME));
    try std.testing.expect(selftest_probe.homePathUsable("/tmp"));
}
