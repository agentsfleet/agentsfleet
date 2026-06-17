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
//! (the `make test-integration-agentsfleet-runner` lane).

const std = @import("std");
const builtin = @import("builtin");
const common = @import("common");
const clock = common.clock;

const child_process = @import("child_process.zig");
const child_supervisor = @import("child_supervisor.zig");
const cgroup = @import("engine/CgroupScope.zig");
const pipe_proto = @import("pipe_proto.zig");

const SH = "/bin/sh";
const PLANTED_TOKEN = "agt_rplanted_probe_value";
const WAIT_BUDGET_MS = 5_000;

// Worker-pool concurrency proofs share the integration lane (real forked
// children; the pool forks the executor_provider_stub runner per lease).
test {
    _ = @import("worker_pool_integration_test.zig");
    _ = @import("network/egress_integration_test.zig");
}

/// Read up to `cap` bytes from `fd` until EOF (the child's output is small).
fn readToEnd(alloc: std.mem.Allocator, fd: std.posix.fd_t, cap: usize) ![]u8 {
    // BUFFER GATE: ArrayList(u8) — read-to-EOF, size unknown up front, need one
    // contiguous slice for std.mem.indexOf below.
    var buf: std.ArrayList(u8) = .empty;
    errdefer buf.deinit(alloc);
    var chunk: [4096]u8 = undefined;
    while (buf.items.len < cap) {
        const n = try std.posix.read(fd, &chunk); // surface I/O errors, not a silent truncation
        if (n == 0) break; // EOF
        try buf.appendSlice(alloc, chunk[0..n]);
    }
    return buf.toOwnedSlice(alloc);
}

/// A spawn-capable IO. `common.globalIo()` carries a `.failing` allocator — fine
/// for the sync/sleep seam, but `std.process.spawn` allocates the argv/envp block
/// before fork, so it must reach a real gpa or every spawn errors. Production uses
/// the process `Init.io`; tests stand up their own. Caller owns `t`, must `deinit`.
fn spawnIo(t: *std.Io.Threaded) std.Io {
    t.* = .init(std.testing.allocator, .{});
    return t.io();
}

test "a planted daemon token never reaches a real spawned child's environment" {
    if (builtin.os.tag != .linux) return error.SkipZigTest;
    const alloc = std.testing.allocator;
    var threaded: std.Io.Threaded = undefined;
    const io = spawnIo(&threaded);
    defer threaded.deinit();

    // A daemon environ carrying the control-plane secret plus the load-bearing
    // allowlisted vars.
    var daemon: std.process.Environ.Map = .init(alloc);
    defer daemon.deinit();
    try daemon.put("AGENTSFLEET_RUNNER_TOKEN", PLANTED_TOKEN);
    try daemon.put("HOME", "/home/agentsfleet-runner");
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
    // HOME but never the planted token nor its AGENTSFLEET_ key.
    try std.testing.expect(std.mem.indexOf(u8, dump, PLANTED_TOKEN) == null);
    try std.testing.expect(std.mem.indexOf(u8, dump, "AGENTSFLEET_RUNNER_TOKEN") == null);
    try std.testing.expect(std.mem.indexOf(u8, dump, "HOME=/home/agentsfleet-runner") != null);
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
    // Drain the whole "ready\n" marker — POSIX read may deliver it in pieces;
    // residual bytes would later masquerade as non-EOF in expectReapedViaEof.
    const MARKER = "ready\n";
    var rb: [MARKER.len]u8 = undefined;
    var drained: usize = 0;
    while (drained < MARKER.len) {
        const n = try std.posix.read(child.stdout.?.handle, rb[drained..]);
        if (n == 0) break; // unexpected EOF before the full marker
        drained += n;
    }
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
    var threaded: std.Io.Threaded = undefined;
    const io = spawnIo(&threaded);
    defer threaded.deinit();

    // sh backgrounds two sleeps (grandchildren holding the stdout pipe). Absolute
    // /bin/sleep: the child inherits this io's (empty) environ, so PATH is unset.
    var child = try spawnReadyGroup(io, "/bin/sleep 60 & /bin/sleep 60 & echo ready; wait");

    // Kill the whole group (scope=null → the pure pgroup-signal path).
    var scope: ?cgroup = null;
    child_process.killChild(child.id.?, &scope);
    try expectReapedViaEof(child.stdout.?.handle); // EOF ⟺ both sleeps reaped
    _ = child.wait(io) catch {};
}

