//! Unit tier for the probe-verdict parser.
//!
//! The spawn/reap half needs a real bubblewrap and lives in the integration
//! lane (`sandbox_integration_test.zig`). What is proven here is the part that
//! turns a child's line into an operator's verdict — because a parser that
//! reads a missing key as "passed" would report a probe that never ran as a
//! healthy sandbox, which is the exact failure this milestone exists to remove.

const std = @import("std");
const builtin = @import("builtin");
const common = @import("common");
const Config = @import("daemon/config.zig");
const sandbox_args = @import("sandbox_args.zig");
const selftest = @import("selftest.zig");
const selftest_exec = @import("selftest_exec.zig");

const PASSING = "resolver=1 scratch=1 home=1 dns=1 egress=1 binds=1\n";

test "every check reads back off a full passing line" {
    const o = selftest_exec.outcomeFrom(PASSING, false);
    try std.testing.expect(o.resolver_readable);
    try std.testing.expect(o.scratch_writable);
    try std.testing.expect(o.home_writable);
    try std.testing.expect(o.dns_resolved);
    try std.testing.expect(o.egress_reachable);
    try std.testing.expect(o.extra_binds_present);
    try std.testing.expect(o.dns_testable);
    try std.testing.expect(!o.timed_out);
}

test "a refused scratch write reads back failed" {
    const o = selftest_exec.outcomeFrom("resolver=1 scratch=0 dns=1 egress=1 binds=1", false);
    try std.testing.expect(!o.scratch_writable);
}

test "a line with no scratch key reads scratch as failed" {
    // Fail-closed: a probe that never attempted the write cannot certify it.
    // The probe and parser ship in one binary, so this line is unreachable in
    // production — the pin exists for exactly the day that stops being true.
    const o = selftest_exec.outcomeFrom("resolver=1 dns=1 egress=1 binds=1", false);
    try std.testing.expect(!o.scratch_writable);
    try std.testing.expect(o.resolver_readable);
}

test "a refused home write reads back failed" {
    const o = selftest_exec.outcomeFrom("resolver=1 scratch=1 home=0 dns=1 egress=1 binds=1", false);
    try std.testing.expect(!o.home_writable);
    // The shape that shipped: the floor is writable, the home is not.
    try std.testing.expect(o.scratch_writable);
}

test "a line with no home key reads home as failed" {
    // Same fail-closed reading as scratch: an absent key is a write nobody
    // attempted, and a write nobody attempted is not a pass. This is also the
    // exact line an OLD probe emits, so an unupgraded probe grades red rather
    // than certifying a home it never looked at.
    const o = selftest_exec.outcomeFrom("resolver=1 scratch=1 dns=1 egress=1 binds=1", false);
    try std.testing.expect(!o.home_writable);
    try std.testing.expect(o.scratch_writable);
}

test "a check the probe never ran is untested, not failed" {
    // `x` is the probe saying "nothing asked me to test this". Reading it as a
    // failure would red-flag a runner that declared no registry — a correct
    // configuration, not a fault.
    const o = selftest_exec.outcomeFrom("resolver=1 dns=x egress=x binds=x", false);
    try std.testing.expect(o.resolver_readable);
    try std.testing.expect(!o.dns_testable);
    // Binds are the one check that does NOT get the benefit of the doubt: only
    // an explicit pass certifies an assigned mount. `grade` iterates the
    // assigned list, so this reads false-but-unused when nothing is assigned,
    // and fail-closed when something is.
    try std.testing.expect(!o.extra_binds_present);
}

test "an assigned bind is never certified by a probe that did not look for it" {
    // The failure this guards: a probe built without the bind arguments emits
    // `binds=x`, and treating "untested" as present would report every
    // operator-assigned mount healthy without one having been checked —
    // Dimension 4.5 reporting a green row it never earned.
    const untested = selftest_exec.outcomeFrom("resolver=1 dns=1 egress=1 binds=x", false);
    try std.testing.expect(!untested.extra_binds_present);
    const passed = selftest_exec.outcomeFrom("resolver=1 dns=1 egress=1 binds=1", false);
    try std.testing.expect(passed.extra_binds_present);
}

test "an operator bind that did not land reads as absent" {
    const o = selftest_exec.outcomeFrom("resolver=1 dns=1 egress=1 binds=0", false);
    try std.testing.expect(!o.extra_binds_present);
}

test "a silent probe passes nothing" {
    // Empty stdout means the child died before printing, or printed nothing we
    // could read. Every check must read false: a probe that said nothing has
    // proven nothing, and defaulting to pass is how a dead sandbox reads green.
    const o = selftest_exec.outcomeFrom("", false);
    try std.testing.expect(!o.resolver_readable);
    try std.testing.expect(!o.dns_resolved);
    try std.testing.expect(!o.egress_reachable);
}

test "a truncated line does not read the next check's verdict" {
    // Key present, value cut off by the read cap. Must not fall through to
    // whatever byte follows.
    const o = selftest_exec.outcomeFrom("resolver=1 dns=", false);
    try std.testing.expect(o.resolver_readable);
    try std.testing.expect(!o.dns_resolved);
}

