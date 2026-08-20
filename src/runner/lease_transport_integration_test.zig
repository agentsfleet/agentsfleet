//! lease_transport_integration_test.zig — Linux-only, real-process proofs that
//! a lease can REACH what the engine needs to run. The mirror image of
//! `selftest_integration_test.zig`'s proof that it cannot reach the daemon's
//! credentials: that file pins what stayed OUT, this one pins what came back.
//!
//! M170 §3 narrowed the lease sandbox on the premise that "no executable is
//! needed inside a lease at all". The premise was false — the NullClaw engine's
//! model transport spawns `curl` — and the whole suite stayed green anyway,
//! because the evidence offered for the premise was the self-test's egress row:
//! a TCP connect issued from inside the STATICALLY LINKED runner, which spawns
//! nothing and so measured the one path in the system that needs no executable.
//! The claim and the measurement never touched (M136/M170 Discovery).
//!
//! Every test here composes the REAL lease argv and runs a command inside it.
//! A bind list that agrees with its own mirror while disagreeing with the
//! kernel cannot pass one of them.

const std = @import("std");
const builtin = @import("builtin");

const sandbox_args = @import("sandbox_args.zig");
const fixtures = @import("selftest_test_fixtures.zig");

const buildArgvOrSkip = fixtures.buildArgvOrSkip;
const makeWorkspace = fixtures.makeWorkspace;
const probeRanHere = fixtures.probeRanHere;
const probeTailIndex = fixtures.probeTailIndex;
const runInLease = fixtures.runInLease;
const runOnHost = fixtures.runOnHost;
const spawnIo = fixtures.spawnIo;

/// A dynamically linked executable every supported host carries, living in the
/// trees the narrowing removed.
///
/// Deliberately NOT `curl`, even though `curl` is the binary that actually
/// matters: the kernel-lane image (`ci-zig-alpine`) ships none, so a
/// curl-only proof would SkipZigTest in the one environment continuous
/// integration runs this lane in — the silent-skip reading that already cost
/// this suite six proofs. `curl` gets its own test below, gated and loud.
///
/// Alpine reaches this path through a busybox symlink into `/bin`, the
/// Debian family as a real file under `/usr/bin`. Both resolve only when the
/// executable AND library trees are bound, which is the property under test.
const PORTABLE_DYNAMIC_EXE = "/usr/bin/env";

/// Where the engine's model transport lives, in the two locations a host puts
/// it. Absolute rather than PATH-resolved: the lease's `PATH` is part of what
/// is under test, and a proof that depends on it cannot report on it.
const TRANSPORT_EXE_CANDIDATES = [_][]const u8{
    "/usr/bin/curl",
    "/bin/curl",
};

/// The trust store as a FILE, not the directory the baseline binds. Reading it
/// is what proves the bind carried its contents: on the Debian family
/// `/etc/ssl/certs` is mostly symlinks into `/usr/share/ca-certificates`, a
/// tree that was itself unbound until the executable trees came back.
const TLS_BUNDLE = "/etc/ssl/certs/ca-certificates.crt";

/// `cat`, for reading a host path from inside a lease.
const CAT_EXE = "/bin/cat";

/// The executable and library trees M170 §3 removed, restated here rather than
/// imported from `BASELINE_RO_PATHS`.
///
/// The duplication is the point: this list is what the negative control strips,
/// and a list that moved with the baseline would follow it straight into the
/// next narrowing instead of catching it. If these ever stop being the trees a
/// transport needs, that is a deliberate edit in two places, not a silent one.
const SYSTEM_CORE_TREES = [_][]const u8{ "/usr", "/lib", "/lib64", "/bin", "/sbin" };

