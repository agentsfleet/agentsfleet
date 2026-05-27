//! execution_result.zig — the terminal result of one stage execution.
//!
//! Shared by both build graphs: the runner produces it (engine → child stdout
//! `result` frame → parent), and `zombied`'s `report` verb consumes it to write
//! the durable `core.zombie_events` row. One canonical type, so the runner's
//! output and the control plane's write can never drift (it superseded the
//! executor sidecar's `StageResult` at the M80 cutover).

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
    executor_crash,
    transport_loss,
    landlock_deny,
    lease_expired,

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
};

test "FailureClass.label returns the tag name for every variant" {
    const variants = [_]FailureClass{
        .startup_posture, .policy_deny,    .timeout_kill,   .oom_kill,
        .resource_kill,   .executor_crash, .transport_loss, .landlock_deny,
        .lease_expired,
    };
    for (variants) |fc| try std.testing.expect(fc.label().len > 0);
    try std.testing.expectEqualStrings("oom_kill", FailureClass.oom_kill.label());
}

test "ExecutionResult defaults describe an unrun stage" {
    const r = ExecutionResult{};
    try std.testing.expect(!r.exit_ok);
    try std.testing.expectEqual(@as(u64, 0), r.token_count);
    try std.testing.expect(r.failure == null);
}