test "checks are matched by key, not by position" {
    // A future check inserted ahead of `egress` must not shift its verdict onto
    // a neighbour — the parser indexes by name for exactly this reason.
    const reordered = "binds=1 egress=0 dns=1 resolver=1";
    const o = selftest_exec.outcomeFrom(reordered, false);
    try std.testing.expect(o.resolver_readable);
    try std.testing.expect(o.dns_resolved);
    try std.testing.expect(!o.egress_reachable);
    try std.testing.expect(o.extra_binds_present);
}

test "an unrecognised verdict character is not a pass" {
    const o = selftest_exec.outcomeFrom("resolver=? dns=1 egress=1 binds=1", false);
    try std.testing.expect(!o.resolver_readable);
}

test "a reaped probe reports nothing it half-observed" {
    // The child may have printed a partial line before the kill landed.
    // Presenting that as fact would render a half-run as a verdict.
    const o = selftest_exec.outcomeFrom(PASSING, true);
    try std.testing.expect(o.timed_out);
    try std.testing.expect(!o.resolver_readable);
    try std.testing.expect(!o.dns_resolved);
    try std.testing.expect(!o.egress_reachable);
    try std.testing.expect(!o.extra_binds_present);
}

test "a child that did not exit cleanly is never trusted" {
    // The probe prints and then returns 0. A non-zero status means it died
    // partway, so its line describes a run that did not finish — accepting it
    // would let a crash halfway through report the checks it managed to print.
    try std.testing.expect(selftest_exec.exitedClean(.{ .exited = 0 }));
    try std.testing.expect(!selftest_exec.exitedClean(.{ .exited = 1 }));
    try std.testing.expect(!selftest_exec.exitedClean(.{ .signal = .KILL }));
    // A wait we could not perform proves nothing about the child.
    try std.testing.expect(!selftest_exec.exitedClean(null));
}

test "the drain reaches end-of-file even when the child overruns the cap" {
    // The regression this pins: stopping at a full buffer returns WITHOUT
    // end-of-file, and the caller retires the watchdog on that return — so a
    // chatty-then-hung child would leave the reap blocked with nothing alive to
    // kill it. Reading past the cap and discarding is what keeps EOF the exit.
    const io = common.globalIo();
    const path = "/tmp/agentsfleet-selftest-drain-test";
    const long = "resolver=1 dns=1 egress=1 binds=1\n" ++ ("x" ** 400);
    try std.Io.Dir.cwd().writeFile(io, .{ .sub_path = path, .data = long });
    defer std.Io.Dir.cwd().deleteFile(io, path) catch |err|
        std.debug.print("drain fixture cleanup ignored: {s}\n", .{@errorName(err)});

    const file = try std.Io.Dir.openFileAbsolute(io, path, .{});
    defer file.close(io);

    var buf: [128]u8 = undefined;
    const line = selftest_exec.drainVerdict(io, file, &buf);
    try std.testing.expectEqual(@as(usize, 128), line.len);
    // The verdict still parses out of the retained prefix.
    const o = selftest_exec.outcomeFrom(line, false);
    try std.testing.expect(o.resolver_readable);
    try std.testing.expect(o.extra_binds_present);
}

test "a host without bubblewrap reports an unestablished sandbox, not an empty panel" {
    // Runs on exactly the host the coverage lane uses: Linux, no bwrap. Guarded
    // twice — off Linux the tier is not sandboxed and `run` would spawn the
    // probe for real, and with a real bwrap present it would build a sandbox.
    if (builtin.os.tag != .linux) return error.SkipZigTest;
    const io = common.globalIo();
    if (sandbox_args.bwrapPath(io) != null) return error.SkipZigTest;

    const alloc = std.testing.allocator;
    var daemon_env: std.process.Environ.Map = .init(alloc);
    defer daemon_env.deinit();
    const r = try selftest_exec.run(io, alloc, probeCfg(), &daemon_env, "/tmp");
    defer r.deinit(alloc);

    // A missing mechanism is a named failed CHECK, never an error return — the
    // operator reads what to install instead of an empty self-test.
    try std.testing.expect(!r.allOk());
    try std.testing.expectEqual(@as(usize, 1), r.checks.len);
    try std.testing.expectEqualStrings(selftest.CHECK_SANDBOX, r.checks[0].name);
    try std.testing.expectEqualStrings(selftest.DETAIL_NO_BWRAP, r.checks[0].detail);
}

fn probeCfg() Config {
    return .{
        .control_plane_url = "http://127.0.0.1:8080",
        .runner_token = "agt_rtest",
        .sandbox_tier = .landlock_full,
        .storage_home = "/tmp/agentsfleet-runner",
        .network_policy = .deny_all_egress,
        .worker_count = 1,
        .cp_deadlines = .{},
        .registry_allowlist = &.{},
        .alloc = std.testing.allocator,
    };
}
