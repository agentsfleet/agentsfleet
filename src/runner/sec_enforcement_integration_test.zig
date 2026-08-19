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
const CGROUP_MOUNT = "/sys/fs/cgroup";
const CGROUP_PROC_PATH = "/proc/self/cgroup";
const PROC_STATUS_PATH = "/proc/self/status";
/// The field naming the attached tracer's process id, or 0 when untraced.
const TRACER_PID_FIELD = "TracerPid:";
/// `/proc/self/status` runs ~1.3 KB and carries TracerPid in its first lines.
const MAX_PROC_STATUS_BYTES = 4096;
const S_CGROUP_CONTROLLERS = "{s}{s}/cgroup.controllers";
const UNIFIED_RUNNER_PLACEMENT = "0::/system.slice/agentsfleet-runner.service/runner\n";
const ROOT_CGROUP_PLACEMENT = "0::/\n";
const LEGACY_CGROUP_PLACEMENT = "11:memory:/system.slice/agentsfleet-runner.service/runner\n";
const MALFORMED_CGROUP_PLACEMENT = "0::system.slice/agentsfleet-runner.service/runner\n";
const SERVICE_CGROUP_PLACEMENT = "0::/system.slice/agentsfleet-runner.service\n";
const MAX_CGROUP_PLACEMENT_BYTES = 4096;
const OOM_KILL_COUNTER = "oom_kill ";
const PIDS_MAX_COUNTER = "max ";
const THROTTLED_USEC_COUNTER = "throttled_usec ";
const MEMORY_EVENTS_CONTENT = "low 0\nhigh 0\nmax 0\noom 0\noom_kill 3\n";
const PIDS_EVENTS_CONTENT = "max 7\n";
const CPU_STAT_CONTENT = "nr_periods 0\nthrottled_usec 12345\n";
const MALFORMED_OOM_KILL_CONTENT = "oom_kill abc\n";

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

/// True when a debugger or a ptrace-based profiler is attached to this process.
/// `/proc/self/status` reports `TracerPid: 0` when nothing is tracing us.
fn tracedByAnotherProcess() bool {
    var buf: [MAX_PROC_STATUS_BYTES]u8 = undefined;
    const fd: isize = @bitCast(linux.openat(linux.AT.FDCWD, PROC_STATUS_PATH, .{ .ACCMODE = .RDONLY }, 0));
    if (fd < 0) return false;
    const n: isize = @bitCast(linux.read(@intCast(fd), &buf, buf.len));
    _ = linux.close(@intCast(fd));
    if (n <= 0) return false;

    const status = buf[0..@intCast(n)];
    const field = std.mem.indexOf(u8, status, TRACER_PID_FIELD) orelse return false;
    const rest = status[field + TRACER_PID_FIELD.len ..];
    const line = rest[0 .. std.mem.indexOfScalar(u8, rest, '\n') orelse rest.len];
    const tracer = std.fmt.parseInt(std.posix.pid_t, std.mem.trim(u8, line, " \t"), 10) catch return false;
    return tracer != 0;
}

// ── seccomp: a denied syscall traps the walled child ─────────────────────────

