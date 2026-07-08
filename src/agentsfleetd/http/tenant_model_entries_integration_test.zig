// HTTP integration tests for /v1/tenants/me/models (M121 §2).
//
// Requires DATABASE_URL (or TEST_DATABASE_URL) — skipped otherwise via
// `TestHarness.start` returning `error.SkipZigTest`. Reuses the seeded
// tenant/workspace + JWT tokens baked into secrets_json_integration_test.zig
// (`setupSeedData` there also catalogues anthropic/claude-sonnet-4-6 so the
// PUT /provider activation gate passes).

const std = @import("std");
const pg = @import("pg");
const base = @import("secrets_json_integration_test.zig");
const error_codes = @import("../errors/error_registry.zig");
const entries_state = @import("../state/tenant_model_entries.zig");

// Matches base's private TEST_TENANT_ID (not exported) — same literal, same
// seeded row, per the sibling suite's precedent
// (tenant_provider_platform_default_available_integration_test.zig).
const TEST_TENANT_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01";

fn cleanup(conn: *pg.Conn) void {
    _ = conn.exec("DELETE FROM core.tenant_model_entries WHERE tenant_id = $1::uuid", .{TEST_TENANT_ID}) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
    base.cleanupRows(conn);
}

fn extractId(alloc: std.mem.Allocator, body: []const u8) ![]const u8 {
    const IdOnly = struct { id: []const u8 };
    const parsed = try std.json.parseFromSlice(IdOnly, alloc, body, .{ .ignore_unknown_fields = true });
    defer parsed.deinit();
    return alloc.dupe(u8, parsed.value.id);
}

test "test_models_list_joins_metadata_and_active — GET joins secret metadata, flags exactly one active entry, never leaks api_key" {
    base.setTestEncryptionKey();
    const alloc = std.testing.allocator;
    const h = base.seedAndHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const secrets_path = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/secrets", .{base.TEST_WS_ID});
    defer alloc.free(secrets_path);
    const SENTINEL = "sk-ant-DO-NOT-LEAK-2c1f";
    {
        const r = try (try (try h.post(secrets_path).bearer(base.TOKEN_OPERATOR))
            .json("{\"name\":\"anthropic-shared\",\"data\":{\"provider\":\"anthropic\",\"api_key\":\"" ++ SENTINEL ++ "\",\"model\":\"claude-sonnet-4-6\"}}")).send();
        defer r.deinit();
        try r.expectStatus(.created);
    }

    const models_path = "/v1/tenants/me/models";
    {
        const r = try (try (try h.post(models_path).bearer(base.TOKEN_OPERATOR))
            .json("{\"model_id\":\"claude-sonnet-4-6\",\"secret_ref\":\"anthropic-shared\"}")).send();
        defer r.deinit();
        try r.expectStatus(.created);
    }
    {
        const r = try (try (try h.post(models_path).bearer(base.TOKEN_OPERATOR))
            .json("{\"model_id\":\"claude-opus-4-1\",\"secret_ref\":\"anthropic-shared\"}")).send();
        defer r.deinit();
        try r.expectStatus(.created);
    }
    {
        // Activates (secret_ref=anthropic-shared, model=claude-sonnet-4-6) — the
        // credential's own stored model, since no override is given.
        const r = try (try (try h.put("/v1/tenants/me/provider").bearer(base.TOKEN_OPERATOR))
            .json("{\"mode\":\"self_managed\",\"secret_ref\":\"anthropic-shared\"}")).send();
        defer r.deinit();
        try r.expectStatus(.ok);
    }

    const r = try (try h.get(models_path).bearer(base.TOKEN_OPERATOR)).send();
    defer r.deinit();
    try r.expectStatus(.ok);
    try std.testing.expectEqual(@as(usize, 2), std.mem.count(u8, r.body, "\"kind\":\"provider_key\""));
    try std.testing.expectEqual(@as(usize, 2), std.mem.count(u8, r.body, "\"provider\":\"anthropic\""));
    try std.testing.expectEqual(@as(usize, 1), std.mem.count(u8, r.body, "\"active\":true"));
    try std.testing.expectEqual(@as(usize, 1), std.mem.count(u8, r.body, "\"active\":false"));
    try std.testing.expect(r.bodyContains("\"platform_default_available\""));
    try std.testing.expect(!r.bodyContains(SENTINEL));
    try std.testing.expect(!r.bodyContains("api_key"));

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    cleanup(conn);
}

