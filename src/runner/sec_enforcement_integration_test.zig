//! sec_enforcement_integration_test.zig — Linux-only, root-capable real-process
//! proofs that the in-child sandbox primitives ENFORCE, not merely install. A
//! forked child applies the *real* enforcer (the same `seccomp.applyFilter` /
//! `landlock.applyPolicy` / `CgroupScope` the `__execute` child runs), attempts a
//! concrete violation, and the parent asserts the kernel's response — a SIGSYS
//! trap exit, a denied write, a refused fork, an OOM kill. This is the layer the
//! existing `sandbox_integration_test.zig` (env-filter, kill-tree, CLOEXEC) does
//! NOT cover: that the syscall wall, the filesystem wall, and the resource cage
//! actually hold against a hostile child.
//!
//! Skipped (SkipZigTest) off-Linux and when the kernel/privilege prerequisites are
//! absent — these need a Linux host with the LSMs enabled and (for cgroup) a
//! delegated controller subtree, so CI gates them behind a privileged lane while
//! the macOS dev loop still compile-checks the bodies via cross-compile.
//!
//! Run (privileged Linux): zig build --build-file build_runner.zig test-integration

const std = @import("std");
const builtin = @import("builtin");
const linux = std.os.linux;

const seccomp = @import("engine/seccomp.zig");
const landlock = @import("engine/landlock.zig");
const pipe_proto = @import("pipe_proto.zig");
const CgroupScope = @import("engine/CgroupScope.zig");
const types = @import("engine/types.zig");
const supervisor_result = @import("child_supervisor_result.zig");

// ── child exit-code vocabulary ────────────────────────────────────────────────
// Distinct codes so the parent can tell a correct enforcement from a setup
// failure or a missed block. None collide with the child↔parent protocol codes
// (SECCOMP_VIOLATION_EXIT=79, SANDBOX_FAIL_EXIT) or a clean 0.

/// The child trapped a denied syscall — onSigsys exited it with this code.
const SECCOMP_VIOLATION_EXIT: u8 = pipe_proto.SECCOMP_VIOLATION_EXIT;
/// Every in-child sub-assertion held (the all-correct path).
const EXIT_ALL_CORRECT: u8 = 0;
/// no_new_privs or the enforcer install itself failed (a precondition, not the proof).
const EXIT_SETUP_FAILED: u8 = 91;
/// The violation was NOT blocked — the enforcement proof failed.
const EXIT_NOT_ENFORCED: u8 = 92;
/// A control action that MUST stay allowed was wrongly denied (deny-all regression).
const EXIT_CONTROL_DENIED: u8 = 93;

// ── fork / wait plumbing (Zig 0.16 removed std.posix.fork → raw linux layer) ──

fn setNoNewPrivs() bool {
    // prctl(PR_SET_NO_NEW_PRIVS, 1, …) → 0 on success. Precondition for an
    // unprivileged seccomp filter install and for landlock_restrict_self.
    return linux.prctl(@intFromEnum(linux.PR.SET_NO_NEW_PRIVS), 1, 0, 0, 0) == 0;
}

fn forkOrError() error{ForkFailed}!std.posix.pid_t {
    const signed: isize = @bitCast(linux.fork());
    if (signed < 0) return error.ForkFailed; // -errno
    return @intCast(signed); // 0 in the child, child pid in the parent
}

const ChildOutcome = union(enum) { exited: u8, signaled: u32 };

fn waitChild(pid: std.posix.pid_t) ChildOutcome {
    var status: u32 = 0;
    _ = linux.wait4(pid, &status, 0, null);
    if (std.posix.W.IFEXITED(status))
        return .{ .exited = @intCast(std.posix.W.EXITSTATUS(status)) };
    return .{ .signaled = @intCast(@intFromEnum(std.posix.W.TERMSIG(status))) };
}

/// Assert the child exited with exactly `want`; a signal death or a different
/// code is the failure, surfaced with the observed value for triage.
fn expectExit(pid: std.posix.pid_t, want: u8) !void {
    switch (waitChild(pid)) {
        .exited => |code| {
            if (code != want) {
                std.debug.print("enforcement child exited {d}, wanted {d}\n", .{ code, want });
                return error.WrongChildExitCode;
            }
        },
        .signaled => |sig| {
            std.debug.print("enforcement child died on signal {d}, wanted exit {d}\n", .{ sig, want });
            return error.ChildSignaled;
        },
    }
}