/// Every bwrap bind emits as `<flag> <src> <dst>`, so dropping one means
/// dropping three argv elements. Returns a slice of BORROWED strings — free the
/// outer slice only; `argv` still owns the contents.
fn withoutSystemCoreBinds(alloc: std.mem.Allocator, argv: []const []const u8) ![]const []const u8 {
    var kept: std.ArrayList([]const u8) = .empty;
    errdefer kept.deinit(alloc);
    var i: usize = 0;
    while (i < argv.len) {
        const is_bind_triple = i + 2 < argv.len and
            std.mem.startsWith(u8, argv[i], "--ro-bind") and
            std.mem.eql(u8, argv[i + 1], argv[i + 2]);
        if (is_bind_triple and isSystemCoreTree(argv[i + 1])) {
            i += 3;
            continue;
        }
        try kept.append(alloc, argv[i]);
        i += 1;
    }
    return kept.toOwnedSlice(alloc);
}

fn isSystemCoreTree(path: []const u8) bool {
    for (SYSTEM_CORE_TREES) |tree| {
        if (std.mem.eql(u8, tree, path)) return true;
    }
    return false;
}

/// Everything every test here needs: a real lease argv and the index its own
/// tail starts at. `openLease` returns `error.SkipZigTest` on a host that
/// cannot produce one — no bubblewrap, or a probe that did not execute here —
/// because that is a harness fact rather than a verdict on the bind set.
const Lease = struct {
    argv: []const []const u8,
    tail: usize,

    fn deinit(self: Lease, alloc: std.mem.Allocator) void {
        sandbox_args.freeArgv(alloc, self.argv);
    }
};

fn openLease(io: std.Io, alloc: std.mem.Allocator) !Lease {
    try makeWorkspace(io);
    if (!try probeRanHere(io, alloc)) return error.SkipZigTest;
    const argv = try buildArgvOrSkip(io, alloc);
    errdefer sandbox_args.freeArgv(alloc, argv);
    const tail = probeTailIndex(argv) orelse return error.NoProbeTail;
    return .{ .argv = argv, .tail = tail };
}

/// The first candidate this host actually has, or `null`.
fn presentPath(io: std.Io, candidates: []const []const u8) ?[]const u8 {
    for (candidates) |p| {
        std.Io.Dir.accessAbsolute(io, p, .{}) catch continue;
        return p;
    }
    return null;
}

test "a dynamically linked executable runs inside a real lease sandbox" {
    // THE regression pin for M170 §3. With `/usr`, `/lib` and `/bin` unbound
    // this exits non-zero — the loader is not there to run — which is exactly
    // how every lease died at `execvp` before its first model call. Nothing
    // else in the suite fails when those trees go, because everything else
    // asks the bind list rather than the kernel.
    //
    // Runs on every Linux host: no network, no `curl`, no privileged
    // capability beyond the bubblewrap the lane already needs.
    if (builtin.os.tag != .linux) return error.SkipZigTest;
    const alloc = std.testing.allocator;
    var threaded: std.Io.Threaded = undefined;
    const io = spawnIo(&threaded);
    defer threaded.deinit();

    const lease = try openLease(io, alloc);
    defer lease.deinit(alloc);

    const command = [_][]const u8{PORTABLE_DYNAMIC_EXE};
    // Control arm: a host that cannot run this outside a sandbox says nothing
    // about the bind set, and grading it would be the vacuous pass this file
    // exists to remove.
    const on_host = try runOnHost(io, alloc, &command);
    if (on_host == null or on_host.? != 0) return error.SkipZigTest;

    const in_lease = try runInLease(io, alloc, lease.argv, lease.tail, &command);
    // `null` is bwrap failing to start or a signalled child — neither is a
    // verdict on the bind set.
    const code = in_lease orelse return error.SkipZigTest;
    try std.testing.expectEqual(@as(u8, 0), code);
}

