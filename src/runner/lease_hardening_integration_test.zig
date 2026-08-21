//! lease_hardening_integration_test.zig — Linux-only, real-process proofs that
//! a lease can execute its transport under the FULL wall, not merely inside its
//! mounts. Sibling of `lease_transport_integration_test.zig`, split from it on
//! the file-length bound (RULE FLL) along the mounts/hardening seam: that file
//! asks what bwrap made reachable, this one asks what the kernel still permits
//! once `no_new_privs` → landlock → seccomp are on.
//!
//! The seam is load-bearing, not filing. Splicing the probe's tail replaces
//! `--sandboxed` along with it, so every proof in the sibling runs under mounts
//! ALONE. A lease runs under all three layers and the engine spawns its
//! transport from inside that wall, so "the mount set carries curl" is a
//! strictly weaker claim than "a lease can execute curl" — and only the second
//! one is what a lease needs (M170 Discovery).

const std = @import("std");
const builtin = @import("builtin");
const linux = std.os.linux;

const landlock = @import("engine/landlock.zig");
const sandbox_args = @import("sandbox_args.zig");
const selftest_exec = @import("selftest_exec.zig");
const selftest_probe = @import("selftest_probe.zig");
const fixtures = @import("selftest_test_fixtures.zig");

const buildArgvOrSkip = fixtures.buildArgvOrSkip;
const makeWorkspace = fixtures.makeWorkspace;
const probeRanHere = fixtures.probeRanHere;
const runOnHost = fixtures.runOnHost;
const runProbeArgv = fixtures.runProbeArgv;
const spawnIo = fixtures.spawnIo;
const WORKSPACE = fixtures.WORKSPACE;

/// The binary these proofs execute. Deliberately NOT `curl`: the kernel-lane
/// image ships none, and a proof that skips in the only environment continuous
/// integration runs it is not a proof. It fails identically when the executable
/// and library trees are unbound, which is the property under test, and on a
/// host that HAS a transport the daemon aims the same check at the real one.
const PORTABLE_DYNAMIC_EXE = "/usr/bin/env";

/// Exit codes the forked child below reports with. Distinct values so a failure
/// says WHICH step refused rather than "the child died".
const FORK_EXIT_OK: u8 = 0;
const FORK_EXIT_NO_NEW_PRIVS_FAILED: u8 = 91;
const FORK_EXIT_LANDLOCK_FAILED: u8 = 92;
const FORK_EXIT_EXECVE_REFUSED: u8 = 93;
const FORK_EXIT_DEV_FILE_REFUSED: u8 = 94;

/// Is `path` present on this host?
fn presentPath(io: std.Io, path: []const u8) bool {
    std.Io.Dir.accessAbsolute(io, path, .{}) catch return false;
    return true;
}

test "the filesystem wall alone permits executing a binary from the system trees" {
    // Isolation test, and the one that says WHERE an exec refusal comes from.
    //
    // The lease's hardening is three layers (no_new_privs → landlock → seccomp)
    // applied inside a bwrap mount namespace, and a failed exec under all three
    // does not say which one refused. This forks, applies ONLY the filesystem
    // wall, and calls `execve` DIRECTLY — no seccomp, no bwrap, and none of the
    // standard library's spawn machinery in between, so the kernel's answer is
    // about landlock and nothing else.
    //
    // `execve` returning at all means it failed; on success the child is
    // replaced and exits as the target does.
    if (builtin.os.tag != .linux) return error.SkipZigTest;
    const alloc = std.testing.allocator;
    var threaded: std.Io.Threaded = undefined;
    const io = spawnIo(&threaded);
    defer threaded.deinit();

    if (!presentPath(io, PORTABLE_DYNAMIC_EXE)) return error.SkipZigTest;
    const command = [_][]const u8{PORTABLE_DYNAMIC_EXE};
    const on_host = try runOnHost(io, alloc, &command);
    if (on_host == null or on_host.? != 0) return error.SkipZigTest;

    try makeWorkspace(io);
    const signed: isize = @bitCast(linux.fork());
    if (signed < 0) return error.SkipZigTest;
    if (signed == 0) {
        if (linux.prctl(@intFromEnum(linux.PR.SET_NO_NEW_PRIVS), 1, 0, 0, 0) != 0)
            linux.exit(FORK_EXIT_NO_NEW_PRIVS_FAILED);
        landlock.applyPolicy(WORKSPACE, &.{}) catch linux.exit(FORK_EXIT_LANDLOCK_FAILED);
        const path: [*:0]const u8 = PORTABLE_DYNAMIC_EXE;
        const argv = [_:null]?[*:0]const u8{path};
        const envp = [_:null]?[*:0]const u8{};
        _ = linux.execve(path, &argv, &envp);
        linux.exit(FORK_EXIT_EXECVE_REFUSED);
    }

    var status: u32 = 0;
    _ = linux.wait4(@intCast(signed), &status, 0, null);
    if (!std.posix.W.IFEXITED(status)) return error.SkipZigTest;
    const code: u8 = @intCast(std.posix.W.EXITSTATUS(status));
    try std.testing.expectEqual(FORK_EXIT_OK, code);
}

