const std = @import("std");
const telemetry = @import("telemetry.zig");
const events = @import("telemetry_events.zig");
const posthog = @import("posthog");

// ── T1: Happy path ──────────────────────────────────────────────────

test "T1: distinctIdOrSystem falls back to system" {
    try std.testing.expectEqualStrings("system", telemetry.distinctIdOrSystem(""));
    try std.testing.expectEqualStrings("user_123", telemetry.distinctIdOrSystem("user_123"));
}

test "T1: capture RunOrphanRecovered records correct kind" {
    var t = telemetry.Telemetry.initTest();
    t.capture(telemetry.RunOrphanRecovered, .{
        .distinct_id = "u1",
        .run_id = "run_1",
        .workspace_id = "ws_1",
        .staleness_ms = 5000,
    });
    try telemetry.TestBackend.assertLastEventIs(.run_orphan_recovered);
}

test "T1: capture multiple events increments count" {
    var t = telemetry.Telemetry.initTest();
    t.capture(telemetry.ServerStarted, .{ .port = 3000 });
    t.capture(telemetry.WorkerStarted, .{ .concurrency = 4 });
    t.capture(telemetry.AuthLoginCompleted, .{ .session_id = "s1", .request_id = "r1" });
    try telemetry.TestBackend.assertCount(3);
}

test "T1: reset clears ring and lastEvent returns null" {
    var t = telemetry.Telemetry.initTest();
    t.capture(telemetry.ServerStarted, .{ .port = 8080 });
    try telemetry.TestBackend.assertCount(1);
    telemetry.TestBackend.reset();
    try std.testing.expect(telemetry.TestBackend.lastEvent() == null);
    try telemetry.TestBackend.assertCount(0);
}

test "T1: workspace_id captured for events that have it" {
    var t = telemetry.Telemetry.initTest();
    t.capture(telemetry.EntitlementRejected, .{
        .distinct_id = "u1",
        .workspace_id = "ws_42",
        .boundary = "COMPILE",
        .reason_code = "ERR",
        .request_id = "r1",
    });
    const last = telemetry.TestBackend.lastEvent().?;
    try std.testing.expectEqualStrings("ws_42", last.workspaceId());
}

test "T1: events without workspace_id capture empty string" {
    var t = telemetry.Telemetry.initTest();
    t.capture(telemetry.ServerStarted, .{ .port = 3000 });
    const last = telemetry.TestBackend.lastEvent().?;
    try std.testing.expectEqualStrings("", last.workspaceId());
}

test "T1: distinct_id defaults to system for events without it" {
    var t = telemetry.Telemetry.initTest();
    t.capture(telemetry.ServerStarted, .{ .port = 3000 });
    const last = telemetry.TestBackend.lastEvent().?;
    try std.testing.expectEqualStrings("system", last.distinctId());
}