/// Child-safe (no allocator, single-shot) create+write probe: true ⟺ the file
/// could be created and one byte written. A Landlock-denied open returns EACCES
/// → fd < 0 → false.
fn tryCreateWrite(path: [*:0]const u8) bool {
    const fd: isize = @bitCast(linux.openat(linux.AT.FDCWD, path, .{ .ACCMODE = .WRONLY, .CREAT = true }, 0o600));
    if (fd < 0) return false;
    const wrote: isize = @bitCast(linux.write(@intCast(fd), "x", 1));
    _ = linux.close(@intCast(fd));
    return wrote == 1;
}

// ── seccomp: a denied syscall traps the walled child ─────────────────────────

test "integration: seccomp filter traps a denied syscall to the violation exit code" {
    if (builtin.os.tag != .linux) return error.SkipZigTest;

    const pid = try forkOrError();
    if (pid == 0) {
        // CHILD — the real in-child syscall-wall sequence (no_new_privs → filter).
        if (!setNoNewPrivs()) linux.exit(EXIT_SETUP_FAILED);
        seccomp.applyFilter() catch linux.exit(EXIT_SETUP_FAILED);
        // An ALLOWED syscall still works — proves the filter is default-allow, not
        // a deny-all that would make the trap below meaningless.
        _ = linux.getpid();
        // A DENIED syscall (ptrace) must trap: SECCOMP_RET_TRAP → SIGSYS →
        // onSigsys → exit_group(SECCOMP_VIOLATION_EXIT). RET_TRAP never runs the
        // call, so ptrace has no effect even if the trap somehow did not fire.
        _ = linux.syscall4(.ptrace, 0, 0, 0, 0);
        // Reached only if the trap did NOT fire — the wall failed.
        linux.exit(EXIT_NOT_ENFORCED);
    }
    try expectExit(pid, SECCOMP_VIOLATION_EXIT);
}

// ── Landlock: a write outside the workspace is denied ────────────────────────

// /tmp is bound read-only by applyPolicy; a workspace *under* /tmp gets a more
// specific RW path-beneath rule. So a write under the workspace is allowed while
// a sibling write elsewhere under /tmp is denied — the exact boundary the policy
// promises, proven without leaving the writable test tmp.
const LL_WORKSPACE: [*:0]const u8 = "/tmp/enforce-ws";
const LL_INSIDE: [*:0]const u8 = "/tmp/enforce-ws/inside.txt";
const LL_OUTSIDE: [*:0]const u8 = "/tmp/enforce-outside.txt";

test "integration: Landlock denies a write outside the workspace and allows one inside" {
    if (builtin.os.tag != .linux) return error.SkipZigTest;

    // The workspace dir must exist before applyPolicy opens it (addPathRule).
    _ = linux.mkdirat(linux.AT.FDCWD, LL_WORKSPACE, 0o755); // benign if it exists

    const pid = try forkOrError();
    if (pid == 0) {
        // CHILD — the real in-child filesystem wall (no_new_privs → restrict_self).
        if (!setNoNewPrivs()) linux.exit(EXIT_SETUP_FAILED);
        landlock.applyPolicy(std.mem.span(LL_WORKSPACE)) catch linux.exit(EXIT_SETUP_FAILED);
        // OUTSIDE the workspace → denied (default-deny; /tmp is read-only).
        if (tryCreateWrite(LL_OUTSIDE)) linux.exit(EXIT_NOT_ENFORCED);
        // INSIDE the workspace → still allowed (workspace RW) — not a deny-all.
        if (!tryCreateWrite(LL_INSIDE)) linux.exit(EXIT_CONTROL_DENIED);
        linux.exit(EXIT_ALL_CORRECT);
    }
    try expectExit(pid, EXIT_ALL_CORRECT);
}

// ── cgroup: the resource cage refuses a fork past pids.max and OOM-kills ──────
//
// These need a delegated cgroup-v2 controller subtree (memory+pids+cpu present in
// the parent's cgroup.subtree_control) — a privileged, host-level prerequisite.
// When it is absent (unprivileged CI, no delegation), CgroupScope.create fails and
// the test SkipZigTests rather than false-fail; the privileged lane sets it up
// first (scripts/cgroup-delegate.sh). The CONTROL halves (fork allowed under a high
// cap, an in-budget child) are covered by the existing real-process suite.

