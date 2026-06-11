//! Terminal result mapping for child_supervisor — the pure, security-free tail
//! of supervision: how a child's read outcome + exit status become an
//! `ExecutionResult`. Split out of `child_supervisor.zig` to keep that file
//! within the line budget; the supervisor re-exports `ReadOutcome`/`classify`.

const std = @import("std");
const types = @import("engine/types.zig");
const cgroup = @import("engine/cgroup.zig");

const ExecutionResult = types.ExecutionResult;

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

/// A failed execution with no body — the supervisor's universal "something went
/// wrong" outcome, classified by `class`.
pub fn failed(class: types.FailureClass) ExecutionResult {
    return .{ .exit_ok = false, .failure = class };
}

/// Map the child's exit status + read outcome to an `ExecutionResult`.
/// Precedence: renewal-terminate → deadline timeout → OOM (cgroup) → clean exit
/// (parse result) → abnormal exit/signal (crash). A renewal `.terminate` (lease
/// lost / capped / no credits) is `renewal_terminate` — a policy stop, kept
/// distinct from `timeout_kill` (the wall-clock deadline) so triage and billing
/// can tell a policy kill from a clock kill. Terminate wins over a co-occurring
/// timeout: the policy reason is the more actionable cause.
pub fn classify(
    alloc: std.mem.Allocator,
    outcome: ReadOutcome,
    term: std.process.Child.Term,
    scope: *?cgroup.CgroupScope,
) ExecutionResult {
    if (outcome.terminated) return failed(.renewal_terminate);
    if (outcome.timed_out) return failed(.timeout_kill);
    if (scope.*) |*s| if (s.wasOomKilled()) return failed(.oom_kill);
    // Zig 0.16 reap returns a typed Term, not a raw wait() status word. A clean
    // run is a zero exit; a non-zero exit or a signal/stop is a crash.
    const clean_exit = switch (term) {
        .exited => |code| code == 0,
        else => false,
    };
    if (!clean_exit) return failed(.runner_crash);
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
        .input_tokens = v.input_tokens,
        .cached_input_tokens = v.cached_input_tokens,
        .output_tokens = v.output_tokens,
        .wall_seconds = v.wall_seconds,
        .exit_ok = v.exit_ok,
        .failure = v.failure,
    };
}