test "test_models_list_synthesizes_active_entry — GET synthesizes a missing entry for a pre-registry activation, idempotently" {
    base.setTestEncryptionKey();
    const alloc = std.testing.allocator;
    const h = base.seedAndHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const secrets_path = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/secrets", .{base.TEST_WS_ID});
    defer alloc.free(secrets_path);
    {
        const r = try (try (try h.post(secrets_path).bearer(base.TOKEN_OPERATOR))
            .json("{\"name\":\"sync-key\",\"data\":{\"provider\":\"anthropic\",\"api_key\":\"sk-sync\",\"model\":\"claude-sonnet-4-6\"}}")).send();
        defer r.deinit();
        try r.expectStatus(.created);
    }
    {
        // Activated directly through PUT /provider — the registry (§2) never
        // saw a POST for this model, matching a pre-registry tenant.
        const r = try (try (try h.put("/v1/tenants/me/provider").bearer(base.TOKEN_OPERATOR))
            .json("{\"mode\":\"self_managed\",\"secret_ref\":\"sync-key\"}")).send();
        defer r.deinit();
        try r.expectStatus(.ok);
    }

    {
        const r = try (try h.get("/v1/tenants/me/models").bearer(base.TOKEN_OPERATOR)).send();
        defer r.deinit();
        try r.expectStatus(.ok);
        try std.testing.expect(r.bodyContains("\"model_id\":\"claude-sonnet-4-6\""));
        try std.testing.expect(r.bodyContains("\"secret_ref\":\"sync-key\""));
        try std.testing.expect(r.bodyContains("\"active\":true"));
    }
    {
        // Second GET must not synthesize a duplicate row.
        const r = try (try h.get("/v1/tenants/me/models").bearer(base.TOKEN_OPERATOR)).send();
        defer r.deinit();
        try r.expectStatus(.ok);
        try std.testing.expectEqual(@as(usize, 1), std.mem.count(u8, r.body, "\"model_id\":\"claude-sonnet-4-6\""));
    }

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    const rows = try entries_state.list(alloc, conn, TEST_TENANT_ID);
    defer entries_state.deinitEntryList(rows, alloc);
    try std.testing.expectEqual(@as(usize, 1), rows.len);
    cleanup(conn);
}

test "test_models_create_guards — POST guards unknown secret_ref (404) and duplicate entries (409)" {
    base.setTestEncryptionKey();
    const alloc = std.testing.allocator;
    const h = base.seedAndHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const models_path = "/v1/tenants/me/models";
    {
        const r = try (try (try h.post(models_path).bearer(base.TOKEN_OPERATOR))
            .json("{\"model_id\":\"claude-sonnet-4-6\",\"secret_ref\":\"does-not-exist\"}")).send();
        defer r.deinit();
        try r.expectStatus(.not_found);
        try std.testing.expect(r.bodyContains(error_codes.ERR_MODELS_SECRET_NOT_FOUND));
    }

    const secrets_path = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/secrets", .{base.TEST_WS_ID});
    defer alloc.free(secrets_path);
    {
        // Dashboard-created secrets are stored under a "fleet:"-prefixed key —
        // this also exercises the raw→prefixed existence-check fallback.
        const r = try (try (try h.post(secrets_path).bearer(base.TOKEN_OPERATOR))
            .json("{\"name\":\"dup-key\",\"data\":{\"provider\":\"anthropic\",\"api_key\":\"sk-dup\",\"model\":\"claude-sonnet-4-6\"}}")).send();
        defer r.deinit();
        try r.expectStatus(.created);
    }
    {
        const r = try (try (try h.post(models_path).bearer(base.TOKEN_OPERATOR))
            .json("{\"model_id\":\"claude-sonnet-4-6\",\"secret_ref\":\"dup-key\"}")).send();
        defer r.deinit();
        try r.expectStatus(.created);
    }
    {
        const r = try (try (try h.post(models_path).bearer(base.TOKEN_OPERATOR))
            .json("{\"model_id\":\"claude-sonnet-4-6\",\"secret_ref\":\"dup-key\"}")).send();
        defer r.deinit();
        try r.expectStatus(.conflict);
        try std.testing.expect(r.bodyContains(error_codes.ERR_MODELS_DUPLICATE_ENTRY));
    }

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    cleanup(conn);
}