const PIDS_EXEC_ID: types.ExecutionId = [_]u8{0xa1} ** 16;
const OOM_EXEC_ID: types.ExecutionId = [_]u8{0xb2} ** 16;
const ROOMY_MEM_MB: u64 = 256; // generous memory so the PID cap — not memory — bites
const OOM_LIMIT_MB: u64 = 64; // small budget the child deliberately blows past
const FULL_CPU: u64 = 100;
const PAGE_BYTES: usize = 4096;
const MIB: usize = 1024 * 1024;
const OOM_TOUCH_BYTES: usize = 256 * MIB; // 4× the budget → a certain OOM
/// A representative non-special crash code so `classify` takes the resource branch
/// (not the 0 / SANDBOX_FAIL / SECCOMP_VIOLATION special cases).
const CRASH_EXIT_CODE: u8 = pipe_proto.GENERIC_FAIL_EXIT;

const GoPipe = struct { r: i32, w: i32 };

fn makeGoPipe() error{PipeFailed}!GoPipe {
    var fds: [2]i32 = undefined;
    const rc: isize = @bitCast(linux.pipe2(&fds, .{}));
    if (rc < 0) return error.PipeFailed;
    return .{ .r = fds[0], .w = fds[1] };
}

/// Child side: block until the parent writes the one-byte "enrolled" signal, so
/// the violation below happens only AFTER the child is inside the cgroup.
fn awaitGo(r: i32) void {
    var b: [1]u8 = undefined;
    _ = linux.read(r, &b, 1);
}

fn threadedIo(t: *std.Io.Threaded) std.Io {
    t.* = .init(std.testing.allocator, .{});
    return t.io();
}

/// Disable swap for the scope so an over-budget child is OOM-killed deterministically.
/// memory.max limits RAM only; with swap available the anon charge spills to swap and
/// never trips the OOM-killer (oom_kill stays 0). Setting memory.swap.max=0 mirrors a
/// swapless production node and makes the proof host-independent. Best-effort: a kernel
/// without swap accounting just ignores it. Writes to the scope's pub `path`.
/// Skip ONLY when the cgroup-v2 controller subtree is genuinely not delegated (the
/// host prerequisite is absent — scripts/cgroup-delegate.sh did not run / no
/// privilege). A delegated-but-broken cgroup must NOT skip: that is a real failure
/// the lane has to surface, not hide as green. So this gates on the observable
/// prerequisite and the tests then let CgroupScope.create errors propagate as
/// failures — no silent green on a misconfigured privileged lane.
fn requireCgroupDelegation() error{SkipZigTest}!void {
    if (builtin.os.tag != .linux) return error.SkipZigTest;
    var buf: [256]u8 = undefined;
    const fd: isize = @bitCast(linux.openat(linux.AT.FDCWD, "/sys/fs/cgroup/fleet.runner/cgroup.controllers", .{ .ACCMODE = .RDONLY }, 0));
    if (fd < 0) return error.SkipZigTest; // the runner base scope is not delegated
    const n: isize = @bitCast(linux.read(@intCast(fd), &buf, buf.len));
    _ = linux.close(@intCast(fd));
    if (n <= 0) return error.SkipZigTest;
    const ctrls = buf[0..@intCast(n)];
    if (std.mem.indexOf(u8, ctrls, "memory") == null or std.mem.indexOf(u8, ctrls, "pids") == null)
        return error.SkipZigTest; // memory/pids not delegated to child scopes
}

fn disableScopeSwap(scope_path: []const u8) void {
    var buf: [std.fs.max_path_bytes]u8 = undefined;
    const swap_path = std.fmt.bufPrintZ(&buf, "{s}/memory.swap.max", .{scope_path}) catch return;
    const fd: isize = @bitCast(linux.openat(linux.AT.FDCWD, swap_path.ptr, .{ .ACCMODE = .WRONLY }, 0));
    if (fd < 0) return;
    _ = linux.write(@intCast(fd), "0", 1);
    _ = linux.close(@intCast(fd));
}

