//! Terminal result mapping for child_supervisor — the pure, security-free tail
//! of supervision: how a child's read outcome + exit status become an
//! `ExecutionResult`. Split out of `child_supervisor.zig` to keep that file
//! within the line budget; the supervisor re-exports `ReadOutcome`/`classify`.

const std = @import("std");
const types = @import("engine/types.zig");
const cgroup = @import("engine/CgroupScope.zig");
const pipe_proto = @import("pipe_proto.zig");

const ExecutionResult = types.ExecutionResult;

/// What the parent observed reading the child's stdout.
pub const ReadOutcome = struct {
    /// Result bytes the child wrote (alloc-owned; empty on timeout/crash).
    bytes: []u8 = &.{},
    /// The wall-clock lease deadline (possibly extended by renewals) elapsed
    /// before the child produced a result.
    timed_out: bool = false,
    /// A renewal hook returned `.terminate` (lease lost / capped / no credits /
    /// fleet budget exhausted) — the child must be killed even though the
    /// deadline has not elapsed.
    terminated: bool = false,
    /// Why `terminated` fired. Defaulted so a hook that reports no reason — and
    /// every call site that never set one — keeps the historical behaviour of
    /// attributing the stop to renewal policy.
    terminate_reason: types.FailureClass = .renewal_terminate,
};

/// A failed execution with no body — the supervisor's universal "something went
/// wrong" outcome, classified by `class`.
pub fn failed(class: types.FailureClass) ExecutionResult {
    return .{ .exit_ok = false, .failure = class };
}

/// Map the child's exit status + read outcome to an `ExecutionResult`.
/// Precedence: renewal-terminate → deadline timeout → OOM (cgroup) → exit 0
/// (parse result) → SANDBOX_FAIL_EXIT (startup_posture) → SECCOMP_VIOLATION_EXIT
/// (landlock_deny) → PID-cap-exhausted crash (resource_kill) → other crash. A
/// renewal `.terminate` carries its own class — `renewal_terminate` for a lease
/// lost / capped / no-credits stop, `budget_breach` when the fleet reached its
/// own ceiling — both kept distinct from `timeout_kill` (the wall-clock
/// deadline) so triage and billing can tell a policy kill from a clock kill.
/// Terminate wins over a co-occurring timeout: the policy reason is the more
/// actionable cause.
pub fn classify(
    alloc: std.mem.Allocator,
    outcome: ReadOutcome,
    term: std.process.Child.Term,
    scope: *?cgroup,
) ExecutionResult {
    if (outcome.terminated) return failed(outcome.terminate_reason);
    if (outcome.timed_out) return failed(.timeout_kill);
    if (scope.*) |*s| if (s.wasOomKilled()) return failed(.oom_kill);
    // Exit 0 = parse the result frame. The child's distinct exit codes attribute
    // sandbox-setup aborts (startup_posture) and seccomp traps (landlock_deny);
    // any other failure is a crash, or resource_kill if the PID cap was hit.
    return switch (term) {
        .exited => |code| switch (code) {
            0 => parseResult(alloc, outcome.bytes),
            pipe_proto.SANDBOX_FAIL_EXIT => failed(.startup_posture),
            pipe_proto.SECCOMP_VIOLATION_EXIT => failed(.landlock_deny),
            else => crashOrResource(scope),
        },
        else => crashOrResource(scope),
    };
}

/// A crash with no more-specific cause -> resource_kill if the PID cap was hit, else runner_crash.
fn crashOrResource(scope: *?cgroup) ExecutionResult {
    if (scope.*) |*s| if (s.wasPidsExhausted()) return failed(.resource_kill);
    return failed(.runner_crash);
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
        .input_tokens = v.input_tokens,
        .cached_input_tokens = v.cached_input_tokens,
        .output_tokens = v.output_tokens,
        .wall_seconds = v.wall_seconds,
        .exit_ok = v.exit_ok,
        .failure = v.failure,
    };
}
