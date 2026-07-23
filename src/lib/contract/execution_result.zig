//! execution_result.zig — the terminal result of one stage execution.
//!
//! Shared by both build graphs: the runner produces it (engine → child stdout
//! `result` frame → parent), and `agentsfleetd`'s `report` verb consumes it to write
//! the durable `core.fleet_events` row. One canonical type, so the runner's
//! output and the control plane's write can never drift (it superseded the
//! pre-cutover sidecar's `StageResult` at the M80 cutover).
//!
//! The verdict is a tagged union, not a flag beside an optional cause: a result
//! that claims success while naming a failure is unrepresentable, so the
//! "cause only on a failure" invariant is the compiler's job rather than a
//! conditional repeated at every consumer. The child `result` frame carries
//! this shape directly (parent and child are the same binary); the flat,
//! defaulted report wire that crosses versions is converted in exactly one
//! place — `report_mapping.zig`.

const std = @import("std");

/// Failure classification for an execution that did not complete cleanly.
/// A `failed` outcome carries one of these; the label is the durable
/// `failure_label`.
pub const FailureClass = enum {
    startup_posture,
    policy_deny,
    timeout_kill,
    oom_kill,
    resource_kill,
    runner_crash,
    transport_loss,
    landlock_deny,
    lease_expired,
    /// Killed by renewal policy — the control plane's `/renew` returned a
    /// definitive rejection (lease lost, max-runtime cap, or credit exhausted),
    /// so the run was stopped before completion. Distinct from `timeout_kill`
    /// (the wall-clock deadline elapsed) so triage and billing/analytics can
    /// tell a policy stop from a clock stop.
    renewal_terminate,
    /// Killed because the FLEET's own `daily_dollars`/`monthly_dollars` ceiling
    /// is reached — the control plane's `/renew` answered `UZ-RUN-015`. Distinct
    /// from `renewal_terminate` (a platform/billing stop) so an operator can
    /// answer "did my budget hold?" from the event row alone: a budget breach is
    /// the fleet author's own limit working, not a billing failure.
    budget_breach,

    pub fn label(self: FailureClass) []const u8 {
        return @tagName(self);
    }
};

test "budget_breach serialises as the exact durable failure_label" {
    // The wire + `core.fleet_events.failure_label` spelling is pinned here: the
    // docs, the gate label, and the runner all agree on this one string.
    try std.testing.expectEqualStrings("budget_breach", FailureClass.budget_breach.label());
    try std.testing.expectEqualStrings("renewal_terminate", FailureClass.renewal_terminate.label());
}

/// Result of a single stage execution. Defaults describe a not-yet-run stage —
/// an unrun stage is not a success. `memory_peak_bytes`/`cpu_throttled_ms` come
/// from the child's cgroup (0 when unavailable, e.g. dev/macOS).
pub const ExecutionResult = struct {
    outcome: Outcome = .{ .failed = .{} },
    content: []const u8 = "",
    token_count: u64 = 0,
    wall_seconds: u64 = 0,
    memory_peak_bytes: u64 = 0,
    cpu_throttled_ms: u64 = 0,
    /// Cumulative token splits for the whole run (defaults 0: an older child
    /// omits them and the report settles run-fee-only — wire-compatible both
    /// directions). `cached_input_tokens` stays 0 until the fleet layer
    /// surfaces cache reads separately from prompt tokens.
    input_tokens: u64 = 0,
    cached_input_tokens: u64 = 0,
    output_tokens: u64 = 0,

    /// The run's verdict. `completed` carries no cause because a clean run has
    /// none to carry.
    pub const Outcome = union(enum) {
        completed: Completed,
        failed: Failure,
    };

    /// A clean finish. Empty by construction — the run's numbers live on the
    /// result itself, shared by both verdicts.
    pub const Completed = struct {};

    /// Why a run failed. `class` is null only when the peer reported a failure
    /// without classifying it; a cause is never guessed from a bare failure.
    pub const Failure = struct {
        class: ?FailureClass = null,
        /// Human-readable cause from the classification site (which check
        /// failed, and why). Child-side values are static and serialized into
        /// the frame; parent-side values are alloc-owned and freed under the
        /// same len guard as `content`.
        detail: []const u8 = "",
    };

    /// A clean result carrying `content` and the run's numbers.
    pub fn completed(content: []const u8) ExecutionResult {
        return .{ .outcome = .{ .completed = .{} }, .content = content };
    }

    /// A failed result classified by `class`, with no cause line.
    pub fn failedWith(class: FailureClass) ExecutionResult {
        return .{ .outcome = .{ .failed = .{ .class = class } } };
    }

    pub fn succeeded(self: ExecutionResult) bool {
        return switch (self.outcome) {
            .completed => true,
            .failed => false,
        };
    }

    /// The failure payload, or null on a clean run — the only way to read a
    /// cause, so no consumer can pair one with a success.
    pub fn failure(self: ExecutionResult) ?Failure {
        return switch (self.outcome) {
            .completed => null,
            .failed => |f| f,
        };
    }

    /// The classified cause, or null when the run succeeded or the peer
    /// reported no classification.
    pub fn failureClass(self: ExecutionResult) ?FailureClass {
        const f = self.failure() orelse return null;
        return f.class;
    }

    /// The cause line, empty when the run succeeded or carried no detail.
    pub fn failureDetail(self: ExecutionResult) []const u8 {
        const f = self.failure() orelse return "";
        return f.detail;
    }
};

