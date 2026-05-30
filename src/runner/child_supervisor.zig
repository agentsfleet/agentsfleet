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
const client_errors = @import("engine/client_errors.zig");
const Config = @import("daemon/config.zig");
const contract = @import("contract");
const pipe_proto = @import("pipe_proto.zig");
const child_process = @import("child_process.zig");

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

/// What the read loop should do after a renewal tick or a progress frame.
/// `extend` carries the new absolute kill deadline (epoch ms).
pub const RenewDecision = union(enum) { keep, extend: i64, terminate };

/// Hook the daemon installs so the supervisor can drive lease renewal during a
/// long execution without the supervisor knowing any HTTP. `onTick` is called
/// with the current epoch ms in the idle gap between frames (renewal-tick
/// cadence) and again after each progress frame; the daemon renews if inside the
/// window and returns a decision. A live child that emits no frames still ticks,
/// so a legitimate long run renews and is never falsely reclaimed.
pub const RenewHook = struct {
    ctx: *anyopaque,
    onTick: *const fn (ctx: *anyopaque, now_ms: i64) RenewDecision,
    /// How often (ms) the read loop wakes between frames to consider renewal.
    /// Production sets `constants.RENEWAL_TICK_MS`; tests inject a small value.
    tick_ms: i64,
};

/// Cap on the serialized result we read back from a child (defensive against a
/// runaway child flooding stdout).
const MAX_RESULT_BYTES: usize = 8 * 1024 * 1024;

/// The one tier that runs WITHOUT isolation (dev only) — derived from the
/// `SandboxTier` enum (RULE UFS: single source, never a re-spelled literal). A
/// production startup guard must reject this tier so it can't become the prod
/// default.
const SANDBOX_TIER_DEV_NONE = @tagName(contract.protocol.SandboxTier.dev_none);

