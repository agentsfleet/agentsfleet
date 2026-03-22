const std = @import("std");
const posthog_events = @import("posthog_events.zig");
const posthog = @import("posthog");

test "unit: distinctIdOrSystem falls back to system" {
    try std.testing.expectEqualStrings("system", posthog_events.distinctIdOrSystem(""));
    try std.testing.expectEqualStrings("user_123", posthog_events.distinctIdOrSystem("user_123"));
}

test "integration: telemetry helpers are no-op when posthog client is disabled" {
    const disabled: ?*posthog.PostHogClient = null;
    posthog_events.trackRunStarted(disabled, "u", "run_1", "ws_1", "spec_1", "auto", "req_1");
    posthog_events.trackRunRetried(disabled, "u", "run_1", "ws_1", 2, "req_1");
    posthog_events.trackRunCompleted(disabled, "u", "run_1", "ws_1", "passed", 42);
    posthog_events.trackRunFailed(disabled, "u", "run_1", "ws_1", "blocked", 42);
    posthog_events.trackAgentCompleted(disabled, "u", "run_1", "ws_1", "Echo", 10, 50, "ok");
    posthog_events.trackAgentRunScored(disabled, "u", "run_1", "ws_1", "agent_1", 95, "ELITE", "m9_v1", "{}", "{}", 42, 100, 100, 100, 50);
    posthog_events.trackEntitlementRejected(disabled, "u", "ws_1", "COMPILE", "ERR_ENTITLEMENT_STAGE_LIMIT", "req_1");
    posthog_events.trackProfileActivated(disabled, "u", "ws_1", "prof_1", "ver_1", "ver_1", "req_1");
    posthog_events.trackBillingLifecycleEvent(disabled, "u", "ws_1", "PAYMENT_FAILED", "invoice_failed", "SCALE", "GRACE", "req_1");
    posthog_events.trackAgentTrustEarned(disabled, "u", "run_1", "ws_1", "agent_1", 10);
    posthog_events.trackAgentTrustLost(disabled, "u", "run_1", "ws_1", "agent_1", 0);
    posthog_events.trackAgentHarnessChanged(disabled, "u", "agent_1", "proposal_1", "ws_1", "AUTO", "DECLINING_SCORE", &[_][]const u8{"stage_insert"});
    posthog_events.trackAgentImprovementStalled(disabled, "u", "run_1", "ws_1", "agent_1", "proposal_1", 3);
    posthog_events.trackServerStarted(disabled, 3000, 4);
    posthog_events.trackWorkerStarted(disabled, 4);
    posthog_events.trackStartupFailed(disabled, "serve", "db_connect", "connection_refused", "UZ-STARTUP-003");
    posthog_events.trackWorkspaceCreated(disabled, "u", "ws_1", "t_1", "https://github.com/org/repo", "req_1");
    posthog_events.trackWorkspaceGithubConnected(disabled, "ws_1", "12345", "req_1");
    posthog_events.trackAuthLoginCompleted(disabled, "sess_1", "req_1");
    posthog_events.trackAuthRejected(disabled, "token_expired", "req_1");
    posthog_events.trackApiError(disabled, "u", "UZ-BILLING-001", "invalid subscription", "req_1");
    posthog_events.trackApiErrorWithContext(disabled, "u", "UZ-ENTL-003", "stage limit reached", "ws_1", "req_1");
    try std.testing.expect(true);
}

test "server started payload includes port and worker concurrency" {
    const props = posthog_events.serverStartedProps(3000, 4);
    try std.testing.expectEqual(@as(usize, 2), props.len);
    try std.testing.expectEqualStrings("port", props[0].key);
    try std.testing.expectEqual(@as(i64, 3000), props[0].value.integer);
    try std.testing.expectEqualStrings("worker_concurrency", props[1].key);
    try std.testing.expectEqual(@as(i64, 4), props[1].value.integer);
}

test "startup failed payload includes command phase reason and error code" {
    const props = posthog_events.startupFailedProps("worker", "db_connect", "connection_refused", "UZ-STARTUP-003");
    try std.testing.expectEqual(@as(usize, 4), props.len);
    try std.testing.expectEqualStrings("command", props[0].key);
    try std.testing.expectEqualStrings("worker", props[0].value.string);
    try std.testing.expectEqualStrings("phase", props[1].key);
    try std.testing.expectEqualStrings("db_connect", props[1].value.string);
    try std.testing.expectEqualStrings("reason", props[2].key);
    try std.testing.expectEqualStrings("connection_refused", props[2].value.string);
    try std.testing.expectEqualStrings("error_code", props[3].key);
    try std.testing.expectEqualStrings("UZ-STARTUP-003", props[3].value.string);
}

test "agent run scored payload includes structured and flat scoring fields" {
    const props = posthog_events.agentRunScoredProps(
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

test "trust transition payload includes required event fields" {
    const props = posthog_events.trustTransitionProps("run_7", "ws_7", "agent_7", 10);

    try std.testing.expectEqual(@as(usize, 4), props.len);
    try std.testing.expectEqualStrings("run_id", props[0].key);
    try std.testing.expectEqualStrings("workspace_id", props[1].key);
    try std.testing.expectEqualStrings("agent_id", props[2].key);
    try std.testing.expectEqualStrings("consecutive_count_at_event", props[3].key);
    try std.testing.expectEqualStrings("agent_7", props[2].value.string);
    try std.testing.expectEqual(@as(i64, 10), props[3].value.integer);
}
