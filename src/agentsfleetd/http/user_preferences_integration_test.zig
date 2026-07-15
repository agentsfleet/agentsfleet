// HTTP integration tests for /v1/workspaces/{workspace_id}/preferences.
//
// Requires DATABASE_URL (or TEST_DATABASE_URL) — skipped otherwise via
// `TestHarness.start` returning `error.SkipZigTest`. Reuses the seeded
// tenant/workspace + JWT personas from secrets_json_integration_test.zig.
//
// Prefs are keyed on core.users.user_id, which the base seed does not create
// (it only seeds a tenant + workspace), so this suite seeds the two user rows
// its personas resolve to. That is also what makes the isolation test real:
// TENANT_ADMIN and PLATFORM_ADMIN carry DIFFERENT Clerk subjects inside the
// SAME workspace, so one reading the other's bag is a live cross-user probe,
// not a cross-workspace one.

const std = @import("std");
const clock = @import("common").clock;
const pg = @import("pg");
const base = @import("secrets_json_integration_test.zig");
const scope_fixtures = @import("test_scope_tokens.zig");
const harness_mod = @import("test_harness.zig");
const prefs_store = @import("../state/user_preferences.zig");
const PgQuery = @import("../db/pg_query.zig").PgQuery;
const error_codes = @import("../errors/error_registry.zig");

// Same tenant + workspace claims as TENANT_ADMIN, but a different `sub`.
const TOKEN_OTHER_USER = scope_fixtures.PLATFORM_ADMIN;

const TEST_TENANT_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01";
// Not owned by the personas' tenant — the ownership check must refuse it.
const FOREIGN_WS_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f99";

// core.users rows the two personas' `sub` claims resolve to.
const USER_A_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a7001";
const USER_A_SUBJECT = "user_test"; // TENANT_ADMIN / OPERATOR / VIEWER
const USER_B_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a7002";
const USER_B_SUBJECT = "user_op_m104"; // PLATFORM_ADMIN

const PREF_DISMISSED = "getting_started_dismissed";
const PREF_COLLAPSED = "getting_started_collapsed";

fn seedUsers(conn: *pg.Conn) !void {
    const now_ms = clock.nowMillis();
    _ = try conn.exec("DELETE FROM core.user_preferences WHERE user_id IN ($1::uuid, $2::uuid)", .{ USER_A_ID, USER_B_ID });
    _ = try conn.exec(
        \\INSERT INTO core.users (user_id, tenant_id, oidc_subject, email, created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, $3, $4, $5, $5)
        \\ON CONFLICT (user_id) DO NOTHING
    , .{ USER_A_ID, TEST_TENANT_ID, USER_A_SUBJECT, "a@prefs.test", now_ms });
    _ = try conn.exec(
        \\INSERT INTO core.users (user_id, tenant_id, oidc_subject, email, created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, $3, $4, $5, $5)
        \\ON CONFLICT (user_id) DO NOTHING
    , .{ USER_B_ID, TEST_TENANT_ID, USER_B_SUBJECT, "b@prefs.test", now_ms });
}

fn cleanup(conn: *pg.Conn) void {
    _ = conn.exec("DELETE FROM core.user_preferences WHERE user_id IN ($1::uuid, $2::uuid)", .{ USER_A_ID, USER_B_ID }) catch |err|
        std.log.warn("ignored: {s}", .{@errorName(err)});
    _ = conn.exec("DELETE FROM core.users WHERE user_id IN ($1::uuid, $2::uuid)", .{ USER_A_ID, USER_B_ID }) catch |err|
        std.log.warn("ignored: {s}", .{@errorName(err)});
    base.cleanupRows(conn);
}

fn prefRowCount(conn: *pg.Conn) !i64 {
    var q = PgQuery.from(try conn.query(
        "SELECT COUNT(*)::bigint FROM core.user_preferences WHERE user_id IN ($1::uuid, $2::uuid)",
        .{ USER_A_ID, USER_B_ID },
    ));
    defer q.deinit();
    const row = (try q.next()) orelse return error.RowMissing;
    return try row.get(i64, 0);
}

fn seededHarness(alloc: std.mem.Allocator) !*harness_mod.TestHarness {
    const h = try base.seedAndHarness(alloc);
    errdefer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try seedUsers(conn);
    return h;
}

