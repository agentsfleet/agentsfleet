//! §2 tests — the daemon proves egress from inside a real sandbox.
//!
//! The grading tests are platform-INDEPENDENT: how a verdict is graded is the
//! product behaviour an operator reads, so it is proven on every host. The argv
//! tests are Linux-gated because bwrap flags exist only there.

const std = @import("std");
const builtin = @import("builtin");
const contract = @import("contract");
const common = @import("common");

const selftest = @import("selftest.zig");
const sandbox_args = @import("sandbox_args.zig");
const Config = @import("daemon/config.zig");

const WORKSPACE = "/tmp/fleet-ws-selftest";

fn cfg(policy: contract.protocol.NetworkPolicy, binds: []const contract.protocol.ExtraBind) Config {
    return Config{
        .control_plane_url = "http://127.0.0.1:8080",
        .runner_token = "agt_rtest",
        .sandbox_tier = .landlock_full,
        .storage_home = "/tmp/agentsfleet-runner",
        .network_policy = policy,
        .worker_count = 1,
        .cp_deadlines = .{},
        .registry_allowlist = &.{},
        .extra_binds = binds,
        .alloc = std.testing.allocator,
    };
}

const HEALTHY: selftest.Outcome = .{
    .resolver_readable = true,
    .dns_resolved = true,
    .egress_reachable = true,
};

fn findCheck(r: selftest.Result, name: []const u8) ?selftest.Check {
    for (r.checks) |c| {
        if (std.mem.eql(u8, c.name, name)) return c;
    }
    return null;
}

test "test_probe_uses_the_lease_argv_builder" {
    if (builtin.os.tag != .linux) return error.SkipZigTest;
    const alloc = std.testing.allocator;
    // Dimension 2.1 / Invariant 1 — the probe's sandbox must be the SAME
    // construction a lease gets. A parallel argv built for testing would prove
    // nothing about real work, which is the whole failure mode this milestone
    // exists to close: the M167 incident had a green host check and a dead
    // sandbox. So the probe's prefix must be byte-identical to a lease's.
    const c = cfg(.allow_all, &.{});

    const lease = sandbox_args.buildArgv(common.globalIo(), alloc, c, WORKSPACE, null) catch |err| {
        try std.testing.expectEqual(error.BwrapUnavailable, err);
        return error.SkipZigTest;
    };
    defer sandbox_args.freeArgv(alloc, lease);

    const probe = try selftest.buildProbeArgv(common.globalIo(), alloc, c, WORKSPACE);
    defer sandbox_args.freeArgv(alloc, probe);

    // Both end their sandbox wrapper at `--`; everything before it is the
    // sandbox itself (namespaces, mounts, network posture).
    var lease_cut: usize = 0;
    for (lease, 0..) |s, i| {
        if (std.mem.eql(u8, s, "--")) {
            lease_cut = i;
            break;
        }
    }
    var probe_cut: usize = 0;
    for (probe, 0..) |s, i| {
        if (std.mem.eql(u8, s, "--")) {
            probe_cut = i;
            break;
        }
    }
    try std.testing.expect(lease_cut > 0);
    try std.testing.expectEqual(lease_cut, probe_cut);
    for (lease[0..lease_cut], probe[0..probe_cut]) |a, b| {
        try std.testing.expectEqualStrings(a, b);
    }

    // And the probe does NOT re-exec the real executor — the sandbox is what
    // must be identical, not the payload.
    for (probe) |s| {
        try std.testing.expect(!std.mem.eql(u8, s, "__execute"));
    }
}

test "test_probe_detects_a_dangling_resolver" {
    const alloc = std.testing.allocator;
    // Dimension 2.2 — the incident itself. An unreadable /etc/resolv.conf is
    // reported as a named failed check naming the mechanism, so an operator
    // reads the cause instead of "leases keep dying".
    const r = try selftest.grade(alloc, cfg(.allow_all, &.{}), .{
        .resolver_readable = false,
        .dns_resolved = false,
        .egress_reachable = false,
    });
    defer r.deinit(alloc);

    const resolver = findCheck(r, selftest.CHECK_RESOLVER).?;
    try std.testing.expect(!resolver.ok);
    try std.testing.expectEqualStrings(selftest.DETAIL_RESOLVER_DANGLING, resolver.detail);
    try std.testing.expect(!r.allOk());
}

test "test_probe_reports_deny_all_as_expected" {
    const alloc = std.testing.allocator;
    // Dimension 2.3 — under deny_all_egress there is no network BY DESIGN, so
    // an unreachable endpoint is the correct outcome. Grading it a fault would
    // make every correctly-configured deny_all runner report unhealthy, and an
    // alert that fires on correct configuration gets muted — then it is not
    // there when the sandbox really breaks.
    const r = try selftest.grade(alloc, cfg(.deny_all_egress, &.{}), .{
        .resolver_readable = true,
        .dns_resolved = true,
        .egress_reachable = false,
    });
    defer r.deinit(alloc);

    const egress = findCheck(r, selftest.CHECK_EGRESS).?;
    try std.testing.expect(egress.ok);
    try std.testing.expectEqualStrings(selftest.DETAIL_EGRESS_DENIED_EXPECTED, egress.detail);
    try std.testing.expect(r.allOk());

    // The same observation under an OPEN posture is a real fault.
    const open = try selftest.grade(alloc, cfg(.allow_all, &.{}), .{
        .resolver_readable = true,
        .dns_resolved = true,
        .egress_reachable = false,
    });
    defer open.deinit(alloc);
    try std.testing.expect(!findCheck(open, selftest.CHECK_EGRESS).?.ok);
}

