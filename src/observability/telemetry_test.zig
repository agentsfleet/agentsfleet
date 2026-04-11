const std = @import("std");
const telemetry = @import("telemetry.zig");

test "unit: distinctIdOrSystem falls back to system" {
    try std.testing.expectEqualStrings("system", telemetry.distinctIdOrSystem(""));
    try std.testing.expectEqualStrings("user_123", telemetry.distinctIdOrSystem("user_123"));
}

test "capture RunOrphanRecovered records correct kind" {
    var t = telemetry.Telemetry.initTest();
    t.capture(telemetry.RunOrphanRecovered, .{
        .distinct_id = "u1",
        .run_id = "run_1",
        .workspace_id = "ws_1",
        .staleness_ms = 5000,
    });
    try telemetry.TestBackend.assertLastEventIs(.run_orphan_recovered);
}

test "capture multiple events increments count" {
    var t = telemetry.Telemetry.initTest();
    t.capture(telemetry.ServerStarted, .{ .port = 3000 });
    t.capture(telemetry.WorkerStarted, .{ .concurrency = 4 });
    t.capture(telemetry.AuthLoginCompleted, .{ .session_id = "s1", .request_id = "r1" });
    try telemetry.TestBackend.assertCount(3);
}

test "reset clears ring and lastEvent returns null" {
    var t = telemetry.Telemetry.initTest();
    t.capture(telemetry.ServerStarted, .{ .port = 8080 });
    try telemetry.TestBackend.assertCount(1);
    telemetry.TestBackend.reset();
    try std.testing.expect(telemetry.TestBackend.lastEvent() == null);
    try telemetry.TestBackend.assertCount(0);
}

test "workspace_id captured for events that have it" {
    var t = telemetry.Telemetry.initTest();
    t.capture(telemetry.EntitlementRejected, .{
        .distinct_id = "u1",
        .workspace_id = "ws_42",
        .boundary = "COMPILE",
        .reason_code = "ERR",
        .request_id = "r1",
    });
    const last = telemetry.TestBackend.lastEvent().?;
    try std.testing.expectEqualStrings("ws_42", last.workspace_id);
}

test "events without workspace_id capture empty string" {
    var t = telemetry.Telemetry.initTest();
    t.capture(telemetry.ServerStarted, .{ .port = 3000 });
    const last = telemetry.TestBackend.lastEvent().?;
    try std.testing.expectEqualStrings("", last.workspace_id);
}

test "distinct_id defaults to system for events without it" {
    var t = telemetry.Telemetry.initTest();
    t.capture(telemetry.ServerStarted, .{ .port = 3000 });
    const last = telemetry.TestBackend.lastEvent().?;
    try std.testing.expectEqualStrings("system", last.distinct_id);
}

test "all 14 event types can be captured without error" {
    var t = telemetry.Telemetry.initTest();
    t.capture(telemetry.AgentCompleted, .{ .distinct_id = "u", .run_id = "r", .workspace_id = "w", .actor = "a", .tokens = 10, .duration_ms = 50, .exit_status = "ok" });
    t.capture(telemetry.EntitlementRejected, .{ .distinct_id = "u", .workspace_id = "w", .boundary = "COMPILE", .reason_code = "ERR", .request_id = "r" });
    t.capture(telemetry.ProfileActivated, .{ .distinct_id = "u", .workspace_id = "w", .agent_id = "a", .config_version_id = "v", .run_snapshot_version = "v", .request_id = "r" });
    t.capture(telemetry.BillingLifecycleEvent, .{ .distinct_id = "u", .workspace_id = "w", .event_type = "PAYMENT_FAILED", .reason = "r", .plan_tier = "SCALE", .billing_status = "GRACE", .request_id = "r" });
    t.capture(telemetry.ServerStarted, .{ .port = 3000 });
    t.capture(telemetry.WorkerStarted, .{ .concurrency = 4 });
    t.capture(telemetry.StartupFailed, .{ .command = "serve", .phase = "db", .reason = "err", .error_code = "UZ-001" });
    t.capture(telemetry.ApiError, .{ .distinct_id = "u", .error_code = "UZ-001", .message = "m", .request_id = "r" });
    t.capture(telemetry.ApiErrorWithContext, .{ .distinct_id = "u", .error_code = "UZ-001", .message = "m", .workspace_id = "w", .request_id = "r" });
    t.capture(telemetry.WorkspaceCreated, .{ .distinct_id = "u", .workspace_id = "w", .tenant_id = "t", .repo_url = "https://x", .request_id = "r" });
    t.capture(telemetry.WorkspaceGithubConnected, .{ .workspace_id = "w", .installation_id = "12345", .request_id = "r" });
    t.capture(telemetry.AuthLoginCompleted, .{ .session_id = "s", .request_id = "r" });
    t.capture(telemetry.AuthRejected, .{ .reason = "token_expired", .request_id = "r" });
    t.capture(telemetry.RunOrphanNoAgentProfile, .{ .distinct_id = "u", .run_id = "r", .workspace_id = "w" });
    try telemetry.TestBackend.assertCount(14);
    try telemetry.TestBackend.assertLastEventIs(.run_orphan_no_agent_profile);
}