test "integration: test_preferences_roundtrip — an unset bag is empty, a PUT round-trips through the next GET" {
    base.setTestEncryptionKey();
    const alloc = std.testing.allocator;
    const h = seededHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const bag_path = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/preferences", .{base.TEST_WS_ID});
    defer alloc.free(bag_path);

    {
        // Nothing written yet: an empty bag, never a 404 — the dashboard must be
        // able to tell "no prefs" from "prefs unavailable" without branching.
        const r = try (try h.get(bag_path).bearer(base.TOKEN_OPERATOR)).send();
        defer r.deinit();
        try r.expectStatus(.ok);
        try std.testing.expect(r.bodyContains("\"prefs\":{}"));
    }

    const collapsed_path = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/preferences/{s}", .{ base.TEST_WS_ID, PREF_COLLAPSED });
    defer alloc.free(collapsed_path);
    {
        const r = try (try (try h.put(collapsed_path).bearer(base.TOKEN_OPERATOR)).json("true")).send();
        defer r.deinit();
        try r.expectStatus(.ok);
        try std.testing.expect(r.bodyContains("\"" ++ PREF_COLLAPSED ++ "\":true"));
    }
    {
        const r = try (try h.get(bag_path).bearer(base.TOKEN_OPERATOR)).send();
        defer r.deinit();
        try r.expectStatus(.ok);
        try std.testing.expect(r.bodyContains("\"" ++ PREF_COLLAPSED ++ "\":true"));
    }
    {
        // Same key again with the opposite value: last write wins, one row still.
        const r = try (try (try h.put(collapsed_path).bearer(base.TOKEN_OPERATOR)).json("false")).send();
        defer r.deinit();
        try r.expectStatus(.ok);
        try std.testing.expect(r.bodyContains("\"" ++ PREF_COLLAPSED ++ "\":false"));
    }

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try std.testing.expectEqual(@as(i64, 1), try prefRowCount(conn));
    cleanup(conn);
}

test "integration: test_preferences_unknown_key_rejected — a key outside the registry is refused and writes no row" {
    base.setTestEncryptionKey();
    const alloc = std.testing.allocator;
    const h = seededHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const bogus_path = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/preferences/bogus", .{base.TEST_WS_ID});
    defer alloc.free(bogus_path);
    {
        const r = try (try (try h.put(bogus_path).bearer(base.TOKEN_OPERATOR)).json("true")).send();
        defer r.deinit();
        try r.expectStatus(.bad_request);
        try std.testing.expect(r.bodyContains(error_codes.ERR_PREF_KEY_UNKNOWN));
    }

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try std.testing.expectEqual(@as(i64, 0), try prefRowCount(conn));
    cleanup(conn);
}

test "integration: test_preferences_value_too_large_rejected — an oversize value is refused and writes no row" {
    base.setTestEncryptionKey();
    const alloc = std.testing.allocator;
    const h = seededHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    // A well-formed JSON string one byte past the cap: the size gate must fire
    // before the parse gate, so this is a UZ-PREFS-002, not a malformed-JSON 400.
    const filler = try alloc.alloc(u8, prefs_store.MAX_PREF_VALUE_BYTES);
    defer alloc.free(filler);
    @memset(filler, 'x');
    const oversize = try std.fmt.allocPrint(alloc, "\"{s}\"", .{filler});
    defer alloc.free(oversize);

    const dismissed_path = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/preferences/{s}", .{ base.TEST_WS_ID, PREF_DISMISSED });
    defer alloc.free(dismissed_path);
    {
        const r = try (try (try h.put(dismissed_path).bearer(base.TOKEN_OPERATOR)).json(oversize)).send();
        defer r.deinit();
        try r.expectStatus(.bad_request);
        try std.testing.expect(r.bodyContains(error_codes.ERR_PREF_VALUE_TOO_LARGE));
    }

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try std.testing.expectEqual(@as(i64, 0), try prefRowCount(conn));
    cleanup(conn);
}

test "integration: test_preferences_cross_tenant_isolation — another user never sees this user's bag, and a foreign workspace is refused" {
    base.setTestEncryptionKey();
    const alloc = std.testing.allocator;
    const h = seededHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const dismissed_path = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/preferences/{s}", .{ base.TEST_WS_ID, PREF_DISMISSED });
    defer alloc.free(dismissed_path);
    {
        // User A dismisses onboarding in this workspace.
        const r = try (try (try h.put(dismissed_path).bearer(base.TOKEN_OPERATOR)).json("true")).send();
        defer r.deinit();
        try r.expectStatus(.ok);
        try std.testing.expect(r.bodyContains("\"" ++ PREF_DISMISSED ++ "\":true"));
    }

    const bag_path = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/preferences", .{base.TEST_WS_ID});
    defer alloc.free(bag_path);
    {
        // User B, same workspace, different Clerk subject: A's dismissal must not
        // hide B's onboarding. This is the whole point of keying on user_id.
        const r = try (try h.get(bag_path).bearer(TOKEN_OTHER_USER)).send();
        defer r.deinit();
        try r.expectStatus(.ok);
        try std.testing.expect(r.bodyContains("\"prefs\":{}"));
        try std.testing.expect(!r.bodyContains(PREF_DISMISSED));
    }

    const foreign_bag_path = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/preferences", .{FOREIGN_WS_ID});
    defer alloc.free(foreign_bag_path);
    {
        // A workspace this tenant does not own is refused before any row is read.
        const r = try (try h.get(foreign_bag_path).bearer(base.TOKEN_OPERATOR)).send();
        defer r.deinit();
        try r.expectStatus(.forbidden);
    }
    const foreign_pref_path = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/preferences/{s}", .{ FOREIGN_WS_ID, PREF_DISMISSED });
    defer alloc.free(foreign_pref_path);
    {
        const r = try (try (try h.put(foreign_pref_path).bearer(base.TOKEN_OPERATOR)).json("true")).send();
        defer r.deinit();
        try r.expectStatus(.forbidden);
    }

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    // Exactly A's single row: B wrote nothing, and neither foreign call landed.
    try std.testing.expectEqual(@as(i64, 1), try prefRowCount(conn));
    cleanup(conn);
}
