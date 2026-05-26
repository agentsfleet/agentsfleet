//! child_supervisor.zig — fork-per-lease sandboxed execution (parent side).
//!
//! Each lease runs in a fresh child process. The runner forks; the child
//! re-execs `zombie-runner __execute` under bubblewrap (Linux) or directly
//! (dev/macOS). The parent binds the child to a cgroup scope, feeds the lease
//! in over the child's stdin, reads the `ExecutionResult` back over its stdout,
//! and enforces a wall-clock deadline (`lease_expires_at`).
//!
//! The cgroup is the control plane: `scope.kill()` on a timeout/revocation
//! (atomic, whole-tree, no PID chase), `wasOomKilled()` to classify a memory
//! death. The child is ALWAYS reaped (orphan-safe) and the scope ALWAYS
//! destroyed (idempotent), on every exit path — the lifecycle, not the
//! syscalls, is the point.
//!
//! Isolation rests on the process boundary: a misbehaving agent (OOM, crash,
//! escape attempt) is contained in the child; the parent daemon survives,
//! reaps it, and reports the failure rather than dying. The lease's inline
//! secrets ride the child's stdin pipe — never argv or env (both readable in
//! /proc) — per RULE VLT.

const std = @import("std");
const builtin = @import("builtin");
const logging = @import("log");

const types = @import("engine/types.zig");
const cgroup = @import("engine/cgroup.zig");
const sandbox = @import("sandbox_args.zig");
const Config = @import("daemon/config.zig");
const contract = @import("contract");
const pipe_proto = @import("pipe_proto.zig");

const log = logging.scoped(.runner_supervisor);

const ExecutionResult = types.ExecutionResult;
const LeasePayload = contract.protocol.LeasePayload;
const ActivityFrame = contract.activity.ActivityFrame;

/// Best-effort sink for the `activity` frames the child streams while running.
/// The parent forwards each to the control plane (`POST .../activity`); a
/// dropped frame is cosmetic (the durable record is `report`), so `forward`
/// returns void and never fails the lease.
pub const ActivitySink = struct {
    ctx: *anyopaque,
    forward: *const fn (ctx: *anyopaque, frame: ActivityFrame) void,
};

/// Cap on the serialized result we read back from a child (defensive against a
/// runaway child flooding stdout).
const MAX_RESULT_BYTES: usize = 8 * 1024 * 1024;

/// The one tier that runs WITHOUT isolation (dev only) — derived from the
/// `SandboxTier` enum (RULE UFS: single source, never a re-spelled literal). A
/// production startup guard must reject this tier so it can't become the prod
/// default.
const SANDBOX_TIER_DEV_NONE = @tagName(contract.protocol.SandboxTier.dev_none);

/// A forked child and the parent ends of its stdio pipes.
const Child = struct {
    pid: std.posix.pid_t,
    stdin_w: std.posix.fd_t,
    stdout_r: std.posix.fd_t,
};

/// What the parent observed reading the child's stdout.
const ReadOutcome = struct {
    /// Result bytes the child wrote (alloc-owned; empty on timeout/crash).
    bytes: []u8 = &.{},
    /// The wall-clock deadline elapsed before the child finished.
    timed_out: bool = false,
};

/// Run one leased event in a forked, sandboxed child and return its result.
/// Never errors: every supervision failure maps to a failed `ExecutionResult`
/// so the caller always has an outcome to report.
pub fn run(
    alloc: std.mem.Allocator,
    cfg: Config,
    workspace_path: []const u8,
    payload: LeasePayload,
    sink: ActivitySink,
) ExecutionResult {
    const lease_json = std.json.Stringify.valueAlloc(alloc, payload, .{}) catch
        return failed(.startup_posture);
    defer alloc.free(lease_json);

    return supervise(alloc, cfg, workspace_path, payload, lease_json, sink) catch |err| {
        log.err("supervise_failed", .{ .lease_id = payload.lease_id, .err = @errorName(err) });
        return failed(.executor_crash);
    };
}