test "integration: seccomp filter traps a denied syscall to the violation exit code" {
    if (builtin.os.tag != .linux) return error.SkipZigTest;
    // A tracer owns signal delivery: SIGSYS stops the child for the tracer, which
    // resumes it without redelivering, so the handler never runs and the child
    // falls through to EXIT_NOT_ENFORCED — a wall that held, reported as broken.
    // The kernel lane (`make test-integration-kernel`) runs this binary untraced
    // and is where the proof binds; the coverage lane runs the same binary under
    // kcov, whose instrumentation IS ptrace, so the answer there means nothing.
    if (tracedByAnotherProcess()) return error.SkipZigTest;

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

// The policy's write boundary has three arms now that /tmp is on the writable
// floor: a write under the WORKSPACE is allowed (its own rule), a write under
// the FLOOR is allowed (the shared tmpfs grant), and a write anywhere else is
// denied (default-deny). /var/tmp is the denied target — world-writable at the
// DAC layer like /tmp, so only landlock stands between the child and the file,
// and on no rule list (not workspace, not floor, not a read-only baseline).
const LL_WORKSPACE: [*:0]const u8 = "/var/tmp/enforce-ws";
const LL_INSIDE: [*:0]const u8 = "/var/tmp/enforce-ws/inside.txt";
const LL_OUTSIDE: [*:0]const u8 = "/var/tmp/enforce-outside.txt";
const LL_FLOOR: [*:0]const u8 = "/tmp/enforce-floor.txt";

test "integration: Landlock denies a write outside the workspace and allows one inside" {
    if (builtin.os.tag != .linux) return error.SkipZigTest;

    // The workspace dir must exist before applyPolicy opens it (addPathRule).
    _ = linux.mkdirat(linux.AT.FDCWD, LL_WORKSPACE, 0o755); // benign if it exists

    const pid = try forkOrError();
    if (pid == 0) {
        // CHILD — the real in-child filesystem wall (no_new_privs → restrict_self).
        if (!setNoNewPrivs()) linux.exit(EXIT_SETUP_FAILED);
        landlock.applyPolicy(std.mem.span(LL_WORKSPACE), &.{}) catch linux.exit(EXIT_SETUP_FAILED);
        // OUTSIDE workspace and floor → denied (default-deny, no rule names it).
        if (tryCreateWrite(LL_OUTSIDE)) linux.exit(EXIT_NOT_ENFORCED);
        // INSIDE the workspace → still allowed (workspace RW) — not a deny-all.
        if (!tryCreateWrite(LL_INSIDE)) linux.exit(EXIT_CONTROL_DENIED);
        // UNDER the writable floor → allowed: the kernel-level proof that the
        // shared tmpfs grant reaches a real ruleset, not just the list tests.
        if (!tryCreateWrite(LL_FLOOR)) linux.exit(EXIT_CONTROL_DENIED);
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
    var buf: [MAX_CGROUP_PLACEMENT_BYTES]u8 = undefined;
    const fd: isize = @bitCast(linux.openat(linux.AT.FDCWD, CGROUP_PROC_PATH, .{ .ACCMODE = .RDONLY }, 0));
    if (fd < 0) return error.SkipZigTest;
    const n: isize = @bitCast(linux.read(@intCast(fd), &buf, buf.len));
    _ = linux.close(@intCast(fd));
    if (n <= 0) return error.SkipZigTest;

    const placement = CgroupScope.delegatedCgroupPath(buf[0..@intCast(n)]) orelse return error.SkipZigTest;
    var controllers_path_buf: [std.fs.max_path_bytes]u8 = undefined;
    const controllers_path = std.fmt.bufPrintZ(&controllers_path_buf, S_CGROUP_CONTROLLERS, .{ CGROUP_MOUNT, placement }) catch return error.SkipZigTest;
    const controllers_fd: isize = @bitCast(linux.openat(linux.AT.FDCWD, controllers_path.ptr, .{ .ACCMODE = .RDONLY }, 0));
    if (controllers_fd < 0) return error.SkipZigTest;
    const controllers_len: isize = @bitCast(linux.read(@intCast(controllers_fd), &buf, buf.len));
    _ = linux.close(@intCast(controllers_fd));
    if (controllers_len <= 0) return error.SkipZigTest;

    const ctrls = buf[0..@intCast(controllers_len)];
    if (std.mem.indexOf(u8, ctrls, "memory") == null or std.mem.indexOf(u8, ctrls, "pids") == null)
        return error.SkipZigTest; // memory/pids not delegated to child scopes
}

test "runner cgroup base requires a unified delegated service path" {
    const service_path = CgroupScope.delegatedCgroupPath(UNIFIED_RUNNER_PLACEMENT) orelse return error.TestExpectedEqual;
    try std.testing.expectEqualStrings("/system.slice/agentsfleet-runner.service", service_path);
    try std.testing.expect(CgroupScope.delegatedCgroupPath(ROOT_CGROUP_PLACEMENT) == null);
    try std.testing.expect(CgroupScope.delegatedCgroupPath(LEGACY_CGROUP_PLACEMENT) == null);
    try std.testing.expect(CgroupScope.delegatedCgroupPath(MALFORMED_CGROUP_PLACEMENT) == null);
    try std.testing.expect(CgroupScope.delegatedCgroupPath(SERVICE_CGROUP_PLACEMENT) == null);
}

test "runner cgroup counters parse valid values and fail safe" {
    try std.testing.expectEqual(@as(u64, 3), CgroupScope.parseEventCount(MEMORY_EVENTS_CONTENT, OOM_KILL_COUNTER));
    try std.testing.expectEqual(@as(u64, 7), CgroupScope.parseEventCount(PIDS_EVENTS_CONTENT, PIDS_MAX_COUNTER));
    try std.testing.expectEqual(@as(u64, 12345), CgroupScope.parseEventCount(CPU_STAT_CONTENT, THROTTLED_USEC_COUNTER));
    try std.testing.expectEqual(@as(u64, 0), CgroupScope.parseEventCount(MALFORMED_OOM_KILL_CONTENT, OOM_KILL_COUNTER));
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

// ── Delegated-subtree enablement + scope reclaim ─────────────────────────────
//
// These prove the two halves of the cgroup lifecycle the daemon owns: writing
// `cgroup.subtree_control` (which systemd never does for a delegatee) and
// removing an execution scope afterwards. Both are Linux-only and both skip
// without real delegation, so they are green in CI's delegated container and
// silent on a developer's laptop.

const RECLAIM_EXEC_ID: types.ExecutionId = [_]u8{0xc3} ** 16;
/// Same identifier the scope itself uses to derive `memory_limit_bytes`
/// (`engine/CgroupScope.zig`), so the expectation cannot drift from the code.
const BYTES_PER_KIB: u64 = 1024;
const RECLAIM_MEM_MB: u64 = 64;
const RECLAIM_PIDS: u64 = 16;
const EXEC_SCOPE_PREFIX = "exec-";

test "integration: enabling the delegated controllers is idempotent across restarts" {
    try requireCgroupDelegation();
    const alloc = std.testing.allocator;
    var threaded: std.Io.Threaded = undefined;
    const io = threadedIo(&threaded);
    defer threaded.deinit();

    // The daemon does this at startup, so a restart re-runs it against a subtree
    // that may already carry the controllers. Re-enabling an enabled controller
    // is a kernel no-op; if it were not, every restart would fail here.
    try CgroupScope.enableDelegatedControllers(io, alloc);
    try CgroupScope.enableDelegatedControllers(io, alloc);

    const base = try CgroupScope.resolveCgroupBase(io, alloc);
    defer alloc.free(base);
    const subtree = try std.fmt.allocPrint(alloc, "{s}/cgroup.subtree_control", .{base});
    defer alloc.free(subtree);

    const file = try std.Io.Dir.openFileAbsolute(io, subtree, .{});
    defer file.close(io);
    var reader = file.reader(io, &.{});
    var buf: [MAX_CGROUP_PLACEMENT_BYTES]u8 = undefined;
    const len = try reader.interface.readSliceShort(&buf);
    const enabled = buf[0..len];

    for ([_][]const u8{ "cpu", "memory", "pids" }) |controller| {
        if (std.mem.indexOf(u8, enabled, controller) == null) {
            std.debug.print("controller '{s}' absent from subtree_control '{s}'\n", .{ controller, enabled });
            return error.ControllerNotEnabled;
        }
    }
}

test "integration: destroying a scope removes its directory and leaves no orphan" {
    try requireCgroupDelegation();
    const alloc = std.testing.allocator;
    var threaded: std.Io.Threaded = undefined;
    const io = threadedIo(&threaded);
    defer threaded.deinit();

    try CgroupScope.enableDelegatedControllers(io, alloc);

    const limits = types.ResourceLimits{
        .memory_limit_mb = RECLAIM_MEM_MB,
        .cpu_limit_percent = FULL_CPU,
        .pids_limit = RECLAIM_PIDS,
    };
    var scope = try CgroupScope.create(io, alloc, RECLAIM_EXEC_ID, limits);

    // Copy the path: destroy frees the scope's own copy.
    const scope_path = try alloc.dupe(u8, scope.path);
    defer alloc.free(scope_path);
    std.Io.Dir.accessAbsolute(io, scope_path, .{}) catch return error.ScopeNotCreated;

    _ = scope.destroy(limits);

    // The regression: reclaim used a recursive tree delete, which the kernel
    // refuses because the control files inside a cgroup cannot be unlinked. Every
    // teardown failed with PermissionDenied and the directories accumulated —
    // 25 of them were resident on the dev host when this was found.
    if (std.Io.Dir.accessAbsolute(io, scope_path, .{})) |_| {
        std.debug.print("scope survived destroy: {s}\n", .{scope_path});
        return error.ScopeNotReclaimed;
    } else |_| {}
}

test "integration: no exec- scope directory survives a create/destroy cycle" {
    try requireCgroupDelegation();
    const alloc = std.testing.allocator;
    var threaded: std.Io.Threaded = undefined;
    const io = threadedIo(&threaded);
    defer threaded.deinit();

    try CgroupScope.enableDelegatedControllers(io, alloc);
    const base = try CgroupScope.resolveCgroupBase(io, alloc);
    defer alloc.free(base);

    const limits = types.ResourceLimits{
        .memory_limit_mb = RECLAIM_MEM_MB,
        .cpu_limit_percent = FULL_CPU,
        .pids_limit = RECLAIM_PIDS,
    };
    var scope = try CgroupScope.create(io, alloc, RECLAIM_EXEC_ID, limits);
    _ = scope.destroy(limits);

    // Sweep the delegated base: the daemon's own `runner` leaf is expected, any
    // leftover `exec-` sibling is a leaked lease scope.
    var dir = try std.Io.Dir.openDirAbsolute(io, base, .{ .iterate = true });
    defer dir.close(io);
    var it = dir.iterate();
    while (try it.next(io)) |entry| {
        if (std.mem.startsWith(u8, entry.name, EXEC_SCOPE_PREFIX)) {
            std.debug.print("orphan execution scope: {s}/{s}\n", .{ base, entry.name });
            return error.OrphanScopeLeaked;
        }
    }
}

test "integration: a failed reclaim still returns the lease's own metrics" {
    try requireCgroupDelegation();
    const alloc = std.testing.allocator;
    var threaded: std.Io.Threaded = undefined;
    const io = threadedIo(&threaded);
    defer threaded.deinit();

    try CgroupScope.enableDelegatedControllers(io, alloc);

    const limits = types.ResourceLimits{
        .memory_limit_mb = RECLAIM_MEM_MB,
        .cpu_limit_percent = FULL_CPU,
        .pids_limit = RECLAIM_PIDS,
    };
    var scope = try CgroupScope.create(io, alloc, RECLAIM_EXEC_ID, limits);

    // Remove the directory out from under destroy() so its reclaim must fail.
    // The lease's outcome must not move because of it: a teardown problem is
    // logged, never folded into what the caller reports about the run.
    std.Io.Dir.deleteDirAbsolute(io, scope.path) catch return error.SetupRemoveFailed;

    const metrics = scope.destroy(limits);
    try std.testing.expectEqual(
        RECLAIM_MEM_MB * BYTES_PER_KIB * BYTES_PER_KIB,
        metrics.memory_limit_bytes,
    );
}