test "FailureClass.label returns the tag name for every variant" {
    const variants = [_]FailureClass{
        .startup_posture, .policy_deny,       .timeout_kill,   .oom_kill,
        .resource_kill,   .runner_crash,      .transport_loss, .landlock_deny,
        .lease_expired,   .renewal_terminate,
    };
    for (variants) |fc| try std.testing.expect(fc.label().len > 0);
    try std.testing.expectEqualStrings("oom_kill", FailureClass.oom_kill.label());
    try std.testing.expectEqualStrings("renewal_terminate", FailureClass.renewal_terminate.label());
}

test "ExecutionResult defaults describe an unrun stage" {
    const r = ExecutionResult{};
    try std.testing.expect(!r.succeeded());
    try std.testing.expectEqual(@as(u64, 0), r.token_count);
    // Unrun is a failure with nothing known about it — never a guessed class.
    try std.testing.expect(r.failureClass() == null);
    try std.testing.expectEqualStrings("", r.failureDetail());
    // Split fields default 0 — an old-wire result parses to run-fee-only.
    try std.testing.expectEqual(@as(u64, 0), r.input_tokens);
    try std.testing.expectEqual(@as(u64, 0), r.cached_input_tokens);
    try std.testing.expectEqual(@as(u64, 0), r.output_tokens);
}

test "a completed outcome exposes no failure and a failed one always carries its payload" {
    const ok = ExecutionResult.completed("done");
    try std.testing.expect(ok.succeeded());
    // The illegal pairing the old bool+optional encoding permitted — a success
    // that also names a cause — has no representation to test against here;
    // reading a cause off a clean result is only ever null.
    try std.testing.expect(ok.failure() == null);
    try std.testing.expectEqualStrings("", ok.failureDetail());

    const bad = ExecutionResult.failedWith(.oom_kill);
    try std.testing.expect(!bad.succeeded());
    try std.testing.expectEqual(FailureClass.oom_kill, bad.failureClass().?);
}

test "the result frame round-trips both verdicts through JSON" {
    // Parent and child are the same binary, so the frame carries the union
    // directly — this pins that std.json handles both variants.
    const alloc = std.testing.allocator;
    inline for (.{
        ExecutionResult.completed("hello"),
        ExecutionResult{ .outcome = .{ .failed = .{ .class = .startup_posture, .detail = "no instructions configured" } } },
    }) |original| {
        const json = try std.json.Stringify.valueAlloc(alloc, original, .{});
        defer alloc.free(json);
        const parsed = try std.json.parseFromSlice(ExecutionResult, alloc, json, .{ .ignore_unknown_fields = true });
        defer parsed.deinit();
        try std.testing.expectEqual(original.succeeded(), parsed.value.succeeded());
        try std.testing.expectEqualStrings(original.failureDetail(), parsed.value.failureDetail());
        try std.testing.expectEqualStrings(original.content, parsed.value.content);
    }
}
