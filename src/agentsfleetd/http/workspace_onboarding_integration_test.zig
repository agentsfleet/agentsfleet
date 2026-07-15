// HTTP integration tests for GET /v1/workspaces/{ws}/onboarding.
//
// Requires DATABASE_URL (or TEST_DATABASE_URL) — skipped otherwise. Reuses the
// seeded tenant/workspace + JWT personas from secrets_json_integration_test.zig
// and seeds the one core.users row the operator persona resolves to (prefs and
// the model check both key on it).

const std = @import("std");
const clock = @import("common").clock;
const pg = @import("pg");
const base = @import("secrets_json_integration_test.zig");
const fixtures_provider = @import("../db/test_fixtures.zig");

const TEST_TENANT_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01";
const USER_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a7101";
const USER_SUBJECT = "user_test"; // the TENANT_ADMIN / OPERATOR persona subject

fn seedUser(conn: *pg.Conn) !void {
    const now_ms = clock.nowMillis();
    _ = try conn.exec("DELETE FROM core.user_preferences WHERE user_id = $1::uuid", .{USER_ID});
    _ = try conn.exec(
        \\INSERT INTO core.users (user_id, tenant_id, oidc_subject, email, created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, $3, $4, $5, $5)
        \\ON CONFLICT (user_id) DO NOTHING
    , .{ USER_ID, TEST_TENANT_ID, USER_SUBJECT, "o@onboarding.test", now_ms });
}

fn cleanup(conn: *pg.Conn) void {
    _ = conn.exec("DELETE FROM core.user_preferences WHERE user_id = $1::uuid", .{USER_ID}) catch |err|
        std.log.warn("ignored: {s}", .{@errorName(err)});
    _ = conn.exec("DELETE FROM core.users WHERE user_id = $1::uuid", .{USER_ID}) catch |err|
        std.log.warn("ignored: {s}", .{@errorName(err)});
    fixtures_provider.teardownPlatformProvider(conn, base.TEST_WS_ID);
    base.cleanupRows(conn);
}

test "integration: test_onboarding_empty_workspace_all_false — a fresh workspace with no platform default reports every signal false" {
    base.setTestEncryptionKey();
    const alloc = std.testing.allocator;
    const h = base.seedAndHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    {
        const conn = try h.acquireConn();
        defer h.releaseConn(conn);
        try seedUser(conn);
        // No platform default and no tenant selection → model is unconfigured.
        fixtures_provider.teardownPlatformProvider(conn, base.TEST_WS_ID);
    }

    const path = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/onboarding", .{base.TEST_WS_ID});
    defer alloc.free(path);
    const r = try (try h.get(path).bearer(base.TOKEN_OPERATOR)).send();
    defer r.deinit();
    try r.expectStatus(.ok);
    try std.testing.expect(r.bodyContains("\"has_fleet\":false"));
    try std.testing.expect(r.bodyContains("\"has_secret\":false"));
    try std.testing.expect(r.bodyContains("\"has_processed_event\":false"));
    try std.testing.expect(r.bodyContains("\"has_steer_event\":false"));
    try std.testing.expect(r.bodyContains("\"model_configured\":false"));
    try std.testing.expect(r.bodyContains("\"dismissed\":false"));
    try std.testing.expect(r.bodyContains("\"cli_ticked\":false"));

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    cleanup(conn);
}

test "integration: test_onboarding_reflects_platform_default_and_secret — a platform default ticks the model step, a stored secret ticks the credential step" {
    base.setTestEncryptionKey();
    const alloc = std.testing.allocator;
    const h = base.seedAndHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    {
        const conn = try h.acquireConn();
        defer h.releaseConn(conn);
        try seedUser(conn);
        try fixtures_provider.seedPlatformProvider(alloc, conn, base.TEST_WS_ID);
    }

    const secrets_path = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/secrets", .{base.TEST_WS_ID});
    defer alloc.free(secrets_path);
    {
        const r = try (try (try h.post(secrets_path).bearer(base.TOKEN_OPERATOR))
            .json("{\"name\":\"gh-token\",\"data\":{\"provider\":\"github\",\"api_key\":\"ghp_x\"}}")).send();
        defer r.deinit();
        try r.expectStatus(.created);
    }

    const path = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/onboarding", .{base.TEST_WS_ID});
    defer alloc.free(path);
    const r = try (try h.get(path).bearer(base.TOKEN_OPERATOR)).send();
    defer r.deinit();
    try r.expectStatus(.ok);
    try std.testing.expect(r.bodyContains("\"model_configured\":true"));
    try std.testing.expect(r.bodyContains("\"has_secret\":true"));
    // Still no fleet, no events.
    try std.testing.expect(r.bodyContains("\"has_fleet\":false"));
    try std.testing.expect(r.bodyContains("\"has_steer_event\":false"));

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    cleanup(conn);
}

test "integration: test_onboarding_folds_in_preferences — a dismissed/collapsed/cli preference shows through the onboarding response" {
    base.setTestEncryptionKey();
    const alloc = std.testing.allocator;
    const h = base.seedAndHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    {
        const conn = try h.acquireConn();
        defer h.releaseConn(conn);
        try seedUser(conn);
    }

    // Set the CLI tick and the dismissed pref through the preferences endpoint.
    for ([_][]const u8{ "getting_started_cli_ticked", "getting_started_dismissed" }) |key| {
        const pref_path = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/preferences/{s}", .{ base.TEST_WS_ID, key });
        defer alloc.free(pref_path);
        const r = try (try (try h.put(pref_path).bearer(base.TOKEN_OPERATOR)).json("true")).send();
        defer r.deinit();
        try r.expectStatus(.ok);
    }

    const path = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/onboarding", .{base.TEST_WS_ID});
    defer alloc.free(path);
    const r = try (try h.get(path).bearer(base.TOKEN_OPERATOR)).send();
    defer r.deinit();
    try r.expectStatus(.ok);
    try std.testing.expect(r.bodyContains("\"cli_ticked\":true"));
    try std.testing.expect(r.bodyContains("\"dismissed\":true"));
    try std.testing.expect(r.bodyContains("\"collapsed\":false"));

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    cleanup(conn);
}