fn failed(class: types.FailureClass) ExecutionResult {
    return .{ .exit_ok = false, .failure = class };
}

/// Fork → bind to cgroup → feed lease → read result under a deadline → reap.
/// `defer` reaps the child and destroys the scope on every path (errors too).
fn supervise(
    alloc: std.mem.Allocator,
    cfg: Config,
    workspace_path: []const u8,
    payload: LeasePayload,
    lease_json: []const u8,
    sink: ActivitySink,
) !ExecutionResult {
    // Fail-closed (Invariant 7): if the tier requires isolation and it cannot
    // be established, refuse the lease — never run prompt-injectable tool
    // execution unsandboxed.
    const requires_sandbox = !std.mem.eql(u8, cfg.sandbox_tier, SANDBOX_TIER_DEV_NONE);
    var scope: ?cgroup.CgroupScope = establishSandbox(alloc, requires_sandbox) catch {
        log.err("sandbox_unavailable_fail_closed", .{ .lease_id = payload.lease_id, .tier = cfg.sandbox_tier });
        return failed(.startup_posture);
    };
    defer if (scope) |*s| {
        _ = s.destroy(.{});
    };

    const child = try forkExec(alloc, cfg, workspace_path);
    var reaped = false;
    defer if (!reaped) {
        _ = std.posix.waitpid(child.pid, 0);
    };

    if (scope) |*s| s.addProcess(child.pid) catch |err|
        log.warn("cgroup_add_failed", .{ .lease_id = payload.lease_id, .err = @errorName(err) });

    // Feed the lease (incl. inline secrets) in, then EOF the child's stdin.
    writeAll(child.stdin_w, lease_json) catch |err|
        log.warn("lease_write_failed", .{ .err = @errorName(err) });
    std.posix.close(child.stdin_w);

    const outcome = try readResult(alloc, child.stdout_r, payload.lease_expires_at, sink);
    defer alloc.free(outcome.bytes);
    std.posix.close(child.stdout_r);

    if (outcome.timed_out) {
        log.warn("lease_timed_out", .{ .lease_id = payload.lease_id });
        killChild(child.pid, &scope);
    }

    const wait_result = std.posix.waitpid(child.pid, 0);
    reaped = true;
    return classify(alloc, outcome, wait_result.status, &scope);
}

/// Fail-closed sandbox setup (Invariant 7). `dev_none` (requires=false) returns
/// null — explicit no-isolation, dev only. Every other tier MUST have its
/// isolation domain established or this errors, so the caller refuses the lease
/// rather than running the agent unsandboxed. (Landlock + netns are applied by
/// the child before it runs the agent; this sets up the parent-side cgroup
/// kill/limit domain.)
fn establishSandbox(alloc: std.mem.Allocator, requires: bool) !?cgroup.CgroupScope {
    if (!requires) return null;
    // macOS Seatbelt is not yet wired — a required sandbox we cannot apply must
    // fail closed rather than pretend. (Dev on macOS uses the dev_none tier.)
    if (builtin.os.tag != .linux) return error.SandboxUnavailable;
    // SAFETY: filled by std.crypto.random.bytes on the next line before any read.
    var exec_id: types.ExecutionId = undefined;
    std.crypto.random.bytes(&exec_id);
    return cgroup.CgroupScope.create(alloc, exec_id, .{}) catch error.SandboxUnavailable;
}

