const std = @import("std");
const posthog = @import("posthog");

pub fn distinctIdOrSystem(raw: []const u8) []const u8 {
    if (raw.len == 0) return "system";
    return raw;
}

pub fn trackRunStarted(
    client: ?*posthog.PostHogClient,
    distinct_id: []const u8,
    run_id: []const u8,
    workspace_id: []const u8,
    spec_id: []const u8,
    mode: []const u8,
    request_id: []const u8,
) void {
    if (client) |ph| {
        const props = [_]posthog.Property{
            .{ .key = "run_id", .value = .{ .string = run_id } },
            .{ .key = "workspace_id", .value = .{ .string = workspace_id } },
            .{ .key = "spec_id", .value = .{ .string = spec_id } },
            .{ .key = "mode", .value = .{ .string = mode } },
            .{ .key = "request_id", .value = .{ .string = request_id } },
        };
        ph.capture(.{
            .distinct_id = distinct_id,
            .event = "run_started",
            .properties = &props,
        }) catch {};
    }
}

pub fn trackRunRetried(
    client: ?*posthog.PostHogClient,
    distinct_id: []const u8,
    run_id: []const u8,
    workspace_id: []const u8,
    attempt: u32,
    request_id: []const u8,
) void {
    if (client) |ph| {
        const props = [_]posthog.Property{
            .{ .key = "run_id", .value = .{ .string = run_id } },
            .{ .key = "workspace_id", .value = .{ .string = workspace_id } },
            .{ .key = "attempt", .value = .{ .integer = @intCast(attempt) } },
            .{ .key = "request_id", .value = .{ .string = request_id } },
        };
        ph.capture(.{
            .distinct_id = distinct_id,
            .event = "run_retried",
            .properties = &props,
        }) catch {};
    }
}

pub fn trackRunCompleted(
    client: ?*posthog.PostHogClient,
    distinct_id: []const u8,
    run_id: []const u8,
    workspace_id: []const u8,
    verdict: []const u8,
    duration_ms: u64,
) void {
    if (client) |ph| {
        const props = [_]posthog.Property{
            .{ .key = "run_id", .value = .{ .string = run_id } },
            .{ .key = "workspace_id", .value = .{ .string = workspace_id } },
            .{ .key = "verdict", .value = .{ .string = verdict } },
            .{ .key = "duration_ms", .value = .{ .integer = @intCast(duration_ms) } },
        };
        ph.capture(.{
            .distinct_id = distinct_id,
            .event = "run_completed",
            .properties = &props,
        }) catch {};
    }
}

pub fn trackRunFailed(
    client: ?*posthog.PostHogClient,
    distinct_id: []const u8,
    run_id: []const u8,
    workspace_id: []const u8,
    reason: []const u8,
    duration_ms: u64,
) void {
    if (client) |ph| {
        const props = [_]posthog.Property{
            .{ .key = "run_id", .value = .{ .string = run_id } },
            .{ .key = "workspace_id", .value = .{ .string = workspace_id } },
            .{ .key = "reason", .value = .{ .string = reason } },
            .{ .key = "duration_ms", .value = .{ .integer = @intCast(duration_ms) } },
        };
        ph.capture(.{
            .distinct_id = distinct_id,
            .event = "run_failed",
            .properties = &props,
        }) catch {};
    }
}

pub fn trackAgentCompleted(
    client: ?*posthog.PostHogClient,
    distinct_id: []const u8,
    run_id: []const u8,
    workspace_id: []const u8,
    actor: []const u8,
    tokens: u64,
    duration_ms: u64,
    exit_status: []const u8,
) void {
    if (client) |ph| {
        const props = [_]posthog.Property{
            .{ .key = "run_id", .value = .{ .string = run_id } },
            .{ .key = "workspace_id", .value = .{ .string = workspace_id } },
            .{ .key = "actor", .value = .{ .string = actor } },
            .{ .key = "tokens", .value = .{ .integer = @intCast(tokens) } },
            .{ .key = "duration_ms", .value = .{ .integer = @intCast(duration_ms) } },
            .{ .key = "exit_status", .value = .{ .string = exit_status } },
        };
        ph.capture(.{
            .distinct_id = distinct_id,
            .event = "agent_completed",
            .properties = &props,
        }) catch {};
    }
}