test "test_models_delete_active_guard — DELETE refuses the active entry, allows a non-active sibling" {
    base.setTestEncryptionKey();
    const alloc = std.testing.allocator;
    const h = base.seedAndHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const secrets_path = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/secrets", .{base.TEST_WS_ID});
    defer alloc.free(secrets_path);
    {
        const r = try (try (try h.post(secrets_path).bearer(base.TOKEN_OPERATOR))
            .json("{\"name\":\"del-key\",\"data\":{\"provider\":\"anthropic\",\"api_key\":\"sk-del\",\"model\":\"claude-sonnet-4-6\"}}")).send();
        defer r.deinit();
        try r.expectStatus(.created);
    }

    const models_path = "/v1/tenants/me/models";
    const active_id = blk: {
        const r = try (try (try h.post(models_path).bearer(base.TOKEN_OPERATOR))
            .json("{\"model_id\":\"claude-sonnet-4-6\",\"secret_ref\":\"del-key\"}")).send();
        defer r.deinit();
        try r.expectStatus(.created);
        break :blk try extractId(alloc, r.body);
    };
    defer alloc.free(active_id);
    const other_id = blk: {
        const r = try (try (try h.post(models_path).bearer(base.TOKEN_OPERATOR))
            .json("{\"model_id\":\"claude-opus-4-1\",\"secret_ref\":\"del-key\"}")).send();
        defer r.deinit();
        try r.expectStatus(.created);
        break :blk try extractId(alloc, r.body);
    };
    defer alloc.free(other_id);
    {
        const r = try (try (try h.put("/v1/tenants/me/provider").bearer(base.TOKEN_OPERATOR))
            .json("{\"mode\":\"self_managed\",\"secret_ref\":\"del-key\"}")).send();
        defer r.deinit();
        try r.expectStatus(.ok);
    }

    const active_path = try std.fmt.allocPrint(alloc, "/v1/tenants/me/models/{s}", .{active_id});
    defer alloc.free(active_path);
    {
        const r = try (try h.delete(active_path).bearer(base.TOKEN_OPERATOR)).send();
        defer r.deinit();
        try r.expectStatus(.conflict);
        try std.testing.expect(r.bodyContains(error_codes.ERR_MODELS_DELETE_ACTIVE));
    }

    const other_path = try std.fmt.allocPrint(alloc, "/v1/tenants/me/models/{s}", .{other_id});
    defer alloc.free(other_path);
    {
        const r = try (try h.delete(other_path).bearer(base.TOKEN_OPERATOR)).send();
        defer r.deinit();
        try r.expectStatus(.no_content);
    }

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    cleanup(conn);
}

test "test_secret_delete_blocked_when_referenced — secret DELETE is blocked while referenced by a model entry, naming the count" {
    base.setTestEncryptionKey();
    const alloc = std.testing.allocator;
    const h = base.seedAndHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const secrets_path = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/secrets", .{base.TEST_WS_ID});
    defer alloc.free(secrets_path);
    {
        const r = try (try (try h.post(secrets_path).bearer(base.TOKEN_OPERATOR))
            .json("{\"name\":\"ref-key\",\"data\":{\"provider\":\"anthropic\",\"api_key\":\"sk-ref\",\"model\":\"claude-sonnet-4-6\"}}")).send();
        defer r.deinit();
        try r.expectStatus(.created);
    }

    const entry_id = blk: {
        const r = try (try (try h.post("/v1/tenants/me/models").bearer(base.TOKEN_OPERATOR))
            .json("{\"model_id\":\"claude-sonnet-4-6\",\"secret_ref\":\"ref-key\"}")).send();
        defer r.deinit();
        try r.expectStatus(.created);
        break :blk try extractId(alloc, r.body);
    };
    defer alloc.free(entry_id);

    const secret_item_path = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/secrets/ref-key", .{base.TEST_WS_ID});
    defer alloc.free(secret_item_path);
    {
        const r = try (try h.delete(secret_item_path).bearer(base.TOKEN_OPERATOR)).send();
        defer r.deinit();
        try r.expectStatus(.conflict);
        try std.testing.expect(r.bodyContains(error_codes.ERR_SECRET_REFERENCED_BY_MODEL_ENTRIES));
        try std.testing.expect(r.bodyContains("1"));
    }

    const entry_path = try std.fmt.allocPrint(alloc, "/v1/tenants/me/models/{s}", .{entry_id});
    defer alloc.free(entry_path);
    {
        // Not the active entry (no PUT /provider call in this test) — deletes clean.
        const r = try (try h.delete(entry_path).bearer(base.TOKEN_OPERATOR)).send();
        defer r.deinit();
        try r.expectStatus(.no_content);
    }
    {
        // Unreferenced now — the secret delete proceeds as it did before this guard.
        const r = try (try h.delete(secret_item_path).bearer(base.TOKEN_OPERATOR)).send();
        defer r.deinit();
        try r.expectStatus(.no_content);
    }

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    cleanup(conn);
}