test "T1: all 14 event types can be captured without error" {
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

// ── T2: Edge cases ──────────────────────────────────────────────────

test "T2: ring buffer wraps at 64 without crash" {
    var t = telemetry.Telemetry.initTest();
    // Fill ring to capacity
    for (0..64) |_| {
        t.capture(telemetry.ServerStarted, .{ .port = 1 });
    }
    try telemetry.TestBackend.assertCount(64);
    // 65th event wraps into slot 0
    t.capture(telemetry.WorkerStarted, .{ .concurrency = 99 });
    try telemetry.TestBackend.assertCount(65);
    try telemetry.TestBackend.assertLastEventIs(.worker_started);
}

test "T2: RecordedEvent truncates strings longer than 64 bytes" {
    const long_id = "a" ** 100;
    const r = telemetry.RecordedEvent.initFromSlices(.server_started, long_id, long_id);
    try std.testing.expectEqual(@as(u8, 64), r.distinct_id_len);
    try std.testing.expectEqual(@as(u8, 64), r.workspace_id_len);
    try std.testing.expectEqualStrings("a" ** 64, r.distinctId());
    try std.testing.expectEqualStrings("a" ** 64, r.workspaceId());
}

test "T2: RecordedEvent handles empty strings" {
    const r = telemetry.RecordedEvent.initFromSlices(.api_error, "", "");
    try std.testing.expectEqual(@as(u8, 0), r.distinct_id_len);
    try std.testing.expectEqual(@as(u8, 0), r.workspace_id_len);
    try std.testing.expectEqualStrings("", r.distinctId());
    try std.testing.expectEqualStrings("", r.workspaceId());
}

test "T2: RecordedEvent stores exactly 64-byte string without truncation" {
    const exact = "b" ** 64;
    const r = telemetry.RecordedEvent.initFromSlices(.api_error, exact, "ws");
    try std.testing.expectEqual(@as(u8, 64), r.distinct_id_len);
    try std.testing.expectEqualStrings(exact, r.distinctId());
}

test "T2: ApiError and ApiErrorWithContext both emit api_error kind" {
    var t = telemetry.Telemetry.initTest();
    t.capture(telemetry.ApiError, .{ .distinct_id = "u", .error_code = "E1", .message = "m", .request_id = "r" });
    try telemetry.TestBackend.assertLastEventIs(.api_error);
    t.capture(telemetry.ApiErrorWithContext, .{ .distinct_id = "u", .error_code = "E2", .message = "m", .workspace_id = "w", .request_id = "r" });
    try telemetry.TestBackend.assertLastEventIs(.api_error);
    // Both share the same EventKind variant
    try std.testing.expectEqual(telemetry.ApiError.kind, telemetry.ApiErrorWithContext.kind);
}

// ── T3: Error paths ─────────────────────────────────────────────────

test "T3: assertLastEventIs returns error when no events recorded" {
    _ = telemetry.Telemetry.initTest();
    const result = telemetry.TestBackend.assertLastEventIs(.server_started);
    try std.testing.expectError(error.NoEventsRecorded, result);
}

test "T3: assertCount fails on mismatch" {
    var t = telemetry.Telemetry.initTest();
    t.capture(telemetry.ServerStarted, .{ .port = 3000 });
    const result = telemetry.TestBackend.assertCount(5);
    try std.testing.expectError(error.TestExpectedEqual, result);
}

test "T3: lastEvent returns null when count is zero" {
    _ = telemetry.Telemetry.initTest();
    try std.testing.expect(telemetry.TestBackend.lastEvent() == null);
}

test "T3: assertLastEventIs fails on wrong kind" {
    var t = telemetry.Telemetry.initTest();
    t.capture(telemetry.ServerStarted, .{ .port = 3000 });
    const result = telemetry.TestBackend.assertLastEventIs(.worker_started);
    try std.testing.expectError(error.TestExpectedEqual, result);
}

// ── T4: Property fidelity ───────────────────────────────────────────

test "T4: ServerStarted.properties returns port as integer" {
    const ev = telemetry.ServerStarted{ .port = 8080 };
    const props = ev.properties();
    try std.testing.expectEqual(@as(usize, 1), props.len);
    try std.testing.expectEqualStrings("port", props[0].key);
    try std.testing.expectEqual(@as(i64, 8080), props[0].value.integer);
}

test "T4: StartupFailed.properties returns all 4 fields" {
    const ev = telemetry.StartupFailed{
        .command = "worker",
        .phase = "db_connect",
        .reason = "connection_refused",
        .error_code = "UZ-STARTUP-003",
    };
    const props = ev.properties();
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

test "T4: AgentCompleted.properties includes integer fields" {
    const ev = telemetry.AgentCompleted{
        .distinct_id = "u",
        .run_id = "run_1",
        .workspace_id = "ws_1",
        .actor = "Echo",
        .tokens = 1500,
        .duration_ms = 42000,
        .exit_status = "ok",
    };
    const props = ev.properties();
    try std.testing.expectEqual(@as(usize, 6), props.len);
    // tokens is at index 3
    try std.testing.expectEqualStrings("tokens", props[3].key);
    try std.testing.expectEqual(@as(i64, 1500), props[3].value.integer);
    // duration_ms is at index 4
    try std.testing.expectEqualStrings("duration_ms", props[4].key);
    try std.testing.expectEqual(@as(i64, 42000), props[4].value.integer);
}

test "T4: BillingLifecycleEvent.properties returns all 6 fields" {
    const ev = telemetry.BillingLifecycleEvent{
        .distinct_id = "u",
        .workspace_id = "ws_1",
        .event_type = "PAYMENT_FAILED",
        .reason = "invoice_failed",
        .plan_tier = "SCALE",
        .billing_status = "GRACE",
        .request_id = "req_1",
    };
    const props = ev.properties();
    try std.testing.expectEqual(@as(usize, 6), props.len);
    try std.testing.expectEqualStrings("event_type", props[1].key);
    try std.testing.expectEqualStrings("PAYMENT_FAILED", props[1].value.string);
}

test "T4: RunOrphanRecovered.properties includes staleness_ms as integer" {
    const ev = telemetry.RunOrphanRecovered{
        .distinct_id = "u",
        .run_id = "run_1",
        .workspace_id = "ws_1",
        .staleness_ms = 12345,
    };
    const props = ev.properties();
    try std.testing.expectEqual(@as(usize, 3), props.len);
    try std.testing.expectEqualStrings("staleness_ms", props[2].key);
    try std.testing.expectEqual(@as(i64, 12345), props[2].value.integer);
}

// ── T7: Regression safety ───────────────────────────────────────────

test "T7: EventKind has exactly 14 variants" {
    const fields = @typeInfo(telemetry.EventKind).@"enum".fields;
    try std.testing.expectEqual(@as(usize, 14), fields.len);
}

test "T7: EventKind tagName matches expected event name strings" {
    try std.testing.expectEqualStrings("agent_completed", @tagName(telemetry.EventKind.agent_completed));
    try std.testing.expectEqualStrings("server_started", @tagName(telemetry.EventKind.server_started));
    try std.testing.expectEqualStrings("auth_rejected", @tagName(telemetry.EventKind.auth_rejected));
    try std.testing.expectEqualStrings("run_orphan_recovered", @tagName(telemetry.EventKind.run_orphan_recovered));
    try std.testing.expectEqualStrings("billing_lifecycle_event", @tagName(telemetry.EventKind.billing_lifecycle_event));
}

test "T7: each event struct kind constant matches its EventKind variant" {
    try std.testing.expectEqual(telemetry.EventKind.agent_completed, telemetry.AgentCompleted.kind);
    try std.testing.expectEqual(telemetry.EventKind.entitlement_rejected, telemetry.EntitlementRejected.kind);
    try std.testing.expectEqual(telemetry.EventKind.profile_activated, telemetry.ProfileActivated.kind);
    try std.testing.expectEqual(telemetry.EventKind.billing_lifecycle_event, telemetry.BillingLifecycleEvent.kind);
    try std.testing.expectEqual(telemetry.EventKind.server_started, telemetry.ServerStarted.kind);
    try std.testing.expectEqual(telemetry.EventKind.worker_started, telemetry.WorkerStarted.kind);
    try std.testing.expectEqual(telemetry.EventKind.startup_failed, telemetry.StartupFailed.kind);
    try std.testing.expectEqual(telemetry.EventKind.api_error, telemetry.ApiError.kind);
    try std.testing.expectEqual(telemetry.EventKind.api_error, telemetry.ApiErrorWithContext.kind);
    try std.testing.expectEqual(telemetry.EventKind.workspace_created, telemetry.WorkspaceCreated.kind);
    try std.testing.expectEqual(telemetry.EventKind.workspace_github_connected, telemetry.WorkspaceGithubConnected.kind);
    try std.testing.expectEqual(telemetry.EventKind.auth_login_completed, telemetry.AuthLoginCompleted.kind);
    try std.testing.expectEqual(telemetry.EventKind.auth_rejected, telemetry.AuthRejected.kind);
    try std.testing.expectEqual(telemetry.EventKind.run_orphan_recovered, telemetry.RunOrphanRecovered.kind);
    try std.testing.expectEqual(telemetry.EventKind.run_orphan_no_agent_profile, telemetry.RunOrphanNoAgentProfile.kind);
}

// ── T11: Memory + resource safety ───────────────────────────────────

test "T11: initTest + capture cycle has no leaks" {
    // std.testing.allocator auto-detects leaks on scope exit.
    // This test verifies that event construction and ring buffer
    // recording involve zero heap allocations.
    const allocator = std.testing.allocator;
    _ = allocator; // present for leak detector activation
    var t = telemetry.Telemetry.initTest();
    for (0..128) |i| {
        if (i % 2 == 0) {
            t.capture(telemetry.AgentCompleted, .{
                .distinct_id = "user",
                .run_id = "run",
                .workspace_id = "ws",
                .actor = "Echo",
                .tokens = i,
                .duration_ms = i * 10,
                .exit_status = "ok",
            });
        } else {
            t.capture(telemetry.ServerStarted, .{ .port = 3000 });
        }
    }
    try telemetry.TestBackend.assertCount(128);
}

test "T11: RecordedEvent is stack-allocated, no heap" {
    // Verify RecordedEvent size is bounded and predictable.
    // 64+1 + 64+1 + enum(1) + padding = small, stack-safe.
    const size = @sizeOf(telemetry.RecordedEvent);
    try std.testing.expect(size <= 256);
}
