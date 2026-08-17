//! §2 tests — the daemon proves egress from inside a real sandbox.
//!
//! Every test here is platform-INDEPENDENT, including the argv one. How a
//! verdict is graded is the product behaviour an operator reads, and which
//! sandbox the probe is built into is the security property — neither should
//! be provable only on a configured Linux box. The argv test goes through
//! `composeProbeArgv`, which takes the bwrap path as an argument, because
//! gating it on a real binary skipped it on macOS AND in continuous
//! integration (no bubblewrap in the CI image) — leaving Invariant 1 asserted
//! by nothing at all.
//!
//! What still needs a real sandbox is EXECUTION — that a sandbox built from
//! this argv actually starts and resolves a name. That is the integration
//! tier's job (RULE ITF), not this file's.

const std = @import("std");
const contract = @import("contract");

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

const FAKE_BWRAP = "/usr/bin/bwrap";
const FAKE_SELF_EXE = "/opt/agentsfleet/bin/agentsfleet-runner";

test "test_probe_argv_frees_its_partial_copy_when_an_allocation_fails" {
    // The probe argv is built one duped string at a time. A failure partway
    // through must free every string already copied AND the in-flight one, or
    // the daemon leaks a whole sandbox argv per attempt — and the self-test is
    // meant to run on a host that is ALREADY unhealthy, which is exactly when
    // an allocator is most likely to refuse. testing.allocator underneath the
    // FailingAllocator fails this test if anything survives.
    //
    // Walked rather than pinned to one index: the argv's length is a property
    // of the bind set, so a fixed index would silently stop covering the second
    // loop the moment the baseline grows.
    const alloc = std.testing.allocator;
    const c = cfg(.allow_all, &.{});

    const full = try selftest.composeProbeArgv(alloc, FAKE_BWRAP, FAKE_SELF_EXE, c, WORKSPACE);
    const argv_allocs = full.len + 1; // every string, plus the list itself
    sandbox_args.freeArgv(alloc, full);

    for (0..argv_allocs) |fail_index| {
        var fa = std.testing.FailingAllocator.init(alloc, .{ .fail_index = fail_index });
        try std.testing.expectError(
            error.OutOfMemory,
            selftest.composeProbeArgv(fa.allocator(), FAKE_BWRAP, FAKE_SELF_EXE, c, WORKSPACE),
        );
    }
}

test "test_probe_uses_the_lease_argv_builder" {
    const alloc = std.testing.allocator;
    // Dimension 2.1 / Invariant 1 — the probe's sandbox must be the SAME
    // construction a lease gets. A parallel argv built for testing would prove
    // nothing about real work, which is the whole failure mode this milestone
    // exists to close: the M167 incident had a green host check and a dead
    // sandbox. So the probe's prefix must be byte-identical to a lease's.
    const c = cfg(.allow_all, &.{});

    // Both go through the pure composition, so this runs on every platform.
    // Gating it on a real bubblewrap binary skipped it on macOS AND in
    // continuous integration, leaving Invariant 1 asserted by nothing.
    const lease = try sandbox_args.composeSandboxPrefix(alloc, FAKE_BWRAP, FAKE_SELF_EXE, c, WORKSPACE, null);
    defer sandbox_args.freeArgv(alloc, lease);

    const probe = try selftest.composeProbeArgv(alloc, FAKE_BWRAP, FAKE_SELF_EXE, c, WORKSPACE);
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

    // The same observation under an OPEN posture, against a registry the
    // operator DECLARED, is a real fault: assigning a registry asserts that
    // leases need it, so failing to reach it is actionable every time.
    var open_cfg = cfg(.allow_all, &.{});
    open_cfg.registry_allowlist = &.{"pypi.org"};
    const open = try selftest.grade(alloc, open_cfg, .{
        .resolver_readable = true,
        .dns_resolved = true,
        .egress_reachable = false,
    });
    defer open.deinit(alloc);
    try std.testing.expect(!findCheck(open, selftest.CHECK_EGRESS).?.ok);
}

test "an open posture with no declared registry reports egress as untested, not as broken" {
    const alloc = std.testing.allocator;
    // The probe dials only what the operator named. Reaching for the daemon's
    // own fallback registry set would red-flag a runner configured exactly as
    // intended — a red row nobody can act on, which is how an alert gets muted.
    const r = try selftest.grade(alloc, cfg(.allow_all, &.{}), .{
        .resolver_readable = true,
        .dns_resolved = true,
        .egress_reachable = false,
    });
    defer r.deinit(alloc);

    const egress = findCheck(r, selftest.CHECK_EGRESS).?;
    try std.testing.expect(egress.ok);
    try std.testing.expectEqualStrings(selftest.DETAIL_EGRESS_NONE_DECLARED, egress.detail);
    try std.testing.expect(r.allOk());
}

test "under deny_all_egress an unresolvable name is the assignment working, not a fault" {
    const alloc = std.testing.allocator;
    // Same reasoning the egress arm has always used: with no network by
    // assignment, no name can resolve. Grading that a fault makes every
    // correctly locked-down runner read unhealthy.
    const r = try selftest.grade(alloc, cfg(.deny_all_egress, &.{}), .{
        .resolver_readable = true,
        .dns_resolved = false,
        .egress_reachable = false,
    });
    defer r.deinit(alloc);

    const dns = findCheck(r, selftest.CHECK_DNS).?;
    try std.testing.expect(dns.ok);
    try std.testing.expectEqualStrings(selftest.DETAIL_DNS_NO_NETWORK, dns.detail);
    try std.testing.expect(r.allOk());
}

