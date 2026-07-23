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
    return ExecutionResult.failedWith(class);
}

// Cause lines for parent-side classifications. Always passed through
// `failedDetailed` so the daemon's ownership convention holds: any non-empty
// `failure_detail` it sees is alloc-owned.
pub const DETAIL_LEASE_SERIALIZE = "failed to serialize the lease for the child";
pub const DETAIL_SANDBOX_UNAVAILABLE = "sandbox could not be established on this runner";
pub const DETAIL_EGRESS_UNIMPLEMENTED = "strict egress policy is not implemented on this runner";
pub const DETAIL_CGROUP_ENROLL = "the child could not be enrolled in the resource-control domain";
const DETAIL_SANDBOX_ABORT = "sandbox setup aborted before the fleet started";
const DETAIL_SECCOMP_TRAP = "a denylisted syscall was trapped by the sandbox";

/// `failed` carrying an alloc-owned cause line — caller must free
/// `failure_detail` (the daemon's len-guarded convention). A dupe failure
/// degrades to the detail-less shape.
pub fn failedDetailed(alloc: std.mem.Allocator, class: types.FailureClass, detail: []const u8) ExecutionResult {
    const owned = alloc.dupe(u8, detail) catch return failed(class);
    return .{ .outcome = .{ .failed = .{ .class = class, .detail = owned } } };
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
            pipe_proto.SANDBOX_FAIL_EXIT => failedDetailed(alloc, .startup_posture, DETAIL_SANDBOX_ABORT),
            pipe_proto.SECCOMP_VIOLATION_EXIT => failedDetailed(alloc, .landlock_deny, DETAIL_SECCOMP_TRAP),
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
    // The cause line is duped off the parse arena like `content`; an absent one
    // stays un-alloc'd, matching the len-guarded free at the consumer.
    const outcome: ExecutionResult.Outcome = switch (v.outcome) {
        .completed => .{ .completed = .{} },
        .failed => |f| .{ .failed = .{
            .class = f.class,
            .detail = if (f.detail.len > 0) alloc.dupe(u8, f.detail) catch "" else "",
        } },
    };
    return .{
        .outcome = outcome,
        .content = content,
        .token_count = v.token_count,
        .input_tokens = v.input_tokens,
        .cached_input_tokens = v.cached_input_tokens,
        .output_tokens = v.output_tokens,
        .wall_seconds = v.wall_seconds,
    };
}

// ── Tests ───────────────────────────────────────────────────────────────────

test "classify parses a result frame missing failure_detail to the empty default" {
    // Wire compatibility: an older child omits the field; the parse must land
    // on the struct default, not an error — and nothing extra is allocated.
    var scope: ?cgroup = null;
    const bytes = try std.testing.allocator.dupe(u8, "{\"outcome\":{\"completed\":{}},\"content\":\"done\"}");
    defer std.testing.allocator.free(bytes);
    const r = classify(std.testing.allocator, .{ .bytes = bytes }, .{ .exited = 0 }, &scope);
    defer if (r.content.len > 0) std.testing.allocator.free(r.content);
    try std.testing.expect(r.succeeded());
    try std.testing.expectEqualStrings("", r.failureDetail());
}

test "classify dupes a frame-carried failure_detail off the parse arena" {
    var scope: ?cgroup = null;
    const bytes = try std.testing.allocator.dupe(
        u8,
        "{\"outcome\":{\"failed\":{\"class\":\"startup_posture\",\"detail\":\"no instructions configured\"}}}",
    );
    defer std.testing.allocator.free(bytes);
    const r = classify(std.testing.allocator, .{ .bytes = bytes }, .{ .exited = 0 }, &scope);
    defer if (r.failureDetail().len > 0) std.testing.allocator.free(r.failureDetail());
    try std.testing.expectEqual(types.FailureClass.startup_posture, r.failureClass().?);
    try std.testing.expectEqualStrings("no instructions configured", r.failureDetail());
}

test "sandbox abort exit classifies startup_posture with an owned cause line" {
    var scope: ?cgroup = null;
    const r = classify(std.testing.allocator, .{}, .{ .exited = pipe_proto.SANDBOX_FAIL_EXIT }, &scope);
    defer if (r.failureDetail().len > 0) std.testing.allocator.free(r.failureDetail());
    try std.testing.expectEqual(types.FailureClass.startup_posture, r.failureClass().?);
    try std.testing.expectEqualStrings(DETAIL_SANDBOX_ABORT, r.failureDetail());
}

test "failedDetailed dupes the cause; failed leaves it empty" {
    const detailed = failedDetailed(std.testing.allocator, .startup_posture, DETAIL_LEASE_SERIALIZE);
    defer if (detailed.failureDetail().len > 0) std.testing.allocator.free(detailed.failureDetail());
    try std.testing.expectEqualStrings(DETAIL_LEASE_SERIALIZE, detailed.failureDetail());
    // The dupe means the result never borrows the caller's buffer.
    try std.testing.expect(detailed.failureDetail().ptr != DETAIL_LEASE_SERIALIZE.ptr);
    try std.testing.expectEqualStrings("", failed(.runner_crash).failureDetail());
}