test "enrollOrFail refuses the lease fail-closed AND kills the child on enrollment failure" {
    if (builtin.os.tag != .linux) return error.SkipZigTest;
    var threaded: std.Io.Threaded = undefined;
    const io = spawnIo(&threaded);
    defer threaded.deinit();

    var child = try spawnReadyGroup(io, "/bin/sleep 60 & echo ready; wait");

    // A scope whose cgroup dir does not exist → addProcess (open cgroup.procs)
    // fails deterministically — the exact enrollment failure the fail-closed
    // path guards, with no root or real cgroup needed.
    var scope: ?cgroup = .{
        .path = "/sys/fs/cgroup/agentsfleet-runner-no-such-scope/exec-enrolltest",
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

test "a daemon-opened marker fd does not cross exec into a real spawned child" {
    if (builtin.os.tag != .linux) return error.SkipZigTest;
    const alloc = std.testing.allocator;
    var threaded: std.Io.Threaded = undefined;
    const io = spawnIo(&threaded);
    defer threaded.deinit();

    // The daemon opens a fd the std way — O_CLOEXEC by default. /dev/null is a
    // generic credential-like marker; if the CLOEXEC default ever regressed it
    // would survive exec and appear in the child's /proc/self/fd.
    const marker = try std.Io.Dir.openFileAbsolute(io, "/dev/null", .{});
    defer marker.close(io);

    // The child proves (a) it can see its own wired stdout symlink (probe is
    // live) and (b) the parent's marker fd number is absent — CLOEXEC closed it
    // on exec. `[ -L ]` matches the /proc/self/fd symlink regardless of target
    // type and opens no fd, so the check never perturbs the table.
    const script = try std.fmt.allocPrint(alloc,
        \\if [ -L /proc/self/fd/1 ]; then P=ok; else P=broken; fi
        \\if [ -L /proc/self/fd/{d} ]; then M=LEAKED; else M=absent; fi
        \\printf '%s %s' "$P" "$M"
    , .{marker.handle});
    defer alloc.free(script);

    var child = try std.process.spawn(io, .{
        .argv = &.{ SH, "-c", script },
        .stdin = .ignore,
        .stdout = .pipe,
        .stderr = .ignore,
    });
    const out = try readToEnd(alloc, child.stdout.?.handle, 4096);
    defer alloc.free(out);
    _ = child.wait(io) catch {};

    try std.testing.expectEqualStrings("ok absent", out);
}

test "the spawn path introduces no file descriptor the parent did not already hold" {
    if (builtin.os.tag != .linux) return error.SkipZigTest;
    const alloc = std.testing.allocator;
    var threaded: std.Io.Threaded = undefined;
    const io = spawnIo(&threaded);
    defer threaded.deinit();

    // std.process.spawn (the syscall path forkExec uses) wires stdio but does
    // NOT close other inherited fds — production forkExec closes them via the
    // bwrap wrapper (sandbox_args.appendBwrap); this raw-spawn test has no
    // bwrap, so the test harness's own non-CLOEXEC fds (the zig test-runner
    // --listen pipe, the Threaded io eventfd) legitimately cross. The proof is
    // therefore RELATIVE: every fd the child holds must already be open in the
    // parent (inherited), never one the spawn path newly introduced — e.g. a
    // progress fd 3 (forkExec leaves progress_node .none). Credential CLOEXEC
    // safety is proven by the marker test above. The probe is self-validating:
    // if /proc is unreadable it prints `probe_broken`, not a vacuous clean set.
    const script =
        \\[ -L /proc/self/fd/1 ] || { printf 'probe_broken'; exit 0; }
        \\S=
        \\n=3
        \\while [ "$n" -le 63 ]; do
        \\  if [ -L /proc/self/fd/$n ]; then S="$S $n"; fi
        \\  n=$((n+1))
        \\done
        \\printf '%s' "$S"
    ;
    var child = try std.process.spawn(io, .{
        .argv = &.{ SH, "-c", script },
        .stdin = .ignore,
        .stdout = .pipe,
        .stderr = .ignore,
    });
    const out = try readToEnd(alloc, child.stdout.?.handle, 4096);
    defer alloc.free(out);
    _ = child.wait(io) catch {};

    try std.testing.expect(!std.mem.eql(u8, out, "probe_broken"));
    // Each fd the child reported must resolve to an open fd in the parent too:
    // readlink of /proc/self/fd/N succeeds (inherited) or ENOENTs (the spawn
    // path introduced an fd the parent never had — a real leak). readlink reads
    // the symlink without opening its target, so the check perturbs no fd.
    var link_buf: [256]u8 = undefined;
    var path_buf: [64]u8 = undefined;
    var it = std.mem.tokenizeScalar(u8, out, ' ');
    while (it.next()) |tok| {
        const fd = try std.fmt.parseInt(u32, tok, 10);
        const path = try std.fmt.bufPrint(&path_buf, "/proc/self/fd/{d}", .{fd});
        _ = std.Io.Dir.readLinkAbsolute(io, path, &link_buf) catch return error.SpawnIntroducedStrayFd;
    }
}
