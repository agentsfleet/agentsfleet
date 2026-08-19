//! Real-sandbox execution proofs for the self-test probe.
//!
//! Linux + a real `bwrap` only; `SkipZigTest` everywhere else. These are the
//! arms that a pure grading test cannot stand in for: they spawn the ACTUAL
//! probe through the ACTUAL lease argv builder and read what it found.
//!
//! Why that distinction is load-bearing here. `selftest_test.zig` proves
//! `grade` turns an `Outcome` into the right verdict, but it hands `grade` a
//! struct literal — so it would keep passing if the probe never ran, which is
//! precisely the state this milestone shipped in before these tests existed.
//! A dangling resolver has to be DETECTED, not described.
//!
//! Run on Linux: `make test-integration-agentsfleet-runner`. In continuous
//! integration (CI) the `test-integration-kernel` lane runs privileged with
//! bubblewrap baked into the image, which is what lets these establish
//! namespaces at all.

const std = @import("std");
const builtin = @import("builtin");
const contract = @import("contract");

const Config = @import("daemon/config.zig");
const sandbox_args = @import("sandbox_args.zig");
const selftest = @import("selftest.zig");
const selftest_exec = @import("selftest_exec.zig");
const selftest_probe = @import("selftest_probe.zig");

const WORKSPACE = "/tmp/agentsfleet-selftest-probe-test";

fn baseCfg() Config {
    return .{
        .control_plane_url = "http://127.0.0.1:8080",
        .runner_token = "agt_rtest",
        .sandbox_tier = contract.protocol.SandboxTier.landlock_full,
        .storage_home = "/tmp/agentsfleet-runner",
        .network_policy = .allow_all,
        .worker_count = 1,
        .cp_deadlines = .{},
        .registry_allowlist = &.{},
        // SAFETY: the probe path takes its allocator as a parameter and never
        // reads `cfg.alloc`; every test below passes `std.testing.allocator`
        // explicitly, so this field is never observed.
        .alloc = undefined,
    };
}

fn spawnIo(threaded: *std.Io.Threaded) std.Io {
    threaded.* = .init(std.testing.allocator, .{});
    return threaded.io();
}

fn makeWorkspace(io: std.Io) !void {
    std.Io.Dir.createDirAbsolute(io, WORKSPACE, .default_dir) catch |err| switch (err) {
        error.PathAlreadyExists => {},
        else => return err,
    };
}

/// Index of the probe's own tail (`<self_exe> __selftest_probe ...`) in a built
/// argv — everything before it is bwrap's option list.
fn probeTailIndex(argv: []const []const u8) ?usize {
    for (argv, 0..) |a, i| {
        if (std.mem.eql(u8, a, selftest_probe.SUBCOMMAND)) return i - 1;
    }
    return null;
}

/// Did the probe actually execute here? A silent probe means the child exe this
/// build baked in (`stub_runner_exe_path`) does not resolve on THIS filesystem
/// — true when a cross-compiled binary is run somewhere other than where it was
/// built. That is a harness fact, not a verdict.
///
/// Every real-execution test below gates on this. Without the gate,
/// `test_probe_detects_a_dangling_resolver` passes when the probe never ran at
/// all: an empty line parses to every check failing, which looks exactly like a
/// correctly detected dangling resolver. A test that green-lights on a probe
/// that did not run is the same false confidence the milestone exists to
/// remove — so it is checked, not assumed.
fn probeRanHere(io: std.Io, alloc: std.mem.Allocator) !bool {
    const argv = selftest.buildProbeArgv(io, alloc, baseCfg(), WORKSPACE) catch |err| {
        try std.testing.expectEqual(error.BwrapUnavailable, err);
        return error.SkipZigTest;
    };
    defer sandbox_args.freeArgv(alloc, argv);
    var buf: [128]u8 = undefined;
    const line = try runProbeArgv(io, argv, &buf);
    return line.len > 0;
}

/// Spawn a probe argv directly and return its verdict line (into `buf`).
/// Bypasses `selftest_exec.run` deliberately: these tests need to MUTATE the
/// sandbox the probe runs in, which the production path rightly does not allow.
fn runProbeArgv(io: std.Io, argv: []const []const u8, buf: []u8) ![]const u8 {
    var child = try std.process.spawn(io, .{
        .argv = argv,
        .stdin = .ignore,
        .stdout = .pipe,
        .stderr = .ignore,
        .pgid = 0,
    });
    const out = child.stdout orelse return error.NoProbeStdout;
    var fr = out.reader(io, &.{});
    const len = fr.interface.readSliceShort(buf) catch 0;
    _ = child.wait(io) catch |err| std.debug.print("probe wait: {s}\n", .{@errorName(err)});
    return buf[0..len];
}