/// Fork and, in the child, dup the pipes onto stdin/stdout, lead a new process
/// group (so the parent can signal the whole group on the dev path), and exec
/// the sandbox wrapper. Returns the child pid + parent pipe ends.
fn forkExec(alloc: std.mem.Allocator, cfg: Config, workspace_path: []const u8) !Child {
    const in_pipe = try std.posix.pipe(); // [0]=read (child), [1]=write (parent)
    const out_pipe = try std.posix.pipe(); // [0]=read (parent), [1]=write (child)

    const argv = try sandbox.buildArgv(alloc, cfg, workspace_path);
    defer sandbox.freeArgv(alloc, argv);

    const pid = try std.posix.fork();
    if (pid == 0) {
        // ── child ──
        std.posix.dup2(in_pipe[0], std.posix.STDIN_FILENO) catch std.posix.exit(127);
        std.posix.dup2(out_pipe[1], std.posix.STDOUT_FILENO) catch std.posix.exit(127);
        std.posix.close(in_pipe[0]);
        std.posix.close(in_pipe[1]);
        std.posix.close(out_pipe[0]);
        std.posix.close(out_pipe[1]);
        std.posix.setpgid(0, 0) catch |err| log.warn("child_setpgid_failed", .{ .err = @errorName(err) });
        const err = std.process.execv(alloc, argv);
        log.err("child_execv_failed", .{ .err = @errorName(err) });
        std.posix.exit(127);
    }
    // ── parent ──
    std.posix.close(in_pipe[0]);
    std.posix.close(out_pipe[1]);
    return .{ .pid = pid, .stdin_w = in_pipe[1], .stdout_r = out_pipe[0] };
}

/// Read the child's framed stdout up to the terminal `result` frame, bounded by
/// the lease deadline. Each `activity` frame is forwarded best-effort and freed;
/// the `result` frame's bytes are returned (caller-owned). EOF before a result
/// yields empty bytes (the caller classifies that as a transport loss); deadline
/// elapse sets `timed_out` and the caller kills the child.
fn readResult(alloc: std.mem.Allocator, fd: std.posix.fd_t, deadline_ms: i64, sink: ActivitySink) !ReadOutcome {
    while (true) {
        switch (try pipe_proto.readFrame(alloc, fd, deadline_ms, MAX_RESULT_BYTES)) {
            .timed_out => return .{ .timed_out = true },
            .eof => return .{},
            .frame => |f| switch (f.ftype) {
                .activity => {
                    defer alloc.free(f.payload);
                    forwardActivity(alloc, sink, f.payload);
                },
                .result => return .{ .bytes = f.payload },
            },
        }
    }
}

/// Parse one `activity` frame payload and hand it to the sink. Best-effort: a
/// malformed frame is dropped (activity is cosmetic). The parsed frame's slices
/// borrow `arena`, valid for the synchronous `forward` call.
fn forwardActivity(alloc: std.mem.Allocator, sink: ActivitySink, payload: []const u8) void {
    var arena = std.heap.ArenaAllocator.init(alloc);
    defer arena.deinit();
    const frame = std.json.parseFromSliceLeaky(ActivityFrame, arena.allocator(), payload, .{}) catch return;
    sink.forward(sink.ctx, frame);
}

/// Kill the whole child tree. On Linux the cgroup is the atomic kill domain;
/// otherwise signal the child's process group (it leads its own via setpgid).
fn killChild(pid: std.posix.pid_t, scope: *?cgroup.CgroupScope) void {
    if (scope.*) |*s| {
        s.kill() catch |err| {
            log.warn("cgroup_kill_failed_fallback_signal", .{ .err = @errorName(err) });
            std.posix.kill(-pid, std.posix.SIG.KILL) catch |kerr| log.warn("child_group_kill_failed", .{ .err = @errorName(kerr) });
        };
        return;
    }
    std.posix.kill(-pid, std.posix.SIG.KILL) catch |err| log.warn("child_group_kill_failed", .{ .err = @errorName(err) });
}

/// Map the child's exit status + read outcome to an `ExecutionResult`.
/// Precedence: deadline timeout → OOM (cgroup) → clean exit (parse result) →
/// abnormal exit/signal (crash).
fn classify(
    alloc: std.mem.Allocator,
    outcome: ReadOutcome,
    status: u32,
    scope: *?cgroup.CgroupScope,
) ExecutionResult {
    if (outcome.timed_out) return failed(.timeout_kill);
    if (scope.*) |*s| if (s.wasOomKilled()) return failed(.oom_kill);
    if (!std.posix.W.IFEXITED(status) or std.posix.W.EXITSTATUS(status) != 0)
        return failed(.executor_crash);
    return parseResult(alloc, outcome.bytes);
}