/// What the parent observed reading the child's stdout.
pub const ReadOutcome = struct {
    /// Result bytes the child wrote (alloc-owned; empty on timeout/crash).
    bytes: []u8 = &.{},
    /// The wall-clock lease deadline (possibly extended by renewals) elapsed
    /// before the child produced a result.
    timed_out: bool = false,
    /// A renewal hook returned `.terminate` (lease lost / capped / no credits) —
    /// the child must be killed even though the deadline has not elapsed.
    terminated: bool = false,
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
    renew_hook: ?RenewHook,
) ExecutionResult {
    const lease_json = std.json.Stringify.valueAlloc(alloc, payload, .{}) catch
        return failed(.startup_posture);
    defer alloc.free(lease_json);

    return supervise(alloc, cfg, workspace_path, payload, lease_json, sink, renew_hook) catch |err| {
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
    renew_hook: ?RenewHook,
) !ExecutionResult {
    // Fail-closed (Invariant 7): if the tier requires isolation and it cannot
    // be established, refuse the lease — never run prompt-injectable tool
    // execution unsandboxed.
    const requires_sandbox = !std.mem.eql(u8, cfg.sandbox_tier, SANDBOX_TIER_DEV_NONE);
    var scope: ?cgroup.CgroupScope = establishSandbox(alloc, requires_sandbox) catch {
        log.err("sandbox_unavailable_fail_closed", .{
            .error_code = client_errors.ERR_RUN_SANDBOX_ESTABLISH_FAILED,
            .lease_id = payload.lease_id,
            .tier = cfg.sandbox_tier,
        });
        return failed(.startup_posture);
    };
    defer if (scope) |*s| {
        _ = s.destroy(.{});
    };

    const child = try child_process.forkExec(alloc, cfg, workspace_path);
    var reaped = false;
    defer if (!reaped) {
        _ = std.posix.waitpid(child.pid, 0);
    };

    if (scope) |*s| s.addProcess(child.pid) catch |err|
        log.warn("cgroup_add_failed", .{ .lease_id = payload.lease_id, .err = @errorName(err) });

    // Feed the lease (incl. inline secrets) in, then EOF the child's stdin.
    child_process.writeAll(child.stdin_w, lease_json) catch |err|
        log.warn("lease_write_failed", .{ .err = @errorName(err) });
    std.posix.close(child.stdin_w);

    const outcome = try readResult(alloc, child.stdout_r, payload.lease_expires_at, sink, renew_hook);
    defer alloc.free(outcome.bytes);
    std.posix.close(child.stdout_r);

    if (outcome.terminated) {
        log.warn("lease_terminated_by_renewal", .{ .lease_id = payload.lease_id });
        child_process.killChild(child.pid, &scope);
    } else if (outcome.timed_out) {
        log.warn("lease_timed_out", .{ .lease_id = payload.lease_id });
        child_process.killChild(child.pid, &scope);
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
pub fn establishSandbox(alloc: std.mem.Allocator, requires: bool) !?cgroup.CgroupScope {
    if (!requires) return null;
    // macOS Seatbelt is not yet wired — a required sandbox we cannot apply must
    // fail closed rather than pretend. (Dev on macOS uses the dev_none tier.)
    if (builtin.os.tag != .linux) return error.SandboxUnavailable;
    // SAFETY: filled by std.crypto.random.bytes on the next line before any read.
    var exec_id: types.ExecutionId = undefined;
    std.crypto.random.bytes(&exec_id);
    return cgroup.CgroupScope.create(alloc, exec_id, .{}) catch error.SandboxUnavailable;
}

/// Read the child's framed stdout up to the terminal `result` frame, bounded by
/// the lease deadline. Each `activity` frame is forwarded best-effort and freed;
/// the `result` frame's bytes are returned (caller-owned). EOF before a result
/// yields empty bytes (the caller classifies that as a transport loss); deadline
/// elapse sets `timed_out` and the caller kills the child.
pub fn readResult(
    alloc: std.mem.Allocator,
    fd: std.posix.fd_t,
    deadline_ms: i64,
    sink: ActivitySink,
    renew_hook: ?RenewHook,
) !ReadOutcome {
    var deadline = deadline_ms;
    while (true) {
        const tick_deadline = if (renew_hook) |h|
            @min(deadline, std.time.milliTimestamp() + h.tick_ms)
        else
            deadline;
        switch (try pipe_proto.waitReadable(fd, tick_deadline)) {
            .timed_out => {
                const now = std.time.milliTimestamp();
                if (now >= deadline) return .{ .timed_out = true };
                if (applyTick(renew_hook, &deadline, now)) return .{ .terminated = true };
                continue;
            },
            .readable => {},
        }
        // Data is present: read one whole frame at the full lease deadline so a
        // tick never interrupts a frame mid-read (which would desync the stream).
        switch (try pipe_proto.readFrame(alloc, fd, deadline, MAX_RESULT_BYTES)) {
            .timed_out => return .{ .timed_out = true },
            .eof => return .{},
            .frame => |f| switch (f.ftype) {
                .activity => {
                    defer alloc.free(f.payload);
                    forwardActivity(alloc, sink, f.payload);
                    // A progress frame attests liveness and is a renewal point too.
                    if (applyTick(renew_hook, &deadline, std.time.milliTimestamp()))
                        return .{ .terminated = true };
                },
                .result => return .{ .bytes = f.payload },
            },
        }
    }
}

/// Ask the renewal hook for a decision and apply it to `deadline`. Returns true
/// iff the child must be terminated (lease lost / capped / no credits). A null
/// hook (no renewal configured) is a no-op.
fn applyTick(renew_hook: ?RenewHook, deadline: *i64, now_ms: i64) bool {
    const h = renew_hook orelse return false;
    switch (h.onTick(h.ctx, now_ms)) {
        .keep => {},
        .extend => |new_deadline| deadline.* = new_deadline,
        .terminate => return true,
    }
    return false;
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

/// Map the child's exit status + read outcome to an `ExecutionResult`.
/// Precedence: renewal-terminate / deadline timeout → OOM (cgroup) → clean exit
/// (parse result) → abnormal exit/signal (crash). A renewal `.terminate` (lease
/// lost / capped / no credits) is a killed-before-completion outcome, same class
/// as a deadline kill — the work is handled elsewhere or stopped by policy.
fn classify(
    alloc: std.mem.Allocator,
    outcome: ReadOutcome,
    status: u32,
    scope: *?cgroup.CgroupScope,
) ExecutionResult {
    if (outcome.timed_out or outcome.terminated) return failed(.timeout_kill);
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

// Tests live in child_supervisor_test.zig (sibling) to keep this file within
// the line budget; registered via the runner test aggregator in main.zig.