test "test_probe_detects_a_dangling_resolver" {
    // Dimension 2.2 — THE incident, reproduced. `/etc/resolv.conf` is
    // unreachable inside the sandbox while being perfectly fine on the host,
    // which is the exact asymmetry that made the outage invisible for a week:
    // `doctor` reads the host and answered ok the whole time.
    if (builtin.os.tag != .linux) return error.SkipZigTest;
    const alloc = std.testing.allocator;
    var threaded: std.Io.Threaded = undefined;
    const io = spawnIo(&threaded);
    defer threaded.deinit();
    try makeWorkspace(io);
    if (!try probeRanHere(io, alloc)) return error.SkipZigTest;

    const argv = selftest.buildProbeArgv(io, alloc, baseCfg(), WORKSPACE) catch |err| {
        try std.testing.expectEqual(error.BwrapUnavailable, err);
        return error.SkipZigTest;
    };
    defer sandbox_args.freeArgv(alloc, argv);

    // Splice `--tmpfs /etc` in AFTER the baseline binds and before the probe's
    // own tail, so it wins the ordering and the resolver file vanishes from the
    // sandbox — without touching the host's /etc.
    const tail = probeTailIndex(argv) orelse return error.NoProbeTail;
    var broken: std.ArrayList([]const u8) = .empty;
    defer broken.deinit(alloc);
    try broken.appendSlice(alloc, argv[0..tail]);
    try broken.appendSlice(alloc, &.{ "--tmpfs", "/etc" });
    try broken.appendSlice(alloc, argv[tail..]);

    var buf: [128]u8 = undefined;
    const line = try runProbeArgv(io, broken.items, &buf);
    const o = selftest_exec.outcomeFrom(line, false);
    try std.testing.expect(!o.resolver_readable);

    // And the operator reads the mechanism, not a red dot.
    const graded = try selftest.grade(alloc, baseCfg(), o);
    defer graded.deinit(alloc);
    try std.testing.expect(!graded.allOk());
    for (graded.checks) |c| {
        if (std.mem.eql(u8, c.name, selftest.CHECK_RESOLVER)) {
            try std.testing.expectEqualStrings(selftest.DETAIL_RESOLVER_DANGLING, c.detail);
            return;
        }
    }
    return error.ResolverCheckMissing;
}

test "the resolver check passes in an unmodified sandbox" {
    // The control for the test above. Without it, a probe that reported
    // `resolver=0` unconditionally — a broken probe — would satisfy 2.2 and
    // red-flag every healthy runner in the fleet.
    if (builtin.os.tag != .linux) return error.SkipZigTest;
    const alloc = std.testing.allocator;
    var threaded: std.Io.Threaded = undefined;
    const io = spawnIo(&threaded);
    defer threaded.deinit();
    try makeWorkspace(io);
    if (!try probeRanHere(io, alloc)) return error.SkipZigTest;

    const argv = selftest.buildProbeArgv(io, alloc, baseCfg(), WORKSPACE) catch |err| {
        try std.testing.expectEqual(error.BwrapUnavailable, err);
        return error.SkipZigTest;
    };
    defer sandbox_args.freeArgv(alloc, argv);

    var buf: [128]u8 = undefined;
    const line = try runProbeArgv(io, argv, &buf);
    const o = selftest_exec.outcomeFrom(line, false);
    try std.testing.expect(o.resolver_readable);
}

test "the scratch check passes in an unmodified sandbox" {
    // Dimension 1.3 — the write floor is DETECTED, not derived twice: the
    // probe runs under the full lease hardening (bwrap + landlock + seccomp)
    // and creates a real file in its private tmpfs. Before the shared write
    // floor this exact probe failed, which is how TempFileCreateFailed shipped.
    if (builtin.os.tag != .linux) return error.SkipZigTest;
    const alloc = std.testing.allocator;
    var threaded: std.Io.Threaded = undefined;
    const io = spawnIo(&threaded);
    defer threaded.deinit();
    try makeWorkspace(io);
    if (!try probeRanHere(io, alloc)) return error.SkipZigTest;

    const argv = selftest.buildProbeArgv(io, alloc, baseCfg(), WORKSPACE) catch |err| {
        try std.testing.expectEqual(error.BwrapUnavailable, err);
        return error.SkipZigTest;
    };
    defer sandbox_args.freeArgv(alloc, argv);

    var buf: [128]u8 = undefined;
    const line = try runProbeArgv(io, argv, &buf);
    const o = selftest_exec.outcomeFrom(line, false);
    try std.testing.expect(o.scratch_writable);
}