pub fn trackEntitlementRejected(
    client: ?*posthog.PostHogClient,
    distinct_id: []const u8,
    workspace_id: []const u8,
    boundary: []const u8,
    reason_code: []const u8,
    request_id: []const u8,
) void {
    if (client) |ph| {
        const props = [_]posthog.Property{
            .{ .key = "workspace_id", .value = .{ .string = workspace_id } },
            .{ .key = "boundary", .value = .{ .string = boundary } },
            .{ .key = "reason_code", .value = .{ .string = reason_code } },
            .{ .key = "request_id", .value = .{ .string = request_id } },
        };
        ph.capture(.{
            .distinct_id = distinct_id,
            .event = "entitlement_rejected",
            .properties = &props,
        }) catch {};
    }
}

pub fn trackProfileActivated(
    client: ?*posthog.PostHogClient,
    distinct_id: []const u8,
    workspace_id: []const u8,
    agent_id: []const u8,
    config_version_id: []const u8,
    run_snapshot_version: []const u8,
    request_id: []const u8,
) void {
    if (client) |ph| {
        const props = [_]posthog.Property{
            .{ .key = "workspace_id", .value = .{ .string = workspace_id } },
            .{ .key = "agent_id", .value = .{ .string = agent_id } },
            .{ .key = "config_version_id", .value = .{ .string = config_version_id } },
            .{ .key = "run_snapshot_version", .value = .{ .string = run_snapshot_version } },
            .{ .key = "request_id", .value = .{ .string = request_id } },
        };
        ph.capture(.{
            .distinct_id = distinct_id,
            .event = "profile_activated",
            .properties = &props,
        }) catch {};
    }
}

pub fn trackBillingLifecycleEvent(
    client: ?*posthog.PostHogClient,
    distinct_id: []const u8,
    workspace_id: []const u8,
    event_type: []const u8,
    reason: []const u8,
    plan_tier: []const u8,
    billing_status: []const u8,
    request_id: []const u8,
) void {
    if (client) |ph| {
        const props = [_]posthog.Property{
            .{ .key = "workspace_id", .value = .{ .string = workspace_id } },
            .{ .key = "event_type", .value = .{ .string = event_type } },
            .{ .key = "reason", .value = .{ .string = reason } },
            .{ .key = "plan_tier", .value = .{ .string = plan_tier } },
            .{ .key = "billing_status", .value = .{ .string = billing_status } },
            .{ .key = "request_id", .value = .{ .string = request_id } },
        };
        ph.capture(.{
            .distinct_id = distinct_id,
            .event = "billing_lifecycle_event",
            .properties = &props,
        }) catch {};
    }
}

pub fn trackAgentRunScored(
    client: ?*posthog.PostHogClient,
    distinct_id: []const u8,
    run_id: []const u8,
    workspace_id: []const u8,
    agent_id: []const u8,
    score: u8,
    tier: []const u8,
    formula_version: []const u8,
    axis_scores_json: []const u8,
    weight_snapshot_json: []const u8,
    scored_at: i64,
    axis_completion: u8,
    axis_error_rate: u8,
    axis_latency: u8,
    axis_resource: u8,
) void {
    if (client) |ph| {
        const props = agentRunScoredProps(
            run_id,
            workspace_id,
            agent_id,
            score,
            tier,
            formula_version,
            axis_scores_json,
            weight_snapshot_json,
            scored_at,
            axis_completion,
            axis_error_rate,
            axis_latency,
            axis_resource,
        );
        ph.capture(.{
            .distinct_id = distinct_id,
            .event = "agent.run.scored",
            .properties = &props,
        }) catch {};
    }
}