test "test_probe_timeout_reaps_and_reports" {
    const alloc = std.testing.allocator;
    // Dimension 2.4 — a hung probe reports a timeout verdict rather than
    // hanging the heartbeat or reporting a false negative.
    const r = try selftest.grade(alloc, cfg(.allow_all, &.{}), .{
        .resolver_readable = true,
        .dns_resolved = false,
        .egress_reachable = false,
        .timed_out = true,
    });
    defer r.deinit(alloc);

    const dns = findCheck(r, selftest.CHECK_DNS).?;
    try std.testing.expect(!dns.ok);
    try std.testing.expectEqualStrings(selftest.DETAIL_TIMEOUT, dns.detail);
}

test "test_probe_result_carries_no_secrets" {
    const alloc = std.testing.allocator;
    // Dimension 2.5 / Invariant 7 — every detail must come from the fixed
    // vocabulary, never from child output or the environment. The config below
    // carries a token and a URL precisely so the assertion has something to
    // catch if a future edit starts interpolating them.
    const c = cfg(.allow_all, &.{
        .{ .path = "/srv/models", .mode = .read_write, .note = "shared model cache" },
    });
    const r = try selftest.grade(alloc, c, .{
        .resolver_readable = false,
        .dns_resolved = false,
        .egress_reachable = false,
    });
    defer r.deinit(alloc);

    const VOCAB = [_][]const u8{
        selftest.DETAIL_OK,
        selftest.DETAIL_RESOLVER_DANGLING,
        selftest.DETAIL_DNS_FAILED,
        selftest.DETAIL_EGRESS_BLOCKED,
        selftest.DETAIL_EGRESS_DENIED_EXPECTED,
        selftest.DETAIL_TIMEOUT,
        selftest.DETAIL_NO_BWRAP,
        selftest.DETAIL_SPAWN_FAILED,
        "read-only",
        "read-write",
    };
    for (r.checks) |check| {
        var known = false;
        for (VOCAB) |v| {
            if (std.mem.eql(u8, check.detail, v)) known = true;
        }
        try std.testing.expect(known);
        // Belt and braces: the token and control-plane URL never appear.
        try std.testing.expect(std.mem.indexOf(u8, check.detail, c.runner_token) == null);
        try std.testing.expect(std.mem.indexOf(u8, check.detail, c.control_plane_url) == null);
    }
}

test "test_selftest_reports_operator_binds_individually" {
    const alloc = std.testing.allocator;
    // Dimension 4.5 — one named check per operator bind, carrying its mode, so
    // an operator sees WHICH entry did not land and which one can be written.
    const c = cfg(.allow_all, &.{
        .{ .path = "/srv/models", .mode = .read_write, .note = "shared model cache" },
        .{ .path = "/srv/fonts" },
    });
    const r = try selftest.grade(alloc, c, HEALTHY);
    defer r.deinit(alloc);

    const models = findCheck(r, "/srv/models").?;
    try std.testing.expectEqualStrings("read-write", models.detail);
    const fonts = findCheck(r, "/srv/fonts").?;
    try std.testing.expectEqualStrings("read-only", fonts.detail);
}

test "an unestablished sandbox is a named failed check, never a silent pass" {
    const alloc = std.testing.allocator;
    // A host with no bwrap must not read as healthy. The probe reports the
    // mechanism by name so the operator knows what to install.
    const r = try selftest.unavailable(alloc, cfg(.allow_all, &.{}), selftest.DETAIL_NO_BWRAP);
    defer r.deinit(alloc);

    try std.testing.expect(!r.allOk());
    try std.testing.expectEqualStrings(selftest.CHECK_SANDBOX, r.checks[0].name);
    try std.testing.expectEqualStrings(selftest.DETAIL_NO_BWRAP, r.checks[0].detail);
}

test "the result records the policy it ran under, so a stale result is detectable" {
    const alloc = std.testing.allocator;
    // Invariant 4 — a result that outlives its assignment must render as stale
    // rather than as a verdict on the current policy, which needs the policy
    // stored ON the result.
    const r = try selftest.grade(alloc, cfg(.deny_all_egress, &.{}), HEALTHY);
    defer r.deinit(alloc);

    try std.testing.expectEqual(contract.protocol.NetworkPolicy.deny_all_egress, r.network_policy);
    try std.testing.expectEqual(contract.protocol.SandboxTier.landlock_full, r.sandbox_tier);
}
