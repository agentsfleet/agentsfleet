//! sandbox_integration_test.zig — Linux-only, real-process integration proofs
//! for the runner's process-boundary hardening. These fork ACTUAL children via
//! std.process.spawn (the same mechanism child_process.forkExec uses) to prove
//! the env-allowlist filter and the process-group kill domain hold end-to-end,
//! not just at the unit-logic layer. Skipped on non-Linux (SkipZigTest); the
//! macOS dev loop compile-checks the bodies by cross-compiling the test graph.
//!
//! These two need no bwrap/cgroup/root — they exercise the spawn-boundary env
//! filter and the kill(-pgid) reap directly, so they run on any Linux CI host.
//! The privileged proofs (NoNewPrivs:1, no controlling tty, cgroup-enrollment
//! fault) require the `__execute`-stub child harness and are authored against
//! the Linux CI runtime, not blind on macOS.
//!
//! Run on Linux: zig build --build-file build_runner.zig test-integration
//! (the `make test-integration-runner` lane).

const std = @import("std");
const builtin = @import("builtin");
const common = @import("common");
const clock = common.clock;

const child_process = @import("child_process.zig");
const child_supervisor = @import("child_supervisor.zig");
const cgroup = @import("engine/cgroup.zig");
const pipe_proto = @import("pipe_proto.zig");

const SH = "/bin/sh";
const PLANTED_TOKEN = "zrn_planted_probe_value";
const WAIT_BUDGET_MS = 5_000;

/// Read up to `cap` bytes from `fd` until EOF (the child's output is small).
fn readToEnd(alloc: std.mem.Allocator, fd: std.posix.fd_t, cap: usize) ![]u8 {
    // BUFFER GATE: ArrayList(u8) — read-to-EOF, size unknown up front, need one
    // contiguous slice for std.mem.indexOf below.
    var buf: std.ArrayList(u8) = .empty;
    errdefer buf.deinit(alloc);
    var chunk: [4096]u8 = undefined;
    while (buf.items.len < cap) {
        const n = std.posix.read(fd, &chunk) catch break;
        if (n == 0) break; // EOF
        try buf.appendSlice(alloc, chunk[0..n]);
    }
    return buf.toOwnedSlice(alloc);
}

test "a planted daemon token never reaches a real spawned child's environment" {
    if (builtin.os.tag != .linux) return error.SkipZigTest;
    const alloc = std.testing.allocator;
    const io = common.globalIo();

    // A daemon environ carrying the control-plane secret plus the load-bearing
    // allowlisted vars.
    var daemon: std.process.Environ.Map = .init(alloc);
    defer daemon.deinit();
    try daemon.put("ZOMBIE_RUNNER_TOKEN", PLANTED_TOKEN);
    try daemon.put("HOME", "/home/zombie-runner");
    try daemon.put("PATH", "/usr/bin:/bin");

    // forkExec's REAL filter → a child that dumps its own environ.
    var child_env = try child_process.buildChildEnviron(alloc, &daemon);
    defer child_env.deinit();

    var child = try std.process.spawn(io, .{
        .argv = &.{ SH, "-c", "cat /proc/self/environ" },
        .environ_map = &child_env,
        .stdin = .ignore,
        .stdout = .pipe,
        .stderr = .ignore,
    });
    const dump = try readToEnd(alloc, child.stdout.?.handle, 64 * 1024);
    defer alloc.free(dump);
    _ = child.wait(io) catch {};

    // The agent's real read path (cat /proc/self/environ) shows the allowlisted
    // HOME but never the planted token nor its ZOMBIE_ key.
    try std.testing.expect(std.mem.indexOf(u8, dump, PLANTED_TOKEN) == null);
    try std.testing.expect(std.mem.indexOf(u8, dump, "ZOMBIE_RUNNER_TOKEN") == null);
    try std.testing.expect(std.mem.indexOf(u8, dump, "HOME=/home/zombie-runner") != null);
}

/// Spawn `sh -c <script>` as its own process-group leader (pgid=0) with a piped
/// stdout, blocking until it prints `ready` — the script must `echo ready` once
/// its backgrounded sleeps are running and holding that pipe. Returns the Child.
fn spawnReadyGroup(io: std.Io, script: []const u8) !std.process.Child {
    var child = try std.process.spawn(io, .{
        .argv = &.{ SH, "-c", script },
        .stdin = .ignore,
        .stdout = .pipe,
        .stderr = .ignore,
        .pgid = 0,
    });
    errdefer if (child.wait(io)) |_| {} else |_| {};
    if ((try pipe_proto.waitReadable(child.stdout.?.handle, clock.nowMillis() + WAIT_BUDGET_MS)) != .readable)
        return error.ChildNeverReady;
    var rb: [16]u8 = undefined;
    _ = try std.posix.read(child.stdout.?.handle, &rb);
    return child;
}

/// Assert the child's stdout pipe reaches EOF within the budget — the sleeps
/// held it open, so EOF ⟺ every descendant was reaped. A survivor keeps the
/// write end open and the wait times out.
fn expectReapedViaEof(fd: std.posix.fd_t) !void {
    switch (try pipe_proto.waitReadable(fd, clock.nowMillis() + WAIT_BUDGET_MS)) {
        .readable => {
            var rb: [16]u8 = undefined;
            try std.testing.expectEqual(@as(usize, 0), try std.posix.read(fd, &rb));
        },
        .timed_out => return error.DescendantsSurvivedKill,
    }
}

test "killChild reaps a forking child's whole process-group tree" {
    if (builtin.os.tag != .linux) return error.SkipZigTest;
    const io = common.globalIo();

    // sh backgrounds two sleeps (grandchildren holding the stdout pipe).
    var child = try spawnReadyGroup(io, "sleep 60 & sleep 60 & echo ready; wait");

    // Kill the whole group (scope=null → the pure pgroup-signal path).
    var scope: ?cgroup.CgroupScope = null;
    child_process.killChild(child.id.?, &scope);
    try expectReapedViaEof(child.stdout.?.handle); // EOF ⟺ both sleeps reaped
    _ = child.wait(io) catch {};
}

test "enrollOrFail refuses the lease fail-closed AND kills the child on enrollment failure" {
    if (builtin.os.tag != .linux) return error.SkipZigTest;
    const io = common.globalIo();

    var child = try spawnReadyGroup(io, "sleep 60 & echo ready; wait");

    // A scope whose cgroup dir does not exist → addProcess (open cgroup.procs)
    // fails deterministically — the exact enrollment failure the fail-closed
    // path guards, with no root or real cgroup needed.
    var scope: ?cgroup.CgroupScope = .{
        .path = "/sys/fs/cgroup/zombie-runner-no-such-scope/exec-enrolltest",
        .alloc = std.testing.allocator,
        .io = io,
    };

    // Fix A: the lease is refused (error) AND the child is killed — routed
    // through the Fix-B killChild, since scope.kill no-ops on the missing cgroup.
    try std.testing.expectError(
        error.CgroupEnrollFailed,
        child_supervisor.enrollOrFail(&scope, child.id.?, "enroll-test-lease"),
    );
    try expectReapedViaEof(child.stdout.?.handle); // not a silently-empty kill domain
    _ = child.wait(io) catch {};
}
