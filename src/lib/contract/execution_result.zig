//! execution_result.zig — the terminal result of one stage execution.
//!
//! Shared by both build graphs: the runner produces it (engine → child stdout
//! `result` frame → parent), and `agentsfleetd`'s `report` verb consumes it to write
//! the durable `core.fleet_events` row. One canonical type, so the runner's
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

/// Result of a single stage execution. Defaults describe a not-yet-run stage;
/// `exit_ok` flips true on a clean finish. `memory_peak_bytes`/`cpu_throttled_ms`
/// come from the child's cgroup (0 when unavailable, e.g. dev/macOS).
pub const ExecutionResult = struct {
    content: []const u8 = "",
    token_count: u64 = 0,
    wall_seconds: u64 = 0,
    exit_ok: bool = false,
    failure: ?FailureClass = null,
    /// Human-readable cause from the classification site (which check failed,
    /// and why). Defaults empty so an older peer's frame parses unchanged; the
    /// daemon treats any non-empty value as alloc-owned (same len-guarded free
    /// convention as `content`).
    failure_detail: []const u8 = "",
    memory_peak_bytes: u64 = 0,
    cpu_throttled_ms: u64 = 0,
    /// Cumulative token splits for the whole run (defaults 0: an older child
    /// omits them and the report settles run-fee-only — wire-compatible both
    /// directions). `cached_input_tokens` stays 0 until the fleet layer
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
    // Wire compatibility floor: an older peer that omits the field must parse
    // to this same empty default.
    try std.testing.expectEqualStrings("", r.failure_detail);
    // Split fields default 0 — an old-wire result parses to run-fee-only.
    try std.testing.expectEqual(@as(u64, 0), r.input_tokens);
    try std.testing.expectEqual(@as(u64, 0), r.cached_input_tokens);
    try std.testing.expectEqual(@as(u64, 0), r.output_tokens);
}