/// Parse the child's serialized `ExecutionResult`; content is dup'd into the
/// caller's allocator (the caller frees it), the parse arena is released here.
fn parseResult(alloc: std.mem.Allocator, bytes: []const u8) ExecutionResult {
    const parsed = std.json.parseFromSlice(ExecutionResult, alloc, bytes, .{
        .ignore_unknown_fields = true,
    }) catch return failed(.transport_loss);
    defer parsed.deinit();
    const v = parsed.value;
    const content = alloc.dupe(u8, v.content) catch "";
    return .{
        .content = content,
        .token_count = v.token_count,
        .wall_seconds = v.wall_seconds,
        .exit_ok = v.exit_ok,
        .failure = v.failure,
    };
}

fn writeAll(fd: std.posix.fd_t, bytes: []const u8) !void {
    var off: usize = 0;
    while (off < bytes.len) off += try std.posix.write(fd, bytes[off..]);
}

// ── Tests ───────────────────────────────────────────────────────────────────

test "readResult forwards activity frames in order and returns the result frame" {
    const Cap = struct {
        count: usize = 0,
        name_buf: [64]u8 = [_]u8{0} ** 64,
        name_len: usize = 0,
        fn forward(ctx: *anyopaque, frame: ActivityFrame) void {
            const self: *@This() = @ptrCast(@alignCast(ctx));
            self.count += 1;
            if (frame == .tool_call_started) {
                const n = frame.tool_call_started.name;
                @memcpy(self.name_buf[0..n.len], n);
                self.name_len = n.len;
            }
        }
    };

    const fds = try std.posix.pipe();
    defer std.posix.close(fds[0]);
    const af = ActivityFrame{ .tool_call_started = .{ .name = "fly_deploy", .args_redacted = "{}" } };
    const af_json = try std.json.Stringify.valueAlloc(std.testing.allocator, af, .{});
    defer std.testing.allocator.free(af_json);
    try pipe_proto.writeFrame(fds[1], .activity, af_json);
    try pipe_proto.writeFrame(fds[1], .result, "{\"exit_ok\":true}");
    std.posix.close(fds[1]);

    var cap: Cap = .{};
    const sink = ActivitySink{ .ctx = &cap, .forward = Cap.forward };
    const dl = std.time.milliTimestamp() + 5_000;
    const outcome = try readResult(std.testing.allocator, fds[0], dl, sink);
    defer std.testing.allocator.free(outcome.bytes);

    try std.testing.expect(!outcome.timed_out);
    try std.testing.expectEqualStrings("{\"exit_ok\":true}", outcome.bytes);
    try std.testing.expectEqual(@as(usize, 1), cap.count);
    try std.testing.expectEqualStrings("fly_deploy", cap.name_buf[0..cap.name_len]);
}

test "readResult tolerates a malformed activity frame and still returns the result" {
    const Noop = struct {
        fn forward(_: *anyopaque, _: ActivityFrame) void {}
    };
    const fds = try std.posix.pipe();
    defer std.posix.close(fds[0]);
    try pipe_proto.writeFrame(fds[1], .activity, "{not valid json"); // dropped
    try pipe_proto.writeFrame(fds[1], .result, "{\"exit_ok\":false}");
    std.posix.close(fds[1]);

    var dummy: u8 = 0;
    const sink = ActivitySink{ .ctx = &dummy, .forward = Noop.forward };
    const dl = std.time.milliTimestamp() + 5_000;
    const outcome = try readResult(std.testing.allocator, fds[0], dl, sink);
    defer std.testing.allocator.free(outcome.bytes);
    try std.testing.expectEqualStrings("{\"exit_ok\":false}", outcome.bytes);
}