fn agentRunScoredProps(
    run_id: []const u8,
    workspace_id: []const u8,
    agent_id: []const u8,
    score: u8,
    tier: []const u8,
    formula_version: []const u8,
    axis_scores_json: []const u8,
    weight_snapshot_json: []const u8,
    scored_at: i64,
    axis_completion: u8,
    axis_error_rate: u8,
    axis_latency: u8,
    axis_resource: u8,
) [13]posthog.Property {
    return .{
        .{ .key = "run_id", .value = .{ .string = run_id } },
        .{ .key = "workspace_id", .value = .{ .string = workspace_id } },
        .{ .key = "agent_id", .value = .{ .string = agent_id } },
        .{ .key = "score", .value = .{ .integer = @intCast(score) } },
        .{ .key = "tier", .value = .{ .string = tier } },
        .{ .key = "score_formula_version", .value = .{ .string = formula_version } },
        .{ .key = "axis_scores", .value = .{ .string = axis_scores_json } },
        .{ .key = "weight_snapshot", .value = .{ .string = weight_snapshot_json } },
        .{ .key = "scored_at", .value = .{ .integer = scored_at } },
        .{ .key = "axis_completion", .value = .{ .integer = @intCast(axis_completion) } },
        .{ .key = "axis_error_rate", .value = .{ .integer = @intCast(axis_error_rate) } },
        .{ .key = "axis_latency", .value = .{ .integer = @intCast(axis_latency) } },
        .{ .key = "axis_resource", .value = .{ .integer = @intCast(axis_resource) } },
    };
}

pub fn trackAgentScoringFailed(
    client: ?*posthog.PostHogClient,
    distinct_id: []const u8,
    run_id: []const u8,
    workspace_id: []const u8,
    err_name: []const u8,
) void {
    if (client) |ph| {
        const props = [_]posthog.Property{
            .{ .key = "run_id", .value = .{ .string = run_id } },
            .{ .key = "workspace_id", .value = .{ .string = workspace_id } },
            .{ .key = "error", .value = .{ .string = err_name } },
        };
        ph.capture(.{
            .distinct_id = distinct_id,
            .event = "agent.scoring.failed",
            .properties = &props,
        }) catch {};
    }
}

test "unit: distinctIdOrSystem falls back to system" {
    try std.testing.expectEqualStrings("system", distinctIdOrSystem(""));
    try std.testing.expectEqualStrings("user_123", distinctIdOrSystem("user_123"));
}

test "integration: telemetry helpers are no-op when posthog client is disabled" {
    const disabled: ?*posthog.PostHogClient = null;
    trackRunStarted(disabled, "u", "run_1", "ws_1", "spec_1", "auto", "req_1");
    trackRunRetried(disabled, "u", "run_1", "ws_1", 2, "req_1");
    trackRunCompleted(disabled, "u", "run_1", "ws_1", "passed", 42);
    trackRunFailed(disabled, "u", "run_1", "ws_1", "blocked", 42);
    trackAgentCompleted(disabled, "u", "run_1", "ws_1", "Echo", 10, 50, "ok");
    trackAgentRunScored(disabled, "u", "run_1", "ws_1", "agent_1", 95, "ELITE", "m9_v1", "{}", "{}", 42, 100, 100, 100, 50);
    trackEntitlementRejected(disabled, "u", "ws_1", "COMPILE", "ERR_ENTITLEMENT_STAGE_LIMIT", "req_1");
    trackProfileActivated(disabled, "u", "ws_1", "prof_1", "ver_1", "ver_1", "req_1");
    trackBillingLifecycleEvent(disabled, "u", "ws_1", "PAYMENT_FAILED", "invoice_failed", "SCALE", "GRACE", "req_1");
    try std.testing.expect(true);
}

test "agent run scored payload includes structured and flat scoring fields" {
    const props = agentRunScoredProps(
        "run_1",
        "ws_1",
        "agent_1",
        91,
        "ELITE",
        "m9_v1",
        "{\"completion\":100}",
        "{\"completion\":0.4}",
        1234,
        100,
        90,
        80,
        50,
    );

    try std.testing.expectEqual(@as(usize, 13), props.len);
    try std.testing.expectEqualStrings("run_id", props[0].key);
    try std.testing.expectEqualStrings("score_formula_version", props[5].key);
    try std.testing.expectEqualStrings("axis_scores", props[6].key);
    try std.testing.expectEqualStrings("weight_snapshot", props[7].key);
    try std.testing.expectEqualStrings("scored_at", props[8].key);
    try std.testing.expectEqualStrings("axis_resource", props[12].key);
    try std.testing.expectEqualStrings("ELITE", props[4].value.string);
    try std.testing.expectEqualStrings("m9_v1", props[5].value.string);
    try std.testing.expectEqualStrings("{\"completion\":100}", props[6].value.string);
    try std.testing.expectEqual(@as(i64, 1234), props[8].value.integer);
    try std.testing.expectEqual(@as(i64, 50), props[12].value.integer);
}