test "the same executable fails in a lease stripped of the system trees" {
    // The control arm for the test above, and the reason it can be believed.
    //
    // A green "it runs inside a lease" proves nothing on its own: it stays
    // green if the command would have run anywhere, if the argv never applied,
    // or if bwrap quietly ignored the tail. This test removes ONLY the
    // system-core bind triples from the same argv, changes nothing else, and
    // requires the outcome to flip. Passing both means the bind set is what
    // decides — which is exactly the reasoning M170 §3 shipped without.
    //
    // It also reconstructs the withdrawn narrowing, so this is the shape of
    // lease every model call died in for a week.
    if (builtin.os.tag != .linux) return error.SkipZigTest;
    const alloc = std.testing.allocator;
    var threaded: std.Io.Threaded = undefined;
    const io = spawnIo(&threaded);
    defer threaded.deinit();

    const lease = try openLease(io, alloc);
    defer lease.deinit(alloc);

    const stripped = try withoutSystemCoreBinds(alloc, lease.argv);
    defer alloc.free(stripped);
    // Nothing removed means the bind spelling moved and this test is now
    // grading an unmodified lease — a silent no-op, so it fails loudly instead.
    try std.testing.expect(stripped.len < lease.argv.len);

    const command = [_][]const u8{PORTABLE_DYNAMIC_EXE};
    const tail = probeTailIndex(stripped) orelse return error.NoProbeTail;
    const in_stripped = try runInLease(io, alloc, stripped, tail, &command);
    const code = in_stripped orelse return error.SkipZigTest;
    // Non-zero is the loader failing to resolve, which is `execvp` dying — the
    // exact death every lease took under the withdrawn narrowing.
    try std.testing.expect(code != 0);
}

test "the engine's model transport is executable inside a real lease sandbox" {
    // The same property as above, measured against the ACTUAL binary the ten
    // NullClaw provider modules and the `http_request` tool spawn, rather than
    // a stand-in. `curl --version` touches loader, every one of its shared
    // libraries, and nothing else — no network, so a lane without egress still
    // grades it.
    //
    // Skips where the host has no `curl` (the kernel-lane Alpine image is one).
    // That skip is REPORTED, not silent: the test above holds the line in those
    // environments, and this one is the sharper proof wherever a transport
    // exists — the dev host and any Debian-family runner.
    if (builtin.os.tag != .linux) return error.SkipZigTest;
    const alloc = std.testing.allocator;
    var threaded: std.Io.Threaded = undefined;
    const io = spawnIo(&threaded);
    defer threaded.deinit();

    const transport = presentPath(io, &TRANSPORT_EXE_CANDIDATES) orelse return error.SkipZigTest;

    const lease = try openLease(io, alloc);
    defer lease.deinit(alloc);

    const command = [_][]const u8{ transport, "--version" };
    const on_host = try runOnHost(io, alloc, &command);
    if (on_host == null or on_host.? != 0) return error.SkipZigTest;

    const in_lease = try runInLease(io, alloc, lease.argv, lease.tail, &command);
    const code = in_lease orelse return error.SkipZigTest;
    try std.testing.expectEqual(@as(u8, 0), code);
}

test "the TLS trust store is readable inside a real lease sandbox" {
    // `/etc/ssl/certs` is bound, but binding a directory is not the same as its
    // contents resolving. On the Debian family the bundle is reached through
    // symlinks into `/usr/share/ca-certificates` — a path that was unbound for
    // the whole week M170 §3 stood, so the trust store was present and
    // unreadable, and every credentialed dial would have failed certificate
    // verification with the bind list still reading correct.
    //
    // Read from inside, with the host read as the control arm.
    if (builtin.os.tag != .linux) return error.SkipZigTest;
    const alloc = std.testing.allocator;
    var threaded: std.Io.Threaded = undefined;
    const io = spawnIo(&threaded);
    defer threaded.deinit();

    const bundle = presentPath(io, &.{TLS_BUNDLE}) orelse return error.SkipZigTest;
    if (presentPath(io, &.{CAT_EXE}) == null) return error.SkipZigTest;

    const lease = try openLease(io, alloc);
    defer lease.deinit(alloc);

    const command = [_][]const u8{ CAT_EXE, bundle };
    const on_host = try runOnHost(io, alloc, &command);
    if (on_host == null or on_host.? != 0) return error.SkipZigTest;

    const in_lease = try runInLease(io, alloc, lease.argv, lease.tail, &command);
    const code = in_lease orelse return error.SkipZigTest;
    try std.testing.expectEqual(@as(u8, 0), code);
}