test "integration: cgroup pids.max refuses a fork past the cap, attributed resource_kill" {
    try requireCgroupDelegation();
    const alloc = std.testing.allocator;
    var threaded: std.Io.Threaded = undefined;
    const io = threadedIo(&threaded);
    defer threaded.deinit();

    const limits = types.ResourceLimits{ .memory_limit_mb = ROOMY_MEM_MB, .cpu_limit_percent = FULL_CPU, .pids_limit = 1 };
    // Delegation is present (checked above), so a create failure here is a real
    // failure to surface — not a skip.
    var scope_opt: ?CgroupScope = try CgroupScope.create(io, alloc, PIDS_EXEC_ID, limits);
    defer _ = scope_opt.?.destroy(limits);

    const gp = try makeGoPipe();
    const pid = try forkOrError();
    if (pid == 0) {
        _ = linux.close(gp.w);
        awaitGo(gp.r);
        // In a pids.max=1 cage now (the child is the sole pid): a fork MUST be
        // refused (EAGAIN → negative). RET of 0 would be a forbidden grandchild.
        const f: isize = @bitCast(linux.fork());
        if (f < 0) linux.exit(EXIT_ALL_CORRECT); // cap enforced — fork refused
        if (f == 0) linux.exit(0); // grandchild that should never exist: exit quietly
        linux.exit(EXIT_NOT_ENFORCED); // the cap did NOT hold
    }
    _ = linux.close(gp.r);
    scope_opt.?.addProcess(pid) catch return error.CgroupEnrollFailed;
    _ = linux.write(gp.w, "g", 1);
    _ = linux.close(gp.w);
    try expectExit(pid, EXIT_ALL_CORRECT);

    // The kernel recorded the refused fork, and classify attributes a resulting
    // crash to resource_kill (the PID-cap cause, read via wasPidsExhausted).
    try std.testing.expect(scope_opt.?.wasPidsExhausted());
    const result = supervisor_result.classify(alloc, .{}, .{ .exited = CRASH_EXIT_CODE }, &scope_opt);
    try std.testing.expectEqual(types.FailureClass.resource_kill, result.failureClass().?);
}

test "integration: cgroup memory.max OOM-kills an over-budget child, attributed oom_kill" {
    try requireCgroupDelegation();
    const alloc = std.testing.allocator;
    var threaded: std.Io.Threaded = undefined;
    const io = threadedIo(&threaded);
    defer threaded.deinit();

    const limits = types.ResourceLimits{ .memory_limit_mb = OOM_LIMIT_MB, .cpu_limit_percent = FULL_CPU, .pids_limit = 64 };
    var scope_opt: ?CgroupScope = try CgroupScope.create(io, alloc, OOM_EXEC_ID, limits);
    defer _ = scope_opt.?.destroy(limits);
    disableScopeSwap(scope_opt.?.path); // force OOM instead of a silent swap-out

    const gp = try makeGoPipe();
    const pid = try forkOrError();
    if (pid == 0) {
        _ = linux.close(gp.w);
        awaitGo(gp.r); // enrolled before we allocate → the charge hits memory.max
        const mem = std.posix.mmap(null, OOM_TOUCH_BYTES, .{ .READ = true, .WRITE = true }, .{ .TYPE = .PRIVATE, .ANONYMOUS = true }, -1, 0) catch linux.exit(EXIT_SETUP_FAILED);
        // Touch each page so the kernel charges it; crossing memory.max trips the
        // cgroup OOM-killer (SIGKILL) before this loop completes.
        var i: usize = 0;
        while (i < mem.len) : (i += PAGE_BYTES) mem[i] = 1;
        linux.exit(EXIT_NOT_ENFORCED); // reached only if never OOM-killed
    }
    _ = linux.close(gp.r);
    scope_opt.?.addProcess(pid) catch return error.CgroupEnrollFailed;
    _ = linux.write(gp.w, "g", 1);
    _ = linux.close(gp.w);
    _ = waitChild(pid); // SIGKILLed by the OOM-killer; the kernel records the OOM

    try std.testing.expect(scope_opt.?.wasOomKilled());
    // wasOomKilled is checked first in classify, so even a clean exit-0 term is
    // reclassified oom_kill — the cgroup's verdict wins over the exit code.
    const result = supervisor_result.classify(alloc, .{}, .{ .exited = 0 }, &scope_opt);
    try std.testing.expectEqual(types.FailureClass.oom_kill, result.failureClass().?);
}
