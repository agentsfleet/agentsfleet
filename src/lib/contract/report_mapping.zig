//! report_mapping.zig — the one place the domain execution result and the flat
//! report wire convert into each other.
//!
//! The runner produces an `ExecutionResult`, ships it as a `ReportRequest`, and
//! `agentsfleetd` rebuilds an `ExecutionResult` to write the durable row. Those
//! two translations used to live at their call sites, field by field, so every
//! new field had to be threaded through both halves by hand and silently
//! vanished if either was missed. They live here instead, proven by a
//! round-trip test: a field that reaches the wire and back is a field neither
//! side can drop.
//!
//! The wire stays flat and defaulted because it is the genuine cross-version
//! boundary (an older runner reports to a newer control plane); the domain type
//! stays precise because nothing in-process benefits from illegal states. The
//! trust boundary — a cause never accompanies a clean outcome — is structural
//! here: `processed` maps onto a variant with nowhere to put one.

const std = @import("std");
const protocol = @import("protocol.zig");
const execution_result = @import("execution_result.zig");

const ExecutionResult = execution_result.ExecutionResult;

/// What a report carries that the result itself does not know: which lease and
/// event it settles, the measured wall time, the session checkpoint, and the
/// cumulative splits already saturated onto the wire's frozen u32 width by the
/// caller (the runner owns that one width policy).
pub const ReportContext = struct {
    lease_id: []const u8,
    event_id: []const u8,
    fencing_token: u64,
    wall_ms: u64,
    input_tokens: u32 = 0,
    cached_input_tokens: u32 = 0,
    output_tokens: u32 = 0,
    time_to_first_token_ms: u32 = 0,
    checkpoint_response: []const u8 = "",
};

/// Project a finished execution onto the report wire.
pub fn toReport(result: ExecutionResult, ctx: ReportContext) protocol.ReportRequest {
    const failure = result.failure();
    return .{
        .lease_id = ctx.lease_id,
        .event_id = ctx.event_id,
        .fencing_token = ctx.fencing_token,
        .outcome = if (result.succeeded()) .processed else .fleet_error,
        .failure_reason = if (failure) |f| f.class else null,
        .failure_detail = if (failure) |f| f.detail else "",
        .response_text = result.content,
        .tokens = result.token_count,
        .input_tokens = ctx.input_tokens,
        .cached_input_tokens = ctx.cached_input_tokens,
        .output_tokens = ctx.output_tokens,
        .telemetry = .{ .time_to_first_token_ms = ctx.time_to_first_token_ms, .wall_ms = ctx.wall_ms },
        .checkpoint = .{ .last_event_id = ctx.event_id, .last_response = ctx.checkpoint_response },
    };
}

/// Rebuild the domain result from a reported body. A `processed` outcome has
/// nowhere to carry a cause, so a misbehaving runner that pairs one with a
/// clean verdict loses it here rather than persisting a contradiction.
pub fn fromReport(body: protocol.ReportRequest) ExecutionResult {
    return .{
        .outcome = switch (body.outcome) {
            .processed => .{ .completed = .{} },
            .fleet_error => .{ .failed = .{ .class = body.failure_reason, .detail = body.failure_detail } },
        },
        .content = body.response_text,
        .token_count = body.tokens,
        .wall_seconds = body.telemetry.wall_ms / std.time.ms_per_s,
        .input_tokens = body.input_tokens,
        .cached_input_tokens = body.cached_input_tokens,
        .output_tokens = body.output_tokens,
    };
}

// ── Tests ───────────────────────────────────────────────────────────────────

const TEST_CTX = ReportContext{
    .lease_id = "lease-1",
    .event_id = "event-1",
    .fencing_token = 7,
    .wall_ms = 4_000,
    .input_tokens = 11,
    .cached_input_tokens = 2,
    .output_tokens = 5,
};

test "round-trip preserves every wire field for a failed run" {
    const original = ExecutionResult{
        .outcome = .{ .failed = .{ .class = .startup_posture, .detail = "no instructions configured" } },
        .content = "partial output",
        .token_count = 321,
        .input_tokens = 11,
        .cached_input_tokens = 2,
        .output_tokens = 5,
    };
    const back = fromReport(toReport(original, TEST_CTX));
    try std.testing.expect(!back.succeeded());
    try std.testing.expectEqual(execution_result.FailureClass.startup_posture, back.failureClass().?);
    try std.testing.expectEqualStrings("no instructions configured", back.failureDetail());
    try std.testing.expectEqualStrings(original.content, back.content);
    try std.testing.expectEqual(original.token_count, back.token_count);
    try std.testing.expectEqual(original.input_tokens, back.input_tokens);
    try std.testing.expectEqual(original.cached_input_tokens, back.cached_input_tokens);
    try std.testing.expectEqual(original.output_tokens, back.output_tokens);
    // Wall time crosses as milliseconds and lands back in seconds.
    try std.testing.expectEqual(@as(u64, 4), back.wall_seconds);
}

test "round-trip preserves a clean run and carries no cause" {
    const back = fromReport(toReport(ExecutionResult.completed("done"), TEST_CTX));
    try std.testing.expect(back.succeeded());
    try std.testing.expect(back.failure() == null);
    try std.testing.expectEqualStrings("done", back.content);
}

test "a processed body drops a cause a misbehaving runner paired with it" {
    // The old encoding let this contradiction reach the durable row; the
    // `completed` variant has no field to put it in.
    var body = toReport(ExecutionResult.completed("done"), TEST_CTX);
    body.failure_reason = .oom_kill;
    body.failure_detail = "should not survive";
    const back = fromReport(body);
    try std.testing.expect(back.succeeded());
    try std.testing.expect(back.failureClass() == null);
    try std.testing.expectEqualStrings("", back.failureDetail());
}

test "an unclassified failure survives as a failure with no guessed cause" {
    var body = toReport(ExecutionResult{}, TEST_CTX);
    body.failure_reason = null;
    const back = fromReport(body);
    try std.testing.expect(!back.succeeded());
    try std.testing.expect(back.failureClass() == null);
}

test "the report outcome follows the verdict, not a separate flag" {
    try std.testing.expectEqual(protocol.Outcome.processed, toReport(ExecutionResult.completed(""), TEST_CTX).outcome);
    try std.testing.expectEqual(protocol.Outcome.fleet_error, toReport(ExecutionResult.failedWith(.timeout_kill), TEST_CTX).outcome);
}