test "test_probe_reports_deny_all_as_expected" {
    // Dimension 2.3 — under `deny_all_egress` the sandbox has no network by
    // assignment, so an unreachable endpoint is the assignment WORKING. Graded
    // a fault, every correctly locked-down runner reads unhealthy, the alert
    // gets muted, and it is not there when the sandbox really breaks.
    if (builtin.os.tag != .linux) return error.SkipZigTest;
    const alloc = std.testing.allocator;
    var threaded: std.Io.Threaded = undefined;
    const io = spawnIo(&threaded);
    defer threaded.deinit();
    try makeWorkspace(io);

    if (!try probeRanHere(io, alloc)) return error.SkipZigTest;

    var cfg = baseCfg();
    cfg.network_policy = .deny_all_egress;

    const r = selftest_exec.run(io, alloc, cfg, WORKSPACE) catch |err| {
        try std.testing.expectEqual(error.BwrapUnavailable, err);
        return error.SkipZigTest;
    };
    defer r.deinit(alloc);

    // The resolver file is still bound under deny_all — the mount namespace is
    // not the network namespace, which is the whole reason a network policy
    // could not have prevented the M167 outage.
    var saw_resolver = false;
    for (r.checks) |c| {
        if (std.mem.eql(u8, c.name, selftest.CHECK_RESOLVER)) {
            saw_resolver = true;
            try std.testing.expect(c.ok);
        }
        if (std.mem.eql(u8, c.name, selftest.CHECK_EGRESS)) {
            try std.testing.expect(c.ok);
            try std.testing.expectEqualStrings(selftest.DETAIL_EGRESS_DENIED_EXPECTED, c.detail);
        }
        if (std.mem.eql(u8, c.name, selftest.CHECK_DNS)) {
            try std.testing.expect(c.ok);
            try std.testing.expectEqualStrings(selftest.DETAIL_DNS_NO_NETWORK, c.detail);
        }
    }
    try std.testing.expect(saw_resolver);
    // A correctly configured deny_all runner is HEALTHY, not merely tolerated.
    try std.testing.expect(r.allOk());
}

test "a completed probe leaves no process behind" {
    // Dimension 2.4's orphan half (the timeout VERDICT is graded in
    // `selftest_test.zig`, which can force the reap deterministically). A probe
    // runs on the heartbeat path, so a leaked bwrap per operator click would
    // accumulate silently on a long-lived daemon.
    if (builtin.os.tag != .linux) return error.SkipZigTest;
    const alloc = std.testing.allocator;
    var threaded: std.Io.Threaded = undefined;
    const io = spawnIo(&threaded);
    defer threaded.deinit();
    try makeWorkspace(io);

    if (!try probeRanHere(io, alloc)) return error.SkipZigTest;

    const before = try childCount(io, alloc);
    const r = selftest_exec.run(io, alloc, baseCfg(), WORKSPACE) catch |err| {
        try std.testing.expectEqual(error.BwrapUnavailable, err);
        return error.SkipZigTest;
    };
    r.deinit(alloc);
    try std.testing.expectEqual(before, try childCount(io, alloc));
}

/// How many children this process currently has. Read from `/proc/self/task`'s
/// children lists — cheap, and it counts un-reaped zombies, which is what a
/// leaked probe would leave.
fn childCount(io: std.Io, alloc: std.mem.Allocator) !usize {
    var dir = try std.Io.Dir.openDirAbsolute(io, "/proc/self/task", .{ .iterate = true });
    defer dir.close(io);
    var total: usize = 0;
    var it = dir.iterate();
    while (try it.next(io)) |entry| {
        if (entry.kind != .directory) continue;
        var path_buf: [64]u8 = undefined;
        const path = try std.fmt.bufPrint(&path_buf, "/proc/self/task/{s}/children", .{entry.name});
        const text = std.Io.Dir.cwd().readFileAlloc(io, path, alloc, .limited(4096)) catch continue;
        defer alloc.free(text);
        var fields = std.mem.tokenizeAny(u8, text, " \n");
        while (fields.next()) |_| total += 1;
    }
    return total;
}
