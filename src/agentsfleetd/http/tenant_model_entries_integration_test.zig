// HTTP integration tests for /v1/tenants/me/models.
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
const fixtures_provider = @import("../db/test_fixtures.zig");

// Matches base's private TEST_TENANT_ID (not exported) — same literal, same
// seeded row, per the sibling suite's precedent
// (tenant_provider_platform_default_available_integration_test.zig).
const TEST_TENANT_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01";
const SONNET_INPUT_RATE_JSON = "\"input_nanos_per_mtok\":3000000000";
const SONNET_CACHED_INPUT_RATE_JSON = "\"cached_input_nanos_per_mtok\":300000000";
const SONNET_OUTPUT_RATE_JSON = "\"output_nanos_per_mtok\":15000000000";

// base.cleanupRows deletes this suite's entries too (it owns the shared
// tenant's core.tenant_model_entries cleanup since activation upserts rows).
fn cleanup(conn: *pg.Conn) void {
    base.cleanupRows(conn);
}

fn extractId(alloc: std.mem.Allocator, body: []const u8) ![]const u8 {
    const IdOnly = struct { id: []const u8 };
    const parsed = try std.json.parseFromSlice(IdOnly, alloc, body, .{ .ignore_unknown_fields = true });
    defer parsed.deinit();
    return alloc.dupe(u8, parsed.value.id);
}

test "integration: test_models_list_joins_metadata_and_active — GET joins secret metadata, flags exactly one active entry, never leaks api_key" {
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
    try std.testing.expect(r.bodyContains(SONNET_INPUT_RATE_JSON));
    try std.testing.expect(r.bodyContains(SONNET_CACHED_INPUT_RATE_JSON));
    try std.testing.expect(r.bodyContains(SONNET_OUTPUT_RATE_JSON));
    try std.testing.expect(!r.bodyContains(SENTINEL));
    try std.testing.expect(!r.bodyContains("api_key"));

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    cleanup(conn);
}

test "integration: test_models_activation_upserts_entry — PUT /provider upserts the matching registry entry at write time; GET is a pure read" {
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
        // Activated directly through PUT /provider with no prior POST to the
        // registry — the activation itself must create the matching entry.
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
        // Repeat GET stays a pure read — no row is ever created on the read path.
        const r = try (try h.get("/v1/tenants/me/models").bearer(base.TOKEN_OPERATOR)).send();
        defer r.deinit();
        try r.expectStatus(.ok);
        try std.testing.expectEqual(@as(usize, 1), std.mem.count(u8, r.body, "\"model_id\":\"claude-sonnet-4-6\""));
    }
    {
        // Repeat activation of the same (secret_ref, model) is idempotent —
        // the entry upsert tolerates the duplicate and state converges.
        const r = try (try (try h.put("/v1/tenants/me/provider").bearer(base.TOKEN_OPERATOR))
            .json("{\"mode\":\"self_managed\",\"secret_ref\":\"sync-key\"}")).send();
        defer r.deinit();
        try r.expectStatus(.ok);
    }

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    const rows = try entries_state.list(alloc, conn, TEST_TENANT_ID);
    defer entries_state.deinitEntryList(rows, alloc);
    try std.testing.expectEqual(@as(usize, 1), rows.len);
    cleanup(conn);
}

