//! execution_result.zig — the terminal result of one stage execution.
//!
//! Shared by both build graphs: the runner produces it (engine → child stdout
//! `result` frame → parent), and `agentsfleetd`'s `report` verb consumes it to write
//! the durable `core.zombie_events` row. One canonical type, so the runner's
//! output and the control plane's write can never drift (it superseded the
//! pre-cutover sidecar's `StageResult` at the M80 cutover).

const std = @import("std");

/// Failure classification for an execution that did not complete cleanly.
/// `exit_ok == false` carries one of these; the label is the durable
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

    pub fn label(self: FailureClass) []const u8 {
        return @tagName(self);
    }
};

/// Result of a single stage execution. Defaults describe a not-yet-run stage;
/// `exit_ok` flips true on a clean finish. `memory_peak_bytes`/`cpu_throttled_ms`
/// come from the child's cgroup (0 when unavailable, e.g. dev/macOS).
pub const ExecutionResult = struct {
    content: []const u8 = "",
    token_count: u64 = 0,
    wall_seconds: u64 = 0,
    exit_ok: bool = false,
    failure: ?FailureClass = null,
    memory_peak_bytes: u64 = 0,
    cpu_throttled_ms: u64 = 0,
    /// Cumulative token splits for the whole run (defaults 0: an older child
    /// omits them and the report settles run-fee-only — wire-compatible both
    /// directions). `cached_input_tokens` stays 0 until the agent layer
    /// surfaces cache reads separately from prompt tokens.
    input_tokens: u64 = 0,
    cached_input_tokens: u64 = 0,
    output_tokens: u64 = 0,
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
    try std.testing.expect(!r.exit_ok);
    try std.testing.expectEqual(@as(u64, 0), r.token_count);
    try std.testing.expect(r.failure == null);
    // Split fields default 0 — an old-wire result parses to run-fee-only.
    try std.testing.expectEqual(@as(u64, 0), r.input_tokens);
    try std.testing.expectEqual(@as(u64, 0), r.cached_input_tokens);
    try std.testing.expectEqual(@as(u64, 0), r.output_tokens);
}