test "a sandbox with no resolver tool reports DNS untested rather than broken" {
    const alloc = std.testing.allocator;
    // "Not tested" and "tested and broken" are different facts. Collapsing
    // them would report a missing `getent` as a dead resolver and send an
    // operator hunting a network fault that does not exist.
    const r = try selftest.grade(alloc, cfg(.allow_all, &.{}), .{
        .resolver_readable = true,
        .dns_resolved = false,
        .egress_reachable = true,
        .dns_testable = false,
    });
    defer r.deinit(alloc);

    const dns = findCheck(r, selftest.CHECK_DNS).?;
    try std.testing.expect(dns.ok);
    try std.testing.expectEqualStrings(selftest.DETAIL_DNS_NOT_TESTABLE, dns.detail);
}

test "a timeout still outranks the posture arms — a hung probe proves nothing" {
    const alloc = std.testing.allocator;
    // The timeout branch must stay ahead of the deny_all and not-testable
    // arms: a reaped probe observed nothing, so reporting "expected, not a
    // fault" would turn a hang into a green check.
    const r = try selftest.grade(alloc, cfg(.deny_all_egress, &.{}), .{
        .resolver_readable = true,
        .dns_resolved = false,
        .egress_reachable = false,
        .timed_out = true,
    });
    defer r.deinit(alloc);

    const dns = findCheck(r, selftest.CHECK_DNS).?;
    try std.testing.expect(!dns.ok);
    try std.testing.expectEqualStrings(selftest.DETAIL_TIMEOUT, dns.detail);
    try std.testing.expect(!r.allOk());
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
        selftest.DETAIL_DNS_NO_NETWORK,
        selftest.DETAIL_DNS_NOT_TESTABLE,
        selftest.DETAIL_EGRESS_NONE_DECLARED,
        selftest.DETAIL_POSTURE_UNBUILDABLE,
        selftest.DETAIL_BIND_PRESENT_RO,
        selftest.DETAIL_BIND_PRESENT_RW,
        selftest.DETAIL_BIND_ABSENT,
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

    // The detail names the ASSIGNED mode without claiming the probe verified it
    // — bwrap enforces the mode in the kernel, and a bare "read-only" here would
    // read as a verification the probe never performed.
    const models = findCheck(r, "/srv/models").?;
    try std.testing.expectEqualStrings(selftest.DETAIL_BIND_PRESENT_RW, models.detail);
    const fonts = findCheck(r, "/srv/fonts").?;
    try std.testing.expectEqualStrings(selftest.DETAIL_BIND_PRESENT_RO, fonts.detail);

    // A bind that did not land says so, instead of echoing its mode as if it had.
    var absent = HEALTHY;
    absent.extra_binds_present = false;
    const missed = try selftest.grade(alloc, c, absent);
    defer missed.deinit(alloc);
    try std.testing.expectEqualStrings(selftest.DETAIL_BIND_ABSENT, findCheck(missed, "/srv/fonts").?.detail);
    try std.testing.expect(!findCheck(missed, "/srv/fonts").?.ok);
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

test "the probe only ever aims at what the assignment declared" {
    // `targetsFor` decides what the child is asked to reach. It is the guard
    // against the false red this milestone exists to remove: probing a name or
    // an endpoint the operator never declared red-flags a runner that is
    // configured exactly as intended.

    // Nothing declared → nothing dialled, nothing resolved.
    const bare = selftest.targetsFor(cfg(.allow_all, &.{}));
    try std.testing.expectEqual(@as(?[]const u8, null), bare.resolve);
    try std.testing.expectEqual(@as(?[]const u8, null), bare.dial);

    // A declared registry is both the name to resolve and the endpoint to dial.
    var declared = cfg(.allow_all, &.{});
    declared.registry_allowlist = &.{"pypi.org"};
    const open = selftest.targetsFor(declared);
    try std.testing.expectEqualStrings("pypi.org", open.resolve.?);
    try std.testing.expectEqualStrings("pypi.org", open.dial.?);

    // An explicit port belongs to the dial target, never to the name lookup.
    var ported = cfg(.allow_all, &.{});
    ported.registry_allowlist = &.{"registry.npmjs.org:5000"};
    const with_port = selftest.targetsFor(ported);
    try std.testing.expectEqualStrings("registry.npmjs.org", with_port.resolve.?);
    try std.testing.expectEqualStrings("registry.npmjs.org:5000", with_port.dial.?);

    // Under deny_all there is no network by assignment, so spending a probe
    // timeout to rediscover that would be pure latency — `grade` already knows.
    var denied = cfg(.deny_all_egress, &.{});
    denied.registry_allowlist = &.{"pypi.org"};
    const closed = selftest.targetsFor(denied);
    try std.testing.expectEqual(@as(?[]const u8, null), closed.resolve);
    try std.testing.expectEqual(@as(?[]const u8, null), closed.dial);
}

test "operator binds are confirmed under every posture, including deny_all" {
    // A mount is a filesystem question. A locked-down network says nothing
    // about whether the operator's path landed, so the bind checks must survive
    // the posture that skips the network targets.
    const binds = [_]contract.protocol.ExtraBind{.{ .path = "/srv/models" }};
    const denied = selftest.targetsFor(cfg(.deny_all_egress, &binds));
    try std.testing.expectEqual(@as(usize, 1), denied.binds.len);
    try std.testing.expectEqualStrings("/srv/models", denied.binds[0].path);

    const open = selftest.targetsFor(cfg(.allow_all, &binds));
    try std.testing.expectEqual(@as(usize, 1), open.binds.len);
}