test "integration: test_models_create_guards — POST guards unknown secret_ref (404) and duplicate entries (409)" {
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

test "integration: test_models_delete_active_guard — DELETE refuses the active entry, allows a non-active sibling" {
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

test "integration: test_secret_delete_blocked_when_referenced — secret DELETE is blocked while referenced by a model entry, naming the count" {
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

test "integration: test_entries_list_default_identity — GET carries the active platform default's provider/model/context alongside the availability boolean" {
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
        try fixtures_provider.seedPlatformProvider(alloc, conn, base.TEST_WS_ID);
    }

    const r = try (try h.get("/v1/tenants/me/models").bearer(base.TOKEN_OPERATOR)).send();
    defer r.deinit();
    try r.expectStatus(.ok);
    try std.testing.expect(r.bodyContains("\"platform_default_available\":true"));
    // Distinguished from the boolean's key by the colon-brace: the object is
    // present and carries the exact identity the fixture seeded.
    try std.testing.expect(r.bodyContains("\"platform_default\":{"));
    try std.testing.expect(r.bodyContains("\"provider\":\"" ++ fixtures_provider.TEST_PROVIDER_NAME ++ "\""));
    try std.testing.expect(r.bodyContains("\"model\":\"" ++ fixtures_provider.TEST_PLATFORM_MODEL ++ "\""));
    const cap_json = comptime std.fmt.comptimePrint("\"context_cap_tokens\":{d}", .{fixtures_provider.TEST_PLATFORM_CAP_TOKENS});
    try std.testing.expect(r.bodyContains(cap_json));
    try std.testing.expect(r.bodyContains("\"input_nanos_per_mtok\":0"));
    try std.testing.expect(r.bodyContains("\"cached_input_nanos_per_mtok\":0"));
    try std.testing.expect(r.bodyContains("\"output_nanos_per_mtok\":0"));

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    fixtures_provider.teardownPlatformProvider(conn, base.TEST_WS_ID);
    cleanup(conn);
}

test "integration: test_entries_list_no_default_omits_identity — GET omits platform_default (never null) and reports availability false when none is active" {
    base.setTestEncryptionKey();
    const alloc = std.testing.allocator;
    const h = base.seedAndHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    {
        // Clear any active default a prior test (or suite) left behind.
        const conn = try h.acquireConn();
        defer h.releaseConn(conn);
        fixtures_provider.teardownPlatformProvider(conn, base.TEST_WS_ID);
    }

    const r = try (try h.get("/v1/tenants/me/models").bearer(base.TOKEN_OPERATOR)).send();
    defer r.deinit();
    try r.expectStatus(.ok);
    try std.testing.expect(r.bodyContains("\"platform_default_available\":false"));
    // emit_null_optional_fields=false — the key must be absent, not null.
    try std.testing.expect(!r.bodyContains("\"platform_default\":"));

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    cleanup(conn);
}

// Regression guard: the Add-model dialog stores a known-provider secret with no
// `model` field (the model lives on the registry entry / PUT body). Other
// activation tests in this file put a `model` in the secret body, which masked
// the bug — before the probe fix this exact flow returned UZ-PROVIDER-003 while
// the registry row still committed.
test "integration: test_activate_model_less_secret — a {provider,api_key} secret with NO model activates when the model rides the PUT body" {
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
        // No `model` in the secret body; the PUT body carries the selected model.
        const r = try (try (try h.post(secrets_path).bearer(base.TOKEN_OPERATOR))
            .json("{\"name\":\"no-model-key\",\"data\":{\"provider\":\"anthropic\",\"api_key\":\"sk-nomodel\"}}")).send();
        defer r.deinit();
        try r.expectStatus(.created);
    }
    {
        // The model rides the PUT body (the registry entry's model_id), not the secret.
        const r = try (try (try h.put("/v1/tenants/me/provider").bearer(base.TOKEN_OPERATOR))
            .json("{\"mode\":\"self_managed\",\"secret_ref\":\"no-model-key\",\"model\":\"claude-sonnet-4-6\"}")).send();
        defer r.deinit();
        try r.expectStatus(.ok);
        try std.testing.expect(r.bodyContains("\"model\":\"claude-sonnet-4-6\""));
    }

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    cleanup(conn);
}

// Guard restored after the probe relaxation: a bare PUT for an openai-compatible
// secret carrying no model (and no model in the body) must fail up front rather
// than persist a blank model that only breaks later at dial time. Named
// providers already fail via the catalogue lookup; the custom-endpoint cap path
// takes a sentinel, so resolveSelfManagedCap now rejects an empty model for it too.
test "integration: test_activate_custom_endpoint_without_model_rejected — empty effective model fails UZ-PROVIDER-004, not a blank persist" {
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
            .json("{\"name\":\"gw-key\",\"data\":{\"provider\":\"openai-compatible\",\"base_url\":\"https://gw.example.com/v1\",\"api_key\":\"sk-gw\"}}")).send();
        defer r.deinit();
        try r.expectStatus(.created);
    }
    {
        // No model on the credential AND none in the body → empty effective model.
        const r = try (try (try h.put("/v1/tenants/me/provider").bearer(base.TOKEN_OPERATOR))
            .json("{\"mode\":\"self_managed\",\"secret_ref\":\"gw-key\"}")).send();
        defer r.deinit();
        try r.expectStatus(.bad_request);
        try std.testing.expect(r.bodyContains(error_codes.ERR_PROVIDER_MODEL_NOT_IN_CATALOGUE));
    }
    {
        // A whitespace-only model is rejected too — the cap gate trims before the
        // blank check, so `" "` can't slip through `.len == 0` and persist.
        const r = try (try (try h.put("/v1/tenants/me/provider").bearer(base.TOKEN_OPERATOR))
            .json("{\"mode\":\"self_managed\",\"secret_ref\":\"gw-key\",\"model\":\"  \"}")).send();
        defer r.deinit();
        try r.expectStatus(.bad_request);
        try std.testing.expect(r.bodyContains(error_codes.ERR_PROVIDER_MODEL_NOT_IN_CATALOGUE));
    }
    {
        // A whitespace-PADDED but otherwise valid model is rejected too — the cap
        // gate requires the trimmed value to equal the submitted one, so a padded
        // name can't be persisted for the custom endpoint to choke on at dial time.
        const r = try (try (try h.put("/v1/tenants/me/provider").bearer(base.TOKEN_OPERATOR))
            .json("{\"mode\":\"self_managed\",\"secret_ref\":\"gw-key\",\"model\":\" claude-sonnet-4-6 \"}")).send();
        defer r.deinit();
        try r.expectStatus(.bad_request);
        try std.testing.expect(r.bodyContains(error_codes.ERR_PROVIDER_MODEL_NOT_IN_CATALOGUE));
    }

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    cleanup(conn);
}