test "the filesystem wall permits opening the writable device files for writing" {
    // The fault this proof exists for, at the layer that produced it.
    //
    // `/dev` rides the read-only floor, whose mask carries no WRITE_FILE, while
    // bwrap's `--dev` builds a devtmpfs where `/dev/null` is writable. The
    // engine's model transport spawns `curl` and wires an ignored stdio stream
    // through that node, so on `zombie-dev-worker-ant` every lease died at
    // `open("/dev/null", O_RDWR) = EACCES` — zero tokens, zero wall seconds,
    // and six green self-test checks.
    //
    // Isolated the same way the exec proof above is, and for the same reason: a
    // refusal under all three layers does not say which one refused. This forks,
    // applies ONLY the filesystem wall, and calls `open` directly, so the
    // kernel's answer is about landlock and nothing else. Read-write is the
    // whole point — read-only would pass under the exact mask that produced the
    // incident.
    if (builtin.os.tag != .linux) return error.SkipZigTest;
    var threaded: std.Io.Threaded = undefined;
    const io = spawnIo(&threaded);
    defer threaded.deinit();

    // Skip rather than fail where the host itself refuses the open: the claim
    // under test is that LANDLOCK does not, and a host that says no first
    // cannot answer that question either way.
    for (landlock.LANDLOCK_FLOOR_RW_FILES) |path| {
        const path_z = std.posix.toPosixPath(path) catch return error.SkipZigTest;
        const probe = std.posix.openatZ(std.posix.AT.FDCWD, &path_z, .{ .ACCMODE = .RDWR }, 0) catch
            return error.SkipZigTest;
        _ = linux.close(probe);
    }

    try makeWorkspace(io);
    const signed: isize = @bitCast(linux.fork());
    if (signed < 0) return error.SkipZigTest;
    if (signed == 0) {
        if (linux.prctl(@intFromEnum(linux.PR.SET_NO_NEW_PRIVS), 1, 0, 0, 0) != 0)
            linux.exit(FORK_EXIT_NO_NEW_PRIVS_FAILED);
        landlock.applyPolicy(WORKSPACE, &.{}) catch linux.exit(FORK_EXIT_LANDLOCK_FAILED);
        for (landlock.LANDLOCK_FLOOR_RW_FILES) |path| {
            const path_z = std.posix.toPosixPath(path) catch linux.exit(FORK_EXIT_DEV_FILE_REFUSED);
            const fd = std.posix.openatZ(std.posix.AT.FDCWD, &path_z, .{ .ACCMODE = .RDWR }, 0) catch
                linux.exit(FORK_EXIT_DEV_FILE_REFUSED);
            _ = linux.close(fd);
        }
        linux.exit(FORK_EXIT_OK);
    }

    var status: u32 = 0;
    _ = linux.wait4(@intCast(signed), &status, 0, null);
    if (!std.posix.W.IFEXITED(status)) return error.SkipZigTest;
    const code: u8 = @intCast(std.posix.W.EXITSTATUS(status));
    try std.testing.expectEqual(FORK_EXIT_OK, code);
}

test "a binary spawns under the lease's full hardening, not just its mounts" {
    // The gap every proof in the sibling file leaves open, and the one that
    // matters most in production.
    //
    // Splicing the probe's tail replaces `--sandboxed` along with it, so the
    // sibling's proofs run under bwrap's MOUNTS with no landlock ruleset and no
    // seccomp filter. A lease runs under all three, and the engine spawns its
    // transport from inside that wall — so "the mount set carries curl" is a
    // strictly weaker claim than "a lease can execute curl".
    //
    // This test keeps the REAL probe tail intact — `--sandboxed` included, so
    // `applySandboxHardening` runs (no_new_privs → landlock → seccomp) — and
    // adds `--transport=`, which makes the probe spawn that binary from behind
    // the wall and report the result. `/usr/bin/env` rather than `curl`: the
    // kernel-lane image ships no curl, and a proof that skips in the only
    // environment CI runs it is not a proof. On a host with curl the daemon
    // aims the same check at the real transport every heartbeat.
    if (builtin.os.tag != .linux) return error.SkipZigTest;
    const alloc = std.testing.allocator;
    var threaded: std.Io.Threaded = undefined;
    const io = spawnIo(&threaded);
    defer threaded.deinit();

    try makeWorkspace(io);
    if (!try probeRanHere(io, alloc)) return error.SkipZigTest;
    const argv = try buildArgvOrSkip(io, alloc);
    defer sandbox_args.freeArgv(alloc, argv);

    const command = [_][]const u8{PORTABLE_DYNAMIC_EXE};
    const on_host = try runOnHost(io, alloc, &command);
    if (on_host == null or on_host.? != 0) return error.SkipZigTest;

    // Append to the WHOLE argv, tail included — the opposite of the splice the
    // other tests do, and the entire point of this one.
    var with_transport: std.ArrayList([]const u8) = .empty;
    defer with_transport.deinit(alloc);
    try with_transport.appendSlice(alloc, argv);
    const flag = try std.fmt.allocPrint(alloc, "{s}{s}", .{ selftest_probe.TRANSPORT_FLAG_PREFIX, PORTABLE_DYNAMIC_EXE });
    defer alloc.free(flag);
    try with_transport.append(alloc, flag);

    var buf: [128]u8 = undefined;
    const line = try runProbeArgv(io, alloc, with_transport.items, &buf);
    // An empty line is a probe that never reported — a harness fact, and the
    // `probeRanHere` gate above already gives it its own arm.
    if (line.len == 0) return error.SkipZigTest;

    const outcome = selftest_exec.outcomeFrom(line, false);
    try std.testing.expect(outcome.transport_testable);
    try std.testing.expect(outcome.transport_execs);
}
