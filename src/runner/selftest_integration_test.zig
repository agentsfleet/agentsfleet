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

const sandbox_args = @import("sandbox_args.zig");
const selftest = @import("selftest.zig");
const selftest_exec = @import("selftest_exec.zig");
const fixtures = @import("selftest_test_fixtures.zig");

const WORKSPACE = fixtures.WORKSPACE;
const baseCfg = fixtures.baseCfg;
const spawnIo = fixtures.spawnIo;
const makeWorkspace = fixtures.makeWorkspace;
const probeTailIndex = fixtures.probeTailIndex;
const probeRanHere = fixtures.probeRanHere;
const buildArgvOrSkip = fixtures.buildArgvOrSkip;
const runProbeArgv = fixtures.runProbeArgv;
const ensureResolverTarget = fixtures.ensureResolverTarget;

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

    const argv = try buildArgvOrSkip(io, alloc);
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
    const line = try runProbeArgv(io, alloc, broken.items, &buf);
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
    // A host with neither the systemd-resolved layout nor the privilege to
    // fake it (root in the kernel-lane container) proves nothing here.
    const resolver = ensureResolverTarget(io) orelse return error.SkipZigTest;
    defer resolver.deinit(io);

    const argv = try buildArgvOrSkip(io, alloc);
    defer sandbox_args.freeArgv(alloc, argv);

    var buf: [128]u8 = undefined;
    const line = try runProbeArgv(io, alloc, argv, &buf);
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

    const argv = try buildArgvOrSkip(io, alloc);
    defer sandbox_args.freeArgv(alloc, argv);

    var buf: [128]u8 = undefined;
    const line = try runProbeArgv(io, alloc, argv, &buf);
    const o = selftest_exec.outcomeFrom(line, false);
    try std.testing.expect(o.scratch_writable);
    // The M136 fix, proven against a real bubblewrap sandbox under the lease
    // child's own hardening rather than against the lists agreeing: the HOME the
    // child is actually handed accepts a write. This is the assertion that would
    // have failed for the whole week every dev lease died at AccessDenied.
    try std.testing.expect(o.home_writable);
}

test "the daemon's credentials are unreachable inside a real lease sandbox" {
    // M170 §3's SURVIVING claim, pinned. The executable trees came back — the
    // engine's model transport spawns `curl`, so a lease without `/usr` and
    // `/lib` dies at execvp before its first model call — but the two trees
    // that carried CREDENTIALS stayed out, and that was always the real
    // exposure: `/opt` holds the daemon's control-plane token in its `.env`,
    // `/etc` holds the host account database. Neither has a lease-side
    // consumer, so a prompt-injected agent reaching either is pure loss.
    //
    // Proven by READING, from inside a real lease, rather than by inspecting
    // the bind list: the failure class this milestone kept hitting is the
    // composed argv and the list disagreeing, so a test that re-derives the
    // answer from the same list proves nothing. Each path is confirmed
    // readable on the HOST first — otherwise "cat failed" means "file absent"
    // and the test passes vacuously on any box that lacks it.
    if (builtin.os.tag != .linux) return error.SkipZigTest;
    const alloc = std.testing.allocator;
    var threaded: std.Io.Threaded = undefined;
    const io = spawnIo(&threaded);
    defer threaded.deinit();
    try makeWorkspace(io);
    if (!try probeRanHere(io, alloc)) return error.SkipZigTest;

    const argv = try buildArgvOrSkip(io, alloc);
    defer sandbox_args.freeArgv(alloc, argv);

    // Keep every bind, tmpfs, and the resolver link byte-identical to a real
    // lease, and replace the probe's whole tail with a read of the secret.
    // Composed from `probeTailIndex`, not by patching the last element: the
    // tail ends in `--workspace=…`, so the first version of this test
    // overwrote an argument, ran the REAL probe, and passed on the probe's own
    // failure — it asserted nothing.
    const tail = probeTailIndex(argv) orelse return error.NoProbeTail;
    const secrets = [_][]const u8{
        // The daemon's control-plane token lives in this file.
        "/opt/agentsfleet/.env",
        // The host account database.
        "/etc/shadow",
    };
    // A run where every arm took a harness exit must not read as proof — the
    // same vacuous green `probeRanHere` exists to prevent, one layer up.
    var proven: usize = 0;
    for (secrets) |path| {
        // Readable from HERE, outside the sandbox? If not, a failed read
        // inside proves nothing about the bind set.
        std.Io.Dir.accessAbsolute(io, path, .{}) catch continue;

        var attempt: std.ArrayList([]const u8) = .empty;
        defer attempt.deinit(alloc);
        try attempt.appendSlice(alloc, argv[0..tail]);
        try attempt.appendSlice(alloc, &.{ "/bin/cat", path });

        var child = std.process.spawn(io, .{
            .argv = attempt.items,
            .stdin = .ignore,
            .stdout = .ignore,
            .stderr = .ignore,
            .pgid = 0,
        }) catch continue; // bwrap itself would not start — a harness fact
        const term = child.wait(io) catch continue;
        switch (term) {
            // `cat` exits non-zero when the path is absent from the lease's
            // mount namespace. A ZERO exit means the lease read the host's
            // secret, which is the regression this pins.
            .exited => |code| {
                try std.testing.expect(code != 0);
                proven += 1;
            },
            // Signalled: the sandbox never reported on the read, so this arm
            // proves nothing either way.
            else => {},
        }
    }
    // Nothing was both host-readable and cleanly observed → report the
    // harness, do not claim the property.
    if (proven == 0) return error.SkipZigTest;
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
    // `allOk` below includes the resolver row, so this test needs the layout
    // (real or faked) just like the unmodified-sandbox control above.
    const resolver = ensureResolverTarget(io) orelse return error.SkipZigTest;
    defer resolver.deinit(io);

    var cfg = baseCfg();
    cfg.network_policy = .deny_all_egress;

    var daemon_env: std.process.Environ.Map = .init(alloc);
    defer daemon_env.deinit();
    const r = selftest_exec.run(io, alloc, cfg, &daemon_env, WORKSPACE) catch |err| {
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
    // A correctly configured deny_all runner is HEALTHY, not merely tolerated —
    // on a host that HAS the transport the engine spawns. Where there is none
    // (the kernel-lane image ships no `curl`) the transport row is CORRECTLY
    // red: no lease on that host could reach a model, and `allOk` papering over
    // it would be the green-probe/dead-runner reading this milestone removes.
    // Gated rather than dropped, so the strong claim still runs wherever a
    // transport exists — including the deploy target and any Debian-family CI.
    if (selftest.transportPath(io) != null) try std.testing.expect(r.allOk());
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
    var daemon_env: std.process.Environ.Map = .init(alloc);
    defer daemon_env.deinit();
    const r = selftest_exec.run(io, alloc, baseCfg(), &daemon_env, WORKSPACE) catch |err| {
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
